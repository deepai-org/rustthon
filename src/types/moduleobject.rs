//! Python module object.
//!
//! Module objects are what import creates. C extensions create them
//! via PyModule_Create in their PyInit_* function.

use crate::object::pyobject::{PyObjectWithData, RawPyObject};
use crate::object::typeobj::{PyMethodDef, RawPyTypeObject};
use crate::object::SyncUnsafeCell;
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;

static MODULE_TYPE: SyncUnsafeCell<RawPyTypeObject> = SyncUnsafeCell::new({
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"module\0".as_ptr() as *const _;
    tp.tp_basicsize = 0;
    tp.tp_getattro = Some(module_getattro);
    tp.tp_setattro = Some(module_setattro);
    tp
});

pub fn module_type() -> *mut RawPyTypeObject {
    MODULE_TYPE.get()
}

/// module_getattro — lookup attribute in module __dict__
unsafe extern "C" fn module_getattro(
    obj: *mut RawPyObject,
    name: *mut RawPyObject,
) -> *mut RawPyObject {
    let dict = PyModule_GetDict(obj);
    if dict.is_null() {
        return ptr::null_mut();
    }
    let result = crate::types::dict::PyDict_GetItem(dict, name);
    if !result.is_null() {
        (*result).incref();
        return result;
    }
    // Attribute not found — set AttributeError with useful message
    let name_s = crate::types::unicode::PyUnicode_AsUTF8(name);
    let attr = if !name_s.is_null() {
        std::ffi::CStr::from_ptr(name_s).to_string_lossy()
    } else {
        std::borrow::Cow::Borrowed("(null)")
    };
    let data = PyObjectWithData::<ModuleData>::data_from_raw(obj);
    let msg = format!("module '{}' has no attribute '{}'\0", data.name, attr);
    crate::runtime::error::PyErr_SetString(
        *crate::runtime::error::PyExc_AttributeError.get(),
        msg.as_ptr() as *const _,
    );
    ptr::null_mut()
}

/// module_setattro — set attribute in module __dict__
unsafe extern "C" fn module_setattro(
    obj: *mut RawPyObject,
    name: *mut RawPyObject,
    value: *mut RawPyObject,
) -> c_int {
    let dict = PyModule_GetDict(obj);
    if dict.is_null() {
        return -1;
    }
    if value.is_null() {
        // Delete attribute
        return crate::types::dict::PyDict_DelItem(dict, name);
    }
    crate::types::dict::PyDict_SetItem(dict, name, value)
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
    crate::ffi::panic_guard::guard_ptr("PyModule_Create2", || unsafe {
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
            MODULE_TYPE.get(),
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
    })
}

/// PyModule_GetDict - get module's __dict__
#[no_mangle]
pub unsafe extern "C" fn PyModule_GetDict(module: *mut RawPyObject) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyModule_GetDict", || unsafe {
        if module.is_null() {
            return ptr::null_mut();
        }
        let data = PyObjectWithData::<ModuleData>::data_from_raw(module);
        data.dict
    })
}

/// PyModule_GetName
#[no_mangle]
pub unsafe extern "C" fn PyModule_GetName(module: *mut RawPyObject) -> *const c_char {
    crate::ffi::panic_guard::guard_const_ptr("PyModule_GetName", || unsafe {
        if module.is_null() {
            return ptr::null();
        }
        let data = PyObjectWithData::<ModuleData>::data_from_raw(module);
        // This is a slight hack - we need a stable pointer to the name
        data.name.as_ptr() as *const c_char
    })
}

/// PyModule_GetNameObject
#[no_mangle]
pub unsafe extern "C" fn PyModule_GetNameObject(module: *mut RawPyObject) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyModule_GetNameObject", || unsafe {
        if module.is_null() {
            return ptr::null_mut();
        }
        let data = PyObjectWithData::<ModuleData>::data_from_raw(module);
        crate::types::unicode::create_from_str(&data.name)
    })
}

/// PyModule_AddObject - add an object to a module
#[no_mangle]
pub unsafe extern "C" fn PyModule_AddObject(
    module: *mut RawPyObject,
    name: *const c_char,
    value: *mut RawPyObject,
) -> c_int {
    crate::ffi::panic_guard::guard_int("PyModule_AddObject", || unsafe {
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
    })
}

/// PyModule_AddIntConstant
#[no_mangle]
pub unsafe extern "C" fn PyModule_AddIntConstant(
    module: *mut RawPyObject,
    name: *const c_char,
    value: std::os::raw::c_long,
) -> c_int {
    crate::ffi::panic_guard::guard_int("PyModule_AddIntConstant", || unsafe {
        let obj = crate::types::longobject::PyLong_FromLong(value);
        PyModule_AddObject(module, name, obj)
    })
}

