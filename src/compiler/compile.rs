//! AST -> Bytecode compiler.
//!
//! Takes a Python AST (from rustpython-parser) and compiles it
//! into our bytecode format.

use crate::compiler::bytecode::{CodeObject, OpCode};
use crate::object::safe_api;
use crate::runtime::gil::Python;
use crate::runtime::pyerr::PyResult;
use rustpython_parser::ast::{self, Constant, Expr, Stmt};
use rustpython_parser::Parse;

/// Compile Python source code into a CodeObject.
/// Takes a Python<'py> GIL token for compile-time proof the GIL is held.
pub fn compile_source(py: Python<'_>, source: &str, filename: &str) -> Result<CodeObject, String> {
    // Parse the source into an AST
    let ast = ast::Suite::parse(source, filename)
        .map_err(|e| format!("Parse error: {}", e))?;

    let mut compiler = Compiler::new(py, filename.to_string());
    compiler.compile_body(&ast)?;

    // Add implicit return None at the end
    let none_idx = compiler.add_none_const();
    compiler.emit(OpCode::LoadConst, none_idx);
    compiler.emit(OpCode::ReturnValue, 0);

    Ok(compiler.code_stack.pop().unwrap())
}

/// Compile a function body into a CodeObject (used by VM for class bodies too).
pub fn compile_function_body(
    py: Python<'_>,
    source_stmts: &[Stmt],
    name: &str,
    filename: &str,
) -> Result<CodeObject, String> {
    let mut compiler = Compiler::new(py, filename.to_string());
    compiler.code_stack.last_mut().unwrap().name = name.to_string();
    compiler.compile_body(source_stmts)?;
    let none_idx = compiler.add_none_const();
    compiler.emit(OpCode::LoadConst, none_idx);
    compiler.emit(OpCode::ReturnValue, 0);
    Ok(compiler.code_stack.pop().unwrap())
}

struct Compiler<'py> {
    py: Python<'py>,
    /// Stack of code objects for nested compilation (functions inside functions)
    code_stack: Vec<CodeObject>,
    /// Loop context stack for break/continue patching
    /// Each entry: (loop_start_ip, break_patches: Vec<u32>)
    loop_stack: Vec<LoopContext>,
}

struct LoopContext {
    start_ip: u32,
    break_patches: Vec<u32>,
}

impl<'py> Compiler<'py> {
    fn new(py: Python<'py>, filename: String) -> Self {
        Compiler {
            py,
            code_stack: vec![CodeObject::new("<module>".to_string(), filename)],
            loop_stack: Vec::new(),
        }
    }

    /// Get mutable reference to the current (innermost) code object.
    fn code(&mut self) -> &mut CodeObject {
        self.code_stack.last_mut().unwrap()
    }

    /// Get shared reference to the current code object.
    fn code_ref(&self) -> &CodeObject {
        self.code_stack.last().unwrap()
    }

    fn emit(&mut self, opcode: OpCode, arg: u32) {
        self.code().emit(opcode, arg);
    }

    fn current_offset(&self) -> u32 {
        self.code_ref().current_offset()
    }

    fn patch_jump(&mut self, instr_idx: u32, target: u32) {
        self.code().patch_jump(instr_idx, target);
    }

    fn add_none_const(&mut self) -> u32 {
        let none = safe_api::none_obj(self.py);
        self.code().add_const(none)
    }

    /// Create and add a constant, mapping allocation failure to a compile error string.
    fn add_const_result(&mut self, result: PyResult) -> Result<u32, String> {
        let obj = result.map_err(|e| format!("Allocation error: {}", e))?;
        Ok(self.code().add_const(obj))
    }

    fn compile_body(&mut self, stmts: &[Stmt]) -> Result<(), String> {
        for stmt in stmts {
            self.compile_stmt(stmt)?;
        }
        Ok(())
    }

    fn compile_stmt(&mut self, stmt: &Stmt) -> Result<(), String> {
        match stmt {
            Stmt::Expr(expr_stmt) => {
                self.compile_expr(&expr_stmt.value)?;
                self.emit(OpCode::PrintExpr, 0);
                self.emit(OpCode::PopTop, 0);
            }

            Stmt::Assign(assign) => {
                self.compile_expr(&assign.value)?;
                // For multiple targets, dup the value
                for (i, target) in assign.targets.iter().enumerate() {
                    if i < assign.targets.len() - 1 {
                        self.emit(OpCode::DupTop, 0);
                    }
                    self.compile_store_target(target)?;
                }
            }

            Stmt::AugAssign(aug) => {
                self.compile_expr(&aug.target)?;
                self.compile_expr(&aug.value)?;
                let opcode = match aug.op {
                    ast::Operator::Add => OpCode::InplaceAdd,
                    ast::Operator::Sub => OpCode::InplaceSubtract,
                    ast::Operator::Mult => OpCode::InplaceMultiply,
                    _ => OpCode::BinaryAdd,
                };
                self.emit(opcode, 0);
                self.compile_store_target(&aug.target)?;
            }

            Stmt::Return(ret) => {
                if let Some(ref value) = ret.value {
                    self.compile_expr(value)?;
                } else {
                    let idx = self.add_none_const();
                    self.emit(OpCode::LoadConst, idx);
                }
                self.emit(OpCode::ReturnValue, 0);
            }

            Stmt::If(if_stmt) => {
                self.compile_if(if_stmt)?;
            }

            Stmt::While(while_stmt) => {
                self.compile_while(while_stmt)?;
            }

            Stmt::For(for_stmt) => {
                self.compile_for(for_stmt)?;
            }

            Stmt::FunctionDef(func_def) => {
                self.compile_function_def(func_def)?;
            }

            Stmt::Import(import) => {
                self.compile_import(import)?;
            }

            Stmt::ImportFrom(import_from) => {
                self.compile_import_from(import_from)?;
            }

            Stmt::ClassDef(class_def) => {
                self.compile_class_def(class_def)?;
            }

            Stmt::Try(try_stmt) => {
                self.compile_try(try_stmt)?;
            }

            Stmt::Raise(raise_stmt) => {
                self.compile_raise(raise_stmt)?;
            }

            Stmt::Delete(del_stmt) => {
                for target in &del_stmt.targets {
                    self.compile_delete_target(target)?;
                }
            }

            Stmt::Assert(assert_stmt) => {
                self.compile_assert(assert_stmt)?;
            }

            Stmt::Global(_) | Stmt::Nonlocal(_) => {
                // These are declarations that affect scope analysis.
                // For now, no-op — we handle them at compilation time.
            }

            Stmt::Pass(_) => {}

            Stmt::Break(_) => {
                // Emit a jump that will be patched to point past the loop end
                let patch_idx = self.current_offset();
                self.emit(OpCode::JumpAbsolute, 0); // placeholder target
                if let Some(loop_ctx) = self.loop_stack.last_mut() {
                    loop_ctx.break_patches.push(patch_idx);
                }
            }

            Stmt::Continue(_) => {
                if let Some(loop_ctx) = self.loop_stack.last() {
                    let target = loop_ctx.start_ip;
                    self.emit(OpCode::JumpAbsolute, target);
                }
            }

            _ => {
                self.emit(OpCode::Nop, 0);
            }
        }
        Ok(())
    }

