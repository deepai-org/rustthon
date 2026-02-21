//! The bytecode interpreter (VM execution loop).
//!
//! This is the beating heart of Rustthon — the main eval loop
//! that fetches instructions and dispatches them.

use crate::compiler::bytecode::{CodeObject, OpCode};
use crate::object::pyobject::RawPyObject;
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
                return Ok(unsafe { crate::types::none::return_none() });
            }

            let instr = frame.code.instructions[frame.ip].clone();
            frame.ip += 1;

            match instr.opcode {
                OpCode::Nop => {}

                OpCode::LoadConst => {
                    let obj = frame.code.constants[instr.arg as usize];
                    unsafe {
                        if !obj.is_null() {
                            (*obj).incref();
                        }
                    }
                    frame.push(obj);
                }

                OpCode::LoadName => {
                    let name = &frame.code.names[instr.arg as usize].clone();
                    let obj = frame.lookup_name(name);
                    if obj.is_null() {
                        return Err(format!("NameError: name '{}' is not defined", name));
                    }
                    unsafe {
                        (*obj).incref();
                    }
                    frame.push(obj);
                }

                OpCode::StoreName => {
                    let name = frame.code.names[instr.arg as usize].clone();
                    let obj = frame.pop();
                    frame.store_name(&name, obj);
                    // store_name increfs, so we can decref our copy
                    unsafe {
                        if !obj.is_null() {
                            (*obj).decref();
                        }
                    }
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
                    unsafe {
                        if !obj.is_null() {
                            (*obj).incref();
                        }
                    }
                    frame.push(obj);
                }

                OpCode::StoreGlobal => {
                    let name = frame.code.names[instr.arg as usize].clone();
                    let obj = frame.pop();
                    unsafe {
                        if !obj.is_null() {
                            (*obj).incref();
                        }
                    }
                    frame.globals.insert(name, obj);
                }

                OpCode::PopTop => {
                    let obj = frame.pop();
                    unsafe {
                        if !obj.is_null() {
                            (*obj).decref();
                        }
                    }
                }

                OpCode::DupTop => {
                    let obj = frame.top();
                    unsafe {
                        if !obj.is_null() {
                            (*obj).incref();
                        }
                    }
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
                    let result = unsafe { binary_add(left, right) };
                    frame.push(result);
                    unsafe {
                        if !left.is_null() { (*left).decref(); }
                        if !right.is_null() { (*right).decref(); }
                    }
                }

                OpCode::BinarySubtract => {
                    let right = frame.pop();
                    let left = frame.pop();
                    let result = unsafe { binary_sub(left, right) };
                    frame.push(result);
                    unsafe {
                        if !left.is_null() { (*left).decref(); }
                        if !right.is_null() { (*right).decref(); }
                    }
                }

                OpCode::BinaryMultiply => {
                    let right = frame.pop();
                    let left = frame.pop();
                    let result = unsafe { binary_mul(left, right) };
                    frame.push(result);
                    unsafe {
                        if !left.is_null() { (*left).decref(); }
                        if !right.is_null() { (*right).decref(); }
                    }
                }

                OpCode::BinaryTrueDivide => {
                    let right = frame.pop();
                    let left = frame.pop();
                    let result = unsafe { binary_truediv(left, right) };
                    frame.push(result);
                    unsafe {
                        if !left.is_null() { (*left).decref(); }
                        if !right.is_null() { (*right).decref(); }
                    }
                }

                OpCode::BinaryFloorDivide => {
                    let right = frame.pop();
                    let left = frame.pop();
                    let result = unsafe { binary_floordiv(left, right) };
                    frame.push(result);
                    unsafe {
                        if !left.is_null() { (*left).decref(); }
                        if !right.is_null() { (*right).decref(); }
                    }
                }

                OpCode::BinaryModulo => {
                    let right = frame.pop();
                    let left = frame.pop();
                    let result = unsafe { binary_mod(left, right) };
                    frame.push(result);
                    unsafe {
                        if !left.is_null() { (*left).decref(); }
                        if !right.is_null() { (*right).decref(); }
                    }
                }

                OpCode::BinaryPower => {
                    let right = frame.pop();
                    let left = frame.pop();
                    let result = unsafe { binary_pow(left, right) };
                    frame.push(result);
                    unsafe {
                        if !left.is_null() { (*left).decref(); }
                        if !right.is_null() { (*right).decref(); }
                    }
                }

                OpCode::BinaryAnd | OpCode::BinaryOr | OpCode::BinaryXor |
                OpCode::BinaryLShift | OpCode::BinaryRShift => {
                    let right = frame.pop();
                    let left = frame.pop();
                    let result = unsafe { binary_bitop(left, right, instr.opcode) };
                    frame.push(result);
                    unsafe {
                        if !left.is_null() { (*left).decref(); }
                        if !right.is_null() { (*right).decref(); }
                    }
                }

                OpCode::InplaceAdd => {
                    let right = frame.pop();
                    let left = frame.pop();
                    let result = unsafe { binary_add(left, right) };
                    frame.push(result);
                    unsafe {
                        if !left.is_null() { (*left).decref(); }
                        if !right.is_null() { (*right).decref(); }
                    }
                }

                OpCode::InplaceSubtract => {
                    let right = frame.pop();
                    let left = frame.pop();
                    let result = unsafe { binary_sub(left, right) };
                    frame.push(result);
                    unsafe {
                        if !left.is_null() { (*left).decref(); }
                        if !right.is_null() { (*right).decref(); }
                    }
                }

                OpCode::InplaceMultiply => {
                    let right = frame.pop();
                    let left = frame.pop();
                    let result = unsafe { binary_mul(left, right) };
                    frame.push(result);
                    unsafe {
                        if !left.is_null() { (*left).decref(); }
                        if !right.is_null() { (*right).decref(); }
                    }
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
                    unsafe {
                        if !obj.is_null() { (*obj).decref(); }
                        if !key.is_null() { (*key).decref(); }
                    }
                }

                // ─── Comparison ───
                OpCode::CompareOp => {
                    let right = frame.pop();
                    let left = frame.pop();
                    let result = unsafe { compare_op(left, right, instr.arg) };
                    frame.push(result);
                    unsafe {
                        if !left.is_null() { (*left).decref(); }
                        if !right.is_null() { (*right).decref(); }
                    }
                }

                // ─── Unary ───
                OpCode::UnaryNot => {
                    let obj = frame.pop();
                    let result = unsafe {
                        let is_true = crate::ffi::object_api::PyObject_IsTrue(obj);
                        if is_true != 0 {
                            crate::types::boolobject::PY_FALSE.get()
                        } else {
                            crate::types::boolobject::PY_TRUE.get()
                        }
                    };
                    unsafe {
                        (*result).incref();
                        if !obj.is_null() { (*obj).decref(); }
                    }
                    frame.push(result);
                }

                OpCode::UnaryNegative => {
                    let obj = frame.pop();
                    let result = unsafe { unary_negative(obj) };
                    frame.push(result);
                    unsafe {
                        if !obj.is_null() { (*obj).decref(); }
                    }
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
                    unsafe {
                        if !obj.is_null() { (*obj).decref(); }
                    }
                    if is_true == 0 {
                        frame.ip = instr.arg as usize;
                    }
                }

                OpCode::PopJumpIfTrue => {
                    let obj = frame.pop();
                    let is_true = unsafe {
                        crate::ffi::object_api::PyObject_IsTrue(obj)
                    };
                    unsafe {
                        if !obj.is_null() { (*obj).decref(); }
                    }
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

                    unsafe {
                        if !func.is_null() { (*func).decref(); }
                        for &arg in &args {
                            if !arg.is_null() { (*arg).decref(); }
                        }
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
                        if !obj.is_null() { (*obj).decref(); }
                        if !key.is_null() { (*key).decref(); }
                        if !value.is_null() { (*value).decref(); }
                    }
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
                    if !obj.is_null() {
                        unsafe {
                            if !crate::types::none::is_none(obj) {
                                let repr = crate::ffi::object_api::PyObject_Repr(obj);
                                if !repr.is_null() {
                                    let s = crate::types::unicode::unicode_value(repr);
                                    // Don't print in non-interactive mode
                                    (*repr).decref();
                                }
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
                    let none = unsafe { crate::types::none::return_none() };
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
                        if !obj.is_null() { (*obj).decref(); }
                    }
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
                        if !obj.is_null() { (*obj).decref(); }
                        if !value.is_null() { (*value).decref(); }
                    }
                }

                _ => {
                    // Unhandled opcode
                    return Err(format!("Unimplemented opcode: {:?}", instr.opcode));
                }
            }
        }
    }
}

