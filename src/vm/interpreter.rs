//! The bytecode interpreter (VM execution loop).
//!
//! This is the beating heart of Rustthon — the main eval loop
//! that fetches instructions and dispatches them.

use crate::compiler::bytecode::{CodeObject, OpCode};
use crate::object::pyobject::RawPyObject;
use crate::object::safe_api::{
    py_incref, py_decref, py_true, py_false,
    is_int, is_float, is_str, is_list, is_bool, is_none,
    get_int_value, get_float_value,
    create_int, create_float, create_str,
    return_none, bool_from_long,
};
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
    pub fn execute(&mut self, code: CodeObject) -> Result<*mut RawPyObject, String> {
        let mut frame = Frame::new(code);

        // Register built-in functions
        self.register_builtins(&mut frame);

        self.run_frame(&mut frame)
    }

    fn register_builtins(&self, frame: &mut Frame) {
        unsafe {
            // print function
            frame.builtins.insert(
                "print".to_string(),
                create_builtin_function("print", builtin_print),
            );
            // len function
            frame.builtins.insert(
                "len".to_string(),
                create_builtin_function("len", builtin_len),
            );
            // type function
            frame.builtins.insert(
                "type".to_string(),
                create_builtin_function("type", builtin_type),
            );
            // range function
            frame.builtins.insert(
                "range".to_string(),
                create_builtin_function("range", builtin_range),
            );
            // int function
            frame.builtins.insert(
                "int".to_string(),
                create_builtin_function("int", builtin_int),
            );
            // str function
            frame.builtins.insert(
                "str".to_string(),
                create_builtin_function("str", builtin_str),
            );
            // isinstance
            frame.builtins.insert(
                "isinstance".to_string(),
                create_builtin_function("isinstance", builtin_isinstance),
            );
        }
    }

    fn run_frame(&mut self, frame: &mut Frame) -> Result<*mut RawPyObject, String> {
        loop {
            if frame.ip >= frame.code.instructions.len() {
                // End of code
                return Ok(crate::types::none::return_none());
            }

            let instr = frame.code.instructions[frame.ip].clone();
            frame.ip += 1;

            match instr.opcode {
                OpCode::Nop => {}

                OpCode::LoadConst => {
                    let obj = frame.code.constants[instr.arg as usize];
                    py_incref(obj);
                    frame.push(obj);
                }

                OpCode::LoadName => {
                    let name = &frame.code.names[instr.arg as usize].clone();
                    let obj = frame.lookup_name(name);
                    if obj.is_null() {
                        return Err(format!("NameError: name '{}' is not defined", name));
                    }
                    py_incref(obj);
                    frame.push(obj);
                }

                OpCode::StoreName => {
                    let name = frame.code.names[instr.arg as usize].clone();
                    let obj = frame.pop();
                    frame.store_name(&name, obj);
                    // store_name increfs, so we can decref our copy
                    py_decref(obj);
                }

                OpCode::LoadGlobal => {
                    let name = &frame.code.names[instr.arg as usize].clone();
                    let obj = if let Some(&g) = frame.globals.get(name.as_str()) {
                        g
                    } else if let Some(&b) = frame.builtins.get(name.as_str()) {
                        b
                    } else {
                        return Err(format!("NameError: name '{}' is not defined", name));
                    };
                    py_incref(obj);
                    frame.push(obj);
                }

                OpCode::StoreGlobal => {
                    let name = frame.code.names[instr.arg as usize].clone();
                    let obj = frame.pop();
                    py_incref(obj);
                    frame.globals.insert(name, obj);
                }

                OpCode::PopTop => {
                    let obj = frame.pop();
                    py_decref(obj);
                }

                OpCode::DupTop => {
                    let obj = frame.top();
                    py_incref(obj);
                    frame.push(obj);
                }

                OpCode::RotTwo => {
                    let a = frame.pop();
                    let b = frame.pop();
                    frame.push(a);
                    frame.push(b);
                }

                OpCode::RotThree => {
                    let a = frame.pop();
                    let b = frame.pop();
                    let c = frame.pop();
                    frame.push(a);
                    frame.push(c);
                    frame.push(b);
                }

                // ─── Binary operations ───
                OpCode::BinaryAdd => {
                    let right = frame.pop();
                    let left = frame.pop();
                    let result = binary_add(left, right);
                    frame.push(result);
                    py_decref(left);
                    py_decref(right);
                }

                OpCode::BinarySubtract => {
                    let right = frame.pop();
                    let left = frame.pop();
                    let result = binary_sub(left, right);
                    frame.push(result);
                    py_decref(left);
                    py_decref(right);
                }

                OpCode::BinaryMultiply => {
                    let right = frame.pop();
                    let left = frame.pop();
                    let result = binary_mul(left, right);
                    frame.push(result);
                    py_decref(left);
                    py_decref(right);
                }

                OpCode::BinaryTrueDivide => {
                    let right = frame.pop();
                    let left = frame.pop();
                    let result = binary_truediv(left, right);
                    frame.push(result);
                    py_decref(left);
                    py_decref(right);
                }

                OpCode::BinaryFloorDivide => {
                    let right = frame.pop();
                    let left = frame.pop();
                    let result = binary_floordiv(left, right);
                    frame.push(result);
                    py_decref(left);
                    py_decref(right);
                }

                OpCode::BinaryModulo => {
                    let right = frame.pop();
                    let left = frame.pop();
                    let result = binary_mod(left, right);
                    frame.push(result);
                    py_decref(left);
                    py_decref(right);
                }

                OpCode::BinaryPower => {
                    let right = frame.pop();
                    let left = frame.pop();
                    let result = binary_pow(left, right);
                    frame.push(result);
                    py_decref(left);
                    py_decref(right);
                }

                OpCode::BinaryAnd | OpCode::BinaryOr | OpCode::BinaryXor |
                OpCode::BinaryLShift | OpCode::BinaryRShift => {
                    let right = frame.pop();
                    let left = frame.pop();
                    let result = binary_bitop(left, right, instr.opcode);
                    frame.push(result);
                    py_decref(left);
                    py_decref(right);
                }

                OpCode::InplaceAdd => {
                    let right = frame.pop();
                    let left = frame.pop();
                    let result = binary_add(left, right);
                    frame.push(result);
                    py_decref(left);
                    py_decref(right);
                }

                OpCode::InplaceSubtract => {
                    let right = frame.pop();
                    let left = frame.pop();
                    let result = binary_sub(left, right);
                    frame.push(result);
                    py_decref(left);
                    py_decref(right);
                }

                OpCode::InplaceMultiply => {
                    let right = frame.pop();
                    let left = frame.pop();
                    let result = binary_mul(left, right);
                    frame.push(result);
                    py_decref(left);
                    py_decref(right);
                }

                OpCode::BinarySubscr => {
                    let key = frame.pop();
                    let obj = frame.pop();
                    let result = unsafe {
                        crate::ffi::object_api::PyObject_GetItem(obj, key)
                    };
                    frame.push(if result.is_null() {
                        // Fallback for list/tuple index
                        unsafe { subscr_fallback(obj, key) }
                    } else {
                        result
                    });
                    py_decref(obj);
                    py_decref(key);
                }

                // ─── Comparison ───
                OpCode::CompareOp => {
                    let right = frame.pop();
                    let left = frame.pop();
                    let result = compare_op(left, right, instr.arg);
                    frame.push(result);
                    py_decref(left);
                    py_decref(right);
                }

                // ─── Unary ───
                OpCode::UnaryNot => {
                    let obj = frame.pop();
                    let is_true = unsafe {
                        crate::ffi::object_api::PyObject_IsTrue(obj)
                    };
                    let result = if is_true != 0 { py_false() } else { py_true() };
                    py_incref(result);
                    py_decref(obj);
                    frame.push(result);
                }

                OpCode::UnaryNegative => {
                    let obj = frame.pop();
                    let result = unary_negative(obj);
                    frame.push(result);
                    py_decref(obj);
                }

                OpCode::UnaryPositive => {
                    // Positive is usually identity
                    // (don't decref — we're keeping it)
                }

                // ─── Jumps ───
                OpCode::JumpAbsolute => {
                    frame.ip = instr.arg as usize;
                }

                OpCode::PopJumpIfFalse => {
                    let obj = frame.pop();
                    let is_true = unsafe {
                        crate::ffi::object_api::PyObject_IsTrue(obj)
                    };
                    py_decref(obj);
                    if is_true == 0 {
                        frame.ip = instr.arg as usize;
                    }
                }

                OpCode::PopJumpIfTrue => {
                    let obj = frame.pop();
                    let is_true = unsafe {
                        crate::ffi::object_api::PyObject_IsTrue(obj)
                    };
                    py_decref(obj);
                    if is_true != 0 {
                        frame.ip = instr.arg as usize;
                    }
                }

                OpCode::JumpIfFalse => {
                    let obj = frame.top();
                    let is_true = unsafe {
                        crate::ffi::object_api::PyObject_IsTrue(obj)
                    };
                    if is_true == 0 {
                        frame.ip = instr.arg as usize;
                    }
                }

                OpCode::JumpIfTrue => {
                    let obj = frame.top();
                    let is_true = unsafe {
                        crate::ffi::object_api::PyObject_IsTrue(obj)
                    };
                    if is_true != 0 {
                        frame.ip = instr.arg as usize;
                    }
                }

                // ─── Function calls ───
                OpCode::CallFunction => {
                    let nargs = instr.arg as usize;
                    let mut args = Vec::with_capacity(nargs);
                    for _ in 0..nargs {
                        args.push(frame.pop());
                    }
                    args.reverse();
                    let func = frame.pop();

                    let result = unsafe { call_function(func, &args) };
                    frame.push(result);

                    py_decref(func);
                    for &arg in &args {
                        py_decref(arg);
                    }
                }

                OpCode::ReturnValue => {
                    let retval = frame.pop();
                    return Ok(retval);
                }

                // ─── Container building ───
                OpCode::BuildList => {
                    let n = instr.arg as usize;
                    unsafe {
                        let list = crate::types::list::PyList_New(n as isize);
                        for i in (0..n).rev() {
                            let item = frame.pop();
                            crate::types::list::PyList_SET_ITEM(list, i as isize, item);
                            // SET_ITEM steals reference
                        }
                        frame.push(list);
                    }
                }

                OpCode::BuildTuple => {
                    let n = instr.arg as usize;
                    unsafe {
                        let tuple = crate::types::tuple::PyTuple_New(n as isize);
                        for i in (0..n).rev() {
                            let item = frame.pop();
                            crate::types::tuple::PyTuple_SET_ITEM(tuple, i as isize, item);
                        }
                        frame.push(tuple);
                    }
                }

                OpCode::BuildMap => {
                    let n = instr.arg as usize;
                    unsafe {
                        let dict = crate::types::dict::PyDict_New();
                        // Items are on stack as key, value, key, value, ...
                        let mut pairs = Vec::with_capacity(n);
                        for _ in 0..n {
                            let value = frame.pop();
                            let key = frame.pop();
                            pairs.push((key, value));
                        }
                        pairs.reverse();
                        for (key, value) in pairs {
                            crate::types::dict::PyDict_SetItem(dict, key, value);
                            (*key).decref();
                            (*value).decref();
                        }
                        frame.push(dict);
                    }
                }

                OpCode::BuildSet => {
                    let n = instr.arg as usize;
                    unsafe {
                        let set = crate::types::set::PySet_New(ptr::null_mut());
                        for _ in 0..n {
                            let item = frame.pop();
                            crate::types::set::PySet_Add(set, item);
                            (*item).decref();
                        }
                        frame.push(set);
                    }
                }

                OpCode::StoreSubscr => {
                    let value = frame.pop();
                    let key = frame.pop();
                    let obj = frame.pop();
                    unsafe {
                        // Try list
                        if crate::types::list::PyList_Check(obj) != 0 {
                            let idx = get_int_value(key);
                            (*value).incref();
                            crate::types::list::PyList_SetItem(obj, idx as isize, value);
                        } else if crate::types::dict::PyDict_Check(obj) != 0 {
                            crate::types::dict::PyDict_SetItem(obj, key, value);
                        }
                    }
                    py_decref(obj);
                    py_decref(key);
                    py_decref(value);
                }

                // ─── Import ───
                OpCode::ImportName => {
                    let name = frame.code.names[instr.arg as usize].clone();
                    let module = unsafe {
                        let name_cstr = std::ffi::CString::new(name.as_str()).unwrap();
                        crate::ffi::import::PyImport_ImportModule(name_cstr.as_ptr())
                    };
                    if module.is_null() {
                        return Err(format!("ModuleNotFoundError: No module named '{}'", name));
                    }
                    frame.push(module);
                }

                OpCode::ImportFrom => {
                    let name = frame.code.names[instr.arg as usize].clone();
                    let module = frame.top();
                    unsafe {
                        let name_cstr = std::ffi::CString::new(name.as_str()).unwrap();
                        let attr = crate::ffi::object_api::PyObject_GetAttrString(
                            module,
                            name_cstr.as_ptr(),
                        );
                        frame.push(attr);
                    }
                }

                // ─── Iteration ───
                OpCode::GetIter => {
                    // For now, leave the object on stack as its own iterator
                    // A full implementation would call tp_iter
                }

                OpCode::ForIter => {
                    // For simplified iteration, we need to handle list/tuple iteration
                    // This is a placeholder — real implementation needs iterator protocol
                    let _target = instr.arg;
                    // Jump to target when iterator exhausted
                    frame.ip = instr.arg as usize;
                }

                // ─── Misc ───
                OpCode::PrintExpr => {
                    // In interactive mode, print the expression result
                    let obj = frame.top();
                    if !obj.is_null() && !is_none(obj) {
                        unsafe {
                            let repr = crate::ffi::object_api::PyObject_Repr(obj);
                            if !repr.is_null() {
                                let _s = crate::types::unicode::unicode_value(repr);
                                // Don't print in non-interactive mode
                                (*repr).decref();
                            }
                        }
                    }
                }

                OpCode::MakeFunction => {
                    // Simplified: just store the function name
                    let _name_idx = instr.arg;
                    // Pop the code object placeholder
                    let _code_obj = frame.pop();
                    // Push a placeholder function
                    let none = return_none();
                    frame.push(none);
                }

                OpCode::LoadAttr => {
                    let name = frame.code.names[instr.arg as usize].clone();
                    let obj = frame.pop();
                    unsafe {
                        let name_cstr = std::ffi::CString::new(name.as_str()).unwrap();
                        let attr = crate::ffi::object_api::PyObject_GetAttrString(
                            obj,
                            name_cstr.as_ptr(),
                        );
                        frame.push(if attr.is_null() {
                            // Check dict for module objects
                            if crate::types::dict::PyDict_Check(obj) != 0 {
                                crate::types::dict::PyDict_GetItemString(obj, name_cstr.as_ptr())
                            } else {
                                ptr::null_mut()
                            }
                        } else {
                            attr
                        });
                    }
                    py_decref(obj);
                }

                OpCode::StoreAttr => {
                    let name = frame.code.names[instr.arg as usize].clone();
                    let obj = frame.pop();
                    let value = frame.pop();
                    unsafe {
                        let name_cstr = std::ffi::CString::new(name.as_str()).unwrap();
                        crate::ffi::object_api::PyObject_SetAttrString(
                            obj,
                            name_cstr.as_ptr(),
                            value,
                        );
                    }
                    py_decref(obj);
                    py_decref(value);
                }

                _ => {
                    // Unhandled opcode
                    return Err(format!("Unimplemented opcode: {:?}", instr.opcode));
                }
            }
        }
    }
}

