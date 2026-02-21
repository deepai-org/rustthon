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
}

/// PyObject_Str
#[no_mangle]
pub unsafe extern "C" fn PyObject_Str(obj: *mut RawPyObject) -> *mut RawPyObject {
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
}

/// PyObject_Hash
#[no_mangle]
pub unsafe extern "C" fn PyObject_Hash(obj: *mut RawPyObject) -> isize {
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
}

/// PyObject_RichCompare
#[no_mangle]
pub unsafe extern "C" fn PyObject_RichCompare(
    v: *mut RawPyObject,
    w: *mut RawPyObject,
    op: c_int,
) -> *mut RawPyObject {
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
}

/// PyObject_RichCompareBool
#[no_mangle]
pub unsafe extern "C" fn PyObject_RichCompareBool(
    v: *mut RawPyObject,
    w: *mut RawPyObject,
    op: c_int,
) -> c_int {
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
}

/// PyObject_IsTrue
#[no_mangle]
pub unsafe extern "C" fn PyObject_IsTrue(obj: *mut RawPyObject) -> c_int {
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
    // Check nb_bool
    let tp = (*obj).ob_type;
    if !tp.is_null() && !(*tp).tp_as_number.is_null() {
        if let Some(nb_bool) = (*(*tp).tp_as_number).nb_bool {
            return nb_bool(obj);
        }
    }
    // Check sq_length (empty containers are falsy)
    if !tp.is_null() && !(*tp).tp_as_sequence.is_null() {
        if let Some(sq_length) = (*(*tp).tp_as_sequence).sq_length {
            let len = sq_length(obj);
            return if len > 0 { 1 } else { 0 };
        }
    }
    // Default: true
    1
}

/// PyObject_Not
#[no_mangle]
pub unsafe extern "C" fn PyObject_Not(obj: *mut RawPyObject) -> c_int {
    let result = PyObject_IsTrue(obj);
    if result < 0 {
        return result;
    }
    if result == 0 { 1 } else { 0 }
}

/// PyObject_Type - get the type of an object (new reference)
#[no_mangle]
pub unsafe extern "C" fn PyObject_Type(obj: *mut RawPyObject) -> *mut RawPyObject {
    if obj.is_null() {
        return ptr::null_mut();
    }
    let tp = (*obj).ob_type as *mut RawPyObject;
    if !tp.is_null() {
        (*tp).incref();
    }
    tp
}

/// PyObject_TypeCheck
#[no_mangle]
pub unsafe extern "C" fn PyObject_TypeCheck(
    obj: *mut RawPyObject,
    tp: *mut RawPyTypeObject,
) -> c_int {
    if obj.is_null() || tp.is_null() {
        return 0;
    }
    if (*obj).ob_type == tp { 1 } else { 0 }
}

/// PyObject_HasAttrString
#[no_mangle]
pub unsafe extern "C" fn PyObject_HasAttrString(
    obj: *mut RawPyObject,
    name: *const c_char,
) -> c_int {
    let attr = PyObject_GetAttrString(obj, name);
    if attr.is_null() {
        crate::runtime::error::PyErr_Clear();
        0
    } else {
        (*attr).decref();
        1
    }
}

/// PyObject_GetAttrString
#[no_mangle]
pub unsafe extern "C" fn PyObject_GetAttrString(
    obj: *mut RawPyObject,
    name: *const c_char,
) -> *mut RawPyObject {
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
            return result;
        }
        // Fall back to tp_getattr (legacy)
        if let Some(getattr) = (*tp).tp_getattr {
            return getattr(obj, name as *mut c_char);
        }
    }

    // For module objects, check their dict
    if !tp.is_null() && (*tp).tp_name == crate::types::moduleobject::module_type() as *const _ as *const c_char {
        // It's a module - look in its dict
    }

    ptr::null_mut()
}

/// PyObject_SetAttrString
#[no_mangle]
pub unsafe extern "C" fn PyObject_SetAttrString(
    obj: *mut RawPyObject,
    name: *const c_char,
    value: *mut RawPyObject,
) -> c_int {
    if obj.is_null() || name.is_null() {
        return -1;
    }
    let tp = (*obj).ob_type;
    if !tp.is_null() {
        if let Some(setattro) = (*tp).tp_setattro {
            let name_obj = crate::types::unicode::PyUnicode_FromString(name);
            let result = setattro(obj, name_obj, value);
            (*name_obj).decref();
            return result;
        }
    }
    -1
}

