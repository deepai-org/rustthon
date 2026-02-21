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
