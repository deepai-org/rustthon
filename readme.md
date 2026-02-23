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
| Dicts (creation, set/get, iteration) | Working |
| Sets | Working |
| `if`/`elif`/`else` | Working |
| `while`/`for` loops | Working |
| Functions, closures, `*args`/`**kwargs` | Working |
| Classes, inheritance, `super()` | Working |
| Generators (`yield`) | Working |
| Comprehensions (list, dict, set) | Working |
| Exception handling (`try`/`except`/`finally`) | Working |
| `import` of Python modules and C extensions | Working |
| `print()`, `len()`, `range()`, `type()`, `int()`, `str()` | Working |
| `isinstance()`, `hasattr()`, `getattr()`, `setattr()` | Working |
| `enumerate()`, `zip()`, `sorted()`, `reversed()` | Working |
| `yaml.safe_load()` via Cython `_yaml` C extension | Working |
| REPL mode | Working |

## Native Import

Rustthon can import and call prebuilt C extensions directly from Python source code:

```python
import ujson
print(ujson.encode({"hello": "world", "n": 42, "pi": 3.14}))
# {"hello":"world","n":42,"pi":3.14}

import yaml
print(yaml.safe_load("hello: world"))
# {'hello': 'world'}
```

The VM's `import` statement finds `.cpython-311-darwin.so` files in site-packages, loads them via `dlopen`, calls `PyInit_<module>`, and makes the resulting module object available for attribute access and function calls. For packages like `yaml`, the VM executes `__init__.py` from source while loading `_yaml` as a native C extension — mixed Python/C packages work seamlessly.

## C Extension Compatibility

Rustthon loads and runs real-world C extensions from PyPI. This works in three modes:

1. **Source compilation** — Extensions compiled against Rustthon's own `include/Python.h` header, linked to `librustthon.dylib`.
2. **Prebuilt binary wheels** — Extensions compiled against real CPython 3.11 (pip wheels), loaded at runtime via `dlopen` with no recompilation.
3. **Native import** — Prebuilt extensions loaded via Python's `import` statement from source code executed by the VM.

| Extension | Type | Tests |
|-----------|------|-------|
| markupsafe 3.0.3 | Self-built | 18/18 pass |
| ujson 5.11.0 | Self-built | 48/48 pass |
| markupsafe 3.0.3 | Prebuilt wheel | 18/18 pass |
| ujson 5.11.0 | Prebuilt wheel | 68/68 pass |
| pyyaml 6.0.2 (Cython) | Prebuilt wheel | 14/14 pass |
| bcrypt (PyO3) | Prebuilt wheel | 10/10 pass |
| ujson 5.11.0 | Native VM import | 9/9 pass |
| yaml (pyyaml) | Native VM import | yaml.safe_load working |

The prebuilt wheel tests use `.so` files extracted directly from pip wheels (`cp311-cp311-macosx_11_0_arm64`). These were compiled by their upstream projects against CPython 3.11 headers — Rustthon was not involved in their compilation.

## Architecture

### Thin Binary Design

The `rustthon` executable is a ~70-line C shim (`csrc/main.c`) that `dlopen`s `librustthon.dylib` and calls `rustthon_main()`. **All** interpreter logic — types, VM, compiler, GC, C API — lives in the dylib.

This design is critical on macOS. When Rust compiles both a binary and cdylib from the same source, static globals (`PyFloat_Type`, `_Py_TrueStruct`, etc.) exist at **different addresses** in each image. C extensions resolve data symbols via `RTLD_DEFAULT`, which returns the binary's addresses, while API function calls go through the dylib. This causes inline type checks like `Py_TYPE(obj) == &PyFloat_Type` to fail silently.

By making the binary a thin shim that immediately enters the dylib, there is exactly **one** copy of every global. No sync, no redirect, no split-brain.

### CPython 3.11 ABI Compatibility

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
- Cyclic garbage collector: `tp_traverse`/`tp_clear` for list, tuple, dict, set; mark-and-sweep cycle detection with three-pass INCREF shield; PEP 442 `tp_finalize` with resurrection detection; `tp_is_gc` dynamic opt-out
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

## File Structure

```
csrc/
├── main.c              # Thin binary shim (dlopen librustthon.dylib → rustthon_main)
└── varargs.c           # C implementations of variadic API functions

src/
├── lib.rs              # Crate root + rustthon_main() entry point
├── object/
│   ├── pyobject.rs     # RawPyObject, RawPyVarObject, PyGCHead, PyObjectWithData<T>
│   ├── typeobj.rs      # RawPyTypeObject, PyType_Type, PyBaseObject_Type, PyType_Ready
│   ├── refcount.rs     # Py_IncRef/DecRef exports
│   └── gc.rs           # GC allocation, tracking, and cyclic garbage collector
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

include/
└── Python.h            # CPython 3.11 compatible header for extension compilation

build.rs                # cc crate build script for varargs.c
```

## Test Suites

33 test suites run via `./run_tests.sh`:

**C Driver Suites (9):**

| Test Suite | Tests | What it verifies |
|------------|-------|------------------|
| `test_abi.c` | 97 | Struct layouts at byte offsets |
| `test_gc_torture.c` | 109 | GC headers, cycle collection, allocator |
| `test_ext_driver.c` | 49 | Full C API protocol |
| `test_markupsafe.c` | 18 | Self-compiled markupsafe |
| `test_ujson.c` | 48 | Self-compiled ujson |
| `test_prebuilt.c` | 68 | Prebuilt pip wheel `.so` files |
| `test_cython.c` | 20 | Cython-compiled extension |
| `test_bcrypt.c` | 10 | PyO3 bcrypt extension |
| `test_pyyaml.c` | 14 | Cython pyyaml extension |

**VM Python Suites (24):**

| Test Suite | What it verifies |
|------------|------------------|
| `test_phase1.py` | Functions & default arguments |
| `test_phase3.py` | Exception handling (try/except/finally) |
| `test_phase4.py` | Classes & `__init__` |
| `test_phase6.py` | Closures & decorators |
| `test_phase7.py` | `*args`/`**kwargs` |
| `test_phase8.py` | Comprehensions |
| `test_phase9.py` | Generators (`yield`) |
| `test_phase10.py` | String & list methods |
| `test_phase11.py` | Stdlib stubs & builtins |
| `test_final.py` | Comprehensive type tests |
| `test_nonlocal.py` | Nonlocal closures |
| `test_dict_iter.py` | Dict iteration |
| `test_gen_isinstance.py` | Generator isinstance |
| `test_class_inherit.py` | Class inheritance |
| `test_cross_inherit.py` | Cross-module inheritance |
| `test_super.py` | `super()` calls |
| `test_vm_improvements.py` | Multiple inheritance + re module |
| `test_import.py` | Python source imports |
| `test_import_star.py` | `import *` with `__all__` |
| `test_import_collections.py` | `import collections.abc` |
| `test_native_import.py` | `import ujson` (prebuilt C ext) |
| `test_yaml_import.py` | `import yaml` |
| `test_yaml_full.py` | YAML CParser events |
| `test_yaml_safeload_full.py` | `yaml.safe_load()` |

### Setup & Run

```bash
# First-time setup (downloads dependencies, builds everything)
./setup_tests.sh

# Run all 33 test suites
./run_tests.sh

# Run tests without rebuilding (faster)
./run_tests.sh --quick
```

### Manual Build

```bash
# Build the dylib
cargo build --release

# Build the thin binary shim
cc -o rustthon_bin csrc/main.c -ldl

# Run Python source
./rustthon_bin script.py

# Run the REPL
./rustthon_bin
```
