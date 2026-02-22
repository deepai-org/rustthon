//! Python thread state management.
//!
//! CPython maintains a PyThreadState for each thread, and an
//! PyInterpreterState for each interpreter. C extensions frequently
//! access thread state to get/set the current exception, check
//! the GIL status, etc.

use crate::object::pyobject::RawPyObject;
use parking_lot::Mutex;
use std::cell::RefCell;
use std::os::raw::c_void;
use std::ptr;

/// CPython 3.11 _PyCFrame layout (24 bytes on 64-bit)
#[repr(C)]
pub struct PyCFrame {
    pub use_tracing: u8,
    pub _pad: [u8; 7],
    pub current_frame: *mut c_void, // _PyInterpreterFrame *
    pub previous: *mut PyCFrame,
}

/// CPython 3.11 _PyErr_StackItem — exception handler stack entry.
/// Used by `except` clauses to save/restore the active exception.
#[repr(C)]
pub struct PyErrStackItem {
    /// The handled exception value (or NULL if none).
    pub exc_value: *mut RawPyObject,
    /// Previous item in the exception stack chain.
    pub previous_item: *mut PyErrStackItem,
}

/// Matches CPython 3.11 PyThreadState layout exactly (360 bytes).
/// Verified offsets against CPython 3.11 headers via offsetof().
/// Cython reads curexc_type at offset 96 and exc_info at offset 120 directly.
#[repr(C)]
pub struct PyThreadState {
    // offset 0
    pub prev: *mut PyThreadState,
    // offset 8
    pub next: *mut PyThreadState,
    // offset 16
    pub interp: *mut PyInterpreterState,
    // offset 24
    pub _initialized: i32,
    // offset 28
    pub _static: i32,
    // offset 32
    pub recursion_remaining: i32,
    // offset 36
    pub recursion_limit: i32,
    // offset 40
    pub recursion_headroom: i32,
    // offset 44
    pub tracing: i32,
    // offset 48
    pub tracing_what: i32,
    // offset 52 (padding for 8-byte alignment of cframe)
    pub _pad52: i32,
    // offset 56
    pub cframe: *mut PyCFrame,
    // offset 64
    pub c_profilefunc: *mut c_void, // Py_tracefunc
    // offset 72
    pub c_tracefunc: *mut c_void,   // Py_tracefunc
    // offset 80
    pub c_profileobj: *mut RawPyObject,
    // offset 88
    pub c_traceobj: *mut RawPyObject,

    // offset 96 — curexc_type (Cython reads directly at 0x60)
    pub curexc_type: *mut RawPyObject,
    // offset 104
    pub curexc_value: *mut RawPyObject,
    // offset 112
    pub curexc_traceback: *mut RawPyObject,

    // offset 120 — exc_info pointer (Cython reads at 0x78)
    // Points to the top of the exception handler stack (initially &exc_state)
    pub exc_info: *mut PyErrStackItem,

    // offset 128 — per-thread dict
    pub dict: *mut RawPyObject,
    // offset 136
    pub gilstate_counter: i32,
    // offset 140 (padding)
    pub _pad140: i32,
    // offset 144
    pub async_exc: *mut RawPyObject,
    // offset 152
    pub thread_id: u64,
    // offset 160
    pub native_thread_id: u64,

    // offsets 168..320 — padding to match CPython layout
    // (trash_delete_nesting, on_delete, on_delete_data, coroutine_origin_tracking_depth,
    //  async_gen fields, datastack, etc.)
    pub _reserved: [u8; 152],

    // offset 320 — the bottom of the exception handler stack (inline)
    pub exc_state: PyErrStackItem,
    // offset 336..360 — remaining CPython fields (root_cframe, etc.)
    pub _reserved2: [u8; 24],
}

