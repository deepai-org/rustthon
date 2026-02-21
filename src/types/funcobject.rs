//! Python function/method objects for wrapping C functions.
//!
//! When a C extension defines methods via PyMethodDef,
//! we wrap them in PyCFunction objects.

use crate::object::pyobject::{PyObjectWithData, RawPyObject};
use crate::object::typeobj::{PyCFunction, PyMethodDef, RawPyTypeObject, METH_NOARGS, METH_O, METH_VARARGS, METH_KEYWORDS};
use std::os::raw::{c_char, c_int};
use std::ptr;

static mut CFUNCTION_TYPE: RawPyTypeObject = {
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"builtin_function_or_method\0".as_ptr() as *const _;
    tp.tp_basicsize = 0;
    tp
};

pub unsafe fn cfunction_type() -> *mut RawPyTypeObject {
    &mut CFUNCTION_TYPE
}

pub unsafe fn init_cfunction_type() {
    CFUNCTION_TYPE.tp_getattro = Some(cfunction_getattro);
}

pub struct CFunctionData {
    pub name: *const c_char,
    pub meth: Option<PyCFunction>,
    pub flags: c_int,
    pub self_obj: *mut RawPyObject, // The module or object this is bound to
    pub module: *mut RawPyObject,   // The module object (for __module__)
    pub ml_def: *mut PyMethodDef,   // Original method def (for __doc__)
}

type PyCFunctionObject = PyObjectWithData<CFunctionData>;

/// tp_getattro for PyCFunction — return __name__, __module__, __doc__, __self__
unsafe extern "C" fn cfunction_getattro(
    obj: *mut RawPyObject,
    name: *mut RawPyObject,
) -> *mut RawPyObject {
    let attr = crate::types::unicode::PyUnicode_AsUTF8(name);
    if attr.is_null() {
        return ptr::null_mut();
    }
    let attr_str = std::ffi::CStr::from_ptr(attr).to_bytes();
    let data = PyObjectWithData::<CFunctionData>::data_from_raw(obj);

    match attr_str {
        b"__name__" | b"__qualname__" => {
            if !data.name.is_null() {
                crate::types::unicode::PyUnicode_FromString(data.name)
            } else {
                crate::types::unicode::create_from_str("<unknown>")
            }
        }
        b"__module__" => {
            if !data.module.is_null() {
                let mod_name = crate::types::moduleobject::PyModule_GetNameObject(data.module);
                if !mod_name.is_null() {
                    return mod_name;
                }
            }
            crate::types::none::return_none() // already increfs
        }
        b"__doc__" => {
            if !data.ml_def.is_null() && !(*data.ml_def).ml_doc.is_null() {
                crate::types::unicode::PyUnicode_FromString((*data.ml_def).ml_doc)
            } else {
                crate::types::none::return_none()
            }
        }
        b"__self__" => {
            if !data.self_obj.is_null() {
                (*data.self_obj).incref();
                data.self_obj
            } else {
                crate::types::none::return_none()
            }
        }
        b"__call__" => {
            // Return self — functions are callable
            (*obj).incref();
            obj
        }
        _ => {
            crate::runtime::error::PyErr_SetString(
                crate::runtime::error::PyExc_AttributeError,
                b"builtin_function_or_method has no such attribute\0".as_ptr() as *const _,
            );
            ptr::null_mut()
        }
    }
}

/// Create a PyCFunction wrapping a C method definition.
pub unsafe fn create_cfunction(
    name: *const c_char,
    meth: Option<PyCFunction>,
    flags: c_int,
    self_obj: *mut RawPyObject,
) -> *mut RawPyObject {
    create_cfunction_full(name, meth, flags, self_obj, ptr::null_mut(), ptr::null_mut())
}

/// Create a PyCFunction with full metadata.
pub unsafe fn create_cfunction_full(
    name: *const c_char,
    meth: Option<PyCFunction>,
    flags: c_int,
    self_obj: *mut RawPyObject,
    module: *mut RawPyObject,
    ml_def: *mut PyMethodDef,
) -> *mut RawPyObject {
    if !self_obj.is_null() {
        (*self_obj).incref();
    }
    if !module.is_null() {
        (*module).incref();
    }
    let obj = PyObjectWithData::alloc(
        &mut CFUNCTION_TYPE,
        CFunctionData {
            name,
            meth,
            flags,
            self_obj,
            module,
            ml_def,
        },
    );
    obj as *mut RawPyObject
}

/// Call a PyCFunction with the given arguments.
pub unsafe fn call_cfunction(
    func: *mut RawPyObject,
    args: *mut RawPyObject,
    kwargs: *mut RawPyObject,
) -> *mut RawPyObject {
    if func.is_null() {
        return ptr::null_mut();
    }
    let data = PyObjectWithData::<CFunctionData>::data_from_raw(func);
    let meth = match data.meth {
        Some(m) => m,
        None => return ptr::null_mut(),
    };

    let flags = data.flags;
    let self_obj = data.self_obj;

    if flags & METH_NOARGS != 0 {
        // METH_NOARGS: func(self, NULL)
        meth(self_obj, ptr::null_mut())
    } else if flags & METH_O != 0 {
        // METH_O: func(self, arg)
        // arg is the single positional argument
        if !args.is_null() && crate::types::tuple::PyTuple_Check(args) != 0 {
            let arg = crate::types::tuple::PyTuple_GetItem(args, 0);
            meth(self_obj, arg)
        } else {
            meth(self_obj, args)
        }
    } else if flags & METH_KEYWORDS != 0 {
        // METH_VARARGS | METH_KEYWORDS: func(self, args, kwargs)
        let meth_kw: crate::object::typeobj::PyCFunctionWithKeywords =
            std::mem::transmute(meth);
        meth_kw(self_obj, args, kwargs)
    } else {
        // METH_VARARGS (default): func(self, args)
        meth(self_obj, args)
    }
}

// ─── C API ───

/// PyCFunction_NewEx
#[no_mangle]
pub unsafe extern "C" fn PyCFunction_NewEx(
    ml: *mut PyMethodDef,
    self_obj: *mut RawPyObject,
    module: *mut RawPyObject,
) -> *mut RawPyObject {
    if ml.is_null() {
        return ptr::null_mut();
    }
    create_cfunction_full((*ml).ml_name, (*ml).ml_meth, (*ml).ml_flags, self_obj, module, ml)
}

/// PyCFunction_New
#[no_mangle]
pub unsafe extern "C" fn PyCFunction_New(
    ml: *mut PyMethodDef,
    self_obj: *mut RawPyObject,
) -> *mut RawPyObject {
    PyCFunction_NewEx(ml, self_obj, ptr::null_mut())
}
