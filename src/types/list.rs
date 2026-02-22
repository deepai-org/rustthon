//! Python list type — CPython 3.11 exact ABI layout.
//!
//! CPython layout (40 bytes on 64-bit):
//!   PyVarObject ob_base      (24 bytes: refcnt + type + ob_size=length)
//!   PyObject **ob_item       (8 bytes: heap-allocated pointer array)
//!   Py_ssize_t allocated     (8 bytes: capacity of ob_item)

use crate::object::pyobject::{RawPyObject, RawPyVarObject};
use crate::object::typeobj::{RawPyTypeObject, PY_TPFLAGS_DEFAULT, PY_TPFLAGS_LIST_SUBCLASS, PY_TPFLAGS_HAVE_GC};
use crate::object::SyncUnsafeCell;
use std::os::raw::c_int;
use std::ptr;

/// Exact CPython PyListObject layout.
#[repr(C)]
pub struct PyListObject {
    pub ob_base: RawPyVarObject,        // 24 bytes, ob_size = current length
    pub ob_item: *mut *mut RawPyObject, // 8 bytes, heap-allocated array
    pub allocated: isize,               // 8 bytes, capacity
}

const _: () = assert!(std::mem::size_of::<PyListObject>() == 40);

#[no_mangle]
pub static PyList_Type: SyncUnsafeCell<RawPyTypeObject> = SyncUnsafeCell::new({
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"list\0".as_ptr() as *const _;
    tp.tp_basicsize = 40; // size_of::<PyListObject>()
    tp.tp_itemsize = 0; // items in separate heap array, not inline
    tp
});

pub fn list_type() -> *mut RawPyTypeObject {
    PyList_Type.get()
}

// ─── Internal helpers ───

/// Allocate the item array via libc::calloc.
unsafe fn alloc_items(capacity: usize) -> *mut *mut RawPyObject {
    if capacity == 0 {
        return ptr::null_mut();
    }
    let ptr = libc::calloc(capacity, std::mem::size_of::<*mut RawPyObject>())
        as *mut *mut RawPyObject;
    if ptr.is_null() {
        eprintln!("Fatal: out of memory allocating list items");
        std::process::abort();
    }
    ptr
}

/// Ensure the list has capacity for at least `needed` items.
unsafe fn list_ensure_capacity(list: *mut PyListObject, needed: isize) {
    if needed <= (*list).allocated {
        return;
    }
    // CPython growth pattern
    let new_alloc = needed + (needed >> 3) + if needed < 9 { 3 } else { 6 };
    let new_size = (new_alloc as usize) * std::mem::size_of::<*mut RawPyObject>();
    let new_items = if (*list).ob_item.is_null() {
        libc::calloc(new_alloc as usize, std::mem::size_of::<*mut RawPyObject>())
            as *mut *mut RawPyObject
    } else {
        libc::realloc(
            (*list).ob_item as *mut libc::c_void,
            new_size,
        ) as *mut *mut RawPyObject
    };
    if new_items.is_null() {
        eprintln!("Fatal: out of memory growing list");
        std::process::abort();
    }
    // Zero new slots
    let old_alloc = (*list).allocated as usize;
    for i in old_alloc..(new_alloc as usize) {
        *new_items.add(i) = ptr::null_mut();
    }
    (*list).ob_item = new_items;
    (*list).allocated = new_alloc;
}

/// Dealloc for list objects.
unsafe extern "C" fn list_dealloc(obj: *mut RawPyObject) {
    let list = obj as *mut PyListObject;
    let size = (*list).ob_base.ob_size;
    // Decref all items
    if !(*list).ob_item.is_null() {
        for i in 0..size as usize {
            let item = *(*list).ob_item.add(i);
            if !item.is_null() {
                (*item).decref();
            }
        }
        // Free the item array
        libc::free((*list).ob_item as *mut libc::c_void);
    }
    // Free the object itself (GC-tracked: free from GC head)
    crate::object::gc::PyObject_GC_Del(obj as *mut libc::c_void);
}

// ─── C API ───

#[no_mangle]
pub unsafe extern "C" fn PyList_New(size: isize) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyList_New", || unsafe {
        if size < 0 {
            return ptr::null_mut();
        }
        // GC-tracked allocation
        let obj = crate::object::gc::_PyObject_GC_New(list_type()) as *mut PyListObject;
        (*obj).ob_base.ob_size = size;
        if size > 0 {
            (*obj).ob_item = alloc_items(size as usize);
            (*obj).allocated = size;
        } else {
            (*obj).ob_item = ptr::null_mut();
            (*obj).allocated = 0;
        }
        obj as *mut RawPyObject
    })
}

#[no_mangle]
pub unsafe extern "C" fn PyList_Size(list: *mut RawPyObject) -> isize {
    crate::ffi::panic_guard::guard_ssize("PyList_Size", || unsafe {
        if list.is_null() { return -1; }
        let obj = list as *mut PyListObject;
        (*obj).ob_base.ob_size
    })
}

#[no_mangle]
pub unsafe extern "C" fn PyList_GET_SIZE(list: *mut RawPyObject) -> isize {
    crate::ffi::panic_guard::guard_ssize("PyList_GET_SIZE", || unsafe {
        PyList_Size(list)
    })
}

#[no_mangle]
pub unsafe extern "C" fn PyList_GetItem(list: *mut RawPyObject, index: isize) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyList_GetItem", || unsafe {
        if list.is_null() { return ptr::null_mut(); }
        let obj = list as *mut PyListObject;
        let size = (*obj).ob_base.ob_size;
        if index < 0 || index >= size { return ptr::null_mut(); }
        *(*obj).ob_item.add(index as usize)
    })
}

