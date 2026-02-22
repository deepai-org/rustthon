//! PyTypeObject — the type object that describes every Python type.
//!
//! This must match CPython's PyTypeObject layout exactly for C extensions.
//! CPython's full type object is enormous (~50 function pointer slots).
//! We implement the critical ones that extensions actually use.

use crate::object::pyobject::RawPyObject;
use crate::object::SyncUnsafeCell;
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
pub static PyType_Type: SyncUnsafeCell<RawPyTypeObject> = SyncUnsafeCell::new({
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"type\0".as_ptr() as *const _;
    tp.tp_basicsize = std::mem::size_of::<RawPyTypeObject>() as isize;
    tp
});

/// The base class: object.
#[no_mangle]
pub static PyBaseObject_Type: SyncUnsafeCell<RawPyTypeObject> = SyncUnsafeCell::new({
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"object\0".as_ptr() as *const _;
    tp.tp_basicsize = std::mem::size_of::<RawPyObject>() as isize;
    tp
});

// ─── Default slot function implementations ───

/// PyType_GenericAlloc — default allocator for new instances.
/// Allocates tp_basicsize + nitems * tp_itemsize bytes via calloc.
#[no_mangle]
pub unsafe extern "C" fn PyType_GenericAlloc(
    tp: *mut RawPyTypeObject,
    nitems: PySsizeT,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyType_GenericAlloc", || unsafe {
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
    })
}

/// PyType_GenericNew — default __new__ that calls tp_alloc.
#[no_mangle]
pub unsafe extern "C" fn PyType_GenericNew(
    tp: *mut RawPyTypeObject,
    _args: *mut RawPyObject,
    _kwds: *mut RawPyObject,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyType_GenericNew", || unsafe {
        if let Some(alloc) = (*tp).tp_alloc {
            alloc(tp, 0)
        } else {
            PyType_GenericAlloc(tp, 0)
        }
    })
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
    crate::ffi::panic_guard::guard_ptr("PyObject_GenericGetAttr", || unsafe {
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
    })
}

/// PyObject_GenericSetAttr — set attribute in instance or type dict.
#[no_mangle]
pub unsafe extern "C" fn PyObject_GenericSetAttr(
    obj: *mut RawPyObject,
    name: *mut RawPyObject,
    value: *mut RawPyObject,
) -> c_int {
    crate::ffi::panic_guard::guard_int("PyObject_GenericSetAttr", || unsafe {
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
    })
}

// PyObject_Free is in runtime/memory.rs

// ─── Py_TPFLAGS_READY ───
pub const PY_TPFLAGS_READY: u64 = 1 << 12;

// ─── PyType_Ready — initialize a type object ───

/// PyType_Ready — called by C extensions to initialize their custom types.
/// Inherits slots from the base type, sets metaclass, creates tp_dict.
#[no_mangle]
pub unsafe extern "C" fn PyType_Ready(tp: *mut RawPyTypeObject) -> c_int {
    crate::ffi::panic_guard::guard_int("PyType_Ready", || unsafe {
        if tp.is_null() {
            return -1;
        }

        // Already initialized?
        if (*tp).tp_flags & PY_TPFLAGS_READY != 0 {
            return 0;
        }

        // 1. Set base type if not set
        if (*tp).tp_base.is_null() {
            (*tp).tp_base = PyBaseObject_Type.get();
        }

        // 2. Set metaclass if not set
        if (*tp).ob_base.ob_type.is_null() {
            (*tp).ob_base.ob_type = PyType_Type.get();
        }

        // 3. Ensure base is ready first
        let base = (*tp).tp_base;
        if !base.is_null() {
            // Check alignment before dereferencing
            let base_addr = base as usize;
            if base_addr % std::mem::align_of::<RawPyTypeObject>() != 0 {
                // Misaligned tp_base — fall back to PyBaseObject_Type
                (*tp).tp_base = PyBaseObject_Type.get();
            }
        }
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
                | PY_TPFLAGS_TYPE_SUBCLASS
            );
        }

        // 8. Set default flags
        (*tp).tp_flags |= PY_TPFLAGS_DEFAULT | PY_TPFLAGS_READY;

        // 9. Set immortal refcount on type object
        (*tp).ob_base.ob_refcnt = std::sync::atomic::AtomicIsize::new(isize::MAX / 2);

        0
    })
}

