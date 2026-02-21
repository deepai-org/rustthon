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
use std::os::raw::c_int;
use std::ptr;
use std::sync::OnceLock;

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
pub fn acquire_gil() {
    GIL_DEPTH.with(|depth| {
        let d = *depth.borrow();
        if d == 0 {
            // First acquisition - actually lock
            let guard = get_gil().lock();
            GIL_GUARD.with(|g| {
                *g.borrow_mut() = Some(guard);
            });
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

// ─── C API exports ───

/// PyGILState_STATE enum values
pub const PYGIL_STATE_LOCKED: c_int = 0;
pub const PYGIL_STATE_UNLOCKED: c_int = 1;

/// PyGILState_Ensure - ensure the GIL is held, return previous state.
/// This is how C extensions (e.g., called from C threads) safely enter Python.
#[no_mangle]
pub unsafe extern "C" fn PyGILState_Ensure() -> c_int {
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
}

/// PyGILState_Release - release the GIL if we acquired it.
#[no_mangle]
pub unsafe extern "C" fn PyGILState_Release(oldstate: c_int) {
    release_gil();
}

/// PyGILState_Check - check if GIL is held by current thread
#[no_mangle]
pub unsafe extern "C" fn PyGILState_Check() -> c_int {
    if gil_held() { 1 } else { 0 }
}

/// PyEval_InitThreads - initialize threading (no-op for us, GIL exists from start)
#[no_mangle]
pub unsafe extern "C" fn PyEval_InitThreads() {
    // Ensure the GIL is initialized
    get_gil();
}

/// PyEval_SaveThread - release GIL and return thread state (for Py_BEGIN_ALLOW_THREADS)
#[no_mangle]
pub unsafe extern "C" fn PyEval_SaveThread() -> *mut PyThreadState {
    let tstate = crate::runtime::thread_state::PyThreadState_Get();
    release_gil();
    tstate
}

/// PyEval_RestoreThread - acquire GIL and set thread state (for Py_END_ALLOW_THREADS)
#[no_mangle]
pub unsafe extern "C" fn PyEval_RestoreThread(tstate: *mut PyThreadState) {
    acquire_gil();
    if !tstate.is_null() {
        crate::runtime::thread_state::PyThreadState_Swap(tstate);
    }
}

/// PyEval_AcquireThread
#[no_mangle]
pub unsafe extern "C" fn PyEval_AcquireThread(tstate: *mut PyThreadState) {
    acquire_gil();
    if !tstate.is_null() {
        crate::runtime::thread_state::PyThreadState_Swap(tstate);
    }
}

/// PyEval_ReleaseThread
#[no_mangle]
pub unsafe extern "C" fn PyEval_ReleaseThread(_tstate: *mut PyThreadState) {
    release_gil();
}

/// Py_BEGIN_ALLOW_THREADS / Py_END_ALLOW_THREADS are macros in CPython.
/// Extensions that use them as functions (rare) would call these.
#[no_mangle]
pub unsafe extern "C" fn _PyEval_SaveThread() -> *mut PyThreadState {
    PyEval_SaveThread()
}

#[no_mangle]
pub unsafe extern "C" fn _PyEval_RestoreThread(tstate: *mut PyThreadState) {
    PyEval_RestoreThread(tstate)
}
