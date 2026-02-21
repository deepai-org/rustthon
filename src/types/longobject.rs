//! Python int (long) type — CPython 3.11 exact ABI layout.
//!
//! CPython stores integers as arrays of 30-bit "digits" (uint32_t).
//! The sign is encoded in ob_size: negative = negative number,
//! 0 = zero, positive = positive number. |ob_size| = digit count.
//!
//! Layout:
//!   PyVarObject ob_base   (24 bytes: refcnt + type + ob_size)
//!   digit ob_digit[...]   (flexible array of uint32_t)

use crate::object::pyobject::{RawPyObject, RawPyVarObject};
use crate::object::typeobj::RawPyTypeObject;
use std::os::raw::{c_int, c_long, c_longlong, c_ulong, c_ulonglong};
use std::sync::atomic::AtomicIsize;

use num_bigint::BigInt;
use num_traits::ToPrimitive;

// ─── Digit constants matching CPython ───

/// Each digit is a u32 holding 30 bits of value.
pub type Digit = u32;
pub const PYLONG_SHIFT: u32 = 30;
pub const PYLONG_BASE: u64 = 1u64 << PYLONG_SHIFT; // 1073741824
pub const PYLONG_MASK: Digit = (1u32 << PYLONG_SHIFT) - 1; // 0x3FFFFFFF

/// Fixed portion of PyLongObject (24 bytes).
/// The ob_digit[] array follows immediately after in memory.
#[repr(C)]
pub struct PyLongObject {
    pub ob_base: RawPyVarObject,
    // Followed by ob_digit[ndigits] of Digit (u32)
}

/// Offset to the first digit (right after the header).
const LONG_HEADER_SIZE: usize = std::mem::size_of::<RawPyVarObject>(); // 24

/// Get pointer to the digit array of a PyLongObject.
#[inline]
unsafe fn ob_digit(obj: *mut PyLongObject) -> *mut Digit {
    (obj as *mut u8).add(LONG_HEADER_SIZE) as *mut Digit
}

// ─── Type object ───

static mut LONG_TYPE: RawPyTypeObject = {
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"int\0".as_ptr() as *const _;
    tp.tp_basicsize = LONG_HEADER_SIZE as isize; // 24
    tp.tp_itemsize = std::mem::size_of::<Digit>() as isize; // 4
    tp
};

pub unsafe fn long_type() -> *mut RawPyTypeObject {
    &mut LONG_TYPE
}

unsafe extern "C" fn long_dealloc(obj: *mut RawPyObject) {
    libc::free(obj as *mut libc::c_void);
}

// ─── Allocation ───

/// Allocate a PyLongObject with space for `ndigits` digits.
/// The object is zero-filled. ob_size is NOT set — caller must set it.
unsafe fn alloc_long(ndigits: usize) -> *mut PyLongObject {
    let ndigits = ndigits.max(1); // Always at least 1 digit slot
    let total = LONG_HEADER_SIZE + ndigits * std::mem::size_of::<Digit>();
    let ptr = libc::calloc(1, total) as *mut PyLongObject;
    if ptr.is_null() {
        eprintln!("Fatal: out of memory allocating PyLongObject");
        std::process::abort();
    }
    std::ptr::write(
        &mut (*ptr).ob_base.ob_base.ob_refcnt,
        AtomicIsize::new(1),
    );
    (*ptr).ob_base.ob_base.ob_type = long_type();
    ptr
}

/// Allocate a PyLongObject with a custom type pointer (for bool subtype).
pub(crate) unsafe fn alloc_long_with_type(
    ndigits: usize,
    tp: *mut RawPyTypeObject,
) -> *mut PyLongObject {
    let ndigits = ndigits.max(1);
    let total = LONG_HEADER_SIZE + ndigits * std::mem::size_of::<Digit>();
    let ptr = libc::calloc(1, total) as *mut PyLongObject;
    if ptr.is_null() {
        eprintln!("Fatal: out of memory allocating PyLongObject (with type)");
        std::process::abort();
    }
    std::ptr::write(
        &mut (*ptr).ob_base.ob_base.ob_refcnt,
        AtomicIsize::new(1),
    );
    (*ptr).ob_base.ob_base.ob_type = tp;
    ptr
}

