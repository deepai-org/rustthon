//! Python bool type — CPython 3.11 exact ABI layout.
//!
//! In CPython, bool is a subtype of int. True and False are
//! PyLongObject singletons with ob_type = &PyBool_Type.
//! True: ob_size=1, ob_digit[0]=1
//! False: ob_size=0

use crate::object::pyobject::{RawPyObject, RawPyVarObject};
use crate::object::typeobj::RawPyTypeObject;
use crate::object::SendPtr;
use crate::object::SyncUnsafeCell;
use crate::types::longobject::Digit;
use std::sync::atomic::AtomicIsize;

// ─── Type object (actual struct, not pointer) ───

#[no_mangle]
pub static PyBool_Type: SyncUnsafeCell<RawPyTypeObject> = SyncUnsafeCell::new({
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"bool\0".as_ptr() as *const _;
    // Same as PyLongObject: header 24 bytes, itemsize 4 (digit)
    tp.tp_basicsize = 24;
    tp.tp_itemsize = 4;
    tp
});

pub fn bool_type() -> *mut RawPyTypeObject {
    PyBool_Type.get()
}

// ─── Static True/False structs ───
// Prebuilt CPython extensions expect _Py_TrueStruct and _Py_FalseStruct
// to be actual static data symbols (struct _longobject), not heap-allocated.

/// A PyLongObject with exactly 1 digit inline.
/// Layout: RawPyVarObject (24 bytes) + 1 Digit (4 bytes) = 28 bytes.
#[repr(C)]
pub struct PyLongObject1Digit {
    pub ob_base: RawPyVarObject,
    pub ob_digit: [Digit; 1],
}

unsafe impl Send for PyLongObject1Digit {}
unsafe impl Sync for PyLongObject1Digit {}

// True: ob_size=1, ob_digit[0]=1
#[no_mangle]
pub static _Py_TrueStruct: SyncUnsafeCell<PyLongObject1Digit> = SyncUnsafeCell::new(PyLongObject1Digit {
    ob_base: RawPyVarObject {
        ob_base: RawPyObject {
            ob_refcnt: AtomicIsize::new(isize::MAX / 2), // immortal
            ob_type: std::ptr::null_mut(), // set in init_bool_type
        },
        ob_size: 1,
    },
    ob_digit: [1],
});

// False: ob_size=0, ob_digit[0]=0
#[no_mangle]
pub static _Py_FalseStruct: SyncUnsafeCell<PyLongObject1Digit> = SyncUnsafeCell::new(PyLongObject1Digit {
    ob_base: RawPyVarObject {
        ob_base: RawPyObject {
            ob_refcnt: AtomicIsize::new(isize::MAX / 2), // immortal
            ob_type: std::ptr::null_mut(), // set in init_bool_type
        },
        ob_size: 0,
    },
    ob_digit: [0],
});

// ─── Singleton accessors ───

use once_cell::sync::Lazy;

pub static PY_TRUE: Lazy<SendPtr<RawPyObject>> = Lazy::new(|| {
    SendPtr(_Py_TrueStruct.get() as *mut RawPyObject)
});

pub static PY_FALSE: Lazy<SendPtr<RawPyObject>> = Lazy::new(|| {
    SendPtr(_Py_FalseStruct.get() as *mut RawPyObject)
});

// ─── C API ───

#[no_mangle]
pub extern "C" fn PyBool_FromLong(v: std::os::raw::c_long) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyBool_FromLong", || unsafe {
        if v != 0 {
            let t = PY_TRUE.get();
            (*t).incref();
            t
        } else {
            let f = PY_FALSE.get();
            (*f).incref();
            f
        }
    })
}

#[no_mangle]
pub extern "C" fn _Py_True() -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("_Py_True", || unsafe {
        _Py_TrueStruct.get() as *mut RawPyObject
    })
}

#[no_mangle]
pub extern "C" fn _Py_False() -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("_Py_False", || unsafe {
        _Py_FalseStruct.get() as *mut RawPyObject
    })
}

pub fn is_true(obj: *mut RawPyObject) -> bool {
    obj == PY_TRUE.get()
}

pub fn is_bool(obj: *mut RawPyObject) -> bool {
    !obj.is_null() && unsafe { (*obj).ob_type == bool_type() }
}

#[no_mangle]
pub extern "C" fn Py_IsTrue(obj: *mut RawPyObject) -> i32 {
    crate::ffi::panic_guard::guard_i32("Py_IsTrue", || unsafe {
        if obj == PY_TRUE.get() { 1 } else { 0 }
    })
}

#[no_mangle]
pub extern "C" fn Py_IsFalse(obj: *mut RawPyObject) -> i32 {
    crate::ffi::panic_guard::guard_i32("Py_IsFalse", || unsafe {
        if obj == PY_FALSE.get() { 1 } else { 0 }
    })
}

/// PyBool_Check — returns 1 if the object is a bool (True or False).
#[no_mangle]
pub extern "C" fn PyBool_Check(obj: *mut RawPyObject) -> std::os::raw::c_int {
    crate::ffi::panic_guard::guard_int("PyBool_Check", || unsafe {
        if is_bool(obj) { 1 } else { 0 }
    })
}

pub unsafe fn init_bool_type() {
    (*PyBool_Type.get()).tp_base = crate::types::longobject::long_type();
    (*PyBool_Type.get()).tp_flags = crate::object::typeobj::PY_TPFLAGS_DEFAULT
        | crate::object::typeobj::PY_TPFLAGS_LONG_SUBCLASS;
    // Wire ob_type on static singletons
    (*_Py_TrueStruct.get()).ob_base.ob_base.ob_type = PyBool_Type.get();
    (*_Py_FalseStruct.get()).ob_base.ob_base.ob_type = PyBool_Type.get();
}
