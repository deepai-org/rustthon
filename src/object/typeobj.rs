//! PyTypeObject — the type object that describes every Python type.
//!
//! This must match CPython's PyTypeObject layout exactly for C extensions.
//! CPython's full type object is enormous (~50 function pointer slots).
//! We implement the critical ones that extensions actually use.

use crate::object::pyobject::RawPyObject;
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;

/// Py_ssize_t equivalent
pub type PySsizeT = isize;

// ─── Function pointer type aliases matching CPython ───

/// destructor: void (*)(PyObject *)
pub type Destructor = unsafe extern "C" fn(*mut RawPyObject);

/// getattrfunc: PyObject *(*)(PyObject *, char *)
pub type GetAttrFunc = unsafe extern "C" fn(*mut RawPyObject, *mut c_char) -> *mut RawPyObject;

/// setattrfunc: int (*)(PyObject *, char *, PyObject *)
pub type SetAttrFunc =
    unsafe extern "C" fn(*mut RawPyObject, *mut c_char, *mut RawPyObject) -> c_int;

/// reprfunc: PyObject *(*)(PyObject *)
pub type ReprFunc = unsafe extern "C" fn(*mut RawPyObject) -> *mut RawPyObject;

/// hashfunc: Py_hash_t (*)(PyObject *)
pub type HashFunc = unsafe extern "C" fn(*mut RawPyObject) -> isize;

/// ternaryfunc: PyObject *(*)(PyObject *, PyObject *, PyObject *)
pub type TernaryFunc =
    unsafe extern "C" fn(*mut RawPyObject, *mut RawPyObject, *mut RawPyObject) -> *mut RawPyObject;

/// binaryfunc: PyObject *(*)(PyObject *, PyObject *)
pub type BinaryFunc =
    unsafe extern "C" fn(*mut RawPyObject, *mut RawPyObject) -> *mut RawPyObject;

/// unaryfunc: PyObject *(*)(PyObject *)
pub type UnaryFunc = unsafe extern "C" fn(*mut RawPyObject) -> *mut RawPyObject;

/// inquiry: int (*)(PyObject *)
pub type Inquiry = unsafe extern "C" fn(*mut RawPyObject) -> c_int;

/// lenfunc: Py_ssize_t (*)(PyObject *)
pub type LenFunc = unsafe extern "C" fn(*mut RawPyObject) -> PySsizeT;

/// ssizeargfunc: PyObject *(*)(PyObject *, Py_ssize_t)
pub type SsizeArgFunc = unsafe extern "C" fn(*mut RawPyObject, PySsizeT) -> *mut RawPyObject;

/// ssizeobjargproc: int (*)(PyObject *, Py_ssize_t, PyObject *)
pub type SsizeObjArgProc =
    unsafe extern "C" fn(*mut RawPyObject, PySsizeT, *mut RawPyObject) -> c_int;

/// objobjargproc: int (*)(PyObject *, PyObject *, PyObject *)
pub type ObjObjArgProc =
    unsafe extern "C" fn(*mut RawPyObject, *mut RawPyObject, *mut RawPyObject) -> c_int;

/// objobjproc: int (*)(PyObject *, PyObject *)
pub type ObjObjProc = unsafe extern "C" fn(*mut RawPyObject, *mut RawPyObject) -> c_int;

/// getbufferproc: int (*)(PyObject *, Py_buffer *, int)
pub type GetBufferProc =
    unsafe extern "C" fn(*mut RawPyObject, *mut PyBufferRaw, c_int) -> c_int;

/// releasebufferproc: void (*)(PyObject *, Py_buffer *)
pub type ReleaseBufferProc = unsafe extern "C" fn(*mut RawPyObject, *mut PyBufferRaw);

/// getiterfunc: PyObject *(*)(PyObject *)
pub type GetIterFunc = unsafe extern "C" fn(*mut RawPyObject) -> *mut RawPyObject;

/// iternextfunc: PyObject *(*)(PyObject *)
pub type IterNextFunc = unsafe extern "C" fn(*mut RawPyObject) -> *mut RawPyObject;

