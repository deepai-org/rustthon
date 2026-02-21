//! Python module object.
//!
//! Module objects are what import creates. C extensions create them
//! via PyModule_Create in their PyInit_* function.

use crate::object::pyobject::{PyObjectWithData, RawPyObject};
use crate::object::typeobj::{PyMethodDef, RawPyTypeObject};
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;

static mut MODULE_TYPE: RawPyTypeObject = {
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"module\0".as_ptr() as *const _;
    tp.tp_basicsize = 0;
    tp
};

pub unsafe fn module_type() -> *mut RawPyTypeObject {
    &mut MODULE_TYPE
}

/// Module definition (matches CPython's PyModuleDef)
#[repr(C)]
pub struct PyModuleDef {
    pub m_base: PyModuleDef_Base,
    pub m_name: *const c_char,
    pub m_doc: *const c_char,
    pub m_size: isize,
    pub m_methods: *mut PyMethodDef,
    pub m_slots: *mut PyModuleDef_Slot,
    pub m_traverse: Option<unsafe extern "C" fn(*mut RawPyObject, *mut c_void, *mut c_void) -> c_int>,
    pub m_clear: Option<unsafe extern "C" fn(*mut RawPyObject) -> c_int>,
    pub m_free: Option<unsafe extern "C" fn(*mut c_void)>,
}

#[repr(C)]
pub struct PyModuleDef_Base {
    pub ob_base: RawPyObject,
    pub m_init: Option<unsafe extern "C" fn() -> *mut RawPyObject>,
    pub m_index: isize,
    pub m_copy: *mut RawPyObject,
}

#[repr(C)]
pub struct PyModuleDef_Slot {
    pub slot: c_int,
    pub value: *mut c_void,
}

pub struct ModuleData {
    pub name: String,
    pub dict: *mut RawPyObject, // Module's __dict__
    pub def: *mut PyModuleDef,
}

type PyModuleObject = PyObjectWithData<ModuleData>;

// ─── C API ───

/// PyModule_Create2 - create a module from a PyModuleDef
/// This is what PyModule_Create expands to (with the API version).
#[no_mangle]
pub unsafe extern "C" fn PyModule_Create2(
    def: *mut PyModuleDef,
    _module_api_version: c_int,
) -> *mut RawPyObject {
    if def.is_null() {
        return ptr::null_mut();
    }

    let name = if !(*def).m_name.is_null() {
        std::ffi::CStr::from_ptr((*def).m_name)
            .to_string_lossy()
            .into_owned()
    } else {
        String::from("<unnamed>")
    };

    // Create module dict
    let dict = crate::types::dict::PyDict_New();

    // Set __name__
    let name_obj = crate::types::unicode::create_from_str(&name);
    crate::types::dict::PyDict_SetItemString(dict, b"__name__\0".as_ptr() as *const _, name_obj);
    (*name_obj).decref();

    // Set __doc__
    if !(*def).m_doc.is_null() {
        let doc_obj = crate::types::unicode::PyUnicode_FromString((*def).m_doc);
        crate::types::dict::PyDict_SetItemString(dict, b"__doc__\0".as_ptr() as *const _, doc_obj);
        (*doc_obj).decref();
    }

    let module = PyObjectWithData::alloc(
        &mut MODULE_TYPE,
        ModuleData {
            name,
            dict,
            def,
        },
    );

    // Register methods from the definition
    if !(*def).m_methods.is_null() {
        let mut method_ptr = (*def).m_methods;
        while !(*method_ptr).ml_name.is_null() {
            // Create a PyCFunction object for each method
            let func = crate::types::funcobject::create_cfunction(
                (*method_ptr).ml_name,
                (*method_ptr).ml_meth,
                (*method_ptr).ml_flags,
                module as *mut RawPyObject,
            );
            crate::types::dict::PyDict_SetItemString(
                dict,
                (*method_ptr).ml_name,
                func,
            );
            (*func).decref();
            method_ptr = method_ptr.add(1);
        }
    }

    let result = module as *mut RawPyObject;

    // Register the module for PyState_FindModule
    register_module(def, result);

    result
}

/// PyModule_GetDict - get module's __dict__
#[no_mangle]
pub unsafe extern "C" fn PyModule_GetDict(module: *mut RawPyObject) -> *mut RawPyObject {
    if module.is_null() {
        return ptr::null_mut();
    }
    let data = PyObjectWithData::<ModuleData>::data_from_raw(module);
    data.dict
}

/// PyModule_GetName
#[no_mangle]
pub unsafe extern "C" fn PyModule_GetName(module: *mut RawPyObject) -> *const c_char {
    if module.is_null() {
        return ptr::null();
    }
    let data = PyObjectWithData::<ModuleData>::data_from_raw(module);
    // This is a slight hack - we need a stable pointer to the name
    data.name.as_ptr() as *const c_char
}

