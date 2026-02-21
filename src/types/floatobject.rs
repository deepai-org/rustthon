//! Python float type — CPython 3.11 exact ABI layout.
//!
//! CPython layout (24 bytes on 64-bit):
//!   struct {
//!       PyObject ob_base;   // 16 bytes
//!       double ob_fval;     // 8 bytes
//!   };

use crate::object::pyobject::RawPyObject;
use crate::object::typeobj::RawPyTypeObject;
use std::os::raw::c_int;
use std::sync::atomic::AtomicIsize;

/// Exact CPython PyFloatObject layout.
#[repr(C)]
pub struct PyFloatObject {
    pub ob_base: RawPyObject,
    pub ob_fval: f64,
}

// Static size assertion
const _: () = assert!(std::mem::size_of::<PyFloatObject>() == 24);

#[no_mangle]
pub static mut PyFloat_Type: RawPyTypeObject = {
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"float\0".as_ptr() as *const _;
    tp.tp_basicsize = 24; // size_of::<PyFloatObject>()
    tp
};

pub unsafe fn float_type() -> *mut RawPyTypeObject {
    &mut PyFloat_Type
}

/// Extract the f64 value from a float object.
#[inline]
pub unsafe fn float_value(obj: *mut RawPyObject) -> f64 {
    let typed = obj as *mut PyFloatObject;
    (*typed).ob_fval
}

/// Dealloc for float objects — frees via libc::free.
unsafe extern "C" fn float_dealloc(obj: *mut RawPyObject) {
    libc::free(obj as *mut libc::c_void);
}

// ─── C API ───

#[no_mangle]
pub unsafe extern "C" fn PyFloat_FromDouble(v: f64) -> *mut RawPyObject {
    let ptr = libc::calloc(1, std::mem::size_of::<PyFloatObject>()) as *mut PyFloatObject;
    if ptr.is_null() {
        eprintln!("Fatal: out of memory in PyFloat_FromDouble");
        std::process::abort();
    }
    std::ptr::write(&mut (*ptr).ob_base, RawPyObject::new(float_type()));
    (*ptr).ob_fval = v;
    ptr as *mut RawPyObject
}

#[no_mangle]
pub unsafe extern "C" fn PyFloat_AsDouble(obj: *mut RawPyObject) -> f64 {
    if obj.is_null() {
        return -1.0;
    }
    float_value(obj)
}

#[no_mangle]
pub unsafe extern "C" fn PyFloat_Check(obj: *mut RawPyObject) -> c_int {
    if obj.is_null() {
        return 0;
    }
    if (*obj).ob_type == float_type() { 1 } else { 0 }
}

pub unsafe fn init_float_type() {
    PyFloat_Type.tp_dealloc = Some(float_dealloc);
}
