//! Python bytes type.

use crate::object::pyobject::{PyObjectWithData, RawPyObject};
use crate::object::typeobj::RawPyTypeObject;
use std::os::raw::{c_char, c_int};

static mut BYTES_TYPE: RawPyTypeObject = {
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"bytes\0".as_ptr() as *const _;
    tp.tp_basicsize = 0;
    tp
};

pub unsafe fn bytes_type() -> *mut RawPyTypeObject {
    &mut BYTES_TYPE
}

pub struct BytesData {
    pub value: Vec<u8>,
}

type PyBytesObject = PyObjectWithData<BytesData>;

pub unsafe fn create_bytes_from_slice(data: &[u8]) -> *mut RawPyObject {
    let obj = PyObjectWithData::alloc(
        &mut BYTES_TYPE,
        BytesData {
            value: data.to_vec(),
        },
    );
    obj as *mut RawPyObject
}

pub unsafe fn bytes_value(obj: *mut RawPyObject) -> &'static [u8] {
    let data = PyObjectWithData::<BytesData>::data_from_raw(obj);
    &data.value
}

// ─── C API ───

/// PyBytes_FromString
#[no_mangle]
pub unsafe extern "C" fn PyBytes_FromString(s: *const c_char) -> *mut RawPyObject {
    if s.is_null() {
        return std::ptr::null_mut();
    }
    let len = libc::strlen(s);
    let slice = std::slice::from_raw_parts(s as *const u8, len);
    create_bytes_from_slice(slice)
}

/// PyBytes_FromStringAndSize
#[no_mangle]
pub unsafe extern "C" fn PyBytes_FromStringAndSize(
    s: *const c_char,
    size: isize,
) -> *mut RawPyObject {
    if size < 0 {
        return std::ptr::null_mut();
    }
    if s.is_null() {
        // Allocate uninitialized bytes of given size
        return create_bytes_from_slice(&vec![0u8; size as usize]);
    }
    let slice = std::slice::from_raw_parts(s as *const u8, size as usize);
    create_bytes_from_slice(slice)
}

/// PyBytes_AsString - get pointer to internal buffer
#[no_mangle]
pub unsafe extern "C" fn PyBytes_AsString(obj: *mut RawPyObject) -> *mut c_char {
    if obj.is_null() {
        return std::ptr::null_mut();
    }
    let data = PyObjectWithData::<BytesData>::data_from_raw_mut(obj);
    data.value.as_mut_ptr() as *mut c_char
}

/// PyBytes_AsStringAndSize
#[no_mangle]
pub unsafe extern "C" fn PyBytes_AsStringAndSize(
    obj: *mut RawPyObject,
    s: *mut *mut c_char,
    len: *mut isize,
) -> c_int {
    if obj.is_null() {
        return -1;
    }
    let data = PyObjectWithData::<BytesData>::data_from_raw_mut(obj);
    if !s.is_null() {
        *s = data.value.as_mut_ptr() as *mut c_char;
    }
    if !len.is_null() {
        *len = data.value.len() as isize;
    }
    0
}

/// PyBytes_Size
#[no_mangle]
pub unsafe extern "C" fn PyBytes_Size(obj: *mut RawPyObject) -> isize {
    if obj.is_null() {
        return -1;
    }
    let data = PyObjectWithData::<BytesData>::data_from_raw(obj);
    data.value.len() as isize
}

/// PyBytes_GET_SIZE
#[no_mangle]
pub unsafe extern "C" fn PyBytes_GET_SIZE(obj: *mut RawPyObject) -> isize {
    PyBytes_Size(obj)
}

/// PyBytes_Check
#[no_mangle]
pub unsafe extern "C" fn PyBytes_Check(obj: *mut RawPyObject) -> c_int {
    if obj.is_null() {
        return 0;
    }
    if (*obj).ob_type == bytes_type() { 1 } else { 0 }
}

/// PyBytes_Concat
#[no_mangle]
pub unsafe extern "C" fn PyBytes_Concat(
    bytes: *mut *mut RawPyObject,
    new_part: *mut RawPyObject,
) {
    if bytes.is_null() || (*bytes).is_null() || new_part.is_null() {
        return;
    }
    let left = PyObjectWithData::<BytesData>::data_from_raw(*bytes);
    let right = PyObjectWithData::<BytesData>::data_from_raw(new_part);
    let mut combined = left.value.clone();
    combined.extend_from_slice(&right.value);
    // Create new bytes object and replace
    let new_obj = create_bytes_from_slice(&combined);
    // Decref old
    (*(*bytes)).decref();
    *bytes = new_obj;
}

#[no_mangle]
pub static mut PyBytes_Type: *mut RawPyTypeObject = std::ptr::null_mut();

pub unsafe fn init_bytes_type() {
    PyBytes_Type = bytes_type();
}