// ─── Helper functions ───

unsafe fn get_int_value(obj: *mut RawPyObject) -> i64 {
    if obj.is_null() {
        return 0;
    }
    if is_int(obj) || crate::types::boolobject::is_bool(obj) {
        crate::types::longobject::long_as_i64(obj)
    } else {
        0
    }
}

unsafe fn get_float_value(obj: *mut RawPyObject) -> f64 {
    if obj.is_null() {
        return 0.0;
    }
    if (*obj).ob_type == crate::types::floatobject::float_type() {
        crate::types::floatobject::float_value(obj)
    } else if is_int(obj) || crate::types::boolobject::is_bool(obj) {
        crate::types::longobject::long_as_f64(obj)
    } else {
        0.0
    }
}

unsafe fn is_int(obj: *mut RawPyObject) -> bool {
    !obj.is_null() && (*obj).ob_type == crate::types::longobject::long_type()
}

unsafe fn is_float(obj: *mut RawPyObject) -> bool {
    !obj.is_null() && (*obj).ob_type == crate::types::floatobject::float_type()
}

unsafe fn is_string(obj: *mut RawPyObject) -> bool {
    !obj.is_null() && (*obj).ob_type == crate::types::unicode::unicode_type()
}

unsafe fn is_list(obj: *mut RawPyObject) -> bool {
    !obj.is_null() && (*obj).ob_type == crate::types::list::list_type()
}

