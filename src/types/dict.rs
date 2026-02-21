//! Python dict type — CPython 3.11 exact ABI layout.
//!
//! CPython layout (48 bytes on 64-bit):
//!   PyObject ob_base          (16 bytes: refcnt + type)
//!   Py_ssize_t ma_used        (8 bytes: # active entries)
//!   uint64_t ma_version_tag   (8 bytes)
//!   PyDictKeysObject *ma_keys (8 bytes)
//!   PyObject **ma_values      (8 bytes, null = combined table)
//!
//! Compact dict: index table + entries array in PyDictKeysObject.
//! Entries are in insertion order. Index table maps hash -> entry index.

use crate::object::pyobject::RawPyObject;
use crate::object::typeobj::{RawPyTypeObject, PY_TPFLAGS_DEFAULT, PY_TPFLAGS_DICT_SUBCLASS, PY_TPFLAGS_HAVE_GC};
use std::os::raw::c_int;
use std::ptr;

// ─── Struct layouts ───

#[repr(C)]
pub struct PyDictObject {
    pub ob_base: RawPyObject,                    // 16
    pub ma_used: isize,                          // 8
    pub ma_version_tag: u64,                     // 8
    pub ma_keys: *mut PyDictKeysObject,          // 8
    pub ma_values: *mut *mut RawPyObject,        // 8 (null = combined)
}

const _: () = assert!(std::mem::size_of::<PyDictObject>() == 48);

#[repr(C)]
pub struct PyDictKeysObject {
    pub dk_refcnt: isize,            // 8
    pub dk_log2_size: u8,            // 1
    pub dk_log2_index_bytes: u8,     // 1
    pub dk_kind: u8,                 // 1
    pub _padding: u8,                // 1
    pub dk_version: u32,             // 4
    pub dk_usable: isize,            // 8
    pub dk_nentries: isize,          // 8
    // Followed by: dk_indices[1 << dk_log2_size] of isize
    // Followed by: dk_entries[capacity] of PyDictKeyEntry
}

const DK_HEADER_SIZE: usize = std::mem::size_of::<PyDictKeysObject>(); // 32
const _: () = assert!(DK_HEADER_SIZE == 32);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PyDictKeyEntry {
    pub me_hash: isize,
    pub me_key: *mut RawPyObject,
    pub me_value: *mut RawPyObject,
}

const ENTRY_SIZE: usize = std::mem::size_of::<PyDictKeyEntry>(); // 24
const _: () = assert!(ENTRY_SIZE == 24);

const DKIX_EMPTY: isize = -1;
const DKIX_DUMMY: isize = -2;
const DK_LOG2_MIN: u8 = 3; // minimum index table size = 8

// ─── Type object ───

static mut DICT_TYPE: RawPyTypeObject = {
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"dict\0".as_ptr() as *const _;
    tp.tp_basicsize = 48; // sizeof(PyDictObject)
    tp.tp_itemsize = 0;
    tp
};

pub unsafe fn dict_type() -> *mut RawPyTypeObject {
    &mut DICT_TYPE
}

// ─── Hashing and equality ───

fn hash_string(s: &str) -> isize {
    // FNV-1a hash
    let mut h: u64 = 14695981039346656037;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    let h = h as isize;
    if h == -1 { -2 } else { h } // -1 is reserved for "not computed"
}

unsafe fn hash_object(obj: *mut RawPyObject) -> isize {
    if (*obj).ob_type == crate::types::unicode::unicode_type() {
        hash_string(crate::types::unicode::unicode_value(obj))
    } else if (*obj).ob_type == crate::types::longobject::long_type()
           || (*obj).ob_type == crate::types::boolobject::bool_type() {
        let v = crate::types::longobject::long_as_i64(obj);
        if v == -1 { -2 } else { v as isize }
    } else {
        let h = obj as isize;
        if h == -1 { -2 } else { h }
    }
}

