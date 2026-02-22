//! Rust-side Python exception type.
//!
//! `PyErr` captures CPython's thread-local exception state and converts it
//! into a Rust `Result`-compatible error type. This bridges the gap between
//! CPython's "return NULL and set error indicator" convention and Rust's
//! `Result<T, E>` pattern.

use crate::object::pyobject::{PyObjectRef, RawPyObject};
use std::ptr;

/// A Rust-side Python exception, fetched from CPython's thread-local error state.
///
/// Holds raw pointers to the exception type, value, and traceback.
/// These are owned (new) references obtained from `PyErr_Fetch`.
pub struct PyErr {
    pub exc_type: *mut RawPyObject,
    pub exc_value: *mut RawPyObject,
    pub exc_traceback: *mut RawPyObject,
}

// PyErr holds raw pointers protected by the GIL contract
unsafe impl Send for PyErr {}
unsafe impl Sync for PyErr {}

impl PyErr {
    /// Fetch, normalize, and clear the current CPython thread-local exception.
    ///
    /// CPython's error state is lazy — `exc_value` may be a raw string or tuple,
    /// not an instantiated Exception object. `PyErr_NormalizeException` forces
    /// CPython to materialize the actual Exception instance. Without this,
    /// `try/except` in the eval loop would receive un-normalized garbage.
    pub fn fetch() -> Self {
        let mut tp = ptr::null_mut();
        let mut val = ptr::null_mut();
        let mut tb = ptr::null_mut();
        unsafe {
            crate::runtime::error::PyErr_Fetch(&mut tp, &mut val, &mut tb);
            crate::runtime::error::PyErr_NormalizeException(&mut tp, &mut val, &mut tb);
        }
        PyErr {
            exc_type: tp,
            exc_value: val,
            exc_traceback: tb,
        }
    }

    /// Check if a CPython exception is pending; if so, fetch and normalize it.
    pub fn occurred() -> Option<Self> {
        let pending = unsafe { crate::runtime::error::PyErr_Occurred() };
        if pending.is_null() {
            None
        } else {
            Some(Self::fetch())
        }
    }

    /// Restore this error into CPython's thread-local error state.
    /// Used when returning from Rust back into C API convention (NULL + error set).
    pub fn restore(self) {
        unsafe {
            crate::runtime::error::PyErr_Restore(
                self.exc_type,
                self.exc_value,
                self.exc_traceback,
            );
        }
        // Don't run Drop — ownership transferred to CPython's error state
        std::mem::forget(self);
    }

    // ─── Convenience constructors ───
    // These set the CPython thread-local error and then fetch it back,
    // so the error is properly registered in both the C and Rust worlds.

    /// Create a NameError with the given variable name.
    pub fn name_error(name: &str) -> Self {
        let msg = format!("name '{}' is not defined", name);
        Self::from_exc_and_msg(
            unsafe { *crate::runtime::error::PyExc_RuntimeError.get() },
            &msg,
        )
    }

    /// Create a TypeError with the given message.
    pub fn type_error(msg: &str) -> Self {
        Self::from_exc_and_msg(
            unsafe { *crate::runtime::error::PyExc_TypeError.get() },
            msg,
        )
    }

    /// Create an ImportError with the given module name.
    pub fn import_error(name: &str) -> Self {
        let msg = format!("No module named '{}'", name);
        Self::from_exc_and_msg(
            unsafe { *crate::runtime::error::PyExc_ImportError.get() },
            msg.as_str(),
        )
    }

    /// Create a ValueError with the given message.
    pub fn value_error(msg: &str) -> Self {
        Self::from_exc_and_msg(
            unsafe { *crate::runtime::error::PyExc_ValueError.get() },
            msg,
        )
    }

    /// Create a MemoryError.
    pub fn memory_error() -> Self {
        Self::from_exc_and_msg(
            unsafe { *crate::runtime::error::PyExc_MemoryError.get() },
            "out of memory",
        )
    }

    /// Create an AttributeError with the given message.
    pub fn attribute_error(msg: &str) -> Self {
        Self::from_exc_and_msg(
            unsafe { *crate::runtime::error::PyExc_AttributeError.get() },
            msg,
        )
    }

    /// Create a KeyError with the given message.
    pub fn key_error(msg: &str) -> Self {
        Self::from_exc_and_msg(
            unsafe { *crate::runtime::error::PyExc_KeyError.get() },
            msg,
        )
    }

    /// Create an IndexError with the given message.
    pub fn index_error(msg: &str) -> Self {
        Self::from_exc_and_msg(
            unsafe { *crate::runtime::error::PyExc_IndexError.get() },
            msg,
        )
    }

    /// Create a RuntimeError with the given message.
    pub fn runtime_error(msg: &str) -> Self {
        Self::from_exc_and_msg(
            unsafe { *crate::runtime::error::PyExc_RuntimeError.get() },
            msg,
        )
    }

    /// Create a ZeroDivisionError with the given message.
    pub fn zero_division_error(msg: &str) -> Self {
        Self::from_exc_and_msg(
            unsafe { *crate::runtime::error::PyExc_ZeroDivisionError.get() },
            msg,
        )
    }

    /// Create a StopIteration.
    pub fn stop_iteration() -> Self {
        Self::from_exc_and_msg(
            unsafe { *crate::runtime::error::PyExc_StopIteration.get() },
            "",
        )
    }

    /// Internal helper: set exception via PyErr_SetString, then fetch it back.
    fn from_exc_and_msg(exc_type: *mut RawPyObject, msg: &str) -> Self {
        let cstr = std::ffi::CString::new(msg).unwrap_or_else(|_| {
            std::ffi::CString::new("(error message contained null byte)").unwrap()
        });
        unsafe {
            crate::runtime::error::PyErr_SetString(exc_type, cstr.as_ptr());
        }
        Self::fetch()
    }
}

impl std::fmt::Display for PyErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Extract type name
        let type_name = if !self.exc_type.is_null() {
            let tp = self.exc_type as *const crate::object::typeobj::RawPyTypeObject;
            unsafe {
                if !(*tp).tp_name.is_null() {
                    std::ffi::CStr::from_ptr((*tp).tp_name)
                        .to_string_lossy()
                        .into_owned()
                } else {
                    "Exception".to_string()
                }
            }
        } else {
            "Exception".to_string()
        };

        // Extract value message
        if !self.exc_value.is_null() {
            let val = self.exc_value;
            // Check if it's a string
            if crate::object::safe_api::is_str(val) {
                let msg = crate::types::unicode::unicode_value(val);
                return write!(f, "{}: {}", type_name, msg);
            }
            // Try repr
            let repr = unsafe { crate::ffi::object_api::PyObject_Repr(val) };
            if !repr.is_null() {
                let msg = crate::types::unicode::unicode_value(repr);
                unsafe { (*repr).decref(); }
                return write!(f, "{}: {}", type_name, msg);
            }
        }

        write!(f, "{}", type_name)
    }
}

impl std::fmt::Debug for PyErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PyErr({})", self)
    }
}

impl Drop for PyErr {
    fn drop(&mut self) {
        // Decref owned references if they haven't been restored
        if !self.exc_type.is_null() {
            unsafe { (*self.exc_type).decref(); }
        }
        if !self.exc_value.is_null() {
            unsafe { (*self.exc_value).decref(); }
        }
        if !self.exc_traceback.is_null() {
            unsafe { (*self.exc_traceback).decref(); }
        }
    }
}

/// The canonical Result type for Python operations.
/// Ok holds an owned `PyObjectRef`, Err holds a `PyErr`.
pub type PyResult = Result<PyObjectRef, PyErr>;
