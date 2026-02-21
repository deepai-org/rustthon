//! Safe wrappers for Python object operations.
//!
//! Two API layers:
//!
//! 1. **Legacy raw-pointer API** — takes/returns `*mut RawPyObject`.
//!    Used by current callers (interpreter.rs, frame.rs, compile.rs).
//!    Will be removed when those callers are migrated in Phases 3-7.
//!
//! 2. **New RAII API** — takes `Python<'py>` + `&impl AsPyPointer`,
//!    returns `PyResult` (fallible). No manual refcounting — RAII handles
//!    everything. Container accessors correctly use `borrow_or_err` for
//!    borrowed references and `steal_or_err` for new references.

use crate::object::pyobject::{AsPyPointer, PyObjectRef, RawPyObject};
use crate::object::typeobj::RawPyTypeObject;
use crate::runtime::gil::Python;
use crate::runtime::pyerr::{PyErr, PyResult};

// ═══════════════════════════════════════════════════════════════════
// Legacy raw-pointer API (used by current callers, removed in Phase 7)
// ═══════════════════════════════════════════════════════════════════

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

// ─── Singletons (raw) ───

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

// ─── Object creation (raw, infallible) ───

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

// ─── Value extraction (raw) ───

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

// ═══════════════════════════════════════════════════════════════════
// New RAII API — Python<'py> + AsPyPointer + PyResult
// ═══════════════════════════════════════════════════════════════════

// ─── Singletons (owned PyObjectRef, incref'd) ───

/// Get None as an owned reference (incref'd).
#[inline]
pub fn none_obj(_py: Python<'_>) -> PyObjectRef {
    unsafe { PyObjectRef::borrow_raw(crate::types::none::PY_NONE.get()) }
}

/// Get True as an owned reference (incref'd).
#[inline]
pub fn true_obj(_py: Python<'_>) -> PyObjectRef {
    unsafe { PyObjectRef::borrow_raw(crate::types::boolobject::PY_TRUE.get()) }
}

/// Get False as an owned reference (incref'd).
#[inline]
pub fn false_obj(_py: Python<'_>) -> PyObjectRef {
    unsafe { PyObjectRef::borrow_raw(crate::types::boolobject::PY_FALSE.get()) }
}

/// Get a bool as an owned reference.
#[inline]
pub fn bool_obj(py: Python<'_>, val: bool) -> PyObjectRef {
    if val { true_obj(py) } else { false_obj(py) }
}

// ─── Type-safe creation (fallible → PyResult) ───

/// Create a new int object. Returns Err on allocation failure.
#[inline]
pub fn new_int(_py: Python<'_>, val: i64) -> PyResult {
    let ptr = unsafe { crate::types::longobject::PyLong_FromLong(val as _) };
    PyObjectRef::steal_or_err(ptr)
}

/// Create a new float object. Returns Err on allocation failure.
#[inline]
pub fn new_float(_py: Python<'_>, val: f64) -> PyResult {
    let ptr = unsafe { crate::types::floatobject::PyFloat_FromDouble(val) };
    PyObjectRef::steal_or_err(ptr)
}

/// Create a new str object from a Rust &str. Returns Err on allocation failure.
#[inline]
pub fn new_str(_py: Python<'_>, s: &str) -> PyResult {
    let ptr = crate::types::unicode::create_from_str(s);
    PyObjectRef::steal_or_err(ptr)
}

/// Create a new bytes object from a byte slice. Returns Err on allocation failure.
#[inline]
pub fn new_bytes(_py: Python<'_>, data: &[u8]) -> PyResult {
    let ptr = crate::types::bytes::create_bytes_from_slice(data);
    PyObjectRef::steal_or_err(ptr)
}

// ─── Value extraction (via AsPyPointer) ───

/// Extract i64 from an int or bool object.
/// Returns 0 if not an int/bool type.
#[inline]
pub fn int_value(obj: &impl AsPyPointer) -> i64 {
    let raw = obj.as_raw();
    if is_int(raw) || is_bool(raw) {
        crate::types::longobject::long_as_i64(raw)
    } else {
        0
    }
}

