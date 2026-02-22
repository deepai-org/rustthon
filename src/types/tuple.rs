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
use crate::object::SyncUnsafeCell;
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
pub static PyTuple_Type: SyncUnsafeCell<RawPyTypeObject> = SyncUnsafeCell::new({
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"tuple\0".as_ptr() as *const _;
    tp.tp_basicsize = TUPLE_HEADER_SIZE as isize; // 24
    tp.tp_itemsize = ITEM_SIZE as isize; // 8
    tp
});

pub fn tuple_type() -> *mut RawPyTypeObject {
    PyTuple_Type.get()
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
pub extern "C" fn PyTuple_New(size: isize) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyTuple_New", || unsafe {
        if size < 0 { return ptr::null_mut(); }
        // GC-tracked var-size allocation: GC_HEAD + 24 + 8*size
        let obj = crate::object::gc::_PyObject_GC_NewVar(tuple_type(), size);
        obj as *mut RawPyObject
    })
}

#[no_mangle]
pub extern "C" fn PyTuple_Size(tuple: *mut RawPyObject) -> isize {
    crate::ffi::panic_guard::guard_ssize("PyTuple_Size", || unsafe {
        if tuple.is_null() { return -1; }
        let t = tuple as *mut PyTupleObject;
        (*t).ob_base.ob_size
    })
}

#[no_mangle]
pub extern "C" fn PyTuple_GET_SIZE(tuple: *mut RawPyObject) -> isize {
    crate::ffi::panic_guard::guard_ssize("PyTuple_GET_SIZE", || unsafe {
        PyTuple_Size(tuple)
    })
}

#[no_mangle]
pub extern "C" fn PyTuple_GetItem(tuple: *mut RawPyObject, index: isize) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyTuple_GetItem", || unsafe {
        if tuple.is_null() { return ptr::null_mut(); }
        let t = tuple as *mut PyTupleObject;
        let size = (*t).ob_base.ob_size;
        if index < 0 || index >= size { return ptr::null_mut(); }
        *ob_item(t).add(index as usize)
    })
}

#[no_mangle]
pub extern "C" fn PyTuple_GET_ITEM(tuple: *mut RawPyObject, index: isize) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyTuple_GET_ITEM", || unsafe {
        let t = tuple as *mut PyTupleObject;
        *ob_item(t).add(index as usize)
    })
}

#[no_mangle]
pub extern "C" fn PyTuple_SetItem(
    tuple: *mut RawPyObject, index: isize, item: *mut RawPyObject,
) -> c_int {
    crate::ffi::panic_guard::guard_int("PyTuple_SetItem", || unsafe {
        if tuple.is_null() { return -1; }
        let t = tuple as *mut PyTupleObject;
        let size = (*t).ob_base.ob_size;
        if index < 0 || index >= size { return -1; }
        let items = ob_item(t);
        let old = *items.add(index as usize);
        if !old.is_null() { (*old).decref(); }
        *items.add(index as usize) = item; // steals reference
        0
    })
}

#[no_mangle]
pub extern "C" fn PyTuple_SET_ITEM(
    tuple: *mut RawPyObject, index: isize, item: *mut RawPyObject,
) {
    crate::ffi::panic_guard::guard_void("PyTuple_SET_ITEM", || unsafe {
        let t = tuple as *mut PyTupleObject;
        *ob_item(t).add(index as usize) = item;
    })
}

#[no_mangle]
pub extern "C" fn PyTuple_GetSlice(
    tuple: *mut RawPyObject, low: isize, high: isize,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyTuple_GetSlice", || unsafe {
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
    })
}

// PyTuple_Pack is implemented in csrc/varargs.c (requires C variadic args)

#[no_mangle]
pub extern "C" fn PyTuple_Check(obj: *mut RawPyObject) -> c_int {
    crate::ffi::panic_guard::guard_int("PyTuple_Check", || unsafe {
        if obj.is_null() { return 0; }
        if (*obj).ob_type == tuple_type() { 1 } else { 0 }
    })
}

pub unsafe fn init_tuple_type() {
    (*PyTuple_Type.get()).tp_dealloc = Some(tuple_dealloc);
    (*PyTuple_Type.get()).tp_flags = PY_TPFLAGS_DEFAULT | PY_TPFLAGS_TUPLE_SUBCLASS | PY_TPFLAGS_HAVE_GC;
}
