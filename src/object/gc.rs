//! Garbage collection tracking and cyclic reference detection.
//!
//! CPython uses a generational GC on top of reference counting
//! to break reference cycles. GC-tracked objects have a PyGC_Head
//! allocated *before* the object pointer — C extensions rely on this.
//!
//! The cycle collector uses gc_head fields as temporary scratch space:
//!   - gc_next: stores tentative gc_refs count during collection
//!   - gc_prev bit 0: IS_TRACKED flag (set on Track, cleared on UnTrack)
//!   - gc_prev bit 1: IS_REACHABLE flag (set during BFS reachability scan)

use crate::object::pyobject::{PyGCHead, RawPyObject, RawPyVarObject};
use crate::object::typeobj::{RawPyTypeObject, PY_TPFLAGS_HAVE_GC};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::os::raw::{c_int, c_void};
use std::sync::atomic::{AtomicIsize, Ordering};

/// Size of the GC header prepended before GC-tracked objects.
/// CPython 3.8+ uses a 16-byte GC head (2 words), NOT 24 bytes.
const GC_HEAD_SIZE: usize = std::mem::size_of::<PyGCHead>(); // 16 bytes
const _: () = assert!(GC_HEAD_SIZE == 16);

/// Bit flags stored in gc_prev during collection.
const GC_TRACKED: usize = 1;    // bit 0: object is in the tracked set
const GC_REACHABLE: usize = 2;  // bit 1: object is reachable from roots

/// Global GC state
static GC_STATE: Mutex<Option<GCState>> = Mutex::new(None);

struct GCState {
    /// Set of tracked objects (raw pointers to GC heads)
    tracked: HashSet<usize>,
    /// Generation counts
    gen0_count: usize,
    #[allow(dead_code)]
    gen0_threshold: usize,
    /// Flag to prevent re-entrant collection
    collecting: bool,
}

impl GCState {
    fn new() -> Self {
        GCState {
            tracked: HashSet::new(),
            gen0_count: 0,
            gen0_threshold: 700, // CPython default
            collecting: false,
        }
    }
}

fn with_gc<F, R>(f: F) -> R
where
    F: FnOnce(&mut GCState) -> R,
{
    let mut guard = GC_STATE.lock();
    if guard.is_none() {
        *guard = Some(GCState::new());
    }
    f(guard.as_mut().unwrap())
}

/// Get the GC head pointer from an object pointer.
/// The GC head is allocated immediately before the object.
#[inline]
unsafe fn gc_head_from_obj(obj: *mut c_void) -> *mut PyGCHead {
    (obj as *mut u8).sub(GC_HEAD_SIZE) as *mut PyGCHead
}

/// Get the object pointer from a GC head pointer.
#[inline]
unsafe fn obj_from_gc_head(gc: *mut PyGCHead) -> *mut RawPyObject {
    (gc as *mut u8).add(GC_HEAD_SIZE) as *mut RawPyObject
}

// ─── GC Allocation ───

/// _PyObject_GC_New — allocate a fixed-size GC-tracked object.
/// Allocates PyGC_Head + object, returns pointer past the GC head.
#[no_mangle]
pub unsafe extern "C" fn _PyObject_GC_New(
    tp: *mut RawPyTypeObject,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("_PyObject_GC_New", || unsafe {
        if tp.is_null() {
            eprintln!("[rustthon] FATAL: _PyObject_GC_New called with NULL type pointer!");
            eprintln!("  This usually means a C extension failed to create a type (PyType_FromSpec* returned NULL)");
            eprintln!("  but the extension continued to use the NULL type pointer.");
            return std::ptr::null_mut();
        }
        let obj_size = (*tp).tp_basicsize as usize;

        let total = GC_HEAD_SIZE + obj_size;
        let raw = libc::calloc(1, total) as *mut u8;
        if raw.is_null() {
            eprintln!("Fatal: out of memory in _PyObject_GC_New");
            std::process::abort();
        }

        // GC head is at `raw`, object starts at `raw + GC_HEAD_SIZE`
        let obj = raw.add(GC_HEAD_SIZE) as *mut RawPyObject;
        std::ptr::write(&mut (*obj).ob_refcnt, AtomicIsize::new(1));
        (*obj).ob_type = tp;

        // Debug: trace large object allocations (CyFunction etc.)
        if obj_size > 100 {
            let tp_name = if !tp.is_null() && !(*tp).tp_name.is_null() {
                std::ffi::CStr::from_ptr((*tp).tp_name).to_str().unwrap_or("???")
            } else { "(null tp)" };
            eprintln!("[rustthon] _PyObject_GC_New: tp={:p} name={} basicsize={} -> obj={:p}, ob_type={:p}",
                tp, tp_name, obj_size, obj, (*obj).ob_type);
        }

        // Track the object
        PyObject_GC_Track(obj as *mut c_void);

        obj
    })
}

