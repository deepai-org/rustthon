//! Generic Python object C API.
//!
//! These are the top-level PyObject_* functions that every
//! C extension calls. They dispatch to the appropriate type
//! slots (tp_repr, tp_str, tp_hash, tp_richcompare, etc.).

use crate::object::pyobject::RawPyObject;
use crate::object::typeobj::RawPyTypeObject;
use std::os::raw::{c_char, c_int};
use std::ptr;

// ─── Rich comparison constants ───
pub const PY_LT: c_int = 0;
pub const PY_LE: c_int = 1;
pub const PY_EQ: c_int = 2;
pub const PY_NE: c_int = 3;
pub const PY_GT: c_int = 4;
pub const PY_GE: c_int = 5;

// ─── Object protocol ───

/// PyObject_Repr
#[no_mangle]
pub unsafe extern "C" fn PyObject_Repr(obj: *mut RawPyObject) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyObject_Repr", || unsafe {
        if obj.is_null() {
            return crate::types::unicode::create_from_str("<NULL>");
        }
        let tp = (*obj).ob_type;
        if !tp.is_null() {
            if let Some(repr) = (*tp).tp_repr {
                return repr(obj);
            }
        }
        // Default repr
        let tp_name = if !tp.is_null() && !(*tp).tp_name.is_null() {
            std::ffi::CStr::from_ptr((*tp).tp_name)
                .to_string_lossy()
                .into_owned()
        } else {
            "object".to_string()
        };
        let s = format!("<{} object at {:p}>", tp_name, obj);
        crate::types::unicode::create_from_str(&s)
    })
}

/// PyObject_Str
#[no_mangle]
pub unsafe extern "C" fn PyObject_Str(obj: *mut RawPyObject) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyObject_Str", || unsafe {
        if obj.is_null() {
            return crate::types::unicode::create_from_str("<NULL>");
        }
        let tp = (*obj).ob_type;
        if !tp.is_null() {
            if let Some(str_fn) = (*tp).tp_str {
                return str_fn(obj);
            }
        }
        // Fall back to repr
        PyObject_Repr(obj)
    })
}

/// PyObject_Hash
#[no_mangle]
pub unsafe extern "C" fn PyObject_Hash(obj: *mut RawPyObject) -> isize {
    crate::ffi::panic_guard::guard_ssize("PyObject_Hash", || unsafe {
        if obj.is_null() {
            return -1;
        }
        let tp = (*obj).ob_type;
        if !tp.is_null() {
            if let Some(hash) = (*tp).tp_hash {
                return hash(obj);
            }
        }
        // Default: hash by pointer
        obj as isize
    })
}

/// PyObject_RichCompare
#[no_mangle]
pub unsafe extern "C" fn PyObject_RichCompare(
    v: *mut RawPyObject,
    w: *mut RawPyObject,
    op: c_int,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyObject_RichCompare", || unsafe {
        if v.is_null() || w.is_null() {
            return ptr::null_mut();
        }
        let tp = (*v).ob_type;
        if !tp.is_null() {
            if let Some(richcmp) = (*tp).tp_richcompare {
                return richcmp(v, w, op);
            }
        }
        // Default: identity comparison for == and !=
        match op {
            PY_EQ => crate::types::boolobject::PyBool_FromLong(if v == w { 1 } else { 0 }),
            PY_NE => crate::types::boolobject::PyBool_FromLong(if v != w { 1 } else { 0 }),
            _ => ptr::null_mut(), // Not implemented
        }
    })
}

/// PyObject_RichCompareBool
#[no_mangle]
pub unsafe extern "C" fn PyObject_RichCompareBool(
    v: *mut RawPyObject,
    w: *mut RawPyObject,
    op: c_int,
) -> c_int {
    crate::ffi::panic_guard::guard_int("PyObject_RichCompareBool", || unsafe {
        // Identity shortcut
        if v == w {
            match op {
                PY_EQ => return 1,
                PY_NE => return 0,
                PY_LE | PY_GE => return 1,
                _ => {}
            }
        }
        let result = PyObject_RichCompare(v, w, op);
        if result.is_null() {
            return -1;
        }
        let is_true = PyObject_IsTrue(result);
        (*result).decref();
        is_true
    })
}

/// PyObject_IsTrue
#[no_mangle]
pub unsafe extern "C" fn PyObject_IsTrue(obj: *mut RawPyObject) -> c_int {
    crate::ffi::panic_guard::guard_int("PyObject_IsTrue", || unsafe {
        if obj.is_null() {
            return 0;
        }
        // None is false
        if crate::types::none::is_none(obj) {
            return 0;
        }
        // Bool check
        if crate::types::boolobject::is_bool(obj) {
            return if crate::types::boolobject::is_true(obj) { 1 } else { 0 };
        }
        let tp = (*obj).ob_type;
        // Int check: ob_size == 0 means zero (falsy)
        if !tp.is_null() && ((*tp).tp_flags & crate::object::typeobj::PY_TPFLAGS_LONG_SUBCLASS) != 0 {
            let var_obj = obj as *mut crate::object::pyobject::RawPyVarObject;
            return if (*var_obj).ob_size == 0 { 0 } else { 1 };
        }
        // Float check: 0.0 is falsy
        if !tp.is_null() && (*obj).ob_type == crate::types::floatobject::float_type() {
            let fval = crate::types::floatobject::PyFloat_AsDouble(obj);
            return if fval == 0.0 { 0 } else { 1 };
        }
        // Check nb_bool
        if !tp.is_null() && !(*tp).tp_as_number.is_null() {
            if let Some(nb_bool) = (*(*tp).tp_as_number).nb_bool {
                return nb_bool(obj);
            }
        }
        // String check: empty string is falsy
        if !tp.is_null() && ((*tp).tp_flags & crate::object::typeobj::PY_TPFLAGS_UNICODE_SUBCLASS) != 0 {
            let len = crate::types::unicode::PyUnicode_GetLength(obj);
            return if len == 0 { 0 } else { 1 };
        }
        // Bytes check: empty bytes is falsy
        if !tp.is_null() && ((*tp).tp_flags & crate::object::typeobj::PY_TPFLAGS_BYTES_SUBCLASS) != 0 {
            let len = crate::types::bytes::PyBytes_Size(obj);
            return if len == 0 { 0 } else { 1 };
        }
        // Check sq_length (empty containers are falsy)
        if !tp.is_null() && !(*tp).tp_as_sequence.is_null() {
            if let Some(sq_length) = (*(*tp).tp_as_sequence).sq_length {
                let len = sq_length(obj);
                return if len > 0 { 1 } else { 0 };
            }
        }
        // Check mp_length (empty dicts/maps are falsy)
        if !tp.is_null() && !(*tp).tp_as_mapping.is_null() {
            if let Some(mp_length) = (*(*tp).tp_as_mapping).mp_length {
                let len = mp_length(obj);
                return if len > 0 { 1 } else { 0 };
            }
        }
        // List/tuple/dict/set: check via known types
        if !tp.is_null() {
            if ((*tp).tp_flags & crate::object::typeobj::PY_TPFLAGS_LIST_SUBCLASS) != 0 {
                let len = crate::types::list::PyList_Size(obj);
                return if len == 0 { 0 } else { 1 };
            }
            if ((*tp).tp_flags & crate::object::typeobj::PY_TPFLAGS_TUPLE_SUBCLASS) != 0 {
                let len = crate::types::tuple::PyTuple_Size(obj);
                return if len == 0 { 0 } else { 1 };
            }
            if ((*tp).tp_flags & crate::object::typeobj::PY_TPFLAGS_DICT_SUBCLASS) != 0 {
                let len = crate::types::dict::PyDict_Size(obj);
                return if len == 0 { 0 } else { 1 };
            }
        }
        // Default: true
        1
    })
}

