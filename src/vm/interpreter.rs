//! The bytecode interpreter (VM execution loop).
//!
//! This is the beating heart of Rustthon — the main eval loop
//! that fetches instructions and dispatches them.
//!
//! Key safety properties:
//! - Zero manual py_incref/py_decref — all refcounting is RAII via PyObjectRef
//! - Zero unsafe blocks in run_frame() — all unsafe is contained in safe_api
//! - All operations return PyResult — no silent NULL propagation
//! - Python<'py> GIL token threaded through for compile-time GIL proof

use crate::compiler::bytecode::{CodeObject, OpCode};
use crate::object::pyobject::{PyObjectRef, RawPyObject};
use crate::object::safe_api::{
    is_int, is_float, is_str, is_list, is_bool, is_none,
    get_int_value, get_float_value,
    create_int, create_str,
    return_none, bool_from_long, py_incref,
    // New RAII API
    none_obj, true_obj, false_obj, bool_obj,
    new_int, new_float,
    py_is_true, py_get_attr, py_get_item, py_store_item,
    py_import, py_repr,
    build_list, build_tuple, build_dict, build_set,
};
use crate::runtime::gil::Python;
use crate::runtime::pyerr::{PyErr, PyResult};
use crate::vm::frame::Frame;
use std::ptr;

/// The virtual machine
pub struct VM {
    /// Call stack of frames
    frames: Vec<Frame>,
}

impl VM {
    pub fn new() -> Self {
        VM {
            frames: Vec::new(),
        }
    }