/// Extract f64 from a float, int, or bool object.
/// Handles int->float promotion. Returns 0.0 for other types.
#[inline]
pub fn float_value(obj: &impl AsPyPointer) -> f64 {
    let raw = obj.as_raw();
    if is_float(raw) {
        crate::types::floatobject::float_value(raw)
    } else if is_int(raw) || is_bool(raw) {
        crate::types::longobject::long_as_f64(raw)
    } else {
        0.0
    }
}

/// Extract a string slice from a str object.
/// Returns "" if not a str.
#[inline]
pub fn str_value(obj: &impl AsPyPointer) -> &'static str {
    let raw = obj.as_raw();
    if is_str(raw) {
        crate::types::unicode::unicode_value(raw)
    } else {
        ""
    }
}

// ─── Truthiness ───

/// Test if an object is truthy. Calls PyObject_IsTrue.
/// Returns Err if the truthiness check itself raises an exception.
pub fn py_is_true(_py: Python<'_>, obj: &impl AsPyPointer) -> Result<bool, PyErr> {
    let r = unsafe { crate::ffi::object_api::PyObject_IsTrue(obj.as_raw()) };
    if r < 0 {
        Err(PyErr::fetch())
    } else {
        Ok(r != 0)
    }
}

// ─── Attribute access ───

/// Get an attribute by name. Returns a NEW reference (steal_or_err).
pub fn py_get_attr(_py: Python<'_>, obj: &impl AsPyPointer, name: &str) -> PyResult {
    let cstr = std::ffi::CString::new(name).unwrap_or_else(|_| {
        std::ffi::CString::new("(null)").unwrap()
    });
    let ptr = unsafe {
        crate::ffi::object_api::PyObject_GetAttrString(obj.as_raw(), cstr.as_ptr())
    };
    PyObjectRef::steal_or_err(ptr)
}

/// Set an attribute by name. Returns Err if setting fails.
pub fn py_set_attr(
    _py: Python<'_>,
    obj: &impl AsPyPointer,
    name: &str,
    value: &impl AsPyPointer,
) -> Result<(), PyErr> {
    let cstr = std::ffi::CString::new(name).unwrap_or_else(|_| {
        std::ffi::CString::new("(null)").unwrap()
    });
    let r = unsafe {
        crate::ffi::object_api::PyObject_SetAttrString(
            obj.as_raw(),
            cstr.as_ptr(),
            value.as_raw(),
        )
    };
    if r < 0 {
        Err(PyErr::fetch())
    } else {
        Ok(())
    }
}

// ─── Item access (subscript) ───

/// Get an item by key (obj[key]). Returns a NEW reference (steal_or_err).
pub fn py_get_item(_py: Python<'_>, obj: &impl AsPyPointer, key: &impl AsPyPointer) -> PyResult {
    let ptr = unsafe {
        crate::ffi::object_api::PyObject_GetItem(obj.as_raw(), key.as_raw())
    };
    PyObjectRef::steal_or_err(ptr)
}

/// Set an item by key (obj[key] = val). Returns Err if setting fails.
pub fn py_store_item(
    _py: Python<'_>,
    obj: &impl AsPyPointer,
    key: &impl AsPyPointer,
    val: &impl AsPyPointer,
) -> Result<(), PyErr> {
    let r = unsafe {
        crate::ffi::object_api::PyObject_SetItem(obj.as_raw(), key.as_raw(), val.as_raw())
    };
    if r < 0 {
        Err(PyErr::fetch())
    } else {
        Ok(())
    }
}

// ─── Calling ───