/// Matches CPython's PyInterpreterState (simplified)
#[repr(C)]
pub struct PyInterpreterState {
    pub next: *mut PyInterpreterState,
    pub tstate_head: *mut PyThreadState,
    /// Module dict
    pub modules: *mut RawPyObject,
    /// sys.path, sys.modules, builtins
    pub sysdict: *mut RawPyObject,
    pub builtins: *mut RawPyObject,
}

/// Wrapper to make *mut PyInterpreterState Send
pub struct SendInterpPtr(pub Option<*mut PyInterpreterState>);
unsafe impl Send for SendInterpPtr {}

/// Global interpreter state
pub static INTERP_STATE: Mutex<SendInterpPtr> = Mutex::new(SendInterpPtr(None));

thread_local! {
    static CURRENT_TSTATE: RefCell<*mut PyThreadState> = RefCell::new(ptr::null_mut());
}

/// Initialize the main interpreter and thread state.
pub fn init_thread_state() -> *mut PyThreadState {
    unsafe {
        // Create interpreter state
        let interp = Box::into_raw(Box::new(PyInterpreterState {
            next: ptr::null_mut(),
            tstate_head: ptr::null_mut(),
            modules: ptr::null_mut(),
            sysdict: ptr::null_mut(),
            builtins: ptr::null_mut(),
        }));

        INTERP_STATE.lock().0 = Some(interp);

        // Create main thread state
        let tstate = create_thread_state(interp);
        set_current_tstate(tstate);
        tstate
    }
}

/// Create a new thread state for the given interpreter.
pub unsafe fn create_thread_state(interp: *mut PyInterpreterState) -> *mut PyThreadState {
    let tstate = Box::into_raw(Box::new(PyThreadState {
        prev: ptr::null_mut(),
        next: (*interp).tstate_head,
        interp,
        _initialized: 1,
        _static: 0,
        recursion_remaining: 1000,
        recursion_limit: 1000,
        recursion_headroom: 50,
        tracing: 0,
        tracing_what: 0,
        _pad52: 0,
        cframe: ptr::null_mut(),
        c_profilefunc: ptr::null_mut(),
        c_tracefunc: ptr::null_mut(),
        c_profileobj: ptr::null_mut(),
        c_traceobj: ptr::null_mut(),
        curexc_type: ptr::null_mut(),
        curexc_value: ptr::null_mut(),
        curexc_traceback: ptr::null_mut(),
        exc_info: ptr::null_mut(), // will be set below
        dict: ptr::null_mut(),
        gilstate_counter: 0,
        _pad140: 0,
        async_exc: ptr::null_mut(),
        thread_id: {
            let id = std::thread::current().id();
            let id_str = format!("{:?}", id);
            let mut hash: u64 = 5381;
            for byte in id_str.bytes() {
                hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
            }
            hash
        },
        native_thread_id: 0,
        _reserved: [0u8; 152],
        exc_state: PyErrStackItem {
            exc_value: ptr::null_mut(),
            previous_item: ptr::null_mut(),
        },
        _reserved2: [0u8; 24],
    }));

    // exc_info must point to &tstate->exc_state (the bottom of the exception stack)
    (*tstate).exc_info = &mut (*tstate).exc_state as *mut PyErrStackItem;

    // Link into interpreter's thread list
    if !(*interp).tstate_head.is_null() {
        (*(*interp).tstate_head).prev = tstate;
    }
    (*interp).tstate_head = tstate;

    tstate
}

fn set_current_tstate(tstate: *mut PyThreadState) {
    CURRENT_TSTATE.with(|cell| {
        *cell.borrow_mut() = tstate;
    });
}

fn get_current_tstate() -> *mut PyThreadState {
    CURRENT_TSTATE.with(|cell| *cell.borrow())
}

// ─── C API exports ───