    fn compile_expr(&mut self, expr: &Expr) -> Result<(), String> {
        match expr {
            Expr::Constant(constant) => {
                match &constant.value {
                    Constant::Int(i) => {
                        let val: i64 = i.try_into().unwrap_or(0);
                        let idx = self.add_const_result(safe_api::new_int(self.py, val))?;
                        self.emit(OpCode::LoadConst, idx);
                    }
                    Constant::Float(f) => {
                        let idx = self.add_const_result(safe_api::new_float(self.py, *f))?;
                        self.emit(OpCode::LoadConst, idx);
                    }
                    Constant::Complex { real, imag: _ } => {
                        let idx = self.add_const_result(safe_api::new_float(self.py, *real))?;
                        self.emit(OpCode::LoadConst, idx);
                    }
                    Constant::Str(s) => {
                        let idx = self.add_const_result(safe_api::new_str(self.py, s))?;
                        self.emit(OpCode::LoadConst, idx);
                    }
                    Constant::Bytes(b) => {
                        let idx = self.add_const_result(safe_api::new_bytes(self.py, b))?;
                        self.emit(OpCode::LoadConst, idx);
                    }
                    Constant::Bool(b) => {
                        let obj = safe_api::bool_obj(self.py, *b);
                        let idx = self.code().add_const(obj);
                        self.emit(OpCode::LoadConst, idx);
                    }
                    Constant::None => {
                        let idx = self.add_none_const();
                        self.emit(OpCode::LoadConst, idx);
                    }
                    Constant::Ellipsis => {
                        let idx = self.add_none_const();
                        self.emit(OpCode::LoadConst, idx);
                    }
                    Constant::Tuple(items) => {
                        for item in items {
                            self.compile_constant(item)?;
                        }
                        self.emit(OpCode::BuildTuple, items.len() as u32);
                    }
                }
            }

            Expr::Name(name) => {
                let name_str = name.id.to_string();
                // Check if it's a free variable (nonlocal — loaded from cells)
                let freevar_idx = self.code_ref().freevars.iter().position(|n| n == &name_str);
                if let Some(idx) = freevar_idx {
                    self.emit(OpCode::LoadDeref, idx as u32);
                } else {
                    // Check if it's a fast local
                    let varname_idx = self.code_ref().varnames.iter().position(|n| n == &name_str);
                    if let Some(idx) = varname_idx {
                        self.emit(OpCode::LoadFast, idx as u32);
                    } else {
                        let idx = self.code().add_name(&name_str);
                        self.emit(OpCode::LoadName, idx);
                    }
                }
            }

            Expr::BinOp(binop) => {
                self.compile_expr(&binop.left)?;
                self.compile_expr(&binop.right)?;
                let opcode = match binop.op {
                    ast::Operator::Add => OpCode::BinaryAdd,
                    ast::Operator::Sub => OpCode::BinarySubtract,
                    ast::Operator::Mult => OpCode::BinaryMultiply,
                    ast::Operator::Div => OpCode::BinaryTrueDivide,
                    ast::Operator::FloorDiv => OpCode::BinaryFloorDivide,
                    ast::Operator::Mod => OpCode::BinaryModulo,
                    ast::Operator::Pow => OpCode::BinaryPower,
                    ast::Operator::BitAnd => OpCode::BinaryAnd,
                    ast::Operator::BitOr => OpCode::BinaryOr,
                    ast::Operator::BitXor => OpCode::BinaryXor,
                    ast::Operator::LShift => OpCode::BinaryLShift,
                    ast::Operator::RShift => OpCode::BinaryRShift,
                    ast::Operator::MatMult => OpCode::BinaryMultiply,
                };
                self.emit(opcode, 0);
            }

            Expr::UnaryOp(unop) => {
                self.compile_expr(&unop.operand)?;
                let opcode = match unop.op {
                    ast::UnaryOp::Not => OpCode::UnaryNot,
                    ast::UnaryOp::USub => OpCode::UnaryNegative,
                    ast::UnaryOp::UAdd => OpCode::UnaryPositive,
                    ast::UnaryOp::Invert => OpCode::UnaryNot,
                };
                self.emit(opcode, 0);
            }

            Expr::Compare(cmp) => {
                self.compile_expr(&cmp.left)?;
                // Handle chained comparisons: a < b < c → a < b and b < c
                if cmp.comparators.len() == 1 {
                    self.compile_expr(&cmp.comparators[0])?;
                    let cmp_op = cmpop_to_arg(&cmp.ops[0]);
                    self.emit(OpCode::CompareOp, cmp_op);
                } else {
                    // Chained comparison: a op1 b op2 c ...
                    // Evaluate each pair, short-circuit on false
                    let mut end_jumps = Vec::new();
                    for (i, (op, comparator)) in cmp.ops.iter().zip(cmp.comparators.iter()).enumerate() {
                        if i > 0 {
                            // For chained: b is already on stack from prev dup
                        }
                        self.compile_expr(comparator)?;
                        if i < cmp.ops.len() - 1 {
                            // Not the last comparison — dup the RHS for next comparison
                            self.emit(OpCode::DupTop, 0);
                            self.emit(OpCode::RotThree, 0);
                        }
                        let cmp_op = cmpop_to_arg(op);
                        self.emit(OpCode::CompareOp, cmp_op);
                        if i < cmp.ops.len() - 1 {
                            let jump = self.current_offset();
                            self.emit(OpCode::JumpIfFalse, 0);
                            self.emit(OpCode::PopTop, 0);
                            end_jumps.push(jump);
                        }
                    }
                    let end = self.current_offset();
                    for j in end_jumps {
                        self.patch_jump(j, end);
                    }
                }
            }

            Expr::BoolOp(boolop) => {
                let values = &boolop.values;
                if values.is_empty() {
                    let idx = self.add_none_const();
                    self.emit(OpCode::LoadConst, idx);
                    return Ok(());
                }
                self.compile_expr(&values[0])?;
                for value in &values[1..] {
                    match boolop.op {
                        ast::BoolOp::And => {
                            let jump = self.current_offset();
                            self.emit(OpCode::JumpIfFalse, 0);
                            self.emit(OpCode::PopTop, 0);
                            self.compile_expr(value)?;
                            let end = self.current_offset();
                            self.patch_jump(jump, end);
                        }
                        ast::BoolOp::Or => {
                            let jump = self.current_offset();
                            self.emit(OpCode::JumpIfTrue, 0);
                            self.emit(OpCode::PopTop, 0);
                            self.compile_expr(value)?;
                            let end = self.current_offset();
                            self.patch_jump(jump, end);
                        }
                    }
                }
            }

            Expr::Call(call) => {
                self.compile_call(call)?;
            }

            Expr::Attribute(attr) => {
                self.compile_expr(&attr.value)?;
                let idx = self.code().add_name(&attr.attr.to_string());
                self.emit(OpCode::LoadAttr, idx);
            }

            Expr::Subscript(sub) => {
                self.compile_expr(&sub.value)?;
                self.compile_expr(&sub.slice)?;
                self.emit(OpCode::BinarySubscr, 0);
            }

            Expr::List(list) => {
                let n = list.elts.len() as u32;
                for elt in &list.elts {
                    self.compile_expr(elt)?;
                }
                self.emit(OpCode::BuildList, n);
            }

            Expr::Tuple(tuple) => {
                let n = tuple.elts.len() as u32;
                for elt in &tuple.elts {
                    self.compile_expr(elt)?;
                }
                self.emit(OpCode::BuildTuple, n);
            }

            Expr::Dict(dict) => {
                let n = dict.keys.len() as u32;
                for (key, value) in dict.keys.iter().zip(dict.values.iter()) {
                    if let Some(k) = key {
                        self.compile_expr(k)?;
                    } else {
                        let idx = self.add_none_const();
                        self.emit(OpCode::LoadConst, idx);
                    }
                    self.compile_expr(value)?;
                }
                self.emit(OpCode::BuildMap, n);
            }

            Expr::Set(set) => {
                let n = set.elts.len() as u32;
                for elt in &set.elts {
                    self.compile_expr(elt)?;
                }
                self.emit(OpCode::BuildSet, n);
            }

            Expr::IfExp(ifexp) => {
                self.compile_expr(&ifexp.test)?;
                let jump_to_else = self.current_offset();
                self.emit(OpCode::PopJumpIfFalse, 0);
                self.compile_expr(&ifexp.body)?;
                let jump_to_end = self.current_offset();
                self.emit(OpCode::JumpAbsolute, 0);
                let else_start = self.current_offset();
                self.patch_jump(jump_to_else, else_start);
                self.compile_expr(&ifexp.orelse)?;
                let end = self.current_offset();
                self.patch_jump(jump_to_end, end);
            }

            Expr::JoinedStr(fstring) => {
                if fstring.values.is_empty() {
                    let idx = self.add_const_result(safe_api::new_str(self.py, ""))?;
                    self.emit(OpCode::LoadConst, idx);
                } else {
                    self.compile_expr(&fstring.values[0])?;
                    for value in &fstring.values[1..] {
                        self.compile_expr(value)?;
                        self.emit(OpCode::BinaryAdd, 0);
                    }
                }
            }

            Expr::FormattedValue(fv) => {
                self.compile_expr(&fv.value)?;
            }

            Expr::ListComp(comp) => {
                self.compile_list_comp(comp)?;
            }

            Expr::SetComp(comp) => {
                self.compile_set_comp(comp)?;
            }

            Expr::DictComp(comp) => {
                self.compile_dict_comp(comp)?;
            }

            Expr::GeneratorExp(genexp) => {
                self.compile_generator_exp(genexp)?;
            }

            Expr::Lambda(lambda) => {
                self.compile_lambda(lambda)?;
            }

            Expr::Starred(starred) => {
                // In list context (e.g. [*a, b]), just compile the inner value
                self.compile_expr(&starred.value)?;
            }

            Expr::Slice(slice) => {
                // Compile slice(lower, upper, step) → BuildSlice(nargs)
                // Push lower (or None), upper (or None), optionally step
                if let Some(ref lower) = slice.lower {
                    self.compile_expr(lower)?;
                } else {
                    let idx = self.add_none_const();
                    self.emit(OpCode::LoadConst, idx);
                }
                if let Some(ref upper) = slice.upper {
                    self.compile_expr(upper)?;
                } else {
                    let idx = self.add_none_const();
                    self.emit(OpCode::LoadConst, idx);
                }
                if let Some(ref step) = slice.step {
                    self.compile_expr(step)?;
                    self.emit(OpCode::BuildSlice, 3);
                } else {
                    self.emit(OpCode::BuildSlice, 2);
                }
            }

            _ => {
                let idx = self.add_none_const();
                self.emit(OpCode::LoadConst, idx);
            }
        }
        Ok(())
    }