/// richcmpfunc: PyObject *(*)(PyObject *, PyObject *, int)
pub type RichCmpFunc =
    unsafe extern "C" fn(*mut RawPyObject, *mut RawPyObject, c_int) -> *mut RawPyObject;

/// initproc: int (*)(PyObject *, PyObject *, PyObject *)
pub type InitProc =
    unsafe extern "C" fn(*mut RawPyObject, *mut RawPyObject, *mut RawPyObject) -> c_int;

/// allocfunc: PyObject *(*)(PyTypeObject *, Py_ssize_t)
pub type AllocFunc =
    unsafe extern "C" fn(*mut RawPyTypeObject, PySsizeT) -> *mut RawPyObject;

/// newfunc: PyObject *(*)(PyTypeObject *, PyObject *, PyObject *)
pub type NewFunc = unsafe extern "C" fn(
    *mut RawPyTypeObject,
    *mut RawPyObject,
    *mut RawPyObject,
) -> *mut RawPyObject;

/// freefunc: void (*)(void *)
pub type FreeFunc = unsafe extern "C" fn(*mut c_void);

/// PyCFunction: PyObject *(*)(PyObject *, PyObject *)
pub type PyCFunction =
    unsafe extern "C" fn(*mut RawPyObject, *mut RawPyObject) -> *mut RawPyObject;

/// PyCFunctionWithKeywords: PyObject *(*)(PyObject *, PyObject *, PyObject *)
pub type PyCFunctionWithKeywords = unsafe extern "C" fn(
    *mut RawPyObject,
    *mut RawPyObject,
    *mut RawPyObject,
) -> *mut RawPyObject;

// ─── Py_buffer (Buffer Protocol) ───

/// Raw Py_buffer struct matching CPython layout.
#[repr(C)]
pub struct PyBufferRaw {
    pub buf: *mut c_void,
    pub obj: *mut RawPyObject,
    pub len: PySsizeT,
    pub itemsize: PySsizeT,
    pub readonly: c_int,
    pub ndim: c_int,
    pub format: *mut c_char,
    pub shape: *mut PySsizeT,
    pub strides: *mut PySsizeT,
    pub suboffsets: *mut PySsizeT,
    pub internal: *mut c_void,
}

// ─── Number methods ───

#[repr(C)]
pub struct PyNumberMethods {
    pub nb_add: Option<BinaryFunc>,
    pub nb_subtract: Option<BinaryFunc>,
    pub nb_multiply: Option<BinaryFunc>,
    pub nb_remainder: Option<BinaryFunc>,
    pub nb_divmod: Option<BinaryFunc>,
    pub nb_power: Option<TernaryFunc>,
    pub nb_negative: Option<UnaryFunc>,
    pub nb_positive: Option<UnaryFunc>,
    pub nb_absolute: Option<UnaryFunc>,
    pub nb_bool: Option<Inquiry>,
    pub nb_invert: Option<UnaryFunc>,
    pub nb_lshift: Option<BinaryFunc>,
    pub nb_rshift: Option<BinaryFunc>,
    pub nb_and: Option<BinaryFunc>,
    pub nb_xor: Option<BinaryFunc>,
    pub nb_or: Option<BinaryFunc>,
    pub nb_int: Option<UnaryFunc>,
    pub nb_reserved: *mut c_void,
    pub nb_float: Option<UnaryFunc>,
    pub nb_inplace_add: Option<BinaryFunc>,
    pub nb_inplace_subtract: Option<BinaryFunc>,
    pub nb_inplace_multiply: Option<BinaryFunc>,
    pub nb_inplace_remainder: Option<BinaryFunc>,
    pub nb_inplace_power: Option<TernaryFunc>,
    pub nb_inplace_lshift: Option<BinaryFunc>,
    pub nb_inplace_rshift: Option<BinaryFunc>,
    pub nb_inplace_and: Option<BinaryFunc>,
    pub nb_inplace_xor: Option<BinaryFunc>,
    pub nb_inplace_or: Option<BinaryFunc>,
    pub nb_floor_divide: Option<BinaryFunc>,
    pub nb_true_divide: Option<BinaryFunc>,
    pub nb_inplace_floor_divide: Option<BinaryFunc>,
    pub nb_inplace_true_divide: Option<BinaryFunc>,
    pub nb_index: Option<UnaryFunc>,
    pub nb_matrix_multiply: Option<BinaryFunc>,
    pub nb_inplace_matrix_multiply: Option<BinaryFunc>,
}