unsafe fn keys_equal(a: *mut RawPyObject, b: *mut RawPyObject) -> bool {
    if a == b { return true; }
    if a.is_null() || b.is_null() { return false; }
    if (*a).ob_type == crate::types::unicode::unicode_type()
        && (*b).ob_type == crate::types::unicode::unicode_type()
    {
        return crate::types::unicode::unicode_value(a) == crate::types::unicode::unicode_value(b);
    }
    if ((*a).ob_type == crate::types::longobject::long_type()
        || (*a).ob_type == crate::types::boolobject::bool_type())
       && ((*b).ob_type == crate::types::longobject::long_type()
           || (*b).ob_type == crate::types::boolobject::bool_type())
    {
        return crate::types::longobject::long_as_i64(a) == crate::types::longobject::long_as_i64(b);
    }
    false
}

// ─── Keys object helpers ───

fn dk_usable_for_size(log2_size: u8) -> isize {
    let size = 1isize << log2_size;
    (size * 2) / 3
}

/// Get pointer to index table (starts right after header).
#[inline]
unsafe fn dk_indices(keys: *mut PyDictKeysObject) -> *mut isize {
    (keys as *mut u8).add(DK_HEADER_SIZE) as *mut isize
}

/// Get pointer to entries array (starts after index table).
#[inline]
unsafe fn dk_entries(keys: *mut PyDictKeysObject) -> *mut PyDictKeyEntry {
    let index_table_size = (1usize << (*keys).dk_log2_size) * std::mem::size_of::<isize>();
    (keys as *mut u8).add(DK_HEADER_SIZE + index_table_size) as *mut PyDictKeyEntry
}

/// Allocate a new PyDictKeysObject with the given log2 size.
unsafe fn alloc_keys(log2_size: u8) -> *mut PyDictKeysObject {
    let table_size = 1usize << log2_size;
    let usable = dk_usable_for_size(log2_size) as usize;
    let index_bytes = table_size * std::mem::size_of::<isize>();
    let entries_bytes = usable * ENTRY_SIZE;
    let total = DK_HEADER_SIZE + index_bytes + entries_bytes;
    let raw = libc::malloc(total) as *mut u8;
    if raw.is_null() {
        eprintln!("Fatal: out of memory allocating dict keys");
        std::process::abort();
    }
    // Zero everything
    ptr::write_bytes(raw, 0, total);

    let keys = raw as *mut PyDictKeysObject;
    (*keys).dk_refcnt = 1;
    (*keys).dk_log2_size = log2_size;
    (*keys).dk_log2_index_bytes = 3; // always isize (8 bytes)
    (*keys).dk_kind = 0; // DICT_KEYS_GENERAL
    (*keys).dk_usable = dk_usable_for_size(log2_size);
    (*keys).dk_nentries = 0;

    // Fill index table with DKIX_EMPTY (-1)
    let indices = dk_indices(keys);
    for i in 0..table_size {
        *indices.add(i) = DKIX_EMPTY;
    }

    keys
}

unsafe fn free_keys(keys: *mut PyDictKeysObject) {
    if !keys.is_null() {
        libc::free(keys as *mut libc::c_void);
    }
}

/// Find an index slot for the given hash + key.
/// Returns (index_slot, entry_index) where entry_index is the existing entry
/// or DKIX_EMPTY if not found. index_slot is where to insert.
unsafe fn find_slot(
    keys: *mut PyDictKeysObject,
    hash: isize,
    key: *mut RawPyObject,
) -> (usize, isize) {
    let mask = (1usize << (*keys).dk_log2_size) - 1;
    let indices = dk_indices(keys);
    let entries = dk_entries(keys);
    let mut i = (hash as usize) & mask;
    let mut first_dummy: Option<usize> = None;

    loop {
        let ix = *indices.add(i);
        if ix == DKIX_EMPTY {
            return (first_dummy.unwrap_or(i), DKIX_EMPTY);
        }
        if ix == DKIX_DUMMY {
            if first_dummy.is_none() {
                first_dummy = Some(i);
            }
        } else {
            let entry = &*entries.add(ix as usize);
            if entry.me_hash == hash && keys_equal(entry.me_key, key) {
                return (i, ix);
            }
        }
        i = (i + 1) & mask;
    }
}

