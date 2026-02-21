//! CPython-compatible memory allocation subsystem.
//!
//! CPython has three tiers of allocators:
//!   1. Raw allocator (PyMem_RawMalloc) - wraps system malloc
//!   2. Python allocator (PyMem_Malloc) - for general Python objects
//!   3. Object allocator (PyObject_Malloc) - for PyObject instances
//!
//! For now, we route all three through the system allocator,
//! which is correct enough for extension compatibility.

use std::os::raw::c_void;

// ─── Raw memory allocator (tier 1) ───

#[no_mangle]
pub unsafe extern "C" fn PyMem_RawMalloc(size: usize) -> *mut c_void {
    libc::malloc(size)
}

#[no_mangle]
pub unsafe extern "C" fn PyMem_RawCalloc(nelem: usize, elsize: usize) -> *mut c_void {
    libc::calloc(nelem, elsize)
}

#[no_mangle]
pub unsafe extern "C" fn PyMem_RawRealloc(ptr: *mut c_void, size: usize) -> *mut c_void {
    libc::realloc(ptr, size)
}

#[no_mangle]
pub unsafe extern "C" fn PyMem_RawFree(ptr: *mut c_void) {
    if !ptr.is_null() {
        libc::free(ptr);
    }
}

// ─── Python memory allocator (tier 2) ───

#[no_mangle]
pub unsafe extern "C" fn PyMem_Malloc(size: usize) -> *mut c_void {
    if size == 0 {
        return libc::malloc(1); // CPython returns non-null for size 0
    }
    libc::malloc(size)
}

#[no_mangle]
pub unsafe extern "C" fn PyMem_Calloc(nelem: usize, elsize: usize) -> *mut c_void {
    if nelem == 0 || elsize == 0 {
        return libc::calloc(1, 1);
    }
    libc::calloc(nelem, elsize)
}

#[no_mangle]
pub unsafe extern "C" fn PyMem_Realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
    let size = if size == 0 { 1 } else { size };
    if ptr.is_null() {
        libc::malloc(size)
    } else {
        libc::realloc(ptr, size)
    }
}

#[no_mangle]
pub unsafe extern "C" fn PyMem_Free(ptr: *mut c_void) {
    if !ptr.is_null() {
        libc::free(ptr);
    }
}

// ─── Object allocator (tier 3) ───

#[no_mangle]
pub unsafe extern "C" fn PyObject_Malloc(size: usize) -> *mut c_void {
    PyMem_Malloc(size)
}

#[no_mangle]
pub unsafe extern "C" fn PyObject_Calloc(nelem: usize, elsize: usize) -> *mut c_void {
    PyMem_Calloc(nelem, elsize)
}

#[no_mangle]
pub unsafe extern "C" fn PyObject_Realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
    PyMem_Realloc(ptr, size)
}

#[no_mangle]
pub unsafe extern "C" fn PyObject_Free(ptr: *mut c_void) {
    PyMem_Free(ptr)
}

// ─── Internal helpers used by our runtime ───

/// Allocate memory for a Python object (used internally).
pub unsafe fn py_object_malloc(size: usize) -> *mut c_void {
    PyObject_Malloc(size)
}

/// Free memory for a Python object (used internally).
pub unsafe fn py_object_free(ptr: *mut c_void) {
    PyObject_Free(ptr)
}

// ─── PyObject_Init / PyObject_InitVar ───

use crate::object::pyobject::{RawPyObject, RawPyVarObject};
use crate::object::typeobj::RawPyTypeObject;

/// PyObject_Init - initialize a pre-allocated object
#[no_mangle]
pub unsafe extern "C" fn PyObject_Init(
    op: *mut RawPyObject,
    tp: *mut RawPyTypeObject,
) -> *mut RawPyObject {
    if op.is_null() {
        return std::ptr::null_mut();
    }
    use std::sync::atomic::AtomicIsize;
    std::ptr::write(&mut (*op).ob_refcnt, AtomicIsize::new(1));
    (*op).ob_type = tp;
    op
}

/// PyObject_InitVar - initialize a pre-allocated variable-size object
#[no_mangle]
pub unsafe extern "C" fn PyObject_InitVar(
    op: *mut RawPyVarObject,
    tp: *mut RawPyTypeObject,
    size: isize,
) -> *mut RawPyVarObject {
    if op.is_null() {
        return std::ptr::null_mut();
    }
    PyObject_Init(&mut (*op).ob_base, tp);
    (*op).ob_size = size;
    op
}

/// _PyObject_New - allocate and initialize a new object
#[no_mangle]
pub unsafe extern "C" fn _PyObject_New(
    tp: *mut RawPyTypeObject,
) -> *mut RawPyObject {
    let size = if !tp.is_null() {
        (*tp).tp_basicsize as usize
    } else {
        std::mem::size_of::<RawPyObject>()
    };

    let obj = py_object_malloc(size) as *mut RawPyObject;
    if obj.is_null() {
        return std::ptr::null_mut();
    }
    PyObject_Init(obj, tp)
}

/// PyMemoryView_FromMemory — create a memoryview from a C buffer.
/// Simplified: creates a bytes object instead (memoryview not fully implemented).
#[no_mangle]
pub unsafe extern "C" fn PyMemoryView_FromMemory(
    mem: *mut std::os::raw::c_char,
    size: isize,
    _flags: std::os::raw::c_int,
) -> *mut RawPyObject {
    if mem.is_null() || size < 0 {
        return std::ptr::null_mut();
    }
    // Create a bytes object from the memory (simplified stand-in for memoryview)
    crate::types::bytes::PyBytes_FromStringAndSize(mem, size)
}

/// PyOS_snprintf — safe snprintf wrapper.
/// This is a variadic C function. Since we can't handle varargs in Rust,
/// we provide a stub that handles the simple case.
#[no_mangle]
pub unsafe extern "C" fn PyOS_snprintf(
    buf: *mut std::os::raw::c_char,
    size: usize,
    format: *const std::os::raw::c_char,
    // varargs follow — but Rust can't capture them
) -> () {
    // Best-effort: just copy the format string
    if !buf.is_null() && size > 0 && !format.is_null() {
        libc::snprintf(buf, size, format);
    }
}

/// _PyObject_NewVar - allocate and initialize a new variable-size object
#[no_mangle]
pub unsafe extern "C" fn _PyObject_NewVar(
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

    let size = basicsize + (nitems as usize) * itemsize;
    let obj = py_object_malloc(size) as *mut RawPyVarObject;
    if obj.is_null() {
        return std::ptr::null_mut();
    }
    PyObject_InitVar(obj, tp, nitems)
}