// ─── Conversion: i64 → digits ───

/// Create a PyLongObject from an i64 value.
unsafe fn create_long_from_i64(value: i64) -> *mut RawPyObject {
    if value == 0 {
        let obj = alloc_long(1);
        (*obj).ob_base.ob_size = 0; // zero = 0 digits
        return obj as *mut RawPyObject;
    }

    let negative = value < 0;
    let mut abs_val = if value == i64::MIN {
        // Handle i64::MIN carefully (can't negate it as i64)
        (i64::MAX as u64) + 1
    } else if negative {
        (-value) as u64
    } else {
        value as u64
    };

    // Count digits needed
    let mut ndigits = 0usize;
    let mut tmp = abs_val;
    while tmp > 0 {
        ndigits += 1;
        tmp >>= PYLONG_SHIFT;
    }

    let obj = alloc_long(ndigits);
    (*obj).ob_base.ob_size = if negative {
        -(ndigits as isize)
    } else {
        ndigits as isize
    };

    let digits = ob_digit(obj);
    for i in 0..ndigits {
        *digits.add(i) = (abs_val & PYLONG_MASK as u64) as Digit;
        abs_val >>= PYLONG_SHIFT;
    }

    obj as *mut RawPyObject
}

/// Create a PyLongObject from i64 with a specific type (for bool singletons).
pub(crate) unsafe fn create_long_from_i64_with_type(
    value: i64,
    tp: *mut RawPyTypeObject,
) -> *mut RawPyObject {
    if value == 0 {
        let obj = alloc_long_with_type(1, tp);
        (*obj).ob_base.ob_size = 0;
        return obj as *mut RawPyObject;
    }

    let negative = value < 0;
    let mut abs_val = if negative { (-value) as u64 } else { value as u64 };

    let mut ndigits = 0usize;
    let mut tmp = abs_val;
    while tmp > 0 {
        ndigits += 1;
        tmp >>= PYLONG_SHIFT;
    }

    let obj = alloc_long_with_type(ndigits, tp);
    (*obj).ob_base.ob_size = if negative {
        -(ndigits as isize)
    } else {
        ndigits as isize
    };

    let digits = ob_digit(obj);
    for i in 0..ndigits {
        *digits.add(i) = (abs_val & PYLONG_MASK as u64) as Digit;
        abs_val >>= PYLONG_SHIFT;
    }

    obj as *mut RawPyObject
}

// ─── Conversion: digits → i64 ───

/// Read the integer value as i64. Returns None if it overflows.
unsafe fn pylong_to_i64(obj: *mut PyLongObject) -> Option<i64> {
    let size = (*obj).ob_base.ob_size;
    let ndigits = size.unsigned_abs();
    let negative = size < 0;

    if ndigits == 0 {
        return Some(0);
    }

    let digits = ob_digit(obj);
    let mut result: u64 = 0;

    for i in (0..ndigits).rev() {
        // Check for overflow before shifting
        if result > (u64::MAX >> PYLONG_SHIFT) {
            return None;
        }
        result = (result << PYLONG_SHIFT) | (*digits.add(i) as u64);
    }

    if negative {
        if result > (i64::MAX as u64) + 1 {
            return None;
        }
        if result == (i64::MAX as u64) + 1 {
            Some(i64::MIN)
        } else {
            Some(-(result as i64))
        }
    } else {
        if result > i64::MAX as u64 {
            return None;
        }
        Some(result as i64)
    }
}

/// Read as f64.
unsafe fn pylong_to_f64(obj: *mut PyLongObject) -> f64 {
    let size = (*obj).ob_base.ob_size;
    let ndigits = size.unsigned_abs();
    let negative = size < 0;

    if ndigits == 0 {
        return 0.0;
    }

    let digits = ob_digit(obj);
    let mut result: f64 = 0.0;

    for i in (0..ndigits).rev() {
        result = result * (PYLONG_BASE as f64) + (*digits.add(i) as f64);
    }

    if negative { -result } else { result }
}