// ─── Sequence methods ───

#[repr(C)]
pub struct PySequenceMethods {
    pub sq_length: Option<LenFunc>,
    pub sq_concat: Option<BinaryFunc>,
    pub sq_repeat: Option<SsizeArgFunc>,
    pub sq_item: Option<SsizeArgFunc>,
    pub was_sq_slice: *mut c_void,
    pub sq_ass_item: Option<SsizeObjArgProc>,
    pub was_sq_ass_slice: *mut c_void,
    pub sq_contains: Option<ObjObjProc>,
    pub sq_inplace_concat: Option<BinaryFunc>,
    pub sq_inplace_repeat: Option<SsizeArgFunc>,
}

// ─── Mapping methods ───

#[repr(C)]
pub struct PyMappingMethods {
    pub mp_length: Option<LenFunc>,
    pub mp_subscript: Option<BinaryFunc>,
    pub mp_ass_subscript: Option<ObjObjArgProc>,
}

// ─── Buffer methods ───

#[repr(C)]
pub struct PyBufferProcs {
    pub bf_getbuffer: Option<GetBufferProc>,
    pub bf_releasebuffer: Option<ReleaseBufferProc>,
}

// ─── Method definition (for C extension methods) ───

/// Matches CPython's PyMethodDef
#[repr(C)]
pub struct PyMethodDef {
    pub ml_name: *const c_char,
    pub ml_meth: Option<PyCFunction>,
    pub ml_flags: c_int,
    pub ml_doc: *const c_char,
}

// Method flags
pub const METH_VARARGS: c_int = 0x0001;
pub const METH_KEYWORDS: c_int = 0x0002;
pub const METH_NOARGS: c_int = 0x0004;
pub const METH_O: c_int = 0x0008;
pub const METH_CLASS: c_int = 0x0010;
pub const METH_STATIC: c_int = 0x0020;

// ─── Member definition ───

#[repr(C)]
pub struct PyMemberDef {
    pub name: *const c_char,
    pub type_code: c_int,
    pub offset: PySsizeT,
    pub flags: c_int,
    pub doc: *const c_char,
}

// ─── GetSet definition ───

pub type Getter = unsafe extern "C" fn(*mut RawPyObject, *mut c_void) -> *mut RawPyObject;
pub type Setter = unsafe extern "C" fn(*mut RawPyObject, *mut RawPyObject, *mut c_void) -> c_int;

#[repr(C)]
pub struct PyGetSetDef {
    pub name: *const c_char,
    pub get: Option<Getter>,
    pub set: Option<Setter>,
    pub doc: *const c_char,
    pub closure: *mut c_void,
}

// ─── Type flags (matching CPython) ───

pub const PY_TPFLAGS_DEFAULT: u64 = 0;
pub const PY_TPFLAGS_BASETYPE: u64 = 1 << 10;
pub const PY_TPFLAGS_HAVE_GC: u64 = 1 << 14;
pub const PY_TPFLAGS_HAVE_FINALIZE: u64 = 1 << 0;
pub const PY_TPFLAGS_LONG_SUBCLASS: u64 = 1 << 24;
pub const PY_TPFLAGS_LIST_SUBCLASS: u64 = 1 << 25;
pub const PY_TPFLAGS_TUPLE_SUBCLASS: u64 = 1 << 26;
pub const PY_TPFLAGS_BYTES_SUBCLASS: u64 = 1 << 27;
pub const PY_TPFLAGS_UNICODE_SUBCLASS: u64 = 1 << 28;
pub const PY_TPFLAGS_DICT_SUBCLASS: u64 = 1 << 29;
pub const PY_TPFLAGS_TYPE_SUBCLASS: u64 = 1 << 31;

// ─── The Big One: PyTypeObject ───

/// This is the full PyTypeObject matching CPython's layout.
/// C extensions cast PyObject* to PyTypeObject* and read these fields directly.
#[repr(C)]
pub struct RawPyTypeObject {
    // PyObject_VAR_HEAD
    pub ob_base: RawPyObject,
    pub ob_size: PySsizeT,

