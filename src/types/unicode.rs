//! Python str (unicode) type — CPython 3.11 exact ABI layout.
//!
//! CPython 3.11 uses a three-tier unicode representation:
//!
//!   PyASCIIObject (48 bytes):
//!     PyObject_HEAD      (16 bytes: refcnt + type)
//!     length             (8 bytes: Py_ssize_t)
//!     hash               (8 bytes: Py_hash_t, -1 = not computed)
//!     state              (4 bytes: bitfield — interned:2, kind:3, compact:1, ascii:1, ready:1)
//!     _padding           (4 bytes)
//!     wstr               (8 bytes: *mut wchar_t, null for new strings)
//!
//!   PyCompactUnicodeObject (72 bytes):
//!     _base              (48 bytes: PyASCIIObject)
//!     utf8_length        (8 bytes)
//!     utf8               (8 bytes: *mut u8, cached UTF-8)
//!     wstr_length        (8 bytes)
//!
//! Compact ASCII: kind=1, compact=1, ascii=1, data inline at offset 48.
//!   Allocated: 48 + length + 1
//!
//! Compact non-ASCII: kind=1/2/4, compact=1, ascii=0, data inline at offset 72.
//!   Allocated: 72 + length * kind_size + kind_size (null terminator)
//!   UTF-8 cached in `utf8` field (heap-allocated).

use crate::object::pyobject::RawPyObject;
use crate::object::typeobj::{RawPyTypeObject, PY_TPFLAGS_DEFAULT, PY_TPFLAGS_UNICODE_SUBCLASS};
use std::ffi::CStr;
use std::os::raw::{c_char, c_int};
use std::ptr;
use std::sync::atomic::AtomicIsize;

// ─── Struct layouts ───

/// PyASCIIObject — 48 bytes, base for all unicode objects.
#[repr(C)]
pub struct PyASCIIObject {
    pub ob_refcnt: AtomicIsize,   // 8
    pub ob_type: *mut RawPyTypeObject, // 8
    pub length: isize,            // 8
    pub hash: isize,              // 8 (-1 = not computed)
    pub state: u32,               // 4 (bitfield)
    pub _padding: u32,            // 4
    pub wstr: *mut i32,           // 8 (wchar_t, null)
}

/// PyCompactUnicodeObject — 72 bytes, for non-ASCII compact strings.
#[repr(C)]
pub struct PyCompactUnicodeObject {
    pub _base: PyASCIIObject,     // 48
    pub utf8_length: isize,       // 8
    pub utf8: *mut u8,            // 8 (cached UTF-8, heap-allocated)
    pub wstr_length: isize,       // 8
}

const ASCII_HEADER: usize = std::mem::size_of::<PyASCIIObject>();    // 48
const COMPACT_HEADER: usize = std::mem::size_of::<PyCompactUnicodeObject>(); // 72
const _: () = assert!(ASCII_HEADER == 48);
const _: () = assert!(COMPACT_HEADER == 72);

// ─── State bitfield constants ───
// Layout: interned:2 (bits 0-1), kind:3 (bits 2-4), compact:1 (bit 5), ascii:1 (bit 6), ready:1 (bit 7)

const PYUNICODE_1BYTE_KIND: u32 = 1;
const PYUNICODE_2BYTE_KIND: u32 = 2;
const PYUNICODE_4BYTE_KIND: u32 = 4;

#[inline]
fn make_state(kind: u32, compact: bool, ascii: bool) -> u32 {
    let mut state: u32 = 0;
    state |= (kind & 0x7) << 2;         // kind in bits 2-4
    if compact { state |= 1 << 5; }     // compact in bit 5
    if ascii { state |= 1 << 6; }       // ascii in bit 6
    state |= 1 << 7;                    // ready=1 always
    state
}

#[inline]
fn state_kind(state: u32) -> u32 {
    (state >> 2) & 0x7
}

#[inline]
fn state_is_ascii(state: u32) -> bool {
    (state >> 6) & 1 == 1
}

// ─── Type object ───

#[no_mangle]
pub static mut PyUnicode_Type: RawPyTypeObject = {
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"str\0".as_ptr() as *const _;
    tp.tp_basicsize = ASCII_HEADER as isize;
    tp.tp_itemsize = 0;
    tp
};