unsafe fn binary_add(left: *mut RawPyObject, right: *mut RawPyObject) -> *mut RawPyObject {
    if is_int(left) && is_int(right) {
        let l = crate::types::longobject::long_as_i64(left);
        let r = crate::types::longobject::long_as_i64(right);
        crate::types::longobject::PyLong_FromLong((l.wrapping_add(r)) as _)
    } else if is_float(left) || is_float(right) {
        let l = get_float_value(left);
        let r = get_float_value(right);
        crate::types::floatobject::PyFloat_FromDouble(l + r)
    } else if is_string(left) && is_string(right) {
        crate::types::unicode::PyUnicode_Concat(left, right)
    } else if is_list(left) && is_list(right) {
        // List concatenation via C API
        let l_size = crate::types::list::PyList_Size(left);
        let r_size = crate::types::list::PyList_Size(right);
        let total = l_size + r_size;
        let new_list = crate::types::list::PyList_New(total);
        for i in 0..l_size {
            let item = crate::types::list::PyList_GetItem(left, i);
            if !item.is_null() { (*item).incref(); }
            crate::types::list::PyList_SET_ITEM(new_list, i, item);
        }
        for i in 0..r_size {
            let item = crate::types::list::PyList_GetItem(right, i);
            if !item.is_null() { (*item).incref(); }
            crate::types::list::PyList_SET_ITEM(new_list, l_size + i, item);
        }
        new_list
    } else {
        crate::types::none::return_none()
    }
}

unsafe fn binary_sub(left: *mut RawPyObject, right: *mut RawPyObject) -> *mut RawPyObject {
    if is_int(left) && is_int(right) {
        let l = crate::types::longobject::long_as_i64(left);
        let r = crate::types::longobject::long_as_i64(right);
        crate::types::longobject::PyLong_FromLong(l.wrapping_sub(r) as _)
    } else if is_float(left) || is_float(right) {
        let l = get_float_value(left);
        let r = get_float_value(right);
        crate::types::floatobject::PyFloat_FromDouble(l - r)
    } else {
        crate::types::none::return_none()
    }
}

unsafe fn binary_mul(left: *mut RawPyObject, right: *mut RawPyObject) -> *mut RawPyObject {
    if is_int(left) && is_int(right) {
        let l = crate::types::longobject::long_as_i64(left);
        let r = crate::types::longobject::long_as_i64(right);
        crate::types::longobject::PyLong_FromLong(l.wrapping_mul(r) as _)
    } else if is_float(left) || is_float(right) {
        let l = get_float_value(left);
        let r = get_float_value(right);
        crate::types::floatobject::PyFloat_FromDouble(l * r)
    } else {
        crate::types::none::return_none()
    }
}

unsafe fn binary_truediv(left: *mut RawPyObject, right: *mut RawPyObject) -> *mut RawPyObject {
    let l = get_float_value(left);
    let r = get_float_value(right);
    if r == 0.0 {
        // TODO: ZeroDivisionError
        return crate::types::none::return_none();
    }
    crate::types::floatobject::PyFloat_FromDouble(l / r)
}

