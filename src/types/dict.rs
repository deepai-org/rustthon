//! Python dict type.
//!
//! Dicts are insertion-ordered hash maps (since Python 3.7).
//! We use IndexMap internally for this guarantee.

use crate::object::pyobject::{PyObjectWithData, RawPyObject};
use crate::object::typeobj::RawPyTypeObject;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::os::raw::c_int;
use std::ptr;

static mut DICT_TYPE: RawPyTypeObject = {
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"dict\0".as_ptr() as *const _;
    tp.tp_basicsize = 0;
    tp
};

pub unsafe fn dict_type() -> *mut RawPyTypeObject {
    &mut DICT_TYPE
}

/// Dict key wrapper that hashes by pointer identity for now.
/// A full implementation would call __hash__ on the key object.
#[derive(Clone)]
struct DictKey {
    ptr: *mut RawPyObject,
}

impl PartialEq for DictKey {
    fn eq(&self, other: &Self) -> bool {
        // Pointer identity first (fast path)
        if self.ptr == other.ptr {
            return true;
        }
        // TODO: Call __eq__ for value equality
        false
    }
}

impl Eq for DictKey {}

impl Hash for DictKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Use pointer as hash for now.
        // TODO: Call __hash__ on the object
        (self.ptr as usize).hash(state);
    }
}

pub struct DictData {
    pub entries: indexmap::IndexMap<DictKey, *mut RawPyObject>,
}

type PyDictObject = PyObjectWithData<DictData>;

// ─── C API ───

/// PyDict_New
#[no_mangle]
pub unsafe extern "C" fn PyDict_New() -> *mut RawPyObject {
    let obj = PyObjectWithData::alloc(
        &mut DICT_TYPE,
        DictData {
            entries: indexmap::IndexMap::new(),
        },
    );
    obj as *mut RawPyObject
}

/// PyDict_SetItem - add key/value (increfs both)
#[no_mangle]
pub unsafe extern "C" fn PyDict_SetItem(
    dict: *mut RawPyObject,
    key: *mut RawPyObject,
    value: *mut RawPyObject,
) -> c_int {
    if dict.is_null() || key.is_null() {
        return -1;
    }
    let data = PyObjectWithData::<DictData>::data_from_raw_mut(dict);
    (*key).incref();
    if !value.is_null() {
        (*value).incref();
    }
    // If replacing an existing entry, decref old value
    if let Some(old_val) = data.entries.insert(DictKey { ptr: key }, value) {
        if !old_val.is_null() {
            (*old_val).decref();
        }
        // Key was already present, decref the extra incref
        (*key).decref();
    }
    0
}

/// PyDict_SetItemString - convenience with C string key
#[no_mangle]
pub unsafe extern "C" fn PyDict_SetItemString(
    dict: *mut RawPyObject,
    key: *const std::os::raw::c_char,
    value: *mut RawPyObject,
) -> c_int {
    if key.is_null() {
        return -1;
    }
    let key_obj = crate::types::unicode::PyUnicode_FromString(key);
    let result = PyDict_SetItem(dict, key_obj, value);
    // SetItem increfs key, so we can release our reference
    (*key_obj).decref();
    result
}

/// PyDict_GetItem - get value by key (borrowed reference, returns NULL on miss)
#[no_mangle]
pub unsafe extern "C" fn PyDict_GetItem(
    dict: *mut RawPyObject,
    key: *mut RawPyObject,
) -> *mut RawPyObject {
    if dict.is_null() || key.is_null() {
        return ptr::null_mut();
    }
    let data = PyObjectWithData::<DictData>::data_from_raw(dict);
    match data.entries.get(&DictKey { ptr: key }) {
        Some(&value) => value,
        None => ptr::null_mut(),
    }
}