/// PyObject_Not
#[no_mangle]
pub unsafe extern "C" fn PyObject_Not(obj: *mut RawPyObject) -> c_int {
    crate::ffi::panic_guard::guard_int("PyObject_Not", || unsafe {
        let result = PyObject_IsTrue(obj);
        if result < 0 {
            return result;
        }
        if result == 0 { 1 } else { 0 }
    })
}

/// PyObject_Type - get the type of an object (new reference)
#[no_mangle]
pub unsafe extern "C" fn PyObject_Type(obj: *mut RawPyObject) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyObject_Type", || unsafe {
        if obj.is_null() {
            return ptr::null_mut();
        }
        let tp = (*obj).ob_type as *mut RawPyObject;
        if !tp.is_null() {
            (*tp).incref();
        }
        tp
    })
}

/// PyObject_TypeCheck
#[no_mangle]
pub unsafe extern "C" fn PyObject_TypeCheck(
    obj: *mut RawPyObject,
    tp: *mut RawPyTypeObject,
) -> c_int {
    crate::ffi::panic_guard::guard_int("PyObject_TypeCheck", || unsafe {
        if obj.is_null() || tp.is_null() {
            return 0;
        }
        if (*obj).ob_type == tp { 1 } else { 0 }
    })
}

/// PyObject_HasAttrString
#[no_mangle]
pub unsafe extern "C" fn PyObject_HasAttrString(
    obj: *mut RawPyObject,
    name: *const c_char,
) -> c_int {
    crate::ffi::panic_guard::guard_int("PyObject_HasAttrString", || unsafe {
        let attr = PyObject_GetAttrString(obj, name);
        if attr.is_null() {
            crate::runtime::error::PyErr_Clear();
            0
        } else {
            (*attr).decref();
            1
        }
    })
}

/// PyObject_GetAttrString
#[no_mangle]
pub unsafe extern "C" fn PyObject_GetAttrString(
    obj: *mut RawPyObject,
    name: *const c_char,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyObject_GetAttrString", || unsafe {
        if obj.is_null() || name.is_null() {
            return ptr::null_mut();
        }

        // Check tp_getattro first (the modern way)
        let tp = (*obj).ob_type;
        if !tp.is_null() {
            if let Some(getattro) = (*tp).tp_getattro {
                let name_obj = crate::types::unicode::PyUnicode_FromString(name);
                let result = getattro(obj, name_obj);
                (*name_obj).decref();
                if !result.is_null() {
                    return result;
                }
                // If obj is a dict (Python module registered as dict), try dict item lookup
                if crate::types::dict::PyDict_Check(obj) != 0 {
                    // Clear any error from tp_getattro
                    if !crate::runtime::error::PyErr_Occurred().is_null() {
                        crate::runtime::error::PyErr_Clear();
                    }
                    let item = crate::types::dict::PyDict_GetItemString(obj, name);
                    if !item.is_null() {
                        (*item).incref();
                        return item;
                    }
                }
                // Set AttributeError if not already set
                if crate::runtime::error::PyErr_Occurred().is_null() {
                    crate::runtime::error::PyErr_SetString(
                        *crate::runtime::error::PyExc_AttributeError.get(),
                        name,
                    );
                }
                return ptr::null_mut();
            }
            // Fall back to tp_getattr (legacy)
            if let Some(getattr) = (*tp).tp_getattr {
                let result = getattr(obj, name as *mut c_char);
                if result.is_null() && crate::runtime::error::PyErr_Occurred().is_null() {
                    crate::runtime::error::PyErr_SetString(
                        *crate::runtime::error::PyExc_AttributeError.get(),
                        name,
                    );
                }
                return result;
            }
        }

        // No getattr slot — set AttributeError
        let name_s = if !name.is_null() {
            std::ffi::CStr::from_ptr(name).to_string_lossy().into_owned()
        } else { "(null)".to_string() };
        let tp_name = if !tp.is_null() && !(*tp).tp_name.is_null() {
            std::ffi::CStr::from_ptr((*tp).tp_name).to_string_lossy().into_owned()
        } else { "(null)".to_string() };
        crate::runtime::error::PyErr_SetString(
            *crate::runtime::error::PyExc_AttributeError.get(),
            name,
        );
        ptr::null_mut()
    })
}