unsafe fn binary_floordiv(left: *mut RawPyObject, right: *mut RawPyObject) -> *mut RawPyObject {
    if is_int(left) && is_int(right) {
        let l = crate::types::longobject::long_as_i64(left);
        let r = crate::types::longobject::long_as_i64(right);
        if r == 0 {
            return crate::types::none::return_none();
        }
        // Python floor division: rounds toward negative infinity
        let d = l.wrapping_div(r);
        let result = if (l ^ r) < 0 && d * r != l { d - 1 } else { d };
        crate::types::longobject::PyLong_FromLong(result as _)
    } else {
        let l = get_float_value(left);
        let r = get_float_value(right);
        if r == 0.0 {
            return crate::types::none::return_none();
        }
        crate::types::floatobject::PyFloat_FromDouble((l / r).floor())
    }
}

unsafe fn binary_mod(left: *mut RawPyObject, right: *mut RawPyObject) -> *mut RawPyObject {
    if is_int(left) && is_int(right) {
        let l = crate::types::longobject::long_as_i64(left);
        let r = crate::types::longobject::long_as_i64(right);
        if r == 0 {
            return crate::types::none::return_none();
        }
        // Python modulo: result has same sign as divisor
        let m = l % r;
        let result = if m != 0 && (m ^ r) < 0 { m + r } else { m };
        crate::types::longobject::PyLong_FromLong(result as _)
    } else {
        let l = get_float_value(left);
        let r = get_float_value(right);
        if r == 0.0 {
            return crate::types::none::return_none();
        }
        crate::types::floatobject::PyFloat_FromDouble(l % r)
    }
}

unsafe fn binary_pow(left: *mut RawPyObject, right: *mut RawPyObject) -> *mut RawPyObject {
    if is_int(left) && is_int(right) {
        let l = crate::types::longobject::long_as_i64(left);
        let r = crate::types::longobject::long_as_i64(right);
        if r >= 0 && r <= 63 {
            let result = l.wrapping_pow(r as u32);
            crate::types::longobject::PyLong_FromLong(result as _)
        } else {
            crate::types::floatobject::PyFloat_FromDouble((l as f64).powf(r as f64))
        }
    } else {
        let l = get_float_value(left);
        let r = get_float_value(right);
        crate::types::floatobject::PyFloat_FromDouble(l.powf(r))
    }
}

unsafe fn binary_bitop(left: *mut RawPyObject, right: *mut RawPyObject, op: OpCode) -> *mut RawPyObject {
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
    crate::types::longobject::PyLong_FromLong(result as _)
}

unsafe fn unary_negative(obj: *mut RawPyObject) -> *mut RawPyObject {
    if is_int(obj) {
        let val = crate::types::longobject::long_as_i64(obj);
        crate::types::longobject::PyLong_FromLong(val.wrapping_neg() as _)
    } else if is_float(obj) {
        let val = crate::types::floatobject::float_value(obj);
        crate::types::floatobject::PyFloat_FromDouble(-val)
    } else {
        crate::types::none::return_none()
    }
}