/// PyDict_GetItemString
#[no_mangle]
pub unsafe extern "C" fn PyDict_GetItemString(
    dict: *mut RawPyObject,
    key: *const std::os::raw::c_char,
) -> *mut RawPyObject {
    if key.is_null() {
        return ptr::null_mut();
    }
    // For string lookups, we need to check by string value not pointer
    // This is a common operation in C extensions
    let data = PyObjectWithData::<DictData>::data_from_raw(dict);
    let key_str = std::ffi::CStr::from_ptr(key).to_string_lossy();

    for (k, &v) in &data.entries {
        if !k.ptr.is_null() && (*k.ptr).ob_type == crate::types::unicode::unicode_type() {
            let k_str = crate::types::unicode::unicode_value(k.ptr);
            if k_str == key_str.as_ref() {
                return v;
            }
        }
    }
    ptr::null_mut()
}

/// PyDict_GetItemWithError
#[no_mangle]
pub unsafe extern "C" fn PyDict_GetItemWithError(
    dict: *mut RawPyObject,
    key: *mut RawPyObject,
) -> *mut RawPyObject {
    PyDict_GetItem(dict, key)
}

/// PyDict_DelItem
#[no_mangle]
pub unsafe extern "C" fn PyDict_DelItem(
    dict: *mut RawPyObject,
    key: *mut RawPyObject,
) -> c_int {
    if dict.is_null() || key.is_null() {
        return -1;
    }
    let data = PyObjectWithData::<DictData>::data_from_raw_mut(dict);
    match data.entries.shift_remove(&DictKey { ptr: key }) {
        Some(old_val) => {
            if !old_val.is_null() {
                (*old_val).decref();
            }
            (*key).decref();
            0
        }
        None => -1, // Key not found
    }
}

/// PyDict_DelItemString
#[no_mangle]
pub unsafe extern "C" fn PyDict_DelItemString(
    dict: *mut RawPyObject,
    key: *const std::os::raw::c_char,
) -> c_int {
    // TODO: Lookup by string value
    -1
}

/// PyDict_Contains
#[no_mangle]
pub unsafe extern "C" fn PyDict_Contains(
    dict: *mut RawPyObject,
    key: *mut RawPyObject,
) -> c_int {
    if dict.is_null() || key.is_null() {
        return -1;
    }
    let data = PyObjectWithData::<DictData>::data_from_raw(dict);
    if data.entries.contains_key(&DictKey { ptr: key }) {
        1
    } else {
        0
    }
}

/// PyDict_Size
#[no_mangle]
pub unsafe extern "C" fn PyDict_Size(dict: *mut RawPyObject) -> isize {
    if dict.is_null() {
        return -1;
    }
    let data = PyObjectWithData::<DictData>::data_from_raw(dict);
    data.entries.len() as isize
}

/// PyDict_Keys - return list of keys
#[no_mangle]
pub unsafe extern "C" fn PyDict_Keys(dict: *mut RawPyObject) -> *mut RawPyObject {
    if dict.is_null() {
        return ptr::null_mut();
    }
    let data = PyObjectWithData::<DictData>::data_from_raw(dict);
    let list = crate::types::list::PyList_New(data.entries.len() as isize);
    for (i, (key, _)) in data.entries.iter().enumerate() {
        if !key.ptr.is_null() {
            (*key.ptr).incref();
        }
        crate::types::list::PyList_SET_ITEM(list, i as isize, key.ptr);
    }
    list
}

/// PyDict_Values
#[no_mangle]
pub unsafe extern "C" fn PyDict_Values(dict: *mut RawPyObject) -> *mut RawPyObject {
    if dict.is_null() {
        return ptr::null_mut();
    }
    let data = PyObjectWithData::<DictData>::data_from_raw(dict);
    let list = crate::types::list::PyList_New(data.entries.len() as isize);
    for (i, (_, &val)) in data.entries.iter().enumerate() {
        if !val.is_null() {
            (*val).incref();
        }
        crate::types::list::PyList_SET_ITEM(list, i as isize, val);
    }
    list
}