// ─── Helper functions (now safe where possible) ───

fn binary_add(left: *mut RawPyObject, right: *mut RawPyObject) -> *mut RawPyObject {
    if is_int(left) && is_int(right) {
        let l = get_int_value(left);
        let r = get_int_value(right);
        create_int(l.wrapping_add(r))
    } else if is_float(left) || is_float(right) {
        let l = get_float_value(left);
        let r = get_float_value(right);
        create_float(l + r)
    } else if is_str(left) && is_str(right) {
        unsafe { crate::types::unicode::PyUnicode_Concat(left, right) }
    } else if is_list(left) && is_list(right) {
        // List concatenation via C API
        unsafe {
            let l_size = crate::types::list::PyList_Size(left);
            let r_size = crate::types::list::PyList_Size(right);
            let total = l_size + r_size;
            let new_list = crate::types::list::PyList_New(total);
            for i in 0..l_size {
                let item = crate::types::list::PyList_GetItem(left, i);
                py_incref(item);
                crate::types::list::PyList_SET_ITEM(new_list, i, item);
            }
            for i in 0..r_size {
                let item = crate::types::list::PyList_GetItem(right, i);
                py_incref(item);
                crate::types::list::PyList_SET_ITEM(new_list, l_size + i, item);
            }
            new_list
        }
    } else {
        return_none()
    }
}