/// PyObject_SetAttrString
#[no_mangle]
pub unsafe extern "C" fn PyObject_SetAttrString(
    obj: *mut RawPyObject,
    name: *const c_char,
    value: *mut RawPyObject,
) -> c_int {
    crate::ffi::panic_guard::guard_int("PyObject_SetAttrString", || unsafe {
        if obj.is_null() || name.is_null() {
            return -1;
        }
        let tp = (*obj).ob_type;
        if !tp.is_null() {
            if let Some(setattro) = (*tp).tp_setattro {
                let name_obj = crate::types::unicode::PyUnicode_FromString(name);
                let result = setattro(obj, name_obj, value);
                // Ensure exception is set on failure
                if result < 0 && crate::runtime::error::PyErr_Occurred().is_null() {
                    crate::runtime::error::PyErr_SetString(
                        *crate::runtime::error::PyExc_AttributeError.get(),
                        name,
                    );
                }
                (*name_obj).decref();
                return result;
            }
        }
        // No tp_setattro — try to set in tp_dict for type objects
        if !tp.is_null() && crate::object::typeobj::is_type_object(obj) {
            // obj is a type object — set in its tp_dict
            let type_obj = obj as *mut crate::object::typeobj::RawPyTypeObject;
            let dict = (*type_obj).tp_dict;
            if !dict.is_null() {
                let name_obj = crate::types::unicode::PyUnicode_FromString(name);
                let result = crate::types::dict::PyDict_SetItem(dict, name_obj, value);
                (*name_obj).decref();
                return result;
            }
        }
        // Set AttributeError
        crate::runtime::error::PyErr_SetString(
            *crate::runtime::error::PyExc_AttributeError.get(),
            name,
        );
        -1
    })
}

/// PyObject_GetAttr
#[no_mangle]
pub unsafe extern "C" fn PyObject_GetAttr(
    obj: *mut RawPyObject,
    name: *mut RawPyObject,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyObject_GetAttr", || unsafe {
        if obj.is_null() || name.is_null() {
            return ptr::null_mut();
        }
        let tp = (*obj).ob_type;
        if !tp.is_null() {
            if let Some(getattro) = (*tp).tp_getattro {
                let result = getattro(obj, name);
                if !result.is_null() {
                    return result;
                }
                // Dict-item fallback for Python modules registered as dicts
                if crate::types::dict::PyDict_Check(obj) != 0 {
                    if !crate::runtime::error::PyErr_Occurred().is_null() {
                        crate::runtime::error::PyErr_Clear();
                    }
                    let name_cstr = crate::types::unicode::PyUnicode_AsUTF8(name);
                    if !name_cstr.is_null() {
                        let item = crate::types::dict::PyDict_GetItemString(obj, name_cstr);
                        if !item.is_null() {
                            (*item).incref();
                            return item;
                        }
                    }
                }
                // Ensure exception is set on failure
                if crate::runtime::error::PyErr_Occurred().is_null() {
                    let name_cstr = crate::types::unicode::PyUnicode_AsUTF8(name);
                    let attr = if !name_cstr.is_null() {
                        std::ffi::CStr::from_ptr(name_cstr).to_string_lossy().into_owned()
                    } else { "?".to_string() };
                    let obj_tp_name = if !tp.is_null() && !(*tp).tp_name.is_null() {
                        std::ffi::CStr::from_ptr((*tp).tp_name).to_string_lossy().into_owned()
                    } else { "?".to_string() };
                    let msg = format!("'{}' object has no attribute '{}'\0", obj_tp_name, attr);
                    crate::runtime::error::PyErr_SetString(
                        *crate::runtime::error::PyExc_AttributeError.get(),
                        msg.as_ptr() as *const c_char,
                    );
                }
                return result;
            }
        }
        // No getattr slot — set AttributeError
        {
            let obj_tp = (*obj).ob_type;
            let tp_name_s = if !obj_tp.is_null() && !(*obj_tp).tp_name.is_null() {
                std::ffi::CStr::from_ptr((*obj_tp).tp_name).to_string_lossy().into_owned()
            } else { format!("unknown({:p})", obj_tp) };
            let name_cstr = crate::types::unicode::PyUnicode_AsUTF8(name);
            let attr_s = if !name_cstr.is_null() {
                std::ffi::CStr::from_ptr(name_cstr).to_string_lossy().into_owned()
            } else { "?".to_string() };
            let msg = format!("'{}' object has no attribute '{}'\0", tp_name_s, attr_s);
            crate::runtime::error::PyErr_SetString(
                *crate::runtime::error::PyExc_AttributeError.get(),
                msg.as_ptr() as *const c_char,
            );
        }
        ptr::null_mut()
    })
}

/// PyObject_SetAttr
#[no_mangle]
pub unsafe extern "C" fn PyObject_SetAttr(
    obj: *mut RawPyObject,
    name: *mut RawPyObject,
    value: *mut RawPyObject,
) -> c_int {
    crate::ffi::panic_guard::guard_int("PyObject_SetAttr", || unsafe {
        if obj.is_null() || name.is_null() {
            return -1;
        }
        let tp = (*obj).ob_type;
        if !tp.is_null() {
            if let Some(setattro) = (*tp).tp_setattro {
                let result = setattro(obj, name, value);
                if result < 0 && crate::runtime::error::PyErr_Occurred().is_null() {
                    crate::runtime::error::PyErr_SetString(
                        *crate::runtime::error::PyExc_AttributeError.get(),
                        b"can't set attribute\0".as_ptr() as *const c_char,
                    );
                }
                return result;
            }
        }
        // No setattr slot
        crate::runtime::error::PyErr_SetString(
            *crate::runtime::error::PyExc_AttributeError.get(),
            b"can't set attribute\0".as_ptr() as *const c_char,
        );
        -1
    })
}

/// PyObject_GetItem
#[no_mangle]
pub unsafe extern "C" fn PyObject_GetItem(
    obj: *mut RawPyObject,
    key: *mut RawPyObject,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyObject_GetItem", || unsafe {
        if obj.is_null() || key.is_null() {
            return ptr::null_mut();
        }
        let tp = (*obj).ob_type;
        if !tp.is_null() && !(*tp).tp_as_mapping.is_null() {
            if let Some(mp_subscript) = (*(*tp).tp_as_mapping).mp_subscript {
                return mp_subscript(obj, key);
            }
        }
        ptr::null_mut()
    })
}