    fn compile_call(&mut self, call: &ast::ExprCall) -> Result<(), String> {
        self.compile_expr(&call.func)?;

        // Check if we have keyword arguments
        if call.keywords.is_empty() {
            // Simple positional call
            let nargs = call.args.len() as u32;
            for arg in &call.args {
                self.compile_expr(arg)?;
            }
            self.emit(OpCode::CallFunction, nargs);
        } else {
            // Call with keyword arguments
            let n_positional = call.args.len();
            for arg in &call.args {
                self.compile_expr(arg)?;
            }
            // Compile keyword argument values
            let mut kw_names = Vec::new();
            for kw in &call.keywords {
                self.compile_expr(&kw.value)?;
                if let Some(ref name) = kw.arg {
                    kw_names.push(name.to_string());
                } else {
                    // **kwargs unpacking — for now treat as positional
                    kw_names.push(String::new());
                }
            }
            // Build tuple of keyword names as constant
            let mut name_objs = Vec::new();
            for name in &kw_names {
                let s = safe_api::new_str(self.py, name)
                    .map_err(|e| format!("Allocation error: {}", e))?;
                name_objs.push(s);
            }
            let kw_tuple = safe_api::build_tuple(self.py, name_objs)
                .map_err(|e| format!("Allocation error: {}", e))?;
            let kw_idx = self.code().add_const(kw_tuple);
            self.emit(OpCode::LoadConst, kw_idx);
            let total_args = (n_positional + call.keywords.len()) as u32;
            self.emit(OpCode::CallFunctionKW, total_args);
        }
        Ok(())
    }