/// PyDict_Items
#[no_mangle]
pub unsafe extern "C" fn PyDict_Items(dict: *mut RawPyObject) -> *mut RawPyObject {
    if dict.is_null() {
        return ptr::null_mut();
    }
    let data = PyObjectWithData::<DictData>::data_from_raw(dict);
    let list = crate::types::list::PyList_New(data.entries.len() as isize);
    for (i, (key, &val)) in data.entries.iter().enumerate() {
        let tuple = crate::types::tuple::PyTuple_New(2);
        if !key.ptr.is_null() {
            (*key.ptr).incref();
        }
        crate::types::tuple::PyTuple_SET_ITEM(tuple, 0, key.ptr);
        if !val.is_null() {
            (*val).incref();
        }
        crate::types::tuple::PyTuple_SET_ITEM(tuple, 1, val);
        crate::types::list::PyList_SET_ITEM(list, i as isize, tuple);
    }
    list
}

/// PyDict_Next - iterate over dict entries
#[no_mangle]
pub unsafe extern "C" fn PyDict_Next(
    dict: *mut RawPyObject,
    pos: *mut isize,
    key: *mut *mut RawPyObject,
    value: *mut *mut RawPyObject,
) -> c_int {
    if dict.is_null() || pos.is_null() {
        return 0;
    }
    let data = PyObjectWithData::<DictData>::data_from_raw(dict);
    let idx = *pos as usize;
    if idx >= data.entries.len() {
        return 0;
    }
    let (k, v) = data.entries.get_index(idx).unwrap();
    if !key.is_null() {
        *key = k.ptr;
    }
    if !value.is_null() {
        *value = *v;
    }
    *pos += 1;
    1
}

/// PyDict_Clear
#[no_mangle]
pub unsafe extern "C" fn PyDict_Clear(dict: *mut RawPyObject) {
    if dict.is_null() {
        return;
    }
    let data = PyObjectWithData::<DictData>::data_from_raw_mut(dict);
    // Decref all keys and values
    for (key, &val) in data.entries.iter() {
        if !key.ptr.is_null() {
            (*key.ptr).decref();
        }
        if !val.is_null() {
            (*val).decref();
        }
    }
    data.entries.clear();
}

/// PyDict_Copy
#[no_mangle]
pub unsafe extern "C" fn PyDict_Copy(dict: *mut RawPyObject) -> *mut RawPyObject {
    if dict.is_null() {
        return ptr::null_mut();
    }
    let data = PyObjectWithData::<DictData>::data_from_raw(dict);
    let new_dict = PyDict_New();
    for (key, &val) in data.entries.iter() {
        PyDict_SetItem(new_dict, key.ptr, val);
    }
    new_dict
}

/// PyDict_Update
#[no_mangle]
pub unsafe extern "C" fn PyDict_Update(
    dict: *mut RawPyObject,
    other: *mut RawPyObject,
) -> c_int {
    if dict.is_null() || other.is_null() {
        return -1;
    }
    let other_data = PyObjectWithData::<DictData>::data_from_raw(other);
    let entries: Vec<_> = other_data.entries.iter().map(|(k, &v)| (k.ptr, v)).collect();
    for (key, val) in entries {
        PyDict_SetItem(dict, key, val);
    }
    0
}

/// PyDict_Merge
#[no_mangle]
pub unsafe extern "C" fn PyDict_Merge(
    dict: *mut RawPyObject,
    other: *mut RawPyObject,
    _override_: c_int,
) -> c_int {
    PyDict_Update(dict, other)
}

/// PyDict_Check
#[no_mangle]
pub unsafe extern "C" fn PyDict_Check(obj: *mut RawPyObject) -> c_int {
    if obj.is_null() {
        return 0;
    }
    if (*obj).ob_type == dict_type() { 1 } else { 0 }
}

#[no_mangle]
pub static mut PyDict_Type: *mut RawPyTypeObject = std::ptr::null_mut();

pub unsafe fn init_dict_type() {
    PyDict_Type = dict_type();
}