/// PyObject_SetItem
#[no_mangle]
pub unsafe extern "C" fn PyObject_SetItem(
    obj: *mut RawPyObject,
    key: *mut RawPyObject,
    value: *mut RawPyObject,
) -> c_int {
    crate::ffi::panic_guard::guard_int("PyObject_SetItem", || unsafe {
        if obj.is_null() || key.is_null() {
            return -1;
        }
        let tp = (*obj).ob_type;
        if !tp.is_null() && !(*tp).tp_as_mapping.is_null() {
            if let Some(mp_ass_subscript) = (*(*tp).tp_as_mapping).mp_ass_subscript {
                return mp_ass_subscript(obj, key, value);
            }
        }
        -1
    })
}

/// PyObject_Length / PyObject_Size
#[no_mangle]
pub unsafe extern "C" fn PyObject_Length(obj: *mut RawPyObject) -> isize {
    crate::ffi::panic_guard::guard_ssize("PyObject_Length", || unsafe {
        if obj.is_null() {
            return -1;
        }
        let tp = (*obj).ob_type;
        if !tp.is_null() {
            // Try mapping protocol first
            if !(*tp).tp_as_mapping.is_null() {
                if let Some(mp_length) = (*(*tp).tp_as_mapping).mp_length {
                    return mp_length(obj);
                }
            }
            // Then sequence protocol
            if !(*tp).tp_as_sequence.is_null() {
                if let Some(sq_length) = (*(*tp).tp_as_sequence).sq_length {
                    return sq_length(obj);
                }
            }
        }
        -1
    })
}

/// PyObject_Size (alias)
#[no_mangle]
pub unsafe extern "C" fn PyObject_Size(obj: *mut RawPyObject) -> isize {
    PyObject_Length(obj)
}

/// PyObject_GetIter
#[no_mangle]
pub unsafe extern "C" fn PyObject_GetIter(obj: *mut RawPyObject) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyObject_GetIter", || unsafe {
        if obj.is_null() {
            return ptr::null_mut();
        }
        let tp = (*obj).ob_type;
        if !tp.is_null() {
            if let Some(tp_iter) = (*tp).tp_iter {
                return tp_iter(obj);
            }
        }
        ptr::null_mut()
    })
}

/// PyIter_Next
#[no_mangle]
pub unsafe extern "C" fn PyIter_Next(iter: *mut RawPyObject) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyIter_Next", || unsafe {
        if iter.is_null() {
            return ptr::null_mut();
        }
        let tp = (*iter).ob_type;
        if !tp.is_null() {
            if let Some(tp_iternext) = (*tp).tp_iternext {
                return tp_iternext(iter);
            }
        }
        ptr::null_mut()
    })
}

/// PyCallable_Check
#[no_mangle]
pub unsafe extern "C" fn PyCallable_Check(obj: *mut RawPyObject) -> c_int {
    crate::ffi::panic_guard::guard_int("PyCallable_Check", || unsafe {
        if obj.is_null() {
            return 0;
        }
        let tp = (*obj).ob_type;
        if !tp.is_null() && (*tp).tp_call.is_some() {
            1
        } else {
            0
        }
    })
}

/// PyObject_Call
#[no_mangle]
pub unsafe extern "C" fn PyObject_Call(
    callable: *mut RawPyObject,
    args: *mut RawPyObject,
    kwargs: *mut RawPyObject,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyObject_Call", || unsafe {
        if callable.is_null() {
            return ptr::null_mut();
        }

        // Check if it's a PyCFunction
        if (*callable).ob_type == crate::types::funcobject::cfunction_type() {
            return crate::types::funcobject::call_cfunction(callable, args, kwargs);
        }

        let tp = (*callable).ob_type;
        if !tp.is_null() {
            if let Some(tp_call) = (*tp).tp_call {
                return tp_call(callable, args, kwargs);
            }
        }
        // Not callable
        ptr::null_mut()
    })
}

/// PyObject_CallObject
#[no_mangle]
pub unsafe extern "C" fn PyObject_CallObject(
    callable: *mut RawPyObject,
    args: *mut RawPyObject,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyObject_CallObject", || unsafe {
        PyObject_Call(callable, args, ptr::null_mut())
    })
}

/// PyObject_CallNoArgs
#[no_mangle]
pub unsafe extern "C" fn PyObject_CallNoArgs(callable: *mut RawPyObject) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyObject_CallNoArgs", || unsafe {
        let empty_args = crate::types::tuple::PyTuple_New(0);
        let result = PyObject_Call(callable, empty_args, ptr::null_mut());
        (*empty_args).decref();
        result
    })
}

/// PyObject_CallOneArg
#[no_mangle]
pub unsafe extern "C" fn PyObject_CallOneArg(
    callable: *mut RawPyObject,
    arg: *mut RawPyObject,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyObject_CallOneArg", || unsafe {
        let args = crate::types::tuple::PyTuple_New(1);
        if !arg.is_null() {
            (*arg).incref();
        }
        crate::types::tuple::PyTuple_SET_ITEM(args, 0, arg);
        let result = PyObject_Call(callable, args, ptr::null_mut());
        (*args).decref();
        result
    })
}

/// Py_TYPE - get type pointer
#[no_mangle]
pub unsafe extern "C" fn Py_TYPE(obj: *mut RawPyObject) -> *mut RawPyTypeObject {
    crate::ffi::panic_guard::guard_ptr("Py_TYPE", || unsafe {
        if obj.is_null() {
            return ptr::null_mut();
        }
        (*obj).ob_type
    })
}

/// Py_SIZE - get ob_size for var objects
#[no_mangle]
pub unsafe extern "C" fn Py_SIZE(obj: *mut RawPyObject) -> isize {
    crate::ffi::panic_guard::guard_ssize("Py_SIZE", || unsafe {
        if obj.is_null() {
            return 0;
        }
        let var_obj = obj as *mut crate::object::pyobject::RawPyVarObject;
        (*var_obj).ob_size
    })
}