pub unsafe fn unicode_type() -> *mut RawPyTypeObject {
    &mut PyUnicode_Type
}

unsafe extern "C" fn unicode_dealloc(obj: *mut RawPyObject) {
    let ascii = obj as *mut PyASCIIObject;
    let state = (*ascii).state;
    if !state_is_ascii(state) {
        // Non-ASCII compact: free cached utf8 if allocated
        let compact = obj as *mut PyCompactUnicodeObject;
        if !(*compact).utf8.is_null() {
            libc::free((*compact).utf8 as *mut libc::c_void);
        }
    }
    // Free wstr if allocated (we never allocate it, but be safe)
    if !(*ascii).wstr.is_null() {
        libc::free((*ascii).wstr as *mut libc::c_void);
    }
    libc::free(obj as *mut libc::c_void);
}

// ─── Internal helpers ───

/// Determine the minimum unicode kind needed for a string.
fn determine_kind(s: &str) -> (u32, usize) {
    let mut max_cp: u32 = 0;
    let mut char_count: usize = 0;
    for ch in s.chars() {
        let cp = ch as u32;
        if cp > max_cp { max_cp = cp; }
        char_count += 1;
    }
    let kind = if max_cp < 128 {
        PYUNICODE_1BYTE_KIND
    } else if max_cp < 256 {
        PYUNICODE_1BYTE_KIND
    } else if max_cp < 65536 {
        PYUNICODE_2BYTE_KIND
    } else {
        PYUNICODE_4BYTE_KIND
    };
    (kind, char_count)
}

/// Check if all characters are ASCII.
fn is_ascii(s: &str) -> bool {
    s.bytes().all(|b| b < 128)
}

/// Create a compact ASCII string. Data inline at offset 48.
unsafe fn create_ascii(s: &str) -> *mut RawPyObject {
    let len = s.len();
    let total = ASCII_HEADER + len + 1; // +1 for null terminator
    let raw = libc::calloc(1, total) as *mut PyASCIIObject;
    if raw.is_null() {
        eprintln!("Fatal: out of memory allocating unicode (ascii) object");
        std::process::abort();
    }
    (*raw).ob_refcnt = AtomicIsize::new(1);
    (*raw).ob_type = unicode_type();
    (*raw).length = len as isize;
    (*raw).hash = -1;
    (*raw).state = make_state(PYUNICODE_1BYTE_KIND, true, true);
    (*raw).wstr = ptr::null_mut();
    // Copy data inline at offset 48
    let data = (raw as *mut u8).add(ASCII_HEADER);
    ptr::copy_nonoverlapping(s.as_ptr(), data, len);
    // Null terminator already set by calloc
    raw as *mut RawPyObject
}

/// Create a compact non-ASCII string. Data inline at offset 72.
unsafe fn create_non_ascii(s: &str) -> *mut RawPyObject {
    let (kind, char_count) = determine_kind(s);
    let kind_size = match kind {
        PYUNICODE_1BYTE_KIND => 1,
        PYUNICODE_2BYTE_KIND => 2,
        PYUNICODE_4BYTE_KIND => 4,
        _ => 4,
    };
    let total = COMPACT_HEADER + char_count * kind_size + kind_size; // +kind_size for null terminator
    let raw = libc::calloc(1, total) as *mut PyCompactUnicodeObject;
    if raw.is_null() {
        eprintln!("Fatal: out of memory allocating unicode (non-ascii) object");
        std::process::abort();
    }
    (*raw)._base.ob_refcnt = AtomicIsize::new(1);
    (*raw)._base.ob_type = unicode_type();
    (*raw)._base.length = char_count as isize;
    (*raw)._base.hash = -1;
    (*raw)._base.state = make_state(kind, true, false);
    (*raw)._base.wstr = ptr::null_mut();

    // Write inline data at offset 72
    let data = (raw as *mut u8).add(COMPACT_HEADER);
    match kind {
        PYUNICODE_1BYTE_KIND => {
            let dest = data;
            for (i, ch) in s.chars().enumerate() {
                *dest.add(i) = ch as u8;
            }
        }
        PYUNICODE_2BYTE_KIND => {
            let dest = data as *mut u16;
            for (i, ch) in s.chars().enumerate() {
                *dest.add(i) = ch as u16;
            }
        }
        PYUNICODE_4BYTE_KIND => {
            let dest = data as *mut u32;
            for (i, ch) in s.chars().enumerate() {
                *dest.add(i) = ch as u32;
            }
        }
        _ => {}
    }

    // Cache UTF-8 representation
    let utf8_bytes = s.as_bytes();
    let utf8_buf = libc::malloc(utf8_bytes.len() + 1) as *mut u8;
    if !utf8_buf.is_null() {
        ptr::copy_nonoverlapping(utf8_bytes.as_ptr(), utf8_buf, utf8_bytes.len());
        *utf8_buf.add(utf8_bytes.len()) = 0; // null terminate
        (*raw).utf8 = utf8_buf;
        (*raw).utf8_length = utf8_bytes.len() as isize;
    }
    (*raw).wstr_length = 0;

    raw as *mut RawPyObject
}

