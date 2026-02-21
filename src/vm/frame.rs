//! Execution frame — holds the state for one code object being executed.

use crate::compiler::bytecode::CodeObject;
use crate::object::pyobject::RawPyObject;
use crate::object::safe_api::{py_incref, py_decref};
use std::collections::HashMap;
use std::ptr;

pub struct Frame {
    /// The code being executed
    pub code: CodeObject,
    /// Instruction pointer (index into code.instructions)
    pub ip: usize,
    /// Value stack
    pub stack: Vec<*mut RawPyObject>,
    /// Local variables (by name)
    pub locals: HashMap<String, *mut RawPyObject>,
    /// Global variables (shared dict)
    pub globals: HashMap<String, *mut RawPyObject>,
    /// Built-in functions
    pub builtins: HashMap<String, *mut RawPyObject>,
}

impl Frame {
    pub fn new(code: CodeObject) -> Self {
        Frame {
            code,
            ip: 0,
            stack: Vec::with_capacity(64),
            locals: HashMap::new(),
            globals: HashMap::new(),
            builtins: HashMap::new(),
        }
    }

    /// Push a value onto the stack.
    pub fn push(&mut self, obj: *mut RawPyObject) {
        self.stack.push(obj);
    }

    /// Pop a value from the stack.
    pub fn pop(&mut self) -> *mut RawPyObject {
        self.stack.pop().unwrap_or(ptr::null_mut())
    }

    /// Peek at the top of the stack.
    pub fn top(&self) -> *mut RawPyObject {
        self.stack.last().copied().unwrap_or(ptr::null_mut())
    }

    /// Look up a name in locals, then globals, then builtins.
    pub fn lookup_name(&self, name: &str) -> *mut RawPyObject {
        if let Some(&obj) = self.locals.get(name) {
            return obj;
        }
        if let Some(&obj) = self.globals.get(name) {
            return obj;
        }
        if let Some(&obj) = self.builtins.get(name) {
            return obj;
        }
        ptr::null_mut()
    }

    /// Store a name in locals.
    pub fn store_name(&mut self, name: &str, obj: *mut RawPyObject) {
        py_incref(obj);
        // Decref old value if present
        if let Some(&old) = self.locals.get(name) {
            py_decref(old);
        }
        self.locals.insert(name.to_string(), obj);
    }
}
