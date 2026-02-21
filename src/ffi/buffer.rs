//! Buffer Protocol implementation.
//!
//! The buffer protocol allows C extensions to directly access memory
//! without copying. Critical for numpy (array data), Pillow (pixel data),
//! and any high-performance data processing.
//!
//! The core type is Py_buffer (PyBufferRaw in our code).

use crate::object::pyobject::RawPyObject;
use crate::object::typeobj::{PyBufferRaw, PySsizeT};
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;

// Buffer request flags (matching CPython)
pub const PyBUF_SIMPLE: c_int = 0;
pub const PyBUF_WRITABLE: c_int = 0x0001;
pub const PyBUF_FORMAT: c_int = 0x0004;
pub const PyBUF_ND: c_int = 0x0008;
pub const PyBUF_STRIDES: c_int = 0x0010 | PyBUF_ND;
pub const PyBUF_C_CONTIGUOUS: c_int = 0x0020 | PyBUF_STRIDES;
pub const PyBUF_F_CONTIGUOUS: c_int = 0x0040 | PyBUF_STRIDES;
pub const PyBUF_ANY_CONTIGUOUS: c_int = 0x0080 | PyBUF_STRIDES;
pub const PyBUF_INDIRECT: c_int = 0x0100 | PyBUF_STRIDES;
pub const PyBUF_CONTIG: c_int = PyBUF_ND | PyBUF_WRITABLE;
pub const PyBUF_CONTIG_RO: c_int = PyBUF_ND;
pub const PyBUF_STRIDED: c_int = PyBUF_STRIDES | PyBUF_WRITABLE;
pub const PyBUF_STRIDED_RO: c_int = PyBUF_STRIDES;
pub const PyBUF_RECORDS: c_int = PyBUF_STRIDES | PyBUF_WRITABLE | PyBUF_FORMAT;
pub const PyBUF_RECORDS_RO: c_int = PyBUF_STRIDES | PyBUF_FORMAT;
pub const PyBUF_FULL: c_int = PyBUF_INDIRECT | PyBUF_WRITABLE | PyBUF_FORMAT;
pub const PyBUF_FULL_RO: c_int = PyBUF_INDIRECT | PyBUF_FORMAT;
pub const PyBUF_READ: c_int = 0x100;
pub const PyBUF_WRITE: c_int = 0x200;

/// PyObject_GetBuffer - request a buffer view from an object.
/// This is how numpy gets raw array data, how Pillow gets pixel buffers, etc.
#[no_mangle]
pub unsafe extern "C" fn PyObject_GetBuffer(
    obj: *mut RawPyObject,
    view: *mut PyBufferRaw,
    flags: c_int,
) -> c_int {
    if obj.is_null() || view.is_null() {
        return -1;
    }

    // Zero out the view
    ptr::write_bytes(view, 0, 1);

    let tp = (*obj).ob_type;
    if tp.is_null() {
        return -1;
    }

    let as_buffer = (*tp).tp_as_buffer;
    if as_buffer.is_null() {
        // Type doesn't support buffer protocol
        return -1;
    }

    if let Some(bf_getbuffer) = (*as_buffer).bf_getbuffer {
        let result = bf_getbuffer(obj, view, flags);
        if result == 0 {
            (*view).obj = obj;
            (*obj).incref();
        }
        result
    } else {
        -1
    }
}

/// PyBuffer_Release - release a buffer view
#[no_mangle]
pub unsafe extern "C" fn PyBuffer_Release(view: *mut PyBufferRaw) {
    if view.is_null() {
        return;
    }

    let obj = (*view).obj;
    if !obj.is_null() {
        let tp = (*obj).ob_type;
        if !tp.is_null() && !(*tp).tp_as_buffer.is_null() {
            if let Some(bf_releasebuffer) = (*(*tp).tp_as_buffer).bf_releasebuffer {
                bf_releasebuffer(obj, view);
            }
        }
        (*obj).decref();
    }

    // Clear the view
    (*view).obj = ptr::null_mut();
    (*view).buf = ptr::null_mut();
}

/// PyBuffer_IsContiguous
#[no_mangle]
pub unsafe extern "C" fn PyBuffer_IsContiguous(
    view: *const PyBufferRaw,
    order: c_char,
) -> c_int {
    if view.is_null() {
        return 0;
    }
    // Simple check: if no strides or suboffsets, it's contiguous
    if (*view).strides.is_null() && (*view).suboffsets.is_null() {
        return 1;
    }
    // TODO: Full contiguity check for C/Fortran order
    1
}

/// PyBuffer_GetPointer
#[no_mangle]
pub unsafe extern "C" fn PyBuffer_GetPointer(
    view: *const PyBufferRaw,
    indices: *const PySsizeT,
) -> *mut c_void {
    if view.is_null() || (*view).buf.is_null() {
        return ptr::null_mut();
    }
    if indices.is_null() || (*view).ndim == 0 {
        return (*view).buf;
    }

    let mut pointer = (*view).buf as *mut u8;
    for i in 0..(*view).ndim as usize {
        let idx = *indices.add(i);
        if !(*view).strides.is_null() {
            pointer = pointer.offset(idx * *(*view).strides.add(i));
        } else {
            pointer = pointer.offset(idx * (*view).itemsize);
        }
    }
    pointer as *mut c_void
}

/// PyBuffer_FillInfo - helper to fill a simple buffer view
#[no_mangle]
pub unsafe extern "C" fn PyBuffer_FillInfo(
    view: *mut PyBufferRaw,
    obj: *mut RawPyObject,
    buf: *mut c_void,
    len: PySsizeT,
    readonly: c_int,
    flags: c_int,
) -> c_int {
    if view.is_null() {
        return -1;
    }

    if (flags & PyBUF_WRITABLE) != 0 && readonly != 0 {
        // Requested writable buffer but data is readonly
        return -1;
    }

    (*view).buf = buf;
    (*view).obj = obj;
    if !obj.is_null() {
        (*obj).incref();
    }
    (*view).len = len;
    (*view).itemsize = 1;
    (*view).readonly = readonly;
    (*view).ndim = 1;
    (*view).format = if (flags & PyBUF_FORMAT) != 0 {
        b"B\0".as_ptr() as *mut c_char // unsigned byte
    } else {
        ptr::null_mut()
    };
    (*view).shape = ptr::null_mut();
    (*view).strides = ptr::null_mut();
    (*view).suboffsets = ptr::null_mut();
    (*view).internal = ptr::null_mut();

    0
}

/// PyMemoryView_FromObject
#[no_mangle]
pub unsafe extern "C" fn PyMemoryView_FromObject(
    obj: *mut RawPyObject,
) -> *mut RawPyObject {
    // TODO: Create a memoryview object wrapping the buffer
    ptr::null_mut()
}

/// PyMemoryView_FromBuffer
#[no_mangle]
pub unsafe extern "C" fn PyMemoryView_FromBuffer(
    view: *const PyBufferRaw,
) -> *mut RawPyObject {
    // TODO: Create memoryview from buffer info
    ptr::null_mut()
}