/// Convert digits to BigInt (for display and arbitrary precision).
unsafe fn pylong_to_bigint(obj: *mut PyLongObject) -> BigInt {
    let size = (*obj).ob_base.ob_size;
    let ndigits = size.unsigned_abs();
    let negative = size < 0;

    if ndigits == 0 {
        return BigInt::from(0);
    }

    let digits = ob_digit(obj);

    // Try fast path for values that fit in i64
    if let Some(val) = pylong_to_i64(obj) {
        return BigInt::from(val);
    }

    // Slow path: reconstruct from digits
    let mut result = BigInt::from(0);
    for i in (0..ndigits).rev() {
        result = (result << PYLONG_SHIFT) + BigInt::from(*digits.add(i));
    }
    if negative {
        result = -result;
    }
    result
}

// ─── Public accessor functions ───

/// Get the i64 value. Returns 0 if overflow (use long_value for big ints).
pub unsafe fn long_as_i64(obj: *mut RawPyObject) -> i64 {
    pylong_to_i64(obj as *mut PyLongObject).unwrap_or(0)
}

/// Get the f64 value.
pub unsafe fn long_as_f64(obj: *mut RawPyObject) -> f64 {
    pylong_to_f64(obj as *mut PyLongObject)
}

/// Get the BigInt value (allocates). For display and arbitrary precision.
pub unsafe fn long_value(obj: *mut RawPyObject) -> BigInt {
    pylong_to_bigint(obj as *mut PyLongObject)
}

// ─── Small int cache ───

const SMALL_INT_MIN: i64 = -5;
const SMALL_INT_MAX: i64 = 256;

/// Wrapper for Vec of raw pointers to make it Send
struct SmallIntCache(Vec<*mut RawPyObject>);
unsafe impl Send for SmallIntCache {}

use once_cell::sync::Lazy;
use parking_lot::Mutex;

static SMALL_INTS: Lazy<Mutex<SmallIntCache>> = Lazy::new(|| {
    let mut cache = Vec::new();
    let count = (SMALL_INT_MAX - SMALL_INT_MIN + 1) as usize;
    cache.reserve(count);
    for i in SMALL_INT_MIN..=SMALL_INT_MAX {
        unsafe {
            let obj = create_long_from_i64(i);
            // Make immortal
            (*obj).ob_refcnt = AtomicIsize::new(isize::MAX / 2);
            cache.push(obj);
        }
    }
    Mutex::new(SmallIntCache(cache))
});

/// Create a long, using the small int cache when possible.
unsafe fn create_long(value: i64) -> *mut RawPyObject {
    if value >= SMALL_INT_MIN && value <= SMALL_INT_MAX {
        let cache = SMALL_INTS.lock();
        let idx = (value - SMALL_INT_MIN) as usize;
        let obj = cache.0[idx];
        (*obj).incref();
        return obj;
    }
    create_long_from_i64(value)
}

// ─── C API ───

#[no_mangle]
pub unsafe extern "C" fn PyLong_FromLong(v: c_long) -> *mut RawPyObject {
    create_long(v as i64)
}

#[no_mangle]
pub unsafe extern "C" fn PyLong_FromUnsignedLong(v: c_ulong) -> *mut RawPyObject {
    // For values > i64::MAX, create directly without cache
    if v as u64 > i64::MAX as u64 {
        return create_long_from_i64(v as i64); // wraps, but preserves bits
    }
    create_long(v as i64)
}

#[no_mangle]
pub unsafe extern "C" fn PyLong_FromLongLong(v: c_longlong) -> *mut RawPyObject {
    create_long(v as i64)
}

