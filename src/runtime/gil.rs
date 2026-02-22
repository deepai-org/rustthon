//! Global Interpreter Lock (GIL) emulation.
//!
//! Even though Rust is perfectly capable of safe concurrency,
//! we MUST emulate the GIL because C extensions like mysqlclient
//! and cryptography assume it exists. They call PyGILState_Ensure
//! before accessing Python objects and PyGILState_Release after.
//!
//! Our strategy: a global Mutex that acts as the GIL.

use parking_lot::{Mutex, MutexGuard};
use std::cell::RefCell;
use std::marker::PhantomData;
use std::os::raw::c_int;
use std::sync::OnceLock;

use crate::object::pyobject::RawPyObject;
use crate::runtime::thread_state::PyThreadState;

/// The GIL itself - a simple mutex.
static GIL: OnceLock<Mutex<()>> = OnceLock::new();

fn get_gil() -> &'static Mutex<()> {
    GIL.get_or_init(|| Mutex::new(()))
}

thread_local! {
    /// How many times this thread has acquired the GIL (for reentrant acquire)
    static GIL_DEPTH: RefCell<u32> = RefCell::new(0);
    /// Stored mutex guard (kept alive while GIL is held)
    static GIL_GUARD: RefCell<Option<MutexGuard<'static, ()>>> = RefCell::new(None);
}

/// Acquire the GIL. Called by the runtime at startup and by C extensions.
/// After acquiring, flushes any pending decrefs queued while the GIL was released.
pub fn acquire_gil() {
    GIL_DEPTH.with(|depth| {
        let d = *depth.borrow();
        if d == 0 {
            // First acquisition - actually lock
            let guard = get_gil().lock();
            GIL_GUARD.with(|g| {
                *g.borrow_mut() = Some(guard);
            });
            // Flush pending decrefs now that we hold the GIL
            flush_pending_decrefs();
        }
        *depth.borrow_mut() = d + 1;
    });
}

/// Release the GIL.
pub fn release_gil() {
    GIL_DEPTH.with(|depth| {
        let d = *depth.borrow();
        if d == 0 {
            return; // Not holding the GIL
        }
        *depth.borrow_mut() = d - 1;
        if d == 1 {
            // Last release - actually unlock
            GIL_GUARD.with(|g| {
                *g.borrow_mut() = None; // Drops the guard, releasing the lock
            });
        }
    });
}

/// Check if the current thread holds the GIL.
pub fn gil_held() -> bool {
    GIL_DEPTH.with(|depth| *depth.borrow() > 0)
}

// ─── Python<'py> GIL Token ───

/// Zero-sized proof that the GIL is held by the current thread.
///
/// This type is `!Send` and `!Sync` because the `*mut ()` in `PhantomData`
/// opts out of auto-traits. A GIL token obtained on thread A must NEVER be
/// usable on thread B — the compiler enforces this at zero runtime cost.
#[derive(Copy, Clone)]
pub struct Python<'py>(PhantomData<(&'py (), *mut ())>);

impl Python<'_> {
    /// Acquire the GIL and run a closure with the token.
    ///
    /// The closure receives a `Python<'py>` proving the GIL is held.
    /// The GIL is released when the closure returns.
    pub fn with_gil<F, R>(f: F) -> R
    where
        F: for<'py> FnOnce(Python<'py>) -> R,
    {
        acquire_gil();
        let result = f(Python(PhantomData));
        release_gil();
        result
    }

    /// Create a token when the GIL is known to be held.
    ///
    /// # Safety
    /// Caller must guarantee the GIL is currently held by this thread.
    /// In debug builds, this asserts `gil_held()`.
    pub unsafe fn assume_gil_held() -> Python<'static> {
        debug_assert!(gil_held(), "BUG: assume_gil_held called without holding the GIL");
        Python(PhantomData)
    }
}

// ─── Pending Decref Queue ───
//
// When a PyObjectRef is dropped without the GIL held (e.g., during a
// Py_BEGIN_ALLOW_THREADS block), we cannot call Py_DECREF immediately.
// Instead, we queue the pointer for later decref when the GIL is re-acquired.

/// Wrapper to make raw pointers Send for the pending-decref queue.
/// SAFETY: The GIL contract ensures these pointers are only dereferenced
/// while the GIL is held (inside flush_pending_decrefs).
struct SendPtr(*mut RawPyObject);
unsafe impl Send for SendPtr {}

static PENDING_DECREFS: Mutex<Vec<SendPtr>> = Mutex::new(Vec::new());

/// Queue a pointer for deferred decref. Called from PyObjectRef::drop()
/// when the GIL is not held.
pub fn queue_decref(ptr: *mut RawPyObject) {
    PENDING_DECREFS.lock().push(SendPtr(ptr));
}