fn binary_sub(left: *mut RawPyObject, right: *mut RawPyObject) -> *mut RawPyObject {
    if is_int(left) && is_int(right) {
        let l = get_int_value(left);
        let r = get_int_value(right);
        create_int(l.wrapping_sub(r))
    } else if is_float(left) || is_float(right) {
        let l = get_float_value(left);
        let r = get_float_value(right);
        create_float(l - r)
    } else {
        return_none()
    }
}

fn binary_mul(left: *mut RawPyObject, right: *mut RawPyObject) -> *mut RawPyObject {
    if is_int(left) && is_int(right) {
        let l = get_int_value(left);
        let r = get_int_value(right);
        create_int(l.wrapping_mul(r))
    } else if is_float(left) || is_float(right) {
        let l = get_float_value(left);
        let r = get_float_value(right);
        create_float(l * r)
    } else {
        return_none()
    }
}

fn binary_truediv(left: *mut RawPyObject, right: *mut RawPyObject) -> *mut RawPyObject {
    let l = get_float_value(left);
    let r = get_float_value(right);
    if r == 0.0 {
        // TODO: ZeroDivisionError
        return return_none();
    }
    create_float(l / r)
}

fn binary_floordiv(left: *mut RawPyObject, right: *mut RawPyObject) -> *mut RawPyObject {
    if is_int(left) && is_int(right) {
        let l = get_int_value(left);
        let r = get_int_value(right);
        if r == 0 {
            return return_none();
        }
        // Python floor division: rounds toward negative infinity
        let d = l.wrapping_div(r);
        let result = if (l ^ r) < 0 && d * r != l { d - 1 } else { d };
        create_int(result)
    } else {
        let l = get_float_value(left);
        let r = get_float_value(right);
        if r == 0.0 {
            return return_none();
        }
        create_float((l / r).floor())
    }
}