#[no_mangle]
pub unsafe extern "C" fn PyLong_FromUnsignedLongLong(v: c_ulonglong) -> *mut RawPyObject {
    if v > i64::MAX as u64 {
        // Large unsigned: allocate directly with digit math
        let mut abs_val = v;
        let mut ndigits = 0usize;
        let mut tmp = abs_val;
        while tmp > 0 {
            ndigits += 1;
            tmp >>= PYLONG_SHIFT;
        }
        let obj = alloc_long(ndigits);
        (*obj).ob_base.ob_size = ndigits as isize;
        let digits = ob_digit(obj);
        for i in 0..ndigits {
            *digits.add(i) = (abs_val & PYLONG_MASK as u64) as Digit;
            abs_val >>= PYLONG_SHIFT;
        }
        return obj as *mut RawPyObject;
    }
    create_long(v as i64)
}

#[no_mangle]
pub unsafe extern "C" fn PyLong_FromDouble(v: f64) -> *mut RawPyObject {
    create_long(v as i64)
}

#[no_mangle]
pub unsafe extern "C" fn PyLong_FromSsize_t(v: isize) -> *mut RawPyObject {
    create_long(v as i64)
}

#[no_mangle]
pub unsafe extern "C" fn PyLong_FromSize_t(v: usize) -> *mut RawPyObject {
    if v > i64::MAX as u64 as usize {
        return PyLong_FromUnsignedLongLong(v as c_ulonglong);
    }
    create_long(v as i64)
}

#[no_mangle]
pub unsafe extern "C" fn PyLong_AsLong(obj: *mut RawPyObject) -> c_long {
    if obj.is_null() { return -1; }
    pylong_to_i64(obj as *mut PyLongObject).unwrap_or(-1) as c_long
}

#[no_mangle]
pub unsafe extern "C" fn PyLong_AsUnsignedLong(obj: *mut RawPyObject) -> c_ulong {
    if obj.is_null() { return 0; }
    let v = pylong_to_i64(obj as *mut PyLongObject).unwrap_or(0);
    if v < 0 { 0 } else { v as c_ulong }
}

#[no_mangle]
pub unsafe extern "C" fn PyLong_AsLongLong(obj: *mut RawPyObject) -> c_longlong {
    if obj.is_null() { return -1; }
    pylong_to_i64(obj as *mut PyLongObject).unwrap_or(-1) as c_longlong
}

#[no_mangle]
pub unsafe extern "C" fn PyLong_AsUnsignedLongLong(obj: *mut RawPyObject) -> c_ulonglong {
    if obj.is_null() { return 0; }
    let v = pylong_to_i64(obj as *mut PyLongObject).unwrap_or(0);
    if v < 0 { 0 } else { v as c_ulonglong }
}

#[no_mangle]
pub unsafe extern "C" fn PyLong_AsDouble(obj: *mut RawPyObject) -> f64 {
    if obj.is_null() { return -1.0; }
    pylong_to_f64(obj as *mut PyLongObject)
}

#[no_mangle]
pub unsafe extern "C" fn PyLong_AsSsize_t(obj: *mut RawPyObject) -> isize {
    if obj.is_null() { return -1; }
    pylong_to_i64(obj as *mut PyLongObject).unwrap_or(-1) as isize
}

#[no_mangle]
pub unsafe extern "C" fn PyLong_AsSize_t(obj: *mut RawPyObject) -> usize {
    if obj.is_null() { return 0; }
    let v = pylong_to_i64(obj as *mut PyLongObject).unwrap_or(0);
    if v < 0 { 0 } else { v as usize }
}

#[no_mangle]
pub unsafe extern "C" fn PyLong_Check(obj: *mut RawPyObject) -> c_int {
    if obj.is_null() { return 0; }
    let tp = (*obj).ob_type;
    // Accept both int and bool (bool subclasses int)
    if tp == long_type()
        || tp == crate::types::boolobject::bool_type()
    {
        1
    } else {
        0
    }
}

#[no_mangle]
pub static mut PyLong_Type: *mut RawPyTypeObject = std::ptr::null_mut();

