//! Python exception / error handling at the C level.
//!
//! CPython uses thread-local state to track the current exception.
//! C extensions check PyErr_Occurred() and set errors with PyErr_SetString().
//! We replicate this exact mechanism.

use crate::object::pyobject::RawPyObject;
use crate::object::StaticPtr;
use std::cell::RefCell;
use std::os::raw::{c_char, c_int};
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
    with_error(|state| {
        state.exc_type = exc_type;
        if !message.is_null() {
            state.exc_value = crate::types::unicode::PyUnicode_FromString(message);
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
/// Walks the tp_base chain to check subclass relationships.
#[no_mangle]
pub unsafe extern "C" fn PyErr_ExceptionMatches(exc: *mut RawPyObject) -> i32 {
    with_error(|state| {
        if state.exc_type.is_null() || exc.is_null() {
            return 0;
        }
        if state.exc_type == exc {
            return 1;
        }
        // Walk tp_base chain of the current exception type
        let exc_tp = exc as *mut RawPyTypeObject;
        let mut cur = state.exc_type as *mut RawPyTypeObject;
        while !cur.is_null() {
            if cur as *mut RawPyObject == exc || cur == exc_tp {
                return 1;
            }
            cur = (*cur).tp_base;
        }
        0
    })
}

/// PyErr_GivenExceptionMatches - check if a given exception matches a type.
#[no_mangle]
pub unsafe extern "C" fn PyErr_GivenExceptionMatches(
    err: *mut RawPyObject,
    exc: *mut RawPyObject,
) -> i32 {
    if err.is_null() || exc.is_null() {
        return 0;
    }
    if err == exc {
        return 1;
    }
    // Walk tp_base chain
    let exc_tp = exc as *mut RawPyTypeObject;
    let mut cur = err as *mut RawPyTypeObject;
    while !cur.is_null() {
        if cur == exc_tp {
            return 1;
        }
        cur = (*cur).tp_base;
    }
    0
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

/// PyErr_NewException — create a new exception class.
/// Returns a new reference to an exception type object.
/// For now, returns a minimal sentinel object that can be used as an exception type.
#[no_mangle]
pub unsafe extern "C" fn PyErr_NewException(
    name: *const c_char,
    base: *mut RawPyObject,
    _dict: *mut RawPyObject,
) -> *mut RawPyObject {
    if name.is_null() {
        return ptr::null_mut();
    }

    // Determine the base exception type
    let base_tp = if !base.is_null() {
        base as *mut RawPyTypeObject
    } else {
        *PyExc_Exception.get() as *mut RawPyTypeObject
    };

    // We need a stable name pointer — heap-allocate a copy of the name string
    let name_cstr = std::ffi::CStr::from_ptr(name);
    let name_bytes = name_cstr.to_bytes_with_nul();
    let name_copy = libc::malloc(name_bytes.len()) as *mut u8;
    if name_copy.is_null() {
        return ptr::null_mut();
    }
    std::ptr::copy_nonoverlapping(name_bytes.as_ptr(), name_copy, name_bytes.len());

    // Allocate a real PyTypeObject with proper base chain
    let tp = libc::calloc(1, std::mem::size_of::<RawPyTypeObject>()) as *mut RawPyTypeObject;
    if tp.is_null() {
        return ptr::null_mut();
    }
    std::ptr::write(tp, RawPyTypeObject::zeroed());

    (*tp).tp_name = name_copy as *const c_char;
    (*tp).ob_base.ob_type = crate::object::typeobj::PyType_Type.get();
    (*tp).ob_base.ob_refcnt = std::sync::atomic::AtomicIsize::new(1);
    (*tp).tp_basicsize = std::mem::size_of::<RawPyObject>() as isize;
    (*tp).tp_flags = crate::object::typeobj::PY_TPFLAGS_DEFAULT
        | crate::object::typeobj::PY_TPFLAGS_READY;

    // Set base type for PyType_IsSubtype to work
    if !base_tp.is_null() {
        (*tp).tp_base = base_tp;
        let bases = crate::types::tuple::PyTuple_New(1);
        let base_obj = base_tp as *mut RawPyObject;
        (*base_obj).incref();
        crate::types::tuple::PyTuple_SetItem(bases, 0, base_obj);
        (*tp).tp_bases = bases;
    }

    // Inherit slots from base
    if !base_tp.is_null() {
        if (*tp).tp_alloc.is_none() { (*tp).tp_alloc = (*base_tp).tp_alloc; }
        if (*tp).tp_new.is_none() { (*tp).tp_new = (*base_tp).tp_new; }
        if (*tp).tp_init.is_none() { (*tp).tp_init = (*base_tp).tp_init; }
        if (*tp).tp_free.is_none() { (*tp).tp_free = (*base_tp).tp_free; }
        if (*tp).tp_dealloc.is_none() { (*tp).tp_dealloc = (*base_tp).tp_dealloc; }
        if (*tp).tp_getattro.is_none() { (*tp).tp_getattro = (*base_tp).tp_getattro; }
    }

    tp as *mut RawPyObject
}

// ─── Exception type singletons ───
// Prebuilt C extensions expect these as DATA symbols: `extern PyObject *PyExc_TypeError;`
// Each points to a real PyTypeObject with proper inheritance chain.

use crate::object::typeobj::RawPyTypeObject;

// Pointer variables matching CPython ABI
#[no_mangle] pub static PyExc_BaseException: StaticPtr<*mut RawPyObject> = StaticPtr::new(ptr::null_mut());
#[no_mangle] pub static PyExc_Exception: StaticPtr<*mut RawPyObject> = StaticPtr::new(ptr::null_mut());
#[no_mangle] pub static PyExc_TypeError: StaticPtr<*mut RawPyObject> = StaticPtr::new(ptr::null_mut());
#[no_mangle] pub static PyExc_ValueError: StaticPtr<*mut RawPyObject> = StaticPtr::new(ptr::null_mut());
#[no_mangle] pub static PyExc_OverflowError: StaticPtr<*mut RawPyObject> = StaticPtr::new(ptr::null_mut());
#[no_mangle] pub static PyExc_RuntimeError: StaticPtr<*mut RawPyObject> = StaticPtr::new(ptr::null_mut());
#[no_mangle] pub static PyExc_KeyError: StaticPtr<*mut RawPyObject> = StaticPtr::new(ptr::null_mut());
#[no_mangle] pub static PyExc_IndexError: StaticPtr<*mut RawPyObject> = StaticPtr::new(ptr::null_mut());
#[no_mangle] pub static PyExc_AttributeError: StaticPtr<*mut RawPyObject> = StaticPtr::new(ptr::null_mut());
#[no_mangle] pub static PyExc_StopIteration: StaticPtr<*mut RawPyObject> = StaticPtr::new(ptr::null_mut());
#[no_mangle] pub static PyExc_MemoryError: StaticPtr<*mut RawPyObject> = StaticPtr::new(ptr::null_mut());
#[no_mangle] pub static PyExc_SystemError: StaticPtr<*mut RawPyObject> = StaticPtr::new(ptr::null_mut());
#[no_mangle] pub static PyExc_OSError: StaticPtr<*mut RawPyObject> = StaticPtr::new(ptr::null_mut());
#[no_mangle] pub static PyExc_NotImplementedError: StaticPtr<*mut RawPyObject> = StaticPtr::new(ptr::null_mut());
#[no_mangle] pub static PyExc_UnicodeDecodeError: StaticPtr<*mut RawPyObject> = StaticPtr::new(ptr::null_mut());
#[no_mangle] pub static PyExc_UnicodeEncodeError: StaticPtr<*mut RawPyObject> = StaticPtr::new(ptr::null_mut());
#[no_mangle] pub static PyExc_UnicodeError: StaticPtr<*mut RawPyObject> = StaticPtr::new(ptr::null_mut());
#[no_mangle] pub static PyExc_LookupError: StaticPtr<*mut RawPyObject> = StaticPtr::new(ptr::null_mut());
#[no_mangle] pub static PyExc_ArithmeticError: StaticPtr<*mut RawPyObject> = StaticPtr::new(ptr::null_mut());
#[no_mangle] pub static PyExc_IOError: StaticPtr<*mut RawPyObject> = StaticPtr::new(ptr::null_mut());
#[no_mangle] pub static PyExc_ImportError: StaticPtr<*mut RawPyObject> = StaticPtr::new(ptr::null_mut());
#[no_mangle] pub static PyExc_DeprecationWarning: StaticPtr<*mut RawPyObject> = StaticPtr::new(ptr::null_mut());
#[no_mangle] pub static PyExc_RuntimeWarning: StaticPtr<*mut RawPyObject> = StaticPtr::new(ptr::null_mut());
#[no_mangle] pub static PyExc_UserWarning: StaticPtr<*mut RawPyObject> = StaticPtr::new(ptr::null_mut());
#[no_mangle] pub static PyExc_Warning: StaticPtr<*mut RawPyObject> = StaticPtr::new(ptr::null_mut());

/// Allocate an exception type object with the given name and base.
/// Returns a pointer to a heap-allocated, immortal RawPyTypeObject.
unsafe fn alloc_exc_type(name: &[u8], base: *mut RawPyTypeObject) -> *mut RawPyObject {
    let tp = libc::calloc(1, std::mem::size_of::<RawPyTypeObject>()) as *mut RawPyTypeObject;
    if tp.is_null() {
        eprintln!("Fatal: out of memory allocating exception type");
        std::process::abort();
    }
    // Copy zeroed template
    std::ptr::write(tp, RawPyTypeObject::zeroed());

    // Set fields
    (*tp).tp_name = name.as_ptr() as *const c_char;
    (*tp).ob_base.ob_type = crate::object::typeobj::PyType_Type.get();
    (*tp).ob_base.ob_refcnt = std::sync::atomic::AtomicIsize::new(isize::MAX / 2);
    (*tp).tp_basicsize = std::mem::size_of::<RawPyObject>() as isize;
    (*tp).tp_flags = crate::object::typeobj::PY_TPFLAGS_DEFAULT
        | crate::object::typeobj::PY_TPFLAGS_READY;

    // Set base type
    if !base.is_null() {
        (*tp).tp_base = base;
        // Create tp_bases tuple
        let bases = crate::types::tuple::PyTuple_New(1);
        let base_obj = base as *mut RawPyObject;
        (*base_obj).incref();
        crate::types::tuple::PyTuple_SetItem(bases, 0, base_obj);
        (*tp).tp_bases = bases;
    }

    // Inherit default slots from base
    if !base.is_null() {
        if (*tp).tp_alloc.is_none() { (*tp).tp_alloc = (*base).tp_alloc; }
        if (*tp).tp_new.is_none() { (*tp).tp_new = (*base).tp_new; }
        if (*tp).tp_init.is_none() { (*tp).tp_init = (*base).tp_init; }
        if (*tp).tp_free.is_none() { (*tp).tp_free = (*base).tp_free; }
        if (*tp).tp_dealloc.is_none() { (*tp).tp_dealloc = (*base).tp_dealloc; }
        if (*tp).tp_getattro.is_none() { (*tp).tp_getattro = (*base).tp_getattro; }
    }

    tp as *mut RawPyObject
}

/// Initialize the exception type hierarchy. Must be called after base types are ready.
pub unsafe fn init_exceptions() {
    // BaseException
    *PyExc_BaseException.get() = alloc_exc_type(
        b"BaseException\0", crate::object::typeobj::PyBaseObject_Type.get());

    let base_exc = *PyExc_BaseException.get() as *mut RawPyTypeObject;

    // Exception
    *PyExc_Exception.get() = alloc_exc_type(b"Exception\0", base_exc);
    let exc = *PyExc_Exception.get() as *mut RawPyTypeObject;

    // Direct subclasses of Exception
    *PyExc_TypeError.get() = alloc_exc_type(b"TypeError\0", exc);
    *PyExc_ValueError.get() = alloc_exc_type(b"ValueError\0", exc);
    *PyExc_RuntimeError.get() = alloc_exc_type(b"RuntimeError\0", exc);
    *PyExc_AttributeError.get() = alloc_exc_type(b"AttributeError\0", exc);
    *PyExc_StopIteration.get() = alloc_exc_type(b"StopIteration\0", exc);
    *PyExc_MemoryError.get() = alloc_exc_type(b"MemoryError\0", exc);
    *PyExc_SystemError.get() = alloc_exc_type(b"SystemError\0", exc);
    *PyExc_OSError.get() = alloc_exc_type(b"OSError\0", exc);
    *PyExc_IOError.get() = *PyExc_OSError.get(); // IOError is an alias for OSError

    // Intermediate base classes
    *PyExc_LookupError.get() = alloc_exc_type(b"LookupError\0", exc);
    let lookup = *PyExc_LookupError.get() as *mut RawPyTypeObject;
    *PyExc_KeyError.get() = alloc_exc_type(b"KeyError\0", lookup);
    *PyExc_IndexError.get() = alloc_exc_type(b"IndexError\0", lookup);

    *PyExc_ArithmeticError.get() = alloc_exc_type(b"ArithmeticError\0", exc);
    let arith = *PyExc_ArithmeticError.get() as *mut RawPyTypeObject;
    *PyExc_OverflowError.get() = alloc_exc_type(b"OverflowError\0", arith);

    // RuntimeError subclass
    let runtime = *PyExc_RuntimeError.get() as *mut RawPyTypeObject;
    *PyExc_NotImplementedError.get() = alloc_exc_type(b"NotImplementedError\0", runtime);

    // ValueError subclasses
    let val = *PyExc_ValueError.get() as *mut RawPyTypeObject;
    *PyExc_UnicodeError.get() = alloc_exc_type(b"UnicodeError\0", val);
    let unicode_err = *PyExc_UnicodeError.get() as *mut RawPyTypeObject;
    *PyExc_UnicodeDecodeError.get() = alloc_exc_type(b"UnicodeDecodeError\0", unicode_err);
    *PyExc_UnicodeEncodeError.get() = alloc_exc_type(b"UnicodeEncodeError\0", unicode_err);

    // ImportError
    *PyExc_ImportError.get() = alloc_exc_type(b"ImportError\0", exc);

    // Warning hierarchy
    *PyExc_Warning.get() = alloc_exc_type(b"Warning\0", exc);
    let warn = *PyExc_Warning.get() as *mut RawPyTypeObject;
    *PyExc_DeprecationWarning.get() = alloc_exc_type(b"DeprecationWarning\0", warn);
    *PyExc_RuntimeWarning.get() = alloc_exc_type(b"RuntimeWarning\0", warn);
    *PyExc_UserWarning.get() = alloc_exc_type(b"UserWarning\0", warn);
}

// Backward-compatible function accessors for Rustthon-compiled extensions.
// Our include/Python.h uses macros like #define PyExc_TypeError (_Rustthon_Exc_TypeError())

#[no_mangle]
pub unsafe extern "C" fn _Rustthon_Exc_TypeError() -> *mut RawPyObject { *PyExc_TypeError.get() }
#[no_mangle]
pub unsafe extern "C" fn _Rustthon_Exc_ValueError() -> *mut RawPyObject { *PyExc_ValueError.get() }
#[no_mangle]
pub unsafe extern "C" fn _Rustthon_Exc_OverflowError() -> *mut RawPyObject { *PyExc_OverflowError.get() }
#[no_mangle]
pub unsafe extern "C" fn _Rustthon_Exc_RuntimeError() -> *mut RawPyObject { *PyExc_RuntimeError.get() }
#[no_mangle]
pub unsafe extern "C" fn _Rustthon_Exc_KeyError() -> *mut RawPyObject { *PyExc_KeyError.get() }
#[no_mangle]
pub unsafe extern "C" fn _Rustthon_Exc_IndexError() -> *mut RawPyObject { *PyExc_IndexError.get() }
#[no_mangle]
pub unsafe extern "C" fn _Rustthon_Exc_AttributeError() -> *mut RawPyObject { *PyExc_AttributeError.get() }
#[no_mangle]
pub unsafe extern "C" fn _Rustthon_Exc_StopIteration() -> *mut RawPyObject { *PyExc_StopIteration.get() }
#[no_mangle]
pub unsafe extern "C" fn _Rustthon_Exc_MemoryError() -> *mut RawPyObject { *PyExc_MemoryError.get() }

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

// ─── Warning APIs (needed by Cython and PyO3) ───

/// PyErr_WarnEx — issue a warning. category is the warning class, stacklevel is ignored.
#[no_mangle]
pub unsafe extern "C" fn PyErr_WarnEx(
    category: *mut RawPyObject,
    message: *const c_char,
    _stacklevel: isize,
) -> c_int {
    if !message.is_null() {
        let msg = std::ffi::CStr::from_ptr(message).to_string_lossy();
        eprintln!("Warning: {}", msg);
    }
    0 // success
}

/// PyErr_WarnFormat — issue a warning with printf-style formatting.
/// Simplified: just passes format string directly.
#[no_mangle]
pub unsafe extern "C" fn PyErr_WarnFormat(
    category: *mut RawPyObject,
    _stacklevel: isize,
    format: *const c_char,
) -> c_int {
    PyErr_WarnEx(category, format, _stacklevel)
}

/// PyErr_Print — print the current exception to stderr and clear it.
#[no_mangle]
pub unsafe extern "C" fn PyErr_Print() {
    PyErr_PrintEx(1)
}

/// PyErr_PrintEx — print the current exception. set_sys_last_vars is ignored.
#[no_mangle]
pub unsafe extern "C" fn PyErr_PrintEx(_set_sys_last_vars: c_int) {
    let mut ptype: *mut RawPyObject = ptr::null_mut();
    let mut pvalue: *mut RawPyObject = ptr::null_mut();
    let mut ptb: *mut RawPyObject = ptr::null_mut();
    PyErr_Fetch(&mut ptype, &mut pvalue, &mut ptb);
    if !ptype.is_null() {
        let tp = ptype as *mut RawPyTypeObject;
        let name = if !(*tp).tp_name.is_null() {
            std::ffi::CStr::from_ptr((*tp).tp_name).to_string_lossy().into_owned()
        } else {
            "Exception".to_string()
        };
        if !pvalue.is_null() {
            let val_str = crate::ffi::object_api::PyObject_Str(pvalue);
            if !val_str.is_null() {
                let msg = crate::types::unicode::PyUnicode_AsUTF8(val_str);
                if !msg.is_null() {
                    let s = std::ffi::CStr::from_ptr(msg).to_string_lossy();
                    eprintln!("{}: {}", name, s);
                } else {
                    eprintln!("{}", name);
                }
            } else {
                eprintln!("{}", name);
            }
        } else {
            eprintln!("{}", name);
        }
    }
}

/// PyErr_WriteUnraisable — print a warning about an exception that can't be raised.
#[no_mangle]
pub unsafe extern "C" fn PyErr_WriteUnraisable(obj: *mut RawPyObject) {
    eprintln!("Exception ignored in: {:p}", obj);
    PyErr_Clear();
}

/// PyErr_NewExceptionWithDoc — like PyErr_NewException but with a docstring.
#[no_mangle]
pub unsafe extern "C" fn PyErr_NewExceptionWithDoc(
    name: *const c_char,
    doc: *const c_char,
    base: *mut RawPyObject,
    dict: *mut RawPyObject,
) -> *mut RawPyObject {
    let result = PyErr_NewException(name, base, dict);
    if !result.is_null() && !doc.is_null() {
        let tp = result as *mut RawPyTypeObject;
        (*tp).tp_doc = doc;
    }
    result
}

// ─── Exception object APIs (needed by PyO3) ───

/// PyException_GetTraceback — get the traceback from an exception instance.
#[no_mangle]
pub unsafe extern "C" fn PyException_GetTraceback(
    _exc: *mut RawPyObject,
) -> *mut RawPyObject {
    // Simplified: no traceback support yet
    ptr::null_mut()
}

/// PyException_SetTraceback — set the traceback on an exception instance.
#[no_mangle]
pub unsafe extern "C" fn PyException_SetTraceback(
    _exc: *mut RawPyObject,
    _tb: *mut RawPyObject,
) -> c_int {
    0 // success (no-op)
}

/// PyException_GetCause — get the __cause__ of an exception.
#[no_mangle]
pub unsafe extern "C" fn PyException_GetCause(
    _exc: *mut RawPyObject,
) -> *mut RawPyObject {
    ptr::null_mut()
}

/// PyException_SetCause — set the __cause__ of an exception.
#[no_mangle]
pub unsafe extern "C" fn PyException_SetCause(
    _exc: *mut RawPyObject,
    _cause: *mut RawPyObject,
) {
    // No-op for now
}

#[no_mangle]
pub unsafe extern "C" fn _Rustthon_Exc_ImportError() -> *mut RawPyObject { *PyExc_ImportError.get() }