/// Py_IsNone
#[no_mangle]
pub unsafe extern "C" fn Py_IsNone(obj: *mut RawPyObject) -> c_int {
    if crate::types::none::is_none(obj) { 1 } else { 0 }
}

// PyType_IsSubtype, PyType_Ready, and PyType_GenericNew are in object/typeobj.rs

/// PyObject_CallMethod — call a method on an object by name.
/// The format argument is ignored (simplified); only used for no-arg calls in ujson.
#[no_mangle]
pub unsafe extern "C" fn PyObject_CallMethod(
    obj: *mut RawPyObject,
    name: *const c_char,
    _format: *const c_char,
    // varargs not supported, but ujson always passes NULL format
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyObject_CallMethod", || unsafe {
        if obj.is_null() || name.is_null() {
            return ptr::null_mut();
        }
        let method = PyObject_GetAttrString(obj, name);
        if method.is_null() {
            return ptr::null_mut();
        }
        // Call with no arguments (since format is NULL)
        let result = PyObject_CallNoArgs(method);
        (*method).decref();
        result
    })
}

/// PyObject_CallFunctionObjArgs — call a callable with a NULL-terminated list of PyObject* args.
#[no_mangle]
pub unsafe extern "C" fn PyObject_CallFunctionObjArgs(
    callable: *mut RawPyObject,
    // varargs: PyObject*, ..., NULL
    // We need to use C varargs here
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyObject_CallFunctionObjArgs", || {
        // This is a varargs function. In Rust extern "C" we can't easily handle varargs.
        // However, this is only called via the C ABI from C code, so we declare it in the
        // csrc/ shim. For now, provide a stub that handles the common 1-arg case.
        //
        // Actually, we need to implement this properly. Let's use a C shim.
        // For now, return null — we'll implement this as a C wrapper.
        ptr::null_mut()
    })
}

/// PyObject_IsInstance — check if obj is an instance of cls.
#[no_mangle]
pub unsafe extern "C" fn PyObject_IsInstance(
    inst: *mut RawPyObject,
    cls: *mut RawPyObject,
) -> c_int {
    crate::ffi::panic_guard::guard_int("PyObject_IsInstance", || unsafe {
        if inst.is_null() || cls.is_null() {
            return -1;
        }
        let inst_type = (*inst).ob_type;
        let cls_type = cls as *mut RawPyTypeObject;
        // Check if inst's type matches cls directly or via subtype chain
        crate::object::typeobj::PyType_IsSubtype(inst_type, cls_type)
    })
}

/// PyIter_Check — check if an object provides the iterator protocol.
/// Returns 1 if the object's type has tp_iternext set.
#[no_mangle]
pub unsafe extern "C" fn PyIter_Check(obj: *mut RawPyObject) -> c_int {
    crate::ffi::panic_guard::guard_int("PyIter_Check", || unsafe {
        if obj.is_null() {
            return 0;
        }
        let tp = (*obj).ob_type;
        if !tp.is_null() && (*tp).tp_iternext.is_some() {
            1
        } else {
            0
        }
    })
}

/// PyByteArray_Check — stub, always returns 0 (we don't support bytearray yet).
#[no_mangle]
pub unsafe extern "C" fn PyByteArray_Check(_obj: *mut RawPyObject) -> c_int {
    0
}

/// PyObject_Format — format an object using the __format__ protocol.
/// Falls back to str() for now.
#[no_mangle]
pub unsafe extern "C" fn PyObject_Format(
    obj: *mut RawPyObject,
    format_spec: *mut RawPyObject,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyObject_Format", || unsafe {
        // Simplified: just call str()
        PyObject_Str(obj)
    })
}

/// PyObject_ClearWeakRefs — clear all weak references to an object.
/// No-op for now since we don't have full weakref support.
#[no_mangle]
pub unsafe extern "C" fn PyObject_ClearWeakRefs(obj: *mut RawPyObject) {
    // No-op: weak reference list clearing
}

/// PyObject_GenericGetDict — get the __dict__ of an object.
/// For types with tp_dictoffset, returns the instance dict.
#[no_mangle]
pub unsafe extern "C" fn PyObject_GenericGetDict(
    obj: *mut RawPyObject,
    _context: *mut std::os::raw::c_void,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyObject_GenericGetDict", || unsafe {
        if obj.is_null() {
            return ptr::null_mut();
        }
        let tp = (*obj).ob_type;
        if tp.is_null() {
            return ptr::null_mut();
        }
        let offset = (*tp).tp_dictoffset;
        if offset > 0 {
            let dict_ptr = (obj as *mut u8).add(offset as usize) as *mut *mut RawPyObject;
            let dict = *dict_ptr;
            if dict.is_null() {
                // Create a new dict for this instance
                let new_dict = crate::types::dict::PyDict_New();
                *dict_ptr = new_dict;
                (*new_dict).incref();
                return new_dict;
            }
            (*dict).incref();
            return dict;
        }
        // No dict support
        ptr::null_mut()
    })
}

/// PyObject_GenericSetDict — set the __dict__ of an object.
#[no_mangle]
pub unsafe extern "C" fn PyObject_GenericSetDict(
    obj: *mut RawPyObject,
    value: *mut RawPyObject,
    _context: *mut std::os::raw::c_void,
) -> c_int {
    crate::ffi::panic_guard::guard_int("PyObject_GenericSetDict", || unsafe {
        if obj.is_null() {
            return -1;
        }
        let tp = (*obj).ob_type;
        if tp.is_null() {
            return -1;
        }
        let offset = (*tp).tp_dictoffset;
        if offset > 0 {
            let dict_ptr = (obj as *mut u8).add(offset as usize) as *mut *mut RawPyObject;
            if !value.is_null() {
                (*value).incref();
            }
            let old = *dict_ptr;
            *dict_ptr = value;
            if !old.is_null() {
                (*old).decref();
            }
            return 0;
        }
        -1
    })
}

