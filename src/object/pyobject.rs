//! Core PyObject implementation matching CPython's exact memory layout.
//!
//! In CPython, every object starts with:
//!   - Py_ssize_t ob_refcnt
//!   - PyTypeObject *ob_type
//!
//! For GC-tracked objects (PyObject_GC_*), there's also a GC header
//! prepended before the object.

use std::marker::PhantomData;
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicIsize, Ordering};

use crate::object::typeobj::RawPyTypeObject;
use crate::runtime::gil::Python;
use crate::runtime::pyerr::{PyErr, PyResult};

/// The raw C-compatible PyObject header.
/// This MUST match CPython's PyObject layout exactly.
///
/// CPython layout (64-bit):
///   struct _object {
///       Py_ssize_t ob_refcnt;        // 8 bytes
///       PyTypeObject *ob_type;       // 8 bytes
///   };
#[repr(C)]
pub struct RawPyObject {
    pub ob_refcnt: AtomicIsize,
    pub ob_type: *mut RawPyTypeObject,
}

/// Variable-size object header (for str, list, tuple, etc.).
/// Matches CPython's PyVarObject.
///
///   struct PyVarObject {
///       PyObject ob_base;
///       Py_ssize_t ob_size;
///   };
#[repr(C)]
pub struct RawPyVarObject {
    pub ob_base: RawPyObject,
    pub ob_size: isize,
}

/// GC header prepended before GC-tracked objects.
/// Matches CPython 3.8+ PyGC_Head (16 bytes on 64-bit).
///
/// In CPython 3.8+, gc_refs was removed as a dedicated word and its bits
/// are packed into the lower alignment bits of gc_prev. This makes the
/// header exactly 2 words (16 bytes) instead of the pre-3.8 3-word layout.
///
/// C extensions compiled against 3.11 headers do `((PyGC_Head*)obj) - 1`
/// which subtracts exactly 16 bytes. We MUST match this.
#[repr(C)]
pub struct PyGCHead {
    /// Pointer to next object in GC list (or 0)
    pub gc_next: usize,
    /// Pointer to prev object in GC list.
    /// Lower bits hold gc_refs state flags (masked by alignment).
    pub gc_prev: usize,
}
// Static assertion: PyGC_Head must be exactly 16 bytes
const _: () = assert!(std::mem::size_of::<PyGCHead>() == 16);

unsafe impl Send for RawPyObject {}
unsafe impl Sync for RawPyObject {}

impl RawPyObject {
    /// Create a new RawPyObject with refcount 1 and the given type.
    pub fn new(tp: *mut RawPyTypeObject) -> Self {
        RawPyObject {
            ob_refcnt: AtomicIsize::new(1),
            ob_type: tp,
        }
    }

    #[inline]
    pub fn incref(&self) {
        self.ob_refcnt.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn decref(&self) -> isize {
        let prev = self.ob_refcnt.fetch_sub(1, Ordering::Release);
        if prev == 1 {
            std::sync::atomic::fence(Ordering::Acquire);
        }
        prev - 1
    }

    #[inline]
    pub fn refcnt(&self) -> isize {
        self.ob_refcnt.load(Ordering::Relaxed)
    }
}

// ─── AsPyPointer trait ───

/// Trait for anything that provides a raw PyObject pointer.
/// Implemented by `PyObjectRef`, `&PyObjectRef`, and `BorrowedPyObject<'py>`.
/// Enables safe_api functions to accept both owned and borrowed object references.
pub trait AsPyPointer {
    fn as_raw(&self) -> *mut RawPyObject;

    fn get_type(&self) -> *mut RawPyTypeObject {
        unsafe { (*self.as_raw()).ob_type }
    }
}

// ─── PyObjectRef — owned, RAII smart pointer ───

/// A safe, reference-counted wrapper around a raw PyObject pointer.
/// This is what our Rust-side code uses to hold Python objects.
///
/// Uses `NonNull` internally for niche optimization: `Option<PyObjectRef>`
/// is pointer-sized.
///
/// When cloned, it increments the reference count.
/// When dropped, it decrements and potentially frees (GIL-aware).
#[derive(Debug)]
pub struct PyObjectRef {
    ptr: NonNull<RawPyObject>,
}

// PyObjectRef is Send + Sync because we manage thread safety through the GIL
unsafe impl Send for PyObjectRef {}
unsafe impl Sync for PyObjectRef {}

impl PyObjectRef {
    /// Create a new PyObjectRef from a raw pointer.
    /// Takes ownership of one reference (does NOT incref).
    ///
    /// # Safety
    /// The pointer must point to a valid, initialized RawPyObject
    /// with refcount >= 1, and must not be null.
    pub unsafe fn from_raw(ptr: *mut RawPyObject) -> Self {
        debug_assert!(!ptr.is_null());
        PyObjectRef {
            ptr: NonNull::new_unchecked(ptr),
        }
    }