fn binary_mod(left: *mut RawPyObject, right: *mut RawPyObject) -> *mut RawPyObject {
    if is_int(left) && is_int(right) {
        let l = get_int_value(left);
        let r = get_int_value(right);
        if r == 0 {
            return return_none();
        }
        // Python modulo: result has same sign as divisor
        let m = l % r;
        let result = if m != 0 && (m ^ r) < 0 { m + r } else { m };
        create_int(result)
    } else {
        let l = get_float_value(left);
        let r = get_float_value(right);
        if r == 0.0 {
            return return_none();
        }
        create_float(l % r)
    }
}

fn binary_pow(left: *mut RawPyObject, right: *mut RawPyObject) -> *mut RawPyObject {
    if is_int(left) && is_int(right) {
        let l = get_int_value(left);
        let r = get_int_value(right);
        if r >= 0 && r <= 63 {
            let result = l.wrapping_pow(r as u32);
            create_int(result)
        } else {
            create_float((l as f64).powf(r as f64))
        }
    } else {
        let l = get_float_value(left);
        let r = get_float_value(right);
        create_float(l.powf(r))
    }
}

fn binary_bitop(left: *mut RawPyObject, right: *mut RawPyObject, op: OpCode) -> *mut RawPyObject {
    let l = get_int_value(left);
    let r = get_int_value(right);
    let result = match op {
        OpCode::BinaryAnd => l & r,
        OpCode::BinaryOr => l | r,
        OpCode::BinaryXor => l ^ r,
        OpCode::BinaryLShift => l.wrapping_shl(r as u32),
        OpCode::BinaryRShift => l.wrapping_shr(r as u32),
        _ => 0,
    };
    create_int(result)
}