/// PyObject_GetAttr
#[no_mangle]
pub unsafe extern "C" fn PyObject_GetAttr(
    obj: *mut RawPyObject,
    name: *mut RawPyObject,
) -> *mut RawPyObject {
    if obj.is_null() || name.is_null() {
        return ptr::null_mut();
    }
    let tp = (*obj).ob_type;
    if !tp.is_null() {
        if let Some(getattro) = (*tp).tp_getattro {
            return getattro(obj, name);
        }
    }
    ptr::null_mut()
}

/// PyObject_SetAttr
#[no_mangle]
pub unsafe extern "C" fn PyObject_SetAttr(
    obj: *mut RawPyObject,
    name: *mut RawPyObject,
    value: *mut RawPyObject,
) -> c_int {
    if obj.is_null() || name.is_null() {
        return -1;
    }
    let tp = (*obj).ob_type;
    if !tp.is_null() {
        if let Some(setattro) = (*tp).tp_setattro {
            return setattro(obj, name, value);
        }
    }
    -1
}

/// PyObject_GetItem
#[no_mangle]
pub unsafe extern "C" fn PyObject_GetItem(
    obj: *mut RawPyObject,
    key: *mut RawPyObject,
) -> *mut RawPyObject {
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
}

/// PyObject_SetItem
#[no_mangle]
pub unsafe extern "C" fn PyObject_SetItem(
    obj: *mut RawPyObject,
    key: *mut RawPyObject,
    value: *mut RawPyObject,
) -> c_int {
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
}

/// PyObject_Length / PyObject_Size
#[no_mangle]
pub unsafe extern "C" fn PyObject_Length(obj: *mut RawPyObject) -> isize {
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
}

/// PyObject_Size (alias)
#[no_mangle]
pub unsafe extern "C" fn PyObject_Size(obj: *mut RawPyObject) -> isize {
    PyObject_Length(obj)
}

/// PyObject_GetIter
#[no_mangle]
pub unsafe extern "C" fn PyObject_GetIter(obj: *mut RawPyObject) -> *mut RawPyObject {
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
}

/// PyIter_Next
#[no_mangle]
pub unsafe extern "C" fn PyIter_Next(iter: *mut RawPyObject) -> *mut RawPyObject {
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
}

/// PyCallable_Check
#[no_mangle]
pub unsafe extern "C" fn PyCallable_Check(obj: *mut RawPyObject) -> c_int {
    if obj.is_null() {
        return 0;
    }
    let tp = (*obj).ob_type;
    if !tp.is_null() && (*tp).tp_call.is_some() {
        1
    } else {
        0
    }
}

/// PyObject_Call
#[no_mangle]
pub unsafe extern "C" fn PyObject_Call(
    callable: *mut RawPyObject,
    args: *mut RawPyObject,
    kwargs: *mut RawPyObject,
) -> *mut RawPyObject {
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
}

/// PyObject_CallObject
#[no_mangle]
pub unsafe extern "C" fn PyObject_CallObject(
    callable: *mut RawPyObject,
    args: *mut RawPyObject,
) -> *mut RawPyObject {
    PyObject_Call(callable, args, ptr::null_mut())
}

/// PyObject_CallNoArgs
#[no_mangle]
pub unsafe extern "C" fn PyObject_CallNoArgs(callable: *mut RawPyObject) -> *mut RawPyObject {
    let empty_args = crate::types::tuple::PyTuple_New(0);
    let result = PyObject_Call(callable, empty_args, ptr::null_mut());
    (*empty_args).decref();
    result
}

/// PyObject_CallOneArg
#[no_mangle]
pub unsafe extern "C" fn PyObject_CallOneArg(
    callable: *mut RawPyObject,
    arg: *mut RawPyObject,
) -> *mut RawPyObject {
    let args = crate::types::tuple::PyTuple_New(1);
    if !arg.is_null() {
        (*arg).incref();
    }
    crate::types::tuple::PyTuple_SET_ITEM(args, 0, arg);
    let result = PyObject_Call(callable, args, ptr::null_mut());
    (*args).decref();
    result
}