    fn compile_constant(&mut self, constant: &Constant) -> Result<(), String> {
        match constant {
            Constant::Int(i) => {
                let val: i64 = i.try_into().unwrap_or(0);
                let idx = self.add_const_result(safe_api::new_int(self.py, val))?;
                self.emit(OpCode::LoadConst, idx);
            }
            Constant::Float(f) => {
                let idx = self.add_const_result(safe_api::new_float(self.py, *f))?;
                self.emit(OpCode::LoadConst, idx);
            }
            Constant::Str(s) => {
                let idx = self.add_const_result(safe_api::new_str(self.py, s))?;
                self.emit(OpCode::LoadConst, idx);
            }
            Constant::Bool(b) => {
                let obj = safe_api::bool_obj(self.py, *b);
                let idx = self.code().add_const(obj);
                self.emit(OpCode::LoadConst, idx);
            }
            Constant::None => {
                let idx = self.add_none_const();
                self.emit(OpCode::LoadConst, idx);
            }
            _ => {
                let idx = self.add_none_const();
                self.emit(OpCode::LoadConst, idx);
            }
        }
        Ok(())
    }

    fn compile_store_target(&mut self, target: &Expr) -> Result<(), String> {
        match target {
            Expr::Name(name) => {
                let name_str = name.id.to_string();
                // Check if it's a free variable (nonlocal — stored to cells)
                let freevar_idx = self.code_ref().freevars.iter().position(|n| n == &name_str);
                if let Some(idx) = freevar_idx {
                    self.emit(OpCode::StoreDeref, idx as u32);
                } else {
                    // Check if this is a fast local
                    let varname_idx = self.code_ref().varnames.iter().position(|n| n == &name_str);
                    if let Some(idx) = varname_idx {
                        self.emit(OpCode::StoreFast, idx as u32);
                    } else {
                        let idx = self.code().add_name(&name_str);
                        self.emit(OpCode::StoreName, idx);
                    }
                }
            }
            Expr::Subscript(sub) => {
                self.compile_expr(&sub.value)?;
                self.compile_expr(&sub.slice)?;
                self.emit(OpCode::StoreSubscr, 0);
            }
            Expr::Attribute(attr) => {
                self.compile_expr(&attr.value)?;
                let idx = self.code().add_name(&attr.attr.to_string());
                self.emit(OpCode::StoreAttr, idx);
            }
            Expr::Tuple(tuple) => {
                let n = tuple.elts.len();
                self.emit(OpCode::UnpackSequence, n as u32);
                for elt in &tuple.elts {
                    self.compile_store_target(elt)?;
                }
            }
            Expr::List(list) => {
                let n = list.elts.len();
                self.emit(OpCode::UnpackSequence, n as u32);
                for elt in &list.elts {
                    self.compile_store_target(elt)?;
                }
            }
            Expr::Starred(starred) => {
                self.compile_store_target(&starred.value)?;
            }
            _ => {
                return Err("Unsupported assignment target".to_string());
            }
        }
        Ok(())
    }

    fn compile_delete_target(&mut self, target: &Expr) -> Result<(), String> {
        match target {
            Expr::Name(name) => {
                let name_str = name.id.to_string();
                let idx = self.code().add_name(&name_str);
                self.emit(OpCode::DeleteName, idx);
            }
            Expr::Attribute(attr) => {
                self.compile_expr(&attr.value)?;
                let idx = self.code().add_name(&attr.attr.to_string());
                self.emit(OpCode::DeleteAttr, idx);
            }
            Expr::Subscript(sub) => {
                self.compile_expr(&sub.value)?;
                self.compile_expr(&sub.slice)?;
                self.emit(OpCode::DeleteSubscr, 0);
            }
            _ => {}
        }
        Ok(())
    }

    fn compile_if(&mut self, if_stmt: &ast::StmtIf) -> Result<(), String> {
        self.compile_expr(&if_stmt.test)?;
        let jump_to_else = self.current_offset();
        self.emit(OpCode::PopJumpIfFalse, 0);
        self.compile_body(&if_stmt.body)?;

        if if_stmt.orelse.is_empty() {
            let end = self.current_offset();
            self.patch_jump(jump_to_else, end);
        } else {
            let jump_to_end = self.current_offset();
            self.emit(OpCode::JumpAbsolute, 0);
            let else_start = self.current_offset();
            self.patch_jump(jump_to_else, else_start);
            self.compile_body(&if_stmt.orelse)?;
            let end = self.current_offset();
            self.patch_jump(jump_to_end, end);
        }
        Ok(())
    }

    fn compile_while(&mut self, while_stmt: &ast::StmtWhile) -> Result<(), String> {
        let loop_start = self.current_offset();
        self.loop_stack.push(LoopContext {
            start_ip: loop_start,
            break_patches: Vec::new(),
        });

        self.compile_expr(&while_stmt.test)?;
        let jump_to_end = self.current_offset();
        self.emit(OpCode::PopJumpIfFalse, 0);
        self.compile_body(&while_stmt.body)?;
        self.emit(OpCode::JumpAbsolute, loop_start);
        let end = self.current_offset();
        self.patch_jump(jump_to_end, end);

        // Patch all break jumps to point past the loop
        let loop_ctx = self.loop_stack.pop().unwrap();
        for patch_idx in loop_ctx.break_patches {
            self.patch_jump(patch_idx, end);
        }

        // Compile else clause (executes if loop completes without break)
        if !while_stmt.orelse.is_empty() {
            self.compile_body(&while_stmt.orelse)?;
        }
        Ok(())
    }

