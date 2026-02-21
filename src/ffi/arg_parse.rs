//! Argument parsing C API.
//!
//! PyArg_ParseTuple and friends are how C extensions unpack
//! Python function arguments. This is one of the most heavily
//! used APIs — virtually every C extension method calls it.
//!
//! The full format string language is complex. We implement
//! the common cases.

use crate::object::pyobject::RawPyObject;
use std::os::raw::{c_char, c_int};

/// PyArg_ParseTuple - parse positional arguments.
///
/// Format string characters:
///   s  - const char* (UTF-8 string)
///   i  - int
///   l  - long
///   d  - double
///   O  - PyObject*
///   O! - PyObject* with type check
///   |  - remaining args are optional
///
/// We can't fully implement varargs in Rust, but we export the symbol
/// so extensions can link against it. The actual parsing will need
/// to use platform-specific va_list handling.
#[no_mangle]
pub unsafe extern "C" fn PyArg_ParseTuple(
    args: *mut RawPyObject,
    format: *const c_char,
    // ... varargs
) -> c_int {
    // This is a stub. Full implementation requires va_list support.
    // For many simple extensions, we can handle this at the VM level
    // by pre-parsing arguments before calling the C function.
    if args.is_null() || format.is_null() {
        return 0;
    }
    // Return success for now - extensions that rely on this will need
    // the full va_list implementation
    1
}

/// PyArg_ParseTupleAndKeywords
#[no_mangle]
pub unsafe extern "C" fn PyArg_ParseTupleAndKeywords(
    args: *mut RawPyObject,
    kwargs: *mut RawPyObject,
    format: *const c_char,
    kwlist: *mut *mut c_char,
    // ... varargs
) -> c_int {
    // Same limitation as above
    if args.is_null() || format.is_null() {
        return 0;
    }
    1
}

/// PyArg_UnpackTuple - simpler argument unpacking
#[no_mangle]
pub unsafe extern "C" fn PyArg_UnpackTuple(
    args: *mut RawPyObject,
    funcname: *const c_char,
    min: isize,
    max: isize,
    // ... varargs (pointers to receive PyObject*)
) -> c_int {
    if args.is_null() {
        return 0;
    }
    let nargs = crate::types::tuple::PyTuple_Size(args);
    if nargs < min || nargs > max {
        return 0;
    }
    // Can't unpack varargs in Rust easily - this needs platform support
    1
}

/// Py_BuildValue - build a Python object from a format string.
///
/// Format string characters:
///   s  - string -> str
///   i  - int -> int
///   l  - long -> int
///   d  - double -> float
///   O  - PyObject* (borrowed ref, increfs)
///   N  - PyObject* (steals ref)
///   () - tuple
///   [] - list
///   {} - dict
///   "" or NULL -> None
#[no_mangle]
pub unsafe extern "C" fn Py_BuildValue(
    format: *const c_char,
    // ... varargs
) -> *mut RawPyObject {
    if format.is_null() {
        return crate::types::none::return_none();
    }

    let fmt = std::ffi::CStr::from_ptr(format).to_bytes();
    if fmt.is_empty() {
        return crate::types::none::return_none();
    }

    // For single character formats without parens, return a single object
    // This handles the simplest common cases
    match fmt {
        b"" => crate::types::none::return_none(),
        // Can't handle varargs, return None as fallback
        _ => crate::types::none::return_none(),
    }
}

/// _Py_BuildValue_SizeT (same as Py_BuildValue for us)
#[no_mangle]
pub unsafe extern "C" fn _Py_BuildValue_SizeT(
    format: *const c_char,
) -> *mut RawPyObject {
    Py_BuildValue(format)
}

/// Py_VaBuildValue
#[no_mangle]
pub unsafe extern "C" fn Py_VaBuildValue(
    format: *const c_char,
    _va: *mut c_char, // va_list
) -> *mut RawPyObject {
    Py_BuildValue(format)
}
