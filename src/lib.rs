//! Rustthon: A CPython-extension compatible Python interpreter in Rust.
//!
//! This crate provides a Python 3.x interpreter that can load and run
//! native CPython C extensions (.so/.dylib) by faithfully replicating
//! CPython's C ABI and memory layout.

pub mod object;
pub mod types;
pub mod runtime;
pub mod compiler;
pub mod vm;
pub mod ffi;
pub mod module;

// Re-export core types for convenience
pub use object::pyobject::{PyObject, PyObjectRef, RawPyObject};
pub use object::typeobj::RawPyTypeObject;
pub use runtime::memory;
pub use runtime::thread_state;
pub use runtime::error;

// ─── Entry point (called from the thin binary shim) ───

/// Main entry point for the Rustthon interpreter.
/// The thin binary shim dlopen's librustthon.dylib and calls this.
/// This ensures there is exactly ONE copy of all global state (type objects,
/// singletons, etc.), eliminating the binary/dylib split-brain problem.
#[no_mangle]
pub extern "C" fn rustthon_main(argc: i32, argv: *const *const std::os::raw::c_char) -> i32 {
    // Collect args
    let args: Vec<String> = if argc > 0 && !argv.is_null() {
        (0..argc as usize)
            .map(|i| unsafe {
                let ptr = *argv.add(i);
                if ptr.is_null() {
                    String::new()
                } else {
                    std::ffi::CStr::from_ptr(ptr)
                        .to_string_lossy()
                        .into_owned()
                }
            })
            .collect()
    } else {
        vec![String::from("rustthon")]
    };

    // Initialize the interpreter
    runtime::interp::initialize();

    let exit_code = if args.len() < 2 {
        run_repl();
        0
    } else if args[1] == "-c" && args.len() > 2 {
        execute_code(&args[2], "<string>")
    } else {
        let filename = &args[1];
        match std::fs::read_to_string(filename) {
            Ok(source) => execute_code(&source, filename),
            Err(e) => {
                eprintln!("Error reading {}: {}", filename, e);
                1
            }
        }
    };

    runtime::interp::finalize();
    exit_code
}

fn run_repl() {
    use std::io::{self, BufRead, Write};

    println!("Rustthon 0.1.0 (CPython 3.11 compatible runtime)");
    println!("Built with Rust — Type \"exit()\" to quit.");
    println!();

    let stdin = io::stdin();
    let mut vm = vm::interpreter::VM::new();

    loop {
        print!(">>> ");
        io::stdout().flush().unwrap();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break,
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

        match compiler::compile::compile_source(line, "<stdin>") {
            Ok(code) => {
                match vm.execute(code) {
                    Ok(result) => {
                        unsafe {
                            if !result.is_null()
                                && !types::none::is_none(result)
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

fn execute_code(source: &str, filename: &str) -> i32 {
    match compiler::compile::compile_source(source, filename) {
        Ok(code) => {
            let mut vm = vm::interpreter::VM::new();
            match vm.execute(code) {
                Ok(_) => 0,
                Err(e) => {
                    eprintln!("Traceback (most recent call last):");
                    eprintln!("  File \"{}\"", filename);
                    eprintln!("{}", e);
                    1
                }
            }
        }
        Err(e) => {
            eprintln!("  File \"{}\"", filename);
            eprintln!("    {}", e);
            1
        }
    }
}

unsafe fn print_object(obj: *mut object::pyobject::RawPyObject) {
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
        let repr = ffi::object_api::PyObject_Repr(obj);
        if !repr.is_null() {
            let s = types::unicode::unicode_value(repr);
            println!("{}", s);
            (*repr).decref();
        }
    }
}
