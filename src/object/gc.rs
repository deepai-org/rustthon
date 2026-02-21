//! Garbage collection tracking for cyclic reference detection.
//!
//! CPython uses a generational GC on top of reference counting
//! to break reference cycles. GC-tracked objects have a PyGC_Head
//! allocated *before* the object pointer — C extensions rely on this.

use crate::object::pyobject::{PyGCHead, RawPyObject, RawPyVarObject};
use crate::object::typeobj::RawPyTypeObject;
use crate::runtime::memory::PyObject_Init;
use parking_lot::Mutex;
use std::collections::HashSet;
use std::os::raw::c_void;
use std::sync::atomic::AtomicIsize;

/// Size of the GC header prepended before GC-tracked objects.
/// CPython 3.8+ uses a 16-byte GC head (2 words), NOT 24 bytes.
const GC_HEAD_SIZE: usize = std::mem::size_of::<PyGCHead>(); // 16 bytes
const _: () = assert!(GC_HEAD_SIZE == 16);

/// Global GC state
static GC_STATE: Mutex<Option<GCState>> = Mutex::new(None);

struct GCState {
    /// Set of tracked objects (raw pointers to GC heads)
    tracked: HashSet<usize>,
    /// Generation counts
    gen0_count: usize,
    gen0_threshold: usize,
}

impl GCState {
    fn new() -> Self {
        GCState {
            tracked: HashSet::new(),
            gen0_count: 0,
            gen0_threshold: 700, // CPython default
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
    if tp.is_null() {
        eprintln!("[rustthon] FATAL: _PyObject_GC_New called with NULL type pointer!");
        eprintln!("  This usually means a C extension failed to create a type (PyType_FromSpec* returned NULL)");
        eprintln!("  but the extension continued to use the NULL type pointer.");
        // Don't abort — return null so the caller can handle the error
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
}

/// _PyObject_GC_NewVar — allocate a variable-size GC-tracked object.
/// Allocates PyGC_Head + basicsize + nitems*itemsize.
#[no_mangle]
pub unsafe extern "C" fn _PyObject_GC_NewVar(
    tp: *mut RawPyTypeObject,
    nitems: isize,
) -> *mut RawPyVarObject {
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
}

// ─── GC Tracking ───

/// PyObject_GC_Track — register an object with the cyclic GC.
/// The object must have been allocated with _PyObject_GC_New/NewVar
/// (i.e., it has a PyGC_Head before it).
#[no_mangle]
pub unsafe extern "C" fn PyObject_GC_Track(op: *mut c_void) {
    if !op.is_null() {
        let gc = gc_head_from_obj(op);
        with_gc(|state| {
            state.tracked.insert(gc as usize);
            state.gen0_count += 1;
        });
    }
}

/// PyObject_GC_UnTrack — remove an object from GC tracking.
#[no_mangle]
pub unsafe extern "C" fn PyObject_GC_UnTrack(op: *mut c_void) {
    if !op.is_null() {
        let gc = gc_head_from_obj(op);
        with_gc(|state| {
            state.tracked.remove(&(gc as usize));
        });
    }
}

/// _PyObject_GC_IS_TRACKED
#[no_mangle]
pub unsafe extern "C" fn _PyObject_GC_IS_TRACKED(op: *mut RawPyObject) -> i32 {
    if op.is_null() {
        return 0;
    }
    let gc = gc_head_from_obj(op as *mut c_void);
    with_gc(|state| if state.tracked.contains(&(gc as usize)) { 1 } else { 0 })
}

// ─── GC Deallocation ───

/// PyObject_GC_Del — free a GC-tracked object.
/// Frees starting from the GC head (before the object pointer).
#[no_mangle]
pub unsafe extern "C" fn PyObject_GC_Del(op: *mut c_void) {
    if !op.is_null() {
        PyObject_GC_UnTrack(op);
        // Free from the GC head, which is before the object
        let gc = gc_head_from_obj(op);
        libc::free(gc as *mut c_void);
    }
}

/// PyGC_Collect — run a full GC collection. Returns number of freed objects.
#[no_mangle]
pub unsafe extern "C" fn PyGC_Collect() -> isize {
    // Stub: rely on reference counting for now.
    // A full cycle detector would walk tp_traverse callbacks.
    0
}

/// _PyObject_GC_TRACK (internal)
#[no_mangle]
pub unsafe extern "C" fn _PyObject_GC_TRACK(op: *mut c_void) {
    PyObject_GC_Track(op);
}

/// _PyObject_GC_UNTRACK (internal)
#[no_mangle]
pub unsafe extern "C" fn _PyObject_GC_UNTRACK(op: *mut c_void) {
    PyObject_GC_UnTrack(op);
}