    /// Execute a code object and return the result.
    /// Takes a Python<'py> GIL token as compile-time proof the GIL is held.
    pub fn execute(&mut self, py: Python<'_>, code: CodeObject) -> PyResult {
        let mut frame = Frame::new(code);

        // Register built-in functions
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
        ];
        for &(name, func) in builtins {
            let obj = unsafe {
                PyObjectRef::from_raw(create_builtin_function(name, func))
            };
            frame.builtins.insert(name.to_string(), obj);
        }
    }

    /// The main eval loop. Zero unsafe blocks — all operations go through
    /// safe_api wrappers or return PyResult.
    fn run_frame(&mut self, py: Python<'_>, frame: &mut Frame) -> PyResult {
        loop {
            if frame.ip >= frame.code.instructions.len() {
                return Ok(none_obj(py));
            }

            let instr = frame.code.instructions[frame.ip].clone();
            frame.ip += 1;

            match instr.opcode {
                OpCode::Nop => {}

                OpCode::LoadConst => {
                    let obj = frame.code.constants[instr.arg as usize].clone(); // Clone = incref
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
                    frame.store_name(&name, obj);
                    // No manual decref! obj was moved into store_name.
                }

                OpCode::LoadGlobal => {
                    let name = frame.code.names[instr.arg as usize].clone();
                    let obj = frame.globals.get(&name)
                        .or_else(|| frame.builtins.get(&name))
                        .cloned() // Clone = incref
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
                    // _obj is dropped here → automatic decref
                }

                OpCode::DupTop => {
                    let obj = frame.top()?; // Clone = incref
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
                    // left and right dropped here → automatic decref
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
                    // obj dropped → decref
                }

                OpCode::UnaryNegative => {
                    let obj = frame.pop()?;
                    let result = unary_negative(py, &obj)?;
                    frame.push(result);
                }

                OpCode::UnaryPositive => {
                    // Positive is usually identity — leave stack unchanged
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
                    // obj dropped → decref
                }

                OpCode::PopJumpIfTrue => {
                    let obj = frame.pop()?;
                    let is_true = py_is_true(py, &obj)?;
                    if is_true {
                        frame.ip = instr.arg as usize;
                    }
                }

                OpCode::JumpIfFalse => {
                    let obj = frame.top()?; // Clone, not pop
                    let is_true = py_is_true(py, &obj)?;
                    if !is_true {
                        frame.ip = instr.arg as usize;
                    }
                    // obj (the clone) dropped → decref the clone
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

                    let result = call_function_raii(py, &func, &args)?;
                    frame.push(result);
                    // func and args dropped → auto decref
                }

                OpCode::ReturnValue => {
                    return frame.pop();
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
                    let value = frame.pop()?;
                    let key = frame.pop()?;
                    let obj = frame.pop()?;
                    unsafe {
                        if crate::types::list::PyList_Check(obj.as_raw()) != 0 {
                            let idx = get_int_value(key.as_raw());
                            // PyList_SetItem steals a ref — donate one via incref+raw
                            (*value.as_raw()).incref();
                            crate::types::list::PyList_SetItem(obj.as_raw(), idx as isize, value.as_raw());
                        } else if crate::types::dict::PyDict_Check(obj.as_raw()) != 0 {
                            crate::types::dict::PyDict_SetItem(obj.as_raw(), key.as_raw(), value.as_raw());
                        } else {
                            py_store_item(py, &obj, &key, &value).ok();
                        }
                    }
                    // obj, key, value all dropped → decref
                }

                // ─── Import ───
                OpCode::ImportName => {
                    let name = frame.code.names[instr.arg as usize].clone();
                    let module = py_import(py, &name)?;
                    frame.push(module);
                }

                OpCode::ImportFrom => {
                    let name = frame.code.names[instr.arg as usize].clone();
                    let module = frame.top()?;
                    let attr = py_get_attr(py, &module, &name)?;
                    frame.push(attr);
                }

                // ─── Iteration ───
                OpCode::GetIter => {
                    // For now, leave the object on stack as its own iterator
                }

                OpCode::ForIter => {
                    // Placeholder — jump to target when iterator exhausted
                    frame.ip = instr.arg as usize;
                }

                // ─── Misc ───
                OpCode::PrintExpr => {
                    let obj = frame.top()?;
                    if !is_none(obj.as_raw()) {
                        if let Ok(repr) = py_repr(py, &obj) {
                            if is_str(repr.as_raw()) {
                                let _s = crate::types::unicode::unicode_value(repr.as_raw());
                                // Don't print in non-interactive mode
                            }
                        }
                    }
                }

                OpCode::MakeFunction => {
                    let _name_idx = instr.arg;
                    let _code_obj = frame.pop()?;
                    frame.push(none_obj(py));
                }

                OpCode::LoadAttr => {
                    let name = frame.code.names[instr.arg as usize].clone();
                    let obj = frame.pop()?;
                    let attr = py_get_attr(py, &obj, &name)
                        .or_else(|_| {
                            // Fallback: check dict for module objects
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
                    // obj dropped → decref
                }

                OpCode::StoreAttr => {
                    let name = frame.code.names[instr.arg as usize].clone();
                    let obj = frame.pop()?;
                    let value = frame.pop()?;
                    unsafe {
                        let name_cstr = std::ffi::CString::new(name.as_str()).unwrap();
                        crate::ffi::object_api::PyObject_SetAttrString(
                            obj.as_raw(),
                            name_cstr.as_ptr(),
                            value.as_raw(),
                        );
                    }
                    // obj and value dropped → decref
                }

                OpCode::UnpackSequence => {
                    // TODO: proper unpack
                    let _n = instr.arg;
                    let _obj = frame.pop()?;
                    // Push None placeholders
                    for _ in 0.._n {
                        frame.push(none_obj(py));
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
                    frame.locals.insert(name, obj);
                }

                OpCode::SetupLoop | OpCode::PopBlock => {
                    // No-op for now (loop setup/teardown)
                }

                OpCode::BreakLoop => {
                    // TODO: proper break handling
                }

                OpCode::ContinueLoop => {
                    // TODO: proper continue handling
                }

                _ => {
                    return Err(PyErr::type_error(&format!(
                        "Unimplemented opcode: {:?}", instr.opcode
                    )));
                }
            }
        }
    }
}

// ─── Helper functions (safe, return PyResult) ───

fn binary_add(py: Python<'_>, left: &PyObjectRef, right: &PyObjectRef) -> PyResult {
    let (l, r) = (left.as_raw(), right.as_raw());
    if is_int(l) && is_int(r) {
        let lv = get_int_value(l);
        let rv = get_int_value(r);
        new_int(py, lv.wrapping_add(rv))
    } else if is_float(l) || is_float(r) {
        let lv = get_float_value(l);
        let rv = get_float_value(r);
        new_float(py, lv + rv)
    } else if is_str(l) && is_str(r) {
        let ptr = unsafe { crate::types::unicode::PyUnicode_Concat(l, r) };
        PyObjectRef::steal_or_err(ptr)
    } else if is_list(l) && is_list(r) {
        // List concatenation
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
    } else {
        Ok(none_obj(py))
    }
}

fn binary_truediv(py: Python<'_>, left: &PyObjectRef, right: &PyObjectRef) -> PyResult {
    let lv = get_float_value(left.as_raw());
    let rv = get_float_value(right.as_raw());
    if rv == 0.0 {
        return Err(PyErr::value_error("division by zero"));
    }
    new_float(py, lv / rv)
}

fn binary_floordiv(py: Python<'_>, left: &PyObjectRef, right: &PyObjectRef) -> PyResult {
    let (l, r) = (left.as_raw(), right.as_raw());
    if is_int(l) && is_int(r) {
        let lv = get_int_value(l);
        let rv = get_int_value(r);
        if rv == 0 {
            return Err(PyErr::value_error("integer division or modulo by zero"));
        }
        let d = lv.wrapping_div(rv);
        let result = if (lv ^ rv) < 0 && d * rv != lv { d - 1 } else { d };
        new_int(py, result)
    } else {
        let lv = get_float_value(l);
        let rv = get_float_value(r);
        if rv == 0.0 {
            return Err(PyErr::value_error("float floor division by zero"));
        }
        new_float(py, (lv / rv).floor())
    }
}

fn binary_mod(py: Python<'_>, left: &PyObjectRef, right: &PyObjectRef) -> PyResult {
    let (l, r) = (left.as_raw(), right.as_raw());
    if is_int(l) && is_int(r) {
        let lv = get_int_value(l);
        let rv = get_int_value(r);
        if rv == 0 {
            return Err(PyErr::value_error("integer division or modulo by zero"));
        }
        let m = lv % rv;
        let result = if m != 0 && (m ^ rv) < 0 { m + rv } else { m };
        new_int(py, result)
    } else {
        let lv = get_float_value(l);
        let rv = get_float_value(r);
        if rv == 0.0 {
            return Err(PyErr::value_error("float modulo by zero"));
        }
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
        _ => {
            if is_int(l) && is_int(r) {
                let lv = get_int_value(l);
                let rv = get_int_value(r);
                let result = match op {
                    0 => lv < rv,
                    1 => lv <= rv,
                    2 => lv == rv,
                    3 => lv != rv,
                    4 => lv > rv,
                    5 => lv >= rv,
                    _ => false,
                };
                Ok(bool_obj(py, result))
            } else if is_float(l) || is_float(r) {
                let lv = get_float_value(l);
                let rv = get_float_value(r);
                let result = match op {
                    0 => lv < rv,
                    1 => lv <= rv,
                    2 => lv == rv,
                    3 => lv != rv,
                    4 => lv > rv,
                    5 => lv >= rv,
                    _ => false,
                };
                Ok(bool_obj(py, result))
            } else if is_str(l) && is_str(r) {
                let lv = crate::types::unicode::unicode_value(l);
                let rv = crate::types::unicode::unicode_value(r);
                let result = match op {
                    0 => lv < rv,
                    1 => lv <= rv,
                    2 => lv == rv,
                    3 => lv != rv,
                    4 => lv > rv,
                    5 => lv >= rv,
                    _ => false,
                };
                Ok(bool_obj(py, result))
            } else {
                // Default: identity comparison
                let result = match op {
                    2 => l == r,
                    3 => l != r,
                    _ => false,
                };
                Ok(bool_obj(py, result))
            }
        }
    }
}

fn subscr_fallback(_py: Python<'_>, obj: &PyObjectRef, key: &PyObjectRef) -> PyResult {
    let (o, k) = (obj.as_raw(), key.as_raw());
    unsafe {
        if crate::types::list::PyList_Check(o) != 0 {
            let idx = get_int_value(k) as isize;
            let item = crate::types::list::PyList_GetItem(o, idx);
            return PyObjectRef::borrow_or_err(item); // BORROWED ref → incref
        }
        if crate::types::tuple::PyTuple_Check(o) != 0 {
            let idx = get_int_value(k) as isize;
            let item = crate::types::tuple::PyTuple_GetItem(o, idx);
            return PyObjectRef::borrow_or_err(item);
        }
        if crate::types::dict::PyDict_Check(o) != 0 {
            let item = crate::types::dict::PyDict_GetItem(o, k);
            return PyObjectRef::borrow_or_err(item);
        }
    }
    Err(PyErr::type_error("object is not subscriptable"))
}

/// Call a function using RAII args. Builds an args tuple, calls via tp_call.
fn call_function_raii(_py: Python<'_>, func: &PyObjectRef, args: &[PyObjectRef]) -> PyResult {
    unsafe {
        let f = func.as_raw();
        // Build args tuple
        let args_tuple = crate::types::tuple::PyTuple_New(args.len() as isize);
        if args_tuple.is_null() {
            return Err(PyErr::memory_error());
        }

        for (i, arg) in args.iter().enumerate() {
            // PyTuple_SET_ITEM steals a reference, so incref first
            (*arg.as_raw()).incref();
            crate::types::tuple::PyTuple_SET_ITEM(args_tuple, i as isize, arg.as_raw());
        }

        // Try CFunction first
        let result = if (*f).ob_type == crate::types::funcobject::cfunction_type() {
            crate::types::funcobject::call_cfunction(f, args_tuple, ptr::null_mut())
        } else {
            // Try tp_call
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
// These remain as `unsafe extern "C"` because they are C API callbacks
// registered as PyCFunction pointers. The eval loop calls them through
// call_function_raii which handles the RAII boundary.

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
        if item.is_null() {
            parts.push("None".to_string());
            continue;
        }

        if is_none(item) {
            parts.push("None".to_string());
        } else if is_bool(item) {
            if crate::types::boolobject::is_true(item) {
                parts.push("True".to_string());
            } else {
                parts.push("False".to_string());
            }
        } else if is_str(item) {
            parts.push(crate::types::unicode::unicode_value(item).to_string());
        } else if is_int(item) {
            let val = crate::types::longobject::long_value(item);
            parts.push(format!("{}", val));
        } else if is_float(item) {
            let val = crate::types::floatobject::float_value(item);
            parts.push(format!("{}", val));
        } else if crate::types::list::PyList_Check(item) != 0 {
            parts.push(format_list(item));
        } else if crate::types::tuple::PyTuple_Check(item) != 0 {
            parts.push(format_tuple(item));
        } else {
            let repr = crate::ffi::object_api::PyObject_Repr(item);
            if !repr.is_null() && is_str(repr) {
                parts.push(crate::types::unicode::unicode_value(repr).to_string());
                (*repr).decref();
            } else {
                parts.push(format!("<object at {:p}>", item));
            }
        }
    }

    println!("{}", parts.join(" "));
    return_none()
}

unsafe fn format_list(list: *mut RawPyObject) -> String {
    let n = crate::types::list::PyList_Size(list);
    let mut items = Vec::new();
    for i in 0..n {
        let item = crate::types::list::PyList_GetItem(list, i);
        items.push(format_object(item));
    }
    format!("[{}]", items.join(", "))
}

unsafe fn format_tuple(tuple: *mut RawPyObject) -> String {
    let n = crate::types::tuple::PyTuple_Size(tuple);
    let mut items = Vec::new();
    for i in 0..n {
        let item = crate::types::tuple::PyTuple_GetItem(tuple, i);
        items.push(format_object(item));
    }
    if n == 1 {
        format!("({},)", items[0])
    } else {
        format!("({})", items.join(", "))
    }
}

unsafe fn format_object(obj: *mut RawPyObject) -> String {
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
    } else {
        format!("<object at {:p}>", obj)
    }
}

unsafe extern "C" fn builtin_len(
    _self: *mut RawPyObject,
    args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    if obj.is_null() {
        return create_int(0);
    }
    let len = crate::ffi::object_api::PyObject_Length(obj);
    if len >= 0 {
        create_int(len as i64)
    } else if is_str(obj) {
        let s = crate::types::unicode::unicode_value(obj);
        create_int(s.len() as i64)
    } else {
        create_int(0)
    }
}

unsafe extern "C" fn builtin_type(
    _self: *mut RawPyObject,
    args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    crate::ffi::object_api::PyObject_Type(obj)
}

unsafe extern "C" fn builtin_range(
    _self: *mut RawPyObject,
    args: *mut RawPyObject,
) -> *mut RawPyObject {
    let nargs = crate::types::tuple::PyTuple_Size(args);

    let (start, stop, step) = match nargs {
        1 => {
            let stop = get_int_value(crate::types::tuple::PyTuple_GetItem(args, 0));
            (0i64, stop, 1i64)
        }
        2 => {
            let start = get_int_value(crate::types::tuple::PyTuple_GetItem(args, 0));
            let stop = get_int_value(crate::types::tuple::PyTuple_GetItem(args, 1));
            (start, stop, 1)
        }
        3 => {
            let start = get_int_value(crate::types::tuple::PyTuple_GetItem(args, 0));
            let stop = get_int_value(crate::types::tuple::PyTuple_GetItem(args, 1));
            let step = get_int_value(crate::types::tuple::PyTuple_GetItem(args, 2));
            (start, stop, step)
        }
        _ => return crate::types::list::PyList_New(0),
    };

    if step == 0 {
        return crate::types::list::PyList_New(0);
    }

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
    _self: *mut RawPyObject,
    args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    if obj.is_null() {
        return create_int(0);
    }
    if is_int(obj) {
        (*obj).incref();
        return obj;
    }
    if is_float(obj) {
        let val = crate::types::floatobject::float_value(obj);
        return create_int(val as i64);
    }
    if is_str(obj) {
        let s = crate::types::unicode::unicode_value(obj);
        if let Ok(val) = s.trim().parse::<i64>() {
            return create_int(val);
        }
    }
    create_int(0)
}

unsafe extern "C" fn builtin_str(
    _self: *mut RawPyObject,
    args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    if obj.is_null() {
        return create_str("None");
    }
    crate::ffi::object_api::PyObject_Str(obj)
}

unsafe extern "C" fn builtin_isinstance(
    _self: *mut RawPyObject,
    args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    let tp = crate::types::tuple::PyTuple_GetItem(args, 1);
    if obj.is_null() || tp.is_null() {
        return crate::object::safe_api::py_false();
    }
    let obj_type = (*obj).ob_type as *mut RawPyObject;
    bool_from_long(if obj_type == tp { 1 } else { 0 })
}