/// Create a unicode string from a Rust &str.
unsafe fn create_unicode(s: &str) -> *mut RawPyObject {
    if is_ascii(s) {
        create_ascii(s)
    } else {
        create_non_ascii(s)
    }
}

/// Get the string value as a UTF-8 &str.
/// For ASCII compact: data at offset 48 is valid UTF-8.
/// For non-ASCII compact: read from cached utf8 field.
pub unsafe fn unicode_value(obj: *mut RawPyObject) -> &'static str {
    let ascii = obj as *mut PyASCIIObject;
    let state = (*ascii).state;
    if state_is_ascii(state) {
        // Compact ASCII: data inline at offset 48
        let data = (obj as *mut u8).add(ASCII_HEADER);
        let len = (*ascii).length as usize;
        let bytes = std::slice::from_raw_parts(data, len);
        std::str::from_utf8_unchecked(bytes)
    } else {
        // Non-ASCII compact: read from cached utf8
        let compact = obj as *mut PyCompactUnicodeObject;
        if !(*compact).utf8.is_null() {
            let len = (*compact).utf8_length as usize;
            let bytes = std::slice::from_raw_parts((*compact).utf8, len);
            std::str::from_utf8_unchecked(bytes)
        } else {
            ""
        }
    }
}

// ─── C API ───

#[no_mangle]
pub unsafe extern "C" fn PyUnicode_FromString(s: *const c_char) -> *mut RawPyObject {
    if s.is_null() {
        return ptr::null_mut();
    }
    let c_str = CStr::from_ptr(s);
    match c_str.to_str() {
        Ok(rust_str) => create_unicode(rust_str),
        Err(_) => create_unicode(&c_str.to_string_lossy()),
    }
}

#[no_mangle]
pub unsafe extern "C" fn PyUnicode_FromStringAndSize(
    s: *const c_char,
    size: isize,
) -> *mut RawPyObject {
    if s.is_null() || size < 0 {
        return create_unicode("");
    }
    let slice = std::slice::from_raw_parts(s as *const u8, size as usize);
    let string = String::from_utf8_lossy(slice);
    create_unicode(&string)
}

#[no_mangle]
pub unsafe extern "C" fn PyUnicode_FromFormat(
    format: *const c_char,
) -> *mut RawPyObject {
    if format.is_null() {
        return create_unicode("");
    }
    let c_str = CStr::from_ptr(format);
    create_unicode(&c_str.to_string_lossy())
}

