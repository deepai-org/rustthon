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

    // For dotted names like "yaml._yaml", split into directory path + leaf name
    let (dir_prefix, leaf_name) = if let Some(pos) = name.rfind('.') {
        let pkg = &name[..pos];
        let leaf = &name[pos + 1..];
        (pkg.replace('.', "/"), leaf.to_string())
    } else {
        (String::new(), name.to_string())
    };

    let suffixes = [
        format!("{}.cpython-311-darwin.so", leaf_name),
        format!("{}.abi3.so", leaf_name),
        format!("{}.so", leaf_name),
        format!("{}.dylib", leaf_name),
    ];

    for search_path in search_paths {
        // Build the directory to search in
        let search_dir = if dir_prefix.is_empty() {
            Path::new(search_path).to_path_buf()
        } else {
            Path::new(search_path).join(&dir_prefix)
        };

        for suffix in &suffixes {
            let path = search_dir.join(suffix);
            if path.exists() {
                return Some(path);
            }
        }

        // Also check for package-style: name/__init__.so etc.
        let pkg_dir = search_dir.join(&leaf_name);
        if pkg_dir.is_dir() {
            let init_suffixes = [
                "__init__.cpython-311-darwin.so".to_string(),
                "__init__.abi3.so".to_string(),
                "__init__.so".to_string(),
                "__init__.dylib".to_string(),
            ];
            for suffix in &init_suffixes {
                let path = pkg_dir.join(suffix);
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
    crate::ffi::panic_guard::guard_ptr("PyImport_ImportModule", || unsafe {
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
            // For dotted names like "yaml._yaml", use the leaf name for PyInit_ lookup
            let leaf_name = name_str.rsplit('.').next().unwrap_or(&name_str);
            match load_extension(&ext_path, leaf_name) {
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
    })
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
    crate::ffi::panic_guard::guard_ptr("PyImport_ImportModuleLevel", || unsafe {
        PyImport_ImportModule(name)
    })
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
    crate::ffi::panic_guard::guard_ptr("PyImport_ImportModuleLevelObject", || unsafe {
        if name.is_null() {
            return ptr::null_mut();
        }
        let name_str = crate::types::unicode::PyUnicode_AsUTF8(name);
        PyImport_ImportModule(name_str)
    })
}

/// PyImport_Import
#[no_mangle]
pub unsafe extern "C" fn PyImport_Import(name: *mut RawPyObject) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyImport_Import", || unsafe {
        if name.is_null() {
            return ptr::null_mut();
        }
        let name_str = crate::types::unicode::PyUnicode_AsUTF8(name);
        PyImport_ImportModule(name_str)
    })
}

/// PyImport_GetModuleDict
#[no_mangle]
pub unsafe extern "C" fn PyImport_GetModuleDict() -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyImport_GetModuleDict", || unsafe {
        crate::module::registry::get_modules_dict()
    })
}

/// PyImport_AddModule - get or create a module in sys.modules
#[no_mangle]
pub unsafe extern "C" fn PyImport_AddModule(name: *const c_char) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyImport_AddModule", || unsafe {
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
        (*name_obj).decref();

        // For Cython runtime modules, pre-populate with expected attributes
        if name_str.ends_with("cython_runtime") || name_str.ends_with(".cython_runtime") {
            let dict = crate::types::moduleobject::PyModule_GetDict(module);
            crate::types::dict::PyDict_SetItemString(
                dict,
                b"cline_in_traceback\0".as_ptr() as *const std::os::raw::c_char,
                crate::types::boolobject::PyBool_FromLong(0),
            );
        }

        crate::module::registry::register_module(&name_str, module);
        module
    })
}

/// _Rustthon_CreateStubType — create a simple type that supports instance creation
/// and setattr. Used by test drivers to pre-register stub types for packages.
///
/// Returns a new type object with:
/// - tp_name = name
/// - tp_base = base (or PyBaseObject_Type if NULL)
/// - tp_dictoffset = 16 (instances have __dict__)
/// - tp_basicsize = 24 (16 for PyObject + 8 for dict ptr)
/// - All standard slots inherited from base
#[no_mangle]
pub unsafe extern "C" fn _Rustthon_CreateStubType(
    name: *const c_char,
    base: *mut RawPyObject,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("_Rustthon_CreateStubType", || unsafe {
        use crate::object::typeobj::{RawPyTypeObject, PyType_Type, PyBaseObject_Type, PyType_Ready};

        let base_tp = if !base.is_null() {
            base as *mut RawPyTypeObject
        } else {
            PyBaseObject_Type.get()
        };

        let tp = libc::calloc(1, std::mem::size_of::<RawPyTypeObject>()) as *mut RawPyTypeObject;
        if tp.is_null() {
            return ptr::null_mut();
        }
        std::ptr::write(tp, RawPyTypeObject::zeroed());

        // Heap-allocate a copy of the name
        if !name.is_null() {
            let name_cstr = CStr::from_ptr(name);
            let name_bytes = name_cstr.to_bytes_with_nul();
            let name_copy = libc::malloc(name_bytes.len()) as *mut u8;
            if !name_copy.is_null() {
                std::ptr::copy_nonoverlapping(name_bytes.as_ptr(), name_copy, name_bytes.len());
                (*tp).tp_name = name_copy as *const c_char;
            }
        }

        (*tp).ob_base.ob_type = PyType_Type.get();
        (*tp).ob_base.ob_refcnt = std::sync::atomic::AtomicIsize::new(1);
        (*tp).tp_basicsize = 24; // 16 (PyObject) + 8 (dict ptr)
        (*tp).tp_dictoffset = 16; // dict pointer at offset 16
        (*tp).tp_flags = crate::object::typeobj::PY_TPFLAGS_DEFAULT
            | crate::object::typeobj::PY_TPFLAGS_BASETYPE;
        (*tp).tp_base = base_tp;

        // Create tp_bases tuple
        let bases = crate::types::tuple::PyTuple_New(1);
        let base_obj = base_tp as *mut RawPyObject;
        (*base_obj).incref();
        crate::types::tuple::PyTuple_SetItem(bases, 0, base_obj);
        (*tp).tp_bases = bases;

        // PyType_Ready will inherit all slots from base
        let ret = PyType_Ready(tp);
        if ret < 0 {
            libc::free(tp as *mut std::ffi::c_void);
            return ptr::null_mut();
        }

        tp as *mut RawPyObject
    })
}

/// PyImport_GetModule — look up a module in sys.modules by name (borrowed reference).
#[no_mangle]
pub unsafe extern "C" fn PyImport_GetModule(name: *mut RawPyObject) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyImport_GetModule", || unsafe {
        if name.is_null() {
            return ptr::null_mut();
        }
        let name_str = crate::types::unicode::PyUnicode_AsUTF8(name);
        if name_str.is_null() {
            return ptr::null_mut();
        }
        let s = CStr::from_ptr(name_str).to_string_lossy();
        match crate::module::registry::get_module(&s) {
            Some(m) => m, // borrowed reference
            None => ptr::null_mut(),
        }
    })
}