/// Py_TYPE - get type pointer
#[no_mangle]
pub unsafe extern "C" fn Py_TYPE(obj: *mut RawPyObject) -> *mut RawPyTypeObject {
    if obj.is_null() {
        return ptr::null_mut();
    }
    (*obj).ob_type
}

/// Py_SIZE - get ob_size for var objects
#[no_mangle]
pub unsafe extern "C" fn Py_SIZE(obj: *mut RawPyObject) -> isize {
    if obj.is_null() {
        return 0;
    }
    let var_obj = obj as *mut crate::object::pyobject::RawPyVarObject;
    (*var_obj).ob_size
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
}

/// PyObject_CallFunctionObjArgs — call a callable with a NULL-terminated list of PyObject* args.
#[no_mangle]
pub unsafe extern "C" fn PyObject_CallFunctionObjArgs(
    callable: *mut RawPyObject,
    // varargs: PyObject*, ..., NULL
    // We need to use C varargs here
) -> *mut RawPyObject {
    // This is a varargs function. In Rust extern "C" we can't easily handle varargs.
    // However, this is only called via the C ABI from C code, so we declare it in the
    // csrc/ shim. For now, provide a stub that handles the common 1-arg case.
    //
    // Actually, we need to implement this properly. Let's use a C shim.
    // For now, return null — we'll implement this as a C wrapper.
    ptr::null_mut()
}

/// PyObject_IsInstance — check if obj is an instance of cls.
#[no_mangle]
pub unsafe extern "C" fn PyObject_IsInstance(
    inst: *mut RawPyObject,
    cls: *mut RawPyObject,
) -> c_int {
    if inst.is_null() || cls.is_null() {
        return -1;
    }
    let inst_type = (*inst).ob_type;
    let cls_type = cls as *mut RawPyTypeObject;
    // Check if inst's type matches cls directly or via subtype chain
    crate::object::typeobj::PyType_IsSubtype(inst_type, cls_type)
}

/// PyIter_Check — check if an object provides the iterator protocol.
/// Returns 1 if the object's type has tp_iternext set.
#[no_mangle]
pub unsafe extern "C" fn PyIter_Check(obj: *mut RawPyObject) -> c_int {
    if obj.is_null() {
        return 0;
    }
    let tp = (*obj).ob_type;
    if !tp.is_null() && (*tp).tp_iternext.is_some() {
        1
    } else {
        0
    }
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
    // Simplified: just call str()
    PyObject_Str(obj)
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
}

/// PyObject_GenericSetDict — set the __dict__ of an object.
#[no_mangle]
pub unsafe extern "C" fn PyObject_GenericSetDict(
    obj: *mut RawPyObject,
    value: *mut RawPyObject,
    _context: *mut std::os::raw::c_void,
) -> c_int {
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
}

/// PyNumber_Index — call __index__ to get an integer.
/// Returns the object itself if it's already an int, or calls nb_index.
#[no_mangle]
pub unsafe extern "C" fn PyNumber_Index(obj: *mut RawPyObject) -> *mut RawPyObject {
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
}

/// PyMethod_New — create a bound method from a function and an instance.
#[no_mangle]
pub unsafe extern "C" fn PyMethod_New(
    func: *mut RawPyObject,
    self_obj: *mut RawPyObject,
) -> *mut RawPyObject {
    // Simplified: create a tuple (func, self) as a method stand-in.
    // A real implementation would use a PyMethodObject type.
    if func.is_null() {
        return ptr::null_mut();
    }
    let method = crate::types::tuple::PyTuple_New(2);
    (*func).incref();
    crate::types::tuple::PyTuple_SET_ITEM(method, 0, func);
    if !self_obj.is_null() {
        (*self_obj).incref();
    }
    crate::types::tuple::PyTuple_SET_ITEM(method, 1, self_obj);
    method
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
    if ml.is_null() {
        return ptr::null_mut();
    }
    crate::types::funcobject::PyCFunction_NewEx(ml, self_obj, module)
}

// PyType_GenericAlloc is in object/typeobj.rs
