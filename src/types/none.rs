//! Python None singleton.

use crate::object::pyobject::RawPyObject;
use crate::object::typeobj::RawPyTypeObject;
use crate::object::SendPtr;
use crate::object::SyncUnsafeCell;
use std::sync::atomic::AtomicIsize;

static NONE_TYPE: SyncUnsafeCell<RawPyTypeObject> = SyncUnsafeCell::new({
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"NoneType\0".as_ptr() as *const _;
    tp.tp_basicsize = std::mem::size_of::<RawPyObject>() as isize;
    tp
});

pub fn none_type() -> *mut RawPyTypeObject {
    NONE_TYPE.get()
}

#[no_mangle]
pub static _Py_NoneStruct: SyncUnsafeCell<RawPyObject> = SyncUnsafeCell::new(RawPyObject {
    ob_refcnt: AtomicIsize::new(1),
    ob_type: std::ptr::null_mut(),
});

pub static PY_NONE: once_cell::sync::Lazy<SendPtr<RawPyObject>> =
    once_cell::sync::Lazy::new(|| unsafe {
        (*_Py_NoneStruct.get()).ob_type = NONE_TYPE.get();
        (*_Py_NoneStruct.get()).ob_refcnt = AtomicIsize::new(isize::MAX / 2);
        SendPtr(_Py_NoneStruct.get())
    });

#[no_mangle]
pub unsafe extern "C" fn _Py_None() -> *mut RawPyObject {
    PY_NONE.get()
}

pub fn return_none() -> *mut RawPyObject {
    let none = PY_NONE.get();
    unsafe { (*none).incref(); }
    none
}

pub fn is_none(obj: *mut RawPyObject) -> bool {
    obj == PY_NONE.get()
}
