//! Interpreter initialization and lifecycle.

use crate::runtime::gil;
use crate::runtime::thread_state;
use crate::types;

/// Initialize the Rustthon interpreter.
/// This must be called before any Python code runs.
pub fn initialize() {
    // Initialize the type system (creates built-in type objects)
    types::init_types();

    // Acquire the GIL for the main thread
    gil::acquire_gil();

    // Create the main thread state
    thread_state::init_thread_state();
}

/// Finalize the interpreter.
pub fn finalize() {
    gil::release_gil();
}

// ─── C API exports ───

/// Py_Initialize
#[no_mangle]
pub unsafe extern "C" fn Py_Initialize() {
    initialize();
}

/// Py_InitializeEx
#[no_mangle]
pub unsafe extern "C" fn Py_InitializeEx(_initsigs: i32) {
    initialize();
}

/// Py_Finalize
#[no_mangle]
pub unsafe extern "C" fn Py_Finalize() {
    finalize();
}

/// Py_FinalizeEx
#[no_mangle]
pub unsafe extern "C" fn Py_FinalizeEx() -> i32 {
    finalize();
    0
}

/// Py_IsInitialized
#[no_mangle]
pub unsafe extern "C" fn Py_IsInitialized() -> i32 {
    // For now, always return 1 after init
    1
}

/// Py_GetVersion
#[no_mangle]
pub unsafe extern "C" fn Py_GetVersion() -> *const std::os::raw::c_char {
    // Report as CPython 3.11 compatible
    b"3.11.0 (rustthon 0.1.0)\0".as_ptr() as *const std::os::raw::c_char
}

/// Py_GetPlatform
#[no_mangle]
pub unsafe extern "C" fn Py_GetPlatform() -> *const std::os::raw::c_char {
    b"darwin\0".as_ptr() as *const std::os::raw::c_char
}
