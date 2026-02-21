//! Execution frame — holds the state for one code object being executed.
//!
//! All values are RAII `PyObjectRef` — refcounting is automatic:
//! - `push()` takes ownership (moves in)
//! - `pop()` returns ownership (moves out)
//! - `store_name()` replaces old value (old is dropped → auto decref)
//! - `lookup_name()` clones (= incref) so caller gets an owned reference

use crate::compiler::bytecode::CodeObject;
use crate::object::pyobject::PyObjectRef;
use crate::runtime::pyerr::PyErr;
use std::collections::HashMap;

pub struct Frame {
    /// The code being executed
    pub code: CodeObject,
    /// Instruction pointer (index into code.instructions)
    pub ip: usize,
    /// Value stack (RAII — Drop decrefs all remaining objects)
    pub stack: Vec<PyObjectRef>,
    /// Local variables (by name)
    pub locals: HashMap<String, PyObjectRef>,
    /// Global variables (shared dict)
    pub globals: HashMap<String, PyObjectRef>,
    /// Built-in functions
    pub builtins: HashMap<String, PyObjectRef>,
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

    /// Push a value onto the stack (takes ownership).
    #[inline]
    pub fn push(&mut self, obj: PyObjectRef) {
        self.stack.push(obj);
    }

    /// Pop a value from the stack (returns ownership).
    /// Returns Err on stack underflow (should never happen in correct bytecode).
    #[inline]
    pub fn pop(&mut self) -> Result<PyObjectRef, PyErr> {
        self.stack.pop().ok_or_else(|| PyErr::type_error("VM stack underflow"))
    }

    /// Peek at the top of the stack (clones = increfs).
    #[inline]
    pub fn top(&self) -> Result<PyObjectRef, PyErr> {
        self.stack.last().cloned().ok_or_else(|| PyErr::type_error("VM stack underflow"))
    }

    /// Look up a name in locals, then globals, then builtins.
    /// Returns a cloned (incref'd) reference, or None if not found.
    pub fn lookup_name(&self, name: &str) -> Option<PyObjectRef> {
        self.locals.get(name)
            .or_else(|| self.globals.get(name))
            .or_else(|| self.builtins.get(name))
            .cloned() // Clone = incref. Caller gets an owned reference.
    }

    /// Store a name in locals. The old value (if any) is automatically
    /// dropped (= decref'd) by HashMap::insert.
    pub fn store_name(&mut self, name: &str, obj: PyObjectRef) {
        self.locals.insert(name.to_string(), obj);
    }
}