fn unary_negative(obj: *mut RawPyObject) -> *mut RawPyObject {
    if is_int(obj) {
        let val = get_int_value(obj);
        create_int(val.wrapping_neg())
    } else if is_float(obj) {
        let val = get_float_value(obj);
        create_float(-val)
    } else {
        return_none()
    }
}

fn compare_op(left: *mut RawPyObject, right: *mut RawPyObject, op: u32) -> *mut RawPyObject {
    match op {
        6 => {
            // is
            bool_from_long(if left == right { 1 } else { 0 })
        }
        7 => {
            // is not
            bool_from_long(if left != right { 1 } else { 0 })
        }
        _ => {
            // Numeric comparison
            if is_int(left) && is_int(right) {
                let l = get_int_value(left);
                let r = get_int_value(right);
                let result = match op {
                    0 => l < r,  // <
                    1 => l <= r, // <=
                    2 => l == r, // ==
                    3 => l != r, // !=
                    4 => l > r,  // >
                    5 => l >= r, // >=
                    _ => false,
                };
                bool_from_long(if result { 1 } else { 0 })
            } else if is_float(left) || is_float(right) {
                let l = get_float_value(left);
                let r = get_float_value(right);
                let result = match op {
                    0 => l < r,
                    1 => l <= r,
                    2 => l == r,
                    3 => l != r,
                    4 => l > r,
                    5 => l >= r,
                    _ => false,
                };
                bool_from_long(if result { 1 } else { 0 })
            } else if is_str(left) && is_str(right) {
                let l = crate::types::unicode::unicode_value(left);
                let r = crate::types::unicode::unicode_value(right);
                let result = match op {
                    0 => l < r,
                    1 => l <= r,
                    2 => l == r,
                    3 => l != r,
                    4 => l > r,
                    5 => l >= r,
                    _ => false,
                };
                bool_from_long(if result { 1 } else { 0 })
            } else {
                // Default: identity comparison
                bool_from_long(
                    if op == 2 { if left == right { 1 } else { 0 } }
                    else if op == 3 { if left != right { 1 } else { 0 } }
                    else { 0 }
                )
            }
        }
    }
}