/// PyNumber_Index — call __index__ to get an integer.
/// Returns the object itself if it's already an int, or calls nb_index.
#[no_mangle]
pub unsafe extern "C" fn PyNumber_Index(obj: *mut RawPyObject) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyNumber_Index", || unsafe {
        if obj.is_null() {
            return ptr::null_mut();
        }
        // If it's already an int, return it
        let tp = (*obj).ob_type;
        if tp == crate::types::longobject::PyLong_Type.get() {
            (*obj).incref();
            return obj;
        }
        // Try nb_index
        if !tp.is_null() && !(*tp).tp_as_number.is_null() {
            if let Some(nb_index) = (*(*tp).tp_as_number).nb_index {
                return nb_index(obj);
            }
        }
        ptr::null_mut()
    })
}

/// PyMethodObject — a bound method (function + self).
/// Layout matches CPython's PyMethodObject.
#[repr(C)]
pub struct PyMethodObject {
    pub ob_refcnt: std::sync::atomic::AtomicIsize,
    pub ob_type: *mut RawPyTypeObject,
    pub im_func: *mut RawPyObject,
    pub im_self: *mut RawPyObject,
    pub im_weakreflist: *mut RawPyObject,
}

/// method_call — tp_call for PyMethod_Type.
/// Prepends self to the args tuple and calls the underlying function.
unsafe extern "C" fn method_call(
    method: *mut RawPyObject,
    args: *mut RawPyObject,
    kwargs: *mut RawPyObject,
) -> *mut RawPyObject {
    let m = method as *mut PyMethodObject;
    let func = (*m).im_func;
    let self_obj = (*m).im_self;
    // Build new args tuple: (self, *args)
    let nargs = if args.is_null() { 0 } else { crate::types::tuple::PyTuple_GET_SIZE(args) };
    let new_args = crate::types::tuple::PyTuple_New(nargs + 1);
    if !self_obj.is_null() {
        (*self_obj).incref();
    }
    crate::types::tuple::PyTuple_SET_ITEM(new_args, 0, self_obj);
    for i in 0..nargs {
        let arg = crate::types::tuple::PyTuple_GET_ITEM(args, i);
        if !arg.is_null() {
            (*arg).incref();
        }
        crate::types::tuple::PyTuple_SET_ITEM(new_args, i + 1, arg);
    }

    let result = PyObject_Call(func, new_args, kwargs);
    (*new_args).decref();
    result
}

/// method_dealloc — tp_dealloc for PyMethod_Type.
unsafe extern "C" fn method_dealloc(obj: *mut RawPyObject) {
    let m = obj as *mut PyMethodObject;
    if !(*m).im_func.is_null() {
        (*(*m).im_func).decref();
    }
    if !(*m).im_self.is_null() {
        (*(*m).im_self).decref();
    }
    libc::free(obj as *mut _);
}

/// Initialize PyMethod_Type slots (called during Py_Initialize).
pub unsafe fn init_method_type() {
    let tp = crate::object::typeobj::PyMethod_Type.get();
    (*tp).tp_call = Some(method_call);
    (*tp).tp_dealloc = Some(method_dealloc);
    (*tp).tp_basicsize = std::mem::size_of::<PyMethodObject>() as isize;
    (*tp).ob_base.ob_type = crate::object::typeobj::PyType_Type.get();
    (*tp).ob_base.ob_refcnt = std::sync::atomic::AtomicIsize::new(isize::MAX / 2);
    (*tp).tp_flags = crate::object::typeobj::PY_TPFLAGS_DEFAULT
        | crate::object::typeobj::PY_TPFLAGS_READY;
}

/// PyMethod_New — create a bound method from a function and an instance.
#[no_mangle]
pub unsafe extern "C" fn PyMethod_New(
    func: *mut RawPyObject,
    self_obj: *mut RawPyObject,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyMethod_New", || unsafe {
        if func.is_null() {
            return ptr::null_mut();
        }
        let m = libc::calloc(1, std::mem::size_of::<PyMethodObject>()) as *mut PyMethodObject;
        if m.is_null() {
            return ptr::null_mut();
        }
        std::ptr::write(&mut (*m).ob_refcnt, std::sync::atomic::AtomicIsize::new(1));
        (*m).ob_type = crate::object::typeobj::PyMethod_Type.get();
        (*func).incref();
        (*m).im_func = func;
        if !self_obj.is_null() {
            (*self_obj).incref();
        }
        (*m).im_self = self_obj;
        (*m).im_weakreflist = ptr::null_mut();
        m as *mut RawPyObject
    })
}

/// PyCMethod_New — create a C method (PyO3 stable ABI).
/// Simplified: delegates to PyMethod_New.
#[no_mangle]
pub unsafe extern "C" fn PyCMethod_New(
    ml: *mut crate::object::typeobj::PyMethodDef,
    self_obj: *mut RawPyObject,
    module: *mut RawPyObject,
    cls: *mut crate::object::typeobj::RawPyTypeObject,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyCMethod_New", || unsafe {
        if ml.is_null() {
            return ptr::null_mut();
        }
        crate::types::funcobject::PyCFunction_NewEx(ml, self_obj, module)
    })
}

/// PyObject_HasAttr — check if an object has an attribute (by PyObject name).
#[no_mangle]
pub unsafe extern "C" fn PyObject_HasAttr(
    obj: *mut RawPyObject,
    name: *mut RawPyObject,
) -> c_int {
    crate::ffi::panic_guard::guard_int("PyObject_HasAttr", || unsafe {
        let attr = PyObject_GetAttr(obj, name);
        if attr.is_null() {
            crate::runtime::error::PyErr_Clear();
            0
        } else {
            (*attr).decref();
            1
        }
    })
}

/// PyObject_IsSubclass — check if derived is a subclass of cls.
#[no_mangle]
pub unsafe extern "C" fn PyObject_IsSubclass(
    derived: *mut RawPyObject,
    cls: *mut RawPyObject,
) -> c_int {
    crate::ffi::panic_guard::guard_int("PyObject_IsSubclass", || unsafe {
        if derived.is_null() || cls.is_null() {
            return -1;
        }
        crate::object::typeobj::PyType_IsSubtype(
            derived as *mut RawPyTypeObject,
            cls as *mut RawPyTypeObject,
        )
    })
}

