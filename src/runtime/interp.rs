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

    // Create the builtins module (Cython extensions need this)
    init_builtins();

    // Initialize module search paths
    init_search_paths();
}

/// Create the `builtins` module with exception classes and built-in types.
/// Cython calls `__Pyx_GetBuiltinName` which does
/// `PyObject_GetAttr(builtins_module, name)`.
fn init_builtins() {
    use crate::module::registry::register_module;
    use crate::runtime::error;

    unsafe {
        let name = crate::types::unicode::PyUnicode_FromString(
            b"builtins\0".as_ptr() as *const _,
        );
        let module = crate::types::moduleobject::PyModule_NewObject(name);
        (*name).decref();

        let dict = crate::types::moduleobject::PyModule_GetDict(module);

        // Helper macro to add an exception to builtins
        macro_rules! add_exc {
            ($name:expr, $exc:expr) => {
                crate::types::dict::PyDict_SetItemString(
                    dict,
                    $name.as_ptr() as *const _,
                    *$exc.get(),
                );
            };
        }

        // Exception classes
        add_exc!(b"BaseException\0", error::PyExc_BaseException);
        add_exc!(b"Exception\0", error::PyExc_Exception);
        add_exc!(b"TypeError\0", error::PyExc_TypeError);
        add_exc!(b"ValueError\0", error::PyExc_ValueError);
        add_exc!(b"OverflowError\0", error::PyExc_OverflowError);
        add_exc!(b"RuntimeError\0", error::PyExc_RuntimeError);
        add_exc!(b"KeyError\0", error::PyExc_KeyError);
        add_exc!(b"IndexError\0", error::PyExc_IndexError);
        add_exc!(b"AttributeError\0", error::PyExc_AttributeError);
        add_exc!(b"StopIteration\0", error::PyExc_StopIteration);
        add_exc!(b"MemoryError\0", error::PyExc_MemoryError);
        add_exc!(b"SystemError\0", error::PyExc_SystemError);
        add_exc!(b"OSError\0", error::PyExc_OSError);
        add_exc!(b"IOError\0", error::PyExc_IOError);
        add_exc!(b"NotImplementedError\0", error::PyExc_NotImplementedError);
        add_exc!(b"ImportError\0", error::PyExc_ImportError);
        add_exc!(b"NameError\0", error::PyExc_NameError);
        add_exc!(b"UnboundLocalError\0", error::PyExc_UnboundLocalError);
        add_exc!(b"ZeroDivisionError\0", error::PyExc_ZeroDivisionError);
        add_exc!(b"ModuleNotFoundError\0", error::PyExc_ModuleNotFoundError);
        add_exc!(b"LookupError\0", error::PyExc_LookupError);
        add_exc!(b"ArithmeticError\0", error::PyExc_ArithmeticError);
        add_exc!(b"UnicodeDecodeError\0", error::PyExc_UnicodeDecodeError);
        add_exc!(b"UnicodeEncodeError\0", error::PyExc_UnicodeEncodeError);
        add_exc!(b"UnicodeError\0", error::PyExc_UnicodeError);
        add_exc!(b"Warning\0", error::PyExc_Warning);
        add_exc!(b"DeprecationWarning\0", error::PyExc_DeprecationWarning);
        add_exc!(b"RuntimeWarning\0", error::PyExc_RuntimeWarning);
        add_exc!(b"UserWarning\0", error::PyExc_UserWarning);

        // Built-in type objects
        macro_rules! add_type {
            ($name:expr, $tp:expr) => {
                crate::types::dict::PyDict_SetItemString(
                    dict,
                    $name.as_ptr() as *const _,
                    $tp.get() as *mut crate::object::pyobject::RawPyObject,
                );
            };
        }

        add_type!(b"int\0", crate::types::longobject::PyLong_Type);
        add_type!(b"float\0", crate::types::floatobject::PyFloat_Type);
        add_type!(b"bool\0", crate::types::boolobject::PyBool_Type);
        add_type!(b"str\0", crate::types::unicode::PyUnicode_Type);
        add_type!(b"bytes\0", crate::types::bytes::PyBytes_Type);
        add_type!(b"list\0", crate::types::list::PyList_Type);
        add_type!(b"tuple\0", crate::types::tuple::PyTuple_Type);
        add_type!(b"dict\0", crate::types::dict::PyDict_Type);
        add_type!(b"set\0", crate::types::set::PySet_Type);
        add_type!(b"type\0", crate::object::typeobj::PyType_Type);
        add_type!(b"object\0", crate::object::typeobj::PyBaseObject_Type);

        // Built-in constants
        crate::types::dict::PyDict_SetItemString(
            dict, b"None\0".as_ptr() as *const _, crate::types::none::return_none(),
        );
        crate::types::dict::PyDict_SetItemString(
            dict, b"True\0".as_ptr() as *const _, crate::types::boolobject::PyBool_FromLong(1),
        );
        crate::types::dict::PyDict_SetItemString(
            dict, b"False\0".as_ptr() as *const _, crate::types::boolobject::PyBool_FromLong(0),
        );

        register_module("builtins", module);

        // Create _cython_3_1_4 module that Cython extensions expect
        // Cython looks up `cline_in_traceback` here for debug settings.
        init_cython_runtime();
    }
}

/// Create Cython's internal runtime module.
/// Cython-generated code imports `_cython_3_1_4` and looks up `cline_in_traceback`.
fn init_cython_runtime() {
    unsafe {
        // Create a generic cython runtime module name that covers multiple versions
        for mod_name in &[
            "cython_runtime",
        ] {
            let name = crate::types::unicode::PyUnicode_FromString(
                std::ffi::CString::new(*mod_name).unwrap().as_ptr(),
            );
            let module = crate::types::moduleobject::PyModule_NewObject(name);
            (*name).decref();
            let dict = crate::types::moduleobject::PyModule_GetDict(module);
            // Set cline_in_traceback = False
            crate::types::dict::PyDict_SetItemString(
                dict,
                b"cline_in_traceback\0".as_ptr() as *const _,
                crate::types::boolobject::PyBool_FromLong(0),
            );
            crate::module::registry::register_module(mod_name, module);
        }
    }
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
