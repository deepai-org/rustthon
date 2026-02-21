//! Python str (unicode) type.
//!
//! CPython's unicode implementation is extremely complex (compact, legacy,
//! ASCII-only optimizations, etc.). We implement the essential C API surface
//! that extensions need, backed by a Rust String internally.

use crate::object::pyobject::{PyObjectWithData, RawPyObject};
use crate::object::typeobj::RawPyTypeObject;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};

static mut UNICODE_TYPE: RawPyTypeObject = {
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"str\0".as_ptr() as *const _;
    tp.tp_basicsize = 0; // Variable size
    tp
};

pub unsafe fn unicode_type() -> *mut RawPyTypeObject {
    &mut UNICODE_TYPE
}

/// Internal string data
pub struct UnicodeData {
    /// The actual string content
    pub value: String,
    /// Cached UTF-8 C string (for PyUnicode_AsUTF8)
    cached_utf8: once_cell::sync::OnceCell<CString>,
}

type PyUnicodeObject = PyObjectWithData<UnicodeData>;

pub unsafe fn unicode_value(obj: *mut RawPyObject) -> &'static str {
    let data = PyObjectWithData::<UnicodeData>::data_from_raw(obj);
    &data.value
}

unsafe fn create_unicode(s: String) -> *mut RawPyObject {
    let obj = PyObjectWithData::alloc(
        &mut UNICODE_TYPE,
        UnicodeData {
            value: s,
            cached_utf8: once_cell::sync::OnceCell::new(),
        },
    );
    obj as *mut RawPyObject
}

// ─── C API ───

/// PyUnicode_FromString - create str from C string (UTF-8)
#[no_mangle]
pub unsafe extern "C" fn PyUnicode_FromString(s: *const c_char) -> *mut RawPyObject {
    if s.is_null() {
        return std::ptr::null_mut();
    }
    let c_str = CStr::from_ptr(s);
    match c_str.to_str() {
        Ok(rust_str) => create_unicode(rust_str.to_owned()),
        Err(_) => {
            // Invalid UTF-8 - try lossy conversion
            create_unicode(c_str.to_string_lossy().into_owned())
        }
    }
}

/// PyUnicode_FromStringAndSize - create str from buffer with length
#[no_mangle]
pub unsafe extern "C" fn PyUnicode_FromStringAndSize(
    s: *const c_char,
    size: isize,
) -> *mut RawPyObject {
    if s.is_null() || size < 0 {
        return create_unicode(String::new());
    }
    let slice = std::slice::from_raw_parts(s as *const u8, size as usize);
    let string = String::from_utf8_lossy(slice).into_owned();
    create_unicode(string)
}

/// PyUnicode_FromFormat - simplified version (just handles %s for now)
#[no_mangle]
pub unsafe extern "C" fn PyUnicode_FromFormat(
    format: *const c_char,
    // varargs - we can't fully support this, handle the simple case
) -> *mut RawPyObject {
    if format.is_null() {
        return create_unicode(String::new());
    }
    let c_str = CStr::from_ptr(format);
    create_unicode(c_str.to_string_lossy().into_owned())
}

/// PyUnicode_AsUTF8 - return a pointer to the UTF-8 encoded string.
/// The returned pointer is valid as long as the object lives.
#[no_mangle]
pub unsafe extern "C" fn PyUnicode_AsUTF8(obj: *mut RawPyObject) -> *const c_char {
    if obj.is_null() {
        return std::ptr::null();
    }
    let data = PyObjectWithData::<UnicodeData>::data_from_raw(obj);
    let c_str = data.cached_utf8.get_or_init(|| {
        CString::new(data.value.as_str()).unwrap_or_default()
    });
    c_str.as_ptr()
}

/// PyUnicode_AsUTF8AndSize
#[no_mangle]
pub unsafe extern "C" fn PyUnicode_AsUTF8AndSize(
    obj: *mut RawPyObject,
    size: *mut isize,
) -> *const c_char {
    if obj.is_null() {
        return std::ptr::null();
    }
    let data = PyObjectWithData::<UnicodeData>::data_from_raw(obj);
    if !size.is_null() {
        *size = data.value.len() as isize;
    }
    let c_str = data.cached_utf8.get_or_init(|| {
        CString::new(data.value.as_str()).unwrap_or_default()
    });
    c_str.as_ptr()
}

