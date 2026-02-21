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

/// Matches CPython's PyThreadState (simplified but ABI-compatible for common fields)
#[repr(C)]
pub struct PyThreadState {
    /// Previous thread state in the linked list
    pub prev: *mut PyThreadState,
    /// Next thread state
    pub next: *mut PyThreadState,
    /// The interpreter this thread belongs to
    pub interp: *mut PyInterpreterState,

    /// Current exception info
    pub curexc_type: *mut RawPyObject,
    pub curexc_value: *mut RawPyObject,
    pub curexc_traceback: *mut RawPyObject,

    /// Exception state for generators
    pub exc_state_type: *mut RawPyObject,
    pub exc_state_value: *mut RawPyObject,
    pub exc_state_traceback: *mut RawPyObject,

    /// Current recursion depth
    pub recursion_depth: i32,
    pub recursion_limit: i32,

    /// Thread ID
    pub thread_id: u64,
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
        curexc_type: ptr::null_mut(),
        curexc_value: ptr::null_mut(),
        curexc_traceback: ptr::null_mut(),
        exc_state_type: ptr::null_mut(),
        exc_state_value: ptr::null_mut(),
        exc_state_traceback: ptr::null_mut(),
        recursion_depth: 0,
        recursion_limit: 1000,
        thread_id: {
            // Use a hash of the thread id as a stable u64 identifier
            let id = std::thread::current().id();
            let id_str = format!("{:?}", id);
            let mut hash: u64 = 5381;
            for byte in id_str.bytes() {
                hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
            }
            hash
        },
    }));

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
    let tstate = get_current_tstate();
    if tstate.is_null() {
        eprintln!("Fatal Python error: PyThreadState_Get: the function must be called with the GIL held, but the GIL is released");
        std::process::abort();
    }
    tstate
}

/// _PyThreadState_UncheckedGet
#[no_mangle]
pub unsafe extern "C" fn _PyThreadState_UncheckedGet() -> *mut PyThreadState {
    get_current_tstate()
}

/// PyThreadState_GetInterpreter
#[no_mangle]
pub unsafe extern "C" fn PyThreadState_GetInterpreter(
    tstate: *mut PyThreadState,
) -> *mut PyInterpreterState {
    if tstate.is_null() {
        return ptr::null_mut();
    }
    (*tstate).interp
}

/// PyInterpreterState_Get
#[no_mangle]
pub unsafe extern "C" fn PyInterpreterState_Get() -> *mut PyInterpreterState {
    let tstate = get_current_tstate();
    if tstate.is_null() {
        return ptr::null_mut();
    }
    (*tstate).interp
}

/// PyThreadState_Swap - swap the current thread state, return old one
#[no_mangle]
pub unsafe extern "C" fn PyThreadState_Swap(
    new_tstate: *mut PyThreadState,
) -> *mut PyThreadState {
    let old = get_current_tstate();
    set_current_tstate(new_tstate);
    old
}

/// PyThreadState_New
#[no_mangle]
pub unsafe extern "C" fn PyThreadState_New(
    interp: *mut PyInterpreterState,
) -> *mut PyThreadState {
    create_thread_state(interp)
}

/// PyThreadState_Clear
#[no_mangle]
pub unsafe extern "C" fn PyThreadState_Clear(tstate: *mut PyThreadState) {
    if !tstate.is_null() {
        (*tstate).curexc_type = ptr::null_mut();
        (*tstate).curexc_value = ptr::null_mut();
        (*tstate).curexc_traceback = ptr::null_mut();
    }
}

/// PyThreadState_Delete
#[no_mangle]
pub unsafe extern "C" fn PyThreadState_Delete(tstate: *mut PyThreadState) {
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
}