    /// Create a new PyObjectRef by borrowing (increfs).
    ///
    /// # Safety
    /// The pointer must point to a valid RawPyObject and must not be null.
    pub unsafe fn borrow_raw(ptr: *mut RawPyObject) -> Self {
        debug_assert!(!ptr.is_null());
        (*ptr).incref();
        PyObjectRef {
            ptr: NonNull::new_unchecked(ptr),
        }
    }

    /// Use when the C API returns a **NEW** reference that we now own.
    /// `PyObjectRef` will decref on Drop.
    ///
    /// If the pointer is null, fetches and returns the pending CPython exception.
    ///
    /// Use for: `PyObject_GetAttrString`, `PyObject_Call`, `PyObject_Repr`,
    /// `PyList_New`, `PyTuple_New`, `PyDict_New`, `PyLong_FromLong`,
    /// `PyFloat_FromDouble`, `PyUnicode_FromString`, `PyImport_ImportModule`, etc.
    pub fn steal_or_err(ptr: *mut RawPyObject) -> PyResult {
        NonNull::new(ptr)
            .map(|nn| PyObjectRef { ptr: nn })
            .ok_or_else(PyErr::fetch)
    }

    /// Use when the C API returns a **BORROWED** reference that the container still owns.
    /// We immediately incref so our `PyObjectRef` safely owns an independent reference.
    ///
    /// If the pointer is null, fetches and returns the pending CPython exception.
    ///
    /// Use for: `PyTuple_GetItem`, `PyList_GetItem`, `PyDict_GetItem`,
    /// `PyDict_GetItemString`, etc.
    pub fn borrow_or_err(ptr: *mut RawPyObject) -> PyResult {
        if ptr.is_null() {
            Err(PyErr::fetch())
        } else {
            Ok(unsafe { Self::borrow_raw(ptr) })
        }
    }

    /// Get the raw pointer without affecting refcount.
    pub fn as_raw(&self) -> *mut RawPyObject {
        self.ptr.as_ptr()
    }

    /// Consume this ref and return the raw pointer without decrementing.
    /// Caller takes ownership of the reference.
    pub fn into_raw(self) -> *mut RawPyObject {
        let ptr = self.ptr.as_ptr();
        std::mem::forget(self);
        ptr
    }

    /// Get the type object of this Python object.
    pub fn get_type(&self) -> *mut RawPyTypeObject {
        unsafe { (*self.ptr.as_ptr()).ob_type }
    }

    /// Check if this pointer is null. Always false since we use NonNull.
    /// Kept for backward compatibility but deprecated — prefer pattern matching.
    pub fn is_null(&self) -> bool {
        false // NonNull is never null
    }

    pub fn refcnt(&self) -> isize {
        unsafe { (*self.ptr.as_ptr()).refcnt() }
    }
}

impl AsPyPointer for PyObjectRef {
    fn as_raw(&self) -> *mut RawPyObject {
        self.ptr.as_ptr()
    }
}

impl AsPyPointer for &PyObjectRef {
    fn as_raw(&self) -> *mut RawPyObject {
        self.ptr.as_ptr()
    }
}

impl Clone for PyObjectRef {
    fn clone(&self) -> Self {
        unsafe {
            self.ptr.as_ref().incref();
        }
        PyObjectRef { ptr: self.ptr }
    }
}

impl Drop for PyObjectRef {
    fn drop(&mut self) {
        if crate::runtime::gil::gil_held() {
            // Fast path: GIL held, decref immediately
            unsafe {
                let new_refcnt = self.ptr.as_ref().decref();
                if new_refcnt == 0 {
                    dealloc_object(self.ptr.as_ptr());
                }
            }
        } else {
            // Slow path: GIL released, queue for later decref
            crate::runtime::gil::queue_decref(self.ptr.as_ptr());
        }
    }
}

// ─── BorrowedPyObject<'py> — zero-cost borrowed reference ───

/// A borrowed reference to a Python object, tied to the GIL lifetime.
///
/// Does **NOT** implement `Drop` — no incref or decref. Zero atomic operations.
/// The GIL lifetime `'py` guarantees the object stays alive (the container that
/// owns the real reference will not be collected while the GIL is held).
///
/// Use this for read-only access (type checks, value extraction) to avoid
/// paying the atomic cost of incref+decref.
#[derive(Copy, Clone, Debug)]
pub struct BorrowedPyObject<'py> {
    ptr: NonNull<RawPyObject>,
    _marker: PhantomData<Python<'py>>,
}

impl<'py> BorrowedPyObject<'py> {
    /// Create a borrowed reference from a raw pointer.
    ///
    /// # Safety
    /// The pointer must be valid for the duration of the GIL scope `'py`.
    pub unsafe fn from_raw(ptr: *mut RawPyObject) -> Self {
        debug_assert!(!ptr.is_null());
        BorrowedPyObject {
            ptr: NonNull::new_unchecked(ptr),
            _marker: PhantomData,
        }
    }

