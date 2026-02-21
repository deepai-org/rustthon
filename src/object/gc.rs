//! Garbage collection tracking for cyclic reference detection.
//!
//! CPython uses a generational GC on top of reference counting
//! to break reference cycles. C extensions can register objects
//! with the GC tracker.

use crate::object::pyobject::{PyGCHead, RawPyObject};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::os::raw::c_void;

/// Global GC state
static GC_STATE: Mutex<Option<GCState>> = Mutex::new(None);

struct GCState {
    /// Set of tracked objects (raw pointers)
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

/// PyObject_GC_Track - register an object with the cyclic GC.
#[no_mangle]
pub unsafe extern "C" fn PyObject_GC_Track(op: *mut c_void) {
    if !op.is_null() {
        with_gc(|gc| {
            gc.tracked.insert(op as usize);
            gc.gen0_count += 1;
        });
    }
}

/// PyObject_GC_UnTrack - remove an object from GC tracking.
#[no_mangle]
pub unsafe extern "C" fn PyObject_GC_UnTrack(op: *mut c_void) {
    if !op.is_null() {
        with_gc(|gc| {
            gc.tracked.remove(&(op as usize));
        });
    }
}

/// _PyObject_GC_IS_TRACKED
#[no_mangle]
pub unsafe extern "C" fn _PyObject_GC_IS_TRACKED(op: *mut RawPyObject) -> i32 {
    if op.is_null() {
        return 0;
    }
    with_gc(|gc| if gc.tracked.contains(&(op as usize)) { 1 } else { 0 })
}

/// PyObject_GC_New - allocate a GC-tracked object
#[no_mangle]
pub unsafe extern "C" fn _PyObject_GC_New(
    tp: *mut crate::object::typeobj::RawPyTypeObject,
) -> *mut RawPyObject {
    let size = if !tp.is_null() {
        (*tp).tp_basicsize as usize
    } else {
        std::mem::size_of::<RawPyObject>()
    };

    let obj = crate::runtime::memory::py_object_malloc(size) as *mut RawPyObject;
    if obj.is_null() {
        return std::ptr::null_mut();
    }

    std::ptr::write(
        &mut (*obj).ob_refcnt,
        std::sync::atomic::AtomicIsize::new(1),
    );
    (*obj).ob_type = tp;

    PyObject_GC_Track(obj as *mut c_void);
    obj
}

/// PyObject_GC_Del - free a GC-tracked object
#[no_mangle]
pub unsafe extern "C" fn PyObject_GC_Del(op: *mut c_void) {
    if !op.is_null() {
        PyObject_GC_UnTrack(op);
        crate::runtime::memory::py_object_free(op);
    }
}

/// PyGC_Collect - run a full GC collection. Returns number of freed objects.
#[no_mangle]
pub unsafe extern "C" fn PyGC_Collect() -> isize {
    // For now, we rely on reference counting doing most of the work.
    // A full cycle detector would walk tp_traverse callbacks,
    // find unreachable cycles, and break them.
    // This is a stub that satisfies the ABI.
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
