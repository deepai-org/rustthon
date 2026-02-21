pub mod pyobject;
pub mod typeobj;
pub mod refcount;
pub mod gc;
pub mod safe_api;

/// Re-export SyncUnsafeCell for use throughout the crate.
/// This is `UnsafeCell<T>` + `Sync`, used to replace `static mut` for
/// global state that is protected by the GIL.
///
/// # Safety
/// `SyncUnsafeCell` is `Sync` by definition. All access to the inner value
/// via `.get()` returns `*mut T` and requires the caller to ensure no data
/// races. In Rustthon this is guaranteed by the GIL — only one thread
/// executes Python code at a time.
pub use sync_unsafe_cell::SyncUnsafeCell;

/// Like `SyncUnsafeCell` but with **unconditional** `Sync`.
///
/// `SyncUnsafeCell<T>` requires `T: Sync`. For types like `*mut RawPyObject`
/// (used in exception pointer globals), `T` is a raw pointer which is not
/// `Sync`. This wrapper removes that bound.
///
/// # Safety
/// Thread safety is guaranteed by the GIL, not the type system. All access
/// to the inner value must be behind the GIL.
#[repr(transparent)]
pub struct StaticPtr<T>(std::cell::UnsafeCell<T>);

unsafe impl<T> Sync for StaticPtr<T> {}
unsafe impl<T> Send for StaticPtr<T> {}

impl<T> StaticPtr<T> {
    pub const fn new(value: T) -> Self {
        StaticPtr(std::cell::UnsafeCell::new(value))
    }

    #[inline]
    pub fn get(&self) -> *mut T {
        self.0.get()
    }
}

/// Wrapper to make raw PyObject pointers Send+Sync.
/// This is safe because we protect all access with the GIL.
#[derive(Debug, Clone, Copy)]
pub struct SendPtr<T>(pub *mut T);

unsafe impl<T> Send for SendPtr<T> {}
unsafe impl<T> Sync for SendPtr<T> {}

impl<T> SendPtr<T> {
    pub fn null() -> Self {
        SendPtr(std::ptr::null_mut())
    }

    pub fn get(&self) -> *mut T {
        self.0
    }
}
