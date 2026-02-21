# Rustthon

A CPython 3.11 ABI-compatible Python interpreter written in Rust.
Runs real C extensions from PyPI — including prebuilt binary wheels compiled against CPython 3.11 — without modification.

## What's Running Right Now

The interpreter successfully executes Python code with:

| Feature | Status |
|---------|--------|
| Arithmetic (`+`, `-`, `*`, `/`, `//`, `%`, `**`) | Working |
| Variables & assignment | Working |
| Strings (creation, concatenation, `len`) | Working |
| Booleans (`True`, `False`, `and`, `or`, `not`) | Working |
| Comparisons (`<`, `<=`, `==`, `!=`, `>`, `>=`, `is`) | Working |
| Lists (creation, append, concatenation, indexing) | Working |
| Tuples | Working |
| Dicts (creation, set/get) | Working |
| Sets | Working |
| `if`/`elif`/`else` | Working |
| `while` loops | Working |
| `print()`, `len()`, `range()`, `type()`, `int()`, `str()` | Working |
| REPL mode | Working |

## C Extension Compatibility

Rustthon loads and runs real-world C extensions from PyPI. This works in two modes:

1. **Source compilation** — Extensions compiled against Rustthon's own `include/Python.h` header, linked to `librustthon.dylib`.
2. **Prebuilt binary wheels** — Extensions compiled against real CPython 3.11 (pip wheels), loaded at runtime via `dlopen` with no recompilation.

| Extension | Self-Built | Prebuilt Wheel |
|-----------|------------|----------------|
| markupsafe 3.0.3 | 18/18 pass | 18/18 pass |
| ujson 5.11.0 | 48/48 pass | 50/50 pass |

The prebuilt wheel tests use `.so` files extracted directly from pip wheels (`cp311-cp311-macosx_11_0_arm64`). These were compiled by their upstream projects against CPython 3.11 headers — Rustthon was not involved in their compilation.

## CPython 3.11 ABI Compatibility

250+ exported C API symbols. Every built-in type matches CPython 3.11 byte-for-byte in memory layout, verified by a C test suite that directly reads struct internals through pointer arithmetic.

**Type layouts:**

| Type | Layout |
|------|--------|
| `int` | `PyLongObject` (24B header + u32 digit array), 30-bit digits, sign in `ob_size` |
| `float` | `PyFloatObject` (16B `ob_base` + 8B `ob_fval` = 24B) |
| `bool` | `PyLongObject` subtype, `True`/`False` as static `_Py_TrueStruct`/`_Py_FalseStruct` |
| `str` | `PyASCIIObject` (48B) for ASCII, `PyCompactUnicodeObject` (72B) for non-ASCII |
| `bytes` | `PyBytesObject` (32B header + inline `ob_sval[N+1]`) |
| `list` | `PyListObject` (40B: `ob_base` + `ob_item` + `allocated`), GC-tracked |
| `tuple` | `PyTupleObject` (24B header + inline items), GC-tracked |
| `dict` | `PyDictObject` (48B) + `PyDictKeysObject` compact hash table, GC-tracked |
| `set` | `PySetObject` (200B with inline `smalltable[8]`), GC-tracked |

Type objects are exported as actual ~400-byte `RawPyTypeObject` structs (DATA symbols), not pointers — matching what prebuilt extensions expect when they reference `PyLong_Type`, `PyFloat_Type`, etc.

**Infrastructure:**

- `PyType_Type` and `PyBaseObject_Type` metaclass hierarchy
- `PyType_Ready` with real slot inheritance from base types
- Exception hierarchy as real `PyTypeObject` instances with `tp_base` chains
- 16-byte `PyGC_Head` prepended before all GC-tracked objects
- All allocation via `libc::calloc`/`malloc` (not `std::alloc`)
- Three-tier allocator: `PyMem_Raw*`, `PyMem_*`, `PyObject_*`
- GIL emulation via `parking_lot::Mutex`
- Refcounting with `AtomicIsize` (Release/Acquire semantics)

## C Varargs Shim (`csrc/varargs.c`)

`PyArg_ParseTuple`, `Py_BuildValue`, and friends are variadic C functions (they use `...` and `va_list`). Rust stable cannot define these because `core::ffi::VaList` / the `c_variadic` feature is still nightly-only (as of Rust 1.90). This is a fundamental language limitation — Rust can *call* variadic C functions but cannot *implement* them.

**The solution:** these functions are written in plain C (`csrc/varargs.c`) and compiled into the Rust library via the `cc` crate in `build.rs`.

How it works:

1. `csrc/varargs.c` implements `PyArg_ParseTuple`, `PyArg_ParseTupleAndKeywords`, `PyArg_UnpackTuple`, `Py_BuildValue`, and `Py_VaBuildValue` using standard C `va_list`/`va_arg`.
2. The C code calls back into Rustthon's own exported C API (`PyTuple_GetItem`, `PyLong_AsLong`, `PyUnicode_AsUTF8`, `PyLong_FromLong`, etc.) to do the actual work of extracting and creating Python objects.
3. `build.rs` uses the `cc` crate to compile `varargs.c` into a static library, then passes `-Wl,-force_load` and `-Wl,-exported_symbols_list` to the macOS linker to ensure the C symbols survive LTO and appear in the final `librustthon.dylib`.

