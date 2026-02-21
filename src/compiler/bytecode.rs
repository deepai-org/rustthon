//! Bytecode instruction set.
//!
//! Our bytecode is inspired by CPython's but simplified.
//! Each instruction is an opcode + optional argument.

use crate::object::pyobject::RawPyObject;

/// Bytecode opcodes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OpCode {
    /// Load a constant from the constants pool
    LoadConst = 1,
    /// Load a name from local scope
    LoadName = 2,
    /// Store a name in local scope
    StoreName = 3,
    /// Load a global name
    LoadGlobal = 4,
    /// Store a global
    StoreGlobal = 5,
    /// Load an attribute: TOS = TOS.name
    LoadAttr = 6,
    /// Store an attribute: TOS1.name = TOS
    StoreAttr = 7,
    /// Load a fast local (by index)
    LoadFast = 8,
    /// Store a fast local
    StoreFast = 9,

    // ─── Stack manipulation ───
    /// Pop TOS
    PopTop = 10,
    /// Duplicate TOS
    DupTop = 11,
    /// Rotate top 2
    RotTwo = 12,
    /// Rotate top 3
    RotThree = 13,

    // ─── Unary operations ───
    UnaryNot = 20,
    UnaryNegative = 21,
    UnaryPositive = 22,

    // ─── Binary operations ───
    BinaryAdd = 30,
    BinarySubtract = 31,
    BinaryMultiply = 32,
    BinaryTrueDivide = 33,
    BinaryFloorDivide = 34,
    BinaryModulo = 35,
    BinaryPower = 36,
    BinaryAnd = 37,
    BinaryOr = 38,
    BinaryXor = 39,
    BinaryLShift = 40,
    BinaryRShift = 41,
    BinarySubscr = 42,

    // ─── In-place operations ───
    InplaceAdd = 50,
    InplaceSubtract = 51,
    InplaceMultiply = 52,

    // ─── Comparison ───
    CompareOp = 60,

    // ─── Jumps and control flow ───
    /// Absolute jump
    JumpAbsolute = 70,
    /// Jump if TOS is false (pop)
    JumpIfFalse = 71,
    /// Jump if TOS is true (pop)
    JumpIfTrue = 72,
    /// Pop and jump if false
    PopJumpIfFalse = 73,
    /// Pop and jump if true
    PopJumpIfTrue = 74,

    // ─── Function/call operations ───
    /// Call function with N args on stack
    CallFunction = 80,
    /// Return TOS from function
    ReturnValue = 81,
    /// Make a function from code object + name
    MakeFunction = 82,

    // ─── Container operations ───
    /// Build a list from N items on stack
    BuildList = 90,
    /// Build a tuple from N items on stack
    BuildTuple = 91,
    /// Build a dict from N key/value pairs on stack
    BuildMap = 92,
    /// Build a set from N items on stack
    BuildSet = 93,
    /// Unpack a sequence into N items
    UnpackSequence = 94,
    /// Store TOS[TOS1] = TOS2
    StoreSubscr = 95,

    // ─── Import ───
    ImportName = 100,
    ImportFrom = 101,

    // ─── Loop / iterator ───
    GetIter = 110,
    ForIter = 111,

    // ─── Print (for simple debugging) ───
    PrintExpr = 120,

    // ─── Misc ───
    Nop = 0,
    SetupLoop = 130,
    PopBlock = 131,
    BreakLoop = 132,
    ContinueLoop = 133,
}

/// A single bytecode instruction
#[derive(Debug, Clone)]
pub struct Instruction {
    pub opcode: OpCode,
    pub arg: u32,
}

/// A compiled code object (like CPython's PyCodeObject)
#[derive(Debug)]
pub struct CodeObject {
    /// The bytecode instructions
    pub instructions: Vec<Instruction>,
    /// Constants pool (Python objects)
    pub constants: Vec<*mut RawPyObject>,
    /// Names used in the code (for LOAD_NAME, STORE_NAME)
    pub names: Vec<String>,
    /// Local variable names (for LOAD_FAST, STORE_FAST)
    pub varnames: Vec<String>,
    /// Source filename
    pub filename: String,
    /// Function name (or "<module>" for top-level)
    pub name: String,
    /// Number of arguments (for functions)
    pub argcount: u32,
}

impl CodeObject {
    pub fn new(name: String, filename: String) -> Self {
        CodeObject {
            instructions: Vec::new(),
            constants: Vec::new(),
            names: Vec::new(),
            varnames: Vec::new(),
            filename,
            name,
            argcount: 0,
        }
    }

    /// Add a constant and return its index.
    pub fn add_const(&mut self, obj: *mut RawPyObject) -> u32 {
        let idx = self.constants.len() as u32;
        self.constants.push(obj);
        idx
    }

    /// Add a name and return its index.
    pub fn add_name(&mut self, name: &str) -> u32 {
        if let Some(idx) = self.names.iter().position(|n| n == name) {
            return idx as u32;
        }
        let idx = self.names.len() as u32;
        self.names.push(name.to_string());
        idx
    }

    /// Emit an instruction.
    pub fn emit(&mut self, opcode: OpCode, arg: u32) {
        self.instructions.push(Instruction { opcode, arg });
    }

    /// Get the current instruction offset (for jump targets).
    pub fn current_offset(&self) -> u32 {
        self.instructions.len() as u32
    }

    /// Patch a jump instruction's argument.
    pub fn patch_jump(&mut self, instr_idx: u32, target: u32) {
        self.instructions[instr_idx as usize].arg = target;
    }
}
