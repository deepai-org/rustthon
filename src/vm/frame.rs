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
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Shared mutable cell storage for closures.
/// Inner functions hold an Rc to the same map, so writes via `nonlocal`
/// in an inner function are visible in the outer function and vice versa.
pub type CellMap = Rc<RefCell<HashMap<String, PyObjectRef>>>;

/// A block on the block stack (for try/except/finally/loop).
#[derive(Debug, Clone)]
pub enum BlockType {
    /// try/except block: handler_ip is the except handler entry point
    ExceptHandler { handler_ip: usize },
    /// try/finally block: handler_ip is the finally handler entry point
    FinallyHandler { handler_ip: usize },
    /// Loop block: end_ip is the jump target for break
    Loop { end_ip: usize },
    /// Active except handler sentinel (pushed when entering an except handler)
    /// PopExcept pops this.
    ActiveExceptHandler,
}

/// A saved block state, pushed when entering a try/except/finally/loop.
#[derive(Debug, Clone)]
pub struct Block {
    pub block_type: BlockType,
    /// Stack depth when the block was entered (for unwinding)
    pub stack_depth: usize,
}

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
    /// Block stack for try/except/finally/loop unwinding
    pub block_stack: Vec<Block>,
    /// Cell variables for closures (shared with inner functions via Rc)
    pub cells: Option<CellMap>,
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
            block_stack: Vec::new(),
            cells: None,
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

    /// Unwind the stack to the saved depth (dropping excess values).
    pub fn unwind_stack_to(&mut self, depth: usize) {
        while self.stack.len() > depth {
            let _ = self.stack.pop(); // Drop = decref
        }
    }
}