/// Call a callable with positional args (no kwargs).
///
/// Builds a temporary tuple from `args`, calls `PyObject_Call`, and cleans up.
/// The args tuple is managed via RAII — even if PyObject_Call fails, it is freed.
pub fn py_call(_py: Python<'_>, func: &impl AsPyPointer, args: &[PyObjectRef]) -> PyResult {
    unsafe {
        // Build args tuple
        let tuple_ptr = crate::types::tuple::PyTuple_New(args.len() as isize);
        if tuple_ptr.is_null() {
            return Err(PyErr::fetch());
        }
        // RAII: tuple_ref owns the tuple, will decref on drop (even on error paths)
        let tuple_ref = PyObjectRef::from_raw(tuple_ptr);
        for (i, arg) in args.iter().enumerate() {
            // PyTuple_SetItem steals a reference, so incref first to donate one
            (*arg.as_raw()).incref();
            crate::types::tuple::PyTuple_SetItem(tuple_ref.as_raw(), i as isize, arg.as_raw());
        }
        let result = crate::ffi::object_api::PyObject_Call(
            func.as_raw(),
            tuple_ref.as_raw(),
            std::ptr::null_mut(),
        );
        // tuple_ref is dropped here → decref → tuple freed → each element decref'd
        // The elements were incref'd above, so this is balanced.
        drop(tuple_ref);
        PyObjectRef::steal_or_err(result)
    }
}

// ─── Import ───

/// Import a module by name. Returns a NEW reference.
pub fn py_import(_py: Python<'_>, name: &str) -> PyResult {
    let cstr = std::ffi::CString::new(name).unwrap_or_else(|_| {
        std::ffi::CString::new("(null)").unwrap()
    });
    let ptr = unsafe {
        crate::ffi::import::PyImport_ImportModule(cstr.as_ptr())
    };
    PyObjectRef::steal_or_err(ptr)
}

// ─── Repr / Str ───

/// Get the repr of an object. Returns a NEW str reference.
pub fn py_repr(_py: Python<'_>, obj: &impl AsPyPointer) -> PyResult {
    let ptr = unsafe { crate::ffi::object_api::PyObject_Repr(obj.as_raw()) };
    PyObjectRef::steal_or_err(ptr)
}

/// Get the str of an object. Returns a NEW str reference.
pub fn py_str(_py: Python<'_>, obj: &impl AsPyPointer) -> PyResult {
    let ptr = unsafe { crate::ffi::object_api::PyObject_Str(obj.as_raw()) };
    PyObjectRef::steal_or_err(ptr)
}

// ─── Container builders ───

/// Build a list from items. Items are moved in (ownership transferred via
/// `into_raw` because `PyList_SetItem` steals references).
/// If allocation fails, the remaining items in `items` are dropped → decref'd.
pub fn build_list(_py: Python<'_>, items: Vec<PyObjectRef>) -> PyResult {
    unsafe {
        let list = crate::types::list::PyList_New(items.len() as isize);
        if list.is_null() {
            // items Vec is dropped → all PyObjectRef's decref'd
            return Err(PyErr::fetch());
        }
        for (i, item) in items.into_iter().enumerate() {
            // PyList_SetItem steals a reference — use into_raw to transfer ownership
            let ptr = item.into_raw();
            crate::types::list::PyList_SetItem(list, i as isize, ptr);
        }
        PyObjectRef::steal_or_err(list)
    }
}

/// Build a tuple from items. Items are moved in (ownership transferred via
/// `into_raw` because `PyTuple_SetItem` steals references).
pub fn build_tuple(_py: Python<'_>, items: Vec<PyObjectRef>) -> PyResult {
    unsafe {
        let tuple = crate::types::tuple::PyTuple_New(items.len() as isize);
        if tuple.is_null() {
            return Err(PyErr::fetch());
        }
        for (i, item) in items.into_iter().enumerate() {
            // PyTuple_SetItem steals a reference
            let ptr = item.into_raw();
            crate::types::tuple::PyTuple_SetItem(tuple, i as isize, ptr);
        }
        PyObjectRef::steal_or_err(tuple)
    }
}

