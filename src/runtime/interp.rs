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

    // Initialize module search paths
    init_search_paths();
}

/// Set up module search paths from environment and common locations.
fn init_search_paths() {
    use crate::module::registry::add_search_path;

    // PYTHONPATH environment variable
    if let Ok(pythonpath) = std::env::var("PYTHONPATH") {
        for p in pythonpath.split(':') {
            if !p.is_empty() {
                add_search_path(p.to_string());
            }
        }
    }

    // Homebrew Python 3.11 site-packages (macOS arm64)
    let homebrew_paths = [
        "/opt/homebrew/lib/python3.11/site-packages",
        "/opt/homebrew/lib/python3.11/lib-dynload",
    ];
    for p in &homebrew_paths {
        if std::path::Path::new(p).exists() {
            add_search_path(p.to_string());
        }
    }

    // Standard CPython 3.11 locations (system Python)
    let system_paths = [
        "/usr/local/lib/python3.11/site-packages",
        "/usr/local/lib/python3.11/lib-dynload",
        "/Library/Frameworks/Python.framework/Versions/3.11/lib/python3.11/site-packages",
    ];
    for p in &system_paths {
        if std::path::Path::new(p).exists() {
            add_search_path(p.to_string());
        }
    }
}

/// Finalize the interpreter.
pub fn finalize() {
    gil::release_gil();
}

// ─── C API exports ───

/// Py_Initialize
#[no_mangle]
pub unsafe extern "C" fn Py_Initialize() {
    crate::ffi::panic_guard::guard_void("Py_Initialize", || unsafe {
        initialize();
    })
}

/// Py_InitializeEx
#[no_mangle]
pub unsafe extern "C" fn Py_InitializeEx(_initsigs: i32) {
    crate::ffi::panic_guard::guard_void("Py_InitializeEx", || unsafe {
        initialize();
    })
}

/// Py_Finalize
#[no_mangle]
pub unsafe extern "C" fn Py_Finalize() {
    crate::ffi::panic_guard::guard_void("Py_Finalize", || unsafe {
        finalize();
    })
}

/// Py_FinalizeEx
#[no_mangle]
pub unsafe extern "C" fn Py_FinalizeEx() -> i32 {
    crate::ffi::panic_guard::guard_i32("Py_FinalizeEx", || unsafe {
        finalize();
        0
    })
}

/// Py_IsInitialized
#[no_mangle]
pub unsafe extern "C" fn Py_IsInitialized() -> i32 {
    crate::ffi::panic_guard::guard_i32("Py_IsInitialized", || unsafe {
        // For now, always return 1 after init
        1
    })
}

/// Py_GetVersion
#[no_mangle]
pub unsafe extern "C" fn Py_GetVersion() -> *const std::os::raw::c_char {
    crate::ffi::panic_guard::guard_const_ptr("Py_GetVersion", || unsafe {
        // Report as CPython 3.11 compatible
        b"3.11.0 (rustthon 0.1.0)\0".as_ptr() as *const std::os::raw::c_char
    })
}

/// Py_GetPlatform
#[no_mangle]
pub unsafe extern "C" fn Py_GetPlatform() -> *const std::os::raw::c_char {
    crate::ffi::panic_guard::guard_const_ptr("Py_GetPlatform", || unsafe {
        b"darwin\0".as_ptr() as *const std::os::raw::c_char
    })
}

/// Py_Version — CPython 3.11 version as a single integer.
/// Format: (major << 24) | (minor << 16) | (micro << 8) | (level << 4) | serial
/// 3.11.0 final = 0x030B00F0
#[no_mangle]
pub static Py_Version: u32 = 0x030B_00F0;
