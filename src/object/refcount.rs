//! Reference counting C API exports.
//!
//! These are the #[no_mangle] functions that C extensions call
//! to increment/decrement reference counts.

use crate::object::pyobject::RawPyObject;

/// Py_INCREF - increment reference count.
/// This is the single most called function in all of CPython.
#[no_mangle]
pub unsafe extern "C" fn Py_IncRef(op: *mut RawPyObject) {
    if !op.is_null() {
        (*op).incref();
    }
}

/// Py_DECREF - decrement reference count, dealloc if zero.
#[no_mangle]
pub unsafe extern "C" fn Py_DecRef(op: *mut RawPyObject) {
    if !op.is_null() {
        let new_refcnt = (*op).decref();
        if new_refcnt == 0 {
            super::pyobject::dealloc_object(op);
        }
    }
}

/// _Py_INCREF - internal variant used by some extensions
#[no_mangle]
pub unsafe extern "C" fn _Py_IncRef(op: *mut RawPyObject) {
    Py_IncRef(op);
}

/// _Py_DECREF - internal variant
#[no_mangle]
pub unsafe extern "C" fn _Py_DecRef(op: *mut RawPyObject) {
    Py_DecRef(op);
}

/// Py_XINCREF - increment if non-null (the X stands for "eXtra safe")
#[no_mangle]
pub unsafe extern "C" fn Py_XIncRef(op: *mut RawPyObject) {
    Py_IncRef(op); // Py_IncRef already checks null
}

/// Py_XDECREF - decrement if non-null
#[no_mangle]
pub unsafe extern "C" fn Py_XDecRef(op: *mut RawPyObject) {
    Py_DecRef(op); // Py_DecRef already checks null
}

/// _Py_NewReference - initialize a newly allocated object
#[no_mangle]
pub unsafe extern "C" fn _Py_NewReference(op: *mut RawPyObject) {
    if !op.is_null() {
        use std::sync::atomic::Ordering;
        (*op).ob_refcnt.store(1, Ordering::Relaxed);
    }
}

/// Py_REFCNT - get reference count (used by some extensions as a function)
#[no_mangle]
pub unsafe extern "C" fn Py_REFCNT(op: *mut RawPyObject) -> isize {
    if op.is_null() {
        0
    } else {
        (*op).refcnt()
    }
}