#[no_mangle]
pub unsafe extern "C" fn PyUnicode_AsUTF8(obj: *mut RawPyObject) -> *const c_char {
    if obj.is_null() {
        return ptr::null();
    }
    let ascii = obj as *mut PyASCIIObject;
    let state = (*ascii).state;
    if state_is_ascii(state) {
        // Compact ASCII: data at offset 48, already null-terminated
        (obj as *mut u8).add(ASCII_HEADER) as *const c_char
    } else {
        // Non-ASCII compact: return cached utf8
        let compact = obj as *mut PyCompactUnicodeObject;
        if !(*compact).utf8.is_null() {
            (*compact).utf8 as *const c_char
        } else {
            ptr::null()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn PyUnicode_AsUTF8AndSize(
    obj: *mut RawPyObject,
    size: *mut isize,
) -> *const c_char {
    if obj.is_null() {
        return ptr::null();
    }
    let ascii = obj as *mut PyASCIIObject;
    let state = (*ascii).state;
    if state_is_ascii(state) {
        if !size.is_null() {
            *size = (*ascii).length;
        }
        (obj as *mut u8).add(ASCII_HEADER) as *const c_char
    } else {
        let compact = obj as *mut PyCompactUnicodeObject;
        if !size.is_null() {
            *size = (*compact).utf8_length;
        }
        if !(*compact).utf8.is_null() {
            (*compact).utf8 as *const c_char
        } else {
            ptr::null()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn PyUnicode_AsEncodedString(
    obj: *mut RawPyObject,
    _encoding: *const c_char,
    _errors: *const c_char,
) -> *mut RawPyObject {
    if obj.is_null() {
        return ptr::null_mut();
    }
    // Encode as UTF-8
    let s = unicode_value(obj);
    crate::types::bytes::create_bytes_from_slice(s.as_bytes())
}

#[no_mangle]
pub unsafe extern "C" fn PyUnicode_GetLength(obj: *mut RawPyObject) -> isize {
    if obj.is_null() {
        return -1;
    }
    let ascii = obj as *mut PyASCIIObject;
    (*ascii).length
}

#[no_mangle]
pub unsafe extern "C" fn PyUnicode_GET_LENGTH(obj: *mut RawPyObject) -> isize {
    PyUnicode_GetLength(obj)
}

#[no_mangle]
pub unsafe extern "C" fn PyUnicode_Check(obj: *mut RawPyObject) -> c_int {
    if obj.is_null() {
        return 0;
    }
    if (*obj).ob_type == unicode_type() { 1 } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn PyUnicode_CompareWithASCIIString(
    obj: *mut RawPyObject,
    string: *const c_char,
) -> c_int {
    if obj.is_null() || string.is_null() {
        return -1;
    }
    let s = unicode_value(obj);
    let c_str = CStr::from_ptr(string);
    match c_str.to_str() {
        Ok(other) => {
            if s == other { 0 }
            else if s < other { -1 }
            else { 1 }
        }
        Err(_) => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn PyUnicode_Concat(
    left: *mut RawPyObject,
    right: *mut RawPyObject,
) -> *mut RawPyObject {
    if left.is_null() || right.is_null() {
        return ptr::null_mut();
    }
    let l = unicode_value(left);
    let r = unicode_value(right);
    let mut result = String::with_capacity(l.len() + r.len());
    result.push_str(l);
    result.push_str(r);
    create_unicode(&result)
}

#[no_mangle]
pub unsafe extern "C" fn PyUnicode_Join(
    _separator: *mut RawPyObject,
    _seq: *mut RawPyObject,
) -> *mut RawPyObject {
    // TODO: Implement properly with sequence protocol
    ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn PyUnicode_InternInPlace(_p: *mut *mut RawPyObject) {
    // No-op (interning is an optimization, not correctness-critical)
}

#[no_mangle]
pub unsafe extern "C" fn PyUnicode_InternFromString(s: *const c_char) -> *mut RawPyObject {
    PyUnicode_FromString(s)
}

/// Helper: create a Python string from a Rust &str
pub unsafe fn create_from_str(s: &str) -> *mut RawPyObject {
    create_unicode(s)
}

/// PyUnicode_New — allocate an empty unicode object with room for `size` characters
/// of at most `maxchar` codepoint. The caller writes directly into the data buffer
/// via PyUnicode_1BYTE_DATA / PyUnicode_2BYTE_DATA / PyUnicode_4BYTE_DATA macros.
#[no_mangle]
pub unsafe extern "C" fn PyUnicode_New(size: isize, maxchar: u32) -> *mut RawPyObject {
    if size < 0 {
        return ptr::null_mut();
    }
    let len = size as usize;

    if maxchar < 128 {
        // ASCII compact: header=48, data=len+1
        let total = ASCII_HEADER + len + 1;
        let raw = libc::calloc(1, total) as *mut PyASCIIObject;
        if raw.is_null() { std::process::abort(); }
        (*raw).ob_refcnt = AtomicIsize::new(1);
        (*raw).ob_type = unicode_type();
        (*raw).length = size;
        (*raw).hash = -1;
        (*raw).state = make_state(PYUNICODE_1BYTE_KIND, true, true);
        (*raw).wstr = ptr::null_mut();
        raw as *mut RawPyObject
    } else {
        // Non-ASCII compact: header=72
        let (kind, kind_size) = if maxchar < 256 {
            (PYUNICODE_1BYTE_KIND, 1usize)
        } else if maxchar < 65536 {
            (PYUNICODE_2BYTE_KIND, 2usize)
        } else {
            (PYUNICODE_4BYTE_KIND, 4usize)
        };
        let total = COMPACT_HEADER + len * kind_size + kind_size; // +kind_size for null terminator
        let raw = libc::calloc(1, total) as *mut PyCompactUnicodeObject;
        if raw.is_null() { std::process::abort(); }
        (*raw)._base.ob_refcnt = AtomicIsize::new(1);
        (*raw)._base.ob_type = unicode_type();
        (*raw)._base.length = size;
        (*raw)._base.hash = -1;
        (*raw)._base.state = make_state(kind, true, false);
        (*raw)._base.wstr = ptr::null_mut();
        (*raw).utf8 = ptr::null_mut();
        (*raw).utf8_length = 0;
        (*raw).wstr_length = 0;
        raw as *mut RawPyObject
    }
}

/// PyUnicode_FromKindAndData — create a unicode string from a buffer of
/// code points of the given kind (1, 2, or 4 bytes per character).
#[no_mangle]
pub unsafe extern "C" fn PyUnicode_FromKindAndData(
    kind: c_int,
    buffer: *const std::os::raw::c_void,
    size: isize,
) -> *mut RawPyObject {
    if buffer.is_null() || size < 0 {
        return ptr::null_mut();
    }
    let len = size as usize;
    if len == 0 {
        return create_ascii("");
    }

    match kind as u32 {
        PYUNICODE_1BYTE_KIND => {
            let data = std::slice::from_raw_parts(buffer as *const u8, len);
            // Check if all ASCII
            let all_ascii = data.iter().all(|&b| b < 128);
            if all_ascii {
                let s = std::str::from_utf8_unchecked(data);
                create_ascii(s)
            } else {
                // Latin-1: convert each byte to char
                let s: String = data.iter().map(|&b| b as char).collect();
                create_non_ascii(&s)
            }
        }
        PYUNICODE_2BYTE_KIND => {
            let data = std::slice::from_raw_parts(buffer as *const u16, len);
            let s: String = data.iter().filter_map(|&cp| char::from_u32(cp as u32)).collect();
            create_unicode(&s)
        }
        PYUNICODE_4BYTE_KIND => {
            let data = std::slice::from_raw_parts(buffer as *const u32, len);
            let s: String = data.iter().filter_map(|&cp| char::from_u32(cp)).collect();
            create_unicode(&s)
        }
        _ => ptr::null_mut(),
    }
}

/// PyUnicode_DecodeUTF8 — decode a UTF-8 byte string to a Python unicode object.
#[no_mangle]
pub unsafe extern "C" fn PyUnicode_DecodeUTF8(
    s: *const c_char,
    size: isize,
    _errors: *const c_char,
) -> *mut RawPyObject {
    if s.is_null() || size < 0 {
        return ptr::null_mut();
    }
    let slice = std::slice::from_raw_parts(s as *const u8, size as usize);
    let string = String::from_utf8_lossy(slice);
    create_unicode(&string)
}

/// _PyUnicode_Ready — ensure a unicode object is in canonical form.
/// In CPython 3.11 this is mostly a no-op (all new strings are "ready").
/// Prebuilt extensions still reference this symbol.
#[no_mangle]
pub unsafe extern "C" fn _PyUnicode_Ready(_obj: *mut RawPyObject) -> std::os::raw::c_int {
    0 // Already ready — all our strings are created in canonical PEP 393 form
}

pub unsafe fn init_unicode_type() {
    PyUnicode_Type.tp_dealloc = Some(unicode_dealloc);
    PyUnicode_Type.tp_flags = PY_TPFLAGS_DEFAULT | PY_TPFLAGS_UNICODE_SUBCLASS;
}