/// _PyObject_GC_NewVar — allocate a variable-size GC-tracked object.
/// Allocates PyGC_Head + basicsize + nitems*itemsize.
#[no_mangle]
pub unsafe extern "C" fn _PyObject_GC_NewVar(
    tp: *mut RawPyTypeObject,
    nitems: isize,
) -> *mut RawPyVarObject {
    crate::ffi::panic_guard::guard_ptr("_PyObject_GC_NewVar", || unsafe {
        let basicsize = if !tp.is_null() {
            (*tp).tp_basicsize as usize
        } else {
            std::mem::size_of::<RawPyVarObject>()
        };
        let itemsize = if !tp.is_null() {
            (*tp).tp_itemsize as usize
        } else {
            0
        };

        let obj_size = basicsize + (nitems.max(0) as usize) * itemsize;
        let total = GC_HEAD_SIZE + obj_size;
        let raw = libc::calloc(1, total) as *mut u8;
        if raw.is_null() {
            eprintln!("Fatal: out of memory in _PyObject_GC_NewVar");
            std::process::abort();
        }

        let obj = raw.add(GC_HEAD_SIZE) as *mut RawPyVarObject;
        std::ptr::write(&mut (*obj).ob_base.ob_refcnt, AtomicIsize::new(1));
        (*obj).ob_base.ob_type = tp;
        (*obj).ob_size = nitems;

        // Track the object
        PyObject_GC_Track(obj as *mut c_void);

        obj
    })
}

/// Check whether an object participates in GC.
/// Respects tp_is_gc if set, otherwise checks PY_TPFLAGS_HAVE_GC.
/// Used by the collector and exposed for C extensions.
#[inline]
unsafe fn object_is_gc(obj: *mut RawPyObject) -> bool {
    if obj.is_null() {
        return false;
    }
    let tp = (*obj).ob_type;
    if tp.is_null() {
        return false;
    }
    if (*tp).tp_flags & PY_TPFLAGS_HAVE_GC == 0 {
        return false;
    }
    // If tp_is_gc is set, defer to it (e.g., tuple with only ints)
    if let Some(is_gc) = (*tp).tp_is_gc {
        return is_gc(obj) != 0;
    }
    true
}

/// _PyObject_IS_GC — check if an object participates in GC.
#[no_mangle]
pub unsafe extern "C" fn _PyObject_IS_GC(op: *mut RawPyObject) -> c_int {
    crate::ffi::panic_guard::guard_int("_PyObject_IS_GC", || unsafe {
        if object_is_gc(op) { 1 } else { 0 }
    })
}

// ─── GC Tracking ───

/// PyObject_GC_Track — register an object with the cyclic GC.
/// The object must have been allocated with _PyObject_GC_New/NewVar
/// (i.e., it has a PyGC_Head before it).
#[no_mangle]
pub unsafe extern "C" fn PyObject_GC_Track(op: *mut c_void) {
    crate::ffi::panic_guard::guard_void("PyObject_GC_Track", || unsafe {
        if !op.is_null() {
            let gc = gc_head_from_obj(op);
            // Set the IS_TRACKED bit in gc_prev
            (*gc).gc_prev |= GC_TRACKED;
            with_gc(|state| {
                state.tracked.insert(gc as usize);
                state.gen0_count += 1;
            });
        }
    })
}

/// PyObject_GC_UnTrack — remove an object from GC tracking.
#[no_mangle]
pub unsafe extern "C" fn PyObject_GC_UnTrack(op: *mut c_void) {
    crate::ffi::panic_guard::guard_void("PyObject_GC_UnTrack", || unsafe {
        if !op.is_null() {
            let gc = gc_head_from_obj(op);
            // Clear the IS_TRACKED bit in gc_prev
            (*gc).gc_prev &= !GC_TRACKED;
            with_gc(|state| {
                state.tracked.remove(&(gc as usize));
            });
        }
    })
}

/// _PyObject_GC_IS_TRACKED
#[no_mangle]
pub unsafe extern "C" fn _PyObject_GC_IS_TRACKED(op: *mut RawPyObject) -> i32 {
    crate::ffi::panic_guard::guard_i32("_PyObject_GC_IS_TRACKED", || unsafe {
        if op.is_null() {
            return 0;
        }
        let gc = gc_head_from_obj(op as *mut c_void);
        with_gc(|state| if state.tracked.contains(&(gc as usize)) { 1 } else { 0 })
    })
}

