//! Python float type.
//!
//! Python floats are C doubles (64-bit IEEE 754).

use crate::object::pyobject::{PyObjectWithData, RawPyObject};
use crate::object::typeobj::RawPyTypeObject;
use std::os::raw::c_int;

static mut FLOAT_TYPE: RawPyTypeObject = {
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"float\0".as_ptr() as *const _;
    tp.tp_basicsize = std::mem::size_of::<PyFloatObject>() as isize;
    tp
};

pub unsafe fn float_type() -> *mut RawPyTypeObject {
    &mut FLOAT_TYPE
}

pub struct FloatData {
    pub value: f64,
}

type PyFloatObject = PyObjectWithData<FloatData>;

pub unsafe fn float_value(obj: *mut RawPyObject) -> f64 {
    PyObjectWithData::<FloatData>::data_from_raw(obj).value
}

// ─── C API ───

#[no_mangle]
pub unsafe extern "C" fn PyFloat_FromDouble(v: f64) -> *mut RawPyObject {
    let obj = PyObjectWithData::alloc(&mut FLOAT_TYPE, FloatData { value: v });
    obj as *mut RawPyObject
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

#[no_mangle]
pub static mut PyFloat_Type: *mut RawPyTypeObject = std::ptr::null_mut();

pub unsafe fn init_float_type() {
    PyFloat_Type = float_type();
}