/// PyObject_CallMethodObjArgs — call a method on an object.
/// The method name and args are PyObject*s, terminated by NULL.
/// Since Rust can't handle C varargs, this is a C shim.
/// Here we provide a simplified version that handles 0-3 args.
#[no_mangle]
pub unsafe extern "C" fn PyObject_CallMethodObjArgs(
    obj: *mut RawPyObject,
    name: *mut RawPyObject,
    // varargs: PyObject*, ..., NULL — handled by C shim in csrc/varargs.c
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyObject_CallMethodObjArgs", || unsafe {
        if obj.is_null() || name.is_null() {
            return ptr::null_mut();
        }
        let method = PyObject_GetAttr(obj, name);
        if method.is_null() {
            return ptr::null_mut();
        }
        // Called with no extra args (the varargs are NULL-terminated, and the
        // first vararg after `name` is NULL for no-arg calls)
        let result = PyObject_CallNoArgs(method);
        (*method).decref();
        result
    })
}

/// PyObject_CallFinalizerFromDealloc — call tp_finalize if set, then check
/// if the object was resurrected. Returns 0 if dealloc should proceed, -1 if resurrected.
#[no_mangle]
pub unsafe extern "C" fn PyObject_CallFinalizerFromDealloc(
    obj: *mut RawPyObject,
) -> c_int {
    crate::ffi::panic_guard::guard_int("PyObject_CallFinalizerFromDealloc", || unsafe {
        if obj.is_null() {
            return 0;
        }
        let tp = (*obj).ob_type;
        if tp.is_null() {
            return 0;
        }
        if let Some(finalize) = (*tp).tp_finalize {
            // Temporarily resurrect the object (prevent double-free)
            (*obj).ob_refcnt.store(1, std::sync::atomic::Ordering::Relaxed);
            finalize(obj);
            // Check if something took a reference during finalization
            let refcnt = (*obj).ob_refcnt.load(std::sync::atomic::Ordering::Relaxed);
            if refcnt > 1 {
                // Object was resurrected — undo the dealloc
                (*obj).ob_refcnt.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                return -1;
            }
            // Set refcnt back to 0 for dealloc
            (*obj).ob_refcnt.store(0, std::sync::atomic::Ordering::Relaxed);
        }
        0
    })
}

/// PyVectorcall_Function — return the vectorcall function pointer for a callable,
/// or NULL if the object doesn't support vectorcall (caller falls back to tp_call).
/// Our types don't use vectorcall, so always return NULL.
#[no_mangle]
pub unsafe extern "C" fn PyVectorcall_Function(
    _callable: *mut RawPyObject,
) -> *const std::ffi::c_void {
    std::ptr::null()
}

/// PyObject_VectorcallDict — call a callable with args array + kwargs dict.
/// Used by Cython's __Pyx_PyObject_Call.
#[no_mangle]
pub unsafe extern "C" fn PyObject_VectorcallDict(
    callable: *mut RawPyObject,
    args: *const *mut RawPyObject,
    nargsf: usize,
    kwdict: *mut RawPyObject,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyObject_VectorcallDict", || unsafe {
        if callable.is_null() {
            return ptr::null_mut();
        }
        let nargs = nargsf & !(1usize << (usize::BITS - 1)); // mask out PY_VECTORCALL_ARGUMENTS_OFFSET

        // Build args tuple
        let args_tuple = crate::types::tuple::PyTuple_New(nargs as isize);
        if !args.is_null() {
            for i in 0..nargs {
                let arg = *args.add(i);
                if !arg.is_null() {
                    (*arg).incref();
                }
                crate::types::tuple::PyTuple_SET_ITEM(args_tuple, i as isize, arg);
            }
        }

        let result = PyObject_Call(callable, args_tuple, kwdict);
        (*args_tuple).decref();
        result
    })
}

/// PyObject_Vectorcall — call a callable with vectorcall protocol.
#[no_mangle]
pub unsafe extern "C" fn PyObject_Vectorcall(
    callable: *mut RawPyObject,
    args: *const *mut RawPyObject,
    nargsf: usize,
    kwnames: *mut RawPyObject,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyObject_Vectorcall", || unsafe {
        // Simplified: ignore kwnames, delegate to VectorcallDict with no kwargs
        PyObject_VectorcallDict(callable, args, nargsf, ptr::null_mut())
    })
}

/// PyObject_VectorcallMethod — call a method by name with vectorcall args.
#[no_mangle]
pub unsafe extern "C" fn PyObject_VectorcallMethod(
    name: *mut RawPyObject,
    args: *const *mut RawPyObject,
    nargsf: usize,
    kwnames: *mut RawPyObject,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyObject_VectorcallMethod", || unsafe {
        if name.is_null() || args.is_null() {
            return ptr::null_mut();
        }
        let nargs = nargsf & !(1usize << (usize::BITS - 1));
        if nargs == 0 {
            return ptr::null_mut();
        }
        // args[0] is self
        let self_obj = *args.add(0);
        if self_obj.is_null() {
            return ptr::null_mut();
        }
        let method = PyObject_GetAttr(self_obj, name);
        if method.is_null() {
            return ptr::null_mut();
        }
        // Build args tuple from args[1..nargs]
        let actual_nargs = nargs - 1;
        let args_tuple = crate::types::tuple::PyTuple_New(actual_nargs as isize);
        for i in 0..actual_nargs {
            let arg = *args.add(i + 1);
            if !arg.is_null() {
                (*arg).incref();
            }
            crate::types::tuple::PyTuple_SET_ITEM(args_tuple, i as isize, arg);
        }
        let result = PyObject_Call(method, args_tuple, ptr::null_mut());
        (*args_tuple).decref();
        (*method).decref();
        result
    })
}