// ─── GC Deallocation ───

/// PyObject_GC_Del — free a GC-tracked object.
/// Frees starting from the GC head (before the object pointer).
#[no_mangle]
pub unsafe extern "C" fn PyObject_GC_Del(op: *mut c_void) {
    crate::ffi::panic_guard::guard_void("PyObject_GC_Del", || unsafe {
        if !op.is_null() {
            PyObject_GC_UnTrack(op);
            // Free from the GC head, which is before the object
            let gc = gc_head_from_obj(op);
            libc::free(gc as *mut c_void);
        }
    })
}

// ─── Cycle Collector ───

/// Get the tentative gc_refs count stored in gc_next during collection.
#[inline]
unsafe fn get_gc_refs(gc: *mut PyGCHead) -> isize {
    (*gc).gc_next as isize
}

/// Set the tentative gc_refs count in gc_next.
#[inline]
unsafe fn set_gc_refs(gc: *mut PyGCHead, refs: isize) {
    (*gc).gc_next = refs as usize;
}

/// Decrement gc_refs for an object (stored in gc_next).
#[inline]
unsafe fn dec_gc_refs(gc: *mut PyGCHead) {
    (*gc).gc_next = ((*gc).gc_next as isize - 1) as usize;
}

/// Visitor callback for subtract-internal-refs phase.
/// Decrements gc_refs for each referenced GC-tracked object.
///
/// CRITICAL: Must check PY_TPFLAGS_HAVE_GC first! tp_traverse yields ALL
/// referenced objects, including non-GC types (ints, strings) that have NO
/// PyGCHead prefix. Calling gc_head_from_obj on them reads garbage memory.
unsafe extern "C" fn visit_decref(obj: *mut RawPyObject, _arg: *mut c_void) -> c_int {
    if obj.is_null() {
        return 0;
    }
    let tp = (*obj).ob_type;
    if tp.is_null() || (*tp).tp_flags & PY_TPFLAGS_HAVE_GC == 0 {
        return 0; // Not a GC object — no PyGCHead exists before it
    }
    let gc = gc_head_from_obj(obj as *mut c_void);
    if (*gc).gc_prev & GC_TRACKED != 0 {
        dec_gc_refs(gc);
    }
    0
}

/// Visitor callback for BFS reachability scan.
/// Marks reachable objects and pushes them to the queue.
///
/// Same HAVE_GC guard as visit_decref.
unsafe extern "C" fn visit_reachable(obj: *mut RawPyObject, arg: *mut c_void) -> c_int {
    if obj.is_null() {
        return 0;
    }
    let tp = (*obj).ob_type;
    if tp.is_null() || (*tp).tp_flags & PY_TPFLAGS_HAVE_GC == 0 {
        return 0;
    }
    let gc = gc_head_from_obj(obj as *mut c_void);
    if (*gc).gc_prev & GC_TRACKED != 0 && (*gc).gc_prev & GC_REACHABLE == 0 {
        (*gc).gc_prev |= GC_REACHABLE;
        let queue = &mut *(arg as *mut Vec<*mut RawPyObject>);
        queue.push(obj);
    }
    0
}

/// PyGC_Collect — run a full GC collection. Returns number of freed objects.
///
/// Algorithm:
/// 1. Snapshot tracked set → Vec
/// 2. Init gc_refs: copy ob_refcnt into gc_head.gc_next
/// 3. Subtract internal refs: traverse each object with visit_decref
/// 4. Find roots (gc_refs > 0), BFS to mark all reachable objects
/// 5. Unreachable objects form the garbage set
/// 6. Three-pass clear (INCREF shield):
///    a. Shield: incref all unreachable
///    b. Clear: call tp_clear on each
///    c. Drop: decref all unreachable (deallocs cascade)
#[no_mangle]
pub unsafe extern "C" fn PyGC_Collect() -> isize {
    crate::ffi::panic_guard::guard_ssize("PyGC_Collect", || unsafe {
        // Prevent re-entrant collection
        let is_collecting = with_gc(|state| state.collecting);
        if is_collecting {
            return 0;
        }
        with_gc(|state| { state.collecting = true; });

        let result = gc_collect_impl();

        with_gc(|state| { state.collecting = false; });
        result
    })
}

