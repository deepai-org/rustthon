 Rustthon is alive!

  What's running right now

  The interpreter successfully executes Python code with:
  ┌───────────────────────────────────────────────────┬─────────┐
  │                      Feature                      │ Status  │
  ├───────────────────────────────────────────────────┼─────────┤
  │ Arithmetic (+, -, *, /, //, %, **)                │ Working │
  ├───────────────────────────────────────────────────┼─────────┤
  │ Variables & assignment                            │ Working │
  ├───────────────────────────────────────────────────┼─────────┤
  │ Strings (creation, concatenation, len)            │ Working │
  ├───────────────────────────────────────────────────┼─────────┤
  │ Booleans (True, False, and, or, not)              │ Working │
  ├───────────────────────────────────────────────────┼─────────┤
  │ Comparisons (<, <=, ==, !=, >, >=, is)            │ Working │
  ├───────────────────────────────────────────────────┼─────────┤
  │ Lists (creation, append, concatenation, indexing) │ Working │
  ├───────────────────────────────────────────────────┼─────────┤
  │ Tuples                                            │ Working │
  ├───────────────────────────────────────────────────┼─────────┤
  │ Dicts (creation, set/get)                         │ Working │
  ├───────────────────────────────────────────────────┼─────────┤
  │ Sets                                              │ Working │
  ├───────────────────────────────────────────────────┼─────────┤
  │ if/elif/else                                      │ Working │
  ├───────────────────────────────────────────────────┼─────────┤
  │ while loops                                       │ Working │
  ├───────────────────────────────────────────────────┼─────────┤
  │ print(), len(), range(), type(), int(), str()     │ Working │
  ├───────────────────────────────────────────────────┼─────────┤
  │ REPL mode                                         │ Working │
  └───────────────────────────────────────────────────┴─────────┘

  CPython 3.11 ABI Compatibility

  234 exported C API symbols. Every built-in type matches CPython 3.11
  byte-for-byte in memory layout, verified by a C test suite that
  directly reads struct internals through pointer arithmetic.

  Type layouts:
  - int: PyLongObject (24B header + u32 digit array), 30-bit digits, sign in ob_size
  - float: PyFloatObject (16B ob_base + 8B ob_fval = 24B)
  - bool: PyLongObject subtype, True/False singletons
  - str: PyASCIIObject (48B) for ASCII, PyCompactUnicodeObject (72B) for non-ASCII
  - bytes: PyBytesObject (32B header + inline ob_sval[N+1])
  - list: PyListObject (40B: ob_base + ob_item + allocated), GC-tracked
  - tuple: PyTupleObject (24B header + inline items), GC-tracked
  - dict: PyDictObject (48B) + PyDictKeysObject compact hash table, GC-tracked
  - set: PySetObject (200B with inline smalltable[8]), GC-tracked

  Infrastructure:
  - 16-byte PyGC_Head prepended before all GC-tracked objects
  - All allocation via libc::calloc/malloc (not std::alloc)
  - Three-tier allocator: PyMem_Raw*, PyMem_*, PyObject_*
  - GIL emulation via parking_lot::Mutex
  - Refcounting with AtomicIsize (Release/Acquire semantics)

  C Varargs Shim (csrc/varargs.c)

  PyArg_ParseTuple, Py_BuildValue, and friends are variadic C functions
  (they use `...` and `va_list`). Rust stable cannot define these because
  `core::ffi::VaList` / the `c_variadic` feature is still nightly-only
  (as of Rust 1.90). This is a fundamental language limitation — Rust can
  *call* variadic C functions but cannot *implement* them.

  The solution: these functions are written in plain C (csrc/varargs.c)
  and compiled into the Rust library via the `cc` crate in build.rs.

  How it works:

    1. csrc/varargs.c implements PyArg_ParseTuple, PyArg_ParseTupleAndKeywords,
       PyArg_UnpackTuple, Py_BuildValue, and Py_VaBuildValue using standard
       C va_list/va_arg.

    2. The C code calls back into Rustthon's own exported C API (PyTuple_GetItem,
       PyLong_AsLong, PyUnicode_AsUTF8, PyLong_FromLong, etc.) to do the actual
       work of extracting and creating Python objects.

    3. build.rs uses the `cc` crate to compile varargs.c into a static library,
       then passes `-Wl,-force_load` and `-Wl,-exported_symbols_list` to the
       macOS linker to ensure the C symbols survive LTO and appear in the
       final librustthon.dylib.

  Why this is necessary:

    Virtually every C extension calls PyArg_ParseTuple to unpack its arguments.
    Without a real implementation, no extension can receive data from Python.
    The old Rust stubs just returned 1 (success) without actually writing to
    the output pointers, which meant every extension would read uninitialized
    memory and crash.

  Supported format characters:

    PyArg_ParseTuple:  s s# z y y# i l n f d O O! p S U | : ;
    Py_BuildValue:     s s# y y# i l n f d O N () [] {}

  Linker details (macOS):

    The `cc` crate compiles varargs.c into libvarargs.a. Normally, with LTO
    enabled and no Rust code referencing the symbols, the linker strips them.
    Two linker flags prevent this:

    - `-Wl,-force_load,<path>/libvarargs.a` — forces all object files from
      the archive into the link, even if unreferenced.

    - `-Wl,-exported_symbols_list,<file>` — adds the C function names to the
      dylib's export table. Without this, rustc's auto-generated export list
      only includes #[no_mangle] Rust symbols.

    Both flags are emitted from build.rs via `cargo:rustc-cdylib-link-arg`.

  File structure

  src/
  ├── lib.rs              # Crate root
  ├── main.rs             # REPL + file execution entry point
  ├── object/
  │   ├── pyobject.rs     # RawPyObject, RawPyVarObject, PyGCHead, PyObjectWithData<T>
  │   ├── typeobj.rs      # Full RawPyTypeObject with all slot function pointers
  │   ├── refcount.rs     # Py_IncRef/DecRef exports
  │   └── gc.rs           # GC allocation (_PyObject_GC_New) and tracking
  ├── types/
  │   ├── none.rs         # None singleton (_Py_NoneStruct)
  │   ├── boolobject.rs   # Bool as int subtype, True/False singletons
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
  │   ├── error.rs        # Thread-local exception state
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

  build.rs                # cc crate build script for varargs.c

  ABI Test Suites

  tests/test_abi.c — Phase 1: Direct struct access. Creates objects via
  the C API, then reads ob_digit[], ob_fval, ob_item[], inline data at
  hardcoded byte offsets. 97 tests verifying every type layout.

  tests/test_gc_torture.c — Phase 2: Memory and GC stress. Allocator
  tiers, GC header arithmetic, circular references (list<->dict,
  self-referencing list, 3-way cycles), 10000-allocation stress tests,
  refcount integrity. 99 tests.

  Build:
    cc -o test_abi tests/test_abi.c -L target/release -lrustthon -Wl,-rpath,target/release
    cc -o test_gc_torture tests/test_gc_torture.c -L target/release -lrustthon -Wl,-rpath,target/release

  Next steps

  1. Debug C extension module loading (PyModuleDef -> method dispatch -> PyObject_Call)
  2. Fill in PyType_Ready — inheriting slots from base types
  3. String interning — for faster attribute lookups
  4. For-loop iteration protocol — proper tp_iter/tp_iternext
  5. Exception type objects — PyExc_TypeError, PyExc_ValueError, etc.
  6. Function definitions — compile to separate CodeObjects with proper argument handling
