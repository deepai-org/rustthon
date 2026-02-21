//! Argument parsing C API.
//!
//! PyArg_ParseTuple, Py_BuildValue, and friends are variadic C functions.
//! Since Rust stable cannot handle va_list/va_arg, these are implemented
//! in C (csrc/varargs.c) and compiled via the cc crate in build.rs.
//!
//! We declare them here as extern "C" so the linker keeps the symbols
//! in the final cdylib. The `used` function forces a reference.

use crate::object::pyobject::RawPyObject;
use std::os::raw::{c_char, c_int};

extern "C" {
    pub fn PyArg_ParseTuple(
        args: *mut RawPyObject,
        format: *const c_char,
        ...
    ) -> c_int;

    pub fn PyArg_ParseTupleAndKeywords(
        args: *mut RawPyObject,
        kwargs: *mut RawPyObject,
        format: *const c_char,
        kwlist: *mut *mut c_char,
        ...
    ) -> c_int;

    pub fn PyArg_UnpackTuple(
        args: *mut RawPyObject,
        funcname: *const c_char,
        min: isize,
        max: isize,
        ...
    ) -> c_int;

    pub fn Py_BuildValue(
        format: *const c_char,
        ...
    ) -> *mut RawPyObject;

    #[link_name = "_Py_BuildValue_SizeT"]
    pub fn _Py_BuildValue_SizeT(
        format: *const c_char,
        ...
    ) -> *mut RawPyObject;

    pub fn Py_VaBuildValue(
        format: *const c_char,
        va: *mut c_char,
    ) -> *mut RawPyObject;
}

/// Force the linker to keep all varargs symbols by referencing them.
#[used]
static VARARGS_SYMS: [unsafe extern "C" fn(); 0] = [];

/// This function is never called but ensures the linker pulls in the C object.
#[doc(hidden)]
#[inline(never)]
pub unsafe fn _force_link_varargs() {
    // Touch each symbol so the linker can't dead-strip them.
    // These are never actually called from Rust.
    let _ = PyArg_ParseTuple as *const ();
    let _ = PyArg_ParseTupleAndKeywords as *const ();
    let _ = PyArg_UnpackTuple as *const ();
    let _ = Py_BuildValue as *const ();
    let _ = Py_VaBuildValue as *const ();
}