    fn compile_for(&mut self, for_stmt: &ast::StmtFor) -> Result<(), String> {
        // Compile iterable and get iterator
        self.compile_expr(&for_stmt.iter)?;
        self.emit(OpCode::GetIter, 0);

        let loop_start = self.current_offset();
        self.loop_stack.push(LoopContext {
            start_ip: loop_start,
            break_patches: Vec::new(),
        });

        // ForIter: try to get next item; if exhausted, jump to end
        let for_iter = self.current_offset();
        self.emit(OpCode::ForIter, 0); // placeholder for end jump
        self.compile_store_target(&for_stmt.target)?;
        self.compile_body(&for_stmt.body)?;
        self.emit(OpCode::JumpAbsolute, loop_start);
        let end = self.current_offset();
        self.patch_jump(for_iter, end);

        // Patch break jumps
        let loop_ctx = self.loop_stack.pop().unwrap();
        for patch_idx in loop_ctx.break_patches {
            self.patch_jump(patch_idx, end);
        }

        // Compile else clause
        if !for_stmt.orelse.is_empty() {
            self.compile_body(&for_stmt.orelse)?;
        }
        Ok(())
    }

    fn compile_function_def(&mut self, func_def: &ast::StmtFunctionDef) -> Result<(), String> {
        let func_name = func_def.name.to_string();
        let filename = self.code_ref().filename.clone();

        // 1. Compile default argument values in the OUTER scope
        let defaults: Vec<_> = func_def.args.defaults().collect();
        let n_defaults = defaults.len() as u32;
        for default in &defaults {
            self.compile_expr(default)?;
        }
        if n_defaults > 0 {
            self.emit(OpCode::BuildTuple, n_defaults);
        }

        // 2. Push new CodeObject for the function body
        let mut func_code = CodeObject::new(func_name.clone(), filename);

        // Set up parameter names as varnames (fast locals)
        for arg in &func_def.args.args {
            func_code.add_varname(&arg.def.arg.to_string());
        }
        func_code.argcount = func_def.args.args.len() as u32;

        // *args
        if let Some(ref vararg) = func_def.args.vararg {
            func_code.has_vararg = true;
            func_code.add_varname(&vararg.arg.to_string());
        }

        // keyword-only args
        for arg in &func_def.args.kwonlyargs {
            func_code.add_varname(&arg.def.arg.to_string());
        }
        func_code.kwonlyargcount = func_def.args.kwonlyargs.len() as u32;

        // **kwargs
        if let Some(ref kwarg) = func_def.args.kwarg {
            func_code.has_kwarg = true;
            func_code.add_varname(&kwarg.arg.to_string());
        }

        // Scan for nonlocal declarations first
        let nonlocals = self.scan_nonlocals(&func_def.body);

        // Scan body for local variable assignments (simple scope analysis)
        self.scan_locals(&func_def.body, &mut func_code);

        // Remove nonlocal names from varnames and register as freevars
        if !nonlocals.is_empty() {
            func_code.varnames.retain(|n| !nonlocals.contains(n));
            for nl in &nonlocals {
                if !func_code.freevars.contains(nl) {
                    func_code.freevars.push(nl.clone());
                }
            }
            // Add nonlocal names to the enclosing code's cellvars
            let parent_code = self.code_stack.last_mut().unwrap();
            for nl in &nonlocals {
                if !parent_code.cellvars.contains(nl) {
                    parent_code.cellvars.push(nl.clone());
                }
            }
        }

        // Push the new code object onto the stack
        self.code_stack.push(func_code);

        // 3. Compile function body in the inner scope
        self.compile_body(&func_def.body)?;

        // Add implicit return None
        let none_idx = self.add_none_const();
        self.emit(OpCode::LoadConst, none_idx);
        self.emit(OpCode::ReturnValue, 0);

        // 4. Pop the completed code object
        let inner_code = self.code_stack.pop().unwrap();

        // Store the inner code as a constant in the outer scope (wrapped as a PyObject)
        // We'll use a special marker approach — store the CodeObject in a side channel
        // and put an int index as the constant
        let none_placeholder = safe_api::none_obj(self.py);
        let code_idx = self.code().add_const(none_placeholder);
        // We need to store the actual CodeObject. We'll replace the None with
        // a pointer to the code object stored in a Box.
        let code_box = Box::new(inner_code);
        let code_ptr = Box::into_raw(code_box) as usize;
        let code_marker = safe_api::new_int(self.py, code_ptr as i64)
            .map_err(|e| format!("Allocation error: {}", e))?;
        self.code().constants[code_idx as usize] = code_marker;

        self.emit(OpCode::LoadConst, code_idx);
        self.emit(OpCode::MakeFunction, n_defaults);

        // 5. Apply decorators (in reverse order)
        for decorator in func_def.decorator_list.iter().rev() {
            self.compile_expr(decorator)?;
            self.emit(OpCode::RotTwo, 0);
            self.emit(OpCode::CallFunction, 1);
        }

        // 6. Store the function
        let name_idx = self.code().add_name(&func_name);
        self.emit(OpCode::StoreName, name_idx);

        Ok(())
    }

    /// Scan a function body for local variable assignments and add them to varnames.
    fn scan_locals(&self, stmts: &[Stmt], code: &mut CodeObject) {
        for stmt in stmts {
            match stmt {
                Stmt::Assign(assign) => {
                    for target in &assign.targets {
                        self.scan_target_names(target, code);
                    }
                }
                Stmt::AugAssign(aug) => {
                    self.scan_target_names(&aug.target, code);
                }
                Stmt::For(for_stmt) => {
                    self.scan_target_names(&for_stmt.target, code);
                    self.scan_locals(&for_stmt.body, code);
                    self.scan_locals(&for_stmt.orelse, code);
                }
                Stmt::While(while_stmt) => {
                    self.scan_locals(&while_stmt.body, code);
                    self.scan_locals(&while_stmt.orelse, code);
                }
                Stmt::If(if_stmt) => {
                    self.scan_locals(&if_stmt.body, code);
                    self.scan_locals(&if_stmt.orelse, code);
                }
                Stmt::Try(try_stmt) => {
                    self.scan_locals(&try_stmt.body, code);
                    for handler in &try_stmt.handlers {
                        if let ast::ExceptHandler::ExceptHandler(h) = handler {
                            if let Some(ref name) = h.name {
                                code.add_varname(&name.to_string());
                            }
                            self.scan_locals(&h.body, code);
                        }
                    }
                    self.scan_locals(&try_stmt.orelse, code);
                    self.scan_locals(&try_stmt.finalbody, code);
                }
                // Don't scan into nested function/class defs
                _ => {}
            }
        }
    }