/// PyThreadState_Get - get current thread state (fatal if NULL)
#[no_mangle]
pub unsafe extern "C" fn PyThreadState_Get() -> *mut PyThreadState {
    crate::ffi::panic_guard::guard_ptr("PyThreadState_Get", || unsafe {
        let tstate = get_current_tstate();
        if tstate.is_null() {
            eprintln!("Fatal Python error: PyThreadState_Get: the function must be called with the GIL held, but the GIL is released");
            std::process::abort();
        }
        tstate
    })
}

/// _PyThreadState_UncheckedGet
#[no_mangle]
pub unsafe extern "C" fn _PyThreadState_UncheckedGet() -> *mut PyThreadState {
    crate::ffi::panic_guard::guard_ptr("_PyThreadState_UncheckedGet", || unsafe {
        get_current_tstate()
    })
}

/// PyThreadState_GetInterpreter
#[no_mangle]
pub unsafe extern "C" fn PyThreadState_GetInterpreter(
    tstate: *mut PyThreadState,
) -> *mut PyInterpreterState {
    crate::ffi::panic_guard::guard_ptr("PyThreadState_GetInterpreter", || unsafe {
        if tstate.is_null() {
            return ptr::null_mut();
        }
        (*tstate).interp
    })
}

/// PyInterpreterState_Get
#[no_mangle]
pub unsafe extern "C" fn PyInterpreterState_Get() -> *mut PyInterpreterState {
    crate::ffi::panic_guard::guard_ptr("PyInterpreterState_Get", || unsafe {
        let tstate = get_current_tstate();
        if tstate.is_null() {
            return ptr::null_mut();
        }
        (*tstate).interp
    })
}

/// PyThreadState_Swap - swap the current thread state, return old one
#[no_mangle]
pub unsafe extern "C" fn PyThreadState_Swap(
    new_tstate: *mut PyThreadState,
) -> *mut PyThreadState {
    crate::ffi::panic_guard::guard_ptr("PyThreadState_Swap", || unsafe {
        let old = get_current_tstate();
        set_current_tstate(new_tstate);
        old
    })
}

/// PyThreadState_New
#[no_mangle]
pub unsafe extern "C" fn PyThreadState_New(
    interp: *mut PyInterpreterState,
) -> *mut PyThreadState {
    crate::ffi::panic_guard::guard_ptr("PyThreadState_New", || unsafe {
        create_thread_state(interp)
    })
}

/// PyThreadState_Clear
#[no_mangle]
pub unsafe extern "C" fn PyThreadState_Clear(tstate: *mut PyThreadState) {
    crate::ffi::panic_guard::guard_void("PyThreadState_Clear", || unsafe {
        if !tstate.is_null() {
            (*tstate).curexc_type = ptr::null_mut();
            (*tstate).curexc_value = ptr::null_mut();
            (*tstate).curexc_traceback = ptr::null_mut();
        }
    })
}

/// PyInterpreterState_GetID — return a unique ID for the interpreter.
#[no_mangle]
pub unsafe extern "C" fn PyInterpreterState_GetID(
    interp: *mut PyInterpreterState,
) -> i64 {
    crate::ffi::panic_guard::guard_i64("PyInterpreterState_GetID", || unsafe {
        if interp.is_null() {
            return -1;
        }
        // Return a stable ID based on the pointer
        interp as i64
    })
}

/// PyThreadState_Delete
#[no_mangle]
pub unsafe extern "C" fn PyThreadState_Delete(tstate: *mut PyThreadState) {
    crate::ffi::panic_guard::guard_void("PyThreadState_Delete", || unsafe {
        if tstate.is_null() {
            return;
        }
        // Unlink from interpreter's list
        let prev = (*tstate).prev;
        let next = (*tstate).next;
        if !prev.is_null() {
            (*prev).next = next;
        }
        if !next.is_null() {
            (*next).prev = prev;
        }
        let interp = (*tstate).interp;
        if !interp.is_null() && (*interp).tstate_head == tstate {
            (*interp).tstate_head = next;
        }
        drop(Box::from_raw(tstate));
    })
}
