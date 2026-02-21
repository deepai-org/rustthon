//! Rustthon: A CPython-extension compatible Python interpreter in Rust.
//!
//! This crate provides a Python 3.x interpreter that can load and run
//! native CPython C extensions (.so/.dylib) by faithfully replicating
//! CPython's C ABI and memory layout.

pub mod object;
pub mod types;
pub mod runtime;
pub mod compiler;
pub mod vm;
pub mod ffi;
pub mod module;

// Re-export core types for convenience
pub use object::pyobject::{PyObject, PyObjectRef, RawPyObject};
pub use object::typeobj::RawPyTypeObject;
pub use runtime::memory;
pub use runtime::thread_state;
pub use runtime::error;