    fn scan_target_names(&self, target: &Expr, code: &mut CodeObject) {
        match target {
            Expr::Name(name) => {
                code.add_varname(&name.id.to_string());
            }
            Expr::Tuple(tuple) => {
                for elt in &tuple.elts {
                    self.scan_target_names(elt, code);
                }
            }
            Expr::List(list) => {
                for elt in &list.elts {
                    self.scan_target_names(elt, code);
                }
            }
            _ => {}
        }
    }

    /// Scan function body for `nonlocal` declarations (does NOT recurse into nested functions).
    fn scan_nonlocals(&self, stmts: &[Stmt]) -> Vec<String> {
        let mut nonlocals = Vec::new();
        for stmt in stmts {
            match stmt {
                Stmt::Nonlocal(nl) => {
                    for name in &nl.names {
                        let s = name.to_string();
                        if !nonlocals.contains(&s) {
                            nonlocals.push(s);
                        }
                    }
                }
                Stmt::For(for_stmt) => {
                    nonlocals.extend(self.scan_nonlocals(&for_stmt.body));
                    nonlocals.extend(self.scan_nonlocals(&for_stmt.orelse));
                }
                Stmt::While(while_stmt) => {
                    nonlocals.extend(self.scan_nonlocals(&while_stmt.body));
                    nonlocals.extend(self.scan_nonlocals(&while_stmt.orelse));
                }
                Stmt::If(if_stmt) => {
                    nonlocals.extend(self.scan_nonlocals(&if_stmt.body));
                    nonlocals.extend(self.scan_nonlocals(&if_stmt.orelse));
                }
                Stmt::Try(try_stmt) => {
                    nonlocals.extend(self.scan_nonlocals(&try_stmt.body));
                    for handler in &try_stmt.handlers {
                        if let ast::ExceptHandler::ExceptHandler(h) = handler {
                            nonlocals.extend(self.scan_nonlocals(&h.body));
                        }
                    }
                    nonlocals.extend(self.scan_nonlocals(&try_stmt.orelse));
                    nonlocals.extend(self.scan_nonlocals(&try_stmt.finalbody));
                }
                // Don't recurse into nested function/class defs
                _ => {}
            }
        }
        nonlocals
    }

    fn compile_lambda(&mut self, lambda: &ast::ExprLambda) -> Result<(), String> {
        let filename = self.code_ref().filename.clone();

        // No defaults for simplicity (could add later)
        let mut func_code = CodeObject::new("<lambda>".to_string(), filename);
        for arg in &lambda.args.args {
            func_code.add_varname(&arg.def.arg.to_string());
        }
        func_code.argcount = lambda.args.args.len() as u32;

        self.code_stack.push(func_code);
        self.compile_expr(&lambda.body)?;
        self.emit(OpCode::ReturnValue, 0);
        let inner_code = self.code_stack.pop().unwrap();

        let code_box = Box::new(inner_code);
        let code_ptr = Box::into_raw(code_box) as usize;
        let code_marker = safe_api::new_int(self.py, code_ptr as i64)
            .map_err(|e| format!("Allocation error: {}", e))?;
        let code_idx = self.code().add_const(code_marker);
        self.emit(OpCode::LoadConst, code_idx);
        self.emit(OpCode::MakeFunction, 0);
        Ok(())
    }

    fn compile_class_def(&mut self, class_def: &ast::StmtClassDef) -> Result<(), String> {
        let class_name = class_def.name.to_string();

        // Emit LoadBuildClass
        self.emit(OpCode::LoadBuildClass, 0);

        // Compile class body as a function
        let filename = self.code_ref().filename.clone();
        let mut class_code = CodeObject::new(class_name.clone(), filename);
        // Class body function takes no args but gets __locals__ as the namespace
        self.code_stack.push(class_code);
        self.compile_body(&class_def.body)?;
        let none_idx = self.add_none_const();
        self.emit(OpCode::LoadConst, none_idx);
        self.emit(OpCode::ReturnValue, 0);
        let inner_code = self.code_stack.pop().unwrap();

        let code_box = Box::new(inner_code);
        let code_ptr = Box::into_raw(code_box) as usize;
        let code_marker = safe_api::new_int(self.py, code_ptr as i64)
            .map_err(|e| format!("Allocation error: {}", e))?;
        let code_idx = self.code().add_const(code_marker);
        self.emit(OpCode::LoadConst, code_idx);
        self.emit(OpCode::MakeFunction, 0); // class body function

        // Push class name
        let name_idx = self.add_const_result(safe_api::new_str(self.py, &class_name))?;
        self.emit(OpCode::LoadConst, name_idx);

        // Push base classes
        let n_bases = class_def.bases.len();
        for base in &class_def.bases {
            self.compile_expr(base)?;
        }

        // Check for metaclass keyword
        let has_metaclass = class_def.keywords.iter().any(|kw| {
            kw.arg.as_ref().map_or(false, |a| a.to_string() == "metaclass")
        });

        if has_metaclass {
            // CallFunctionKW with metaclass keyword
            for kw in &class_def.keywords {
                self.compile_expr(&kw.value)?;
            }
            let mut kw_names = Vec::new();
            for kw in &class_def.keywords {
                if let Some(ref name) = kw.arg {
                    let s = safe_api::new_str(self.py, &name.to_string())
                        .map_err(|e| format!("Allocation error: {}", e))?;
                    kw_names.push(s);
                }
            }
            let kw_tuple = safe_api::build_tuple(self.py, kw_names)
                .map_err(|e| format!("Allocation error: {}", e))?;
            let kw_idx = self.code().add_const(kw_tuple);
            self.emit(OpCode::LoadConst, kw_idx);
            let total_args = (2 + n_bases + class_def.keywords.len()) as u32;
            self.emit(OpCode::CallFunctionKW, total_args);
        } else {
            // Simple call: __build_class__(body_func, name, *bases)
            let total_args = (2 + n_bases) as u32;
            self.emit(OpCode::CallFunction, total_args);
        }

        // Apply decorators
        for decorator in class_def.decorator_list.iter().rev() {
            self.compile_expr(decorator)?;
            self.emit(OpCode::RotTwo, 0);
            self.emit(OpCode::CallFunction, 1);
        }

        // Store the class
        let store_idx = self.code().add_name(&class_name);
        self.emit(OpCode::StoreName, store_idx);

        Ok(())
    }

