//! Python bool type — CPython 3.11 exact ABI layout.
//!
//! In CPython, bool is a subtype of int. True and False are
//! PyLongObject singletons with ob_type = &PyBool_Type.
//! True: ob_size=1, ob_digit[0]=1
//! False: ob_size=0

use crate::object::pyobject::RawPyObject;
use crate::object::typeobj::RawPyTypeObject;
use crate::object::SendPtr;
use std::sync::atomic::AtomicIsize;

static mut BOOL_TYPE: RawPyTypeObject = {
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"bool\0".as_ptr() as *const _;
    // Same as PyLongObject: header 24 bytes, itemsize 4 (digit)
    tp.tp_basicsize = 24;
    tp.tp_itemsize = 4;
    tp
};

pub unsafe fn bool_type() -> *mut RawPyTypeObject {
    &mut BOOL_TYPE
}

// ─── Singleton pointers (heap-allocated PyLongObjects) ───

/// Wrapper to make *mut RawPyObject Send
struct BoolPtr(*mut RawPyObject);
unsafe impl Send for BoolPtr {}
unsafe impl Sync for BoolPtr {}

use once_cell::sync::Lazy;

static TRUE_PTR: Lazy<BoolPtr> = Lazy::new(|| unsafe {
    let obj = crate::types::longobject::create_long_from_i64_with_type(1, &mut BOOL_TYPE);
    (*obj).ob_refcnt = AtomicIsize::new(isize::MAX / 2); // immortal
    BoolPtr(obj)
});

static FALSE_PTR: Lazy<BoolPtr> = Lazy::new(|| unsafe {
    let obj = crate::types::longobject::create_long_from_i64_with_type(0, &mut BOOL_TYPE);
    (*obj).ob_refcnt = AtomicIsize::new(isize::MAX / 2); // immortal
    BoolPtr(obj)
});

pub static PY_TRUE: Lazy<SendPtr<RawPyObject>> = Lazy::new(|| {
    SendPtr(TRUE_PTR.0)
});

pub static PY_FALSE: Lazy<SendPtr<RawPyObject>> = Lazy::new(|| {
    SendPtr(FALSE_PTR.0)
});

// ─── Exported singleton symbols ───
// C extensions use `&_Py_TrueStruct` and `&_Py_FalseStruct` as pointers.
// Since our True/False are heap-allocated (flexible array), we export
// pointer-to-pointer symbols. The `_Py_True()` and `_Py_False()` functions
// are the primary interface.

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
    !obj.is_null() && (*obj).ob_type == bool_type()
}

#[no_mangle]
pub unsafe extern "C" fn Py_IsTrue(obj: *mut RawPyObject) -> i32 {
    if obj == PY_TRUE.get() { 1 } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn Py_IsFalse(obj: *mut RawPyObject) -> i32 {
    if obj == PY_FALSE.get() { 1 } else { 0 }
}

pub unsafe fn init_bool_type() {
    BOOL_TYPE.tp_base = crate::types::longobject::long_type();
    BOOL_TYPE.tp_flags = crate::object::typeobj::PY_TPFLAGS_DEFAULT
        | crate::object::typeobj::PY_TPFLAGS_LONG_SUBCLASS;
}