unsafe fn subscr_fallback(obj: *mut RawPyObject, key: *mut RawPyObject) -> *mut RawPyObject {
    if crate::types::list::PyList_Check(obj) != 0 {
        let idx = get_int_value(key) as isize;
        let item = crate::types::list::PyList_GetItem(obj, idx);
        py_incref(item);
        return item;
    }
    if crate::types::tuple::PyTuple_Check(obj) != 0 {
        let idx = get_int_value(key) as isize;
        let item = crate::types::tuple::PyTuple_GetItem(obj, idx);
        py_incref(item);
        return item;
    }
    if crate::types::dict::PyDict_Check(obj) != 0 {
        let item = crate::types::dict::PyDict_GetItem(obj, key);
        py_incref(item);
        return item;
    }
    ptr::null_mut()
}

unsafe fn call_function(func: *mut RawPyObject, args: &[*mut RawPyObject]) -> *mut RawPyObject {
    if func.is_null() {
        return return_none();
    }

    // Check if it's a built-in CFunction
    if (*func).ob_type == crate::types::funcobject::cfunction_type() {
        // Build args tuple
        let args_tuple = crate::types::tuple::PyTuple_New(args.len() as isize);
        for (i, &arg) in args.iter().enumerate() {
            py_incref(arg);
            crate::types::tuple::PyTuple_SET_ITEM(args_tuple, i as isize, arg);
        }
        let result = crate::types::funcobject::call_cfunction(func, args_tuple, ptr::null_mut());
        (*args_tuple).decref();
        return result;
    }

    // Check tp_call
    let tp = (*func).ob_type;
    if !tp.is_null() {
        if let Some(tp_call) = (*tp).tp_call {
            let args_tuple = crate::types::tuple::PyTuple_New(args.len() as isize);
            for (i, &arg) in args.iter().enumerate() {
                py_incref(arg);
                crate::types::tuple::PyTuple_SET_ITEM(args_tuple, i as isize, arg);
            }
            let result = tp_call(func, args_tuple, ptr::null_mut());
            (*args_tuple).decref();
            return result;
        }
    }

    return_none()
}

// ─── Built-in function implementations ───

/// Create a builtin function from a Rust fn pointer.
unsafe fn create_builtin_function(
    name: &str,
    func: unsafe extern "C" fn(*mut RawPyObject, *mut RawPyObject) -> *mut RawPyObject,
) -> *mut RawPyObject {
    let name_cstr = std::ffi::CString::new(name).unwrap();
    // Leak the CString to get a stable pointer (these are permanent)
    let name_ptr = name_cstr.into_raw() as *const std::os::raw::c_char;
    crate::types::funcobject::create_cfunction(
        name_ptr,
        Some(func),
        crate::object::typeobj::METH_VARARGS,
        ptr::null_mut(),
    )
}

/// Built-in print function
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
            // Check bool BEFORE int (bool subclasses int)
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

/// Built-in len function
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
    } else {
        // Try string
        if is_str(obj) {
            let s = crate::types::unicode::unicode_value(obj);
            create_int(s.len() as i64)
        } else {
            create_int(0)
        }
    }
}

/// Built-in type function
unsafe extern "C" fn builtin_type(
    _self: *mut RawPyObject,
    args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    crate::ffi::object_api::PyObject_Type(obj)
}

/// Built-in range function (returns a list for simplicity)
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

/// Built-in int function
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

/// Built-in str function
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

/// Built-in isinstance function
unsafe extern "C" fn builtin_isinstance(
    _self: *mut RawPyObject,
    args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    let tp = crate::types::tuple::PyTuple_GetItem(args, 1);
    if obj.is_null() || tp.is_null() {
        return py_false();
    }
    let obj_type = (*obj).ob_type as *mut RawPyObject;
    bool_from_long(if obj_type == tp { 1 } else { 0 })
}
