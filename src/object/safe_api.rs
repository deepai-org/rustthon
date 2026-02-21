//! Safe wrappers for common unsafe operations.
//!
//! These functions encapsulate the repeated unsafe patterns used throughout
//! the VM interpreter and compiler, allowing callers to avoid writing `unsafe`
//! blocks for routine operations like refcounting, type checking, and
//! object creation.
//!
//! Each function contains internal `unsafe` blocks where necessary, but
//! the function signature itself is safe.

use crate::object::pyobject::RawPyObject;
use crate::object::typeobj::RawPyTypeObject;

// ─── Refcount ───

/// Null-safe incref.
#[inline]
pub fn py_incref(obj: *mut RawPyObject) {
    if !obj.is_null() {
        unsafe { (*obj).incref(); }
    }
}

/// Null-safe decref.
#[inline]
pub fn py_decref(obj: *mut RawPyObject) {
    if !obj.is_null() {
        unsafe { (*obj).decref(); }
    }
}

// ─── Singletons ───

/// Get the None singleton pointer.
#[inline]
pub fn py_none() -> *mut RawPyObject {
    crate::types::none::PY_NONE.get()
}

/// Get the True singleton pointer.
#[inline]
pub fn py_true() -> *mut RawPyObject {
    crate::types::boolobject::PY_TRUE.get()
}

/// Get the False singleton pointer.
#[inline]
pub fn py_false() -> *mut RawPyObject {
    crate::types::boolobject::PY_FALSE.get()
}

/// Return None with an incref (for functions that return a new reference).
#[inline]
pub fn return_none() -> *mut RawPyObject {
    let none = py_none();
    py_incref(none);
    none
}

/// Safe wrapper for PyBool_FromLong.
#[inline]
pub fn bool_from_long(v: std::os::raw::c_long) -> *mut RawPyObject {
    unsafe { crate::types::boolobject::PyBool_FromLong(v) }
}

// ─── Type checks ───

/// Null-safe type identity check.
#[inline]
pub fn is_type(obj: *mut RawPyObject, tp: *mut RawPyTypeObject) -> bool {
    !obj.is_null() && unsafe { (*obj).ob_type == tp }
}

/// Check if obj is None.
#[inline]
pub fn is_none(obj: *mut RawPyObject) -> bool {
    obj == py_none()
}

/// Check if obj is an int (PyLong_Type).
#[inline]
pub fn is_int(obj: *mut RawPyObject) -> bool {
    is_type(obj, crate::types::longobject::long_type())
}

/// Check if obj is a float (PyFloat_Type).
#[inline]
pub fn is_float(obj: *mut RawPyObject) -> bool {
    is_type(obj, crate::types::floatobject::float_type())
}

/// Check if obj is a str (PyUnicode_Type).
#[inline]
pub fn is_str(obj: *mut RawPyObject) -> bool {
    is_type(obj, crate::types::unicode::unicode_type())
}

/// Check if obj is a list (PyList_Type).
#[inline]
pub fn is_list(obj: *mut RawPyObject) -> bool {
    is_type(obj, crate::types::list::list_type())
}

/// Check if obj is a tuple (PyTuple_Type).
#[inline]
pub fn is_tuple(obj: *mut RawPyObject) -> bool {
    is_type(obj, crate::types::tuple::tuple_type())
}

/// Check if obj is a dict (PyDict_Type).
#[inline]
pub fn is_dict(obj: *mut RawPyObject) -> bool {
    is_type(obj, crate::types::dict::dict_type())
}

/// Check if obj is a set (PySet_Type).
#[inline]
pub fn is_set(obj: *mut RawPyObject) -> bool {
    is_type(obj, crate::types::set::set_type())
}

/// Check if obj is a bool (PyBool_Type).
#[inline]
pub fn is_bool(obj: *mut RawPyObject) -> bool {
    crate::types::boolobject::is_bool(obj)
}

// ─── Object creation ───

/// Create a new int object from i64.
#[inline]
pub fn create_int(val: i64) -> *mut RawPyObject {
    unsafe { crate::types::longobject::PyLong_FromLong(val as _) }
}

/// Create a new float object from f64.
#[inline]
pub fn create_float(val: f64) -> *mut RawPyObject {
    unsafe { crate::types::floatobject::PyFloat_FromDouble(val) }
}

/// Create a new str object from a Rust &str.
#[inline]
pub fn create_str(s: &str) -> *mut RawPyObject {
    crate::types::unicode::create_from_str(s)
}

/// Create a new bytes object from a byte slice.
#[inline]
pub fn create_bytes(data: &[u8]) -> *mut RawPyObject {
    crate::types::bytes::create_bytes_from_slice(data)
}

// ─── Value extraction ───

/// Extract i64 from an int object. Returns 0 for null or wrong type.
#[inline]
pub fn get_int_value(obj: *mut RawPyObject) -> i64 {
    if obj.is_null() {
        return 0;
    }
    if is_int(obj) || is_bool(obj) {
        crate::types::longobject::long_as_i64(obj)
    } else {
        0
    }
}

/// Extract f64 from a float or int object. Handles int->float promotion.
#[inline]
pub fn get_float_value(obj: *mut RawPyObject) -> f64 {
    if obj.is_null() {
        return 0.0;
    }
    if is_float(obj) {
        crate::types::floatobject::float_value(obj)
    } else if is_int(obj) || is_bool(obj) {
        crate::types::longobject::long_as_f64(obj)
    } else {
        0.0
    }
}
