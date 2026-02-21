//! Python exception / error handling at the C level.
//!
//! CPython uses thread-local state to track the current exception.
//! C extensions check PyErr_Occurred() and set errors with PyErr_SetString().
//! We replicate this exact mechanism.

use crate::object::pyobject::RawPyObject;
use std::cell::RefCell;
use std::os::raw::c_char;
use std::ptr;

/// Thread-local error state, matching CPython's error indicator.
struct ErrorState {
    /// The exception type (borrowed reference to a type object)
    exc_type: *mut RawPyObject,
    /// The exception value
    exc_value: *mut RawPyObject,
    /// The traceback
    exc_traceback: *mut RawPyObject,
}

impl ErrorState {
    fn new() -> Self {
        ErrorState {
            exc_type: ptr::null_mut(),
            exc_value: ptr::null_mut(),
            exc_traceback: ptr::null_mut(),
        }
    }

    fn clear(&mut self) {
        // In a full implementation, we'd Py_XDECREF these
        self.exc_type = ptr::null_mut();
        self.exc_value = ptr::null_mut();
        self.exc_traceback = ptr::null_mut();
    }

    fn is_set(&self) -> bool {
        !self.exc_type.is_null()
    }
}

thread_local! {
    static ERROR_STATE: RefCell<ErrorState> = RefCell::new(ErrorState::new());
}

fn with_error<F, R>(f: F) -> R
where
    F: FnOnce(&mut ErrorState) -> R,
{
    ERROR_STATE.with(|state| f(&mut state.borrow_mut()))
}

// ─── C API exports ───

/// PyErr_SetString - set an error with a type and message string.
/// This is the most common way C extensions signal errors.
#[no_mangle]
pub unsafe extern "C" fn PyErr_SetString(exc_type: *mut RawPyObject, message: *const c_char) {
    // In a full implementation, we'd create a proper exception object from the message.
    // For now, store the type directly and create a string value.
    with_error(|state| {
        state.exc_type = exc_type;
        // Create a Python string from the message
        if !message.is_null() {
            let msg_str = std::ffi::CStr::from_ptr(message);
            // For now, store the type as the value too (placeholder)
            // A full implementation would create a PyUnicode from msg_str
            state.exc_value = exc_type; // TODO: create proper exception instance
        }
        state.exc_traceback = ptr::null_mut();
    });
}

/// PyErr_SetObject - set an error with a type and value object.
#[no_mangle]
pub unsafe extern "C" fn PyErr_SetObject(
    exc_type: *mut RawPyObject,
    exc_value: *mut RawPyObject,
) {
    with_error(|state| {
        state.exc_type = exc_type;
        state.exc_value = exc_value;
        state.exc_traceback = ptr::null_mut();
    });
}

/// PyErr_Occurred - check if an error is set. Returns the exception type or NULL.
#[no_mangle]
pub unsafe extern "C" fn PyErr_Occurred() -> *mut RawPyObject {
    with_error(|state| {
        if state.is_set() {
            state.exc_type
        } else {
            ptr::null_mut()
        }
    })
}

/// PyErr_Clear - clear the current error.
#[no_mangle]
pub unsafe extern "C" fn PyErr_Clear() {
    with_error(|state| state.clear());
}

/// PyErr_Fetch - fetch and clear the error indicator.
/// This is how C extensions catch exceptions.
#[no_mangle]
pub unsafe extern "C" fn PyErr_Fetch(
    ptype: *mut *mut RawPyObject,
    pvalue: *mut *mut RawPyObject,
    ptraceback: *mut *mut RawPyObject,
) {
    with_error(|state| {
        if !ptype.is_null() {
            *ptype = state.exc_type;
        }
        if !pvalue.is_null() {
            *pvalue = state.exc_value;
        }
        if !ptraceback.is_null() {
            *ptraceback = state.exc_traceback;
        }
        // Clear without decref (ownership transferred to caller)
        state.exc_type = ptr::null_mut();
        state.exc_value = ptr::null_mut();
        state.exc_traceback = ptr::null_mut();
    });
}

/// PyErr_Restore - set the error indicator from fetched values.
#[no_mangle]
pub unsafe extern "C" fn PyErr_Restore(
    exc_type: *mut RawPyObject,
    exc_value: *mut RawPyObject,
    exc_traceback: *mut RawPyObject,
) {
    with_error(|state| {
        state.exc_type = exc_type;
        state.exc_value = exc_value;
        state.exc_traceback = exc_traceback;
    });
}

/// PyErr_NormalizeException - normalize the exception.
/// Some extensions call this to ensure the exception value is an instance of the type.
#[no_mangle]
pub unsafe extern "C" fn PyErr_NormalizeException(
    _exc: *mut *mut RawPyObject,
    _val: *mut *mut RawPyObject,
    _tb: *mut *mut RawPyObject,
) {
    // TODO: Full normalization (instantiate exception if needed)
}

/// PyErr_SetNone - set an error with no value.
#[no_mangle]
pub unsafe extern "C" fn PyErr_SetNone(exc_type: *mut RawPyObject) {
    with_error(|state| {
        state.exc_type = exc_type;
        state.exc_value = ptr::null_mut();
        state.exc_traceback = ptr::null_mut();
    });
}

/// PyErr_ExceptionMatches - check if the current exception matches a given type.
#[no_mangle]
pub unsafe extern "C" fn PyErr_ExceptionMatches(exc: *mut RawPyObject) -> i32 {
    with_error(|state| {
        if state.exc_type == exc {
            1
        } else {
            0 // TODO: check subclass relationships
        }
    })
}

/// PyErr_Format - set error with a formatted string.
/// For now, just passes the format string directly.
#[no_mangle]
pub unsafe extern "C" fn PyErr_Format(
    exc_type: *mut RawPyObject,
    format: *const c_char,
    // varargs not supported in Rust extern "C" easily,
    // but we can handle the common case
) -> *mut RawPyObject {
    PyErr_SetString(exc_type, format);
    ptr::null_mut()
}

/// PyErr_BadArgument
#[no_mangle]
pub unsafe extern "C" fn PyErr_BadArgument() -> i32 {
    // TODO: Set TypeError with "bad argument type for built-in operation"
    0
}

/// PyErr_NoMemory - convenience for setting MemoryError
#[no_mangle]
pub unsafe extern "C" fn PyErr_NoMemory() -> *mut RawPyObject {
    // TODO: Set MemoryError
    ptr::null_mut()
}

// ─── Internal helpers ───

/// Set an error from Rust code (convenience).
pub fn set_error(exc_type: *mut RawPyObject, message: &str) {
    let c_msg = std::ffi::CString::new(message).unwrap_or_default();
    unsafe {
        PyErr_SetString(exc_type, c_msg.as_ptr());
    }
}

/// Check if an error is currently set (Rust-side convenience).
pub fn error_occurred() -> bool {
    unsafe { !PyErr_Occurred().is_null() }
}

/// Clear the current error (Rust-side convenience).
pub fn clear_error() {
    unsafe {
        PyErr_Clear();
    }
}
