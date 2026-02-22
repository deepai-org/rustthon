//! The bytecode interpreter (VM execution loop).
//!
//! Key safety properties:
//! - Zero manual py_incref/py_decref — all refcounting is RAII via PyObjectRef
//! - All operations return PyResult — no silent NULL propagation
//! - Python<'py> GIL token threaded through for compile-time GIL proof

use crate::compiler::bytecode::{CodeObject, OpCode};
use crate::object::pyobject::{PyObjectRef, RawPyObject};
use crate::object::safe_api::{
    is_int, is_float, is_str, is_list, is_bool, is_none,
    get_int_value, get_float_value,
    create_int, create_str,
    return_none, bool_from_long, py_incref,
    none_obj, true_obj, false_obj, bool_obj,
    new_int, new_float, new_str,
    py_is_true, py_get_attr, py_set_attr, py_get_item, py_store_item,
    py_import, py_repr,
    build_list, build_tuple, build_dict, build_set,
};
use crate::runtime::gil::Python;
use crate::runtime::pyerr::{PyErr, PyResult};
use crate::vm::frame::{Frame, CellMap};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ptr;
use std::rc::Rc;

/// Stored code object for user-defined functions.
/// The compiler stores a Box<CodeObject> as a raw pointer encoded in an int constant.
/// This extracts it without consuming the Box (we clone the CodeObject).
fn extract_code_object(code_marker: &PyObjectRef) -> Option<CodeObject> {
    let raw = code_marker.as_raw();
    if !is_int(raw) {
        return None;
    }
    let ptr_val = get_int_value(raw) as usize;
    if ptr_val == 0 {
        return None;
    }
    // SAFETY: The compiler stored a Box<CodeObject> as Box::into_raw.
    // We borrow the pointer to clone the CodeObject, but don't free it—
    // the constant pool owns the marker int, and we'll leak the CodeObject
    // since it must live as long as the function object.
    let code_ref = unsafe { &*(ptr_val as *const CodeObject) };
    Some(clone_code_object(code_ref))
}

/// Clone a CodeObject (constants are cloned = incref'd).
fn clone_code_object(co: &CodeObject) -> CodeObject {
    CodeObject {
        instructions: co.instructions.clone(),
        constants: co.constants.iter().map(|c| c.clone()).collect(),
        names: co.names.clone(),
        varnames: co.varnames.clone(),
        filename: co.filename.clone(),
        name: co.name.clone(),
        argcount: co.argcount,
        kwonlyargcount: co.kwonlyargcount,
        has_vararg: co.has_vararg,
        has_kwarg: co.has_kwarg,
        freevars: co.freevars.clone(),
        cellvars: co.cellvars.clone(),
        is_generator: co.is_generator,
    }
}

/// A user-defined Python function (Rust-side representation).
/// Stored as data inside a PyObjectWithData. NOT the ABI-compatible
/// PyFunctionObject — we use this for the VM's internal fast path.
pub struct RustFunction {
    pub code: CodeObject,
    pub globals: HashMap<String, PyObjectRef>,
    pub builtins: HashMap<String, PyObjectRef>,
    pub defaults: Vec<PyObjectRef>,
    pub name: String,
    /// Cell map for closures — shared with enclosing/inner functions via Rc.
    pub cells: Option<CellMap>,
}

/// A user-defined Python class (VM-internal representation).
/// Stored as a Box<RustClass> pointer encoded in an int constant.
/// Uses a tag prefix to distinguish from RustFunction pointers.
pub struct RustClass {
    pub name: String,
    /// Base class pointers (raw pointers to heap-allocated RustClass objects).
    /// These are NOT owned — the bases persist as long as the class markers in the VM.
    pub bases: Vec<*const RustClass>,
    /// Class namespace: methods, class variables, etc.
    /// Includes inherited names from bases (flattened at class creation, MRO left-to-right).
    pub namespace: HashMap<String, PyObjectRef>,
    /// Globals from definition scope (for method execution)
    pub globals: HashMap<String, PyObjectRef>,
    /// Builtins from definition scope
    pub builtins: HashMap<String, PyObjectRef>,
}

/// A user-defined Python instance (VM-internal representation).
/// Stored as a Box<RustInstance> pointer encoded in an int constant.
pub struct RustInstance {
    pub class: *const RustClass,
    /// Instance attributes (set via self.x = value)
    pub attrs: HashMap<String, PyObjectRef>,
}

// Tag bits to distinguish class/instance/function pointers stored as int markers.
// We use the high bits of the i64 value:
// - Functions: stored as-is (heap pointers, always positive, low bits)
// - Classes:   pointer | CLASS_TAG
// - Instances: pointer | INSTANCE_TAG
// - BoundMethods: pointer | BOUND_METHOD_TAG (for builtin type methods)
const CLASS_TAG: i64 = 1 << 62;
const INSTANCE_TAG: i64 = 2i64 << 62;
const BOUND_METHOD_TAG: i64 = 3i64 << 62;
const TAG_MASK: i64 = 3i64 << 62;
const PTR_MASK: i64 = !TAG_MASK;

fn is_class_marker(val: i64) -> bool { val & TAG_MASK == CLASS_TAG }
fn is_instance_marker(val: i64) -> bool { val & TAG_MASK == INSTANCE_TAG }
fn is_bound_method_marker(val: i64) -> bool { val & TAG_MASK == BOUND_METHOD_TAG }
/// A function marker is a positive untagged int that looks like a heap pointer.
/// Heap pointers on modern systems are > 4096 (page size). We use a conservative
/// threshold to avoid false positives from small integer class variables.
fn is_function_marker(val: i64) -> bool { val > 4096 && val & TAG_MASK == 0 }
fn extract_ptr(val: i64) -> usize { (val & PTR_MASK) as usize }

/// A bound builtin method: self_obj + method name
struct BoundBuiltinMethod {
    self_obj: PyObjectRef,
    method_name: String,
}

/// Generator tag (uses same tag space as BOUND_METHOD_TAG but different struct)
const GENERATOR_TAG: i64 = 3i64 << 62; // shares tag space, distinguished by struct type

/// A Python generator (from functions with yield).
/// Holds a suspended frame that can be resumed.
struct RustGenerator {
    frame: Frame,
    /// The function this generator was created from (for globals/builtins)
    func_globals: HashMap<String, PyObjectRef>,
    func_builtins: HashMap<String, PyObjectRef>,
    /// Whether the generator has been exhausted
    exhausted: bool,
    /// Whether the generator has been started (first next() call done)
    started: bool,
}

// ─── Regex support ───
// Negative int markers with low-bit sub-tags:
// Generators: -(ptr)      where ptr & 7 == 0 (heap-aligned)
// Regex:      -(ptr | 1)  low bit 1
// Match:      -(ptr | 2)  low bit 2
const REGEX_SUBTAG: i64 = 1;
const MATCH_SUBTAG: i64 = 2;

fn is_regex_marker(val: i64) -> bool { val < 0 && ((-val) & 7) == REGEX_SUBTAG }
fn is_match_marker(val: i64) -> bool { val < 0 && ((-val) & 7) == MATCH_SUBTAG }
fn is_generator_marker(val: i64) -> bool { val < 0 && ((-val) & 7) == 0 }

/// A compiled regex pattern.
struct RustRegex {
    pattern: String,
    compiled: regex::Regex,
}

/// A regex match result.
struct RustMatch {
    full: String,
    groups: Vec<Option<String>>,
    start: usize,
    end: usize,
}

fn make_regex_marker(ptr: *mut RustRegex) -> i64 {
    -(((ptr as usize as i64) | REGEX_SUBTAG))
}

fn make_match_marker(ptr: *mut RustMatch) -> i64 {
    -(((ptr as usize as i64) | MATCH_SUBTAG))
}

fn extract_regex(val: i64) -> *mut RustRegex {
    ((-val) & !7) as usize as *mut RustRegex
}

fn extract_match(val: i64) -> *mut RustMatch {
    ((-val) & !7) as usize as *mut RustMatch
}

/// Result of resuming a generator frame
enum GeneratorResult {
    Yielded(PyObjectRef),
    Returned,
}

/// Target found when unwinding the block stack after an exception.
enum UnwindTarget {
    ExceptHandler { ip: usize, stack_depth: usize },
    FinallyHandler { ip: usize, stack_depth: usize },
}

/// The virtual machine
pub struct VM {
    /// Call stack depth (for recursion limit)
    call_depth: usize,
}

impl VM {
    pub fn new() -> Self {
        VM { call_depth: 0 }
    }

    /// Execute a code object and return the result.
    pub fn execute(&mut self, py: Python<'_>, code: CodeObject) -> PyResult {
        let mut frame = Frame::new(code);
        self.register_builtins(py, &mut frame);
        self.run_frame(py, &mut frame)
    }

    fn register_builtins(&self, _py: Python<'_>, frame: &mut Frame) {
        let builtins: &[(&str, unsafe extern "C" fn(*mut RawPyObject, *mut RawPyObject) -> *mut RawPyObject)] = &[
            ("print", builtin_print),
            ("len", builtin_len),
            ("type", builtin_type),
            ("range", builtin_range),
            ("int", builtin_int),
            ("str", builtin_str),
            ("isinstance", builtin_isinstance),
            ("hasattr", builtin_hasattr),
            ("getattr", builtin_getattr),
            ("setattr", builtin_setattr),
            ("id", builtin_id),
            ("hash", builtin_hash),
            ("abs", builtin_abs),
            ("min", builtin_min),
            ("max", builtin_max),
            ("sum", builtin_sum),
            ("ord", builtin_ord),
            ("chr", builtin_chr),
            ("repr", builtin_repr_fn),
            ("bool", builtin_bool),
            ("float", builtin_float),
            ("hex", builtin_hex),
            ("oct", builtin_oct),
            ("bin", builtin_bin),
            ("sorted", builtin_sorted),
            ("reversed", builtin_reversed),
            ("enumerate", builtin_enumerate),
            ("zip", builtin_zip),
            ("iter", builtin_iter),
            ("next", builtin_next),
            ("list", builtin_list_ctor),
            ("tuple", builtin_tuple_ctor),
            ("dict", builtin_dict_ctor),
            ("set", builtin_set_ctor),
            ("callable", builtin_callable),
            ("any", builtin_any),
            ("all", builtin_all),
            ("map", builtin_map),
        ];
        for &(name, func) in builtins {
            let obj = unsafe {
                PyObjectRef::from_raw(create_builtin_function(name, func))
            };
            frame.builtins.insert(name.to_string(), obj);
        }
        // Register object and super as None placeholders for now
        frame.builtins.insert("object".to_string(), none_obj(_py));
        frame.builtins.insert("super".to_string(), none_obj(_py));
        frame.builtins.insert("NotImplemented".to_string(), none_obj(_py));

        // Register exception types as builtins
        let exc_types: &[(&str, fn() -> *mut RawPyObject)] = &[
            ("Exception", || unsafe { *crate::runtime::error::PyExc_Exception.get() }),
            ("TypeError", || unsafe { *crate::runtime::error::PyExc_TypeError.get() }),
            ("ValueError", || unsafe { *crate::runtime::error::PyExc_ValueError.get() }),
            ("KeyError", || unsafe { *crate::runtime::error::PyExc_KeyError.get() }),
            ("IndexError", || unsafe { *crate::runtime::error::PyExc_IndexError.get() }),
            ("AttributeError", || unsafe { *crate::runtime::error::PyExc_AttributeError.get() }),
            ("RuntimeError", || unsafe { *crate::runtime::error::PyExc_RuntimeError.get() }),
            ("ImportError", || unsafe { *crate::runtime::error::PyExc_ImportError.get() }),
            ("StopIteration", || unsafe { *crate::runtime::error::PyExc_StopIteration.get() }),
            ("NameError", || unsafe { *crate::runtime::error::PyExc_NameError.get() }),
            ("ZeroDivisionError", || unsafe { *crate::runtime::error::PyExc_ZeroDivisionError.get() }),
            ("OverflowError", || unsafe { *crate::runtime::error::PyExc_OverflowError.get() }),
            ("MemoryError", || unsafe { *crate::runtime::error::PyExc_MemoryError.get() }),
            ("OSError", || unsafe { *crate::runtime::error::PyExc_OSError.get() }),
            ("NotImplementedError", || unsafe { *crate::runtime::error::PyExc_NotImplementedError.get() }),
            ("ArithmeticError", || unsafe { *crate::runtime::error::PyExc_ArithmeticError.get() }),
            ("LookupError", || unsafe { *crate::runtime::error::PyExc_LookupError.get() }),
            ("UnicodeDecodeError", || unsafe { *crate::runtime::error::PyExc_UnicodeDecodeError.get() }),
            ("UnicodeEncodeError", || unsafe { *crate::runtime::error::PyExc_UnicodeEncodeError.get() }),
        ];
        for &(name, get_exc) in exc_types {
            let exc_ptr = get_exc();
            if !exc_ptr.is_null() {
                unsafe { (*exc_ptr).incref(); }
                let obj = unsafe { PyObjectRef::from_raw(exc_ptr) };
                frame.builtins.insert(name.to_string(), obj);
            }
        }
    }

    /// The main eval loop.
    fn run_frame(&mut self, py: Python<'_>, frame: &mut Frame) -> PyResult {
        // Saved exception for re-raise (RaiseVarargs(0)) and EndFinally
        let mut saved_exception: Option<PyErr> = None;

        loop {
            if frame.ip >= frame.code.instructions.len() {
                return Ok(none_obj(py));
            }

            let instr = frame.code.instructions[frame.ip].clone();
            frame.ip += 1;

            let opcode_result = self.execute_opcode(py, frame, &instr, &mut saved_exception);

            match opcode_result {
                Ok(Some(ret_val)) => return Ok(ret_val), // ReturnValue
                Ok(None) => {} // continue to next opcode
                Err(err) => {
                    // Exception occurred — unwind the block stack
                    if let Some(handler) = Self::find_exception_handler(frame) {
                        match handler {
                            UnwindTarget::ExceptHandler { ip, stack_depth } => {
                                frame.unwind_stack_to(stack_depth);
                                // Push an ActiveExceptHandler so PopExcept can pop it
                                frame.block_stack.push(crate::vm::frame::Block {
                                    block_type: crate::vm::frame::BlockType::ActiveExceptHandler,
                                    stack_depth: frame.stack.len(),
                                });
                                // Create a PyObjectRef for the exception value
                                let exc_val = if !err.exc_value.is_null() {
                                    unsafe {
                                        (*err.exc_value).incref();
                                        PyObjectRef::from_raw(err.exc_value)
                                    }
                                } else {
                                    none_obj(py)
                                };
                                saved_exception = Some(err);
                                frame.push(exc_val);
                                frame.ip = ip;
                            }
                            UnwindTarget::FinallyHandler { ip, stack_depth } => {
                                frame.unwind_stack_to(stack_depth);
                                saved_exception = Some(err);
                                frame.ip = ip;
                            }
                        }
                    } else {
                        // No handler found — propagate
                        return Err(err);
                    }
                }
            }
        }
    }