    // Type info
    pub tp_name: *const c_char,
    pub tp_basicsize: PySsizeT,
    pub tp_itemsize: PySsizeT,

    // Standard methods
    pub tp_dealloc: Option<Destructor>,
    pub tp_vectorcall_offset: PySsizeT,
    pub tp_getattr: Option<GetAttrFunc>,
    pub tp_setattr: Option<SetAttrFunc>,
    pub tp_as_async: *mut c_void, // PyAsyncMethods* - TODO
    pub tp_repr: Option<ReprFunc>,

    // Method suites
    pub tp_as_number: *mut PyNumberMethods,
    pub tp_as_sequence: *mut PySequenceMethods,
    pub tp_as_mapping: *mut PyMappingMethods,

    // More standard ops
    pub tp_hash: Option<HashFunc>,
    pub tp_call: Option<TernaryFunc>,
    pub tp_str: Option<ReprFunc>,
    pub tp_getattro: Option<BinaryFunc>,
    pub tp_setattro: Option<ObjObjArgProc>,

    // Buffer protocol
    pub tp_as_buffer: *mut PyBufferProcs,

    // Flags
    pub tp_flags: u64,

    // Documentation
    pub tp_doc: *const c_char,

    // GC traversal
    pub tp_traverse: Option<unsafe extern "C" fn(*mut RawPyObject, *mut c_void, *mut c_void) -> c_int>,
    pub tp_clear: Option<Inquiry>,

    // Rich comparison
    pub tp_richcompare: Option<RichCmpFunc>,

    // Weak reference support
    pub tp_weaklistoffset: PySsizeT,

    // Iterators
    pub tp_iter: Option<GetIterFunc>,
    pub tp_iternext: Option<IterNextFunc>,

    // Attribute descriptor / subclassing
    pub tp_methods: *mut PyMethodDef,
    pub tp_members: *mut PyMemberDef,
    pub tp_getset: *mut PyGetSetDef,
    pub tp_base: *mut RawPyTypeObject,
    pub tp_dict: *mut RawPyObject,
    pub tp_descr_get: Option<TernaryFunc>,
    pub tp_descr_set: Option<ObjObjArgProc>,
    pub tp_dictoffset: PySsizeT,
    pub tp_init: Option<InitProc>,
    pub tp_alloc: Option<AllocFunc>,
    pub tp_new: Option<NewFunc>,
    pub tp_free: Option<FreeFunc>,
    pub tp_is_gc: Option<Inquiry>,
    pub tp_bases: *mut RawPyObject,
    pub tp_mro: *mut RawPyObject,
    pub tp_cache: *mut RawPyObject,
    pub tp_subclasses: *mut RawPyObject,
    pub tp_weaklist: *mut RawPyObject,
    pub tp_del: Option<Destructor>,
    pub tp_version_tag: u32,
    pub tp_finalize: Option<Destructor>,
    pub tp_vectorcall: Option<unsafe extern "C" fn(
        *mut RawPyObject,
        *const *mut RawPyObject,
        usize,
        *mut RawPyObject,
    ) -> *mut RawPyObject>,
}

unsafe impl Send for RawPyTypeObject {}
unsafe impl Sync for RawPyTypeObject {}