/// Find entry for the given key. Returns entry index or DKIX_EMPTY.
unsafe fn lookup_key(
    keys: *mut PyDictKeysObject,
    hash: isize,
    key: *mut RawPyObject,
) -> isize {
    let (_, entry_ix) = find_slot(keys, hash, key);
    entry_ix
}

// ─── Dealloc ───

unsafe extern "C" fn dict_dealloc(obj: *mut RawPyObject) {
    let d = obj as *mut PyDictObject;
    let keys = (*d).ma_keys;
    if !keys.is_null() {
        let entries = dk_entries(keys);
        let n = (*keys).dk_nentries;
        for i in 0..n as usize {
            let entry = &*entries.add(i);
            if !entry.me_key.is_null() {
                (*entry.me_key).decref();
            }
            if !entry.me_value.is_null() {
                (*entry.me_value).decref();
            }
        }
        free_keys(keys);
    }
    crate::object::gc::PyObject_GC_Del(obj as *mut libc::c_void);
}

// ─── Resize ───

unsafe fn dict_resize(d: *mut PyDictObject, min_used: isize) {
    // Find smallest log2 size that fits
    let mut log2 = DK_LOG2_MIN;
    while dk_usable_for_size(log2) < min_used {
        log2 += 1;
        if log2 > 30 { std::process::abort(); }
    }

    let old_keys = (*d).ma_keys;
    let new_keys = alloc_keys(log2);

    if !old_keys.is_null() {
        let old_entries = dk_entries(old_keys);
        let old_n = (*old_keys).dk_nentries;
        let new_indices = dk_indices(new_keys);
        let new_entries = dk_entries(new_keys);
        let new_mask = (1usize << log2) - 1;
        let mut new_n: isize = 0;

        for i in 0..old_n as usize {
            let entry = &*old_entries.add(i);
            if entry.me_key.is_null() || entry.me_value.is_null() {
                continue; // deleted entry
            }
            // Insert into new table
            let mut slot = (entry.me_hash as usize) & new_mask;
            while *new_indices.add(slot) != DKIX_EMPTY {
                slot = (slot + 1) & new_mask;
            }
            *new_indices.add(slot) = new_n;
            *new_entries.add(new_n as usize) = *entry;
            new_n += 1;
        }
        (*new_keys).dk_nentries = new_n;
        (*new_keys).dk_usable = dk_usable_for_size(log2) - new_n;
        free_keys(old_keys);
    }

    (*d).ma_keys = new_keys;
}

// ─── C API ───

#[no_mangle]
pub unsafe extern "C" fn PyDict_New() -> *mut RawPyObject {
    let obj = crate::object::gc::_PyObject_GC_New(&mut DICT_TYPE) as *mut PyDictObject;
    (*obj).ma_used = 0;
    (*obj).ma_version_tag = 0;
    (*obj).ma_keys = alloc_keys(DK_LOG2_MIN);
    (*obj).ma_values = ptr::null_mut();
    obj as *mut RawPyObject
}

