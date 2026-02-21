//! AST -> Bytecode compiler.
//!
//! Takes a Python AST (from rustpython-parser) and compiles it
//! into our bytecode format.

use crate::compiler::bytecode::{CodeObject, OpCode};
use crate::object::pyobject::RawPyObject;
use rustpython_parser::ast::{self, Constant, Expr, Stmt};
use rustpython_parser::Parse;

/// Compile Python source code into a CodeObject.
pub fn compile_source(source: &str, filename: &str) -> Result<CodeObject, String> {
    // Parse the source into an AST
    let ast = ast::Suite::parse(source, filename)
        .map_err(|e| format!("Parse error: {}", e))?;

    let mut compiler = Compiler::new(filename.to_string());
    compiler.compile_body(&ast)?;

    // Add implicit return None at the end
    let none_idx = compiler.add_none_const();
    compiler.code.emit(OpCode::LoadConst, none_idx);
    compiler.code.emit(OpCode::ReturnValue, 0);

    Ok(compiler.code)
}

struct Compiler {
    code: CodeObject,
}

impl Compiler {
    fn new(filename: String) -> Self {
        Compiler {
            code: CodeObject::new("<module>".to_string(), filename),
        }
    }

    fn add_none_const(&mut self) -> u32 {
        unsafe {
            let none = crate::types::none::PY_NONE.get();
            self.code.add_const(none)
        }
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
                self.code.emit(OpCode::PrintExpr, 0);
                self.code.emit(OpCode::PopTop, 0);
            }

            Stmt::Assign(assign) => {
                self.compile_expr(&assign.value)?;
                for target in &assign.targets {
                    self.compile_store_target(target)?;
                }
            }

            Stmt::AugAssign(aug) => {
                // x += 1 => x = x + 1
                self.compile_expr(&aug.target)?;
                self.compile_expr(&aug.value)?;
                let opcode = match aug.op {
                    ast::Operator::Add => OpCode::InplaceAdd,
                    ast::Operator::Sub => OpCode::InplaceSubtract,
                    ast::Operator::Mult => OpCode::InplaceMultiply,
                    _ => OpCode::BinaryAdd, // fallback
                };
                self.code.emit(opcode, 0);
                self.compile_store_target(&aug.target)?;
            }