impl RawPyTypeObject {
    /// Create a zeroed-out type object. Fields should be filled in after creation.
    pub const fn zeroed() -> Self {
        RawPyTypeObject {
            ob_base: RawPyObject {
                ob_refcnt: std::sync::atomic::AtomicIsize::new(1),
                ob_type: ptr::null_mut(),
            },
            ob_size: 0,
            tp_name: ptr::null(),
            tp_basicsize: 0,
            tp_itemsize: 0,
            tp_dealloc: None,
            tp_vectorcall_offset: 0,
            tp_getattr: None,
            tp_setattr: None,
            tp_as_async: ptr::null_mut(),
            tp_repr: None,
            tp_as_number: ptr::null_mut(),
            tp_as_sequence: ptr::null_mut(),
            tp_as_mapping: ptr::null_mut(),
            tp_hash: None,
            tp_call: None,
            tp_str: None,
            tp_getattro: None,
            tp_setattro: None,
            tp_as_buffer: ptr::null_mut(),
            tp_flags: 0,
            tp_doc: ptr::null(),
            tp_traverse: None,
            tp_clear: None,
            tp_richcompare: None,
            tp_weaklistoffset: 0,
            tp_iter: None,
            tp_iternext: None,
            tp_methods: ptr::null_mut(),
            tp_members: ptr::null_mut(),
            tp_getset: ptr::null_mut(),
            tp_base: ptr::null_mut(),
            tp_dict: ptr::null_mut(),
            tp_descr_get: None,
            tp_descr_set: None,
            tp_dictoffset: 0,
            tp_init: None,
            tp_alloc: None,
            tp_new: None,
            tp_free: None,
            tp_is_gc: None,
            tp_bases: ptr::null_mut(),
            tp_mro: ptr::null_mut(),
            tp_cache: ptr::null_mut(),
            tp_subclasses: ptr::null_mut(),
            tp_weaklist: ptr::null_mut(),
            tp_del: None,
            tp_version_tag: 0,
            tp_finalize: None,
            tp_vectorcall: None,
        }
    }
}

// ─── Metaclass and base type objects ───

/// The metaclass: type of all types.
#[no_mangle]
pub static mut PyType_Type: RawPyTypeObject = {
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"type\0".as_ptr() as *const _;
    tp.tp_basicsize = std::mem::size_of::<RawPyTypeObject>() as isize;
    tp
};

/// The base class: object.
#[no_mangle]
pub static mut PyBaseObject_Type: RawPyTypeObject = {
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"object\0".as_ptr() as *const _;
    tp.tp_basicsize = std::mem::size_of::<RawPyObject>() as isize;
    tp
};

// ─── Default slot function implementations ───

/// PyType_GenericAlloc — default allocator for new instances.
/// Allocates tp_basicsize + nitems * tp_itemsize bytes via calloc.
#[no_mangle]
pub unsafe extern "C" fn PyType_GenericAlloc(
    tp: *mut RawPyTypeObject,
    nitems: PySsizeT,
) -> *mut RawPyObject {
    let basic = (*tp).tp_basicsize as usize;
    let item = (*tp).tp_itemsize as usize;
    let total = basic + (nitems.max(0) as usize) * item;
    let obj = libc::calloc(1, total) as *mut RawPyObject;
    if obj.is_null() {
        eprintln!("Fatal: out of memory in PyType_GenericAlloc");
        std::process::abort();
    }
    std::ptr::write(
        &mut (*obj).ob_refcnt,
        std::sync::atomic::AtomicIsize::new(1),
    );
    (*obj).ob_type = tp;
    obj
}

/// PyType_GenericNew — default __new__ that calls tp_alloc.
#[no_mangle]
pub unsafe extern "C" fn PyType_GenericNew(
    tp: *mut RawPyTypeObject,
    _args: *mut RawPyObject,
    _kwds: *mut RawPyObject,
) -> *mut RawPyObject {
    if let Some(alloc) = (*tp).tp_alloc {
        alloc(tp, 0)
    } else {
        PyType_GenericAlloc(tp, 0)
    }
}

/// Default __init__ — no-op.
unsafe extern "C" fn default_init(
    _self: *mut RawPyObject,
    _args: *mut RawPyObject,
    _kwds: *mut RawPyObject,
) -> c_int {
    0
}

/// PyObject_GenericGetAttr — look up attribute in type's tp_dict.
#[no_mangle]
pub unsafe extern "C" fn PyObject_GenericGetAttr(
    obj: *mut RawPyObject,
    name: *mut RawPyObject,
) -> *mut RawPyObject {
    // Walk the type's MRO/tp_dict to find the attribute
    if obj.is_null() || name.is_null() {
        return ptr::null_mut();
    }
    let tp = (*obj).ob_type;
    if tp.is_null() {
        return ptr::null_mut();
    }
    // Check tp_dict of the type
    let dict = (*tp).tp_dict;
    if !dict.is_null() {
        let result = crate::types::dict::PyDict_GetItem(dict, name);
        if !result.is_null() {
            (*result).incref();
            return result;
        }
    }
    // Walk tp_base chain
    let mut base = (*tp).tp_base;
    while !base.is_null() {
        let bdict = (*base).tp_dict;
        if !bdict.is_null() {
            let result = crate::types::dict::PyDict_GetItem(bdict, name);
            if !result.is_null() {
                (*result).incref();
                return result;
            }
        }
        base = (*base).tp_base;
    }
    // Attribute not found — set AttributeError
    crate::runtime::error::PyErr_SetString(
        crate::runtime::error::_Rustthon_Exc_AttributeError(),
        b"attribute not found\0".as_ptr() as *const c_char,
    );
    ptr::null_mut()
}

