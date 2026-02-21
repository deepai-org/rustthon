//! Python None singleton.

use crate::object::pyobject::RawPyObject;
use crate::object::typeobj::RawPyTypeObject;
use crate::object::SendPtr;
use std::sync::atomic::AtomicIsize;

static mut NONE_TYPE: RawPyTypeObject = {
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"NoneType\0".as_ptr() as *const _;
    tp.tp_basicsize = std::mem::size_of::<RawPyObject>() as isize;
    tp
};

#[no_mangle]
pub static mut _Py_NoneStruct: RawPyObject = RawPyObject {
    ob_refcnt: AtomicIsize::new(1),
    ob_type: std::ptr::null_mut(),
};

pub static PY_NONE: once_cell::sync::Lazy<SendPtr<RawPyObject>> =
    once_cell::sync::Lazy::new(|| unsafe {
        _Py_NoneStruct.ob_type = &mut NONE_TYPE;
        _Py_NoneStruct.ob_refcnt = AtomicIsize::new(isize::MAX / 2);
        SendPtr(&mut _Py_NoneStruct as *mut RawPyObject)
    });

#[no_mangle]
pub unsafe extern "C" fn _Py_None() -> *mut RawPyObject {
    PY_NONE.get()
}

pub unsafe fn return_none() -> *mut RawPyObject {
    let none = PY_NONE.get();
    (*none).incref();
    none
}

pub unsafe fn is_none(obj: *mut RawPyObject) -> bool {
    obj == PY_NONE.get()
}