/// PyModule_AddStringConstant
#[no_mangle]
pub unsafe extern "C" fn PyModule_AddStringConstant(
    module: *mut RawPyObject,
    name: *const c_char,
    value: *const c_char,
) -> c_int {
    crate::ffi::panic_guard::guard_int("PyModule_AddStringConstant", || unsafe {
        let obj = crate::types::unicode::PyUnicode_FromString(value);
        PyModule_AddObject(module, name, obj)
    })
}

/// PyModule_Check
#[no_mangle]
pub unsafe extern "C" fn PyModule_Check(obj: *mut RawPyObject) -> c_int {
    crate::ffi::panic_guard::guard_int("PyModule_Check", || unsafe {
        if obj.is_null() {
            return 0;
        }
        if (*obj).ob_type == module_type() { 1 } else { 0 }
    })
}

/// PyModule_GetState — get the per-module state (m_size bytes after the module object).
/// For modules with m_size > 0, this returns a pointer to the state block.
/// Our simplified implementation stores state inline after ModuleData.
#[no_mangle]
pub unsafe extern "C" fn PyModule_GetState(module: *mut RawPyObject) -> *mut c_void {
    crate::ffi::panic_guard::guard_ptr("PyModule_GetState", || unsafe {
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
    })
}

/// PyState_FindModule — find a module by its PyModuleDef.
/// Simplified: we track modules in a global registry.
#[no_mangle]
pub unsafe extern "C" fn PyState_FindModule(def: *mut PyModuleDef) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyState_FindModule", || unsafe {
        let registry = MODULE_REGISTRY.lock();
        let key = def as usize;
        registry.0.get(&key).copied().unwrap_or(ptr::null_mut())
    })
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

/// PyModule_NewObject — create a new empty module with a PyObject name.
#[no_mangle]
pub unsafe extern "C" fn PyModule_NewObject(name: *mut RawPyObject) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyModule_NewObject", || unsafe {
        if name.is_null() {
            return ptr::null_mut();
        }
        let name_str = crate::types::unicode::unicode_value(name).to_string();
        let dict = crate::types::dict::PyDict_New();

        // Set __name__
        (*name).incref();
        crate::types::dict::PyDict_SetItemString(dict, b"__name__\0".as_ptr() as *const _, name);

        let module = PyObjectWithData::alloc(
            MODULE_TYPE.get(),
            ModuleData {
                name: name_str,
                dict,
                def: ptr::null_mut(),
            },
        );
        module as *mut RawPyObject
    })
}

// PEP 489 multi-phase init slot IDs
const PY_MOD_CREATE: c_int = 1;
const PY_MOD_EXEC: c_int = 2;

/// Type for Py_mod_exec callback: int (*)(PyObject *module)
type PyModExecFunc = unsafe extern "C" fn(*mut RawPyObject) -> c_int;

/// PyModuleDef_Init — PEP 489 multi-phase module initialization.
///
/// For multi-phase init (Cython, modern extensions), we:
/// 1. Create the module via PyModule_Create2 (skipping the Py_mod_create callback
///    which expects a ModuleSpec we don't have)
/// 2. Call the Py_mod_exec callback to populate the module with functions
///
/// This approach works because the Py_mod_create callback in Cython just calls
/// PyModule_NewObject anyway — we accomplish the same thing directly.
#[no_mangle]
pub unsafe extern "C" fn PyModuleDef_Init(def: *mut PyModuleDef) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyModuleDef_Init", || unsafe {
        if def.is_null() {
            return ptr::null_mut();
        }

        // Always create module via PyModule_Create2 first
        let module = PyModule_Create2(def, 1013);
        if module.is_null() {
            return ptr::null_mut();
        }

        // If multi-phase init slots exist, find and call Py_mod_exec
        if !(*def).m_slots.is_null() {
            let mut slot = (*def).m_slots;
            while (*slot).slot != 0 || !(*slot).value.is_null() {
                if (*slot).slot == 0 && (*slot).value.is_null() {
                    break;
                }
                if (*slot).slot == PY_MOD_EXEC && !(*slot).value.is_null() {
                    let exec_func: PyModExecFunc = std::mem::transmute((*slot).value);
                    // Clear any stale error before calling exec
                    crate::runtime::error::PyErr_Clear();
                    let result = exec_func(module);
                    if result != 0 {
                        return ptr::null_mut();
                    }
                }
                slot = slot.add(1);
            }
        }

        module
    })
}