/// Build a dict from key-value pairs. `PyDict_SetItem` increfs both key and value,
/// so our `PyObjectRef`s are dropped (decref'd) after insertion — balanced.
pub fn build_dict(_py: Python<'_>, pairs: Vec<(PyObjectRef, PyObjectRef)>) -> PyResult {
    unsafe {
        let dict_ptr = crate::types::dict::PyDict_New();
        if dict_ptr.is_null() {
            return Err(PyErr::fetch());
        }
        let dict = PyObjectRef::from_raw(dict_ptr);
        for (key, val) in pairs {
            // PyDict_SetItem does NOT steal — it increfs both key and value internally
            let r = crate::types::dict::PyDict_SetItem(
                dict.as_raw(),
                key.as_raw(),
                val.as_raw(),
            );
            if r < 0 {
                // dict dropped → decref dict, remaining pairs dropped → decref keys/values
                return Err(PyErr::fetch());
            }
            // key and val dropped here → decref'd
            // dict holds its own refs from PyDict_SetItem's internal incref
        }
        Ok(dict)
    }
}

/// Build a set from items. `PySet_Add` increfs the item internally,
/// so our `PyObjectRef`s are dropped (decref'd) after insertion — balanced.
pub fn build_set(_py: Python<'_>, items: Vec<PyObjectRef>) -> PyResult {
    unsafe {
        let set_ptr = crate::types::set::PySet_New(std::ptr::null_mut());
        if set_ptr.is_null() {
            return Err(PyErr::fetch());
        }
        let set = PyObjectRef::from_raw(set_ptr);
        for item in items {
            // PySet_Add does NOT steal — it increfs the item internally
            let r = crate::types::set::PySet_Add(set.as_raw(), item.as_raw());
            if r < 0 {
                return Err(PyErr::fetch());
            }
            // item dropped → decref'd
        }
        Ok(set)
    }
}

// ─── Container accessors ───

/// Get the length of a list.
#[inline]
pub fn list_len(_py: Python<'_>, list: &impl AsPyPointer) -> isize {
    unsafe { crate::types::list::PyList_Size(list.as_raw()) }
}

/// Get item from a list by index. Uses `borrow_or_err` because
/// `PyList_GetItem` returns a BORROWED reference.
#[inline]
pub fn list_get(_py: Python<'_>, list: &impl AsPyPointer, idx: isize) -> PyResult {
    let ptr = unsafe { crate::types::list::PyList_GetItem(list.as_raw(), idx) };
    PyObjectRef::borrow_or_err(ptr)
}

/// Get the length of a tuple.
#[inline]
pub fn tuple_len(_py: Python<'_>, tuple: &impl AsPyPointer) -> isize {
    unsafe { crate::types::tuple::PyTuple_Size(tuple.as_raw()) }
}

/// Get item from a tuple by index. Uses `borrow_or_err` because
/// `PyTuple_GetItem` returns a BORROWED reference.
#[inline]
pub fn tuple_get(_py: Python<'_>, tuple: &impl AsPyPointer, idx: isize) -> PyResult {
    let ptr = unsafe { crate::types::tuple::PyTuple_GetItem(tuple.as_raw(), idx) };
    PyObjectRef::borrow_or_err(ptr)
}

/// Get the length of a dict.
#[inline]
pub fn dict_len(_py: Python<'_>, dict: &impl AsPyPointer) -> isize {
    unsafe { crate::types::dict::PyDict_Size(dict.as_raw()) }
}

/// Get item from a dict by key. Uses `borrow_or_err` because
/// `PyDict_GetItem` returns a BORROWED reference.
/// Note: unlike PyDict_GetItemWithError, PyDict_GetItem suppresses exceptions.
#[inline]
pub fn dict_get(_py: Python<'_>, dict: &impl AsPyPointer, key: &impl AsPyPointer) -> PyResult {
    let ptr = unsafe { crate::types::dict::PyDict_GetItem(dict.as_raw(), key.as_raw()) };
    PyObjectRef::borrow_or_err(ptr)
}

// ─── Comparison ───

/// Rich comparison (==, !=, <, >, <=, >=). Returns a NEW reference.
pub fn py_richcompare(
    _py: Python<'_>,
    left: &impl AsPyPointer,
    right: &impl AsPyPointer,
    op: std::os::raw::c_int,
) -> PyResult {
    let ptr = unsafe {
        crate::ffi::object_api::PyObject_RichCompare(left.as_raw(), right.as_raw(), op)
    };
    PyObjectRef::steal_or_err(ptr)
}