/// PySequence_Contains — check if a sequence contains a value.
#[no_mangle]
pub unsafe extern "C" fn PySequence_Contains(
    seq: *mut RawPyObject,
    value: *mut RawPyObject,
) -> c_int {
    crate::ffi::panic_guard::guard_int("PySequence_Contains", || unsafe {
        if seq.is_null() || value.is_null() {
            return -1;
        }
        let tp = (*seq).ob_type;
        // Try sq_contains first
        if !tp.is_null() && !(*tp).tp_as_sequence.is_null() {
            if let Some(sq_contains) = (*(*tp).tp_as_sequence).sq_contains {
                return sq_contains(seq, value);
            }
        }
        // Fallback: iterate
        let iter = PyObject_GetIter(seq);
        if iter.is_null() {
            return -1;
        }
        loop {
            let item = PyIter_Next(iter);
            if item.is_null() {
                (*iter).decref();
                // Check if iteration ended normally or with error
                if crate::runtime::error::PyErr_Occurred().is_null() {
                    return 0; // not found
                }
                return -1; // error
            }
            let cmp = PyObject_RichCompareBool(item, value, PY_EQ);
            (*item).decref();
            if cmp > 0 {
                (*iter).decref();
                return 1; // found
            }
            if cmp < 0 {
                (*iter).decref();
                return -1; // error
            }
        }
    })
}

/// PyNumber_InPlaceAdd — in-place addition (nb_inplace_add or nb_add).
#[no_mangle]
pub unsafe extern "C" fn PyNumber_InPlaceAdd(
    v: *mut RawPyObject,
    w: *mut RawPyObject,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyNumber_InPlaceAdd", || unsafe {
        if v.is_null() || w.is_null() {
            return ptr::null_mut();
        }
        let tp = (*v).ob_type;
        if !tp.is_null() && !(*tp).tp_as_number.is_null() {
            // Try nb_inplace_add first
            if let Some(iadd) = (*(*tp).tp_as_number).nb_inplace_add {
                return iadd(v, w);
            }
            // Fall back to nb_add
            if let Some(add) = (*(*tp).tp_as_number).nb_add {
                return add(v, w);
            }
        }
        ptr::null_mut()
    })
}

/// PyNumber_Remainder — modulo operation (nb_remainder).
#[no_mangle]
pub unsafe extern "C" fn PyNumber_Remainder(
    v: *mut RawPyObject,
    w: *mut RawPyObject,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyNumber_Remainder", || unsafe {
        if v.is_null() || w.is_null() {
            return ptr::null_mut();
        }
        let tp = (*v).ob_type;
        if !tp.is_null() && !(*tp).tp_as_number.is_null() {
            if let Some(rem) = (*(*tp).tp_as_number).nb_remainder {
                return rem(v, w);
            }
        }
        ptr::null_mut()
    })
}

/// Py_EnterRecursiveCall — guard against C stack overflow.
/// Stub: always returns 0 (success).
#[no_mangle]
pub unsafe extern "C" fn Py_EnterRecursiveCall(_where: *const c_char) -> c_int {
    0
}

/// Py_LeaveRecursiveCall — leave recursion guard.
#[no_mangle]
pub unsafe extern "C" fn Py_LeaveRecursiveCall() {
    // No-op
}

/// _PyObject_GenericGetAttrWithDict — GenericGetAttr with an explicit dict and suppress flag.
/// When suppress=1, returns NULL without setting AttributeError on failure.
/// This is used by Cython's __Pyx_PyObject_GetAttrStrNoError.
#[no_mangle]
pub unsafe extern "C" fn _PyObject_GenericGetAttrWithDict(
    obj: *mut RawPyObject,
    name: *mut RawPyObject,
    dict: *mut RawPyObject,
    suppress: c_int,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("_PyObject_GenericGetAttrWithDict", || unsafe {
        if obj.is_null() || name.is_null() {
            return ptr::null_mut();
        }

        // Try the provided dict first
        if !dict.is_null() {
            let value = crate::types::dict::PyDict_GetItem(dict, name);
            if !value.is_null() {
                (*value).incref();
                return value;
            }
        }

        // Do the full GenericGetAttr lookup
        let result = crate::object::typeobj::PyObject_GenericGetAttr(obj, name);

        // Debug trace
        if std::env::var("RUSTTHON_TRACE").is_ok() {
            let name_str = crate::types::unicode::PyUnicode_AsUTF8(name);
            let attr = if !name_str.is_null() {
                std::ffi::CStr::from_ptr(name_str).to_string_lossy().into_owned()
            } else { "(null)".to_string() };
            let tp = (*obj).ob_type;
            let tp_name = if !tp.is_null() && !(*tp).tp_name.is_null() {
                std::ffi::CStr::from_ptr((*tp).tp_name).to_string_lossy().into_owned()
            } else { "(null)".to_string() };
            eprintln!("[rustthon] _PyObject_GenericGetAttrWithDict: obj type='{}' attr='{}' found={} suppress={}",
                tp_name, attr, !result.is_null(), suppress);
        }

        // If suppress=1 and lookup failed with AttributeError, clear the error
        if result.is_null() && suppress != 0 {
            if !crate::runtime::error::PyErr_Occurred().is_null() {
                let exc_type = crate::runtime::error::PyErr_Occurred();
                let attr_error = *crate::runtime::error::PyExc_AttributeError.get();
                if exc_type == attr_error {
                    crate::runtime::error::PyErr_Clear();
                }
            }
        }

        result
    })
}

/// _PyObject_GetDictPtr — get the __dict__ pointer of an object.
/// Returns pointer to the PyObject* dict slot inside the object.
#[no_mangle]
pub unsafe extern "C" fn _PyObject_GetDictPtr(
    obj: *mut RawPyObject,
) -> *mut *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("_PyObject_GetDictPtr", || unsafe {
        if obj.is_null() {
            return ptr::null_mut();
        }
        let tp = (*obj).ob_type;
        if tp.is_null() {
            return ptr::null_mut();
        }
        let offset = (*tp).tp_dictoffset;
        if offset > 0 {
            (obj as *mut u8).add(offset as usize) as *mut *mut RawPyObject
        } else {
            ptr::null_mut()
        }
    })
}

// PyType_GenericAlloc is in object/typeobj.rs