#[no_mangle]
pub unsafe extern "C" fn PyDict_SetItem(
    dict: *mut RawPyObject,
    key: *mut RawPyObject,
    value: *mut RawPyObject,
) -> c_int {
    if dict.is_null() || key.is_null() { return -1; }
    let d = dict as *mut PyDictObject;
    let hash = hash_object(key);

    let (slot, entry_ix) = find_slot((*d).ma_keys, hash, key);

    if entry_ix >= 0 {
        // Key already exists — replace value
        let entries = dk_entries((*d).ma_keys);
        let entry = &mut *entries.add(entry_ix as usize);
        let old_val = entry.me_value;
        if !value.is_null() { (*value).incref(); }
        entry.me_value = value;
        if !old_val.is_null() { (*old_val).decref(); }
    } else {
        // New key
        if (*(*d).ma_keys).dk_usable <= 0 {
            // Resize needed
            dict_resize(d, (*d).ma_used + 1);
            // Re-find slot in new table
            let (new_slot, _) = find_slot((*d).ma_keys, hash, key);
            let keys = (*d).ma_keys;
            let entries = dk_entries(keys);
            let indices = dk_indices(keys);
            let n = (*keys).dk_nentries;
            *indices.add(new_slot) = n;
            let entry = &mut *entries.add(n as usize);
            entry.me_hash = hash;
            (*key).incref();
            entry.me_key = key;
            if !value.is_null() { (*value).incref(); }
            entry.me_value = value;
            (*keys).dk_nentries = n + 1;
            (*keys).dk_usable -= 1;
        } else {
            let keys = (*d).ma_keys;
            let entries = dk_entries(keys);
            let indices = dk_indices(keys);
            let n = (*keys).dk_nentries;
            *indices.add(slot) = n;
            let entry = &mut *entries.add(n as usize);
            entry.me_hash = hash;
            (*key).incref();
            entry.me_key = key;
            if !value.is_null() { (*value).incref(); }
            entry.me_value = value;
            (*keys).dk_nentries = n + 1;
            (*keys).dk_usable -= 1;
        }
        (*d).ma_used += 1;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn PyDict_SetItemString(
    dict: *mut RawPyObject,
    key: *const std::os::raw::c_char,
    value: *mut RawPyObject,
) -> c_int {
    if key.is_null() { return -1; }
    let key_obj = crate::types::unicode::PyUnicode_FromString(key);
    let result = PyDict_SetItem(dict, key_obj, value);
    (*key_obj).decref();
    result
}

#[no_mangle]
pub unsafe extern "C" fn PyDict_GetItem(
    dict: *mut RawPyObject,
    key: *mut RawPyObject,
) -> *mut RawPyObject {
    if dict.is_null() || key.is_null() { return ptr::null_mut(); }
    let d = dict as *mut PyDictObject;
    let hash = hash_object(key);
    let entry_ix = lookup_key((*d).ma_keys, hash, key);
    if entry_ix >= 0 {
        let entries = dk_entries((*d).ma_keys);
        (*entries.add(entry_ix as usize)).me_value
    } else {
        ptr::null_mut()
    }
}

#[no_mangle]
pub unsafe extern "C" fn PyDict_GetItemString(
    dict: *mut RawPyObject,
    key: *const std::os::raw::c_char,
) -> *mut RawPyObject {
    if dict.is_null() || key.is_null() { return ptr::null_mut(); }
    // Create a temp key, look up by value
    let key_obj = crate::types::unicode::PyUnicode_FromString(key);
    let result = PyDict_GetItem(dict, key_obj);
    (*key_obj).decref();
    result
}

#[no_mangle]
pub unsafe extern "C" fn PyDict_GetItemWithError(
    dict: *mut RawPyObject,
    key: *mut RawPyObject,
) -> *mut RawPyObject {
    PyDict_GetItem(dict, key)
}

#[no_mangle]
pub unsafe extern "C" fn PyDict_DelItem(
    dict: *mut RawPyObject,
    key: *mut RawPyObject,
) -> c_int {
    if dict.is_null() || key.is_null() { return -1; }
    let d = dict as *mut PyDictObject;
    let hash = hash_object(key);
    let (slot, entry_ix) = find_slot((*d).ma_keys, hash, key);
    if entry_ix < 0 { return -1; }

    let keys = (*d).ma_keys;
    let indices = dk_indices(keys);
    let entries = dk_entries(keys);
    *indices.add(slot) = DKIX_DUMMY;
    let entry = &mut *entries.add(entry_ix as usize);
    if !entry.me_key.is_null() { (*entry.me_key).decref(); }
    if !entry.me_value.is_null() { (*entry.me_value).decref(); }
    entry.me_key = ptr::null_mut();
    entry.me_value = ptr::null_mut();
    entry.me_hash = 0;
    (*d).ma_used -= 1;
    0
}

#[no_mangle]
pub unsafe extern "C" fn PyDict_DelItemString(
    dict: *mut RawPyObject,
    key: *const std::os::raw::c_char,
) -> c_int {
    if key.is_null() { return -1; }
    let key_obj = crate::types::unicode::PyUnicode_FromString(key);
    let result = PyDict_DelItem(dict, key_obj);
    (*key_obj).decref();
    result
}

#[no_mangle]
pub unsafe extern "C" fn PyDict_Contains(
    dict: *mut RawPyObject,
    key: *mut RawPyObject,
) -> c_int {
    if dict.is_null() || key.is_null() { return -1; }
    let d = dict as *mut PyDictObject;
    let hash = hash_object(key);
    if lookup_key((*d).ma_keys, hash, key) >= 0 { 1 } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn PyDict_Size(dict: *mut RawPyObject) -> isize {
    if dict.is_null() { return -1; }
    let d = dict as *mut PyDictObject;
    (*d).ma_used
}

#[no_mangle]
pub unsafe extern "C" fn PyDict_Keys(dict: *mut RawPyObject) -> *mut RawPyObject {
    if dict.is_null() { return ptr::null_mut(); }
    let d = dict as *mut PyDictObject;
    let list = crate::types::list::PyList_New((*d).ma_used);
    let entries = dk_entries((*d).ma_keys);
    let n = (*(*d).ma_keys).dk_nentries;
    let mut j: isize = 0;
    for i in 0..n as usize {
        let entry = &*entries.add(i);
        if !entry.me_key.is_null() && !entry.me_value.is_null() {
            (*entry.me_key).incref();
            crate::types::list::PyList_SET_ITEM(list, j, entry.me_key);
            j += 1;
        }
    }
    list
}

#[no_mangle]
pub unsafe extern "C" fn PyDict_Values(dict: *mut RawPyObject) -> *mut RawPyObject {
    if dict.is_null() { return ptr::null_mut(); }
    let d = dict as *mut PyDictObject;
    let list = crate::types::list::PyList_New((*d).ma_used);
    let entries = dk_entries((*d).ma_keys);
    let n = (*(*d).ma_keys).dk_nentries;
    let mut j: isize = 0;
    for i in 0..n as usize {
        let entry = &*entries.add(i);
        if !entry.me_key.is_null() && !entry.me_value.is_null() {
            (*entry.me_value).incref();
            crate::types::list::PyList_SET_ITEM(list, j, entry.me_value);
            j += 1;
        }
    }
    list
}

#[no_mangle]
pub unsafe extern "C" fn PyDict_Items(dict: *mut RawPyObject) -> *mut RawPyObject {
    if dict.is_null() { return ptr::null_mut(); }
    let d = dict as *mut PyDictObject;
    let list = crate::types::list::PyList_New((*d).ma_used);
    let entries = dk_entries((*d).ma_keys);
    let n = (*(*d).ma_keys).dk_nentries;
    let mut j: isize = 0;
    for i in 0..n as usize {
        let entry = &*entries.add(i);
        if !entry.me_key.is_null() && !entry.me_value.is_null() {
            let tuple = crate::types::tuple::PyTuple_New(2);
            (*entry.me_key).incref();
            crate::types::tuple::PyTuple_SET_ITEM(tuple, 0, entry.me_key);
            (*entry.me_value).incref();
            crate::types::tuple::PyTuple_SET_ITEM(tuple, 1, entry.me_value);
            crate::types::list::PyList_SET_ITEM(list, j, tuple);
            j += 1;
        }
    }
    list
}

#[no_mangle]
pub unsafe extern "C" fn PyDict_Next(
    dict: *mut RawPyObject,
    pos: *mut isize,
    key: *mut *mut RawPyObject,
    value: *mut *mut RawPyObject,
) -> c_int {
    if dict.is_null() || pos.is_null() { return 0; }
    let d = dict as *mut PyDictObject;
    let keys = (*d).ma_keys;
    let entries = dk_entries(keys);
    let n = (*keys).dk_nentries;
    let mut idx = *pos;

    // Skip deleted entries
    while idx < n {
        let entry = &*entries.add(idx as usize);
        if !entry.me_key.is_null() && !entry.me_value.is_null() {
            if !key.is_null() { *key = entry.me_key; }
            if !value.is_null() { *value = entry.me_value; }
            *pos = idx + 1;
            return 1;
        }
        idx += 1;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn PyDict_Clear(dict: *mut RawPyObject) {
    if dict.is_null() { return; }
    let d = dict as *mut PyDictObject;
    let keys = (*d).ma_keys;
    if !keys.is_null() {
        let entries = dk_entries(keys);
        let n = (*keys).dk_nentries;
        for i in 0..n as usize {
            let entry = &mut *entries.add(i);
            if !entry.me_key.is_null() { (*entry.me_key).decref(); }
            if !entry.me_value.is_null() { (*entry.me_value).decref(); }
        }
        free_keys(keys);
    }
    (*d).ma_keys = alloc_keys(DK_LOG2_MIN);
    (*d).ma_used = 0;
}

#[no_mangle]
pub unsafe extern "C" fn PyDict_Copy(dict: *mut RawPyObject) -> *mut RawPyObject {
    if dict.is_null() { return ptr::null_mut(); }
    let d = dict as *mut PyDictObject;
    let new_dict = PyDict_New();
    let entries = dk_entries((*d).ma_keys);
    let n = (*(*d).ma_keys).dk_nentries;
    for i in 0..n as usize {
        let entry = &*entries.add(i);
        if !entry.me_key.is_null() && !entry.me_value.is_null() {
            PyDict_SetItem(new_dict, entry.me_key, entry.me_value);
        }
    }
    new_dict
}

#[no_mangle]
pub unsafe extern "C" fn PyDict_Update(
    dict: *mut RawPyObject,
    other: *mut RawPyObject,
) -> c_int {
    if dict.is_null() || other.is_null() { return -1; }
    let o = other as *mut PyDictObject;
    let entries = dk_entries((*o).ma_keys);
    let n = (*(*o).ma_keys).dk_nentries;
    for i in 0..n as usize {
        let entry = &*entries.add(i);
        if !entry.me_key.is_null() && !entry.me_value.is_null() {
            PyDict_SetItem(dict, entry.me_key, entry.me_value);
        }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn PyDict_Merge(
    dict: *mut RawPyObject,
    other: *mut RawPyObject,
    _override_: c_int,
) -> c_int {
    PyDict_Update(dict, other)
}

#[no_mangle]
pub unsafe extern "C" fn PyDict_Check(obj: *mut RawPyObject) -> c_int {
    if obj.is_null() { return 0; }
    if (*obj).ob_type == dict_type() { 1 } else { 0 }
}

#[no_mangle]
pub static mut PyDict_Type: *mut RawPyTypeObject = ptr::null_mut();

pub unsafe fn init_dict_type() {
    DICT_TYPE.tp_dealloc = Some(dict_dealloc);
    DICT_TYPE.tp_flags = PY_TPFLAGS_DEFAULT | PY_TPFLAGS_DICT_SUBCLASS | PY_TPFLAGS_HAVE_GC;
    PyDict_Type = dict_type();
}