/// PyLong_FromString — parse a C string as an integer in the given base.
#[no_mangle]
pub unsafe extern "C" fn PyLong_FromString(
    str_ptr: *const std::os::raw::c_char,
    pend: *mut *mut std::os::raw::c_char,
    base: c_int,
) -> *mut RawPyObject {
    if str_ptr.is_null() {
        return std::ptr::null_mut();
    }
    let c_str = std::ffi::CStr::from_ptr(str_ptr);
    let s = c_str.to_string_lossy();
    let s = s.trim();

    // Parse using the specified base
    let result: Result<i64, _> = if base == 0 {
        // Auto-detect base from prefix
        if s.starts_with("0x") || s.starts_with("0X") {
            i64::from_str_radix(&s[2..], 16)
        } else if s.starts_with("0o") || s.starts_with("0O") {
            i64::from_str_radix(&s[2..], 8)
        } else if s.starts_with("0b") || s.starts_with("0B") {
            i64::from_str_radix(&s[2..], 2)
        } else {
            s.parse()
        }
    } else {
        i64::from_str_radix(s, base as u32)
    };

    // Set pend to end of string if requested
    if !pend.is_null() {
        *pend = str_ptr.add(c_str.to_bytes().len()) as *mut _;
    }

    match result {
        Ok(v) => create_long(v),
        Err(_) => {
            // Try BigInt for very large numbers
            if let Ok(big) = s.parse::<BigInt>() {
                let obj = create_long_from_bigint(&big);
                return obj;
            }
            std::ptr::null_mut()
        }
    }
}

/// Create a PyLongObject from a BigInt.
unsafe fn create_long_from_bigint(value: &BigInt) -> *mut RawPyObject {
    use num_traits::Zero;
    use num_traits::Signed;

    if value.is_zero() {
        return create_long(0);
    }

    // Convert to string and parse back via i64 if possible
    if let Some(v) = value.to_i64() {
        return create_long(v);
    }

    // Large value: convert via digit decomposition
    let negative = value.is_negative();
    let abs_val = if negative { -value.clone() } else { value.clone() };

    // Extract 30-bit digits
    let mut digits = Vec::new();
    let mut remaining = abs_val;
    let base = BigInt::from(PYLONG_BASE);
    while remaining > BigInt::from(0) {
        let digit = (&remaining % &base).to_u64().unwrap_or(0) as Digit;
        digits.push(digit);
        remaining = remaining / &base;
    }

    let ndigits = digits.len().max(1);
    let obj = alloc_long(ndigits);
    (*obj).ob_base.ob_size = if negative {
        -(ndigits as isize)
    } else {
        ndigits as isize
    };
    let digit_ptr = ob_digit(obj);
    for (i, &d) in digits.iter().enumerate() {
        *digit_ptr.add(i) = d;
    }
    obj as *mut RawPyObject
}

/// PyNumber_ToBase — convert an integer to a string in the given base.
/// Only base 2, 8, 10, 16 are supported (matching CPython).
#[no_mangle]
pub unsafe extern "C" fn PyNumber_ToBase(
    obj: *mut RawPyObject,
    base: c_int,
) -> *mut RawPyObject {
    if obj.is_null() {
        return std::ptr::null_mut();
    }
    let big = pylong_to_bigint(obj as *mut PyLongObject);
    let s = match base {
        2 => format!("{}", big.to_str_radix(2)),
        8 => format!("{}", big.to_str_radix(8)),
        10 => format!("{}", big.to_str_radix(10)),
        16 => format!("{}", big.to_str_radix(16)),
        _ => format!("{}", big),
    };
    crate::types::unicode::create_from_str(&s)
}

pub unsafe fn init_long_type() {
    LONG_TYPE.tp_dealloc = Some(long_dealloc);
    LONG_TYPE.tp_flags = crate::object::typeobj::PY_TPFLAGS_DEFAULT
        | crate::object::typeobj::PY_TPFLAGS_LONG_SUBCLASS;
    PyLong_Type = long_type();
}
