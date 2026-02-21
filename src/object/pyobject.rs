//! Core PyObject implementation matching CPython's exact memory layout.
//!
//! In CPython, every object starts with:
//!   - Py_ssize_t ob_refcnt
//!   - PyTypeObject *ob_type
//!
//! For GC-tracked objects (PyObject_GC_*), there's also a GC header
//! prepended before the object.

use std::ptr;
use std::sync::atomic::{AtomicIsize, Ordering};

use crate::object::typeobj::RawPyTypeObject;

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

/// A safe, reference-counted wrapper around a raw PyObject pointer.
/// This is what our Rust-side code uses to hold Python objects.
///
/// When cloned, it increments the reference count.
/// When dropped, it decrements and potentially frees.
#[derive(Debug)]
pub struct PyObjectRef {
    ptr: *mut RawPyObject,
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
    /// with refcount >= 1.
    pub unsafe fn from_raw(ptr: *mut RawPyObject) -> Self {
        debug_assert!(!ptr.is_null());
        PyObjectRef { ptr }
    }

    /// Create a new PyObjectRef by borrowing (increfs).
    ///
    /// # Safety
    /// The pointer must point to a valid RawPyObject.
    pub unsafe fn borrow_raw(ptr: *mut RawPyObject) -> Self {
        debug_assert!(!ptr.is_null());
        (*ptr).incref();
        PyObjectRef { ptr }
    }

    /// Get the raw pointer without affecting refcount.
    pub fn as_raw(&self) -> *mut RawPyObject {
        self.ptr
    }

    /// Consume this ref and return the raw pointer without decrementing.
    /// Caller takes ownership of the reference.
    pub fn into_raw(self) -> *mut RawPyObject {
        let ptr = self.ptr;
        std::mem::forget(self);
        ptr
    }

    /// Get the type object of this Python object.
    pub fn get_type(&self) -> *mut RawPyTypeObject {
        unsafe { (*self.ptr).ob_type }
    }

    pub fn is_null(&self) -> bool {
        self.ptr.is_null()
    }

    pub fn refcnt(&self) -> isize {
        unsafe { (*self.ptr).refcnt() }
    }
}

impl Clone for PyObjectRef {
    fn clone(&self) -> Self {
        unsafe {
            (*self.ptr).incref();
        }
        PyObjectRef { ptr: self.ptr }
    }
}

impl Drop for PyObjectRef {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                let new_refcnt = (*self.ptr).decref();
                if new_refcnt == 0 {
                    // Object should be deallocated.
                    // In a full implementation, we'd call tp_dealloc here.
                    dealloc_object(self.ptr);
                }
            }
        }
    }
}

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