    fn compile_try(&mut self, try_stmt: &ast::StmtTry) -> Result<(), String> {
        if !try_stmt.finalbody.is_empty() {
            // Setup finally handler
            let setup_finally = self.current_offset();
            self.emit(OpCode::SetupFinally, 0);

            if !try_stmt.handlers.is_empty() {
                self.compile_try_except_body(try_stmt)?;
            } else {
                self.compile_body(&try_stmt.body)?;
            }

            self.emit(OpCode::PopBlock, 0); // pop finally block

            // Execute finally body (normal path)
            self.compile_body(&try_stmt.finalbody)?;
            let jump_past = self.current_offset();
            self.emit(OpCode::JumpAbsolute, 0);

            // Finally handler (exception path)
            let finally_start = self.current_offset();
            self.patch_jump(setup_finally, finally_start);
            self.compile_body(&try_stmt.finalbody)?;
            self.emit(OpCode::EndFinally, 0);

            let end = self.current_offset();
            self.patch_jump(jump_past, end);
        } else {
            self.compile_try_except_body(try_stmt)?;
        }

        Ok(())
    }

    fn compile_try_except_body(&mut self, try_stmt: &ast::StmtTry) -> Result<(), String> {
        let setup_except = self.current_offset();
        self.emit(OpCode::SetupExcept, 0); // jump to handler on exception

        self.compile_body(&try_stmt.body)?;
        self.emit(OpCode::PopBlock, 0); // pop except block

        // Compile else clause (only if no exception)
        if !try_stmt.orelse.is_empty() {
            self.compile_body(&try_stmt.orelse)?;
        }

        let jump_past_handlers = self.current_offset();
        self.emit(OpCode::JumpAbsolute, 0); // skip handlers on success

        // Exception handler entry point
        let handler_start = self.current_offset();
        self.patch_jump(setup_except, handler_start);

        // Compile each except handler
        let mut handler_end_jumps = Vec::new();
        for (i, handler) in try_stmt.handlers.iter().enumerate() {
            if let ast::ExceptHandler::ExceptHandler(h) = handler {
                if let Some(ref exc_type) = h.type_ {
                    // Typed handler: except SomeException [as name]:
                    // DupTop the exception, compare with the type
                    self.emit(OpCode::DupTop, 0);
                    self.compile_expr(exc_type)?;
                    // Use CompareOp with special "exception match" mode
                    self.emit(OpCode::CompareOp, 10); // 10 = exception match

                    let jump_to_next = self.current_offset();
                    self.emit(OpCode::PopJumpIfFalse, 0);

                    // Match! Bind exception if needed
                    if let Some(ref name) = h.name {
                        let name_str = name.to_string();
                        let varname_idx = self.code_ref().varnames.iter().position(|n| n == &name_str);
                        if let Some(idx) = varname_idx {
                            self.emit(OpCode::StoreFast, idx as u32);
                        } else {
                            let idx = self.code().add_name(&name_str);
                            self.emit(OpCode::StoreName, idx);
                        }
                    } else {
                        self.emit(OpCode::PopTop, 0); // discard exception
                    }
                    self.emit(OpCode::PopExcept, 0);

                    self.compile_body(&h.body)?;

                    let end_jump = self.current_offset();
                    self.emit(OpCode::JumpAbsolute, 0);
                    handler_end_jumps.push(end_jump);

                    let next = self.current_offset();
                    self.patch_jump(jump_to_next, next);
                } else {
                    // Bare except: catches everything
                    if let Some(ref name) = h.name {
                        let name_str = name.to_string();
                        let idx = self.code().add_name(&name_str);
                        self.emit(OpCode::StoreName, idx);
                    } else {
                        self.emit(OpCode::PopTop, 0);
                    }
                    self.emit(OpCode::PopExcept, 0);

                    self.compile_body(&h.body)?;

                    let end_jump = self.current_offset();
                    self.emit(OpCode::JumpAbsolute, 0);
                    handler_end_jumps.push(end_jump);
                }
            }
        }

        // If no handler matched, re-raise
        self.emit(OpCode::RaiseVarargs, 1); // re-raise with TOS

        let end = self.current_offset();
        self.patch_jump(jump_past_handlers, end);
        for jump in handler_end_jumps {
            self.patch_jump(jump, end);
        }

        Ok(())
    }

    fn compile_raise(&mut self, raise_stmt: &ast::StmtRaise) -> Result<(), String> {
        if let Some(ref exc) = raise_stmt.exc {
            self.compile_expr(exc)?;
            self.emit(OpCode::RaiseVarargs, 1);
        } else {
            self.emit(OpCode::RaiseVarargs, 0); // re-raise current
        }
        Ok(())
    }

    fn compile_assert(&mut self, assert_stmt: &ast::StmtAssert) -> Result<(), String> {
        self.compile_expr(&assert_stmt.test)?;
        let jump_over = self.current_offset();
        self.emit(OpCode::PopJumpIfTrue, 0);

        // Raise AssertionError
        let exc_name = self.code().add_name("AssertionError");
        self.emit(OpCode::LoadName, exc_name);

        if let Some(ref msg) = assert_stmt.msg {
            self.compile_expr(msg)?;
            self.emit(OpCode::CallFunction, 1);
        }
        self.emit(OpCode::RaiseVarargs, 1);

        let end = self.current_offset();
        self.patch_jump(jump_over, end);
        Ok(())
    }