/// PyModule_GetNameObject
#[no_mangle]
pub unsafe extern "C" fn PyModule_GetNameObject(module: *mut RawPyObject) -> *mut RawPyObject {
    if module.is_null() {
        return ptr::null_mut();
    }
    let data = PyObjectWithData::<ModuleData>::data_from_raw(module);
    crate::types::unicode::create_from_str(&data.name)
}

/// PyModule_AddObject - add an object to a module
#[no_mangle]
pub unsafe extern "C" fn PyModule_AddObject(
    module: *mut RawPyObject,
    name: *const c_char,
    value: *mut RawPyObject,
) -> c_int {
    if module.is_null() || name.is_null() || value.is_null() {
        return -1;
    }
    let dict = PyModule_GetDict(module);
    // PyModule_AddObject steals a reference on success
    let result = crate::types::dict::PyDict_SetItemString(dict, name, value);
    if result == 0 {
        (*value).decref(); // Steal the reference
    }
    result
}

/// PyModule_AddIntConstant
#[no_mangle]
pub unsafe extern "C" fn PyModule_AddIntConstant(
    module: *mut RawPyObject,
    name: *const c_char,
    value: std::os::raw::c_long,
) -> c_int {
    let obj = crate::types::longobject::PyLong_FromLong(value);
    PyModule_AddObject(module, name, obj)
}

/// PyModule_AddStringConstant
#[no_mangle]
pub unsafe extern "C" fn PyModule_AddStringConstant(
    module: *mut RawPyObject,
    name: *const c_char,
    value: *const c_char,
) -> c_int {
    let obj = crate::types::unicode::PyUnicode_FromString(value);
    PyModule_AddObject(module, name, obj)
}

/// PyModule_Check
#[no_mangle]
pub unsafe extern "C" fn PyModule_Check(obj: *mut RawPyObject) -> c_int {
    if obj.is_null() {
        return 0;
    }
    if (*obj).ob_type == module_type() { 1 } else { 0 }
}

/// PyModule_GetState — get the per-module state (m_size bytes after the module object).
/// For modules with m_size > 0, this returns a pointer to the state block.
/// Our simplified implementation stores state inline after ModuleData.
#[no_mangle]
pub unsafe extern "C" fn PyModule_GetState(module: *mut RawPyObject) -> *mut c_void {
    if module.is_null() {
        return ptr::null_mut();
    }
    let data = PyObjectWithData::<ModuleData>::data_from_raw(module);
    if data.def.is_null() || (*data.def).m_size <= 0 {
        return ptr::null_mut();
    }
    // Return the state block that was allocated after module creation.
    // We store it in a separate allocation pointed to from a static map.
    get_module_state(module)
}

/// PyState_FindModule — find a module by its PyModuleDef.
/// Simplified: we track modules in a global registry.
#[no_mangle]
pub unsafe extern "C" fn PyState_FindModule(def: *mut PyModuleDef) -> *mut RawPyObject {
    let registry = MODULE_REGISTRY.lock();
    let key = def as usize;
    registry.0.get(&key).copied().unwrap_or(ptr::null_mut())
}

// ─── Module registry and state management ───

use std::collections::HashMap;
use parking_lot::Mutex;
use once_cell::sync::Lazy;

struct ModuleRegistryInner(HashMap<usize, *mut RawPyObject>);
unsafe impl Send for ModuleRegistryInner {}

static MODULE_REGISTRY: Lazy<Mutex<ModuleRegistryInner>> =
    Lazy::new(|| Mutex::new(ModuleRegistryInner(HashMap::new())));

struct ModuleStateInner(HashMap<usize, *mut c_void>);
unsafe impl Send for ModuleStateInner {}

static MODULE_STATES: Lazy<Mutex<ModuleStateInner>> =
    Lazy::new(|| Mutex::new(ModuleStateInner(HashMap::new())));

/// Register a module with its def for PyState_FindModule.
unsafe fn register_module(def: *mut PyModuleDef, module: *mut RawPyObject) {
    let key = def as usize;
    MODULE_REGISTRY.lock().0.insert(key, module);

    // Allocate per-module state if m_size > 0
    if !def.is_null() && (*def).m_size > 0 {
        let state = libc::calloc(1, (*def).m_size as usize);
        MODULE_STATES.lock().0.insert(module as usize, state);
    }
}

unsafe fn get_module_state(module: *mut RawPyObject) -> *mut c_void {
    MODULE_STATES.lock().0.get(&(module as usize)).copied().unwrap_or(ptr::null_mut())
}

/// PyModuleDef_Init
#[no_mangle]
pub unsafe extern "C" fn PyModuleDef_Init(def: *mut PyModuleDef) -> *mut RawPyObject {
    // Multi-phase init: return the def itself as a module
    // (simplified - full impl would handle slots)
    PyModule_Create2(def, 1013) // PYTHON_API_VERSION
}