    /// Create from a raw pointer, returning Err if null.
    pub fn from_raw_or_err(ptr: *mut RawPyObject) -> Result<Self, PyErr> {
        NonNull::new(ptr)
            .map(|nn| BorrowedPyObject {
                ptr: nn,
                _marker: PhantomData,
            })
            .ok_or_else(PyErr::fetch)
    }

    /// Get the raw pointer.
    pub fn as_raw(&self) -> *mut RawPyObject {
        self.ptr.as_ptr()
    }

    /// Get the type object.
    pub fn get_type(&self) -> *mut RawPyTypeObject {
        unsafe { (*self.ptr.as_ptr()).ob_type }
    }

    /// Promote to an owned reference by increfing.
    pub fn to_owned(&self) -> PyObjectRef {
        unsafe { PyObjectRef::borrow_raw(self.ptr.as_ptr()) }
    }
}

impl<'py> AsPyPointer for BorrowedPyObject<'py> {
    fn as_raw(&self) -> *mut RawPyObject {
        self.ptr.as_ptr()
    }
}

// ─── Object deallocation ───

/// Deallocate a Python object whose refcount has reached zero.
pub(crate) unsafe fn dealloc_object(obj: *mut RawPyObject) {
    let tp = (*obj).ob_type;
    if !tp.is_null() {
        if let Some(dealloc) = (*tp).tp_dealloc {
            dealloc(obj);
            return;
        }
    }
    // Fallback: free via libc::free (all allocations go through libc::malloc)
    libc::free(obj as *mut libc::c_void);
}

// ─── Allocation helpers using libc::malloc (CPython-compatible) ───

/// Allocate a fixed-size Python object via libc::calloc.
/// Sets refcount=1, type pointer, zeroes everything else.
///
/// # Safety
/// `tp` must point to a valid, initialized RawPyTypeObject.
pub unsafe fn alloc_object(tp: *mut RawPyTypeObject, size: usize) -> *mut RawPyObject {
    let ptr = libc::calloc(1, size) as *mut RawPyObject;
    if ptr.is_null() {
        eprintln!("Fatal: out of memory allocating Python object");
        std::process::abort();
    }
    ptr::write(&mut (*ptr).ob_refcnt, AtomicIsize::new(1));
    (*ptr).ob_type = tp;
    ptr
}

/// Allocate a variable-size Python object via libc::calloc.
/// Total size = basicsize + nitems * itemsize.
/// Sets refcount=1, type pointer, ob_size, zeroes everything else.
///
/// # Safety
/// `tp` must point to a valid, initialized RawPyTypeObject.
pub unsafe fn alloc_var_object(
    tp: *mut RawPyTypeObject,
    nitems: isize,
    basicsize: usize,
    itemsize: usize,
) -> *mut RawPyVarObject {
    let total = basicsize + (nitems.max(0) as usize) * itemsize;
    let ptr = libc::calloc(1, total) as *mut RawPyVarObject;
    if ptr.is_null() {
        eprintln!("Fatal: out of memory allocating Python var object");
        std::process::abort();
    }
    ptr::write(&mut (*ptr).ob_base.ob_refcnt, AtomicIsize::new(1));
    (*ptr).ob_base.ob_type = tp;
    (*ptr).ob_size = nitems;
    ptr
}

/// A Python object that holds arbitrary Rust data alongside the PyObject header.
/// Used for non-built-in types (funcobject, moduleobject) that don't need
/// CPython-exact internal layout.
#[repr(C)]
pub struct PyObjectWithData<T> {
    pub ob_base: RawPyObject,
    pub data: T,
}

impl<T> PyObjectWithData<T> {
    /// Allocate a new PyObjectWithData on the heap via libc::malloc.
    pub fn alloc(tp: *mut RawPyTypeObject, data: T) -> *mut Self {
        unsafe {
            let size = std::mem::size_of::<Self>();
            let ptr = libc::calloc(1, size) as *mut Self;
            if ptr.is_null() {
                eprintln!("Fatal: out of memory allocating PyObjectWithData");
                std::process::abort();
            }
            ptr::write(
                &mut (*ptr).ob_base,
                RawPyObject::new(tp),
            );
            ptr::write(&mut (*ptr).data, data);
            ptr
        }
    }

    /// Get a reference to the data from a raw PyObject pointer.
    ///
    /// # Safety
    /// The pointer must actually point to a PyObjectWithData<T>.
    pub unsafe fn data_from_raw(obj: *mut RawPyObject) -> &'static T {
        let typed = obj as *mut PyObjectWithData<T>;
        &(*typed).data
    }

    /// Get a mutable reference to the data from a raw PyObject pointer.
    ///
    /// # Safety
    /// The pointer must actually point to a PyObjectWithData<T>.
    pub unsafe fn data_from_raw_mut(obj: *mut RawPyObject) -> &'static mut T {
        let typed = obj as *mut PyObjectWithData<T>;
        &mut (*typed).data
    }
}

/// Convenience alias for common object type used by the C API.
pub type PyObject = RawPyObject;
