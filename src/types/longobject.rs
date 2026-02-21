//! Python int (long) type.
//!
//! Python ints are arbitrary precision. We use num-bigint internally
//! but present a CPython-compatible C API. Small ints are cached
//! just like CPython does (-5 to 256).

use crate::object::pyobject::{PyObjectWithData, RawPyObject};
use crate::object::typeobj::RawPyTypeObject;
use std::os::raw::{c_char, c_int, c_long, c_longlong, c_ulong, c_ulonglong};
use std::ptr;
use std::sync::atomic::AtomicIsize;

use num_bigint::BigInt;
use num_traits::{ToPrimitive, Zero};

/// The int type object
static mut LONG_TYPE: RawPyTypeObject = {
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"int\0".as_ptr() as *const _;
    tp.tp_basicsize = std::mem::size_of::<PyLongObject>() as isize;
    tp
};

pub unsafe fn long_type() -> *mut RawPyTypeObject {
    &mut LONG_TYPE
}

/// Internal representation of a Python int
pub struct LongData {
    pub value: BigInt,
}

/// The C-compatible long object
type PyLongObject = PyObjectWithData<LongData>;

/// Small int cache (-5 to 256, matching CPython)
const SMALL_INT_MIN: i64 = -5;
const SMALL_INT_MAX: i64 = 256;

use crate::object::SendPtr;
use once_cell::sync::Lazy;
use parking_lot::Mutex;

/// Wrapper for Vec of raw pointers to make it Send
struct SmallIntCache(Vec<*mut RawPyObject>);
unsafe impl Send for SmallIntCache {}

static SMALL_INTS: Lazy<Mutex<SmallIntCache>> = Lazy::new(|| {
    let mut cache = Vec::new();
    let count = (SMALL_INT_MAX - SMALL_INT_MIN + 1) as usize;
    cache.reserve(count);
    for i in SMALL_INT_MIN..=SMALL_INT_MAX {
        unsafe {
            let obj = create_long_uncached(i.into());
            // Make immortal
            (*obj).ob_refcnt = AtomicIsize::new(isize::MAX / 2);
            cache.push(obj);
        }
    }
    Mutex::new(SmallIntCache(cache))
});

unsafe fn create_long_uncached(value: BigInt) -> *mut RawPyObject {
    let obj = PyObjectWithData::alloc(&mut LONG_TYPE, LongData { value });
    obj as *mut RawPyObject
}

unsafe fn create_long(value: BigInt) -> *mut RawPyObject {
    // Check small int cache
    if let Some(v) = value.to_i64() {
        if v >= SMALL_INT_MIN && v <= SMALL_INT_MAX {
            let cache = SMALL_INTS.lock();
            let idx = (v - SMALL_INT_MIN) as usize;
            let obj = cache.0[idx];
            (*obj).incref();
            return obj;
        }
    }
    create_long_uncached(value)
}

/// Get the BigInt value from a long object
pub unsafe fn long_value(obj: *mut RawPyObject) -> &'static BigInt {
    let data = PyObjectWithData::<LongData>::data_from_raw(obj);
    &data.value
}

// ─── C API ───

/// PyLong_FromLong
#[no_mangle]
pub unsafe extern "C" fn PyLong_FromLong(v: c_long) -> *mut RawPyObject {
    create_long(BigInt::from(v))
}

/// PyLong_FromUnsignedLong
#[no_mangle]
pub unsafe extern "C" fn PyLong_FromUnsignedLong(v: c_ulong) -> *mut RawPyObject {
    create_long(BigInt::from(v))
}

/// PyLong_FromLongLong
#[no_mangle]
pub unsafe extern "C" fn PyLong_FromLongLong(v: c_longlong) -> *mut RawPyObject {
    create_long(BigInt::from(v))
}

/// PyLong_FromUnsignedLongLong
#[no_mangle]
pub unsafe extern "C" fn PyLong_FromUnsignedLongLong(v: c_ulonglong) -> *mut RawPyObject {
    create_long(BigInt::from(v))
}

/// PyLong_FromDouble
#[no_mangle]
pub unsafe extern "C" fn PyLong_FromDouble(v: f64) -> *mut RawPyObject {
    create_long(BigInt::from(v as i64))
}

/// PyLong_FromSsize_t
#[no_mangle]
pub unsafe extern "C" fn PyLong_FromSsize_t(v: isize) -> *mut RawPyObject {
    create_long(BigInt::from(v))
}

/// PyLong_FromSize_t
#[no_mangle]
pub unsafe extern "C" fn PyLong_FromSize_t(v: usize) -> *mut RawPyObject {
    create_long(BigInt::from(v))
}

/// PyLong_AsLong
#[no_mangle]
pub unsafe extern "C" fn PyLong_AsLong(obj: *mut RawPyObject) -> c_long {
    if obj.is_null() {
        return -1;
    }
    let val = long_value(obj);
    val.to_i64().unwrap_or(-1) as c_long
}

/// PyLong_AsUnsignedLong
#[no_mangle]
pub unsafe extern "C" fn PyLong_AsUnsignedLong(obj: *mut RawPyObject) -> c_ulong {
    if obj.is_null() {
        return 0;
    }
    let val = long_value(obj);
    val.to_u64().unwrap_or(0) as c_ulong
}

/// PyLong_AsLongLong
#[no_mangle]
pub unsafe extern "C" fn PyLong_AsLongLong(obj: *mut RawPyObject) -> c_longlong {
    if obj.is_null() {
        return -1;
    }
    let val = long_value(obj);
    val.to_i64().unwrap_or(-1) as c_longlong
}

/// PyLong_AsUnsignedLongLong
#[no_mangle]
pub unsafe extern "C" fn PyLong_AsUnsignedLongLong(obj: *mut RawPyObject) -> c_ulonglong {
    if obj.is_null() {
        return 0;
    }
    let val = long_value(obj);
    val.to_u64().unwrap_or(0) as c_ulonglong
}

/// PyLong_AsDouble
#[no_mangle]
pub unsafe extern "C" fn PyLong_AsDouble(obj: *mut RawPyObject) -> f64 {
    if obj.is_null() {
        return -1.0;
    }
    let val = long_value(obj);
    val.to_f64().unwrap_or(f64::NAN)
}

/// PyLong_AsSsize_t
#[no_mangle]
pub unsafe extern "C" fn PyLong_AsSsize_t(obj: *mut RawPyObject) -> isize {
    if obj.is_null() {
        return -1;
    }
    let val = long_value(obj);
    val.to_isize().unwrap_or(-1)
}

/// PyLong_AsSize_t
#[no_mangle]
pub unsafe extern "C" fn PyLong_AsSize_t(obj: *mut RawPyObject) -> usize {
    if obj.is_null() {
        return 0;
    }
    let val = long_value(obj);
    val.to_usize().unwrap_or(0)
}

/// PyLong_Check (type check)
#[no_mangle]
pub unsafe extern "C" fn PyLong_Check(obj: *mut RawPyObject) -> c_int {
    if obj.is_null() {
        return 0;
    }
    if (*obj).ob_type == long_type() {
        1
    } else {
        0
    }
}

/// PyLong_Type export
#[no_mangle]
pub static mut PyLong_Type: *mut RawPyTypeObject = std::ptr::null_mut();

pub unsafe fn init_long_type() {
    PyLong_Type = long_type();
}
