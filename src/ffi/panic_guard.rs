//! FFI panic safety: catch_unwind guards for every extern "C" return type.
//!
//! Every non-trivial `extern "C"` function in Rustthon wraps its body in one of
//! these guards. If the closure panics (OOM, unwrap failure, index OOB, etc.),
//! the guard catches the unwind, sets a Python `SystemError`, and returns the
//! appropriate C error sentinel — preventing undefined behavior from unwinding
//! across the C/Rust FFI boundary.
//!
//! **Intermediate raw-pointer leaks are acceptable**: if a panic occurs after a
//! raw pointer is allocated but before it's returned, that memory leaks. A small
//! leak during an unrecoverable `SystemError` is vastly preferable to cross-FFI UB.

use std::panic::{catch_unwind, AssertUnwindSafe};

// ─── Compile-time unwind guarantee ───
// catch_unwind is a no-op under panic="abort". If someone sets that
// in Cargo.toml, our entire FFI safety net vanishes silently.
// Fail the build loudly instead.
#[cfg(panic = "abort")]
compile_error!(
    "Rustthon MUST be compiled with panic=\"unwind\" (the default). \
     panic=\"abort\" disables catch_unwind, which protects the C API boundary."
);

/// Set SystemError in CPython's thread-local error state.
/// Uses only static strings — MUST NOT allocate, since we may be
/// in an OOM or double-panic scenario.
///
/// CRITICAL: This function calls PyErr_SetString directly.
/// PyErr_SetString MUST NOT be wrapped in a guard (see Exclusions),
/// otherwise we get infinite recursion on panic.
#[inline(never)] // keep off the hot path
unsafe fn set_panic_error() {
    crate::runtime::error::PyErr_SetString(
        *crate::runtime::error::PyExc_SystemError.get(),
        b"internal error: Rust panic in C API function\0".as_ptr()
            as *const std::os::raw::c_char,
    );
}

/// Guard for functions returning `*mut T` (NULL on panic).
#[inline(always)]
pub fn guard_ptr<T, F: FnOnce() -> *mut T>(_name: &str, f: F) -> *mut T {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(_) => {
            unsafe { set_panic_error(); }
            std::ptr::null_mut()
        }
    }
}

/// Guard for functions returning `c_int` (-1 on panic).
#[inline(always)]
pub fn guard_int<F: FnOnce() -> std::os::raw::c_int>(_name: &str, f: F)
    -> std::os::raw::c_int
{
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(_) => {
            unsafe { set_panic_error(); }
            -1
        }
    }
}

/// Guard for functions returning `i32` (-1 on panic).
#[inline(always)]
pub fn guard_i32<F: FnOnce() -> i32>(_name: &str, f: F) -> i32 {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(_) => {
            unsafe { set_panic_error(); }
            -1
        }
    }
}

/// Guard for functions returning `isize` (-1 on panic).
#[inline(always)]
pub fn guard_ssize<F: FnOnce() -> isize>(_name: &str, f: F) -> isize {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(_) => {
            unsafe { set_panic_error(); }
            -1
        }
    }
}

/// Guard for functions returning `f64` (-1.0 on panic; caller must check PyErr_Occurred).
#[inline(always)]
pub fn guard_f64<F: FnOnce() -> f64>(_name: &str, f: F) -> f64 {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(_) => {
            unsafe { set_panic_error(); }
            -1.0
        }
    }
}

/// Guard for void functions (no return value; sets SystemError on panic).
#[inline(always)]
pub fn guard_void<F: FnOnce()>(_name: &str, f: F) {
    if let Err(_) = catch_unwind(AssertUnwindSafe(f)) {
        unsafe { set_panic_error(); }
    }
}

/// Guard for functions returning `*const T` (null on panic).
#[inline(always)]
pub fn guard_const_ptr<T, F: FnOnce() -> *const T>(_name: &str, f: F) -> *const T {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(_) => {
            unsafe { set_panic_error(); }
            std::ptr::null()
        }
    }
}

/// Guard for functions returning `u64` (0 on panic).
#[inline(always)]
pub fn guard_u64<F: FnOnce() -> u64>(_name: &str, f: F) -> u64 {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(_) => {
            unsafe { set_panic_error(); }
            0
        }
    }
}

/// Guard for functions returning `usize` (0 on panic).
#[inline(always)]
pub fn guard_usize<F: FnOnce() -> usize>(_name: &str, f: F) -> usize {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(_) => {
            unsafe { set_panic_error(); }
            0
        }
    }
}

/// Guard for functions returning `i64` (-1 on panic).
#[inline(always)]
pub fn guard_i64<F: FnOnce() -> i64>(_name: &str, f: F) -> i64 {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(_) => {
            unsafe { set_panic_error(); }
            -1
        }
    }
}
