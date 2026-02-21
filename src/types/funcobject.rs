//! Python function/method objects for wrapping C functions.
//!
//! When a C extension defines methods via PyMethodDef,
//! we wrap them in PyCFunction objects.

use crate::object::pyobject::{PyObjectWithData, RawPyObject};
use crate::object::typeobj::{PyCFunction, RawPyTypeObject, METH_NOARGS, METH_O, METH_VARARGS, METH_KEYWORDS};
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

pub struct CFunctionData {
    pub name: *const c_char,
    pub meth: Option<PyCFunction>,
    pub flags: c_int,
    pub self_obj: *mut RawPyObject, // The module or object this is bound to
}

type PyCFunctionObject = PyObjectWithData<CFunctionData>;

/// Create a PyCFunction wrapping a C method definition.
pub unsafe fn create_cfunction(
    name: *const c_char,
    meth: Option<PyCFunction>,
    flags: c_int,
    self_obj: *mut RawPyObject,
) -> *mut RawPyObject {
    if !self_obj.is_null() {
        (*self_obj).incref();
    }
    let obj = PyObjectWithData::alloc(
        &mut CFUNCTION_TYPE,
        CFunctionData {
            name,
            meth,
            flags,
            self_obj,
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
    ml: *mut crate::object::typeobj::PyMethodDef,
    self_obj: *mut RawPyObject,
    module: *mut RawPyObject,
) -> *mut RawPyObject {
    if ml.is_null() {
        return ptr::null_mut();
    }
    create_cfunction((*ml).ml_name, (*ml).ml_meth, (*ml).ml_flags, self_obj)
}

/// PyCFunction_New
#[no_mangle]
pub unsafe extern "C" fn PyCFunction_New(
    ml: *mut crate::object::typeobj::PyMethodDef,
    self_obj: *mut RawPyObject,
) -> *mut RawPyObject {
    PyCFunction_NewEx(ml, self_obj, ptr::null_mut())
}
