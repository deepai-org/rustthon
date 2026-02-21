//! Python list type.
//!
//! Lists are mutable sequences. C extensions manipulate them
//! via PyList_New, PyList_SetItem, PyList_GetItem, etc.
//! These must be fast — packages like msgpack and hiredis
//! build lists from C at high speed.

use crate::object::pyobject::{PyObjectWithData, RawPyObject};
use crate::object::typeobj::RawPyTypeObject;
use std::os::raw::c_int;
use std::ptr;

static mut LIST_TYPE: RawPyTypeObject = {
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"list\0".as_ptr() as *const _;
    tp.tp_basicsize = 0;
    tp
};

pub unsafe fn list_type() -> *mut RawPyTypeObject {
    &mut LIST_TYPE
}

pub struct ListData {
    /// Items stored as raw pointers (we own references)
    pub items: Vec<*mut RawPyObject>,
}

type PyListObject = PyObjectWithData<ListData>;

unsafe fn create_list(size: usize) -> *mut RawPyObject {
    let mut items = Vec::with_capacity(size);
    // Fill with null (items must be set via SetItem before use)
    items.resize(size, ptr::null_mut());
    let obj = PyObjectWithData::alloc(
        &mut LIST_TYPE,
        ListData { items },
    );
    obj as *mut RawPyObject
}

// ─── C API ───

/// PyList_New - create a new list of given size (items are NULL)
#[no_mangle]
pub unsafe extern "C" fn PyList_New(size: isize) -> *mut RawPyObject {
    if size < 0 {
        return ptr::null_mut();
    }
    create_list(size as usize)
}

/// PyList_Size
#[no_mangle]
pub unsafe extern "C" fn PyList_Size(list: *mut RawPyObject) -> isize {
    if list.is_null() {
        return -1;
    }
    let data = PyObjectWithData::<ListData>::data_from_raw(list);
    data.items.len() as isize
}

/// PyList_GET_SIZE
#[no_mangle]
pub unsafe extern "C" fn PyList_GET_SIZE(list: *mut RawPyObject) -> isize {
    PyList_Size(list)
}

/// PyList_GetItem - get item at index (borrowed reference!)
/// This is a borrowed reference — caller must NOT decref.
#[no_mangle]
pub unsafe extern "C" fn PyList_GetItem(list: *mut RawPyObject, index: isize) -> *mut RawPyObject {
    if list.is_null() {
        return ptr::null_mut();
    }
    let data = PyObjectWithData::<ListData>::data_from_raw(list);
    if index < 0 || index as usize >= data.items.len() {
        // TODO: Set IndexError
        return ptr::null_mut();
    }
    data.items[index as usize]
}

/// PyList_GET_ITEM - unchecked version
#[no_mangle]
pub unsafe extern "C" fn PyList_GET_ITEM(list: *mut RawPyObject, index: isize) -> *mut RawPyObject {
    let data = PyObjectWithData::<ListData>::data_from_raw(list);
    data.items[index as usize]
}

/// PyList_SetItem - set item at index (steals reference!)
/// This "steals" the reference to the new item.
#[no_mangle]
pub unsafe extern "C" fn PyList_SetItem(
    list: *mut RawPyObject,
    index: isize,
    item: *mut RawPyObject,
) -> c_int {
    if list.is_null() {
        return -1;
    }
    let data = PyObjectWithData::<ListData>::data_from_raw_mut(list);
    if index < 0 || index as usize >= data.items.len() {
        return -1;
    }
    // Decref old item
    let old = data.items[index as usize];
    if !old.is_null() {
        (*old).decref();
    }
    // Steal reference to new item (no incref)
    data.items[index as usize] = item;
    0
}

/// PyList_SET_ITEM - unchecked version (steals reference)
#[no_mangle]
pub unsafe extern "C" fn PyList_SET_ITEM(
    list: *mut RawPyObject,
    index: isize,
    item: *mut RawPyObject,
) {
    let data = PyObjectWithData::<ListData>::data_from_raw_mut(list);
    data.items[index as usize] = item;
}

/// PyList_Append - append an item (increfs)
#[no_mangle]
pub unsafe extern "C" fn PyList_Append(
    list: *mut RawPyObject,
    item: *mut RawPyObject,
) -> c_int {
    if list.is_null() || item.is_null() {
        return -1;
    }
    let data = PyObjectWithData::<ListData>::data_from_raw_mut(list);
    (*item).incref();
    data.items.push(item);
    0
}

/// PyList_Insert
#[no_mangle]
pub unsafe extern "C" fn PyList_Insert(
    list: *mut RawPyObject,
    index: isize,
    item: *mut RawPyObject,
) -> c_int {
    if list.is_null() || item.is_null() {
        return -1;
    }
    let data = PyObjectWithData::<ListData>::data_from_raw_mut(list);
    let idx = if index < 0 {
        0
    } else if index as usize > data.items.len() {
        data.items.len()
    } else {
        index as usize
    };
    (*item).incref();
    data.items.insert(idx, item);
    0
}

/// PyList_GetSlice
#[no_mangle]
pub unsafe extern "C" fn PyList_GetSlice(
    list: *mut RawPyObject,
    low: isize,
    high: isize,
) -> *mut RawPyObject {
    if list.is_null() {
        return ptr::null_mut();
    }
    let data = PyObjectWithData::<ListData>::data_from_raw(list);
    let len = data.items.len();
    let lo = (low.max(0) as usize).min(len);
    let hi = (high.max(0) as usize).min(len);

    let new_list = create_list(0);
    let new_data = PyObjectWithData::<ListData>::data_from_raw_mut(new_list);
    for i in lo..hi {
        let item = data.items[i];
        if !item.is_null() {
            (*item).incref();
        }
        new_data.items.push(item);
    }
    new_list
}

/// PyList_Sort
#[no_mangle]
pub unsafe extern "C" fn PyList_Sort(_list: *mut RawPyObject) -> c_int {
    // TODO: implement sorting with Python comparison
    0
}

/// PyList_Reverse
#[no_mangle]
pub unsafe extern "C" fn PyList_Reverse(list: *mut RawPyObject) -> c_int {
    if list.is_null() {
        return -1;
    }
    let data = PyObjectWithData::<ListData>::data_from_raw_mut(list);
    data.items.reverse();
    0
}

/// PyList_AsTuple - convert list to tuple
#[no_mangle]
pub unsafe extern "C" fn PyList_AsTuple(list: *mut RawPyObject) -> *mut RawPyObject {
    if list.is_null() {
        return ptr::null_mut();
    }
    let data = PyObjectWithData::<ListData>::data_from_raw(list);
    let tuple = crate::types::tuple::PyTuple_New(data.items.len() as isize);
    for (i, &item) in data.items.iter().enumerate() {
        if !item.is_null() {
            (*item).incref();
        }
        crate::types::tuple::PyTuple_SetItem(tuple, i as isize, item);
    }
    tuple
}

/// PyList_Check
#[no_mangle]
pub unsafe extern "C" fn PyList_Check(obj: *mut RawPyObject) -> c_int {
    if obj.is_null() {
        return 0;
    }
    if (*obj).ob_type == list_type() { 1 } else { 0 }
}

#[no_mangle]
pub static mut PyList_Type: *mut RawPyTypeObject = std::ptr::null_mut();

pub unsafe fn init_list_type() {
    PyList_Type = list_type();
}
