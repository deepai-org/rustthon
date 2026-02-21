//! Python tuple type — CPython 3.11 exact ABI layout.
//!
//! CPython layout:
//!   PyVarObject ob_base   (24 bytes: refcnt + type + ob_size=length)
//!   PyObject *ob_item[]   (inline flexible array: 8 bytes per element)
//!
//! Total allocated size = 24 + 8*N bytes.
//! Items are stored inline — no separate heap allocation.

use crate::object::pyobject::{RawPyObject, RawPyVarObject};
use crate::object::typeobj::{RawPyTypeObject, PY_TPFLAGS_DEFAULT, PY_TPFLAGS_TUPLE_SUBCLASS, PY_TPFLAGS_HAVE_GC};
use std::os::raw::c_int;
use std::ptr;

/// Fixed portion of PyTupleObject (24 bytes).
/// ob_item[] follows inline.
#[repr(C)]
pub struct PyTupleObject {
    pub ob_base: RawPyVarObject,
    // Followed by ob_item[N] of *mut RawPyObject
}

const TUPLE_HEADER_SIZE: usize = std::mem::size_of::<RawPyVarObject>(); // 24
const ITEM_SIZE: usize = std::mem::size_of::<*mut RawPyObject>(); // 8

/// Get pointer to the item array (inline after header).
#[inline]
unsafe fn ob_item(obj: *mut PyTupleObject) -> *mut *mut RawPyObject {
    (obj as *mut u8).add(TUPLE_HEADER_SIZE) as *mut *mut RawPyObject
}

#[no_mangle]
pub static mut PyTuple_Type: RawPyTypeObject = {
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"tuple\0".as_ptr() as *const _;
    tp.tp_basicsize = TUPLE_HEADER_SIZE as isize; // 24
    tp.tp_itemsize = ITEM_SIZE as isize; // 8
    tp
};

pub unsafe fn tuple_type() -> *mut RawPyTypeObject {
    &mut PyTuple_Type
}

unsafe extern "C" fn tuple_dealloc(obj: *mut RawPyObject) {
    let t = obj as *mut PyTupleObject;
    let size = (*t).ob_base.ob_size;
    let items = ob_item(t);
    for i in 0..size as usize {
        let item = *items.add(i);
        if !item.is_null() { (*item).decref(); }
    }
    // GC-tracked: free via GC del (frees from GC head)
    crate::object::gc::PyObject_GC_Del(obj as *mut libc::c_void);
}

// ─── C API ───

#[no_mangle]
pub unsafe extern "C" fn PyTuple_New(size: isize) -> *mut RawPyObject {
    if size < 0 { return ptr::null_mut(); }
    // GC-tracked var-size allocation: GC_HEAD + 24 + 8*size
    let obj = crate::object::gc::_PyObject_GC_NewVar(&mut PyTuple_Type, size);
    obj as *mut RawPyObject
}

#[no_mangle]
pub unsafe extern "C" fn PyTuple_Size(tuple: *mut RawPyObject) -> isize {
    if tuple.is_null() { return -1; }
    let t = tuple as *mut PyTupleObject;
    (*t).ob_base.ob_size
}

#[no_mangle]
pub unsafe extern "C" fn PyTuple_GET_SIZE(tuple: *mut RawPyObject) -> isize {
    PyTuple_Size(tuple)
}

#[no_mangle]
pub unsafe extern "C" fn PyTuple_GetItem(tuple: *mut RawPyObject, index: isize) -> *mut RawPyObject {
    if tuple.is_null() { return ptr::null_mut(); }
    let t = tuple as *mut PyTupleObject;
    let size = (*t).ob_base.ob_size;
    if index < 0 || index >= size { return ptr::null_mut(); }
    *ob_item(t).add(index as usize)
}

#[no_mangle]
pub unsafe extern "C" fn PyTuple_GET_ITEM(tuple: *mut RawPyObject, index: isize) -> *mut RawPyObject {
    let t = tuple as *mut PyTupleObject;
    *ob_item(t).add(index as usize)
}

#[no_mangle]
pub unsafe extern "C" fn PyTuple_SetItem(
    tuple: *mut RawPyObject, index: isize, item: *mut RawPyObject,
) -> c_int {
    if tuple.is_null() { return -1; }
    let t = tuple as *mut PyTupleObject;
    let size = (*t).ob_base.ob_size;
    if index < 0 || index >= size { return -1; }
    let items = ob_item(t);
    let old = *items.add(index as usize);
    if !old.is_null() { (*old).decref(); }
    *items.add(index as usize) = item; // steals reference
    0
}

#[no_mangle]
pub unsafe extern "C" fn PyTuple_SET_ITEM(
    tuple: *mut RawPyObject, index: isize, item: *mut RawPyObject,
) {
    let t = tuple as *mut PyTupleObject;
    *ob_item(t).add(index as usize) = item;
}

#[no_mangle]
pub unsafe extern "C" fn PyTuple_GetSlice(
    tuple: *mut RawPyObject, low: isize, high: isize,
) -> *mut RawPyObject {
    if tuple.is_null() { return ptr::null_mut(); }
    let t = tuple as *mut PyTupleObject;
    let len = (*t).ob_base.ob_size;
    let lo = low.max(0).min(len) as usize;
    let hi = high.max(0).min(len) as usize;
    let slice_len = if hi > lo { hi - lo } else { 0 };
    let new_tuple = PyTuple_New(slice_len as isize);
    let src_items = ob_item(t);
    let dst = new_tuple as *mut PyTupleObject;
    let dst_items = ob_item(dst);
    for i in 0..slice_len {
        let item = *src_items.add(lo + i);
        if !item.is_null() { (*item).incref(); }
        *dst_items.add(i) = item;
    }
    new_tuple
}

#[no_mangle]
pub unsafe extern "C" fn PyTuple_Pack(n: isize) -> *mut RawPyObject {
    PyTuple_New(n)
}

#[no_mangle]
pub unsafe extern "C" fn PyTuple_Check(obj: *mut RawPyObject) -> c_int {
    if obj.is_null() { return 0; }
    if (*obj).ob_type == tuple_type() { 1 } else { 0 }
}

pub unsafe fn init_tuple_type() {
    PyTuple_Type.tp_dealloc = Some(tuple_dealloc);
    PyTuple_Type.tp_flags = PY_TPFLAGS_DEFAULT | PY_TPFLAGS_TUPLE_SUBCLASS | PY_TPFLAGS_HAVE_GC;
}