            Stmt::Return(ret) => {
                if let Some(ref value) = ret.value {
                    self.compile_expr(value)?;
                } else {
                    let idx = self.add_none_const();
                    self.code.emit(OpCode::LoadConst, idx);
                }
                self.code.emit(OpCode::ReturnValue, 0);
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

            Stmt::Pass(_) => {
                // No-op
            }

            Stmt::Break(_) => {
                self.code.emit(OpCode::BreakLoop, 0);
            }

            Stmt::Continue(_) => {
                self.code.emit(OpCode::ContinueLoop, 0);
            }

            _ => {
                // Unhandled statement types - emit NOP for now
                self.code.emit(OpCode::Nop, 0);
            }
        }
        Ok(())
    }

    fn compile_expr(&mut self, expr: &Expr) -> Result<(), String> {
        match expr {
            // All constants (int, float, str, bool, None) come through ExprConstant
            Expr::Constant(constant) => {
                match &constant.value {
                    Constant::Int(i) => {
                        // BigInt from malachite-bigint
                        let val: i64 = i.try_into().unwrap_or(0);
                        unsafe {
                            let obj = crate::types::longobject::PyLong_FromLong(val as _);
                            let idx = self.code.add_const(obj);
                            self.code.emit(OpCode::LoadConst, idx);
                        }
                    }
                    Constant::Float(f) => {
                        unsafe {
                            let obj = crate::types::floatobject::PyFloat_FromDouble(*f);
                            let idx = self.code.add_const(obj);
                            self.code.emit(OpCode::LoadConst, idx);
                        }
                    }
                    Constant::Complex { real, imag } => {
                        // TODO: Complex number support — emit the real part for now
                        unsafe {
                            let obj = crate::types::floatobject::PyFloat_FromDouble(*real);
                            let idx = self.code.add_const(obj);
                            self.code.emit(OpCode::LoadConst, idx);
                        }
                    }
                    Constant::Str(s) => {
                        unsafe {
                            let obj = crate::types::unicode::create_from_str(s);
                            let idx = self.code.add_const(obj);
                            self.code.emit(OpCode::LoadConst, idx);
                        }
                    }
                    Constant::Bytes(b) => {
                        unsafe {
                            let obj = crate::types::bytes::create_bytes_from_slice(b);
                            let idx = self.code.add_const(obj);
                            self.code.emit(OpCode::LoadConst, idx);
                        }
                    }
                    Constant::Bool(b) => {
                        unsafe {
                            let obj = if *b {
                                crate::types::boolobject::PY_TRUE.get()
                            } else {
                                crate::types::boolobject::PY_FALSE.get()
                            };
                            let idx = self.code.add_const(obj);
                            self.code.emit(OpCode::LoadConst, idx);
                        }
                    }
                    Constant::None => {
                        let idx = self.add_none_const();
                        self.code.emit(OpCode::LoadConst, idx);
                    }
                    Constant::Ellipsis => {
                        // TODO: Ellipsis object
                        let idx = self.add_none_const();
                        self.code.emit(OpCode::LoadConst, idx);
                    }
                    Constant::Tuple(items) => {
                        // Constant tuple — emit each element then build
                        for item in items {
                            self.compile_constant(item)?;
                        }
                        self.code.emit(OpCode::BuildTuple, items.len() as u32);
                    }
                }
            }

            Expr::Name(name) => {
                let idx = self.code.add_name(&name.id.to_string());
                self.code.emit(OpCode::LoadName, idx);
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
                    ast::Operator::MatMult => OpCode::BinaryMultiply, // TODO
                };
                self.code.emit(opcode, 0);
            }

            Expr::UnaryOp(unop) => {
                self.compile_expr(&unop.operand)?;
                let opcode = match unop.op {
                    ast::UnaryOp::Not => OpCode::UnaryNot,
                    ast::UnaryOp::USub => OpCode::UnaryNegative,
                    ast::UnaryOp::UAdd => OpCode::UnaryPositive,
                    ast::UnaryOp::Invert => OpCode::UnaryNot, // TODO: proper invert
                };
                self.code.emit(opcode, 0);
            }

            Expr::Compare(cmp) => {
                self.compile_expr(&cmp.left)?;
                // Handle first comparator (simplified — full impl handles chained)
                if let Some(comparator) = cmp.comparators.first() {
                    self.compile_expr(comparator)?;
                }
                if let Some(op) = cmp.ops.first() {
                    let cmp_op = match op {
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
                    };
                    self.code.emit(OpCode::CompareOp, cmp_op);
                }
            }

            Expr::BoolOp(boolop) => {
                // and/or — short-circuit evaluation
                let values = &boolop.values;
                if values.is_empty() {
                    let idx = self.add_none_const();
                    self.code.emit(OpCode::LoadConst, idx);
                    return Ok(());
                }
                self.compile_expr(&values[0])?;
                for value in &values[1..] {
                    match boolop.op {
                        ast::BoolOp::And => {
                            let jump = self.code.current_offset();
                            self.code.emit(OpCode::JumpIfFalse, 0);
                            self.code.emit(OpCode::PopTop, 0);
                            self.compile_expr(value)?;
                            let end = self.code.current_offset();
                            self.code.patch_jump(jump, end);
                        }
                        ast::BoolOp::Or => {
                            let jump = self.code.current_offset();
                            self.code.emit(OpCode::JumpIfTrue, 0);
                            self.code.emit(OpCode::PopTop, 0);
                            self.compile_expr(value)?;
                            let end = self.code.current_offset();
                            self.code.patch_jump(jump, end);
                        }
                    }
                }
            }

            Expr::Call(call) => {
                self.compile_expr(&call.func)?;
                let nargs = call.args.len() as u32;
                for arg in &call.args {
                    self.compile_expr(arg)?;
                }
                self.code.emit(OpCode::CallFunction, nargs);
            }

            Expr::Attribute(attr) => {
                self.compile_expr(&attr.value)?;
                let idx = self.code.add_name(&attr.attr.to_string());
                self.code.emit(OpCode::LoadAttr, idx);
            }

            Expr::Subscript(sub) => {
                self.compile_expr(&sub.value)?;
                self.compile_expr(&sub.slice)?;
                self.code.emit(OpCode::BinarySubscr, 0);
            }

            Expr::List(list) => {
                let n = list.elts.len() as u32;
                for elt in &list.elts {
                    self.compile_expr(elt)?;
                }
                self.code.emit(OpCode::BuildList, n);
            }

            Expr::Tuple(tuple) => {
                let n = tuple.elts.len() as u32;
                for elt in &tuple.elts {
                    self.compile_expr(elt)?;
                }
                self.code.emit(OpCode::BuildTuple, n);
            }

            Expr::Dict(dict) => {
                let n = dict.keys.len() as u32;
                for (key, value) in dict.keys.iter().zip(dict.values.iter()) {
                    if let Some(k) = key {
                        self.compile_expr(k)?;
                    } else {
                        let idx = self.add_none_const();
                        self.code.emit(OpCode::LoadConst, idx);
                    }
                    self.compile_expr(value)?;
                }
                self.code.emit(OpCode::BuildMap, n);
            }

            Expr::Set(set) => {
                let n = set.elts.len() as u32;
                for elt in &set.elts {
                    self.compile_expr(elt)?;
                }
                self.code.emit(OpCode::BuildSet, n);
            }

            Expr::IfExp(ifexp) => {
                // Ternary: value_if_true if test else value_if_false
                self.compile_expr(&ifexp.test)?;
                let jump_to_else = self.code.current_offset();
                self.code.emit(OpCode::PopJumpIfFalse, 0);
                self.compile_expr(&ifexp.body)?;
                let jump_to_end = self.code.current_offset();
                self.code.emit(OpCode::JumpAbsolute, 0);
                let else_start = self.code.current_offset();
                self.code.patch_jump(jump_to_else, else_start);
                self.compile_expr(&ifexp.orelse)?;
                let end = self.code.current_offset();
                self.code.patch_jump(jump_to_end, end);
            }

            Expr::JoinedStr(fstring) => {
                // f-string — compile each part and concatenate
                // Simplified: just concatenate string parts
                if fstring.values.is_empty() {
                    unsafe {
                        let obj = crate::types::unicode::create_from_str("");
                        let idx = self.code.add_const(obj);
                        self.code.emit(OpCode::LoadConst, idx);
                    }
                } else {
                    self.compile_expr(&fstring.values[0])?;
                    for value in &fstring.values[1..] {
                        self.compile_expr(value)?;
                        self.code.emit(OpCode::BinaryAdd, 0);
                    }
                }
            }

            Expr::FormattedValue(fv) => {
                // Part of an f-string — compile the expression
                self.compile_expr(&fv.value)?;
                // Convert to string
                // In a full impl, we'd handle format_spec and conversion
            }

            _ => {
                // Unhandled expression — push None
                let idx = self.add_none_const();
                self.code.emit(OpCode::LoadConst, idx);
            }
        }
        Ok(())
    }

    /// Compile a constant value (used for constant tuples etc.)
    fn compile_constant(&mut self, constant: &Constant) -> Result<(), String> {
        match constant {
            Constant::Int(i) => {
                let val: i64 = i.try_into().unwrap_or(0);
                unsafe {
                    let obj = crate::types::longobject::PyLong_FromLong(val as _);
                    let idx = self.code.add_const(obj);
                    self.code.emit(OpCode::LoadConst, idx);
                }
            }
            Constant::Float(f) => {
                unsafe {
                    let obj = crate::types::floatobject::PyFloat_FromDouble(*f);
                    let idx = self.code.add_const(obj);
                    self.code.emit(OpCode::LoadConst, idx);
                }
            }
            Constant::Str(s) => {
                unsafe {
                    let obj = crate::types::unicode::create_from_str(s);
                    let idx = self.code.add_const(obj);
                    self.code.emit(OpCode::LoadConst, idx);
                }
            }
            Constant::Bool(b) => {
                unsafe {
                    let obj = if *b {
                        crate::types::boolobject::PY_TRUE.get()
                    } else {
                        crate::types::boolobject::PY_FALSE.get()
                    };
                    let idx = self.code.add_const(obj);
                    self.code.emit(OpCode::LoadConst, idx);
                }
            }
            Constant::None => {
                let idx = self.add_none_const();
                self.code.emit(OpCode::LoadConst, idx);
            }
            _ => {
                let idx = self.add_none_const();
                self.code.emit(OpCode::LoadConst, idx);
            }
        }
        Ok(())
    }

    fn compile_store_target(&mut self, target: &Expr) -> Result<(), String> {
        match target {
            Expr::Name(name) => {
                let idx = self.code.add_name(&name.id.to_string());
                self.code.emit(OpCode::StoreName, idx);
            }
            Expr::Subscript(sub) => {
                self.compile_expr(&sub.value)?;
                self.compile_expr(&sub.slice)?;
                self.code.emit(OpCode::StoreSubscr, 0);
            }
            Expr::Attribute(attr) => {
                self.compile_expr(&attr.value)?;
                let idx = self.code.add_name(&attr.attr.to_string());
                self.code.emit(OpCode::StoreAttr, idx);
            }
            _ => {
                return Err("Unsupported assignment target".to_string());
            }
        }
        Ok(())
    }

    fn compile_if(&mut self, if_stmt: &ast::StmtIf) -> Result<(), String> {
        self.compile_expr(&if_stmt.test)?;

        // Jump to else/end if false
        let jump_to_else = self.code.current_offset();
        self.code.emit(OpCode::PopJumpIfFalse, 0); // Patch later

        // Compile body
        self.compile_body(&if_stmt.body)?;

        if if_stmt.orelse.is_empty() {
            // No else — patch jump to here
            let end = self.code.current_offset();
            self.code.patch_jump(jump_to_else, end);
        } else {
            // Jump over else
            let jump_to_end = self.code.current_offset();
            self.code.emit(OpCode::JumpAbsolute, 0); // Patch later

            // Patch jump-to-else to here
            let else_start = self.code.current_offset();
            self.code.patch_jump(jump_to_else, else_start);

            // Compile else body (elif is represented as nested StmtIf in orelse)
            self.compile_body(&if_stmt.orelse)?;

            let end = self.code.current_offset();
            self.code.patch_jump(jump_to_end, end);
        }
        Ok(())
    }

    fn compile_while(&mut self, while_stmt: &ast::StmtWhile) -> Result<(), String> {
        let loop_start = self.code.current_offset();

        self.compile_expr(&while_stmt.test)?;
        let jump_to_end = self.code.current_offset();
        self.code.emit(OpCode::PopJumpIfFalse, 0);

        self.compile_body(&while_stmt.body)?;
        self.code.emit(OpCode::JumpAbsolute, loop_start);

        let end = self.code.current_offset();
        self.code.patch_jump(jump_to_end, end);
        Ok(())
    }

    fn compile_for(&mut self, for_stmt: &ast::StmtFor) -> Result<(), String> {
        // Compile the iterable
        self.compile_expr(&for_stmt.iter)?;
        self.code.emit(OpCode::GetIter, 0);

        let loop_start = self.code.current_offset();
        let for_iter = self.code.current_offset();
        self.code.emit(OpCode::ForIter, 0); // Jump past body when exhausted

        // Store the loop variable
        self.compile_store_target(&for_stmt.target)?;

        // Compile body
        self.compile_body(&for_stmt.body)?;
        self.code.emit(OpCode::JumpAbsolute, loop_start);

        let end = self.code.current_offset();
        self.code.patch_jump(for_iter, end);
        Ok(())
    }

    fn compile_function_def(&mut self, func_def: &ast::StmtFunctionDef) -> Result<(), String> {
        let func_name = func_def.name.to_string();

        // For now, create a simplified function representation
        // A full implementation would compile a separate CodeObject
        let name_idx = self.code.add_name(&func_name);

        // Push None as a placeholder for the function object
        let idx = self.add_none_const();
        self.code.emit(OpCode::LoadConst, idx);
        self.code.emit(OpCode::MakeFunction, name_idx);
        self.code.emit(OpCode::StoreName, name_idx);

        Ok(())
    }

    fn compile_import(&mut self, import: &ast::StmtImport) -> Result<(), String> {
        for alias in &import.names {
            let module_name = alias.name.to_string();
            let name_idx = self.code.add_name(&module_name);
            self.code.emit(OpCode::ImportName, name_idx);

            // Store as the alias or the module name
            let store_name = if let Some(ref asname) = alias.asname {
                asname.to_string()
            } else {
                module_name
            };
            let store_idx = self.code.add_name(&store_name);
            self.code.emit(OpCode::StoreName, store_idx);
        }
        Ok(())
    }
}
