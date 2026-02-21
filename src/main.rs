//! Rustthon — a CPython-extension compatible Python interpreter in Rust.
//!
//! Usage:
//!   rustthon                  # Interactive REPL
//!   rustthon script.py        # Execute a Python file
//!   rustthon -c "code"        # Execute a string of Python code

use std::io::{self, BufRead, Write};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Initialize the interpreter
    rustthon::runtime::interp::initialize();

    if args.len() < 2 {
        // REPL mode
        run_repl();
    } else if args[1] == "-c" && args.len() > 2 {
        // Execute code string
        let code = &args[2];
        execute_code(code, "<string>");
    } else {
        // Execute file
        let filename = &args[1];
        match std::fs::read_to_string(filename) {
            Ok(source) => {
                execute_code(&source, filename);
            }
            Err(e) => {
                eprintln!("Error reading {}: {}", filename, e);
                std::process::exit(1);
            }
        }
    }

    rustthon::runtime::interp::finalize();
}

fn run_repl() {
    println!("Rustthon 0.1.0 (CPython 3.11 compatible runtime)");
    println!("Built with Rust — Type \"exit()\" to quit.");
    println!();

    let stdin = io::stdin();
    let mut vm = rustthon::vm::interpreter::VM::new();

    loop {
        print!(">>> ");
        io::stdout().flush().unwrap();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error reading input: {}", e);
                break;
            }
        }

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if line == "exit()" || line == "quit()" {
            break;
        }

        // Try to compile and execute
        match rustthon::compiler::compile::compile_source(line, "<stdin>") {
            Ok(code) => {
                match vm.execute(code) {
                    Ok(result) => {
                        unsafe {
                            // Print result if it's not None
                            if !result.is_null()
                                && !rustthon::types::none::is_none(result)
                            {
                                print_object(result);
                                (*result).decref();
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("{}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("  {}", e);
            }
        }
    }
}

fn execute_code(source: &str, filename: &str) {
    match rustthon::compiler::compile::compile_source(source, filename) {
        Ok(code) => {
            let mut vm = rustthon::vm::interpreter::VM::new();
            match vm.execute(code) {
                Ok(_result) => {
                    // Normal completion
                }
                Err(e) => {
                    eprintln!("Traceback (most recent call last):");
                    eprintln!("  File \"{}\"", filename);
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("  File \"{}\"", filename);
            eprintln!("    {}", e);
            std::process::exit(1);
        }
    }
}

unsafe fn print_object(obj: *mut rustthon::object::pyobject::RawPyObject) {
    use rustthon::types;

    if obj.is_null() {
        return;
    }

    if types::boolobject::is_bool(obj) {
        if types::boolobject::is_true(obj) {
            println!("True");
        } else {
            println!("False");
        }
    } else if (*obj).ob_type == types::longobject::long_type() {
        let val = types::longobject::long_value(obj);
        println!("{}", val);
    } else if (*obj).ob_type == types::floatobject::float_type() {
        let val = types::floatobject::float_value(obj);
        println!("{}", val);
    } else if (*obj).ob_type == types::unicode::unicode_type() {
        let val = types::unicode::unicode_value(obj);
        println!("'{}'", val);
    } else {
        let repr = rustthon::ffi::object_api::PyObject_Repr(obj);
        if !repr.is_null() {
            let s = types::unicode::unicode_value(repr);
            println!("{}", s);
            (*repr).decref();
        }
    }
}