/// PyObject_GenericSetAttr — set attribute in instance or type dict.
#[no_mangle]
pub unsafe extern "C" fn PyObject_GenericSetAttr(
    obj: *mut RawPyObject,
    name: *mut RawPyObject,
    value: *mut RawPyObject,
) -> c_int {
    // For now, set on the type's tp_dict (simplified — real CPython has
    // instance dicts, data descriptors, etc.)
    if obj.is_null() || name.is_null() {
        return -1;
    }
    let tp = (*obj).ob_type;
    if tp.is_null() {
        return -1;
    }
    let dict = (*tp).tp_dict;
    if dict.is_null() {
        return -1;
    }
    crate::types::dict::PyDict_SetItem(dict, name, value)
}

// PyObject_Free is in runtime/memory.rs

// ─── Py_TPFLAGS_READY ───
pub const PY_TPFLAGS_READY: u64 = 1 << 12;

// ─── PyType_Ready — initialize a type object ───

/// PyType_Ready — called by C extensions to initialize their custom types.
/// Inherits slots from the base type, sets metaclass, creates tp_dict.
#[no_mangle]
pub unsafe extern "C" fn PyType_Ready(tp: *mut RawPyTypeObject) -> c_int {
    if tp.is_null() {
        return -1;
    }

    // Already initialized?
    if (*tp).tp_flags & PY_TPFLAGS_READY != 0 {
        return 0;
    }

    // 1. Set base type if not set
    if (*tp).tp_base.is_null() {
        (*tp).tp_base = &mut PyBaseObject_Type;
    }

    // 2. Set metaclass if not set
    if (*tp).ob_base.ob_type.is_null() {
        (*tp).ob_base.ob_type = &mut PyType_Type;
    }

    // 3. Ensure base is ready first
    let base = (*tp).tp_base;
    if !base.is_null() && (*base).tp_flags & PY_TPFLAGS_READY == 0 {
        let ret = PyType_Ready(base);
        if ret < 0 {
            return ret;
        }
    }

    // 4. Inherit slots from base
    let base = (*tp).tp_base;
    if !base.is_null() {
        // Inherit basicsize if not set
        if (*tp).tp_basicsize == 0 {
            (*tp).tp_basicsize = (*base).tp_basicsize;
        }

        // Inherit function slots if null
        macro_rules! inherit_slot {
            ($slot:ident) => {
                if (*tp).$slot.is_none() && (*base).$slot.is_some() {
                    (*tp).$slot = (*base).$slot;
                }
            };
        }

        inherit_slot!(tp_dealloc);
        inherit_slot!(tp_repr);
        inherit_slot!(tp_hash);
        inherit_slot!(tp_call);
        inherit_slot!(tp_str);
        inherit_slot!(tp_getattro);
        inherit_slot!(tp_setattro);
        inherit_slot!(tp_richcompare);
        inherit_slot!(tp_iter);
        inherit_slot!(tp_iternext);
        inherit_slot!(tp_init);
        inherit_slot!(tp_alloc);
        inherit_slot!(tp_new);
        inherit_slot!(tp_free);
        inherit_slot!(tp_is_gc);
        inherit_slot!(tp_del);
        inherit_slot!(tp_finalize);
        inherit_slot!(tp_traverse);
        inherit_slot!(tp_clear);

        // Inherit pointer-based slots (null check)
        macro_rules! inherit_ptr_slot {
            ($slot:ident) => {
                if (*tp).$slot.is_null() && !(*base).$slot.is_null() {
                    (*tp).$slot = (*base).$slot;
                }
            };
        }

        inherit_ptr_slot!(tp_as_number);
        inherit_ptr_slot!(tp_as_sequence);
        inherit_ptr_slot!(tp_as_mapping);
        inherit_ptr_slot!(tp_as_buffer);
    }

    // 5. Initialize tp_dict if null
    if (*tp).tp_dict.is_null() {
        (*tp).tp_dict = crate::types::dict::PyDict_New();
    }

    // 6. Create tp_bases tuple if null
    if (*tp).tp_bases.is_null() && !(*tp).tp_base.is_null() {
        let bases = crate::types::tuple::PyTuple_New(1);
        let base_obj = (*tp).tp_base as *mut RawPyObject;
        (*base_obj).incref();
        crate::types::tuple::PyTuple_SetItem(bases, 0, base_obj);
        (*tp).tp_bases = bases;
    }

    // 7. Merge base flags (subtype bits)
    if !base.is_null() {
        (*tp).tp_flags |= (*base).tp_flags & (
            PY_TPFLAGS_LONG_SUBCLASS
            | PY_TPFLAGS_LIST_SUBCLASS
            | PY_TPFLAGS_TUPLE_SUBCLASS
            | PY_TPFLAGS_BYTES_SUBCLASS
            | PY_TPFLAGS_UNICODE_SUBCLASS
            | PY_TPFLAGS_DICT_SUBCLASS
        );
    }

    // 8. Set default flags
    (*tp).tp_flags |= PY_TPFLAGS_DEFAULT | PY_TPFLAGS_READY;

    // 9. Set immortal refcount on type object
    (*tp).ob_base.ob_refcnt = std::sync::atomic::AtomicIsize::new(isize::MAX / 2);

    0
}