/// Initialize PyBaseObject_Type and PyType_Type with default slots.
/// Must be called early in init_types(), before any other type init.
pub fn init_base_types() {
    unsafe {
    // PyBaseObject_Type gets default slot implementations
    (*PyBaseObject_Type.get()).tp_alloc = Some(PyType_GenericAlloc);
    (*PyBaseObject_Type.get()).tp_new = Some(PyType_GenericNew);
    (*PyBaseObject_Type.get()).tp_init = Some(default_init);
    (*PyBaseObject_Type.get()).tp_free = Some(crate::runtime::memory::PyObject_Free);
    (*PyBaseObject_Type.get()).tp_getattro = Some(PyObject_GenericGetAttr);
    (*PyBaseObject_Type.get()).tp_setattro = Some(PyObject_GenericSetAttr);
    (*PyBaseObject_Type.get()).tp_flags = PY_TPFLAGS_DEFAULT | PY_TPFLAGS_READY;
    (*PyBaseObject_Type.get()).ob_base.ob_type = PyType_Type.get();
    (*PyBaseObject_Type.get()).ob_base.ob_refcnt =
        std::sync::atomic::AtomicIsize::new(isize::MAX / 2);

    // PyType_Type — metaclass of all types
    (*PyType_Type.get()).tp_base = PyBaseObject_Type.get();
    (*PyType_Type.get()).tp_alloc = Some(PyType_GenericAlloc);
    (*PyType_Type.get()).tp_new = Some(PyType_GenericNew);
    (*PyType_Type.get()).tp_init = Some(default_init);
    (*PyType_Type.get()).tp_free = Some(crate::runtime::memory::PyObject_Free);
    (*PyType_Type.get()).tp_getattro = Some(PyObject_GenericGetAttr);
    (*PyType_Type.get()).tp_setattro = Some(PyObject_GenericSetAttr);
    (*PyType_Type.get()).tp_flags = PY_TPFLAGS_DEFAULT | PY_TPFLAGS_READY | PY_TPFLAGS_TYPE_SUBCLASS;
    (*PyType_Type.get()).ob_base.ob_type = PyType_Type.get(); // type's type is type
    (*PyType_Type.get()).ob_base.ob_refcnt =
        std::sync::atomic::AtomicIsize::new(isize::MAX / 2);
    }
}

/// PyType_IsSubtype — check if `a` is a subtype of `b`.
#[no_mangle]
pub unsafe extern "C" fn PyType_IsSubtype(
    a: *mut RawPyTypeObject,
    b: *mut RawPyTypeObject,
) -> c_int {
    crate::ffi::panic_guard::guard_int("PyType_IsSubtype", || unsafe {
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
    })
}

/// PyType_GetFlags — return tp_flags of a type (stable ABI).
#[no_mangle]
pub unsafe extern "C" fn PyType_GetFlags(tp: *mut RawPyTypeObject) -> u64 {
    crate::ffi::panic_guard::guard_u64("PyType_GetFlags", || unsafe {
        if tp.is_null() {
            return 0;
        }
        (*tp).tp_flags
    })
}