/// Run a BFS reachability scan from all objects with gc_refs > 0.
/// Returns the set of unreachable object pointers.
/// Precondition: gc_refs and GC_REACHABLE bits are already initialized.
unsafe fn find_unreachable(tracked: &[*mut RawPyObject]) -> Vec<*mut RawPyObject> {
    let mut queue: Vec<*mut RawPyObject> = Vec::new();

    // Seed with roots (gc_refs > 0 means external references exist)
    for &obj in tracked {
        let gc = gc_head_from_obj(obj as *mut c_void);
        if (*gc).gc_prev & GC_TRACKED != 0 && get_gc_refs(gc) > 0 {
            (*gc).gc_prev |= GC_REACHABLE;
            queue.push(obj);
        }
    }

    // BFS from roots
    let mut head = 0;
    while head < queue.len() {
        let obj = queue[head];
        head += 1;
        let tp = (*obj).ob_type;
        if !tp.is_null() {
            if let Some(traverse) = (*tp).tp_traverse {
                traverse(
                    obj,
                    visit_reachable as *mut c_void,
                    &mut queue as *mut Vec<*mut RawPyObject> as *mut c_void,
                );
            }
        }
    }

    // Collect unreachable objects (tracked but not reachable)
    let mut unreachable: Vec<*mut RawPyObject> = Vec::new();
    for &obj in tracked {
        let gc = gc_head_from_obj(obj as *mut c_void);
        if (*gc).gc_prev & GC_TRACKED != 0 && (*gc).gc_prev & GC_REACHABLE == 0 {
            unreachable.push(obj);
        }
    }
    unreachable
}