/// Initialize PyBaseObject_Type and PyType_Type with default slots.
/// Must be called early in init_types(), before any other type init.
pub unsafe fn init_base_types() {
    // PyBaseObject_Type gets default slot implementations
    PyBaseObject_Type.tp_alloc = Some(PyType_GenericAlloc);
    PyBaseObject_Type.tp_new = Some(PyType_GenericNew);
    PyBaseObject_Type.tp_init = Some(default_init);
    PyBaseObject_Type.tp_free = Some(crate::runtime::memory::PyObject_Free);
    PyBaseObject_Type.tp_getattro = Some(PyObject_GenericGetAttr);
    PyBaseObject_Type.tp_setattro = Some(PyObject_GenericSetAttr);
    PyBaseObject_Type.tp_flags = PY_TPFLAGS_DEFAULT | PY_TPFLAGS_READY;
    PyBaseObject_Type.ob_base.ob_type = &mut PyType_Type;
    PyBaseObject_Type.ob_base.ob_refcnt =
        std::sync::atomic::AtomicIsize::new(isize::MAX / 2);

    // PyType_Type — metaclass of all types
    PyType_Type.tp_base = &mut PyBaseObject_Type;
    PyType_Type.tp_alloc = Some(PyType_GenericAlloc);
    PyType_Type.tp_new = Some(PyType_GenericNew);
    PyType_Type.tp_init = Some(default_init);
    PyType_Type.tp_free = Some(crate::runtime::memory::PyObject_Free);
    PyType_Type.tp_getattro = Some(PyObject_GenericGetAttr);
    PyType_Type.tp_setattro = Some(PyObject_GenericSetAttr);
    PyType_Type.tp_flags = PY_TPFLAGS_DEFAULT | PY_TPFLAGS_READY;
    PyType_Type.ob_base.ob_type = &mut PyType_Type; // type's type is type
    PyType_Type.ob_base.ob_refcnt =
        std::sync::atomic::AtomicIsize::new(isize::MAX / 2);
}

/// PyType_IsSubtype — check if `a` is a subtype of `b`.
#[no_mangle]
pub unsafe extern "C" fn PyType_IsSubtype(
    a: *mut RawPyTypeObject,
    b: *mut RawPyTypeObject,
) -> c_int {
    if a.is_null() || b.is_null() {
        return 0;
    }
    let mut tp = a;
    while !tp.is_null() {
        if tp == b {
            return 1;
        }
        tp = (*tp).tp_base;
    }
    0
}