/// Flush all pending decrefs. Called after re-acquiring the GIL.
fn flush_pending_decrefs() {
    let mut queue = PENDING_DECREFS.lock();
    for SendPtr(ptr) in queue.drain(..) {
        unsafe {
            let new_refcnt = (*ptr).decref();
            if new_refcnt == 0 {
                crate::object::pyobject::dealloc_object(ptr);
            }
        }
    }
}

// ─── C API exports ───

/// PyGILState_STATE enum values
pub const PYGIL_STATE_LOCKED: c_int = 0;
pub const PYGIL_STATE_UNLOCKED: c_int = 1;

/// PyGILState_Ensure - ensure the GIL is held, return previous state.
/// This is how C extensions (e.g., called from C threads) safely enter Python.
#[no_mangle]
pub unsafe extern "C" fn PyGILState_Ensure() -> c_int {
    crate::ffi::panic_guard::guard_int("PyGILState_Ensure", || unsafe {
        let already_held = gil_held();
        acquire_gil();

        // If this thread doesn't have a thread state yet, create one
        let tstate = crate::runtime::thread_state::_PyThreadState_UncheckedGet();
        if tstate.is_null() {
            let interp = crate::runtime::thread_state::INTERP_STATE.lock().0;
            if let Some(interp) = interp {
                let new_tstate = crate::runtime::thread_state::create_thread_state(interp);
                crate::runtime::thread_state::PyThreadState_Swap(new_tstate);
            }
        }

        if already_held {
            PYGIL_STATE_LOCKED
        } else {
            PYGIL_STATE_UNLOCKED
        }
    })
}

/// PyGILState_Release - release the GIL if we acquired it.
#[no_mangle]
pub unsafe extern "C" fn PyGILState_Release(oldstate: c_int) {
    crate::ffi::panic_guard::guard_void("PyGILState_Release", || unsafe {
        release_gil();
    })
}

/// PyGILState_Check - check if GIL is held by current thread
#[no_mangle]
pub unsafe extern "C" fn PyGILState_Check() -> c_int {
    crate::ffi::panic_guard::guard_int("PyGILState_Check", || unsafe {
        if gil_held() { 1 } else { 0 }
    })
}

/// PyEval_InitThreads - initialize threading (no-op for us, GIL exists from start)
#[no_mangle]
pub unsafe extern "C" fn PyEval_InitThreads() {
    crate::ffi::panic_guard::guard_void("PyEval_InitThreads", || unsafe {
        // Ensure the GIL is initialized
        get_gil();
    })
}

/// PyEval_SaveThread - release GIL and return thread state (for Py_BEGIN_ALLOW_THREADS)
#[no_mangle]
pub unsafe extern "C" fn PyEval_SaveThread() -> *mut PyThreadState {
    crate::ffi::panic_guard::guard_ptr("PyEval_SaveThread", || unsafe {
        let tstate = crate::runtime::thread_state::PyThreadState_Get();
        release_gil();
        tstate
    })
}

/// PyEval_RestoreThread - acquire GIL and set thread state (for Py_END_ALLOW_THREADS)
#[no_mangle]
pub unsafe extern "C" fn PyEval_RestoreThread(tstate: *mut PyThreadState) {
    crate::ffi::panic_guard::guard_void("PyEval_RestoreThread", || unsafe {
        acquire_gil();
        if !tstate.is_null() {
            crate::runtime::thread_state::PyThreadState_Swap(tstate);
        }
    })
}

/// PyEval_AcquireThread
#[no_mangle]
pub unsafe extern "C" fn PyEval_AcquireThread(tstate: *mut PyThreadState) {
    crate::ffi::panic_guard::guard_void("PyEval_AcquireThread", || unsafe {
        acquire_gil();
        if !tstate.is_null() {
            crate::runtime::thread_state::PyThreadState_Swap(tstate);
        }
    })
}

/// PyEval_ReleaseThread
#[no_mangle]
pub unsafe extern "C" fn PyEval_ReleaseThread(_tstate: *mut PyThreadState) {
    crate::ffi::panic_guard::guard_void("PyEval_ReleaseThread", || unsafe {
        release_gil();
    })
}

/// Py_BEGIN_ALLOW_THREADS / Py_END_ALLOW_THREADS are macros in CPython.
/// Extensions that use them as functions (rare) would call these.
#[no_mangle]
pub unsafe extern "C" fn _PyEval_SaveThread() -> *mut PyThreadState {
    crate::ffi::panic_guard::guard_ptr("_PyEval_SaveThread", || unsafe {
        PyEval_SaveThread()
    })
}

#[no_mangle]
pub unsafe extern "C" fn _PyEval_RestoreThread(tstate: *mut PyThreadState) {
    crate::ffi::panic_guard::guard_void("_PyEval_RestoreThread", || unsafe {
        PyEval_RestoreThread(tstate)
    })
}