/// The actual collection implementation, separated for clarity.
///
/// Algorithm:
/// 1. Snapshot tracked set, filter by tp_is_gc
/// 2. Init gc_refs from ob_refcnt
/// 3. Subtract internal refs via visit_decref
/// 4. BFS to find reachable objects; remainder is unreachable
/// 5. PEP 442: call tp_finalize on unreachable objects that have finalizers
/// 6. Re-run BFS to detect resurrected objects (finalizer may have saved them)
/// 7. Clear weak references on remaining unreachable
/// 8. Three-pass clear (INCREF shield) on final garbage
unsafe fn gc_collect_impl() -> isize {
    // 1. Snapshot tracked set → Vec of object pointers
    //    Filter out objects where tp_is_gc returns false.
    let tracked_snapshot: Vec<*mut RawPyObject> = with_gc(|state| {
        state.tracked
            .iter()
            .filter_map(|&gc_addr| {
                let obj = obj_from_gc_head(gc_addr as *mut PyGCHead);
                if object_is_gc(obj) { Some(obj) } else { None }
            })
            .collect()
    });

    if tracked_snapshot.is_empty() {
        return 0;
    }

    // 2. Init gc_refs: copy ob_refcnt into gc_head.gc_next
    // Also clear the REACHABLE flag from any previous run
    for &obj in &tracked_snapshot {
        let gc = gc_head_from_obj(obj as *mut c_void);
        let refcnt = (*obj).ob_refcnt.load(Ordering::Relaxed);
        set_gc_refs(gc, refcnt);
        (*gc).gc_prev &= !GC_REACHABLE; // clear reachable bit
    }

    // 3. Subtract internal refs: traverse each object with visit_decref
    for &obj in &tracked_snapshot {
        let tp = (*obj).ob_type;
        if !tp.is_null() {
            if let Some(traverse) = (*tp).tp_traverse {
                traverse(obj, visit_decref as *mut c_void, std::ptr::null_mut());
            }
        }
    }

    // 4. BFS from roots to find all reachable; remainder is unreachable
    let mut unreachable = find_unreachable(&tracked_snapshot);

    if unreachable.is_empty() {
        // Clean up gc_next/gc_prev fields
        cleanup_gc_fields(&tracked_snapshot);
        return 0;
    }

    // ─── PEP 442: tp_finalize and Resurrection Detection ───
    //
    // If any unreachable object has a tp_finalize slot, we must call it
    // BEFORE clearing. The finalizer may "resurrect" the object by storing
    // a reference in a reachable container (e.g., appending self to a global
    // list). After running finalizers, we re-run BFS to detect any objects
    // that became reachable again and rescue them.

    // 5. Check if any unreachable objects have finalizers
    let has_finalizers = unreachable.iter().any(|&obj| {
        let tp = (*obj).ob_type;
        !tp.is_null() && (*tp).tp_finalize.is_some()
    });

    if has_finalizers {
        // Shield all unreachable with +1 refcount during finalization
        for &obj in &unreachable {
            (*obj).ob_refcnt.fetch_add(1, Ordering::Relaxed);
        }

        // Call tp_finalize on each object that has it
        for &obj in &unreachable {
            let tp = (*obj).ob_type;
            if !tp.is_null() {
                if let Some(finalize) = (*tp).tp_finalize {
                    finalize(obj);
                }
            }
        }

        // Remove the finalization shield
        for &obj in &unreachable {
            (*obj).ob_refcnt.fetch_sub(1, Ordering::Relaxed);
        }

        // 6. Re-run reachability analysis — finalizers may have resurrected objects.
        //    Reset gc_refs and REACHABLE bits, then redo the full scan.
        for &obj in &tracked_snapshot {
            let gc = gc_head_from_obj(obj as *mut c_void);
            // Only process objects that are still tracked
            if (*gc).gc_prev & GC_TRACKED != 0 {
                let refcnt = (*obj).ob_refcnt.load(Ordering::Relaxed);
                set_gc_refs(gc, refcnt);
                (*gc).gc_prev &= !GC_REACHABLE;
            }
        }
        for &obj in &tracked_snapshot {
            let tp = (*obj).ob_type;
            let gc = gc_head_from_obj(obj as *mut c_void);
            if (*gc).gc_prev & GC_TRACKED != 0 && !tp.is_null() {
                if let Some(traverse) = (*tp).tp_traverse {
                    traverse(obj, visit_decref as *mut c_void, std::ptr::null_mut());
                }
            }
        }
        unreachable = find_unreachable(&tracked_snapshot);

        if unreachable.is_empty() {
            cleanup_gc_fields(&tracked_snapshot);
            return 0;
        }
    }

    let n_garbage = unreachable.len() as isize;

    // 7. Remove unreachable objects from the tracked set BEFORE clearing.
    // This way, PyObject_GC_UnTrack in tp_dealloc safely no-ops.
    with_gc(|state| {
        for &obj in &unreachable {
            let gc = gc_head_from_obj(obj as *mut c_void);
            state.tracked.remove(&(gc as usize));
            // Clear the tracked bit so visit_decref/visit_reachable skip them
            (*gc).gc_prev &= !GC_TRACKED;
        }
    });

    // 8. Clear weak references on unreachable objects BEFORE tp_clear.
    // This prevents weakref callbacks from seeing half-destroyed objects.
    for &obj in &unreachable {
        let tp = (*obj).ob_type;
        if !tp.is_null() && (*tp).tp_weaklistoffset != 0 {
            crate::ffi::object_api::PyObject_ClearWeakRefs(obj);
        }
    }

    // 9. Three-pass clear (INCREF shield)

    // Shield pass: incref every unreachable object to prevent premature dealloc
    for &obj in &unreachable {
        (*obj).ob_refcnt.fetch_add(1, Ordering::Relaxed);
    }

    // Clear pass: call tp_clear on each — no deallocs fire due to shield
    for &obj in &unreachable {
        let tp = (*obj).ob_type;
        if !tp.is_null() {
            if let Some(clear) = (*tp).tp_clear {
                clear(obj);
            }
        }
    }

    // Drop pass: decref each — removes shield, deallocs cascade naturally
    for &obj in &unreachable {
        let prev = (*obj).ob_refcnt.fetch_sub(1, Ordering::Release);
        if prev == 1 {
            std::sync::atomic::fence(Ordering::Acquire);
            // Refcount hit 0 — deallocate
            crate::object::pyobject::dealloc_object(obj);
        }
    }

    // Clean up gc_next/gc_prev fields on surviving objects
    cleanup_gc_fields(&tracked_snapshot);

    n_garbage
}

/// Reset gc_next and GC_REACHABLE on all still-tracked objects.
unsafe fn cleanup_gc_fields(tracked: &[*mut RawPyObject]) {
    for &obj in tracked {
        let gc = gc_head_from_obj(obj as *mut c_void);
        if (*gc).gc_prev & GC_TRACKED != 0 {
            (*gc).gc_next = 0;
            (*gc).gc_prev &= !GC_REACHABLE;
        }
    }
}

/// _PyObject_GC_TRACK (internal)
#[no_mangle]
pub unsafe extern "C" fn _PyObject_GC_TRACK(op: *mut c_void) {
    crate::ffi::panic_guard::guard_void("_PyObject_GC_TRACK", || unsafe {
        PyObject_GC_Track(op);
    })
}

/// _PyObject_GC_UNTRACK (internal)
#[no_mangle]
pub unsafe extern "C" fn _PyObject_GC_UNTRACK(op: *mut c_void) {
    crate::ffi::panic_guard::guard_void("_PyObject_GC_UNTRACK", || unsafe {
        PyObject_GC_UnTrack(op);
    })
}
