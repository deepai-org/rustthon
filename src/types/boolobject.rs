//! Python bool type (True/False singletons).

use crate::object::pyobject::RawPyObject;
use crate::object::typeobj::RawPyTypeObject;
use crate::object::SendPtr;
use std::sync::atomic::AtomicIsize;

static mut BOOL_TYPE: RawPyTypeObject = {
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"bool\0".as_ptr() as *const _;
    tp.tp_basicsize = std::mem::size_of::<PyBoolObject>() as isize;
    tp
};

#[repr(C)]
pub struct PyBoolObject {
    pub ob_base: RawPyObject,
    pub value: i64,
}

#[no_mangle]
pub static mut _Py_TrueStruct: PyBoolObject = PyBoolObject {
    ob_base: RawPyObject {
        ob_refcnt: AtomicIsize::new(1),
        ob_type: std::ptr::null_mut(),
    },
    value: 1,
};

#[no_mangle]
pub static mut _Py_FalseStruct: PyBoolObject = PyBoolObject {
    ob_base: RawPyObject {
        ob_refcnt: AtomicIsize::new(1),
        ob_type: std::ptr::null_mut(),
    },
    value: 0,
};

pub static PY_TRUE: once_cell::sync::Lazy<SendPtr<RawPyObject>> =
    once_cell::sync::Lazy::new(|| unsafe {
        _Py_TrueStruct.ob_base.ob_type = &mut BOOL_TYPE;
        _Py_TrueStruct.ob_base.ob_refcnt = AtomicIsize::new(isize::MAX / 2);
        SendPtr(&mut _Py_TrueStruct as *mut PyBoolObject as *mut RawPyObject)
    });

pub static PY_FALSE: once_cell::sync::Lazy<SendPtr<RawPyObject>> =
    once_cell::sync::Lazy::new(|| unsafe {
        _Py_FalseStruct.ob_base.ob_type = &mut BOOL_TYPE;
        _Py_FalseStruct.ob_base.ob_refcnt = AtomicIsize::new(isize::MAX / 2);
        SendPtr(&mut _Py_FalseStruct as *mut PyBoolObject as *mut RawPyObject)
    });

// ─── C API ───

#[no_mangle]
pub unsafe extern "C" fn PyBool_FromLong(v: std::os::raw::c_long) -> *mut RawPyObject {
    if v != 0 {
        let t = PY_TRUE.get();
        (*t).incref();
        t
    } else {
        let f = PY_FALSE.get();
        (*f).incref();
        f
    }
}

#[no_mangle]
pub unsafe extern "C" fn _Py_True() -> *mut RawPyObject {
    PY_TRUE.get()
}

#[no_mangle]
pub unsafe extern "C" fn _Py_False() -> *mut RawPyObject {
    PY_FALSE.get()
}

pub unsafe fn is_true(obj: *mut RawPyObject) -> bool {
    obj == PY_TRUE.get()
}

pub unsafe fn is_bool(obj: *mut RawPyObject) -> bool {
    obj == PY_TRUE.get() || obj == PY_FALSE.get()
}

#[no_mangle]
pub unsafe extern "C" fn Py_IsTrue(obj: *mut RawPyObject) -> i32 {
    if obj == PY_TRUE.get() { 1 } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn Py_IsFalse(obj: *mut RawPyObject) -> i32 {
    if obj == PY_FALSE.get() { 1 } else { 0 }
}
