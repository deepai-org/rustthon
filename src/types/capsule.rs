//! PyCapsule — opaque container for C pointers.
//!
//! Extensions use capsules to share C-level pointers (function tables,
//! struct pointers) between modules without exposing them to Python.

use crate::object::pyobject::RawPyObject;
use crate::object::typeobj::RawPyTypeObject;
use crate::object::SyncUnsafeCell;
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;
use std::sync::atomic::AtomicIsize;

/// Destructor for capsule cleanup.
pub type PyCapsule_Destructor = unsafe extern "C" fn(*mut RawPyObject);

/// Internal capsule storage.
#[repr(C)]
struct PyCapsuleObject {
    ob_refcnt: AtomicIsize,
    ob_type: *mut RawPyTypeObject,
    pointer: *mut c_void,
    name: *const c_char,
    context: *mut c_void,
    destructor: Option<PyCapsule_Destructor>,
}

static PYCAPSULE_TYPE: SyncUnsafeCell<RawPyTypeObject> = SyncUnsafeCell::new({
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"PyCapsule\0".as_ptr() as *const _;
    tp.tp_basicsize = std::mem::size_of::<PyCapsuleObject>() as isize;
    tp
});

pub fn capsule_type() -> *mut RawPyTypeObject {
    PYCAPSULE_TYPE.get()
}

/// PyCapsule_New — create a new capsule wrapping a C pointer.
#[no_mangle]
pub unsafe extern "C" fn PyCapsule_New(
    pointer: *mut c_void,
    name: *const c_char,
    destructor: Option<PyCapsule_Destructor>,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyCapsule_New", || unsafe {
        if pointer.is_null() {
            return ptr::null_mut();
        }
        let cap = libc::calloc(1, std::mem::size_of::<PyCapsuleObject>()) as *mut PyCapsuleObject;
        if cap.is_null() {
            return ptr::null_mut();
        }
        std::ptr::write(&mut (*cap).ob_refcnt, AtomicIsize::new(1));
        (*cap).ob_type = PYCAPSULE_TYPE.get();
        (*cap).pointer = pointer;
        (*cap).name = name;
        (*cap).context = ptr::null_mut();
        (*cap).destructor = destructor;
        cap as *mut RawPyObject
    })
}

/// PyCapsule_GetPointer — retrieve the C pointer from a capsule.
/// If `name` is non-NULL, checks that it matches the capsule's name.
#[no_mangle]
pub unsafe extern "C" fn PyCapsule_GetPointer(
    capsule: *mut RawPyObject,
    name: *const c_char,
) -> *mut c_void {
    crate::ffi::panic_guard::guard_ptr("PyCapsule_GetPointer", || unsafe {
        if capsule.is_null() {
            return ptr::null_mut();
        }
        let cap = capsule as *mut PyCapsuleObject;
        // Verify it's actually a capsule
        if (*cap).ob_type != PYCAPSULE_TYPE.get() {
            return ptr::null_mut();
        }
        // Name check
        if !name.is_null() && !(*cap).name.is_null() {
            if libc::strcmp(name, (*cap).name) != 0 {
                return ptr::null_mut();
            }
        }
        (*cap).pointer
    })
}

/// PyCapsule_GetName — get the name of a capsule.
#[no_mangle]
pub unsafe extern "C" fn PyCapsule_GetName(
    capsule: *mut RawPyObject,
) -> *const c_char {
    crate::ffi::panic_guard::guard_const_ptr("PyCapsule_GetName", || unsafe {
        if capsule.is_null() {
            return ptr::null();
        }
        let cap = capsule as *mut PyCapsuleObject;
        (*cap).name
    })
}

/// PyCapsule_GetContext — get the context of a capsule.
#[no_mangle]
pub unsafe extern "C" fn PyCapsule_GetContext(
    capsule: *mut RawPyObject,
) -> *mut c_void {
    crate::ffi::panic_guard::guard_ptr("PyCapsule_GetContext", || unsafe {
        if capsule.is_null() {
            return ptr::null_mut();
        }
        let cap = capsule as *mut PyCapsuleObject;
        (*cap).context
    })
}

/// PyCapsule_SetDestructor — set the destructor for a capsule.
#[no_mangle]
pub unsafe extern "C" fn PyCapsule_SetDestructor(
    capsule: *mut RawPyObject,
    destructor: Option<PyCapsule_Destructor>,
) -> c_int {
    crate::ffi::panic_guard::guard_int("PyCapsule_SetDestructor", || unsafe {
        if capsule.is_null() {
            return -1;
        }
        let cap = capsule as *mut PyCapsuleObject;
        (*cap).destructor = destructor;
        0
    })
}

/// PyCapsule_SetName — set the name of a capsule.
#[no_mangle]
pub unsafe extern "C" fn PyCapsule_SetName(
    capsule: *mut RawPyObject,
    name: *const c_char,
) -> c_int {
    crate::ffi::panic_guard::guard_int("PyCapsule_SetName", || unsafe {
        if capsule.is_null() {
            return -1;
        }
        let cap = capsule as *mut PyCapsuleObject;
        (*cap).name = name;
        0
    })
}

/// PyCapsule_IsValid — check if a capsule is valid.
#[no_mangle]
pub unsafe extern "C" fn PyCapsule_IsValid(
    capsule: *mut RawPyObject,
    name: *const c_char,
) -> c_int {
    crate::ffi::panic_guard::guard_int("PyCapsule_IsValid", || unsafe {
        if capsule.is_null() {
            return 0;
        }
        let cap = capsule as *mut PyCapsuleObject;
        if (*cap).ob_type != PYCAPSULE_TYPE.get() {
            return 0;
        }
        if (*cap).pointer.is_null() {
            return 0;
        }
        if !name.is_null() && !(*cap).name.is_null() {
            if libc::strcmp(name, (*cap).name) != 0 {
                return 0;
            }
        }
        1
    })
}

/// PyCapsule_Import — import a capsule from a module by dotted name.
#[no_mangle]
pub unsafe extern "C" fn PyCapsule_Import(
    name: *const c_char,
    _no_block: c_int,
) -> *mut c_void {
    crate::ffi::panic_guard::guard_ptr("PyCapsule_Import", || unsafe {
        if name.is_null() {
            return ptr::null_mut();
        }
        let name_str = std::ffi::CStr::from_ptr(name).to_string_lossy();
        // Split at last '.' to get module.attr
        if let Some(dot_pos) = name_str.rfind('.') {
            let mod_name = &name_str[..dot_pos];
            let attr_name = &name_str[dot_pos + 1..];
            let mod_cstr = std::ffi::CString::new(mod_name).unwrap();
            let module = crate::ffi::import::PyImport_ImportModule(mod_cstr.as_ptr());
            if module.is_null() {
                return ptr::null_mut();
            }
            let attr_cstr = std::ffi::CString::new(attr_name).unwrap();
            let capsule = crate::ffi::object_api::PyObject_GetAttrString(module, attr_cstr.as_ptr());
            (*module).decref();
            if capsule.is_null() {
                return ptr::null_mut();
            }
            let pointer = PyCapsule_GetPointer(capsule, name);
            (*capsule).decref();
            pointer
        } else {
            ptr::null_mut()
        }
    })
}
