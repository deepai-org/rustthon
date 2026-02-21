//! Module registry — tracks imported modules (sys.modules equivalent).

use crate::object::pyobject::RawPyObject;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::ptr;

/// Global module registry
static MODULE_REGISTRY: Mutex<Option<ModuleRegistry>> = Mutex::new(None);

struct ModuleRegistry {
    /// Mapping of module name -> PyObject* (module object)
    modules: HashMap<String, *mut RawPyObject>,
    /// sys.path equivalent
    search_paths: Vec<String>,
    /// The sys.modules dict (for C API compatibility)
    modules_dict: *mut RawPyObject,
}

unsafe impl Send for ModuleRegistry {}

impl ModuleRegistry {
    fn new() -> Self {
        let modules_dict = unsafe { crate::types::dict::PyDict_New() };
        ModuleRegistry {
            modules: HashMap::new(),
            search_paths: vec![
                ".".to_string(),
                "/usr/local/lib/python3.11/lib-dynload".to_string(),
                "/usr/local/lib/python3.11/site-packages".to_string(),
            ],
            modules_dict,
        }
    }
}

fn with_registry<F, R>(f: F) -> R
where
    F: FnOnce(&mut ModuleRegistry) -> R,
{
    let mut guard = MODULE_REGISTRY.lock();
    if guard.is_none() {
        *guard = Some(ModuleRegistry::new());
    }
    f(guard.as_mut().unwrap())
}

/// Register a module in the registry.
pub fn register_module(name: &str, module: *mut RawPyObject) {
    with_registry(|reg| {
        unsafe {
            (*module).incref();
        }
        reg.modules.insert(name.to_string(), module);
        // Also add to the dict for C API
        unsafe {
            let name_cstr = std::ffi::CString::new(name).unwrap();
            crate::types::dict::PyDict_SetItemString(
                reg.modules_dict,
                name_cstr.as_ptr(),
                module,
            );
        }
    });
}

/// Get a module from the registry (borrowed reference).
pub fn get_module(name: &str) -> Option<*mut RawPyObject> {
    with_registry(|reg| reg.modules.get(name).copied())
}

/// Get the modules dict (sys.modules).
pub fn get_modules_dict() -> *mut RawPyObject {
    with_registry(|reg| reg.modules_dict)
}

/// Get current search paths.
pub fn get_search_paths() -> Vec<String> {
    with_registry(|reg| reg.search_paths.clone())
}

/// Add a search path.
pub fn add_search_path(path: String) {
    with_registry(|reg| {
        if !reg.search_paths.contains(&path) {
            reg.search_paths.push(path);
        }
    });
}