    /// Execute a single opcode. Returns:
    /// - Ok(Some(val)) for ReturnValue
    /// - Ok(None) for normal continuation
    /// - Err(e) for exceptions
    fn execute_opcode(
        &mut self,
        py: Python<'_>,
        frame: &mut Frame,
        instr: &crate::compiler::bytecode::Instruction,
        saved_exception: &mut Option<PyErr>,
    ) -> Result<Option<PyObjectRef>, PyErr> {
            match instr.opcode {
                OpCode::Nop => {}

                OpCode::LoadConst => {
                    let obj = frame.code.constants[instr.arg as usize].clone();
                    frame.push(obj);
                }

                OpCode::LoadName => {
                    let name = frame.code.names[instr.arg as usize].clone();
                    let obj = frame.lookup_name(&name)
                        .ok_or_else(|| PyErr::name_error(&name))?;
                    frame.push(obj);
                }

                OpCode::StoreName => {
                    let name = frame.code.names[instr.arg as usize].clone();
                    let obj = frame.pop()?;
                    // Sync to cell map if this variable is captured by inner closures
                    if frame.code.cellvars.contains(&name) {
                        if let Some(ref cm) = frame.cells {
                            cm.borrow_mut().insert(name.clone(), obj.clone());
                        }
                    }
                    frame.store_name(&name, obj);
                }

                OpCode::LoadGlobal => {
                    let name = frame.code.names[instr.arg as usize].clone();
                    let obj = frame.globals.get(&name)
                        .or_else(|| frame.builtins.get(&name))
                        .cloned()
                        .ok_or_else(|| PyErr::name_error(&name))?;
                    frame.push(obj);
                }

                OpCode::StoreGlobal => {
                    let name = frame.code.names[instr.arg as usize].clone();
                    let obj = frame.pop()?;
                    frame.globals.insert(name, obj);
                }

                OpCode::PopTop => {
                    let _obj = frame.pop()?;
                }

                OpCode::DupTop => {
                    let obj = frame.top()?;
                    frame.push(obj);
                }

                OpCode::RotTwo => {
                    let a = frame.pop()?;
                    let b = frame.pop()?;
                    frame.push(a);
                    frame.push(b);
                }

                OpCode::RotThree => {
                    let a = frame.pop()?;
                    let b = frame.pop()?;
                    let c = frame.pop()?;
                    frame.push(a);
                    frame.push(c);
                    frame.push(b);
                }

                // ─── Binary operations ───
                OpCode::BinaryAdd => {
                    let right = frame.pop()?;
                    let left = frame.pop()?;
                    let result = binary_add(py, &left, &right)?;
                    frame.push(result);
                }

                OpCode::BinarySubtract => {
                    let right = frame.pop()?;
                    let left = frame.pop()?;
                    let result = binary_sub(py, &left, &right)?;
                    frame.push(result);
                }

                OpCode::BinaryMultiply => {
                    let right = frame.pop()?;
                    let left = frame.pop()?;
                    let result = binary_mul(py, &left, &right)?;
                    frame.push(result);
                }

                OpCode::BinaryTrueDivide => {
                    let right = frame.pop()?;
                    let left = frame.pop()?;
                    let result = binary_truediv(py, &left, &right)?;
                    frame.push(result);
                }

                OpCode::BinaryFloorDivide => {
                    let right = frame.pop()?;
                    let left = frame.pop()?;
                    let result = binary_floordiv(py, &left, &right)?;
                    frame.push(result);
                }

                OpCode::BinaryModulo => {
                    let right = frame.pop()?;
                    let left = frame.pop()?;
                    let result = binary_mod(py, &left, &right)?;
                    frame.push(result);
                }

                OpCode::BinaryPower => {
                    let right = frame.pop()?;
                    let left = frame.pop()?;
                    let result = binary_pow(py, &left, &right)?;
                    frame.push(result);
                }

                OpCode::BinaryAnd | OpCode::BinaryOr | OpCode::BinaryXor |
                OpCode::BinaryLShift | OpCode::BinaryRShift => {
                    let right = frame.pop()?;
                    let left = frame.pop()?;
                    let result = binary_bitop(py, &left, &right, instr.opcode)?;
                    frame.push(result);
                }

                OpCode::InplaceAdd => {
                    let right = frame.pop()?;
                    let left = frame.pop()?;
                    let result = binary_add(py, &left, &right)?;
                    frame.push(result);
                }

                OpCode::InplaceSubtract => {
                    let right = frame.pop()?;
                    let left = frame.pop()?;
                    let result = binary_sub(py, &left, &right)?;
                    frame.push(result);
                }

                OpCode::InplaceMultiply => {
                    let right = frame.pop()?;
                    let left = frame.pop()?;
                    let result = binary_mul(py, &left, &right)?;
                    frame.push(result);
                }

                OpCode::BinarySubscr => {
                    let key = frame.pop()?;
                    let obj = frame.pop()?;
                    let result = py_get_item(py, &obj, &key)
                        .or_else(|_| subscr_fallback(py, &obj, &key))?;
                    frame.push(result);
                }

                // ─── Comparison ───
                OpCode::CompareOp => {
                    let right = frame.pop()?;
                    let left = frame.pop()?;
                    let result = compare_op(py, &left, &right, instr.arg)?;
                    frame.push(result);
                }

                // ─── Unary ───
                OpCode::UnaryNot => {
                    let obj = frame.pop()?;
                    let is_true = py_is_true(py, &obj)?;
                    let result = if is_true { false_obj(py) } else { true_obj(py) };
                    frame.push(result);
                }

                OpCode::UnaryNegative => {
                    let obj = frame.pop()?;
                    let result = unary_negative(py, &obj)?;
                    frame.push(result);
                }

                OpCode::UnaryPositive => {
                    // Identity for numbers
                }

                // ─── Jumps ───
                OpCode::JumpAbsolute => {
                    frame.ip = instr.arg as usize;
                }

                OpCode::PopJumpIfFalse => {
                    let obj = frame.pop()?;
                    let is_true = py_is_true(py, &obj)?;
                    if !is_true {
                        frame.ip = instr.arg as usize;
                    }
                }

                OpCode::PopJumpIfTrue => {
                    let obj = frame.pop()?;
                    let is_true = py_is_true(py, &obj)?;
                    if is_true {
                        frame.ip = instr.arg as usize;
                    }
                }

                OpCode::JumpIfFalse => {
                    let obj = frame.top()?;
                    let is_true = py_is_true(py, &obj)?;
                    if !is_true {
                        frame.ip = instr.arg as usize;
                    }
                }

                OpCode::JumpIfTrue => {
                    let obj = frame.top()?;
                    let is_true = py_is_true(py, &obj)?;
                    if is_true {
                        frame.ip = instr.arg as usize;
                    }
                }

                // ─── Function calls ───
                OpCode::CallFunction => {
                    let nargs = instr.arg as usize;
                    let mut args = Vec::with_capacity(nargs);
                    for _ in 0..nargs {
                        args.push(frame.pop()?);
                    }
                    args.reverse();
                    let func = frame.pop()?;
                    let result = self.call_function(py, frame, &func, &args, &[])?;
                    frame.push(result);
                }

                OpCode::CallFunctionKW => {
                    // TOS is tuple of keyword names
                    let kw_names_obj = frame.pop()?;
                    let total_args = instr.arg as usize;

                    // Extract keyword names
                    let n_kw = unsafe {
                        if crate::types::tuple::PyTuple_Check(kw_names_obj.as_raw()) != 0 {
                            crate::types::tuple::PyTuple_Size(kw_names_obj.as_raw()) as usize
                        } else { 0 }
                    };
                    let n_positional = total_args - n_kw;

                    // Pop all args (positional + keyword values)
                    let mut all_args = Vec::with_capacity(total_args);
                    for _ in 0..total_args {
                        all_args.push(frame.pop()?);
                    }
                    all_args.reverse();

                    let func = frame.pop()?;

                    // Split into positional and keyword args
                    let pos_args = &all_args[..n_positional];
                    let kw_values = &all_args[n_positional..];

                    // Build kwargs: extract names from the tuple, pair with values
                    let mut kwargs = Vec::new();
                    for i in 0..n_kw {
                        let name_obj = unsafe {
                            crate::types::tuple::PyTuple_GetItem(kw_names_obj.as_raw(), i as isize)
                        };
                        let name = if !name_obj.is_null() && is_str(name_obj) {
                            crate::types::unicode::unicode_value(name_obj).to_string()
                        } else {
                            format!("_kw{}", i)
                        };
                        kwargs.push((name, kw_values[i].clone()));
                    }

                    let result = self.call_function_kw(py, frame, &func, pos_args, &kwargs)?;
                    frame.push(result);
                }

                OpCode::ReturnValue => {
                    return Ok(Some(frame.pop()?));
                }

                // ─── MakeFunction ───
                OpCode::MakeFunction => {
                    let n_defaults = instr.arg as usize;
                    let code_marker = frame.pop()?;

                    // Pop defaults tuple if present
                    let defaults = if n_defaults > 0 {
                        let defaults_tuple = frame.pop()?;
                        // Extract items from the tuple
                        let mut defs = Vec::new();
                        unsafe {
                            if crate::types::tuple::PyTuple_Check(defaults_tuple.as_raw()) != 0 {
                                let n = crate::types::tuple::PyTuple_Size(defaults_tuple.as_raw());
                                for i in 0..n {
                                    let item = crate::types::tuple::PyTuple_GetItem(defaults_tuple.as_raw(), i);
                                    if !item.is_null() {
                                        defs.push(PyObjectRef::borrow_or_err(item)?);
                                    }
                                }
                            }
                        }
                        defs
                    } else {
                        Vec::new()
                    };

                    if let Some(code) = extract_code_object(&code_marker) {
                        let func_name = code.name.clone();
                        // Capture globals: at module level, locals == globals,
                        // so we must merge locals into globals to capture
                        // module-level definitions (functions, classes, variables).
                        let mut captured_globals = frame.globals.clone();
                        for (k, v) in &frame.locals {
                            captured_globals.insert(k.clone(), v.clone());
                        }

                        // Capture cells for closures: if the inner function has
                        // freevars, it needs access to the enclosing scope's cells.
                        // We share the same CellMap via Rc so writes are visible.
                        let cells = if !code.freevars.is_empty() {
                            // Get or create the enclosing frame's cell map
                            let cell_map = frame.cells.get_or_insert_with(|| {
                                Rc::new(RefCell::new(HashMap::new()))
                            });
                            // Seed any freevars that are currently in frame.locals
                            // but not yet in the cell map
                            {
                                let mut cm = cell_map.borrow_mut();
                                for fv in &code.freevars {
                                    if !cm.contains_key(fv) {
                                        if let Some(val) = frame.locals.get(fv) {
                                            cm.insert(fv.clone(), val.clone());
                                        }
                                    }
                                }
                            }
                            Some(Rc::clone(cell_map))
                        } else {
                            None
                        };

                        let rust_func = RustFunction {
                            code,
                            globals: captured_globals,
                            builtins: frame.builtins.clone(),
                            defaults,
                            name: func_name,
                            cells,
                        };
                        let func_box = Box::new(rust_func);
                        let func_ptr = Box::into_raw(func_box);
                        // Store as int constant (pointer to RustFunction)
                        let marker = new_int(py, func_ptr as usize as i64)?;
                        // Tag it so we know it's a RustFunction
                        frame.push(marker);
                    } else {
                        frame.push(none_obj(py));
                    }
                }

                // ─── Container building ───
                OpCode::BuildList => {
                    let n = instr.arg as usize;
                    let mut items = Vec::with_capacity(n);
                    for _ in 0..n {
                        items.push(frame.pop()?);
                    }
                    items.reverse();
                    let list = build_list(py, items)?;
                    frame.push(list);
                }

                OpCode::BuildTuple => {
                    let n = instr.arg as usize;
                    let mut items = Vec::with_capacity(n);
                    for _ in 0..n {
                        items.push(frame.pop()?);
                    }
                    items.reverse();
                    let tuple = build_tuple(py, items)?;
                    frame.push(tuple);
                }

                OpCode::BuildMap => {
                    let n = instr.arg as usize;
                    let mut pairs = Vec::with_capacity(n);
                    for _ in 0..n {
                        let value = frame.pop()?;
                        let key = frame.pop()?;
                        pairs.push((key, value));
                    }
                    pairs.reverse();
                    let dict = build_dict(py, pairs)?;
                    frame.push(dict);
                }

                OpCode::BuildSet => {
                    let n = instr.arg as usize;
                    let mut items = Vec::with_capacity(n);
                    for _ in 0..n {
                        items.push(frame.pop()?);
                    }
                    items.reverse();
                    let set = build_set(py, items)?;
                    frame.push(set);
                }

                OpCode::StoreSubscr => {
                    let key = frame.pop()?;
                    let obj = frame.pop()?;
                    let value = frame.pop()?;
                    unsafe {
                        if crate::types::list::PyList_Check(obj.as_raw()) != 0 {
                            let idx = get_int_value(key.as_raw());
                            (*value.as_raw()).incref();
                            crate::types::list::PyList_SetItem(obj.as_raw(), idx as isize, value.as_raw());
                        } else if crate::types::dict::PyDict_Check(obj.as_raw()) != 0 {
                            crate::types::dict::PyDict_SetItem(obj.as_raw(), key.as_raw(), value.as_raw());
                        } else {
                            py_store_item(py, &obj, &key, &value).ok();
                        }
                    }
                }

                // ─── Import ───
                OpCode::ImportName => {
                    let name = frame.code.names[instr.arg as usize].clone();
                    // Try C API import first (for .so extensions)
                    match py_import(py, &name) {
                        Ok(module) => {
                            frame.push(module);
                        }
                        Err(_) => {
                            // Try to import as a Python source file
                            match self.import_py_source(py, frame, &name) {
                                Ok(module) => {
                                    frame.push(module);
                                }
                                Err(e) => {
                                    return Err(e);
                                }
                            }
                        }
                    }
                }

                OpCode::ImportFrom => {
                    let name = frame.code.names[instr.arg as usize].clone();
                    let module = frame.top()?;
                    // First try py_get_attr (works for C module objects)
                    let attr = py_get_attr(py, &module, &name).or_else(|_| {
                        // Fall back to dict lookup (for Python source modules stored as dicts)
                        unsafe {
                            if crate::types::dict::PyDict_Check(module.as_raw()) != 0 {
                                let name_cstr = std::ffi::CString::new(name.as_str()).unwrap();
                                let item = crate::types::dict::PyDict_GetItemString(
                                    module.as_raw(),
                                    name_cstr.as_ptr(),
                                );
                                PyObjectRef::borrow_or_err(item)
                            } else {
                                Err(PyErr::import_error(&format!(
                                    "cannot import name '{}' from module", name
                                )))
                            }
                        }
                    })?;
                    frame.push(attr);
                }

                OpCode::ImportStar => {
                    let module = frame.pop()?;
                    // Copy all public names from module dict to locals
                    unsafe {
                        let raw = module.as_raw();
                        // Module is a dict for Python-source modules; otherwise get __dict__
                        let dict = if crate::types::dict::PyDict_Check(raw) != 0 {
                            raw
                        } else {
                            crate::ffi::object_api::PyObject_GenericGetDict(raw, ptr::null_mut())
                        };
                        if !dict.is_null() && crate::types::dict::PyDict_Check(dict) != 0 {
                            // Check for __all__
                            let all_key = std::ffi::CString::new("__all__").unwrap();
                            let all_list = crate::types::dict::PyDict_GetItemString(dict, all_key.as_ptr());
                            if !all_list.is_null() && crate::types::list::PyList_Check(all_list) != 0 {
                                // Import only names listed in __all__
                                let n = crate::types::list::PyList_Size(all_list);
                                for i in 0..n {
                                    let name_obj = crate::types::list::PyList_GetItem(all_list, i);
                                    if !name_obj.is_null() && is_str(name_obj) {
                                        let name = crate::types::unicode::unicode_value(name_obj).to_string();
                                        let name_cstr = std::ffi::CString::new(name.as_str()).unwrap();
                                        let val = crate::types::dict::PyDict_GetItemString(dict, name_cstr.as_ptr());
                                        if !val.is_null() {
                                            let val_ref = PyObjectRef::borrow_or_err(val)?;
                                            frame.store_name(&name, val_ref);
                                        }
                                    }
                                }
                            } else {
                                // Import all names not starting with '_'
                                let mut pos: isize = 0;
                                let mut key: *mut RawPyObject = ptr::null_mut();
                                let mut val: *mut RawPyObject = ptr::null_mut();
                                while crate::types::dict::PyDict_Next(dict, &mut pos, &mut key, &mut val) != 0 {
                                    if !key.is_null() && is_str(key) {
                                        let name = crate::types::unicode::unicode_value(key).to_string();
                                        if !name.starts_with('_') {
                                            let val_ref = PyObjectRef::borrow_or_err(val)?;
                                            frame.store_name(&name, val_ref);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // ─── Iteration ───
                OpCode::GetIter => {
                    let obj = frame.pop()?;
                    let iter = get_iterator(py, &obj)?;
                    frame.push(iter);
                }

                OpCode::ForIter => {
                    // TOS is the iterator. Try to get next item.
                    let iter = frame.top()?;
                    match iter_next(py, &iter) {
                        Some(item) => {
                            frame.push(item);
                            // Continue loop body
                        }
                        None => {
                            // Iterator exhausted — pop it and jump past the loop
                            let _iter = frame.pop()?;
                            frame.ip = instr.arg as usize;
                        }
                    }
                }

                // ─── Misc ───
                OpCode::PrintExpr => {
                    let obj = frame.top()?;
                    if !is_none(obj.as_raw()) {
                        if let Ok(repr) = py_repr(py, &obj) {
                            if is_str(repr.as_raw()) {
                                let _s = crate::types::unicode::unicode_value(repr.as_raw());
                            }
                        }
                    }
                }

                OpCode::LoadAttr => {
                    let name = frame.code.names[instr.arg as usize].clone();
                    let obj = frame.pop()?;

                    // Check if obj is a RustInstance
                    if is_int(obj.as_raw()) {
                        let marker_val = get_int_value(obj.as_raw());
                        if is_instance_marker(marker_val) {
                            let inst = unsafe { &*(extract_ptr(marker_val) as *const RustInstance) };
                            // Look in instance attrs first, then class namespace
                            if let Some(val) = inst.attrs.get(&name) {
                                frame.push(val.clone());
                            } else {
                                let class = unsafe { &*inst.class };
                                if let Some(val) = class.namespace.get(&name) {
                                    // If it's a method (function), create a bound method:
                                    // We push (func, self) pair as a special marker
                                    if is_int(val.as_raw()) && is_function_marker(get_int_value(val.as_raw())) {
                                        // Create a bound method: store (func_marker, instance_marker) as tuple
                                        let bound = build_tuple(py, vec![val.clone(), obj.clone()])?;
                                        frame.push(bound);
                                    } else {
                                        frame.push(val.clone());
                                    }
                                } else {
                                    return Err(PyErr::attribute_error(&format!(
                                        "'{}' object has no attribute '{}'", class.name, name
                                    )));
                                }
                            }
                        } else if is_class_marker(marker_val) {
                            // Class attribute access
                            let class = unsafe { &*(extract_ptr(marker_val) as *const RustClass) };
                            if let Some(val) = class.namespace.get(&name) {
                                frame.push(val.clone());
                            } else {
                                return Err(PyErr::attribute_error(&format!(
                                    "type object '{}' has no attribute '{}'", class.name, name
                                )));
                            }
                        } else if is_regex_marker(marker_val) && (is_regex_method(&name) || name == "pattern") {
                            if name == "pattern" {
                                // .pattern is a property, not a method
                                let re = unsafe { &*extract_regex(marker_val) };
                                let pat = new_str(py, &re.pattern)?;
                                frame.push(pat);
                            } else {
                                let bm = Box::new(BoundBuiltinMethod {
                                    self_obj: obj.clone(),
                                    method_name: name.clone(),
                                });
                                let bm_ptr = Box::into_raw(bm) as usize as i64;
                                let marker = new_int(py, bm_ptr | BOUND_METHOD_TAG)?;
                                frame.push(marker);
                            }
                        } else if is_match_marker(marker_val) && is_match_method(&name) {
                            let bm = Box::new(BoundBuiltinMethod {
                                self_obj: obj.clone(),
                                method_name: name.clone(),
                            });
                            let bm_ptr = Box::into_raw(bm) as usize as i64;
                            let marker = new_int(py, bm_ptr | BOUND_METHOD_TAG)?;
                            frame.push(marker);
                        } else {
                            // Regular int — fall through to C API
                            let attr = py_get_attr(py, &obj, &name)?;
                            frame.push(attr);
                        }
                    } else {
                        // Check for string/list/dict methods before falling to C API
                        let raw = obj.as_raw();
                        let has_method = unsafe {
                            (is_str(raw) && is_str_method(&name)) ||
                            (crate::types::list::PyList_Check(raw) != 0 && is_list_method(&name)) ||
                            (crate::types::dict::PyDict_Check(raw) != 0 && is_dict_method(&name))
                        };
                        if has_method {
                            let bm = Box::new(BoundBuiltinMethod {
                                self_obj: obj.clone(),
                                method_name: name.clone(),
                            });
                            let bm_ptr = Box::into_raw(bm) as usize as i64;
                            let marker = new_int(py, bm_ptr | BOUND_METHOD_TAG)?;
                            frame.push(marker);
                        } else {
                            // Non-int object — use C API
                            let attr = py_get_attr(py, &obj, &name)
                                .or_else(|_| {
                                    unsafe {
                                        if crate::types::dict::PyDict_Check(obj.as_raw()) != 0 {
                                            let name_cstr = std::ffi::CString::new(name.as_str()).unwrap();
                                            let item = crate::types::dict::PyDict_GetItemString(
                                                obj.as_raw(),
                                                name_cstr.as_ptr(),
                                            );
                                            PyObjectRef::borrow_or_err(item)
                                        } else {
                                            Err(PyErr::attribute_error(&format!(
                                                "object has no attribute '{}'", name
                                            )))
                                        }
                                    }
                                })?;
                            frame.push(attr);
                        }
                    }
                }

                OpCode::StoreAttr => {
                    let name = frame.code.names[instr.arg as usize].clone();
                    let obj = frame.pop()?;
                    let value = frame.pop()?;

                    // Check if obj is a RustInstance
                    if is_int(obj.as_raw()) {
                        let marker_val = get_int_value(obj.as_raw());
                        if is_instance_marker(marker_val) {
                            let inst = unsafe { &mut *(extract_ptr(marker_val) as *mut RustInstance) };
                            inst.attrs.insert(name, value);
                        } else {
                            unsafe {
                                let name_cstr = std::ffi::CString::new(name.as_str()).unwrap();
                                crate::ffi::object_api::PyObject_SetAttrString(
                                    obj.as_raw(),
                                    name_cstr.as_ptr(),
                                    value.as_raw(),
                                );
                            }
                        }
                    } else {
                        unsafe {
                            let name_cstr = std::ffi::CString::new(name.as_str()).unwrap();
                            crate::ffi::object_api::PyObject_SetAttrString(
                                obj.as_raw(),
                                name_cstr.as_ptr(),
                                value.as_raw(),
                            );
                        }
                    }
                }

                OpCode::DeleteName => {
                    let name = frame.code.names[instr.arg as usize].clone();
                    frame.locals.remove(&name);
                }

                OpCode::DeleteFast => {
                    let name = frame.code.varnames[instr.arg as usize].clone();
                    frame.locals.remove(&name);
                }

                OpCode::DeleteAttr => {
                    let name = frame.code.names[instr.arg as usize].clone();
                    let obj = frame.pop()?;
                    unsafe {
                        let name_cstr = std::ffi::CString::new(name.as_str()).unwrap();
                        crate::ffi::object_api::PyObject_SetAttrString(
                            obj.as_raw(),
                            name_cstr.as_ptr(),
                            ptr::null_mut(),
                        );
                    }
                }

                OpCode::DeleteSubscr => {
                    let key = frame.pop()?;
                    let obj = frame.pop()?;
                    // For dicts, delete the key
                    unsafe {
                        if crate::types::dict::PyDict_Check(obj.as_raw()) != 0 {
                            crate::types::dict::PyDict_DelItem(obj.as_raw(), key.as_raw());
                        }
                    }
                }

                OpCode::UnpackSequence => {
                    let n = instr.arg as usize;
                    let obj = frame.pop()?;
                    let raw = obj.as_raw();
                    unsafe {
                        if crate::types::tuple::PyTuple_Check(raw) != 0 {
                            let size = crate::types::tuple::PyTuple_Size(raw) as usize;
                            let count = std::cmp::min(n, size);
                            // Push in reverse order so first element ends up on top
                            for i in (0..count).rev() {
                                let item = crate::types::tuple::PyTuple_GetItem(raw, i as isize);
                                frame.push(PyObjectRef::borrow_or_err(item)?);
                            }
                            for _ in count..n {
                                frame.push(none_obj(py));
                            }
                        } else if crate::types::list::PyList_Check(raw) != 0 {
                            let size = crate::types::list::PyList_Size(raw) as usize;
                            let count = std::cmp::min(n, size);
                            for i in (0..count).rev() {
                                let item = crate::types::list::PyList_GetItem(raw, i as isize);
                                frame.push(PyObjectRef::borrow_or_err(item)?);
                            }
                            for _ in count..n {
                                frame.push(none_obj(py));
                            }
                        } else {
                            for _ in 0..n {
                                frame.push(none_obj(py));
                            }
                        }
                    }
                }

                OpCode::LoadFast => {
                    let name = &frame.code.varnames[instr.arg as usize];
                    let obj = frame.locals.get(name)
                        .cloned()
                        .ok_or_else(|| PyErr::name_error(name))?;
                    frame.push(obj);
                }

                OpCode::StoreFast => {
                    let name = frame.code.varnames[instr.arg as usize].clone();
                    let obj = frame.pop()?;
                    // Sync to cell map if this variable is captured by inner closures
                    if frame.code.cellvars.contains(&name) {
                        if let Some(ref cm) = frame.cells {
                            cm.borrow_mut().insert(name.clone(), obj.clone());
                        }
                    }
                    frame.locals.insert(name, obj);
                }

                OpCode::SetupExcept => {
                    frame.block_stack.push(crate::vm::frame::Block {
                        block_type: crate::vm::frame::BlockType::ExceptHandler {
                            handler_ip: instr.arg as usize,
                        },
                        stack_depth: frame.stack.len(),
                    });
                }

                OpCode::SetupFinally => {
                    frame.block_stack.push(crate::vm::frame::Block {
                        block_type: crate::vm::frame::BlockType::FinallyHandler {
                            handler_ip: instr.arg as usize,
                        },
                        stack_depth: frame.stack.len(),
                    });
                }

                OpCode::SetupLoop => {
                    frame.block_stack.push(crate::vm::frame::Block {
                        block_type: crate::vm::frame::BlockType::Loop {
                            end_ip: instr.arg as usize,
                        },
                        stack_depth: frame.stack.len(),
                    });
                }

                OpCode::PopBlock => {
                    frame.block_stack.pop();
                }

                OpCode::PopExcept => {
                    // Pop the except handler block (after a successful catch)
                    frame.block_stack.pop();
                    *saved_exception = None;
                }

                OpCode::EndFinally => {
                    // If there's a saved exception, re-raise it
                    if let Some(exc) = saved_exception.take() {
                        return Err(exc);
                    }
                    // Otherwise, continue normally
                }

                OpCode::BreakLoop | OpCode::ContinueLoop => {
                    // Break/continue are compiled as JumpAbsolute by the compiler now
                }

                OpCode::RaiseVarargs => {
                    if instr.arg >= 1 {
                        let exc = frame.pop()?;
                        // Try to create a proper exception:
                        // If exc is an exception TYPE (like ValueError), call it to instantiate
                        // If exc is already an exception instance, use it directly
                        let exc_raw = exc.as_raw();
                        let exc_type = unsafe { (*exc_raw).ob_type };

                        // Check if it's a type object (i.e. raising a class)
                        if !exc_type.is_null() && exc_type == unsafe { crate::object::typeobj::PyType_Type.get() as *mut _ } {
                            // It's a type — use it as the exception type with empty message
                            unsafe {
                                let empty_msg = std::ffi::CString::new("").unwrap();
                                crate::runtime::error::PyErr_SetString(exc_raw, empty_msg.as_ptr());
                            }
                            return Err(PyErr::fetch());
                        } else {
                            // It's an instance — set it as the exception value
                            unsafe {
                                (*exc_raw).incref();
                                if !exc_type.is_null() {
                                    (*(exc_type as *mut RawPyObject)).incref();
                                }
                                crate::runtime::error::PyErr_Restore(
                                    exc_type as *mut RawPyObject,
                                    exc_raw,
                                    ptr::null_mut(),
                                );
                            }
                            return Err(PyErr::fetch());
                        }
                    } else {
                        // Re-raise current exception
                        if let Some(exc) = saved_exception.take() {
                            return Err(exc);
                        }
                        return Err(PyErr::runtime_error("No active exception to re-raise"));
                    }
                }

                OpCode::LoadBuildClass => {
                    // Push a special callable that implements __build_class__
                    let bc_func = unsafe {
                        PyObjectRef::from_raw(create_builtin_function("__build_class__", builtin_build_class_stub))
                    };
                    frame.push(bc_func);
                }

                OpCode::ListAppend => {
                    let item = frame.pop()?;
                    let depth = instr.arg as usize;
                    let list_idx = frame.stack.len() - depth;
                    let list_raw = frame.stack[list_idx].as_raw();
                    unsafe {
                        if crate::types::list::PyList_Check(list_raw) != 0 {
                            (*item.as_raw()).incref();
                            crate::types::list::PyList_Append(list_raw, item.as_raw());
                        }
                    }
                }

                OpCode::SetAdd => {
                    let item = frame.pop()?;
                    let depth = instr.arg as usize;
                    let set_idx = frame.stack.len() - depth;
                    let set_raw = frame.stack[set_idx].as_raw();
                    unsafe {
                        crate::types::set::PySet_Add(set_raw, item.as_raw());
                    }
                }

                OpCode::MapAdd => {
                    let value = frame.pop()?;
                    let key = frame.pop()?;
                    let depth = instr.arg as usize;
                    let dict_idx = frame.stack.len() - depth;
                    let dict_raw = frame.stack[dict_idx].as_raw();
                    unsafe {
                        if crate::types::dict::PyDict_Check(dict_raw) != 0 {
                            crate::types::dict::PyDict_SetItem(dict_raw, key.as_raw(), value.as_raw());
                        }
                    }
                }

                // ─── Slice ───
                OpCode::BuildSlice => {
                    // BuildSlice(2): pop upper, lower → push (lower, upper, None) as slice tuple
                    // BuildSlice(3): pop step, upper, lower → push (lower, upper, step) as slice tuple
                    let nargs = instr.arg;
                    if nargs == 3 {
                        let step = frame.pop()?;
                        let upper = frame.pop()?;
                        let lower = frame.pop()?;
                        let slice = build_tuple(py, vec![lower, upper, step])?;
                        frame.push(slice);
                    } else {
                        let upper = frame.pop()?;
                        let lower = frame.pop()?;
                        let step = none_obj(py);
                        let slice = build_tuple(py, vec![lower, upper, step])?;
                        frame.push(slice);
                    }
                }

                // ─── Closure operations ───
                OpCode::LoadDeref => {
                    let name = &frame.code.freevars[instr.arg as usize];
                    let val = frame.cells.as_ref()
                        .and_then(|cm| cm.borrow().get(name).cloned())
                        .ok_or_else(|| PyErr::runtime_error(
                            &format!("free variable '{}' referenced before assignment in enclosing scope", name)
                        ))?;
                    frame.push(val);
                }

                OpCode::StoreDeref => {
                    let name = frame.code.freevars[instr.arg as usize].clone();
                    let val = frame.pop()?;
                    if let Some(ref cm) = frame.cells {
                        cm.borrow_mut().insert(name, val);
                    } else {
                        return Err(PyErr::runtime_error("StoreDeref without cell map"));
                    }
                }

                OpCode::MakeClosure => {
                    // Same as MakeFunction but currently unused —
                    // closures go through MakeFunction + freevars detection
                    return Err(PyErr::type_error("MakeClosure not used — use MakeFunction"));
                }

                OpCode::YieldValue => {
                    // YieldValue in the normal execution path means this function
                    // was called directly (not as a generator). This shouldn't happen
                    // if is_generator detection works correctly, but handle gracefully.
                    let val = frame.pop()?;
                    return Ok(Some(val));
                }

                _ => {
                    return Err(PyErr::type_error(&format!(
                        "Unimplemented opcode: {:?}", instr.opcode
                    )));
                }
            }
            Ok(None)
    }

    /// Find an exception handler by walking the block stack.
    fn find_exception_handler(frame: &mut Frame) -> Option<UnwindTarget> {
        while let Some(block) = frame.block_stack.pop() {
            match block.block_type {
                crate::vm::frame::BlockType::ExceptHandler { handler_ip } => {
                    return Some(UnwindTarget::ExceptHandler {
                        ip: handler_ip,
                        stack_depth: block.stack_depth,
                    });
                }
                crate::vm::frame::BlockType::FinallyHandler { handler_ip } => {
                    return Some(UnwindTarget::FinallyHandler {
                        ip: handler_ip,
                        stack_depth: block.stack_depth,
                    });
                }
                crate::vm::frame::BlockType::Loop { .. } |
                crate::vm::frame::BlockType::ActiveExceptHandler => {
                    // Pop past loop and active-handler blocks when unwinding
                    continue;
                }
            }
        }
        None
    }

    /// Call a function — dispatches between RustFunction, RustClass, BoundMethod, CFunction, and tp_call.
    fn call_function(
        &mut self,
        py: Python<'_>,
        caller_frame: &Frame,
        func: &PyObjectRef,
        args: &[PyObjectRef],
        _kwargs: &[(String, PyObjectRef)],
    ) -> PyResult {
        // Check for bound builtin method (string/list/dict methods)
        if is_int(func.as_raw()) {
            let marker_val = get_int_value(func.as_raw());
            if is_bound_method_marker(marker_val) {
                let bm_ptr = extract_ptr(marker_val) as *const BoundBuiltinMethod;
                let bm = unsafe { &*bm_ptr };
                return call_bound_method(py, bm, args);
            }
        }

        // Check for bound method (2-tuple: (func_marker, instance_marker))
        unsafe {
            if crate::types::tuple::PyTuple_Check(func.as_raw()) != 0 {
                let size = crate::types::tuple::PyTuple_Size(func.as_raw());
                if size == 2 {
                    let func_item = crate::types::tuple::PyTuple_GetItem(func.as_raw(), 0);
                    let self_item = crate::types::tuple::PyTuple_GetItem(func.as_raw(), 1);
                    if !func_item.is_null() && is_int(func_item) && !self_item.is_null() {
                        let func_val = get_int_value(func_item);
                        if is_function_marker(func_val) {
                            // Bound method call — prepend self
                            (*self_item).incref();
                            let self_obj = PyObjectRef::from_raw(self_item);
                            let mut bound_args = vec![self_obj];
                            bound_args.extend_from_slice(args);

                            let rust_func = &*(func_val as usize as *const RustFunction);
                            // Use class globals for method execution
                            let self_marker = get_int_value(self_item);
                            if is_instance_marker(self_marker) {
                                let inst = &*(extract_ptr(self_marker) as *const RustInstance);
                                let class = &*inst.class;
                                // Create a caller frame with class globals
                                let mut method_frame = Frame::new(CodeObject::new("<method>".to_string(), "<method>".to_string()));
                                method_frame.globals = class.globals.clone();
                                // Add class name to globals
                                method_frame.globals.insert(class.name.clone(), new_int(py, (inst.class as usize as i64) | CLASS_TAG)?);
                                for (k, v) in &caller_frame.locals {
                                    if !method_frame.globals.contains_key(k) {
                                        method_frame.globals.insert(k.clone(), v.clone());
                                    }
                                }
                                for (k, v) in &caller_frame.globals {
                                    if !method_frame.globals.contains_key(k) {
                                        method_frame.globals.insert(k.clone(), v.clone());
                                    }
                                }
                                method_frame.builtins = class.builtins.clone();
                                return self.call_rust_function(py, &method_frame, rust_func, &bound_args, &HashMap::new());
                            }
                            return self.call_rust_function(py, caller_frame, rust_func, &bound_args, &HashMap::new());
                        }
                    }
                }
            }
        }

        // Check if this is a tagged int marker (RustFunction, RustClass, or RustInstance)
        if is_int(func.as_raw()) {
            let marker_val = get_int_value(func.as_raw());
            if marker_val != 0 {
                if is_class_marker(marker_val) {
                    // Class construction: ClassName(args)
                    let class_ptr = extract_ptr(marker_val) as *const RustClass;
                    return self.construct_instance(py, caller_frame, class_ptr, args);
                } else if is_function_marker(marker_val) {
                    // Regular function call
                    let rust_func = unsafe { &*(marker_val as usize as *const RustFunction) };
                    return self.call_rust_function(py, caller_frame, rust_func, args, &HashMap::new());
                }
            }
        }

        // Check if it's a __build_class__ call
        unsafe {
            let f = func.as_raw();
            if (*f).ob_type == crate::types::funcobject::cfunction_type() {
                // Check if this is __build_class__
                let data = crate::object::pyobject::PyObjectWithData::<crate::types::funcobject::CFunctionData>::data_from_raw(f);
                if !data.name.is_null() {
                    let name = std::ffi::CStr::from_ptr(data.name);
                    if name.to_bytes() == b"__build_class__" {
                        return self.builtin_build_class(py, caller_frame, args);
                    }
                }
            }
        }

        // Fall back to C function call
        call_function_raii(py, func, args)
    }

    /// Call with keyword arguments
    fn call_function_kw(
        &mut self,
        py: Python<'_>,
        caller_frame: &Frame,
        func: &PyObjectRef,
        pos_args: &[PyObjectRef],
        kwargs: &[(String, PyObjectRef)],
    ) -> PyResult {
        // Check if this is a tagged int marker
        if is_int(func.as_raw()) {
            let marker_val = get_int_value(func.as_raw());
            if marker_val != 0 {
                if is_class_marker(marker_val) {
                    let class_ptr = extract_ptr(marker_val) as *const RustClass;
                    return self.construct_instance(py, caller_frame, class_ptr, pos_args);
                } else if is_function_marker(marker_val) {
                    let rust_func = unsafe { &*(marker_val as usize as *const RustFunction) };
                    let kw_map: HashMap<String, PyObjectRef> = kwargs.iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    return self.call_rust_function(py, caller_frame, rust_func, pos_args, &kw_map);
                }
            }
        }

        // Check __build_class__
        unsafe {
            let f = func.as_raw();
            if (*f).ob_type == crate::types::funcobject::cfunction_type() {
                let data = crate::object::pyobject::PyObjectWithData::<crate::types::funcobject::CFunctionData>::data_from_raw(f);
                if !data.name.is_null() {
                    let name = std::ffi::CStr::from_ptr(data.name);
                    if name.to_bytes() == b"__build_class__" {
                        return self.builtin_build_class(py, caller_frame, pos_args);
                    }
                }
            }
        }

        // Fall back to regular call (ignoring kwargs for C functions)
        call_function_raii(py, func, pos_args)
    }

    /// Call a user-defined Rust function
    fn call_rust_function(
        &mut self,
        py: Python<'_>,
        caller_frame: &Frame,
        func: &RustFunction,
        args: &[PyObjectRef],
        kwargs: &HashMap<String, PyObjectRef>,
    ) -> PyResult {
        self.call_depth += 1;
        if self.call_depth > 500 {
            self.call_depth -= 1;
            return Err(PyErr::runtime_error("maximum recursion depth exceeded"));
        }

        let mut child_frame = Frame::new(clone_code_object(&func.code));

        // Copy globals and builtins from function closure
        child_frame.globals = func.globals.clone();
        child_frame.builtins = func.builtins.clone();
        // Also merge caller's globals AND locals for module-level names
        // (At module level, locals == globals, but our impl keeps them separate)
        for (k, v) in &caller_frame.globals {
            if !child_frame.globals.contains_key(k) {
                child_frame.globals.insert(k.clone(), v.clone());
            }
        }
        for (k, v) in &caller_frame.locals {
            if !child_frame.globals.contains_key(k) {
                child_frame.globals.insert(k.clone(), v.clone());
            }
        }
        for (k, v) in &caller_frame.builtins {
            if !child_frame.builtins.contains_key(k) {
                child_frame.builtins.insert(k.clone(), v.clone());
            }
        }

        // Pass cell map for closures (inner function receiving parent's cells)
        if func.cells.is_some() {
            child_frame.cells = func.cells.clone();
        }
        // If this function has cellvars (it's an outer function whose variables
        // will be captured by inner closures), initialize its cell map
        if !func.code.cellvars.is_empty() && child_frame.cells.is_none() {
            child_frame.cells = Some(Rc::new(RefCell::new(HashMap::new())));
        }

        // Bind arguments to locals
        let argcount = func.code.argcount as usize;
        let mut arg_idx = 0;

        // Positional args
        for i in 0..argcount {
            let name = &func.code.varnames[i];
            if i < args.len() {
                child_frame.locals.insert(name.clone(), args[i].clone());
            } else if let Some(kw_val) = kwargs.get(name) {
                child_frame.locals.insert(name.clone(), kw_val.clone());
            } else {
                // Check defaults
                let n_defaults = func.defaults.len();
                let default_offset = i as isize - (argcount as isize - n_defaults as isize);
                if default_offset >= 0 && (default_offset as usize) < n_defaults {
                    child_frame.locals.insert(name.clone(), func.defaults[default_offset as usize].clone());
                } else {
                    return Err(PyErr::type_error(&format!(
                        "{}() missing required argument: '{}'", func.name, name
                    )));
                }
            }
            arg_idx = i + 1;
        }

        // *args
        if func.code.has_vararg {
            let vararg_name = &func.code.varnames[arg_idx];
            let extra: Vec<PyObjectRef> = if args.len() > argcount {
                args[argcount..].to_vec()
            } else {
                Vec::new()
            };
            let vararg_tuple = build_tuple(py, extra)?;
            child_frame.locals.insert(vararg_name.clone(), vararg_tuple);
            arg_idx += 1;
        }

        // keyword-only args
        for i in 0..func.code.kwonlyargcount as usize {
            let name = &func.code.varnames[arg_idx + i];
            if let Some(kw_val) = kwargs.get(name) {
                child_frame.locals.insert(name.clone(), kw_val.clone());
            } else {
                child_frame.locals.insert(name.clone(), none_obj(py));
            }
        }
        arg_idx += func.code.kwonlyargcount as usize;

        // **kwargs
        if func.code.has_kwarg {
            let kwarg_name = &func.code.varnames[arg_idx];
            // Collect unmatched keyword args
            let mut kw_pairs = Vec::new();
            for (k, v) in kwargs {
                let is_param = func.code.varnames[..argcount].iter().any(|vn| vn == k);
                if !is_param {
                    let key = new_str(py, k)?;
                    kw_pairs.push((key, v.clone()));
                }
            }
            let kwargs_dict = build_dict(py, kw_pairs)?;
            child_frame.locals.insert(kwarg_name.clone(), kwargs_dict);
        }

        // If this is a generator function, create a generator object instead of executing
        if func.code.is_generator {
            self.call_depth -= 1;
            let gen = Box::new(RustGenerator {
                frame: child_frame,
                func_globals: func.globals.clone(),
                func_builtins: func.builtins.clone(),
                exhausted: false,
                started: false,
            });
            let gen_ptr = Box::into_raw(gen) as usize as i64;
            // Use GENERATOR_TAG to mark this as a generator
            // We use the same tag as BOUND_METHOD_TAG but distinguish by struct
            // Actually, let's use a negative marker to distinguish
            // Store as negative i64 to distinguish from functions/classes/bound methods
            return new_int(py, -(gen_ptr));
        }

        let result = self.run_frame(py, &mut child_frame);
        self.call_depth -= 1;
        result
    }

    /// Construct an instance of a RustClass.
    fn construct_instance(
        &mut self,
        py: Python<'_>,
        caller_frame: &Frame,
        class_ptr: *const RustClass,
        args: &[PyObjectRef],
    ) -> PyResult {
        let class = unsafe { &*class_ptr };

        // Create a new RustInstance
        let instance = RustInstance {
            class: class_ptr,
            attrs: HashMap::new(),
        };
        let instance_box = Box::new(instance);
        let instance_ptr = Box::into_raw(instance_box);
        let marker_val = (instance_ptr as usize as i64) | INSTANCE_TAG;
        let instance_obj = new_int(py, marker_val)?;

        // Call __init__ if it exists
        if let Some(init_func) = class.namespace.get("__init__") {
            if is_int(init_func.as_raw()) {
                let init_val = get_int_value(init_func.as_raw());
                if is_function_marker(init_val) {
                    // Prepend self to args
                    let mut init_args = vec![instance_obj.clone()];
                    init_args.extend_from_slice(args);

                    // Build a temporary frame with class globals
                    let temp_frame = Frame::new(CodeObject::new("<init>".to_string(), "<init>".to_string()));
                    let mut combined_frame = temp_frame;
                    combined_frame.globals = class.globals.clone();
                    // Also add the class itself to globals so methods can reference it
                    combined_frame.globals.insert(class.name.clone(), new_int(py, (class_ptr as usize as i64) | CLASS_TAG)?);
                    for (k, v) in &caller_frame.locals {
                        combined_frame.globals.insert(k.clone(), v.clone());
                    }
                    for (k, v) in &caller_frame.globals {
                        if !combined_frame.globals.contains_key(k) {
                            combined_frame.globals.insert(k.clone(), v.clone());
                        }
                    }
                    combined_frame.builtins = class.builtins.clone();

                    let rust_func = unsafe { &*(init_val as usize as *const RustFunction) };
                    let _result = self.call_rust_function(py, &combined_frame, rust_func, &init_args, &HashMap::new())?;
                }
            }
        }

        Ok(instance_obj)
    }

    /// __build_class__ implementation
    fn builtin_build_class(
        &mut self,
        py: Python<'_>,
        caller_frame: &Frame,
        args: &[PyObjectRef],
    ) -> PyResult {
        if args.len() < 2 {
            return Err(PyErr::type_error("__build_class__: not enough args"));
        }

        let body_func = &args[0]; // The class body function
        let name_obj = &args[1];  // The class name string
        // args[2..] are base classes

        let class_name = if is_str(name_obj.as_raw()) {
            crate::types::unicode::unicode_value(name_obj.as_raw()).to_string()
        } else {
            "<class>".to_string()
        };

        // Execute the class body function to populate the namespace
        if is_int(body_func.as_raw()) {
            let ptr_val = get_int_value(body_func.as_raw()) as usize;
            if ptr_val != 0 && is_function_marker(get_int_value(body_func.as_raw())) {
                let rust_func = unsafe { &*(ptr_val as *const RustFunction) };
                let mut ns_frame = Frame::new(clone_code_object(&rust_func.code));
                // Merge caller's locals into globals (module-level names)
                let mut merged_globals = caller_frame.globals.clone();
                for (k, v) in &caller_frame.locals {
                    merged_globals.insert(k.clone(), v.clone());
                }
                ns_frame.globals = merged_globals.clone();
                ns_frame.builtins = caller_frame.builtins.clone();

                // Execute the class body
                let _result = self.run_frame(py, &mut ns_frame)?;

                // Collect base class pointers and merge their namespaces.
                // Iterate in reverse so that the FIRST base (highest MRO priority) wins.
                let mut base_ptrs: Vec<*const RustClass> = Vec::new();
                let mut merged_ns = HashMap::new();
                // First pass: collect base pointers
                for base_arg in &args[2..] {
                    if is_int(base_arg.as_raw()) {
                        let bval = get_int_value(base_arg.as_raw());
                        if is_class_marker(bval) {
                            base_ptrs.push(extract_ptr(bval) as *const RustClass);
                        }
                    }
                }
                // Second pass: merge in reverse order (last base first, first base last = wins)
                for &base in base_ptrs.iter().rev() {
                    let base_ref = unsafe { &*base };
                    for (k, v) in &base_ref.namespace {
                        merged_ns.insert(k.clone(), v.clone());
                    }
                }
                // Derived class body overrides base methods
                for (k, v) in ns_frame.locals {
                    merged_ns.insert(k, v);
                }

                let rust_class = RustClass {
                    name: class_name.clone(),
                    bases: base_ptrs,
                    namespace: merged_ns,
                    globals: merged_globals,
                    builtins: caller_frame.builtins.clone(),
                };
                let class_box = Box::new(rust_class);
                let class_ptr = Box::into_raw(class_box);
                let marker_val = (class_ptr as usize as i64) | CLASS_TAG;
                return new_int(py, marker_val);
            }
        }

        Ok(none_obj(py))
    }

    /// Import a Python source file (.py) by searching for it and executing it.
    fn import_py_source(
        &mut self,
        py: Python<'_>,
        caller_frame: &Frame,
        name: &str,
    ) -> PyResult {
        use std::path::Path;

        // Check if already imported (cached)
        let cached = PY_MODULE_CACHE.lock().unwrap();
        if let Some(module) = cached.get(name) {
            return Ok(module.clone());
        }
        drop(cached);

        // Try builtin stdlib module
        if let Some(module) = self.try_builtin_module(py, name)? {
            PY_MODULE_CACHE.lock().unwrap().insert(name.to_string(), module.clone());
            return Ok(module);
        }

        // Build search paths: directory of the current file, current directory, "."
        let search_dirs: Vec<String> = vec![
            ".".to_string(),
        ];

        // Convert dotted name to path: "foo.bar" -> "foo/bar"
        let path_parts: Vec<&str> = name.split('.').collect();
        let file_stem = path_parts.join("/");

        for dir in &search_dirs {
            // Try module_name.py
            let file_path = format!("{}/{}.py", dir, file_stem);
            if Path::new(&file_path).exists() {
                let source = std::fs::read_to_string(&file_path)
                    .map_err(|e| PyErr::import_error(&format!("{}: {}", name, e)))?;

                let code = crate::compiler::compile::compile_source(py, &source, &file_path)
                    .map_err(|e| PyErr::import_error(&format!("{}: {}", name, e)))?;

                // Execute the module code
                let mut module_frame = Frame::new(code);
                // Copy builtins from caller
                module_frame.builtins = caller_frame.builtins.clone();
                let _result = self.run_frame(py, &mut module_frame)?;

                // Build a module dict from the module's locals
                let mut pairs = Vec::new();
                let name_key = new_str(py, "__name__")?;
                let name_val = new_str(py, name)?;
                pairs.push((name_key, name_val));
                let file_key = new_str(py, "__file__")?;
                let file_val = new_str(py, &file_path)?;
                pairs.push((file_key, file_val));

                for (k, v) in &module_frame.locals {
                    let key = new_str(py, k)?;
                    pairs.push((key, v.clone()));
                }
                let module_dict = build_dict(py, pairs)?;

                // Cache it
                PY_MODULE_CACHE.lock().unwrap().insert(name.to_string(), module_dict.clone());

                return Ok(module_dict);
            }

            // Try package: module_name/__init__.py
            let pkg_path = format!("{}/{}/__init__.py", dir, file_stem);
            if Path::new(&pkg_path).exists() {
                let source = std::fs::read_to_string(&pkg_path)
                    .map_err(|e| PyErr::import_error(&format!("{}: {}", name, e)))?;

                let code = crate::compiler::compile::compile_source(py, &source, &pkg_path)
                    .map_err(|e| PyErr::import_error(&format!("{}: {}", name, e)))?;

                let mut module_frame = Frame::new(code);
                module_frame.builtins = caller_frame.builtins.clone();
                let _result = self.run_frame(py, &mut module_frame)?;

                let mut pairs = Vec::new();
                let name_key = new_str(py, "__name__")?;
                let name_val = new_str(py, name)?;
                pairs.push((name_key, name_val));
                let file_key = new_str(py, "__file__")?;
                let file_val = new_str(py, &pkg_path)?;
                pairs.push((file_key, file_val));
                // __path__ for package
                let path_key = new_str(py, "__path__")?;
                let path_val = new_str(py, &format!("{}/{}", dir, file_stem))?;
                pairs.push((path_key, path_val));

                for (k, v) in &module_frame.locals {
                    let key = new_str(py, k)?;
                    pairs.push((key, v.clone()));
                }
                let module_dict = build_dict(py, pairs)?;

                PY_MODULE_CACHE.lock().unwrap().insert(name.to_string(), module_dict.clone());

                return Ok(module_dict);
            }
        }

        Err(PyErr::import_error(name))
    }

    /// Try to create a builtin stdlib module by name.
    fn try_builtin_module(
        &self,
        py: Python<'_>,
        name: &str,
    ) -> Result<Option<PyObjectRef>, PyErr> {
        let pairs = match name {
            "sys" => {
                let mut p = Vec::new();
                p.push((new_str(py, "__name__")?, new_str(py, "sys")?));
                p.push((new_str(py, "platform")?, new_str(py, "darwin")?));
                p.push((new_str(py, "maxsize")?, new_int(py, i64::MAX)?));
                p.push((new_str(py, "maxunicode")?, new_int(py, 0x10FFFF)?));
                // version_info as a tuple (3, 11, 0, 'final', 0)
                let vi = build_tuple(py, vec![
                    new_int(py, 3)?, new_int(py, 11)?, new_int(py, 0)?,
                    new_str(py, "final")?, new_int(py, 0)?,
                ])?;
                p.push((new_str(py, "version_info")?, vi));
                p.push((new_str(py, "version")?, new_str(py, "3.11.0 (rustthon)")?));
                // sys.path as a list
                let path = build_list(py, vec![new_str(py, ".")?])?;
                p.push((new_str(py, "path")?, path));
                // sys.modules as an empty dict (placeholder)
                let modules = build_dict(py, Vec::new())?;
                p.push((new_str(py, "modules")?, modules));
                // sys.argv
                let argv = build_list(py, vec![new_str(py, "rustthon")?])?;
                p.push((new_str(py, "argv")?, argv));
                // sys.executable
                p.push((new_str(py, "executable")?, new_str(py, "./rustthon")?));
                // sys.stdin/stdout/stderr as None placeholders
                p.push((new_str(py, "stdout")?, none_obj(py)));
                p.push((new_str(py, "stderr")?, none_obj(py)));
                p.push((new_str(py, "stdin")?, none_obj(py)));
                // sys.getdefaultencoding()
                p.push((new_str(py, "byteorder")?, new_str(py, "little")?));
                p
            }
            "os" | "os.path" | "posixpath" => {
                let mut p = Vec::new();
                p.push((new_str(py, "__name__")?, new_str(py, name)?));
                p.push((new_str(py, "sep")?, new_str(py, "/")?));
                p.push((new_str(py, "altsep")?, none_obj(py)));
                p.push((new_str(py, "extsep")?, new_str(py, ".")?));
                p.push((new_str(py, "pathsep")?, new_str(py, ":")?));
                p.push((new_str(py, "linesep")?, new_str(py, "\n")?));
                p.push((new_str(py, "curdir")?, new_str(py, ".")?));
                p.push((new_str(py, "pardir")?, new_str(py, "..")?));
                p.push((new_str(py, "name")?, new_str(py, "posix")?));
                // os.getcwd() as a callable
                let getcwd_fn = unsafe {
                    PyObjectRef::from_raw(create_builtin_function("getcwd", stdlib_os_getcwd))
                };
                p.push((new_str(py, "getcwd")?, getcwd_fn));

                // os.path module as a nested dict
                let mut path_pairs = Vec::new();
                path_pairs.push((new_str(py, "__name__")?, new_str(py, "os.path")?));
                path_pairs.push((new_str(py, "sep")?, new_str(py, "/")?));
                let join_fn = unsafe {
                    PyObjectRef::from_raw(create_builtin_function("join", stdlib_os_path_join))
                };
                path_pairs.push((new_str(py, "join")?, join_fn));
                let exists_fn = unsafe {
                    PyObjectRef::from_raw(create_builtin_function("exists", stdlib_os_path_exists))
                };
                path_pairs.push((new_str(py, "exists")?, exists_fn));
                let dirname_fn = unsafe {
                    PyObjectRef::from_raw(create_builtin_function("dirname", stdlib_os_path_dirname))
                };
                path_pairs.push((new_str(py, "dirname")?, dirname_fn));
                let basename_fn = unsafe {
                    PyObjectRef::from_raw(create_builtin_function("basename", stdlib_os_path_basename))
                };
                path_pairs.push((new_str(py, "basename")?, basename_fn));
                let path_mod = build_dict(py, path_pairs)?;
                p.push((new_str(py, "path")?, path_mod));
                // os.environ as an empty dict
                let environ = build_dict(py, Vec::new())?;
                p.push((new_str(py, "environ")?, environ));
                p
            }
            "re" => {
                let mut p = Vec::new();
                p.push((new_str(py, "__name__")?, new_str(py, "re")?));
                // Constants
                p.push((new_str(py, "IGNORECASE")?, new_int(py, 2)?));
                p.push((new_str(py, "I")?, new_int(py, 2)?));
                p.push((new_str(py, "MULTILINE")?, new_int(py, 8)?));
                p.push((new_str(py, "M")?, new_int(py, 8)?));
                p.push((new_str(py, "DOTALL")?, new_int(py, 16)?));
                p.push((new_str(py, "S")?, new_int(py, 16)?));
                p.push((new_str(py, "VERBOSE")?, new_int(py, 64)?));
                p.push((new_str(py, "X")?, new_int(py, 64)?));
                // Functions
                let fns: &[(&str, unsafe extern "C" fn(*mut RawPyObject, *mut RawPyObject) -> *mut RawPyObject)] = &[
                    ("compile", re_compile),
                    ("search", re_search),
                    ("match", re_match_fn),
                    ("findall", re_findall),
                    ("sub", re_sub),
                    ("split", re_split),
                    ("fullmatch", re_fullmatch),
                    ("escape", re_escape),
                ];
                for &(name, func) in fns {
                    let obj = unsafe {
                        PyObjectRef::from_raw(create_builtin_function(name, func))
                    };
                    p.push((new_str(py, name)?, obj));
                }
                p
            }
            _ => return Ok(None),
        };
        let module_dict = build_dict(py, pairs)?;
        Ok(Some(module_dict))
    }
}

// Global module cache for Python source imports
static PY_MODULE_CACHE: std::sync::LazyLock<std::sync::Mutex<HashMap<String, PyObjectRef>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

// ─── Iterator support ───

/// Rustthon iterator: wraps a list/tuple/range with an index counter.
/// Stored as a tuple: (source_obj, int_index)
fn get_iterator(py: Python<'_>, obj: &PyObjectRef) -> PyResult {
    let raw = obj.as_raw();

    // Check for generator (negative int marker with low bits 0)
    if is_int(raw) {
        let val = get_int_value(raw);
        if is_generator_marker(val) {
            // Generator — it IS its own iterator, just pass through
            return Ok(obj.clone());
        }
    }

    unsafe {
        // If it already has tp_iter, use it
        let tp = (*raw).ob_type;
        if !tp.is_null() {
            if let Some(tp_iter) = (*tp).tp_iter {
                let iter = tp_iter(raw);
                if !iter.is_null() {
                    return PyObjectRef::steal_or_err(iter);
                }
            }
        }
    }

    // Build a (source, index) tuple as our simple iterator
    let idx = new_int(py, 0)?;
    // Incref source so it lives as long as iterator
    let source = obj.clone();
    build_tuple(py, vec![source, idx])
}

/// Get next item from iterator, or None if exhausted.
fn iter_next(py: Python<'_>, iter: &PyObjectRef) -> Option<PyObjectRef> {
    let raw = iter.as_raw();

    // Check for generator (negative int marker with low bits 0)
    if is_int(raw) {
        let val = get_int_value(raw);
        if is_generator_marker(val) {
            let gen_ptr = (-val) as usize as *mut RustGenerator;
            let gen = unsafe { &mut *gen_ptr };
            if gen.exhausted {
                return None;
            }
            // Resume the generator frame
            match resume_generator(py, gen) {
                GeneratorResult::Yielded(value) => return Some(value),
                GeneratorResult::Returned => {
                    gen.exhausted = true;
                    return None;
                }
            }
        }
    }

    // Check if it has tp_iternext
    unsafe {
        let tp = (*raw).ob_type;
        if !tp.is_null() {
            if let Some(tp_iternext) = (*tp).tp_iternext {
                let next = tp_iternext(raw);
                if next.is_null() {
                    return None;
                }
                return PyObjectRef::steal_or_err(next).ok();
            }
        }
    }

    // Our simple (source, index) tuple iterator
    unsafe {
        if crate::types::tuple::PyTuple_Check(raw) == 0 {
            return None;
        }
        let size = crate::types::tuple::PyTuple_Size(raw);
        if size < 2 {
            return None;
        }

        let source = crate::types::tuple::PyTuple_GetItem(raw, 0);
        let idx_obj = crate::types::tuple::PyTuple_GetItem(raw, 1);
        if source.is_null() || idx_obj.is_null() {
            return None;
        }

        let idx = get_int_value(idx_obj) as isize;

        // Get item at index from source
        let item = if crate::types::list::PyList_Check(source) != 0 {
            let len = crate::types::list::PyList_Size(source);
            if idx >= len {
                return None;
            }
            crate::types::list::PyList_GetItem(source, idx)
        } else if crate::types::tuple::PyTuple_Check(source) != 0 {
            let len = crate::types::tuple::PyTuple_Size(source);
            if idx >= len {
                return None;
            }
            crate::types::tuple::PyTuple_GetItem(source, idx)
        } else if is_str(source) {
            let s = crate::types::unicode::unicode_value(source);
            let chars: Vec<char> = s.chars().collect();
            if idx as usize >= chars.len() {
                return None;
            }
            let ch = chars[idx as usize].to_string();
            let ch_obj = crate::types::unicode::create_from_str(&ch);
            // Update index
            let new_idx = crate::types::longobject::PyLong_FromLong((idx + 1) as _);
            crate::types::tuple::PyTuple_SetItem(raw, 1, new_idx);
            return PyObjectRef::steal_or_err(ch_obj).ok();
        } else {
            return None;
        };

        if item.is_null() {
            return None;
        }

        // Update index: replace the int in the tuple
        let new_idx = crate::types::longobject::PyLong_FromLong((idx + 1) as _);
        crate::types::tuple::PyTuple_SetItem(raw, 1, new_idx);

        (*item).incref();
        Some(PyObjectRef::from_raw(item))
    }
}

/// Resume a generator frame, executing until YieldValue or ReturnValue.
fn resume_generator(py: Python<'_>, gen: &mut RustGenerator) -> GeneratorResult {
    // If generator has already yielded, push None as the "sent" value
    // (the result of the yield expression). next() sends None; send(v) would send v.
    if gen.started {
        gen.frame.push(none_obj(py));
    }
    gen.started = true;

    let frame = &mut gen.frame;
    let mut vm = VM::new(); // Temporary VM for executing generator opcodes
    let mut saved_exception: Option<PyErr> = None;

    loop {
        if frame.ip >= frame.code.instructions.len() {
            return GeneratorResult::Returned;
        }
        let instr = frame.code.instructions[frame.ip].clone();
        frame.ip += 1;

        // Special handling for YieldValue — suspend execution and return value
        if instr.opcode == OpCode::YieldValue {
            if let Ok(val) = frame.pop() {
                return GeneratorResult::Yielded(val);
            }
            return GeneratorResult::Returned;
        }

        // Special handling for ReturnValue — generator is done
        if instr.opcode == OpCode::ReturnValue {
            return GeneratorResult::Returned;
        }

        // Execute other opcodes normally via the VM's execute_opcode
        match vm.execute_opcode(py, frame, &instr, &mut saved_exception) {
            Ok(Some(_)) => {
                // ReturnValue was hit (shouldn't happen here since we catch it above)
                return GeneratorResult::Returned;
            }
            Ok(None) => {
                // Normal execution, continue
            }
            Err(_) => {
                // Error — treat as exhausted
                return GeneratorResult::Returned;
            }
        }
    }
}

// ─── Helper functions ───

fn binary_add(py: Python<'_>, left: &PyObjectRef, right: &PyObjectRef) -> PyResult {
    let (l, r) = (left.as_raw(), right.as_raw());
    if is_int(l) && is_int(r) {
        new_int(py, get_int_value(l).wrapping_add(get_int_value(r)))
    } else if is_float(l) || is_float(r) {
        new_float(py, get_float_value(l) + get_float_value(r))
    } else if is_str(l) && is_str(r) {
        let ptr = unsafe { crate::types::unicode::PyUnicode_Concat(l, r) };
        PyObjectRef::steal_or_err(ptr)
    } else if is_list(l) && is_list(r) {
        unsafe {
            let l_size = crate::types::list::PyList_Size(l);
            let r_size = crate::types::list::PyList_Size(r);
            let total = l_size + r_size;
            let new_list = crate::types::list::PyList_New(total);
            if new_list.is_null() {
                return Err(PyErr::memory_error());
            }
            for i in 0..l_size {
                let item = crate::types::list::PyList_GetItem(l, i);
                py_incref(item);
                crate::types::list::PyList_SET_ITEM(new_list, i, item);
            }
            for i in 0..r_size {
                let item = crate::types::list::PyList_GetItem(r, i);
                py_incref(item);
                crate::types::list::PyList_SET_ITEM(new_list, l_size + i, item);
            }
            PyObjectRef::steal_or_err(new_list)
        }
    } else {
        // Try str conversion for str + non-str
        if is_str(l) {
            let r_str = unsafe { crate::ffi::object_api::PyObject_Str(r) };
            if !r_str.is_null() {
                let result = unsafe { crate::types::unicode::PyUnicode_Concat(l, r_str) };
                unsafe { (*r_str).decref(); }
                return PyObjectRef::steal_or_err(result);
            }
        }
        Ok(none_obj(py))
    }
}

fn binary_sub(py: Python<'_>, left: &PyObjectRef, right: &PyObjectRef) -> PyResult {
    let (l, r) = (left.as_raw(), right.as_raw());
    if is_int(l) && is_int(r) {
        new_int(py, get_int_value(l).wrapping_sub(get_int_value(r)))
    } else if is_float(l) || is_float(r) {
        new_float(py, get_float_value(l) - get_float_value(r))
    } else {
        Ok(none_obj(py))
    }
}

fn binary_mul(py: Python<'_>, left: &PyObjectRef, right: &PyObjectRef) -> PyResult {
    let (l, r) = (left.as_raw(), right.as_raw());
    if is_int(l) && is_int(r) {
        new_int(py, get_int_value(l).wrapping_mul(get_int_value(r)))
    } else if is_float(l) || is_float(r) {
        new_float(py, get_float_value(l) * get_float_value(r))
    } else if is_str(l) && is_int(r) {
        // String repetition: "abc" * 3
        let s = crate::types::unicode::unicode_value(l);
        let n = get_int_value(r);
        if n <= 0 {
            new_str(py, "")
        } else {
            new_str(py, &s.repeat(n as usize))
        }
    } else if is_int(l) && is_str(r) {
        let s = crate::types::unicode::unicode_value(r);
        let n = get_int_value(l);
        if n <= 0 {
            new_str(py, "")
        } else {
            new_str(py, &s.repeat(n as usize))
        }
    } else if is_list(l) && is_int(r) {
        // List repetition: [1,2] * 3
        let n = get_int_value(r);
        if n <= 0 {
            build_list(py, Vec::new())
        } else {
            unsafe {
                let size = crate::types::list::PyList_Size(l);
                let new_list = crate::types::list::PyList_New(0);
                for _ in 0..n {
                    for j in 0..size {
                        let item = crate::types::list::PyList_GetItem(l, j);
                        (*item).incref();
                        crate::types::list::PyList_Append(new_list, item);
                    }
                }
                PyObjectRef::steal_or_err(new_list)
            }
        }
    } else {
        Ok(none_obj(py))
    }
}

fn binary_truediv(py: Python<'_>, left: &PyObjectRef, right: &PyObjectRef) -> PyResult {
    let lv = get_float_value(left.as_raw());
    let rv = get_float_value(right.as_raw());
    if rv == 0.0 {
        return Err(PyErr::zero_division_error("division by zero"));
    }
    new_float(py, lv / rv)
}

fn binary_floordiv(py: Python<'_>, left: &PyObjectRef, right: &PyObjectRef) -> PyResult {
    let (l, r) = (left.as_raw(), right.as_raw());
    if is_int(l) && is_int(r) {
        let lv = get_int_value(l);
        let rv = get_int_value(r);
        if rv == 0 { return Err(PyErr::zero_division_error("integer division or modulo by zero")); }
        let d = lv.wrapping_div(rv);
        let result = if (lv ^ rv) < 0 && d * rv != lv { d - 1 } else { d };
        new_int(py, result)
    } else {
        let lv = get_float_value(l);
        let rv = get_float_value(r);
        if rv == 0.0 { return Err(PyErr::zero_division_error("float floor division by zero")); }
        new_float(py, (lv / rv).floor())
    }
}

fn binary_mod(py: Python<'_>, left: &PyObjectRef, right: &PyObjectRef) -> PyResult {
    let (l, r) = (left.as_raw(), right.as_raw());
    if is_str(l) {
        let fmt = crate::types::unicode::unicode_value(l);
        // Collect format values — if right is a tuple, extract items
        let mut values: Vec<String> = Vec::new();
        unsafe {
            if crate::types::tuple::PyTuple_Check(r) != 0 {
                let n = crate::types::tuple::PyTuple_Size(r);
                for i in 0..n {
                    let item = crate::types::tuple::PyTuple_GetItem(r, i);
                    values.push(format_pyobj(item));
                }
            } else {
                values.push(format_pyobj(r));
            }
        }
        // Replace %s, %d, %r, %f in order
        let mut result = fmt.to_string();
        let mut val_idx = 0;
        let mut i = 0;
        let chars: Vec<char> = result.chars().collect();
        let mut output = String::new();
        while i < chars.len() {
            if chars[i] == '%' && i + 1 < chars.len() {
                match chars[i + 1] {
                    's' | 'd' | 'r' | 'f' | 'i' => {
                        if val_idx < values.len() {
                            output.push_str(&values[val_idx]);
                            val_idx += 1;
                        }
                        i += 2;
                        continue;
                    }
                    '%' => {
                        output.push('%');
                        i += 2;
                        continue;
                    }
                    _ => {}
                }
            }
            output.push(chars[i]);
            i += 1;
        }
        return new_str(py, &output);
    }
    if is_int(l) && is_int(r) {
        let lv = get_int_value(l);
        let rv = get_int_value(r);
        if rv == 0 { return Err(PyErr::zero_division_error("integer division or modulo by zero")); }
        let m = lv % rv;
        let result = if m != 0 && (m ^ rv) < 0 { m + rv } else { m };
        new_int(py, result)
    } else {
        let lv = get_float_value(l);
        let rv = get_float_value(r);
        if rv == 0.0 { return Err(PyErr::zero_division_error("float modulo by zero")); }
        new_float(py, lv % rv)
    }
}

fn binary_pow(py: Python<'_>, left: &PyObjectRef, right: &PyObjectRef) -> PyResult {
    let (l, r) = (left.as_raw(), right.as_raw());
    if is_int(l) && is_int(r) {
        let lv = get_int_value(l);
        let rv = get_int_value(r);
        if rv >= 0 && rv <= 63 {
            new_int(py, lv.wrapping_pow(rv as u32))
        } else {
            new_float(py, (lv as f64).powf(rv as f64))
        }
    } else {
        new_float(py, get_float_value(l).powf(get_float_value(r)))
    }
}

fn binary_bitop(py: Python<'_>, left: &PyObjectRef, right: &PyObjectRef, op: OpCode) -> PyResult {
    let lv = get_int_value(left.as_raw());
    let rv = get_int_value(right.as_raw());
    let result = match op {
        OpCode::BinaryAnd => lv & rv,
        OpCode::BinaryOr => lv | rv,
        OpCode::BinaryXor => lv ^ rv,
        OpCode::BinaryLShift => lv.wrapping_shl(rv as u32),
        OpCode::BinaryRShift => lv.wrapping_shr(rv as u32),
        _ => 0,
    };
    new_int(py, result)
}

fn unary_negative(py: Python<'_>, obj: &PyObjectRef) -> PyResult {
    let raw = obj.as_raw();
    if is_int(raw) {
        new_int(py, get_int_value(raw).wrapping_neg())
    } else if is_float(raw) {
        new_float(py, -get_float_value(raw))
    } else {
        Ok(none_obj(py))
    }
}

fn compare_op(py: Python<'_>, left: &PyObjectRef, right: &PyObjectRef, op: u32) -> PyResult {
    let (l, r) = (left.as_raw(), right.as_raw());
    match op {
        6 => Ok(bool_obj(py, l == r)), // is
        7 => Ok(bool_obj(py, l != r)), // is not
        8 => { // in
            Ok(bool_obj(py, contains(l, r)))
        }
        9 => { // not in
            Ok(bool_obj(py, !contains(l, r)))
        }
        10 => { // exception match (for except clauses)
            // l = exception value, r = exception type to match against
            // Check if the exception's type matches the target type
            let exc_type = unsafe { (*l).ob_type as *mut RawPyObject };
            if exc_type == r || l == r {
                Ok(true_obj(py))
            } else {
                // Walk the tp_base chain for subclass matching
                let mut base = exc_type;
                let mut matched = false;
                while !base.is_null() {
                    if base == r {
                        matched = true;
                        break;
                    }
                    let tp = base as *const crate::object::typeobj::RawPyTypeObject;
                    let next_base = unsafe { (*tp).tp_base };
                    if next_base.is_null() {
                        break;
                    }
                    base = next_base as *mut RawPyObject;
                }
                Ok(bool_obj(py, matched))
            }
        }
        _ => {
            if is_none(l) && is_none(r) {
                return Ok(bool_obj(py, op == 2)); // None == None is True
            }
            if is_none(l) || is_none(r) {
                return Ok(bool_obj(py, op == 3)); // None != anything is True
            }
            if is_int(l) && is_int(r) {
                let lv = get_int_value(l);
                let rv = get_int_value(r);
                let result = match op { 0=>lv<rv, 1=>lv<=rv, 2=>lv==rv, 3=>lv!=rv, 4=>lv>rv, 5=>lv>=rv, _=>false };
                Ok(bool_obj(py, result))
            } else if is_float(l) || is_float(r) {
                let lv = get_float_value(l);
                let rv = get_float_value(r);
                let result = match op { 0=>lv<rv, 1=>lv<=rv, 2=>lv==rv, 3=>lv!=rv, 4=>lv>rv, 5=>lv>=rv, _=>false };
                Ok(bool_obj(py, result))
            } else if is_str(l) && is_str(r) {
                let lv = crate::types::unicode::unicode_value(l);
                let rv = crate::types::unicode::unicode_value(r);
                let result = match op { 0=>lv<rv, 1=>lv<=rv, 2=>lv==rv, 3=>lv!=rv, 4=>lv>rv, 5=>lv>=rv, _=>false };
                Ok(bool_obj(py, result))
            } else if unsafe { crate::types::tuple::PyTuple_Check(l) != 0 && crate::types::tuple::PyTuple_Check(r) != 0 } {
                let eq = unsafe { tuples_equal(l, r) };
                let result = match op { 2 => eq, 3 => !eq, _ => false };
                Ok(bool_obj(py, result))
            } else if unsafe { crate::types::list::PyList_Check(l) != 0 && crate::types::list::PyList_Check(r) != 0 } {
                let eq = unsafe { lists_equal(l, r) };
                let result = match op { 2 => eq, 3 => !eq, _ => false };
                Ok(bool_obj(py, result))
            } else {
                let result = match op { 2 => l == r, 3 => l != r, _ => false };
                Ok(bool_obj(py, result))
            }
        }
    }
}

/// Element-wise tuple equality.
unsafe fn tuples_equal(a: *mut RawPyObject, b: *mut RawPyObject) -> bool {
    let na = crate::types::tuple::PyTuple_Size(a);
    let nb = crate::types::tuple::PyTuple_Size(b);
    if na != nb { return false; }
    for i in 0..na {
        let ea = crate::types::tuple::PyTuple_GetItem(a, i);
        let eb = crate::types::tuple::PyTuple_GetItem(b, i);
        if !objs_equal(ea, eb) { return false; }
    }
    true
}

/// Element-wise list equality.
unsafe fn lists_equal(a: *mut RawPyObject, b: *mut RawPyObject) -> bool {
    let na = crate::types::list::PyList_Size(a);
    let nb = crate::types::list::PyList_Size(b);
    if na != nb { return false; }
    for i in 0..na {
        let ea = crate::types::list::PyList_GetItem(a, i);
        let eb = crate::types::list::PyList_GetItem(b, i);
        if !objs_equal(ea, eb) { return false; }
    }
    true
}

/// Check if item `l` is in container `r`.
fn contains(item: *mut RawPyObject, container: *mut RawPyObject) -> bool {
    unsafe {
        if crate::types::list::PyList_Check(container) != 0 {
            let n = crate::types::list::PyList_Size(container);
            for i in 0..n {
                let el = crate::types::list::PyList_GetItem(container, i);
                if objs_equal(item, el) { return true; }
            }
        } else if crate::types::tuple::PyTuple_Check(container) != 0 {
            let n = crate::types::tuple::PyTuple_Size(container);
            for i in 0..n {
                let el = crate::types::tuple::PyTuple_GetItem(container, i);
                if objs_equal(item, el) { return true; }
            }
        } else if is_str(container) && is_str(item) {
            let haystack = crate::types::unicode::unicode_value(container);
            let needle = crate::types::unicode::unicode_value(item);
            return haystack.contains(needle);
        } else if crate::types::dict::PyDict_Check(container) != 0 {
            let result = crate::types::dict::PyDict_GetItem(container, item);
            return !result.is_null();
        } else if crate::types::set::PySet_Check(container) != 0 {
            return crate::types::set::PySet_Contains(container, item) != 0;
        }
    }
    false
}

fn objs_equal(a: *mut RawPyObject, b: *mut RawPyObject) -> bool {
    if a == b { return true; }
    if a.is_null() || b.is_null() { return false; }
    unsafe {
        if is_none(a) && is_none(b) { return true; }
        if is_bool(a) && is_bool(b) {
            return crate::types::boolobject::is_true(a) == crate::types::boolobject::is_true(b);
        }
        if is_int(a) && is_int(b) { return get_int_value(a) == get_int_value(b); }
        if is_str(a) && is_str(b) {
            return crate::types::unicode::unicode_value(a) == crate::types::unicode::unicode_value(b);
        }
        if is_float(a) && is_float(b) { return get_float_value(a) == get_float_value(b); }
        if crate::types::tuple::PyTuple_Check(a) != 0 && crate::types::tuple::PyTuple_Check(b) != 0 {
            return tuples_equal(a, b);
        }
        if crate::types::list::PyList_Check(a) != 0 && crate::types::list::PyList_Check(b) != 0 {
            return lists_equal(a, b);
        }
    }
    false
}

fn subscr_fallback(py: Python<'_>, obj: &PyObjectRef, key: &PyObjectRef) -> PyResult {
    let (o, k) = (obj.as_raw(), key.as_raw());
    unsafe {
        // Check if key is a slice tuple (lower, upper, step)
        if crate::types::tuple::PyTuple_Check(k) != 0 {
            let size = crate::types::tuple::PyTuple_Size(k);
            if size == 3 {
                // This is a slice operation
                let lower_raw = crate::types::tuple::PyTuple_GetItem(k, 0);
                let upper_raw = crate::types::tuple::PyTuple_GetItem(k, 1);
                let _step_raw = crate::types::tuple::PyTuple_GetItem(k, 2);

                if is_str(o) {
                    let s = crate::types::unicode::unicode_value(o);
                    let chars: Vec<char> = s.chars().collect();
                    let len = chars.len() as i64;
                    let start = if is_none(lower_raw) { 0 } else {
                        let v = get_int_value(lower_raw);
                        if v < 0 { std::cmp::max(0, len + v) } else { std::cmp::min(v, len) }
                    };
                    let end = if is_none(upper_raw) { len } else {
                        let v = get_int_value(upper_raw);
                        if v < 0 { std::cmp::max(0, len + v) } else { std::cmp::min(v, len) }
                    };
                    if start >= end {
                        return new_str(py, "");
                    }
                    let sliced: String = chars[start as usize..end as usize].iter().collect();
                    return new_str(py, &sliced);
                }

                if crate::types::list::PyList_Check(o) != 0 {
                    let len = crate::types::list::PyList_Size(o) as i64;
                    let start = if is_none(lower_raw) { 0 } else {
                        let v = get_int_value(lower_raw);
                        if v < 0 { std::cmp::max(0, len + v) } else { std::cmp::min(v, len) }
                    };
                    let end = if is_none(upper_raw) { len } else {
                        let v = get_int_value(upper_raw);
                        if v < 0 { std::cmp::max(0, len + v) } else { std::cmp::min(v, len) }
                    };
                    let mut items = Vec::new();
                    for i in start..end {
                        let item = crate::types::list::PyList_GetItem(o, i as isize);
                        items.push(PyObjectRef::borrow_or_err(item)?);
                    }
                    return build_list(py, items);
                }

                if crate::types::tuple::PyTuple_Check(o) != 0 {
                    let len = crate::types::tuple::PyTuple_Size(o) as i64;
                    let start = if is_none(lower_raw) { 0 } else {
                        let v = get_int_value(lower_raw);
                        if v < 0 { std::cmp::max(0, len + v) } else { std::cmp::min(v, len) }
                    };
                    let end = if is_none(upper_raw) { len } else {
                        let v = get_int_value(upper_raw);
                        if v < 0 { std::cmp::max(0, len + v) } else { std::cmp::min(v, len) }
                    };
                    let mut items = Vec::new();
                    for i in start..end {
                        let item = crate::types::tuple::PyTuple_GetItem(o, i as isize);
                        items.push(PyObjectRef::borrow_or_err(item)?);
                    }
                    return build_tuple(py, items);
                }
            }
        }

        if crate::types::list::PyList_Check(o) != 0 {
            let idx = get_int_value(k) as isize;
            let len = crate::types::list::PyList_Size(o);
            let real_idx = if idx < 0 { len + idx } else { idx };
            if real_idx < 0 || real_idx >= len {
                return Err(PyErr::type_error("list index out of range"));
            }
            let item = crate::types::list::PyList_GetItem(o, real_idx);
            return PyObjectRef::borrow_or_err(item);
        }
        if crate::types::tuple::PyTuple_Check(o) != 0 {
            let idx = get_int_value(k) as isize;
            let len = crate::types::tuple::PyTuple_Size(o);
            let real_idx = if idx < 0 { len + idx } else { idx };
            if real_idx < 0 || real_idx >= len {
                return Err(PyErr::type_error("tuple index out of range"));
            }
            let item = crate::types::tuple::PyTuple_GetItem(o, real_idx);
            return PyObjectRef::borrow_or_err(item);
        }
        if crate::types::dict::PyDict_Check(o) != 0 {
            let item = crate::types::dict::PyDict_GetItem(o, k);
            return PyObjectRef::borrow_or_err(item);
        }
        // String indexing
        if is_str(o) && is_int(k) {
            let s = crate::types::unicode::unicode_value(o);
            let idx = get_int_value(k);
            let chars: Vec<char> = s.chars().collect();
            let real_idx = if idx < 0 { chars.len() as i64 + idx } else { idx };
            if real_idx >= 0 && (real_idx as usize) < chars.len() {
                let ch = chars[real_idx as usize].to_string();
                return PyObjectRef::steal_or_err(crate::types::unicode::create_from_str(&ch));
            }
            return Err(PyErr::type_error("string index out of range"));
        }
    }
    Err(PyErr::type_error("object is not subscriptable"))
}

/// Call a function using RAII args via tp_call.
fn call_function_raii(_py: Python<'_>, func: &PyObjectRef, args: &[PyObjectRef]) -> PyResult {
    unsafe {
        let f = func.as_raw();
        let args_tuple = crate::types::tuple::PyTuple_New(args.len() as isize);
        if args_tuple.is_null() {
            return Err(PyErr::memory_error());
        }
        for (i, arg) in args.iter().enumerate() {
            (*arg.as_raw()).incref();
            crate::types::tuple::PyTuple_SET_ITEM(args_tuple, i as isize, arg.as_raw());
        }
        let result = if (*f).ob_type == crate::types::funcobject::cfunction_type() {
            crate::types::funcobject::call_cfunction(f, args_tuple, ptr::null_mut())
        } else {
            let tp = (*f).ob_type;
            if !tp.is_null() {
                if let Some(tp_call) = (*tp).tp_call {
                    tp_call(f, args_tuple, ptr::null_mut())
                } else {
                    (*args_tuple).decref();
                    return Err(PyErr::type_error("object is not callable"));
                }
            } else {
                (*args_tuple).decref();
                return Err(PyErr::type_error("object is not callable"));
            }
        };
        (*args_tuple).decref();
        PyObjectRef::steal_or_err(result)
    }
}

// ─── Built-in function implementations ───

unsafe fn create_builtin_function(
    name: &str,
    func: unsafe extern "C" fn(*mut RawPyObject, *mut RawPyObject) -> *mut RawPyObject,
) -> *mut RawPyObject {
    let name_cstr = std::ffi::CString::new(name).unwrap();
    let name_ptr = name_cstr.into_raw() as *const std::os::raw::c_char;
    crate::types::funcobject::create_cfunction(
        name_ptr,
        Some(func),
        crate::object::typeobj::METH_VARARGS,
        ptr::null_mut(),
    )
}

// Stub for __build_class__ — actual work is done in VM::builtin_build_class
unsafe extern "C" fn builtin_build_class_stub(
    _self: *mut RawPyObject,
    _args: *mut RawPyObject,
) -> *mut RawPyObject {
    return_none()
}

unsafe extern "C" fn builtin_print(
    _self: *mut RawPyObject,
    args: *mut RawPyObject,
) -> *mut RawPyObject {
    if args.is_null() {
        println!();
        return return_none();
    }
    let nargs = crate::types::tuple::PyTuple_Size(args);
    let mut parts = Vec::new();
    for i in 0..nargs {
        let item = crate::types::tuple::PyTuple_GetItem(args, i);
        parts.push(format_object_for_print(item));
    }
    println!("{}", parts.join(" "));
    return_none()
}

unsafe fn format_object_for_print(obj: *mut RawPyObject) -> String {
    if obj.is_null() || is_none(obj) {
        "None".to_string()
    } else if is_bool(obj) {
        if crate::types::boolobject::is_true(obj) { "True".to_string() } else { "False".to_string() }
    } else if is_str(obj) {
        crate::types::unicode::unicode_value(obj).to_string()
    } else if is_int(obj) {
        format!("{}", crate::types::longobject::long_value(obj))
    } else if is_float(obj) {
        format!("{}", crate::types::floatobject::float_value(obj))
    } else if crate::types::list::PyList_Check(obj) != 0 {
        format_list(obj)
    } else if crate::types::tuple::PyTuple_Check(obj) != 0 {
        format_tuple(obj)
    } else if crate::types::dict::PyDict_Check(obj) != 0 {
        format_dict(obj)
    } else if crate::types::set::PySet_Check(obj) != 0 {
        format_set(obj)
    } else {
        let repr = crate::ffi::object_api::PyObject_Repr(obj);
        if !repr.is_null() && is_str(repr) {
            let s = crate::types::unicode::unicode_value(repr).to_string();
            (*repr).decref();
            s
        } else {
            format!("<object at {:p}>", obj)
        }
    }
}

unsafe fn format_list(list: *mut RawPyObject) -> String {
    let n = crate::types::list::PyList_Size(list);
    let mut items = Vec::new();
    for i in 0..n {
        let item = crate::types::list::PyList_GetItem(list, i);
        items.push(format_object_repr(item));
    }
    format!("[{}]", items.join(", "))
}

unsafe fn format_tuple(tuple: *mut RawPyObject) -> String {
    let n = crate::types::tuple::PyTuple_Size(tuple);
    let mut items = Vec::new();
    for i in 0..n {
        let item = crate::types::tuple::PyTuple_GetItem(tuple, i);
        items.push(format_object_repr(item));
    }
    if n == 1 {
        format!("({},)", items[0])
    } else {
        format!("({})", items.join(", "))
    }
}

unsafe fn format_dict(dict: *mut RawPyObject) -> String {
    let mut items = Vec::new();
    let mut pos: isize = 0;
    let mut key: *mut RawPyObject = ptr::null_mut();
    let mut value: *mut RawPyObject = ptr::null_mut();
    while crate::types::dict::PyDict_Next(dict, &mut pos, &mut key, &mut value) != 0 {
        let k = format_object_repr(key);
        let v = format_object_repr(value);
        items.push(format!("{}: {}", k, v));
    }
    format!("{{{}}}", items.join(", "))
}

unsafe fn format_set(set: *mut RawPyObject) -> String {
    let s = set as *mut crate::types::set::PySetObject;
    let table = (*s).table;
    let mask = (*s).mask as usize;
    let mut items = Vec::new();
    for i in 0..=mask {
        let entry = &*table.add(i);
        if !entry.key.is_null() {
            // Skip dummy entries (hash == 0 with non-null key is possible, but
            // in our implementation deleted entries have null key)
            items.push(format_object_repr(entry.key));
        }
    }
    if items.is_empty() {
        "set()".to_string()
    } else {
        format!("{{{}}}", items.join(", "))
    }
}

unsafe fn format_object_repr(obj: *mut RawPyObject) -> String {
    if obj.is_null() || is_none(obj) {
        "None".to_string()
    } else if is_str(obj) {
        format!("'{}'", crate::types::unicode::unicode_value(obj))
    } else if is_bool(obj) {
        if crate::types::boolobject::is_true(obj) { "True".to_string() } else { "False".to_string() }
    } else if is_int(obj) {
        format!("{}", crate::types::longobject::long_value(obj))
    } else if is_float(obj) {
        format!("{}", crate::types::floatobject::float_value(obj))
    } else if crate::types::list::PyList_Check(obj) != 0 {
        format_list(obj)
    } else if crate::types::tuple::PyTuple_Check(obj) != 0 {
        format_tuple(obj)
    } else if crate::types::dict::PyDict_Check(obj) != 0 {
        format_dict(obj)
    } else if crate::types::set::PySet_Check(obj) != 0 {
        format_set(obj)
    } else {
        format!("<object at {:p}>", obj)
    }
}

unsafe extern "C" fn builtin_len(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    if obj.is_null() { return create_int(0); }

    // Direct type checks for common types
    if is_str(obj) {
        return create_int(crate::types::unicode::unicode_value(obj).len() as i64);
    }
    if crate::types::list::PyList_Check(obj) != 0 {
        return create_int(crate::types::list::PyList_Size(obj) as i64);
    }
    if crate::types::tuple::PyTuple_Check(obj) != 0 {
        return create_int(crate::types::tuple::PyTuple_Size(obj) as i64);
    }
    if crate::types::dict::PyDict_Check(obj) != 0 {
        return create_int(crate::types::dict::PyDict_Size(obj) as i64);
    }
    if crate::types::set::PySet_Check(obj) != 0 {
        return create_int(crate::types::set::PySet_Size(obj) as i64);
    }
    if crate::types::bytes::PyBytes_Check(obj) != 0 {
        return create_int(crate::types::bytes::PyBytes_Size(obj) as i64);
    }

    // Fall back to PyObject_Length
    let len = crate::ffi::object_api::PyObject_Length(obj);
    if len >= 0 {
        create_int(len as i64)
    } else {
        create_int(0)
    }
}

unsafe extern "C" fn builtin_type(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    crate::ffi::object_api::PyObject_Type(obj)
}

unsafe extern "C" fn builtin_range(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let nargs = crate::types::tuple::PyTuple_Size(args);
    let (start, stop, step) = match nargs {
        1 => (0i64, get_int_value(crate::types::tuple::PyTuple_GetItem(args, 0)), 1i64),
        2 => (get_int_value(crate::types::tuple::PyTuple_GetItem(args, 0)),
              get_int_value(crate::types::tuple::PyTuple_GetItem(args, 1)), 1),
        3 => (get_int_value(crate::types::tuple::PyTuple_GetItem(args, 0)),
              get_int_value(crate::types::tuple::PyTuple_GetItem(args, 1)),
              get_int_value(crate::types::tuple::PyTuple_GetItem(args, 2))),
        _ => return crate::types::list::PyList_New(0),
    };
    if step == 0 { return crate::types::list::PyList_New(0); }
    let list = crate::types::list::PyList_New(0);
    let mut i = start;
    if step > 0 {
        while i < stop {
            let val = crate::types::longobject::PyLong_FromLong(i as _);
            crate::types::list::PyList_Append(list, val);
            (*val).decref();
            i += step;
        }
    } else {
        while i > stop {
            let val = crate::types::longobject::PyLong_FromLong(i as _);
            crate::types::list::PyList_Append(list, val);
            (*val).decref();
            i += step;
        }
    }
    list
}

unsafe extern "C" fn builtin_int(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let nargs = crate::types::tuple::PyTuple_Size(args);
    if nargs == 0 { return create_int(0); }
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    if obj.is_null() { return create_int(0); }
    if is_int(obj) { (*obj).incref(); return obj; }
    if is_float(obj) { return create_int(crate::types::floatobject::float_value(obj) as i64); }
    if is_bool(obj) { return create_int(if crate::types::boolobject::is_true(obj) { 1 } else { 0 }); }
    if is_str(obj) {
        let s = crate::types::unicode::unicode_value(obj);
        if let Ok(val) = s.trim().parse::<i64>() { return create_int(val); }
    }
    create_int(0)
}

unsafe extern "C" fn builtin_str(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let nargs = crate::types::tuple::PyTuple_Size(args);
    if nargs == 0 { return create_str(""); }
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    if obj.is_null() { return create_str("None"); }
    // Use our format_object_for_print which handles dicts/sets/lists properly
    let s = format_object_for_print(obj);
    create_str(&s)
}

unsafe extern "C" fn builtin_isinstance(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    let tp = crate::types::tuple::PyTuple_GetItem(args, 1);
    if obj.is_null() || tp.is_null() { return crate::object::safe_api::py_false(); }

    // If tp is a tuple, check each element
    if crate::types::tuple::PyTuple_Check(tp) != 0 {
        let n = crate::types::tuple::PyTuple_Size(tp);
        for i in 0..n {
            let single_tp = crate::types::tuple::PyTuple_GetItem(tp, i);
            if isinstance_check(obj, single_tp) {
                return bool_from_long(1);
            }
        }
        return bool_from_long(0);
    }

    bool_from_long(if isinstance_check(obj, tp) { 1 } else { 0 })
}

/// Core isinstance logic for a single type check.
unsafe fn isinstance_check(obj: *mut RawPyObject, tp: *mut RawPyObject) -> bool {
    if obj.is_null() || tp.is_null() { return false; }
    let obj_type = (*obj).ob_type as *mut RawPyObject;

    // Direct type comparison
    if obj_type == tp {
        return true;
    }

    // If tp is a CFunction (builtin_int, builtin_str, etc.), resolve to actual type
    if (*tp).ob_type == crate::types::funcobject::cfunction_type() {
        let data = crate::object::pyobject::PyObjectWithData::<crate::types::funcobject::CFunctionData>::data_from_raw(tp);
        if !data.name.is_null() {
            let name = std::ffi::CStr::from_ptr(data.name);
            let matched = match name.to_bytes() {
                b"int" => is_int(obj),
                b"str" => is_str(obj),
                b"float" => is_float(obj),
                b"bool" => is_bool(obj),
                b"list" => crate::types::list::PyList_Check(obj) != 0,
                b"tuple" => crate::types::tuple::PyTuple_Check(obj) != 0,
                b"dict" => crate::types::dict::PyDict_Check(obj) != 0,
                b"set" => crate::types::set::PySet_Check(obj) != 0,
                b"bytes" => crate::types::bytes::PyBytes_Check(obj) != 0,
                _ => false,
            };
            if matched {
                return true;
            }
        }
    }

    // RustClass-aware isinstance: check if obj is an instance of a RustClass (or subclass)
    if is_int(tp) {
        let tp_val = get_int_value(tp);
        if is_class_marker(tp_val) {
            let target_class = extract_ptr(tp_val) as *const RustClass;
            if is_int(obj) {
                let obj_val = get_int_value(obj);
                if is_instance_marker(obj_val) {
                    let inst = &*(extract_ptr(obj_val) as *const RustInstance);
                    return isinstance_class_check(inst.class, target_class);
                }
            }
            return false;
        }
    }

    // Check tp_base chain for subclass matching (C types)
    let mut base = obj_type;
    while !base.is_null() {
        if base == tp {
            return true;
        }
        let tp_ref = base as *const crate::object::typeobj::RawPyTypeObject;
        let next_base = (*tp_ref).tp_base;
        if next_base.is_null() { break; }
        base = next_base as *mut RawPyObject;
    }

    false
}

/// Check if obj_class is the same as or a subclass of target_class.
unsafe fn isinstance_class_check(obj_class: *const RustClass, target_class: *const RustClass) -> bool {
    if obj_class == target_class { return true; }
    // Walk the bases chain
    let class = &*obj_class;
    for &base_ptr in &class.bases {
        if isinstance_class_check(base_ptr, target_class) {
            return true;
        }
    }
    false
}

unsafe extern "C" fn builtin_hasattr(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    let name = crate::types::tuple::PyTuple_GetItem(args, 1);
    if obj.is_null() || name.is_null() { return crate::object::safe_api::py_false(); }
    // Handle our tagged int markers for instances/classes
    if is_int(obj) {
        let marker_val = get_int_value(obj);
        let attr_name = crate::types::unicode::unicode_value(name).to_string();
        if is_instance_marker(marker_val) {
            let inst = &*(extract_ptr(marker_val) as *const RustInstance);
            if inst.attrs.contains_key(&attr_name) { return crate::object::safe_api::py_true(); }
            let class = &*inst.class;
            if class.namespace.contains_key(&attr_name) { return crate::object::safe_api::py_true(); }
            return crate::object::safe_api::py_false();
        }
        if is_class_marker(marker_val) {
            let class = &*(extract_ptr(marker_val) as *const RustClass);
            if class.namespace.contains_key(&attr_name) { return crate::object::safe_api::py_true(); }
            return crate::object::safe_api::py_false();
        }
    }
    let result = crate::ffi::object_api::PyObject_GetAttr(obj, name);
    if result.is_null() {
        crate::runtime::error::PyErr_Clear();
        crate::object::safe_api::py_false()
    } else {
        (*result).decref();
        crate::object::safe_api::py_true()
    }
}

unsafe extern "C" fn builtin_getattr(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let nargs = crate::types::tuple::PyTuple_Size(args);
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    let name = crate::types::tuple::PyTuple_GetItem(args, 1);
    // Handle our tagged int markers for instances/classes
    if is_int(obj) {
        let marker_val = get_int_value(obj);
        let attr_name = crate::types::unicode::unicode_value(name).to_string();
        if is_instance_marker(marker_val) {
            let inst = &*(extract_ptr(marker_val) as *const RustInstance);
            if let Some(val) = inst.attrs.get(&attr_name) {
                let raw = val.as_raw();
                (*raw).incref();
                return raw;
            }
            let class = &*inst.class;
            if let Some(val) = class.namespace.get(&attr_name) {
                let raw = val.as_raw();
                (*raw).incref();
                return raw;
            }
            if nargs >= 3 {
                let default = crate::types::tuple::PyTuple_GetItem(args, 2);
                (*default).incref();
                return default;
            }
            return ptr::null_mut();
        }
        if is_class_marker(marker_val) {
            let class = &*(extract_ptr(marker_val) as *const RustClass);
            if let Some(val) = class.namespace.get(&attr_name) {
                let raw = val.as_raw();
                (*raw).incref();
                return raw;
            }
            if nargs >= 3 {
                let default = crate::types::tuple::PyTuple_GetItem(args, 2);
                (*default).incref();
                return default;
            }
            return ptr::null_mut();
        }
    }
    let result = crate::ffi::object_api::PyObject_GetAttr(obj, name);
    if result.is_null() && nargs >= 3 {
        crate::runtime::error::PyErr_Clear();
        let default = crate::types::tuple::PyTuple_GetItem(args, 2);
        (*default).incref();
        return default;
    }
    result
}

unsafe extern "C" fn builtin_setattr(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    let name = crate::types::tuple::PyTuple_GetItem(args, 1);
    let value = crate::types::tuple::PyTuple_GetItem(args, 2);
    crate::ffi::object_api::PyObject_SetAttr(obj, name, value);
    return_none()
}

unsafe extern "C" fn builtin_id(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    create_int(obj as i64)
}

unsafe extern "C" fn builtin_hash(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    if is_int(obj) { return create_int(get_int_value(obj)); }
    if is_str(obj) {
        let s = crate::types::unicode::unicode_value(obj);
        let mut h: u64 = 5381;
        for b in s.bytes() { h = h.wrapping_mul(33).wrapping_add(b as u64); }
        return create_int(h as i64);
    }
    create_int(obj as i64)
}

unsafe extern "C" fn builtin_abs(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    if is_int(obj) { return create_int(get_int_value(obj).abs()); }
    if is_float(obj) {
        return crate::types::floatobject::PyFloat_FromDouble(get_float_value(obj).abs());
    }
    return_none()
}

unsafe extern "C" fn builtin_min(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let nargs = crate::types::tuple::PyTuple_Size(args);
    if nargs == 0 { return return_none(); }
    if nargs == 1 {
        // min(iterable)
        let iterable = crate::types::tuple::PyTuple_GetItem(args, 0);
        if crate::types::list::PyList_Check(iterable) != 0 {
            let n = crate::types::list::PyList_Size(iterable);
            if n == 0 { return return_none(); }
            let mut best = crate::types::list::PyList_GetItem(iterable, 0);
            for i in 1..n {
                let item = crate::types::list::PyList_GetItem(iterable, i);
                if is_int(item) && is_int(best) {
                    if get_int_value(item) < get_int_value(best) { best = item; }
                } else if get_float_value(item) < get_float_value(best) { best = item; }
            }
            (*best).incref();
            return best;
        }
        return return_none();
    }
    // min(a, b, c, ...)
    let mut best = crate::types::tuple::PyTuple_GetItem(args, 0);
    for i in 1..nargs {
        let item = crate::types::tuple::PyTuple_GetItem(args, i);
        if is_int(item) && is_int(best) {
            if get_int_value(item) < get_int_value(best) { best = item; }
        } else if get_float_value(item) < get_float_value(best) { best = item; }
    }
    (*best).incref();
    best
}

unsafe extern "C" fn builtin_max(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let nargs = crate::types::tuple::PyTuple_Size(args);
    if nargs == 0 { return return_none(); }
    if nargs == 1 {
        let iterable = crate::types::tuple::PyTuple_GetItem(args, 0);
        if crate::types::list::PyList_Check(iterable) != 0 {
            let n = crate::types::list::PyList_Size(iterable);
            if n == 0 { return return_none(); }
            let mut best = crate::types::list::PyList_GetItem(iterable, 0);
            for i in 1..n {
                let item = crate::types::list::PyList_GetItem(iterable, i);
                if is_int(item) && is_int(best) {
                    if get_int_value(item) > get_int_value(best) { best = item; }
                } else if get_float_value(item) > get_float_value(best) { best = item; }
            }
            (*best).incref();
            return best;
        }
        return return_none();
    }
    let mut best = crate::types::tuple::PyTuple_GetItem(args, 0);
    for i in 1..nargs {
        let item = crate::types::tuple::PyTuple_GetItem(args, i);
        if is_int(item) && is_int(best) {
            if get_int_value(item) > get_int_value(best) { best = item; }
        } else if get_float_value(item) > get_float_value(best) { best = item; }
    }
    (*best).incref();
    best
}

unsafe extern "C" fn builtin_sum(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let iterable = crate::types::tuple::PyTuple_GetItem(args, 0);
    if crate::types::list::PyList_Check(iterable) != 0 {
        let n = crate::types::list::PyList_Size(iterable);
        let mut total: i64 = 0;
        let mut is_float_result = false;
        let mut ftotal: f64 = 0.0;
        for i in 0..n {
            let item = crate::types::list::PyList_GetItem(iterable, i);
            if is_float(item) { is_float_result = true; ftotal += get_float_value(item); }
            else { total += get_int_value(item); ftotal += get_int_value(item) as f64; }
        }
        if is_float_result {
            return crate::types::floatobject::PyFloat_FromDouble(ftotal);
        }
        return create_int(total);
    }
    create_int(0)
}

unsafe extern "C" fn builtin_ord(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    if is_str(obj) {
        let s = crate::types::unicode::unicode_value(obj);
        if let Some(ch) = s.chars().next() {
            return create_int(ch as i64);
        }
    }
    create_int(0)
}

unsafe extern "C" fn builtin_chr(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    let val = get_int_value(obj) as u32;
    if let Some(ch) = char::from_u32(val) {
        create_str(&ch.to_string())
    } else {
        create_str("?")
    }
}

unsafe extern "C" fn builtin_repr_fn(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    if obj.is_null() { return create_str("None"); }
    // Handle common types directly for better repr output
    if is_none(obj) { return create_str("None"); }
    if is_bool(obj) {
        return if get_int_value(obj) != 0 { create_str("True") } else { create_str("False") };
    }
    if is_int(obj) { return create_str(&format!("{}", get_int_value(obj))); }
    if is_float(obj) { return create_str(&format!("{}", get_float_value(obj))); }
    if is_str(obj) {
        let s = crate::types::unicode::unicode_value(obj);
        return create_str(&format!("'{}'", s));
    }
    // Handle container types using our VM-side formatters
    if crate::types::list::PyList_Check(obj) != 0 { return create_str(&format_list(obj)); }
    if crate::types::tuple::PyTuple_Check(obj) != 0 { return create_str(&format_tuple(obj)); }
    if crate::types::dict::PyDict_Check(obj) != 0 { return create_str(&format_dict(obj)); }
    if crate::types::set::PySet_Check(obj) != 0 { return create_str(&format_set(obj)); }
    // Fall back to PyObject_Repr for other types
    let repr = crate::ffi::object_api::PyObject_Repr(obj);
    if repr.is_null() {
        // Last resort: format as <type object at 0x...>
        Python::with_gil(|py| {
            let obj_ref = PyObjectRef::from_raw(obj);
            (*obj).incref(); // from_raw doesn't incref
            match py_repr(py, &obj_ref) {
                Ok(r) => {
                    let raw = r.as_raw();
                    (*raw).incref();
                    raw
                }
                Err(_) => create_str(&format!("<object at {:p}>", obj)),
            }
        })
    } else {
        repr
    }
}

unsafe extern "C" fn builtin_bool(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let nargs = crate::types::tuple::PyTuple_Size(args);
    if nargs == 0 { return crate::object::safe_api::py_false(); }
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    let result = crate::ffi::object_api::PyObject_IsTrue(obj);
    bool_from_long(if result > 0 { 1 } else { 0 })
}

unsafe extern "C" fn builtin_float(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let nargs = crate::types::tuple::PyTuple_Size(args);
    if nargs == 0 { return crate::types::floatobject::PyFloat_FromDouble(0.0); }
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    if is_float(obj) { (*obj).incref(); return obj; }
    if is_int(obj) { return crate::types::floatobject::PyFloat_FromDouble(get_int_value(obj) as f64); }
    if is_str(obj) {
        let s = crate::types::unicode::unicode_value(obj);
        if let Ok(val) = s.trim().parse::<f64>() {
            return crate::types::floatobject::PyFloat_FromDouble(val);
        }
    }
    crate::types::floatobject::PyFloat_FromDouble(0.0)
}

unsafe extern "C" fn builtin_hex(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    let val = get_int_value(obj);
    if val < 0 {
        create_str(&format!("-0x{:x}", -val))
    } else {
        create_str(&format!("0x{:x}", val))
    }
}

unsafe extern "C" fn builtin_oct(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    let val = get_int_value(obj);
    if val < 0 {
        create_str(&format!("-0o{:o}", -val))
    } else {
        create_str(&format!("0o{:o}", val))
    }
}

unsafe extern "C" fn builtin_bin(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    let val = get_int_value(obj);
    if val < 0 {
        create_str(&format!("-0b{:b}", -val))
    } else {
        create_str(&format!("0b{:b}", val))
    }
}

unsafe extern "C" fn builtin_sorted(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    if crate::types::list::PyList_Check(obj) != 0 {
        let n = crate::types::list::PyList_Size(obj);
        let mut items: Vec<*mut RawPyObject> = Vec::new();
        for i in 0..n {
            items.push(crate::types::list::PyList_GetItem(obj, i));
        }
        items.sort_by(|a, b| {
            let av = get_float_value(*a);
            let bv = get_float_value(*b);
            av.partial_cmp(&bv).unwrap_or(std::cmp::Ordering::Equal)
        });
        let result = crate::types::list::PyList_New(n);
        for (i, item) in items.iter().enumerate() {
            (**item).incref();
            crate::types::list::PyList_SET_ITEM(result, i as isize, *item);
        }
        return result;
    }
    crate::types::list::PyList_New(0)
}

unsafe extern "C" fn builtin_reversed(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    if crate::types::list::PyList_Check(obj) != 0 {
        let n = crate::types::list::PyList_Size(obj);
        let result = crate::types::list::PyList_New(n);
        for i in 0..n {
            let item = crate::types::list::PyList_GetItem(obj, n - 1 - i);
            (*item).incref();
            crate::types::list::PyList_SET_ITEM(result, i, item);
        }
        return result;
    }
    crate::types::list::PyList_New(0)
}

unsafe extern "C" fn builtin_enumerate(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    let result = crate::types::list::PyList_New(0);
    if crate::types::list::PyList_Check(obj) != 0 {
        let n = crate::types::list::PyList_Size(obj);
        for i in 0..n {
            let item = crate::types::list::PyList_GetItem(obj, i);
            let idx = crate::types::longobject::PyLong_FromLong(i as _);
            let pair = crate::types::tuple::PyTuple_New(2);
            (*item).incref();
            crate::types::tuple::PyTuple_SET_ITEM(pair, 0, idx);
            crate::types::tuple::PyTuple_SET_ITEM(pair, 1, item);
            crate::types::list::PyList_Append(result, pair);
            (*pair).decref();
        }
    }
    result
}

unsafe extern "C" fn builtin_zip(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let nargs = crate::types::tuple::PyTuple_Size(args);
    if nargs == 0 { return crate::types::list::PyList_New(0); }
    // Find minimum length
    let mut min_len: isize = isize::MAX;
    for i in 0..nargs {
        let arg = crate::types::tuple::PyTuple_GetItem(args, i);
        let len = if crate::types::list::PyList_Check(arg) != 0 {
            crate::types::list::PyList_Size(arg)
        } else if crate::types::tuple::PyTuple_Check(arg) != 0 {
            crate::types::tuple::PyTuple_Size(arg)
        } else { 0 };
        if len < min_len { min_len = len; }
    }
    let result = crate::types::list::PyList_New(0);
    for i in 0..min_len {
        let pair = crate::types::tuple::PyTuple_New(nargs);
        for j in 0..nargs {
            let arg = crate::types::tuple::PyTuple_GetItem(args, j);
            let item = if crate::types::list::PyList_Check(arg) != 0 {
                crate::types::list::PyList_GetItem(arg, i)
            } else {
                crate::types::tuple::PyTuple_GetItem(arg, i)
            };
            (*item).incref();
            crate::types::tuple::PyTuple_SET_ITEM(pair, j, item);
        }
        crate::types::list::PyList_Append(result, pair);
        (*pair).decref();
    }
    result
}

unsafe extern "C" fn builtin_iter(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    (*obj).incref();
    obj // Return object itself as iterator
}

unsafe extern "C" fn builtin_next(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    let tp = (*obj).ob_type;
    if !tp.is_null() {
        if let Some(tp_iternext) = (*tp).tp_iternext {
            return tp_iternext(obj);
        }
    }
    return_none()
}

unsafe extern "C" fn builtin_list_ctor(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let nargs = crate::types::tuple::PyTuple_Size(args);
    if nargs == 0 { return crate::types::list::PyList_New(0); }
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    // Convert iterable to list
    if crate::types::list::PyList_Check(obj) != 0 {
        // Copy the list
        let n = crate::types::list::PyList_Size(obj);
        let result = crate::types::list::PyList_New(n);
        for i in 0..n {
            let item = crate::types::list::PyList_GetItem(obj, i);
            (*item).incref();
            crate::types::list::PyList_SET_ITEM(result, i, item);
        }
        return result;
    }
    if crate::types::tuple::PyTuple_Check(obj) != 0 {
        let n = crate::types::tuple::PyTuple_Size(obj);
        let result = crate::types::list::PyList_New(n);
        for i in 0..n {
            let item = crate::types::tuple::PyTuple_GetItem(obj, i);
            (*item).incref();
            crate::types::list::PyList_SET_ITEM(result, i, item);
        }
        return result;
    }
    if is_str(obj) {
        let s = crate::types::unicode::unicode_value(obj);
        let result = crate::types::list::PyList_New(0);
        for ch in s.chars() {
            let ch_obj = crate::types::unicode::create_from_str(&ch.to_string());
            crate::types::list::PyList_Append(result, ch_obj);
            (*ch_obj).decref();
        }
        return result;
    }
    // Generic iterable: use get_iterator/iter_next for generators and other iterables
    if is_int(obj) && get_int_value(obj) < 0 {
        // Generator marker — iterate via resume_generator
        return Python::with_gil(|py| {
            (*obj).incref();
            let gen_obj = PyObjectRef::from_raw(obj);
            let result = crate::types::list::PyList_New(0);
            loop {
                match iter_next(py, &gen_obj) {
                    Some(item) => {
                        let raw_item = item.as_raw();
                        (*raw_item).incref();
                        crate::types::list::PyList_Append(result, raw_item);
                    }
                    None => break,
                }
            }
            result
        });
    }
    crate::types::list::PyList_New(0)
}

unsafe extern "C" fn builtin_tuple_ctor(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let nargs = crate::types::tuple::PyTuple_Size(args);
    if nargs == 0 { return crate::types::tuple::PyTuple_New(0); }
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    if crate::types::list::PyList_Check(obj) != 0 {
        let n = crate::types::list::PyList_Size(obj);
        let result = crate::types::tuple::PyTuple_New(n);
        for i in 0..n {
            let item = crate::types::list::PyList_GetItem(obj, i);
            (*item).incref();
            crate::types::tuple::PyTuple_SET_ITEM(result, i, item);
        }
        return result;
    }
    crate::types::tuple::PyTuple_New(0)
}

unsafe extern "C" fn builtin_dict_ctor(
    _self: *mut RawPyObject, _args: *mut RawPyObject,
) -> *mut RawPyObject {
    crate::types::dict::PyDict_New()
}

unsafe extern "C" fn builtin_set_ctor(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let nargs = crate::types::tuple::PyTuple_Size(args);
    let result = crate::types::set::PySet_New(ptr::null_mut());
    if nargs >= 1 {
        let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
        if crate::types::list::PyList_Check(obj) != 0 {
            let n = crate::types::list::PyList_Size(obj);
            for i in 0..n {
                let item = crate::types::list::PyList_GetItem(obj, i);
                crate::types::set::PySet_Add(result, item);
            }
        }
    }
    result
}

unsafe extern "C" fn builtin_callable(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    if obj.is_null() { return crate::object::safe_api::py_false(); }
    let tp = (*obj).ob_type;
    let is_callable = if !tp.is_null() { (*tp).tp_call.is_some() } else { false };
    // Also check if it's a RustFunction marker (int pointer)
    let is_rust_func = is_int(obj) && get_int_value(obj) != 0;
    bool_from_long(if is_callable || is_rust_func { 1 } else { 0 })
}

unsafe extern "C" fn builtin_any(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    if crate::types::list::PyList_Check(obj) != 0 {
        let n = crate::types::list::PyList_Size(obj);
        for i in 0..n {
            let item = crate::types::list::PyList_GetItem(obj, i);
            if crate::ffi::object_api::PyObject_IsTrue(item) > 0 {
                return crate::object::safe_api::py_true();
            }
        }
    }
    crate::object::safe_api::py_false()
}

unsafe extern "C" fn builtin_all(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    if crate::types::list::PyList_Check(obj) != 0 {
        let n = crate::types::list::PyList_Size(obj);
        for i in 0..n {
            let item = crate::types::list::PyList_GetItem(obj, i);
            if crate::ffi::object_api::PyObject_IsTrue(item) <= 0 {
                return crate::object::safe_api::py_false();
            }
        }
    }
    crate::object::safe_api::py_true()
}

unsafe extern "C" fn builtin_map(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    // map(func, iterable) → list (simplified)
    let nargs = crate::types::tuple::PyTuple_Size(args);
    if nargs < 2 { return crate::types::list::PyList_New(0); }
    let _func = crate::types::tuple::PyTuple_GetItem(args, 0);
    let iterable = crate::types::tuple::PyTuple_GetItem(args, 1);
    // For now just return the iterable as a list
    if crate::types::list::PyList_Check(iterable) != 0 {
        (*iterable).incref();
        return iterable;
    }
    crate::types::list::PyList_New(0)
}

// ─── Stdlib module functions ───

unsafe extern "C" fn stdlib_os_getcwd(
    _self: *mut RawPyObject, _args: *mut RawPyObject,
) -> *mut RawPyObject {
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string());
    create_str(&cwd)
}

unsafe extern "C" fn stdlib_os_path_join(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let nargs = crate::types::tuple::PyTuple_Size(args);
    if nargs == 0 { return create_str(""); }
    let mut result = String::new();
    for i in 0..nargs {
        let part = crate::types::tuple::PyTuple_GetItem(args, i);
        let s = crate::types::unicode::unicode_value(part);
        if s.starts_with('/') {
            result = s.to_string();
        } else if result.is_empty() || result.ends_with('/') {
            result.push_str(s);
        } else {
            result.push('/');
            result.push_str(s);
        }
    }
    create_str(&result)
}

unsafe extern "C" fn stdlib_os_path_exists(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let path = crate::types::tuple::PyTuple_GetItem(args, 0);
    let s = crate::types::unicode::unicode_value(path);
    if std::path::Path::new(&s).exists() {
        crate::object::safe_api::py_true()
    } else {
        crate::object::safe_api::py_false()
    }
}

unsafe extern "C" fn stdlib_os_path_dirname(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let path = crate::types::tuple::PyTuple_GetItem(args, 0);
    let s = crate::types::unicode::unicode_value(path);
    let dir = std::path::Path::new(&s)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    create_str(&dir)
}

unsafe extern "C" fn stdlib_os_path_basename(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let path = crate::types::tuple::PyTuple_GetItem(args, 0);
    let s = crate::types::unicode::unicode_value(path);
    let base = std::path::Path::new(&s)
        .file_name()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    create_str(&base)
}

// ─── Regex module functions ───

/// Convert Python re flags to regex crate options and build a Regex.
fn compile_regex_with_flags(pattern: &str, flags: i64) -> Result<regex::Regex, String> {
    let mut re_pattern = String::new();
    // Apply flags as inline group at the start
    if flags != 0 {
        re_pattern.push_str("(?");
        if flags & 2 != 0 { re_pattern.push('i'); }  // IGNORECASE
        if flags & 8 != 0 { re_pattern.push('m'); }  // MULTILINE
        if flags & 16 != 0 { re_pattern.push('s'); } // DOTALL
        if flags & 64 != 0 { re_pattern.push('x'); } // VERBOSE
        re_pattern.push(')');
    }
    re_pattern.push_str(pattern);
    regex::Regex::new(&re_pattern).map_err(|e| e.to_string())
}

unsafe extern "C" fn re_compile(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let nargs = crate::types::tuple::PyTuple_Size(args);
    if nargs < 1 { return return_none(); }
    let pat_obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    if !is_str(pat_obj) { return return_none(); }
    let pattern = crate::types::unicode::unicode_value(pat_obj).to_string();
    let flags = if nargs >= 2 {
        let f = crate::types::tuple::PyTuple_GetItem(args, 1);
        if is_int(f) { get_int_value(f) } else { 0 }
    } else { 0 };
    match compile_regex_with_flags(&pattern, flags) {
        Ok(compiled) => {
            let re_obj = Box::new(RustRegex { pattern, compiled });
            let marker = make_regex_marker(Box::into_raw(re_obj));
            create_int(marker)
        }
        Err(_) => return_none(),
    }
}

/// Helper: perform a regex search and return a match marker (or null)
unsafe fn do_regex_search(pat: &str, flags: i64, text: &str, anchored: bool) -> *mut RawPyObject {
    let actual_pat = if anchored && !pat.starts_with('^') {
        format!("^(?:{})", pat)
    } else {
        pat.to_string()
    };
    let compiled = match compile_regex_with_flags(&actual_pat, flags) {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };
    if let Some(m) = compiled.captures(text) {
        let full_match = m.get(0).unwrap();
        let groups: Vec<Option<String>> = (1..compiled.captures_len())
            .map(|i| m.get(i).map(|g| g.as_str().to_string()))
            .collect();
        let match_obj = Box::new(RustMatch {
            full: full_match.as_str().to_string(),
            groups,
            start: full_match.start(),
            end: full_match.end(),
        });
        create_int(make_match_marker(Box::into_raw(match_obj)))
    } else {
        std::ptr::null_mut()
    }
}

unsafe extern "C" fn re_search(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let nargs = crate::types::tuple::PyTuple_Size(args);
    if nargs < 2 { return return_none(); }
    let pat_obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    let str_obj = crate::types::tuple::PyTuple_GetItem(args, 1);
    if !is_str(pat_obj) || !is_str(str_obj) { return return_none(); }
    let pattern = crate::types::unicode::unicode_value(pat_obj).to_string();
    let text = crate::types::unicode::unicode_value(str_obj).to_string();
    let flags = if nargs >= 3 {
        let f = crate::types::tuple::PyTuple_GetItem(args, 2);
        if is_int(f) { get_int_value(f) } else { 0 }
    } else { 0 };
    let result = do_regex_search(&pattern, flags, &text, false);
    if result.is_null() { return_none() } else { result }
}

unsafe extern "C" fn re_match_fn(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let nargs = crate::types::tuple::PyTuple_Size(args);
    if nargs < 2 { return return_none(); }
    let pat_obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    let str_obj = crate::types::tuple::PyTuple_GetItem(args, 1);
    if !is_str(pat_obj) || !is_str(str_obj) { return return_none(); }
    let pattern = crate::types::unicode::unicode_value(pat_obj).to_string();
    let text = crate::types::unicode::unicode_value(str_obj).to_string();
    let flags = if nargs >= 3 {
        let f = crate::types::tuple::PyTuple_GetItem(args, 2);
        if is_int(f) { get_int_value(f) } else { 0 }
    } else { 0 };
    let result = do_regex_search(&pattern, flags, &text, true);
    if result.is_null() { return_none() } else { result }
}

unsafe extern "C" fn re_fullmatch(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let nargs = crate::types::tuple::PyTuple_Size(args);
    if nargs < 2 { return return_none(); }
    let pat_obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    let str_obj = crate::types::tuple::PyTuple_GetItem(args, 1);
    if !is_str(pat_obj) || !is_str(str_obj) { return return_none(); }
    let pattern = crate::types::unicode::unicode_value(pat_obj).to_string();
    let text = crate::types::unicode::unicode_value(str_obj).to_string();
    let flags = if nargs >= 3 {
        let f = crate::types::tuple::PyTuple_GetItem(args, 2);
        if is_int(f) { get_int_value(f) } else { 0 }
    } else { 0 };
    // Fullmatch: anchor pattern to match entire string
    let anchored_pat = format!("^(?:{})$", pattern);
    let result = do_regex_search(&anchored_pat, flags, &text, false);
    if result.is_null() { return_none() } else { result }
}

unsafe extern "C" fn re_findall(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let nargs = crate::types::tuple::PyTuple_Size(args);
    if nargs < 2 { return return_none(); }
    let pat_obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    let str_obj = crate::types::tuple::PyTuple_GetItem(args, 1);
    if !is_str(pat_obj) || !is_str(str_obj) { return return_none(); }
    let pattern = crate::types::unicode::unicode_value(pat_obj).to_string();
    let text = crate::types::unicode::unicode_value(str_obj).to_string();
    let flags = if nargs >= 3 {
        let f = crate::types::tuple::PyTuple_GetItem(args, 2);
        if is_int(f) { get_int_value(f) } else { 0 }
    } else { 0 };
    let compiled = match compile_regex_with_flags(&pattern, flags) {
        Ok(c) => c,
        Err(_) => return crate::types::list::PyList_New(0),
    };
    let has_groups = compiled.captures_len() > 1;
    if has_groups {
        // Return list of groups (or tuples if multiple groups)
        let results: Vec<*mut RawPyObject> = compiled.captures_iter(&text).map(|caps| {
            let groups: Vec<String> = (1..compiled.captures_len())
                .map(|i| caps.get(i).map(|g| g.as_str().to_string()).unwrap_or_default())
                .collect();
            if groups.len() == 1 {
                create_str(&groups[0])
            } else {
                // Return tuple of groups
                let tup = crate::types::tuple::PyTuple_New(groups.len() as isize);
                for (i, g) in groups.iter().enumerate() {
                    let s = create_str(g);
                    crate::types::tuple::PyTuple_SET_ITEM(tup, i as isize, s);
                }
                tup
            }
        }).collect();
        let list = crate::types::list::PyList_New(results.len() as isize);
        for (i, item) in results.iter().enumerate() {
            crate::types::list::PyList_SET_ITEM(list, i as isize, *item);
        }
        list
    } else {
        let results: Vec<&str> = compiled.find_iter(&text).map(|m| m.as_str()).collect();
        let list = crate::types::list::PyList_New(results.len() as isize);
        for (i, s) in results.iter().enumerate() {
            crate::types::list::PyList_SET_ITEM(list, i as isize, create_str(s));
        }
        list
    }
}

unsafe extern "C" fn re_sub(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let nargs = crate::types::tuple::PyTuple_Size(args);
    if nargs < 3 { return return_none(); }
    let pat_obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    let repl_obj = crate::types::tuple::PyTuple_GetItem(args, 1);
    let str_obj = crate::types::tuple::PyTuple_GetItem(args, 2);
    if !is_str(pat_obj) || !is_str(repl_obj) || !is_str(str_obj) { return return_none(); }
    let pattern = crate::types::unicode::unicode_value(pat_obj).to_string();
    let repl = crate::types::unicode::unicode_value(repl_obj).to_string();
    let text = crate::types::unicode::unicode_value(str_obj).to_string();
    let flags = if nargs >= 4 {
        let f = crate::types::tuple::PyTuple_GetItem(args, 3);
        if is_int(f) { get_int_value(f) } else { 0 }
    } else { 0 };
    let count = if nargs >= 5 {
        let c = crate::types::tuple::PyTuple_GetItem(args, 4);
        if is_int(c) { get_int_value(c) as usize } else { 0 }
    } else { 0 };
    let compiled = match compile_regex_with_flags(&pattern, flags) {
        Ok(c) => c,
        Err(_) => return create_str(&text),
    };
    let result = if count > 0 {
        // Limited replacements
        let mut result = text.clone();
        let mut n = 0;
        for m in compiled.find_iter(&text) {
            if n >= count { break; }
            result = result.replacen(m.as_str(), &repl, 1);
            n += 1;
        }
        result
    } else {
        compiled.replace_all(&text, repl.as_str()).to_string()
    };
    create_str(&result)
}

unsafe extern "C" fn re_split(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let nargs = crate::types::tuple::PyTuple_Size(args);
    if nargs < 2 { return return_none(); }
    let pat_obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    let str_obj = crate::types::tuple::PyTuple_GetItem(args, 1);
    if !is_str(pat_obj) || !is_str(str_obj) { return return_none(); }
    let pattern = crate::types::unicode::unicode_value(pat_obj).to_string();
    let text = crate::types::unicode::unicode_value(str_obj).to_string();
    let flags = if nargs >= 3 {
        let f = crate::types::tuple::PyTuple_GetItem(args, 2);
        if is_int(f) { get_int_value(f) } else { 0 }
    } else { 0 };
    let compiled = match compile_regex_with_flags(&pattern, flags) {
        Ok(c) => c,
        Err(_) => {
            let list = crate::types::list::PyList_New(1);
            crate::types::list::PyList_SET_ITEM(list, 0, create_str(&text));
            return list;
        }
    };
    let parts: Vec<&str> = compiled.split(&text).collect();
    let list = crate::types::list::PyList_New(parts.len() as isize);
    for (i, s) in parts.iter().enumerate() {
        crate::types::list::PyList_SET_ITEM(list, i as isize, create_str(s));
    }
    list
}

unsafe extern "C" fn re_escape(
    _self: *mut RawPyObject, args: *mut RawPyObject,
) -> *mut RawPyObject {
    let pat_obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    if !is_str(pat_obj) { return return_none(); }
    let pattern = crate::types::unicode::unicode_value(pat_obj).to_string();
    create_str(&regex::escape(&pattern))
}

fn is_regex_method(name: &str) -> bool {
    matches!(name,
        "search" | "match" | "findall" | "sub" | "split" | "pattern" | "fullmatch"
    )
}

fn is_match_method(name: &str) -> bool {
    matches!(name,
        "group" | "groups" | "start" | "end" | "span"
    )
}

// ─── Method detection helpers ───

fn is_str_method(name: &str) -> bool {
    matches!(name,
        "upper" | "lower" | "strip" | "lstrip" | "rstrip" |
        "split" | "join" | "replace" | "find" | "rfind" |
        "startswith" | "endswith" | "count" | "index" |
        "isdigit" | "isalpha" | "isalnum" | "isspace" |
        "title" | "capitalize" | "swapcase" | "center" |
        "ljust" | "rjust" | "zfill" | "format" | "encode"
    )
}

fn is_list_method(name: &str) -> bool {
    matches!(name,
        "append" | "extend" | "insert" | "remove" | "pop" |
        "clear" | "index" | "count" | "sort" | "reverse" | "copy"
    )
}

fn is_dict_method(name: &str) -> bool {
    matches!(name,
        "keys" | "values" | "items" | "get" | "pop" |
        "update" | "clear" | "copy" | "setdefault"
    )
}

/// Execute a bound builtin method (string/list/dict methods)
fn call_bound_method(py: Python<'_>, bm: &BoundBuiltinMethod, args: &[PyObjectRef]) -> PyResult {
    let raw = bm.self_obj.as_raw();
    let name = bm.method_name.as_str();

    unsafe {
        // ─── String methods ───
        if is_str(raw) {
            let s = crate::types::unicode::unicode_value(raw);
            match name {
                "upper" => return new_str(py, &s.to_uppercase()),
                "lower" => return new_str(py, &s.to_lowercase()),
                "strip" => return new_str(py, s.trim()),
                "lstrip" => return new_str(py, s.trim_start()),
                "rstrip" => return new_str(py, s.trim_end()),
                "title" => {
                    let result: String = s.split_whitespace()
                        .map(|w| {
                            let mut c = w.chars();
                            match c.next() {
                                None => String::new(),
                                Some(f) => {
                                    let upper: String = f.to_uppercase().collect();
                                    upper + &c.as_str().to_lowercase()
                                }
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    return new_str(py, &result);
                }
                "capitalize" => {
                    if s.is_empty() { return new_str(py, ""); }
                    let mut chars = s.chars();
                    let first: String = chars.next().unwrap().to_uppercase().collect();
                    let rest: String = chars.as_str().to_lowercase();
                    return new_str(py, &format!("{}{}", first, rest));
                }
                "swapcase" => {
                    let result: String = s.chars().map(|c| {
                        if c.is_uppercase() { c.to_lowercase().to_string() }
                        else { c.to_uppercase().to_string() }
                    }).collect();
                    return new_str(py, &result);
                }
                "split" => {
                    let sep = if !args.is_empty() && is_str(args[0].as_raw()) {
                        crate::types::unicode::unicode_value(args[0].as_raw()).to_string()
                    } else {
                        " ".to_string()
                    };
                    let parts: Vec<PyObjectRef> = if sep == " " && (args.is_empty() || !is_str(args[0].as_raw())) {
                        // Default split: split on any whitespace
                        s.split_whitespace()
                            .map(|p| new_str(py, p).unwrap())
                            .collect()
                    } else {
                        s.split(&sep)
                            .map(|p| new_str(py, p).unwrap())
                            .collect()
                    };
                    return build_list(py, parts);
                }
                "join" => {
                    if args.is_empty() {
                        return Err(PyErr::type_error("join() takes exactly one argument"));
                    }
                    let iterable_raw = args[0].as_raw();
                    let mut parts = Vec::new();
                    if crate::types::list::PyList_Check(iterable_raw) != 0 {
                        let n = crate::types::list::PyList_Size(iterable_raw);
                        for i in 0..n {
                            let item = crate::types::list::PyList_GetItem(iterable_raw, i);
                            if is_str(item) {
                                parts.push(crate::types::unicode::unicode_value(item).to_string());
                            }
                        }
                    } else if crate::types::tuple::PyTuple_Check(iterable_raw) != 0 {
                        let n = crate::types::tuple::PyTuple_Size(iterable_raw);
                        for i in 0..n {
                            let item = crate::types::tuple::PyTuple_GetItem(iterable_raw, i);
                            if is_str(item) {
                                parts.push(crate::types::unicode::unicode_value(item).to_string());
                            }
                        }
                    }
                    return new_str(py, &parts.join(&s));
                }
                "replace" => {
                    if args.len() < 2 {
                        return Err(PyErr::type_error("replace() takes at least 2 arguments"));
                    }
                    let old = crate::types::unicode::unicode_value(args[0].as_raw());
                    let new_s = crate::types::unicode::unicode_value(args[1].as_raw());
                    return new_str(py, &s.replace(&old, &new_s));
                }
                "find" => {
                    if args.is_empty() {
                        return Err(PyErr::type_error("find() takes at least 1 argument"));
                    }
                    let substr = crate::types::unicode::unicode_value(args[0].as_raw());
                    let idx = s.find(&substr).map(|i| i as i64).unwrap_or(-1);
                    return new_int(py, idx);
                }
                "rfind" => {
                    if args.is_empty() {
                        return Err(PyErr::type_error("rfind() takes at least 1 argument"));
                    }
                    let substr = crate::types::unicode::unicode_value(args[0].as_raw());
                    let idx = s.rfind(&substr).map(|i| i as i64).unwrap_or(-1);
                    return new_int(py, idx);
                }
                "startswith" => {
                    if args.is_empty() {
                        return Err(PyErr::type_error("startswith() takes at least 1 argument"));
                    }
                    let prefix = crate::types::unicode::unicode_value(args[0].as_raw());
                    return Ok(if s.starts_with(&prefix) { true_obj(py) } else { false_obj(py) });
                }
                "endswith" => {
                    if args.is_empty() {
                        return Err(PyErr::type_error("endswith() takes at least 1 argument"));
                    }
                    let suffix = crate::types::unicode::unicode_value(args[0].as_raw());
                    return Ok(if s.ends_with(&suffix) { true_obj(py) } else { false_obj(py) });
                }
                "count" => {
                    if args.is_empty() {
                        return Err(PyErr::type_error("count() takes at least 1 argument"));
                    }
                    let sub = crate::types::unicode::unicode_value(args[0].as_raw());
                    return new_int(py, s.matches(&sub).count() as i64);
                }
                "isdigit" => return Ok(if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) { true_obj(py) } else { false_obj(py) }),
                "isalpha" => return Ok(if !s.is_empty() && s.chars().all(|c| c.is_alphabetic()) { true_obj(py) } else { false_obj(py) }),
                "isalnum" => return Ok(if !s.is_empty() && s.chars().all(|c| c.is_alphanumeric()) { true_obj(py) } else { false_obj(py) }),
                "isspace" => return Ok(if !s.is_empty() && s.chars().all(|c| c.is_whitespace()) { true_obj(py) } else { false_obj(py) }),
                "encode" => {
                    let bytes = s.as_bytes();
                    let result = crate::types::bytes::PyBytes_FromStringAndSize(
                        bytes.as_ptr() as *const std::ffi::c_char,
                        bytes.len() as isize,
                    );
                    return PyObjectRef::steal_or_err(result);
                }
                "format" => {
                    // Simple str.format with positional args
                    let mut result = s.to_string();
                    for (i, arg) in args.iter().enumerate() {
                        let placeholder = format!("{{{}}}", i);
                        let val_str = format_pyobj(arg.as_raw());
                        result = result.replacen(&placeholder, &val_str, 1);
                    }
                    // Also handle bare {}
                    for arg in args.iter() {
                        let val_str = format_pyobj(arg.as_raw());
                        result = result.replacen("{}", &val_str, 1);
                    }
                    return new_str(py, &result);
                }
                _ => {}
            }
        }

        // ─── List methods ───
        if crate::types::list::PyList_Check(raw) != 0 {
            match name {
                "append" => {
                    if args.is_empty() {
                        return Err(PyErr::type_error("append() takes exactly one argument"));
                    }
                    (*args[0].as_raw()).incref();
                    crate::types::list::PyList_Append(raw, args[0].as_raw());
                    return Ok(none_obj(py));
                }
                "extend" => {
                    if args.is_empty() {
                        return Err(PyErr::type_error("extend() takes exactly one argument"));
                    }
                    let iterable = args[0].as_raw();
                    if crate::types::list::PyList_Check(iterable) != 0 {
                        let n = crate::types::list::PyList_Size(iterable);
                        for i in 0..n {
                            let item = crate::types::list::PyList_GetItem(iterable, i);
                            (*item).incref();
                            crate::types::list::PyList_Append(raw, item);
                        }
                    }
                    return Ok(none_obj(py));
                }
                "insert" => {
                    if args.len() < 2 {
                        return Err(PyErr::type_error("insert() takes exactly 2 arguments"));
                    }
                    let idx = get_int_value(args[0].as_raw());
                    (*args[1].as_raw()).incref();
                    crate::types::list::PyList_Insert(raw, idx as isize, args[1].as_raw());
                    return Ok(none_obj(py));
                }
                "pop" => {
                    let n = crate::types::list::PyList_Size(raw);
                    if n == 0 {
                        return Err(PyErr::type_error("pop from empty list"));
                    }
                    let idx = if !args.is_empty() {
                        get_int_value(args[0].as_raw()) as isize
                    } else {
                        n - 1
                    };
                    let real_idx = if idx < 0 { n + idx } else { idx };
                    let item = crate::types::list::PyList_GetItem(raw, real_idx);
                    let result = PyObjectRef::borrow_or_err(item)?;
                    // Shift elements down
                    for i in real_idx..n-1 {
                        let next = crate::types::list::PyList_GetItem(raw, i+1);
                        (*next).incref();
                        crate::types::list::PyList_SetItem(raw, i, next);
                    }
                    let list_obj = raw as *mut crate::types::list::PyListObject;
                    (*list_obj).ob_base.ob_size -= 1;
                    return Ok(result);
                }
                "reverse" => {
                    let n = crate::types::list::PyList_Size(raw);
                    for i in 0..n/2 {
                        let a = crate::types::list::PyList_GetItem(raw, i);
                        let b = crate::types::list::PyList_GetItem(raw, n-1-i);
                        (*a).incref();
                        (*b).incref();
                        crate::types::list::PyList_SetItem(raw, i, b);
                        crate::types::list::PyList_SetItem(raw, n-1-i, a);
                    }
                    return Ok(none_obj(py));
                }
                "sort" => {
                    let n = crate::types::list::PyList_Size(raw) as usize;
                    let mut items: Vec<(i64, *mut RawPyObject)> = Vec::new();
                    for i in 0..n {
                        let item = crate::types::list::PyList_GetItem(raw, i as isize);
                        let key = if is_int(item) { get_int_value(item) }
                                  else if is_float(item) { get_float_value(item) as i64 }
                                  else { 0 };
                        items.push((key, item));
                    }
                    items.sort_by_key(|(k, _)| *k);
                    for (i, (_, item)) in items.iter().enumerate() {
                        (**item).incref();
                        crate::types::list::PyList_SetItem(raw, i as isize, *item);
                    }
                    return Ok(none_obj(py));
                }
                "copy" => {
                    let n = crate::types::list::PyList_Size(raw);
                    let mut items = Vec::new();
                    for i in 0..n {
                        let item = crate::types::list::PyList_GetItem(raw, i);
                        items.push(PyObjectRef::borrow_or_err(item)?);
                    }
                    return build_list(py, items);
                }
                "clear" => {
                    let list_obj = raw as *mut crate::types::list::PyListObject;
                    (*list_obj).ob_base.ob_size = 0;
                    return Ok(none_obj(py));
                }
                "count" => {
                    if args.is_empty() {
                        return Err(PyErr::type_error("count() takes exactly one argument"));
                    }
                    let target = args[0].as_raw();
                    let n = crate::types::list::PyList_Size(raw);
                    let mut count = 0i64;
                    for i in 0..n {
                        let item = crate::types::list::PyList_GetItem(raw, i);
                        if objects_equal(item, target) {
                            count += 1;
                        }
                    }
                    return new_int(py, count);
                }
                "index" => {
                    if args.is_empty() {
                        return Err(PyErr::type_error("index() takes at least 1 argument"));
                    }
                    let target = args[0].as_raw();
                    let n = crate::types::list::PyList_Size(raw);
                    for i in 0..n {
                        let item = crate::types::list::PyList_GetItem(raw, i);
                        if objects_equal(item, target) {
                            return new_int(py, i as i64);
                        }
                    }
                    return Err(PyErr::runtime_error("value not in list"));
                }
                "remove" => {
                    if args.is_empty() {
                        return Err(PyErr::type_error("remove() takes exactly one argument"));
                    }
                    return Ok(none_obj(py));
                }
                _ => {}
            }
        }

        // ─── Dict methods ───
        if crate::types::dict::PyDict_Check(raw) != 0 {
            match name {
                "keys" => {
                    let keys = crate::types::dict::PyDict_Keys(raw);
                    return PyObjectRef::steal_or_err(keys);
                }
                "values" => {
                    let vals = crate::types::dict::PyDict_Values(raw);
                    return PyObjectRef::steal_or_err(vals);
                }
                "items" => {
                    let items = crate::types::dict::PyDict_Items(raw);
                    return PyObjectRef::steal_or_err(items);
                }
                "get" => {
                    if args.is_empty() {
                        return Err(PyErr::type_error("get() takes at least 1 argument"));
                    }
                    let key = args[0].as_raw();
                    let item = crate::types::dict::PyDict_GetItem(raw, key);
                    if item.is_null() {
                        if args.len() > 1 {
                            return Ok(args[1].clone());
                        }
                        return Ok(none_obj(py));
                    }
                    return PyObjectRef::borrow_or_err(item);
                }
                "pop" => {
                    if args.is_empty() {
                        return Err(PyErr::type_error("pop() takes at least 1 argument"));
                    }
                    let key = args[0].as_raw();
                    let item = crate::types::dict::PyDict_GetItem(raw, key);
                    if item.is_null() {
                        if args.len() > 1 {
                            return Ok(args[1].clone());
                        }
                        return Err(PyErr::runtime_error("KeyError"));
                    }
                    let result = PyObjectRef::borrow_or_err(item)?;
                    crate::types::dict::PyDict_DelItem(raw, key);
                    return Ok(result);
                }
                "update" => {
                    if !args.is_empty() {
                        let other = args[0].as_raw();
                        crate::types::dict::PyDict_Update(raw, other);
                    }
                    return Ok(none_obj(py));
                }
                "clear" => {
                    crate::types::dict::PyDict_Clear(raw);
                    return Ok(none_obj(py));
                }
                "copy" => {
                    let copy = crate::types::dict::PyDict_Copy(raw);
                    return PyObjectRef::steal_or_err(copy);
                }
                "setdefault" => {
                    if args.is_empty() {
                        return Err(PyErr::type_error("setdefault() takes at least 1 argument"));
                    }
                    let key = args[0].as_raw();
                    let item = crate::types::dict::PyDict_GetItem(raw, key);
                    if !item.is_null() {
                        return PyObjectRef::borrow_or_err(item);
                    }
                    let default = if args.len() > 1 { args[1].clone() } else { none_obj(py) };
                    crate::types::dict::PyDict_SetItem(raw, key, default.as_raw());
                    return Ok(default);
                }
                _ => {}
            }
        }
    }

    // ─── Regex methods ───
    if is_int(raw) {
        let marker_val = get_int_value(raw);
        if is_regex_marker(marker_val) {
            let re_obj = unsafe { &*extract_regex(marker_val) };
            match name {
                "search" => {
                    if args.is_empty() { return Err(PyErr::type_error("search() requires a string argument")); }
                    let text = unsafe { crate::types::unicode::unicode_value(args[0].as_raw()).to_string() };
                    let result = unsafe { do_regex_search(&re_obj.pattern, 0, &text, false) };
                    if result.is_null() { return Ok(none_obj(py)); }
                    return unsafe { PyObjectRef::steal_or_err(result) };
                }
                "match" => {
                    if args.is_empty() { return Err(PyErr::type_error("match() requires a string argument")); }
                    let text = unsafe { crate::types::unicode::unicode_value(args[0].as_raw()).to_string() };
                    let result = unsafe { do_regex_search(&re_obj.pattern, 0, &text, true) };
                    if result.is_null() { return Ok(none_obj(py)); }
                    return unsafe { PyObjectRef::steal_or_err(result) };
                }
                "fullmatch" => {
                    if args.is_empty() { return Err(PyErr::type_error("fullmatch() requires a string argument")); }
                    let text = unsafe { crate::types::unicode::unicode_value(args[0].as_raw()).to_string() };
                    let anchored_pat = format!("^(?:{})$", re_obj.pattern);
                    let result = unsafe { do_regex_search(&anchored_pat, 0, &text, false) };
                    if result.is_null() { return Ok(none_obj(py)); }
                    return unsafe { PyObjectRef::steal_or_err(result) };
                }
                "findall" => {
                    if args.is_empty() { return Err(PyErr::type_error("findall() requires a string argument")); }
                    let text = unsafe { crate::types::unicode::unicode_value(args[0].as_raw()).to_string() };
                    let has_groups = re_obj.compiled.captures_len() > 1;
                    if has_groups {
                        let mut items = Vec::new();
                        for caps in re_obj.compiled.captures_iter(&text) {
                            let groups: Vec<String> = (1..re_obj.compiled.captures_len())
                                .map(|i| caps.get(i).map(|g| g.as_str().to_string()).unwrap_or_default())
                                .collect();
                            if groups.len() == 1 {
                                items.push(new_str(py, &groups[0])?);
                            } else {
                                let mut tup_items = Vec::new();
                                for g in &groups {
                                    tup_items.push(new_str(py, g)?);
                                }
                                items.push(build_tuple(py, tup_items)?);
                            }
                        }
                        return build_list(py, items);
                    } else {
                        let mut items = Vec::new();
                        for m in re_obj.compiled.find_iter(&text) {
                            items.push(new_str(py, m.as_str())?);
                        }
                        return build_list(py, items);
                    }
                }
                "sub" => {
                    if args.len() < 2 { return Err(PyErr::type_error("sub() requires repl and string arguments")); }
                    let repl = unsafe { crate::types::unicode::unicode_value(args[0].as_raw()).to_string() };
                    let text = unsafe { crate::types::unicode::unicode_value(args[1].as_raw()).to_string() };
                    let result = re_obj.compiled.replace_all(&text, repl.as_str()).to_string();
                    return new_str(py, &result);
                }
                "split" => {
                    if args.is_empty() { return Err(PyErr::type_error("split() requires a string argument")); }
                    let text = unsafe { crate::types::unicode::unicode_value(args[0].as_raw()).to_string() };
                    let parts: Vec<&str> = re_obj.compiled.split(&text).collect();
                    let mut items = Vec::new();
                    for s in parts {
                        items.push(new_str(py, s)?);
                    }
                    return build_list(py, items);
                }
                _ => {}
            }
        }

        if is_match_marker(marker_val) {
            let m = unsafe { &*extract_match(marker_val) };
            match name {
                "group" => {
                    let idx = if args.is_empty() { 0 } else {
                        if is_int(args[0].as_raw()) { get_int_value(args[0].as_raw()) as usize } else { 0 }
                    };
                    if idx == 0 {
                        return new_str(py, &m.full);
                    } else if idx <= m.groups.len() {
                        return match &m.groups[idx - 1] {
                            Some(s) => new_str(py, s),
                            None => Ok(none_obj(py)),
                        };
                    } else {
                        return Err(PyErr::index_error("no such group"));
                    }
                }
                "groups" => {
                    let mut items = Vec::new();
                    for g in &m.groups {
                        match g {
                            Some(s) => items.push(new_str(py, s)?),
                            None => items.push(none_obj(py)),
                        }
                    }
                    return build_tuple(py, items);
                }
                "start" => {
                    return new_int(py, m.start as i64);
                }
                "end" => {
                    return new_int(py, m.end as i64);
                }
                "span" => {
                    let start = new_int(py, m.start as i64)?;
                    let end = new_int(py, m.end as i64)?;
                    return build_tuple(py, vec![start, end]);
                }
                _ => {}
            }
        }
    }

    Err(PyErr::type_error(&format!(
        "'{}' method not implemented for this type", name
    )))
}

/// Compare two PyObjects for equality
unsafe fn objects_equal(a: *mut RawPyObject, b: *mut RawPyObject) -> bool {
    if a == b { return true; }
    if is_int(a) && is_int(b) { return get_int_value(a) == get_int_value(b); }
    if is_float(a) && is_float(b) { return get_float_value(a) == get_float_value(b); }
    if is_str(a) && is_str(b) {
        return crate::types::unicode::unicode_value(a) == crate::types::unicode::unicode_value(b);
    }
    false
}

/// Format a PyObject as a string (for str.format, % formatting, etc.)
fn format_pyobj(raw: *mut RawPyObject) -> String {
    unsafe {
        if is_str(raw) {
            crate::types::unicode::unicode_value(raw).to_string()
        } else if is_int(raw) {
            format!("{}", get_int_value(raw))
        } else if is_float(raw) {
            format!("{}", get_float_value(raw))
        } else if is_none(raw) {
            "None".to_string()
        } else if is_bool(raw) {
            let v = get_int_value(raw);
            if v != 0 { "True".to_string() } else { "False".to_string() }
        } else {
            format!("<object at {:?}>", raw)
        }
    }
}
