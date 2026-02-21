//! C extension loading via dlopen.
//!
//! When someone does `import numpy`, and numpy has a C extension
//! (numpy/core/_multiarray_umath.cpython-311-darwin.so), we need to:
//!
//! 1. Find the .so/.dylib file
//! 2. dlopen() it (macOS Mach-O dynamic loading)
//! 3. Look up the PyInit_<modulename> symbol
//! 4. Call it — it returns a PyObject* (the module)
//!
//! The extension's code will call back into our exported C API functions
//! (PyList_New, PyDict_SetItem, etc.) through the dynamic linker.

use crate::object::pyobject::RawPyObject;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::path::Path;
use std::ptr;

/// Type for the PyInit_* entry point
type PyInitFunc = unsafe extern "C" fn() -> *mut RawPyObject;

/// Load a C extension module from a shared library.
///
/// `path` - path to the .so/.dylib file
/// `name` - the module name (used to look up PyInit_<name>)
pub unsafe fn load_extension(path: &Path, name: &str) -> Result<*mut RawPyObject, String> {
    let path_str = CString::new(path.to_str().ok_or("Invalid path")?)
        .map_err(|e| format!("Invalid path: {}", e))?;

    // dlopen the shared library
    let handle = libc::dlopen(path_str.as_ptr(), libc::RTLD_NOW | libc::RTLD_GLOBAL);
    if handle.is_null() {
        let err = libc::dlerror();
        let err_str = if !err.is_null() {
            CStr::from_ptr(err).to_string_lossy().into_owned()
        } else {
            "unknown dlopen error".to_string()
        };
        return Err(format!("Failed to load {}: {}", path.display(), err_str));
    }

    // Look up PyInit_<name>
    let init_name = CString::new(format!("PyInit_{}", name))
        .map_err(|e| format!("Invalid module name: {}", e))?;

    let init_fn = libc::dlsym(handle, init_name.as_ptr());
    if init_fn.is_null() {
        let err = libc::dlerror();
        let err_str = if !err.is_null() {
            CStr::from_ptr(err).to_string_lossy().into_owned()
        } else {
            "symbol not found".to_string()
        };
        return Err(format!(
            "Module {} has no PyInit_{} function: {}",
            path.display(),
            name,
            err_str
        ));
    }

    // Call the init function
    let init: PyInitFunc = std::mem::transmute(init_fn);
    let module = init();

    if module.is_null() {
        return Err(format!(
            "PyInit_{} returned NULL for {}",
            name,
            path.display()
        ));
    }

    Ok(module)
}

/// Find a C extension file for the given module name.
/// Searches sys.path for files matching CPython's naming convention.
pub fn find_extension(name: &str, search_paths: &[String]) -> Option<std::path::PathBuf> {
    // CPython extension naming conventions on macOS:
    // - <name>.cpython-311-darwin.so
    // - <name>.abi3.so
    // - <name>.so
    // - <name>.dylib
    let suffixes = [
        format!("{}.cpython-311-darwin.so", name),
        format!("{}.abi3.so", name),
        format!("{}.so", name),
        format!("{}.dylib", name),
    ];

    for search_path in search_paths {
        for suffix in &suffixes {
            let path = Path::new(search_path).join(suffix);
            if path.exists() {
                return Some(path);
            }
        }

        // Also check for package-style: name/__init__.so etc.
        let pkg_dir = Path::new(search_path).join(name);
        if pkg_dir.is_dir() {
            for suffix in &suffixes {
                let path = pkg_dir.join(suffix.replace(name, "__init__"));
                if path.exists() {
                    return Some(path);
                }
            }
        }
    }

    None
}

// ─── C API exports for dynamic import ───

/// PyImport_ImportModule
#[no_mangle]
pub unsafe extern "C" fn PyImport_ImportModule(name: *const c_char) -> *mut RawPyObject {
    if name.is_null() {
        return ptr::null_mut();
    }
    let name_str = CStr::from_ptr(name).to_string_lossy();

    // Check if already imported (in module registry)
    if let Some(module) = crate::module::registry::get_module(&name_str) {
        (*module).incref();
        return module;
    }

    // Try to find and load C extension
    let search_paths = crate::module::registry::get_search_paths();
    if let Some(ext_path) = find_extension(&name_str, &search_paths) {
        match load_extension(&ext_path, &name_str) {
            Ok(module) => {
                crate::module::registry::register_module(&name_str, module);
                return module;
            }
            Err(e) => {
                eprintln!("Error loading extension {}: {}", name_str, e);
                return ptr::null_mut();
            }
        }
    }

    // Try to find Python source file
    // TODO: Implement Python source importing via compiler/VM

    ptr::null_mut()
}

/// PyImport_ImportModuleLevel
#[no_mangle]
pub unsafe extern "C" fn PyImport_ImportModuleLevel(
    name: *const c_char,
    _globals: *mut RawPyObject,
    _locals: *mut RawPyObject,
    _fromlist: *mut RawPyObject,
    _level: c_int,
) -> *mut RawPyObject {
    PyImport_ImportModule(name)
}

/// PyImport_ImportModuleLevelObject — import with PyObject name (Cython uses this).
#[no_mangle]
pub unsafe extern "C" fn PyImport_ImportModuleLevelObject(
    name: *mut RawPyObject,
    _globals: *mut RawPyObject,
    _locals: *mut RawPyObject,
    _fromlist: *mut RawPyObject,
    _level: c_int,
) -> *mut RawPyObject {
    if name.is_null() {
        return ptr::null_mut();
    }
    let name_str = crate::types::unicode::PyUnicode_AsUTF8(name);
    PyImport_ImportModule(name_str)
}

/// PyImport_Import
#[no_mangle]
pub unsafe extern "C" fn PyImport_Import(name: *mut RawPyObject) -> *mut RawPyObject {
    if name.is_null() {
        return ptr::null_mut();
    }
    let name_str = crate::types::unicode::PyUnicode_AsUTF8(name);
    PyImport_ImportModule(name_str)
}

/// PyImport_GetModuleDict
#[no_mangle]
pub unsafe extern "C" fn PyImport_GetModuleDict() -> *mut RawPyObject {
    crate::module::registry::get_modules_dict()
}

/// PyImport_AddModule - get or create a module in sys.modules
#[no_mangle]
pub unsafe extern "C" fn PyImport_AddModule(name: *const c_char) -> *mut RawPyObject {
    if name.is_null() {
        return ptr::null_mut();
    }
    let name_str = CStr::from_ptr(name).to_string_lossy();

    // Check if exists
    if let Some(module) = crate::module::registry::get_module(&name_str) {
        return module; // Borrowed reference
    }

    // Create proper module object (not a dict!)
    let name_obj = crate::types::unicode::PyUnicode_FromString(name);
    let module = crate::types::moduleobject::PyModule_NewObject(name_obj);
    crate::module::registry::register_module(&name_str, module);
    (*name_obj).decref();
    module
}