unsafe fn compare_op(left: *mut RawPyObject, right: *mut RawPyObject, op: u32) -> *mut RawPyObject {
    match op {
        6 => {
            // is
            crate::types::boolobject::PyBool_FromLong(if left == right { 1 } else { 0 })
        }
        7 => {
            // is not
            crate::types::boolobject::PyBool_FromLong(if left != right { 1 } else { 0 })
        }
        _ => {
            // Numeric comparison
            if is_int(left) && is_int(right) {
                let l = crate::types::longobject::long_as_i64(left);
                let r = crate::types::longobject::long_as_i64(right);
                let result = match op {
                    0 => l < r,  // <
                    1 => l <= r, // <=
                    2 => l == r, // ==
                    3 => l != r, // !=
                    4 => l > r,  // >
                    5 => l >= r, // >=
                    _ => false,
                };
                crate::types::boolobject::PyBool_FromLong(if result { 1 } else { 0 })
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
                crate::types::boolobject::PyBool_FromLong(if result { 1 } else { 0 })
            } else if is_string(left) && is_string(right) {
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
                crate::types::boolobject::PyBool_FromLong(if result { 1 } else { 0 })
            } else {
                // Default: identity comparison
                crate::types::boolobject::PyBool_FromLong(
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
        if !item.is_null() {
            (*item).incref();
        }
        return item;
    }
    if crate::types::tuple::PyTuple_Check(obj) != 0 {
        let idx = get_int_value(key) as isize;
        let item = crate::types::tuple::PyTuple_GetItem(obj, idx);
        if !item.is_null() {
            (*item).incref();
        }
        return item;
    }
    if crate::types::dict::PyDict_Check(obj) != 0 {
        let item = crate::types::dict::PyDict_GetItem(obj, key);
        if !item.is_null() {
            (*item).incref();
        }
        return item;
    }
    ptr::null_mut()
}

unsafe fn call_function(func: *mut RawPyObject, args: &[*mut RawPyObject]) -> *mut RawPyObject {
    if func.is_null() {
        return crate::types::none::return_none();
    }

    // Check if it's a built-in CFunction
    if (*func).ob_type == crate::types::funcobject::cfunction_type() {
        // Build args tuple
        let args_tuple = crate::types::tuple::PyTuple_New(args.len() as isize);
        for (i, &arg) in args.iter().enumerate() {
            if !arg.is_null() {
                (*arg).incref();
            }
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
                if !arg.is_null() {
                    (*arg).incref();
                }
                crate::types::tuple::PyTuple_SET_ITEM(args_tuple, i as isize, arg);
            }
            let result = tp_call(func, args_tuple, ptr::null_mut());
            (*args_tuple).decref();
            return result;
        }
    }

    crate::types::none::return_none()
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
        return crate::types::none::return_none();
    }

    let nargs = crate::types::tuple::PyTuple_Size(args);
    let mut parts = Vec::new();

    for i in 0..nargs {
        let item = crate::types::tuple::PyTuple_GetItem(args, i);
        if item.is_null() {
            parts.push("None".to_string());
            continue;
        }

        if crate::types::none::is_none(item) {
            parts.push("None".to_string());
        } else if crate::types::boolobject::is_bool(item) {
            // Check bool BEFORE int (bool subclasses int)
            if crate::types::boolobject::is_true(item) {
                parts.push("True".to_string());
            } else {
                parts.push("False".to_string());
            }
        } else if is_string(item) {
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
            if !repr.is_null() && is_string(repr) {
                parts.push(crate::types::unicode::unicode_value(repr).to_string());
                (*repr).decref();
            } else {
                parts.push(format!("<object at {:p}>", item));
            }
        }
    }

    println!("{}", parts.join(" "));
    crate::types::none::return_none()
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
    if obj.is_null() || crate::types::none::is_none(obj) {
        "None".to_string()
    } else if is_string(obj) {
        format!("'{}'", crate::types::unicode::unicode_value(obj))
    } else if crate::types::boolobject::is_bool(obj) {
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
        return crate::types::longobject::PyLong_FromLong(0);
    }
    let len = crate::ffi::object_api::PyObject_Length(obj);
    if len >= 0 {
        crate::types::longobject::PyLong_FromLong(len as _)
    } else {
        // Try string
        if is_string(obj) {
            let s = crate::types::unicode::unicode_value(obj);
            crate::types::longobject::PyLong_FromLong(s.len() as _)
        } else {
            crate::types::longobject::PyLong_FromLong(0)
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
        return crate::types::longobject::PyLong_FromLong(0);
    }
    if is_int(obj) {
        (*obj).incref();
        return obj;
    }
    if is_float(obj) {
        let val = crate::types::floatobject::float_value(obj);
        return crate::types::longobject::PyLong_FromLong(val as _);
    }
    if is_string(obj) {
        let s = crate::types::unicode::unicode_value(obj);
        if let Ok(val) = s.trim().parse::<i64>() {
            return crate::types::longobject::PyLong_FromLong(val as _);
        }
    }
    crate::types::longobject::PyLong_FromLong(0)
}

/// Built-in str function
unsafe extern "C" fn builtin_str(
    _self: *mut RawPyObject,
    args: *mut RawPyObject,
) -> *mut RawPyObject {
    let obj = crate::types::tuple::PyTuple_GetItem(args, 0);
    if obj.is_null() {
        return crate::types::unicode::create_from_str("None");
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
        return crate::types::boolobject::PY_FALSE.get();
    }
    let obj_type = (*obj).ob_type as *mut RawPyObject;
    crate::types::boolobject::PyBool_FromLong(if obj_type == tp { 1 } else { 0 })
}
