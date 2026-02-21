//! Python tuple type.
//!
//! Tuples are immutable sequences. SetItem is only valid during construction.

use crate::object::pyobject::{PyObjectWithData, RawPyObject};
use crate::object::typeobj::RawPyTypeObject;
use std::os::raw::c_int;
use std::ptr;

static mut TUPLE_TYPE: RawPyTypeObject = {
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"tuple\0".as_ptr() as *const _;
    tp.tp_basicsize = 0;
    tp
};

pub unsafe fn tuple_type() -> *mut RawPyTypeObject {
    &mut TUPLE_TYPE
}

pub struct TupleData {
    pub items: Vec<*mut RawPyObject>,
}

type PyTupleObject = PyObjectWithData<TupleData>;

// ─── C API ───

/// PyTuple_New - create a new tuple of given size
#[no_mangle]
pub unsafe extern "C" fn PyTuple_New(size: isize) -> *mut RawPyObject {
    if size < 0 {
        return ptr::null_mut();
    }
    let mut items = Vec::with_capacity(size as usize);
    items.resize(size as usize, ptr::null_mut());
    let obj = PyObjectWithData::alloc(
        &mut TUPLE_TYPE,
        TupleData { items },
    );
    obj as *mut RawPyObject
}

/// PyTuple_Size
#[no_mangle]
pub unsafe extern "C" fn PyTuple_Size(tuple: *mut RawPyObject) -> isize {
    if tuple.is_null() {
        return -1;
    }
    let data = PyObjectWithData::<TupleData>::data_from_raw(tuple);
    data.items.len() as isize
}

/// PyTuple_GET_SIZE
#[no_mangle]
pub unsafe extern "C" fn PyTuple_GET_SIZE(tuple: *mut RawPyObject) -> isize {
    PyTuple_Size(tuple)
}

/// PyTuple_GetItem - borrowed reference
#[no_mangle]
pub unsafe extern "C" fn PyTuple_GetItem(
    tuple: *mut RawPyObject,
    index: isize,
) -> *mut RawPyObject {
    if tuple.is_null() {
        return ptr::null_mut();
    }
    let data = PyObjectWithData::<TupleData>::data_from_raw(tuple);
    if index < 0 || index as usize >= data.items.len() {
        return ptr::null_mut();
    }
    data.items[index as usize]
}

/// PyTuple_GET_ITEM - unchecked borrowed reference
#[no_mangle]
pub unsafe extern "C" fn PyTuple_GET_ITEM(
    tuple: *mut RawPyObject,
    index: isize,
) -> *mut RawPyObject {
    let data = PyObjectWithData::<TupleData>::data_from_raw(tuple);
    data.items[index as usize]
}

/// PyTuple_SetItem - steals reference (only valid during construction!)
#[no_mangle]
pub unsafe extern "C" fn PyTuple_SetItem(
    tuple: *mut RawPyObject,
    index: isize,
    item: *mut RawPyObject,
) -> c_int {
    if tuple.is_null() {
        return -1;
    }
    let data = PyObjectWithData::<TupleData>::data_from_raw_mut(tuple);
    if index < 0 || index as usize >= data.items.len() {
        return -1;
    }
    let old = data.items[index as usize];
    if !old.is_null() {
        (*old).decref();
    }
    data.items[index as usize] = item;
    0
}

/// PyTuple_SET_ITEM - unchecked, steals reference
#[no_mangle]
pub unsafe extern "C" fn PyTuple_SET_ITEM(
    tuple: *mut RawPyObject,
    index: isize,
    item: *mut RawPyObject,
) {
    let data = PyObjectWithData::<TupleData>::data_from_raw_mut(tuple);
    data.items[index as usize] = item;
}

/// PyTuple_GetSlice
#[no_mangle]
pub unsafe extern "C" fn PyTuple_GetSlice(
    tuple: *mut RawPyObject,
    low: isize,
    high: isize,
) -> *mut RawPyObject {
    if tuple.is_null() {
        return ptr::null_mut();
    }
    let data = PyObjectWithData::<TupleData>::data_from_raw(tuple);
    let len = data.items.len();
    let lo = (low.max(0) as usize).min(len);
    let hi = (high.max(0) as usize).min(len);
    let size = hi.saturating_sub(lo);

    let new_tuple = PyTuple_New(size as isize);
    let new_data = PyObjectWithData::<TupleData>::data_from_raw_mut(new_tuple);
    for (j, i) in (lo..hi).enumerate() {
        let item = data.items[i];
        if !item.is_null() {
            (*item).incref();
        }
        new_data.items[j] = item;
    }
    new_tuple
}

/// PyTuple_Pack - create a tuple from a variable number of args
/// For now, limited to manual construction. C extensions call this
/// with literal argument counts.
#[no_mangle]
pub unsafe extern "C" fn PyTuple_Pack(n: isize) -> *mut RawPyObject {
    // Without varargs support, this just creates an empty tuple of size n
    // Extensions using this will typically use PyTuple_SetItem afterwards
    PyTuple_New(n)
}

/// PyTuple_Check
#[no_mangle]
pub unsafe extern "C" fn PyTuple_Check(obj: *mut RawPyObject) -> c_int {
    if obj.is_null() {
        return 0;
    }
    if (*obj).ob_type == tuple_type() { 1 } else { 0 }
}

#[no_mangle]
pub static mut PyTuple_Type: *mut RawPyTypeObject = std::ptr::null_mut();

pub unsafe fn init_tuple_type() {
    PyTuple_Type = tuple_type();
}
