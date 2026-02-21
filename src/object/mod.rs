pub mod pyobject;
pub mod typeobj;
pub mod refcount;
pub mod gc;

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