#[no_mangle]
pub unsafe extern "C" fn PyList_GET_ITEM(list: *mut RawPyObject, index: isize) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyList_GET_ITEM", || unsafe {
        let obj = list as *mut PyListObject;
        *(*obj).ob_item.add(index as usize)
    })
}

#[no_mangle]
pub unsafe extern "C" fn PyList_SetItem(
    list: *mut RawPyObject,
    index: isize,
    item: *mut RawPyObject,
) -> c_int {
    crate::ffi::panic_guard::guard_int("PyList_SetItem", || unsafe {
        if list.is_null() { return -1; }
        let obj = list as *mut PyListObject;
        let size = (*obj).ob_base.ob_size;
        if index < 0 || index >= size { return -1; }
        // Decref old item
        let old = *(*obj).ob_item.add(index as usize);
        if !old.is_null() { (*old).decref(); }
        // Steal reference to new item (no incref)
        *(*obj).ob_item.add(index as usize) = item;
        0
    })
}

#[no_mangle]
pub unsafe extern "C" fn PyList_SET_ITEM(
    list: *mut RawPyObject,
    index: isize,
    item: *mut RawPyObject,
) {
    crate::ffi::panic_guard::guard_void("PyList_SET_ITEM", || unsafe {
        let obj = list as *mut PyListObject;
        *(*obj).ob_item.add(index as usize) = item;
    })
}

#[no_mangle]
pub unsafe extern "C" fn PyList_Append(
    list: *mut RawPyObject,
    item: *mut RawPyObject,
) -> c_int {
    crate::ffi::panic_guard::guard_int("PyList_Append", || unsafe {
        if list.is_null() || item.is_null() { return -1; }
        let obj = list as *mut PyListObject;
        let size = (*obj).ob_base.ob_size;
        list_ensure_capacity(obj, size + 1);
        (*item).incref();
        *(*obj).ob_item.add(size as usize) = item;
        (*obj).ob_base.ob_size = size + 1;
        0
    })
}

#[no_mangle]
pub unsafe extern "C" fn PyList_Insert(
    list: *mut RawPyObject,
    index: isize,
    item: *mut RawPyObject,
) -> c_int {
    crate::ffi::panic_guard::guard_int("PyList_Insert", || unsafe {
        if list.is_null() || item.is_null() { return -1; }
        let obj = list as *mut PyListObject;
        let size = (*obj).ob_base.ob_size;
        let idx = index.max(0).min(size) as usize;
        list_ensure_capacity(obj, size + 1);
        // Shift items right
        let n = size as usize;
        for i in (idx..n).rev() {
            *(*obj).ob_item.add(i + 1) = *(*obj).ob_item.add(i);
        }
        (*item).incref();
        *(*obj).ob_item.add(idx) = item;
        (*obj).ob_base.ob_size = size + 1;
        0
    })
}

#[no_mangle]
pub unsafe extern "C" fn PyList_GetSlice(
    list: *mut RawPyObject,
    low: isize,
    high: isize,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyList_GetSlice", || unsafe {
        if list.is_null() { return ptr::null_mut(); }
        let obj = list as *mut PyListObject;
        let len = (*obj).ob_base.ob_size;
        let lo = low.max(0).min(len) as usize;
        let hi = high.max(0).min(len) as usize;
        let slice_len = if hi > lo { hi - lo } else { 0 };
        let new_list = PyList_New(slice_len as isize);
        for i in 0..slice_len {
            let item = *(*obj).ob_item.add(lo + i);
            if !item.is_null() { (*item).incref(); }
            PyList_SET_ITEM(new_list, i as isize, item);
        }
        new_list
    })
}

#[no_mangle]
pub unsafe extern "C" fn PyList_Sort(_list: *mut RawPyObject) -> c_int {
    crate::ffi::panic_guard::guard_int("PyList_Sort", || {
        // TODO: implement sorting with Python comparison
        0
    })
}

#[no_mangle]
pub unsafe extern "C" fn PyList_Reverse(list: *mut RawPyObject) -> c_int {
    crate::ffi::panic_guard::guard_int("PyList_Reverse", || unsafe {
        if list.is_null() { return -1; }
        let obj = list as *mut PyListObject;
        let size = (*obj).ob_base.ob_size as usize;
        let items = (*obj).ob_item;
        for i in 0..size / 2 {
            let tmp = *items.add(i);
            *items.add(i) = *items.add(size - 1 - i);
            *items.add(size - 1 - i) = tmp;
        }
        0
    })
}

#[no_mangle]
pub unsafe extern "C" fn PyList_AsTuple(list: *mut RawPyObject) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyList_AsTuple", || unsafe {
        if list.is_null() { return ptr::null_mut(); }
        let obj = list as *mut PyListObject;
        let size = (*obj).ob_base.ob_size;
        let tuple = crate::types::tuple::PyTuple_New(size);
        for i in 0..size as usize {
            let item = *(*obj).ob_item.add(i);
            if !item.is_null() { (*item).incref(); }
            crate::types::tuple::PyTuple_SetItem(tuple, i as isize, item);
        }
        tuple
    })
}

#[no_mangle]
pub unsafe extern "C" fn PyList_Check(obj: *mut RawPyObject) -> c_int {
    crate::ffi::panic_guard::guard_int("PyList_Check", || unsafe {
        if obj.is_null() { return 0; }
        if (*obj).ob_type == list_type() { 1 } else { 0 }
    })
}

pub unsafe fn init_list_type() {
    (*PyList_Type.get()).tp_dealloc = Some(list_dealloc);
    (*PyList_Type.get()).tp_flags = PY_TPFLAGS_DEFAULT | PY_TPFLAGS_LIST_SUBCLASS | PY_TPFLAGS_HAVE_GC;
}
