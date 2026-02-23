//! Frame, code, and traceback object stubs for Cython compatibility.
//!
//! Cython generates code that creates empty code objects and frames for
//! traceback construction. These are simplified stubs that provide enough
//! structure for the symbols to resolve without full traceback support.

use crate::object::pyobject::RawPyObject;
use crate::object::typeobj::RawPyTypeObject;
use crate::object::SyncUnsafeCell;
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;

// ─── Code object (simplified) ───

/// Minimal PyCodeObject stand-in. We only need enough to satisfy Cython's
/// PyCode_NewEmpty which creates sentinel code objects for tracebacks.
#[repr(C)]
pub struct PyCodeObject {
    pub ob_refcnt: std::sync::atomic::AtomicIsize,
    pub ob_type: *mut RawPyTypeObject,
    // Simplified: just store filename and firstlineno
    pub co_filename: *mut RawPyObject,
    pub co_name: *mut RawPyObject,
    pub co_firstlineno: c_int,
}

static CODE_TYPE: SyncUnsafeCell<RawPyTypeObject> = SyncUnsafeCell::new({
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"code\0".as_ptr() as *const _;
    tp.tp_basicsize = std::mem::size_of::<PyCodeObject>() as isize;
    tp
});

/// PyCode_NewEmpty — create a minimal code object (used by Cython for tracebacks).
#[no_mangle]
pub unsafe extern "C" fn PyCode_NewEmpty(
    filename: *const c_char,
    funcname: *const c_char,
    firstlineno: c_int,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyCode_NewEmpty", || unsafe {
        let code = libc::calloc(1, std::mem::size_of::<PyCodeObject>()) as *mut PyCodeObject;
        if code.is_null() {
            return ptr::null_mut();
        }
        (*code).ob_refcnt = std::sync::atomic::AtomicIsize::new(1);
        (*code).ob_type = CODE_TYPE.get();
        (*code).co_filename = if !filename.is_null() {
            crate::types::unicode::PyUnicode_FromString(filename)
        } else {
            ptr::null_mut()
        };
        (*code).co_name = if !funcname.is_null() {
            crate::types::unicode::PyUnicode_FromString(funcname)
        } else {
            ptr::null_mut()
        };
        (*code).co_firstlineno = firstlineno;
        code as *mut RawPyObject
    })
}

/// PyCode_NewWithPosOnlyArgs — Cython references this for Python 3.8+ compat.
/// Simplified: delegates to PyCode_NewEmpty with just the relevant fields.
#[no_mangle]
pub unsafe extern "C" fn PyCode_NewWithPosOnlyArgs(
    _argcount: c_int,
    _posonlyargcount: c_int,
    _kwonlyargcount: c_int,
    _nlocals: c_int,
    _stacksize: c_int,
    _flags: c_int,
    _code: *mut RawPyObject,
    _consts: *mut RawPyObject,
    _names: *mut RawPyObject,
    _varnames: *mut RawPyObject,
    _freevars: *mut RawPyObject,
    _cellvars: *mut RawPyObject,
    filename: *mut RawPyObject,
    name: *mut RawPyObject,
    firstlineno: c_int,
    _lnotab: *mut RawPyObject,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyCode_NewWithPosOnlyArgs", || unsafe {
        // Extract filename as C string
        let fname = if !filename.is_null() {
            crate::types::unicode::PyUnicode_AsUTF8(filename)
        } else {
            b"<unknown>\0".as_ptr() as *const c_char
        };
        let funcname = if !name.is_null() {
            crate::types::unicode::PyUnicode_AsUTF8(name)
        } else {
            b"<unknown>\0".as_ptr() as *const c_char
        };
        PyCode_NewEmpty(fname, funcname, firstlineno)
    })
}

// ─── Frame object (simplified) ───

/// Minimal PyFrameObject. Cython creates these for traceback construction.
#[repr(C)]
pub struct PyFrameObject {
    pub ob_refcnt: std::sync::atomic::AtomicIsize,
    pub ob_type: *mut RawPyTypeObject,
    pub f_back: *mut PyFrameObject,
    pub f_code: *mut PyCodeObject,
    pub f_lineno: c_int,
}

static FRAME_TYPE: SyncUnsafeCell<RawPyTypeObject> = SyncUnsafeCell::new({
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"frame\0".as_ptr() as *const _;
    tp.tp_basicsize = std::mem::size_of::<PyFrameObject>() as isize;
    tp
});

/// PyFrame_New — create a frame object.
/// Cython uses this to build traceback chains.
#[no_mangle]
pub unsafe extern "C" fn PyFrame_New(
    tstate: *mut crate::runtime::thread_state::PyThreadState,
    code: *mut RawPyObject,
    globals: *mut RawPyObject,
    locals: *mut RawPyObject,
) -> *mut PyFrameObject {
    crate::ffi::panic_guard::guard_ptr("PyFrame_New", || unsafe {
        let frame = libc::calloc(1, std::mem::size_of::<PyFrameObject>()) as *mut PyFrameObject;
        if frame.is_null() {
            return ptr::null_mut();
        }
        (*frame).ob_refcnt = std::sync::atomic::AtomicIsize::new(1);
        (*frame).ob_type = FRAME_TYPE.get();
        (*frame).f_back = ptr::null_mut();
        (*frame).f_code = code as *mut PyCodeObject;
        if !code.is_null() {
            (*(code as *mut RawPyObject)).incref();
            (*frame).f_lineno = (*(code as *mut PyCodeObject)).co_firstlineno;
        }
        frame
    })
}

// ─── Traceback (simplified) ───

/// PyTraceBack_Here — record a traceback entry for the current exception.
/// Cython calls this during exception handling. No-op stub for now.
#[no_mangle]
pub unsafe extern "C" fn PyTraceBack_Here(
    frame: *mut PyFrameObject,
) -> c_int {
    crate::ffi::panic_guard::guard_int("PyTraceBack_Here", || unsafe {
        // No-op: we don't maintain a real traceback chain yet
        0
    })
}

/// PyTraceBack_Print — print traceback to file. No-op stub.
#[no_mangle]
pub unsafe extern "C" fn PyTraceBack_Print(
    _tb: *mut RawPyObject,
    _f: *mut std::os::raw::c_void,
) -> c_int {
    0
}