/// Slot IDs for PyType_GetSlot / PyType_FromSpec.
/// Values from CPython 3.11 Include/typeslots.h — these are part of the stable ABI.
const PY_BF_GETBUFFER: c_int = 1;
const PY_BF_RELEASEBUFFER: c_int = 2;
const PY_MP_ASS_SUBSCRIPT: c_int = 3;
const PY_MP_LENGTH: c_int = 4;
const PY_MP_SUBSCRIPT: c_int = 5;
const PY_NB_ADD: c_int = 7;
const PY_NB_BOOL: c_int = 9;
const PY_NB_FLOAT: c_int = 11;
const PY_NB_INDEX: c_int = 13;
const PY_NB_INT: c_int = 26;
const PY_NB_MULTIPLY: c_int = 29;
const PY_NB_SUBTRACT: c_int = 36;
const PY_SQ_ITEM: c_int = 44;
const PY_SQ_LENGTH: c_int = 45;
const PY_TP_ALLOC: c_int = 47;
const PY_TP_BASE: c_int = 48;
// const PY_TP_BASES: c_int = 49;
const PY_TP_CALL: c_int = 50;
const PY_TP_CLEAR: c_int = 51;
const PY_TP_DEALLOC: c_int = 52;
// const PY_TP_DEL: c_int = 53;
const PY_TP_DESCR_GET: c_int = 54;
const PY_TP_DESCR_SET: c_int = 55;
const PY_TP_DOC: c_int = 56;
const PY_TP_GETATTR: c_int = 57;
const PY_TP_GETATTRO: c_int = 58;
const PY_TP_HASH: c_int = 59;
const PY_TP_INIT: c_int = 60;
// const PY_TP_IS_GC: c_int = 61;
const PY_TP_ITER: c_int = 62;
const PY_TP_ITERNEXT: c_int = 63;
const PY_TP_METHODS: c_int = 64;
const PY_TP_NEW: c_int = 65;
const PY_TP_REPR: c_int = 66;
const PY_TP_RICHCOMPARE: c_int = 67;
const PY_TP_SETATTR: c_int = 68;
const PY_TP_SETATTRO: c_int = 69;
const PY_TP_STR: c_int = 70;
const PY_TP_TRAVERSE: c_int = 71;
const PY_TP_MEMBERS: c_int = 72;
const PY_TP_GETSET: c_int = 73;
const PY_TP_FREE: c_int = 74;
const PY_TP_FINALIZE: c_int = 80;

/// PyType_GetSlot — get a slot function from a type (stable ABI).
#[no_mangle]
pub unsafe extern "C" fn PyType_GetSlot(
    tp: *mut RawPyTypeObject,
    slot: c_int,
) -> *mut c_void {
    crate::ffi::panic_guard::guard_ptr("PyType_GetSlot", || unsafe {
        if tp.is_null() {
            return ptr::null_mut();
        }
        match slot {
            PY_TP_DEALLOC => (*tp).tp_dealloc.map_or(ptr::null_mut(), |f| f as *mut c_void),
            PY_TP_REPR => (*tp).tp_repr.map_or(ptr::null_mut(), |f| f as *mut c_void),
            PY_TP_HASH => (*tp).tp_hash.map_or(ptr::null_mut(), |f| f as *mut c_void),
            PY_TP_CALL => (*tp).tp_call.map_or(ptr::null_mut(), |f| f as *mut c_void),
            PY_TP_STR => (*tp).tp_str.map_or(ptr::null_mut(), |f| f as *mut c_void),
            PY_TP_GETATTRO => (*tp).tp_getattro.map_or(ptr::null_mut(), |f| f as *mut c_void),
            PY_TP_SETATTRO => (*tp).tp_setattro.map_or(ptr::null_mut(), |f| f as *mut c_void),
            PY_TP_TRAVERSE => (*tp).tp_traverse.map_or(ptr::null_mut(), |f| f as *mut c_void),
            PY_TP_CLEAR => (*tp).tp_clear.map_or(ptr::null_mut(), |f| f as *mut c_void),
            PY_TP_RICHCOMPARE => (*tp).tp_richcompare.map_or(ptr::null_mut(), |f| f as *mut c_void),
            PY_TP_ITER => (*tp).tp_iter.map_or(ptr::null_mut(), |f| f as *mut c_void),
            PY_TP_ITERNEXT => (*tp).tp_iternext.map_or(ptr::null_mut(), |f| f as *mut c_void),
            PY_TP_METHODS => (*tp).tp_methods as *mut c_void,
            PY_TP_MEMBERS => (*tp).tp_members as *mut c_void,
            PY_TP_GETSET => (*tp).tp_getset as *mut c_void,
            PY_TP_BASE => (*tp).tp_base as *mut c_void,
            PY_TP_INIT => (*tp).tp_init.map_or(ptr::null_mut(), |f| f as *mut c_void),
            PY_TP_ALLOC => (*tp).tp_alloc.map_or(ptr::null_mut(), |f| f as *mut c_void),
            PY_TP_NEW => (*tp).tp_new.map_or(ptr::null_mut(), |f| f as *mut c_void),
            PY_TP_FREE => (*tp).tp_free.map_or(ptr::null_mut(), |f| f as *mut c_void),
            PY_TP_FINALIZE => (*tp).tp_finalize.map_or(ptr::null_mut(), |f| f as *mut c_void),
            PY_TP_DOC => (*tp).tp_doc as *mut c_void,
            _ => ptr::null_mut(),
        }
    })
}