    fn compile_import(&mut self, import: &ast::StmtImport) -> Result<(), String> {
        for alias in &import.names {
            let module_name = alias.name.to_string();
            let name_idx = self.code().add_name(&module_name);
            self.emit(OpCode::ImportName, name_idx);
            let store_name = if let Some(ref asname) = alias.asname {
                asname.to_string()
            } else {
                // For "import a.b.c", store as "a"
                module_name.split('.').next().unwrap_or(&module_name).to_string()
            };
            let store_idx = self.code().add_name(&store_name);
            self.emit(OpCode::StoreName, store_idx);
        }
        Ok(())
    }

    fn compile_import_from(&mut self, import_from: &ast::StmtImportFrom) -> Result<(), String> {
        let module_name = import_from.module.as_ref()
            .map(|m| m.to_string())
            .unwrap_or_default();

        let name_idx = self.code().add_name(&module_name);
        self.emit(OpCode::ImportName, name_idx);

        for alias in &import_from.names {
            let attr_name = alias.name.to_string();
            if attr_name == "*" {
                self.emit(OpCode::ImportStar, 0);
            } else {
                let attr_idx = self.code().add_name(&attr_name);
                self.emit(OpCode::ImportFrom, attr_idx);
                let store_name = if let Some(ref asname) = alias.asname {
                    asname.to_string()
                } else {
                    attr_name
                };
                let store_idx = self.code().add_name(&store_name);
                self.emit(OpCode::StoreName, store_idx);
            }
        }
        // Pop the module from stack (ImportName left it there)
        self.emit(OpCode::PopTop, 0);

        Ok(())
    }

    fn compile_list_comp(&mut self, comp: &ast::ExprListComp) -> Result<(), String> {
        // Build an empty list, iterate, append
        self.emit(OpCode::BuildList, 0);
        let total_depth = comp.generators.len() as u32 + 1;
        self.compile_comprehension_generators(&comp.generators, &comp.elt, OpCode::ListAppend, total_depth)?;
        Ok(())
    }

    fn compile_set_comp(&mut self, comp: &ast::ExprSetComp) -> Result<(), String> {
        self.emit(OpCode::BuildSet, 0);
        let total_depth = comp.generators.len() as u32 + 1;
        self.compile_comprehension_generators(&comp.generators, &comp.elt, OpCode::SetAdd, total_depth)?;
        Ok(())
    }

    fn compile_dict_comp(&mut self, comp: &ast::ExprDictComp) -> Result<(), String> {
        self.emit(OpCode::BuildMap, 0);
        // For dict comp we need special handling — compile key and value
        let gen = &comp.generators[0];
        self.compile_expr(&gen.iter)?;
        self.emit(OpCode::GetIter, 0);
        let loop_start = self.current_offset();
        let for_iter = self.current_offset();
        self.emit(OpCode::ForIter, 0);
        self.compile_store_target(&gen.target)?;
        // Compile filters
        let mut end_jumps = Vec::new();
        for cond in &gen.ifs {
            self.compile_expr(cond)?;
            let jump = self.current_offset();
            self.emit(OpCode::PopJumpIfFalse, 0);
            end_jumps.push(jump);
        }
        self.compile_expr(&comp.key)?;
        self.compile_expr(&comp.value)?;
        self.emit(OpCode::MapAdd, 2); // depth=2 (dict is 2 below TOS)
        for j in &end_jumps {
            let target = self.current_offset();
            // Actually these should jump to loop continuation, not end
        }
        self.emit(OpCode::JumpAbsolute, loop_start);
        let end = self.current_offset();
        self.patch_jump(for_iter, end);
        for j in end_jumps {
            self.patch_jump(j, loop_start); // skip this iteration
        }
        Ok(())
    }

    fn compile_generator_exp(&mut self, genexp: &ast::ExprGeneratorExp) -> Result<(), String> {
        // For now, compile generator expressions as list comprehensions
        self.emit(OpCode::BuildList, 0);
        let total_depth = genexp.generators.len() as u32 + 1;
        self.compile_comprehension_generators(&genexp.generators, &genexp.elt, OpCode::ListAppend, total_depth)?;
        // Convert list to iterator
        self.emit(OpCode::GetIter, 0);
        Ok(())
    }

    fn compile_comprehension_generators(
        &mut self,
        generators: &[ast::Comprehension],
        elt: &Expr,
        append_op: OpCode,
        total_depth: u32,
    ) -> Result<(), String> {
        if generators.is_empty() {
            // Base case: compile the element and append
            self.compile_expr(elt)?;
            self.emit(append_op, total_depth);
            return Ok(());
        }

        let gen = &generators[0];
        self.compile_expr(&gen.iter)?;
        self.emit(OpCode::GetIter, 0);
        let loop_start = self.current_offset();
        let for_iter = self.current_offset();
        self.emit(OpCode::ForIter, 0);
        self.compile_store_target(&gen.target)?;

        // Compile filter conditions
        let mut skip_jumps = Vec::new();
        for cond in &gen.ifs {
            self.compile_expr(cond)?;
            let jump = self.current_offset();
            self.emit(OpCode::PopJumpIfFalse, 0);
            skip_jumps.push(jump);
        }

        if generators.len() > 1 {
            self.compile_comprehension_generators(&generators[1..], elt, append_op, total_depth)?;
        } else {
            self.compile_expr(elt)?;
            self.emit(append_op, total_depth);
        }

        // Patch skip jumps to continue loop
        let continue_target = self.current_offset();
        for j in skip_jumps {
            self.patch_jump(j, continue_target);
        }

        self.emit(OpCode::JumpAbsolute, loop_start);
        let end = self.current_offset();
        self.patch_jump(for_iter, end);
        Ok(())
    }
}

fn cmpop_to_arg(op: &ast::CmpOp) -> u32 {
    match op {
        ast::CmpOp::Lt => 0,
        ast::CmpOp::LtE => 1,
        ast::CmpOp::Eq => 2,
        ast::CmpOp::NotEq => 3,
        ast::CmpOp::Gt => 4,
        ast::CmpOp::GtE => 5,
        ast::CmpOp::Is => 6,
        ast::CmpOp::IsNot => 7,
        ast::CmpOp::In => 8,
        ast::CmpOp::NotIn => 9,
    }
}