**Supported format characters:**

| Function | Formats |
|----------|---------|
| `PyArg_ParseTuple` | `s` `s#` `z` `y` `y#` `i` `l` `n` `f` `d` `O` `O!` `p` `S` `U` `\|` `:` `;` |
| `Py_BuildValue` | `s` `s#` `y` `y#` `i` `l` `n` `f` `d` `O` `N` `()` `[]` `{}` |

## Prebuilt Extension Loading (macOS)

Loading prebuilt CPython extensions on macOS requires special handling due to the two-level namespace. The host process must load `librustthon.dylib` with `RTLD_GLOBAL | RTLD_LAZY` to force all exported symbols into the flat namespace. Without `RTLD_GLOBAL`, macOS isolates symbol namespaces and the prebuilt `.so` cannot resolve Rustthon's C API symbols.

```c
void *rt = dlopen("librustthon.dylib", RTLD_GLOBAL | RTLD_LAZY);
Py_Initialize();
void *ext = dlopen("ujson.cpython-311-darwin.so", RTLD_LAZY);
```

## File Structure

```
src/
├── lib.rs              # Crate root
├── main.rs             # REPL + file execution entry point
├── object/
│   ├── pyobject.rs     # RawPyObject, RawPyVarObject, PyGCHead, PyObjectWithData<T>
│   ├── typeobj.rs      # RawPyTypeObject, PyType_Type, PyBaseObject_Type, PyType_Ready
│   ├── refcount.rs     # Py_IncRef/DecRef exports
│   └── gc.rs           # GC allocation (_PyObject_GC_New) and tracking
├── types/
│   ├── none.rs         # None singleton (_Py_NoneStruct)
│   ├── boolobject.rs   # Bool as int subtype, _Py_TrueStruct/_Py_FalseStruct statics
│   ├── longobject.rs   # int (30-bit digit arrays, CPython layout)
│   ├── floatobject.rs  # float (ob_fval at offset 16)
│   ├── unicode.rs      # str (three-tier: ASCII compact / non-ASCII compact)
│   ├── bytes.rs        # bytes (inline ob_sval after 32B header)
│   ├── list.rs         # list (ob_item pointer + allocated capacity)
│   ├── tuple.rs        # tuple (inline items after 24B header)
│   ├── dict.rs         # dict (compact hash table, insertion-ordered)
│   ├── set.rs          # set (open addressing, inline smalltable[8])
│   ├── moduleobject.rs # module (PyModuleDef support)
│   └── funcobject.rs   # PyCFunction wrapper + dispatch
├── runtime/
│   ├── memory.rs       # PyMem_Malloc/Free, PyObject_Init
│   ├── error.rs        # Exception hierarchy (real PyTypeObject instances)
│   ├── thread_state.rs # PyThreadState, PyInterpreterState
│   ├── gil.rs          # GIL emulation (Mutex-based)
│   └── interp.rs       # Py_Initialize/Finalize
├── compiler/
│   ├── bytecode.rs     # OpCode enum, CodeObject, Instruction
│   └── compile.rs      # AST -> bytecode compiler
├── vm/
│   ├── frame.rs        # Execution frame (stack, locals, globals)
│   └── interpreter.rs  # Main eval loop + builtin functions
├── ffi/
│   ├── object_api.rs   # PyObject_* generic protocol
│   ├── buffer.rs       # Buffer protocol (Py_buffer)
│   ├── import.rs       # dlopen-based C extension loading
│   └── arg_parse.rs    # Extern declarations for C varargs symbols
└── module/
    └── registry.rs     # sys.modules equivalent

csrc/
└── varargs.c           # C implementations of variadic API functions

include/
└── Python.h            # CPython 3.11 compatible header for extension compilation

build.rs                # cc crate build script for varargs.c
```

## Test Suites

| Test Suite | Tests | What it verifies |
|------------|-------|------------------|
| `test_abi.c` | 97 | Struct layouts at byte offsets |
| `test_gc_torture.c` | 99 | GC headers, cycles, allocator |
| `test_ext_driver.c` | 49 | Full C API protocol |
| `test_markupsafe.c` | 18 | Self-compiled markupsafe |
| `test_ujson.c` | 48 | Self-compiled ujson |
| `test_prebuilt.c` | 68 | Prebuilt pip wheel `.so` files |
| **TOTAL** | **379** | |

### Build & Run Tests

```bash
cargo build --release

# ABI + GC + protocol tests (link directly)
cc -o test_abi tests/test_abi.c -L target/release -lrustthon -Wl,-rpath,target/release
cc -o test_gc tests/test_gc_torture.c -L target/release -lrustthon -Wl,-rpath,target/release
cc -o test_ext tests/test_ext_driver.c -L target/release -lrustthon -Wl,-rpath,target/release

# Self-compiled extension tests (link directly)
cc -o test_markupsafe tests/test_markupsafe.c -L target/release -lrustthon -Wl,-rpath,target/release
cc -o test_ujson tests/test_ujson.c -L target/release -lrustthon -Wl,-rpath,target/release

# Prebuilt wheel tests (dlopen, no linking)
cc -o test_prebuilt tests/test_prebuilt.c -ldl
```