/// PyType_Spec slot entry, matching CPython.
#[repr(C)]
pub struct PyType_Slot {
    pub slot: c_int,
    pub pfunc: *mut c_void,
}

/// PyType_Spec, matching CPython.
#[repr(C)]
pub struct PyType_Spec {
    pub name: *const c_char,
    pub basicsize: c_int,
    pub itemsize: c_int,
    pub flags: u32,
    pub slots: *mut PyType_Slot,
}

/// PyType_FromModuleAndSpec — create a type from a module and spec (stable ABI).
/// This is how PyO3 and Cython create heap types.
#[no_mangle]
pub unsafe extern "C" fn PyType_FromModuleAndSpec(
    module: *mut RawPyObject,
    spec: *mut PyType_Spec,
    bases: *mut RawPyObject,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyType_FromModuleAndSpec", || unsafe {
        if spec.is_null() {
            return ptr::null_mut();
        }

        // Allocate a new type object
        let tp = libc::calloc(1, std::mem::size_of::<RawPyTypeObject>()) as *mut RawPyTypeObject;
        if tp.is_null() {
            return ptr::null_mut();
        }
        std::ptr::write(tp, RawPyTypeObject::zeroed());

        (*tp).tp_name = (*spec).name;
        (*tp).tp_basicsize = (*spec).basicsize as PySsizeT;
        (*tp).tp_itemsize = (*spec).itemsize as PySsizeT;
        (*tp).tp_flags = (*spec).flags as u64;
        (*tp).ob_base.ob_type = PyType_Type.get();
        (*tp).ob_base.ob_refcnt = std::sync::atomic::AtomicIsize::new(1);

        // Process slots
        if !(*spec).slots.is_null() {
            let mut slot_ptr = (*spec).slots;
            while (*slot_ptr).slot != 0 || !(*slot_ptr).pfunc.is_null() {
                let slot_id = (*slot_ptr).slot;
                let pfunc = (*slot_ptr).pfunc;
                if slot_id == 0 && pfunc.is_null() {
                    break;
                }
                match slot_id {
                    PY_TP_DEALLOC => (*tp).tp_dealloc = Some(std::mem::transmute(pfunc)),
                    PY_TP_REPR => (*tp).tp_repr = Some(std::mem::transmute(pfunc)),
                    PY_TP_HASH => (*tp).tp_hash = Some(std::mem::transmute(pfunc)),
                    PY_TP_CALL => (*tp).tp_call = Some(std::mem::transmute(pfunc)),
                    PY_TP_STR => (*tp).tp_str = Some(std::mem::transmute(pfunc)),
                    PY_TP_GETATTRO => (*tp).tp_getattro = Some(std::mem::transmute(pfunc)),
                    PY_TP_SETATTRO => (*tp).tp_setattro = Some(std::mem::transmute(pfunc)),
                    PY_TP_DOC => (*tp).tp_doc = pfunc as *const c_char,
                    PY_TP_TRAVERSE => (*tp).tp_traverse = Some(std::mem::transmute(pfunc)),
                    PY_TP_CLEAR => (*tp).tp_clear = Some(std::mem::transmute(pfunc)),
                    PY_TP_RICHCOMPARE => (*tp).tp_richcompare = Some(std::mem::transmute(pfunc)),
                    PY_TP_ITER => (*tp).tp_iter = Some(std::mem::transmute(pfunc)),
                    PY_TP_ITERNEXT => (*tp).tp_iternext = Some(std::mem::transmute(pfunc)),
                    PY_TP_METHODS => (*tp).tp_methods = pfunc as *mut PyMethodDef,
                    PY_TP_MEMBERS => (*tp).tp_members = pfunc as *mut PyMemberDef,
                    PY_TP_GETSET => (*tp).tp_getset = pfunc as *mut PyGetSetDef,
                    PY_TP_INIT => (*tp).tp_init = Some(std::mem::transmute(pfunc)),
                    PY_TP_ALLOC => (*tp).tp_alloc = Some(std::mem::transmute(pfunc)),
                    PY_TP_NEW => (*tp).tp_new = Some(std::mem::transmute(pfunc)),
                    PY_TP_FREE => (*tp).tp_free = Some(std::mem::transmute(pfunc)),
                    PY_TP_FINALIZE => (*tp).tp_finalize = Some(std::mem::transmute(pfunc)),
                    PY_TP_DESCR_GET => (*tp).tp_descr_get = Some(std::mem::transmute(pfunc)),
                    PY_TP_BASE => { /* handled separately below */ },
                    _ => {} // Unknown slot, ignore
                }
                slot_ptr = slot_ptr.add(1);
            }
        }

        // Handle Py_tp_base from slots (if specified)
        if !(*spec).slots.is_null() {
            let mut slot_ptr2 = (*spec).slots;
            while (*slot_ptr2).slot != 0 || !(*slot_ptr2).pfunc.is_null() {
                if (*slot_ptr2).slot == PY_TP_BASE && !(*slot_ptr2).pfunc.is_null() {
                    (*tp).tp_base = (*slot_ptr2).pfunc as *mut RawPyTypeObject;
                }
                if (*slot_ptr2).slot == 0 && (*slot_ptr2).pfunc.is_null() {
                    break;
                }
                slot_ptr2 = slot_ptr2.add(1);
            }
        }

        // Handle bases argument — extract primary base from tuple
        if (*tp).tp_base.is_null() && !bases.is_null() {
            if crate::types::tuple::PyTuple_Check(bases) != 0 {
                let size = crate::types::tuple::PyTuple_Size(bases);
                if size > 0 {
                    let first = crate::types::tuple::PyTuple_GetItem(bases, 0);
                    if !first.is_null() {
                        (*tp).tp_base = first as *mut RawPyTypeObject;
                    }
                }
            } else {
                // Single type object passed directly
                (*tp).tp_base = bases as *mut RawPyTypeObject;
            }
        }

        // Store bases tuple on the type
        if !bases.is_null() && (*tp).tp_bases.is_null() {
            (*bases).incref();
            (*tp).tp_bases = bases;
        }

        // Call PyType_Ready to finalize
        let ret = PyType_Ready(tp);
        if ret < 0 {
            libc::free(tp as *mut c_void);
            return ptr::null_mut();
        }

        tp as *mut RawPyObject
    })
}