/// PyUnicode_AsEncodedString - encode to bytes
#[no_mangle]
pub unsafe extern "C" fn PyUnicode_AsEncodedString(
    obj: *mut RawPyObject,
    encoding: *const c_char,
    _errors: *const c_char,
) -> *mut RawPyObject {
    if obj.is_null() {
        return std::ptr::null_mut();
    }
    let data = PyObjectWithData::<UnicodeData>::data_from_raw(obj);
    // For now, always encode as UTF-8
    crate::types::bytes::create_bytes_from_slice(data.value.as_bytes())
}

/// PyUnicode_GetLength
#[no_mangle]
pub unsafe extern "C" fn PyUnicode_GetLength(obj: *mut RawPyObject) -> isize {
    if obj.is_null() {
        return -1;
    }
    let data = PyObjectWithData::<UnicodeData>::data_from_raw(obj);
    data.value.chars().count() as isize
}

/// PyUnicode_GET_LENGTH (same as GetLength for our impl)
#[no_mangle]
pub unsafe extern "C" fn PyUnicode_GET_LENGTH(obj: *mut RawPyObject) -> isize {
    PyUnicode_GetLength(obj)
}

/// PyUnicode_Check
#[no_mangle]
pub unsafe extern "C" fn PyUnicode_Check(obj: *mut RawPyObject) -> c_int {
    if obj.is_null() {
        return 0;
    }
    if (*obj).ob_type == unicode_type() { 1 } else { 0 }
}

/// PyUnicode_CompareWithASCIIString
#[no_mangle]
pub unsafe extern "C" fn PyUnicode_CompareWithASCIIString(
    obj: *mut RawPyObject,
    string: *const c_char,
) -> c_int {
    if obj.is_null() || string.is_null() {
        return -1;
    }
    let data = PyObjectWithData::<UnicodeData>::data_from_raw(obj);
    let c_str = CStr::from_ptr(string);
    match c_str.to_str() {
        Ok(s) => {
            if data.value == s {
                0
            } else if data.value.as_str() < s {
                -1
            } else {
                1
            }
        }
        Err(_) => -1,
    }
}

/// PyUnicode_Concat - concatenate two strings
#[no_mangle]
pub unsafe extern "C" fn PyUnicode_Concat(
    left: *mut RawPyObject,
    right: *mut RawPyObject,
) -> *mut RawPyObject {
    if left.is_null() || right.is_null() {
        return std::ptr::null_mut();
    }
    let l = PyObjectWithData::<UnicodeData>::data_from_raw(left);
    let r = PyObjectWithData::<UnicodeData>::data_from_raw(right);
    let mut result = l.value.clone();
    result.push_str(&r.value);
    create_unicode(result)
}

/// PyUnicode_Join - join a sequence of strings with separator
#[no_mangle]
pub unsafe extern "C" fn PyUnicode_Join(
    separator: *mut RawPyObject,
    seq: *mut RawPyObject,
) -> *mut RawPyObject {
    // TODO: Implement properly with sequence protocol
    std::ptr::null_mut()
}

/// PyUnicode_InternInPlace - intern a string (make it a singleton)
#[no_mangle]
pub unsafe extern "C" fn PyUnicode_InternInPlace(_p: *mut *mut RawPyObject) {
    // TODO: String interning table
    // For now, no-op (interning is an optimization, not correctness-critical)
}

/// PyUnicode_InternFromString
#[no_mangle]
pub unsafe extern "C" fn PyUnicode_InternFromString(s: *const c_char) -> *mut RawPyObject {
    PyUnicode_FromString(s)
}

/// Helper: create a Python string from a Rust &str
pub unsafe fn create_from_str(s: &str) -> *mut RawPyObject {
    create_unicode(s.to_owned())
}

#[no_mangle]
pub static mut PyUnicode_Type: *mut RawPyTypeObject = std::ptr::null_mut();

pub unsafe fn init_unicode_type() {
    PyUnicode_Type = unicode_type();
}
