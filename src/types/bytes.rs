//! Python bytes type — CPython 3.11 exact ABI layout.
//!
//! CPython layout:
//!   PyVarObject ob_base   (24 bytes: refcnt + type + ob_size=length)
//!   Py_hash_t  ob_shash   (8 bytes: cached hash, -1 = not computed)
//!   char       ob_sval[]  (inline flexible array, null-terminated: N+1 bytes)
//!
//! Total allocated size = 32 + N + 1 bytes.
//! Data is stored inline — no separate heap allocation.

use crate::object::pyobject::{RawPyObject, RawPyVarObject};
use crate::object::typeobj::{RawPyTypeObject, PY_TPFLAGS_DEFAULT, PY_TPFLAGS_BYTES_SUBCLASS};
use crate::object::SyncUnsafeCell;
use std::os::raw::{c_char, c_int};
use std::ptr;

/// Fixed portion of PyBytesObject (32 bytes).
/// ob_sval[] follows inline.
#[repr(C)]
pub struct PyBytesObject {
    pub ob_base: RawPyVarObject,  // 24 bytes
    pub ob_shash: isize,          // 8 bytes (cached hash, -1 = not computed)
    // Followed by ob_sval[N+1] of u8 (null-terminated)
}

const BYTES_HEADER_SIZE: usize = std::mem::size_of::<PyBytesObject>(); // 32
const _: () = assert!(BYTES_HEADER_SIZE == 32);

/// Get pointer to the inline data (ob_sval) after the header.
#[inline]
unsafe fn ob_sval(obj: *mut PyBytesObject) -> *mut u8 {
    (obj as *mut u8).add(BYTES_HEADER_SIZE)
}

#[no_mangle]
pub static PyBytes_Type: SyncUnsafeCell<RawPyTypeObject> = SyncUnsafeCell::new({
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"bytes\0".as_ptr() as *const _;
    tp.tp_basicsize = BYTES_HEADER_SIZE as isize; // 32
    tp.tp_itemsize = 1; // each item is 1 byte
    tp
});

pub fn bytes_type() -> *mut RawPyTypeObject {
    PyBytes_Type.get()
}

unsafe extern "C" fn bytes_dealloc(obj: *mut RawPyObject) {
    // Bytes are not GC-tracked (no cycles possible), just free directly.
    libc::free(obj as *mut libc::c_void);
}

// ─── Internal helpers ───

/// Allocate a bytes object of `size` bytes (uninitialized data, null-terminated).
unsafe fn alloc_bytes(size: usize) -> *mut RawPyObject {
    // 32 (header) + size + 1 (null terminator)
    let total = BYTES_HEADER_SIZE + size + 1;
    let raw = libc::calloc(1, total) as *mut PyBytesObject;
    if raw.is_null() {
        eprintln!("Fatal: out of memory allocating bytes object");
        std::process::abort();
    }
    // Initialize header
    (*raw).ob_base.ob_base.ob_refcnt = std::sync::atomic::AtomicIsize::new(1);
    (*raw).ob_base.ob_base.ob_type = bytes_type();
    (*raw).ob_base.ob_size = size as isize;
    (*raw).ob_shash = -1; // not computed
    // ob_sval is already zeroed by calloc (null terminated)
    raw as *mut RawPyObject
}

pub fn create_bytes_from_slice(data: &[u8]) -> *mut RawPyObject {
    unsafe {
        let obj = alloc_bytes(data.len());
        let b = obj as *mut PyBytesObject;
        let dest = ob_sval(b);
        ptr::copy_nonoverlapping(data.as_ptr(), dest, data.len());
        // Null terminator already set by calloc
        obj
    }
}

pub fn bytes_value(obj: *mut RawPyObject) -> &'static [u8] {
    unsafe {
        let b = obj as *mut PyBytesObject;
        let len = (*b).ob_base.ob_size as usize;
        let data = ob_sval(b);
        std::slice::from_raw_parts(data, len)
    }
}

// ─── C API ───

#[no_mangle]
pub unsafe extern "C" fn PyBytes_FromString(s: *const c_char) -> *mut RawPyObject {
    if s.is_null() {
        return ptr::null_mut();
    }
    let len = libc::strlen(s);
    let slice = std::slice::from_raw_parts(s as *const u8, len);
    create_bytes_from_slice(slice)
}

#[no_mangle]
pub unsafe extern "C" fn PyBytes_FromStringAndSize(
    s: *const c_char,
    size: isize,
) -> *mut RawPyObject {
    if size < 0 {
        return ptr::null_mut();
    }
    if s.is_null() {
        // Allocate zeroed bytes of given size
        return alloc_bytes(size as usize);
    }
    let slice = std::slice::from_raw_parts(s as *const u8, size as usize);
    create_bytes_from_slice(slice)
}

#[no_mangle]
pub unsafe extern "C" fn PyBytes_AsString(obj: *mut RawPyObject) -> *mut c_char {
    if obj.is_null() {
        return ptr::null_mut();
    }
    let b = obj as *mut PyBytesObject;
    ob_sval(b) as *mut c_char
}

#[no_mangle]
pub unsafe extern "C" fn PyBytes_AsStringAndSize(
    obj: *mut RawPyObject,
    s: *mut *mut c_char,
    len: *mut isize,
) -> c_int {
    if obj.is_null() {
        return -1;
    }
    let b = obj as *mut PyBytesObject;
    if !s.is_null() {
        *s = ob_sval(b) as *mut c_char;
    }
    if !len.is_null() {
        *len = (*b).ob_base.ob_size;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn PyBytes_Size(obj: *mut RawPyObject) -> isize {
    if obj.is_null() {
        return -1;
    }
    let b = obj as *mut PyBytesObject;
    (*b).ob_base.ob_size
}

#[no_mangle]
pub unsafe extern "C" fn PyBytes_GET_SIZE(obj: *mut RawPyObject) -> isize {
    PyBytes_Size(obj)
}

#[no_mangle]
pub unsafe extern "C" fn PyBytes_Check(obj: *mut RawPyObject) -> c_int {
    if obj.is_null() {
        return 0;
    }
    if (*obj).ob_type == bytes_type() { 1 } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn PyBytes_Concat(
    bytes: *mut *mut RawPyObject,
    new_part: *mut RawPyObject,
) {
    if bytes.is_null() || (*bytes).is_null() || new_part.is_null() {
        return;
    }
    let left_len = PyBytes_Size(*bytes) as usize;
    let right_len = PyBytes_Size(new_part) as usize;
    let total_len = left_len + right_len;
    let new_obj = alloc_bytes(total_len);
    let nb = new_obj as *mut PyBytesObject;
    let dest = ob_sval(nb);
    // Copy left
    let left_data = ob_sval(*bytes as *mut PyBytesObject);
    ptr::copy_nonoverlapping(left_data, dest, left_len);
    // Copy right
    let right_data = ob_sval(new_part as *mut PyBytesObject);
    ptr::copy_nonoverlapping(right_data, dest.add(left_len), right_len);
    // Decref old
    (*(*bytes)).decref();
    *bytes = new_obj;
}

pub unsafe fn init_bytes_type() {
    (*PyBytes_Type.get()).tp_dealloc = Some(bytes_dealloc);
    (*PyBytes_Type.get()).tp_flags = PY_TPFLAGS_DEFAULT | PY_TPFLAGS_BYTES_SUBCLASS;
}