/// PyType_FromSpecWithBases — create a type from a spec with explicit bases.
/// This is what Cython calls for CPython < 3.12.
#[no_mangle]
pub unsafe extern "C" fn PyType_FromSpecWithBases(
    spec: *mut PyType_Spec,
    bases: *mut RawPyObject,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyType_FromSpecWithBases", || unsafe {
        if spec.is_null() {
            return ptr::null_mut();
        }

        // Extract the primary base type from bases
        let mut base_type: *mut RawPyTypeObject = ptr::null_mut();
        if !bases.is_null() {
            // bases can be a tuple of types, or a single type
            if crate::types::tuple::PyTuple_Check(bases) != 0 {
                let size = crate::types::tuple::PyTuple_Size(bases);
                if size > 0 {
                    let first = crate::types::tuple::PyTuple_GetItem(bases, 0);
                    if !first.is_null() {
                        base_type = first as *mut RawPyTypeObject;
                    }
                }
            } else {
                // Single type object passed directly
                base_type = bases as *mut RawPyTypeObject;
            }
        }

        // Allocate a new type object
        let tp = libc::calloc(1, std::mem::size_of::<RawPyTypeObject>()) as *mut RawPyTypeObject;
        if tp.is_null() {
            return ptr::null_mut();
        }
        std::ptr::write(tp, RawPyTypeObject::zeroed());

        (*tp).tp_name = (*spec).name;
        (*tp).tp_basicsize = (*spec).basicsize as PySsizeT;
        (*tp).tp_itemsize = (*spec).itemsize as PySsizeT;
        (*tp).tp_flags = (*spec).flags as u64;
        (*tp).ob_base.ob_type = PyType_Type.get();
        (*tp).ob_base.ob_refcnt = std::sync::atomic::AtomicIsize::new(1);

        // Set base type if extracted from bases
        if !base_type.is_null() {
            (*tp).tp_base = base_type;
        }

        // Process slots
        if !(*spec).slots.is_null() {
            let mut slot_ptr = (*spec).slots;
            while (*slot_ptr).slot != 0 || !(*slot_ptr).pfunc.is_null() {
                let slot_id = (*slot_ptr).slot;
                let pfunc = (*slot_ptr).pfunc;
                if slot_id == 0 && pfunc.is_null() {
                    break;
                }
                match slot_id {
                    PY_TP_DEALLOC => (*tp).tp_dealloc = Some(std::mem::transmute(pfunc)),
                    PY_TP_REPR => (*tp).tp_repr = Some(std::mem::transmute(pfunc)),
                    PY_TP_HASH => (*tp).tp_hash = Some(std::mem::transmute(pfunc)),
                    PY_TP_CALL => (*tp).tp_call = Some(std::mem::transmute(pfunc)),
                    PY_TP_STR => (*tp).tp_str = Some(std::mem::transmute(pfunc)),
                    PY_TP_GETATTRO => (*tp).tp_getattro = Some(std::mem::transmute(pfunc)),
                    PY_TP_SETATTRO => (*tp).tp_setattro = Some(std::mem::transmute(pfunc)),
                    PY_TP_DOC => (*tp).tp_doc = pfunc as *const c_char,
                    PY_TP_TRAVERSE => (*tp).tp_traverse = Some(std::mem::transmute(pfunc)),
                    PY_TP_CLEAR => (*tp).tp_clear = Some(std::mem::transmute(pfunc)),
                    PY_TP_RICHCOMPARE => (*tp).tp_richcompare = Some(std::mem::transmute(pfunc)),
                    PY_TP_ITER => (*tp).tp_iter = Some(std::mem::transmute(pfunc)),
                    PY_TP_ITERNEXT => (*tp).tp_iternext = Some(std::mem::transmute(pfunc)),
                    PY_TP_METHODS => (*tp).tp_methods = pfunc as *mut PyMethodDef,
                    PY_TP_MEMBERS => (*tp).tp_members = pfunc as *mut PyMemberDef,
                    PY_TP_GETSET => (*tp).tp_getset = pfunc as *mut PyGetSetDef,
                    PY_TP_INIT => (*tp).tp_init = Some(std::mem::transmute(pfunc)),
                    PY_TP_ALLOC => (*tp).tp_alloc = Some(std::mem::transmute(pfunc)),
                    PY_TP_NEW => (*tp).tp_new = Some(std::mem::transmute(pfunc)),
                    PY_TP_FREE => (*tp).tp_free = Some(std::mem::transmute(pfunc)),
                    PY_TP_FINALIZE => (*tp).tp_finalize = Some(std::mem::transmute(pfunc)),
                    PY_TP_DESCR_GET => (*tp).tp_descr_get = Some(std::mem::transmute(pfunc)),
                    PY_TP_BASE => {
                        // Override base type from slot
                        (*tp).tp_base = pfunc as *mut RawPyTypeObject;
                    },
                    _ => {} // Unknown slot, ignore
                }
                slot_ptr = slot_ptr.add(1);
            }
        }

        // Store bases tuple on the type
        if !bases.is_null() {
            (*bases).incref();
            (*tp).tp_bases = bases;
        }

        // Call PyType_Ready to finalize
        let ret = PyType_Ready(tp);
        if ret < 0 {
            libc::free(tp as *mut c_void);
            return ptr::null_mut();
        }

        tp as *mut RawPyObject
    })
}

/// PyType_FromSpec — create a type from a spec (no bases).
#[no_mangle]
pub unsafe extern "C" fn PyType_FromSpec(
    spec: *mut PyType_Spec,
) -> *mut RawPyObject {
    crate::ffi::panic_guard::guard_ptr("PyType_FromSpec", || unsafe {
        PyType_FromSpecWithBases(spec, ptr::null_mut())
    })
}

/// PyType_Modified — notify that a type's dict has been modified.
/// No-op in our implementation (no method caches to invalidate).
#[no_mangle]
pub unsafe extern "C" fn PyType_Modified(_tp: *mut RawPyTypeObject) {
    // No-op: we don't have method resolution caches to invalidate
}
