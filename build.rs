use std::env;
use std::path::PathBuf;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    // Compile C source files that implement variadic functions
    cc::Build::new()
        .file("csrc/varargs.c")
        .warnings(false)
        .compile("varargs");

    // Force-load the C static library so symbols aren't stripped
    let lib_path = PathBuf::from(&out_dir).join("libvarargs.a");
    println!("cargo:rustc-cdylib-link-arg=-Wl,-force_load,{}", lib_path.display());

    // Also create an export list file so the linker keeps C symbols visible
    let exports_path = PathBuf::from(&out_dir).join("extra_exports.txt");
    std::fs::write(&exports_path,
        "_PyArg_ParseTuple\n\
         _PyArg_ParseTupleAndKeywords\n\
         _PyArg_UnpackTuple\n\
         _Py_BuildValue\n\
         __Py_BuildValue_SizeT\n\
         _Py_VaBuildValue\n"
    ).unwrap();
    println!("cargo:rustc-cdylib-link-arg=-Wl,-exported_symbols_list,{}", exports_path.display());
}
