//! Python set type.

use crate::object::pyobject::{PyObjectWithData, RawPyObject};
use crate::object::typeobj::RawPyTypeObject;
use std::collections::HashSet;
use std::os::raw::c_int;
use std::ptr;

static mut SET_TYPE: RawPyTypeObject = {
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"set\0".as_ptr() as *const _;
    tp.tp_basicsize = 0;
    tp
};

pub unsafe fn set_type() -> *mut RawPyTypeObject {
    &mut SET_TYPE
}

pub struct SetData {
    /// Items stored by pointer identity (TODO: use __hash__)
    pub items: HashSet<usize>,
    /// Keep raw pointers for reference management
    pub ptrs: Vec<*mut RawPyObject>,
}

type PySetObject = PyObjectWithData<SetData>;

// ─── C API ───

/// PySet_New
#[no_mangle]
pub unsafe extern "C" fn PySet_New(iterable: *mut RawPyObject) -> *mut RawPyObject {
    let obj = PyObjectWithData::alloc(
        &mut SET_TYPE,
        SetData {
            items: HashSet::new(),
            ptrs: Vec::new(),
        },
    );
    // TODO: If iterable is provided, iterate and add items
    obj as *mut RawPyObject
}

/// PyFrozenSet_New
#[no_mangle]
pub unsafe extern "C" fn PyFrozenSet_New(iterable: *mut RawPyObject) -> *mut RawPyObject {
    // For now, same as set (TODO: separate frozenset type)
    PySet_New(iterable)
}

/// PySet_Add
#[no_mangle]
pub unsafe extern "C" fn PySet_Add(
    set: *mut RawPyObject,
    key: *mut RawPyObject,
) -> c_int {
    if set.is_null() || key.is_null() {
        return -1;
    }
    let data = PyObjectWithData::<SetData>::data_from_raw_mut(set);
    let ptr_val = key as usize;
    if data.items.insert(ptr_val) {
        (*key).incref();
        data.ptrs.push(key);
    }
    0
}

/// PySet_Discard
#[no_mangle]
pub unsafe extern "C" fn PySet_Discard(
    set: *mut RawPyObject,
    key: *mut RawPyObject,
) -> c_int {
    if set.is_null() || key.is_null() {
        return -1;
    }
    let data = PyObjectWithData::<SetData>::data_from_raw_mut(set);
    let ptr_val = key as usize;
    if data.items.remove(&ptr_val) {
        (*key).decref();
        data.ptrs.retain(|&p| p != key);
        1
    } else {
        0
    }
}

/// PySet_Contains
#[no_mangle]
pub unsafe extern "C" fn PySet_Contains(
    set: *mut RawPyObject,
    key: *mut RawPyObject,
) -> c_int {
    if set.is_null() || key.is_null() {
        return -1;
    }
    let data = PyObjectWithData::<SetData>::data_from_raw(set);
    if data.items.contains(&(key as usize)) { 1 } else { 0 }
}

/// PySet_Size
#[no_mangle]
pub unsafe extern "C" fn PySet_Size(set: *mut RawPyObject) -> isize {
    if set.is_null() {
        return -1;
    }
    let data = PyObjectWithData::<SetData>::data_from_raw(set);
    data.items.len() as isize
}

/// PySet_Clear
#[no_mangle]
pub unsafe extern "C" fn PySet_Clear(set: *mut RawPyObject) -> c_int {
    if set.is_null() {
        return -1;
    }
    let data = PyObjectWithData::<SetData>::data_from_raw_mut(set);
    for &ptr in &data.ptrs {
        if !ptr.is_null() {
            (*ptr).decref();
        }
    }
    data.items.clear();
    data.ptrs.clear();
    0
}

/// PySet_Check
#[no_mangle]
pub unsafe extern "C" fn PySet_Check(obj: *mut RawPyObject) -> c_int {
    if obj.is_null() {
        return 0;
    }
    if (*obj).ob_type == set_type() { 1 } else { 0 }
}
