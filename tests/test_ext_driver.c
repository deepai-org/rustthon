/*
 * Phase 3: C Extension Driver (Upgraded for ABI & GC Torture Testing)
 *
 * Build:
 * cc -o test_ext_driver tests/test_ext_driver.c \
 * -L target/release -lrustthon -Wl,-rpath,target/release
 *
 * Run:
 * ./test_ext_driver
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <stddef.h>
#include <dlfcn.h>

typedef intptr_t Py_ssize_t;

/* ─── CPython 3.11 Exact Memory Layouts ─── */
typedef struct _object {
    Py_ssize_t ob_refcnt;
    struct _typeobject *ob_type;
} PyObject;

typedef struct {
    PyObject ob_base;
    Py_ssize_t ob_size;
} PyVarObject;

typedef struct {
    PyVarObject ob_base;
    uint32_t ob_digit[1];
} PyLongObject;

typedef struct {
    PyObject ob_base;
    double ob_fval;
} PyFloatObject;

typedef struct {
    PyVarObject ob_base;
    PyObject **ob_item;
    Py_ssize_t allocated;
} PyListObject;

typedef struct {
    PyVarObject ob_base;
    PyObject *ob_item[1]; // Inline flexible array
} PyTupleObject;

typedef struct {
    PyObject ob_base;
    Py_ssize_t length;
    Py_ssize_t hash;
    uint32_t state;
    uint32_t _padding;
    void *wstr;
} PyASCIIObject;

/* 16-byte GC Header (Python 3.8+) */
typedef struct {
    uintptr_t gc_next;
    uintptr_t gc_prev;
} PyGC_Head;


/* ─── Extern declarations ─── */
extern void Py_Initialize(void);
extern PyObject *PyLong_FromLong(long v);
extern long PyLong_AsLong(PyObject *obj);
extern PyObject *PyFloat_FromDouble(double v);
extern double PyFloat_AsDouble(PyObject *obj);
extern PyObject *PyUnicode_FromString(const char *s);
extern const char *PyUnicode_AsUTF8(PyObject *obj);

/* Containers */
extern PyObject *PyTuple_New(Py_ssize_t size);
extern int PyTuple_SetItem(PyObject *tuple, Py_ssize_t i, PyObject *v);
extern PyObject *PyTuple_GetItem(PyObject *tuple, Py_ssize_t i);
extern Py_ssize_t PyTuple_Size(PyObject *tuple);

extern PyObject *PyList_New(Py_ssize_t size);
extern int PyList_Append(PyObject *list, PyObject *item);
extern int PyList_SetItem(PyObject *list, Py_ssize_t i, PyObject *item);
extern Py_ssize_t PyList_Size(PyObject *list);
extern PyObject *PyList_GetItem(PyObject *list, Py_ssize_t i);

extern PyObject *PyDict_New(void);
extern int PyDict_SetItem(PyObject *p, PyObject *key, PyObject *val);
extern PyObject *PyDict_GetItem(PyObject *p, PyObject *key);
extern PyObject *PyDict_GetItemString(PyObject *dict, const char *key);

extern PyObject *PySet_New(PyObject *iterable);
extern int PySet_Add(PyObject *set, PyObject *key);

/* Core & Memory */
extern PyObject *PyModule_GetDict(PyObject *module);
extern PyObject *PyObject_Call(PyObject *callable, PyObject *args, PyObject *kwargs);
extern PyObject *_Py_None(void);
extern void Py_IncRef(PyObject *o);
extern void Py_DecRef(PyObject *o);

extern void *PyMem_Malloc(size_t size);
extern void *PyMem_Realloc(void *ptr, size_t new_size);
extern void PyMem_Free(void *ptr);
extern void PyObject_GC_Track(PyObject *obj);
extern void PyObject_GC_UnTrack(PyObject *obj);


/* ─── Test infrastructure (Unchanged) ─── */
static int tests_run = 0;
static int tests_passed = 0;
static int tests_failed = 0;

#define TEST(name) do { tests_run++; printf("  %-55s ", name); } while(0)
#define PASS() do { tests_passed++; printf("\033[32mPASS\033[0m\n"); } while(0)
#define FAIL(fmt, ...) do { tests_failed++; printf("\033[31mFAIL\033[0m  " fmt "\n", ##__VA_ARGS__); } while(0)
#define CHECK(cond, fmt, ...) do { if (cond) { PASS(); } else { FAIL(fmt, ##__VA_ARGS__); } } while(0)

/* (Your existing make_args_* and call_method functions remain exactly the same here) */
static PyObject *call_method(PyObject *dict, const char *name, PyObject *args) {
    PyObject *func = PyDict_GetItemString(dict, name);
    if (!func) return NULL;
    return PyObject_Call(func, args, NULL);
}
static PyObject *make_args_0(void) { return PyTuple_New(0); }
static PyObject *make_args_1(PyObject *a) { PyObject *t = PyTuple_New(1); Py_IncRef(a); PyTuple_SetItem(t, 0, a); return t; }
static PyObject *make_args_2(PyObject *a, PyObject *b) { PyObject *t = PyTuple_New(2); Py_IncRef(a); Py_IncRef(b); PyTuple_SetItem(t, 0, a); PyTuple_SetItem(t, 1, b); return t; }
static PyObject *make_args_3(PyObject *a, PyObject *b, PyObject *c) { PyObject *t = PyTuple_New(3); Py_IncRef(a); Py_IncRef(b); Py_IncRef(c); PyTuple_SetItem(t, 0, a); PyTuple_SetItem(t, 1, b); PyTuple_SetItem(t, 2, c); return t; }


/* ═══════════════════════════════════════════════════════
 * NEW: ABI & Internal Struct Tests
 * ═══════════════════════════════════════════════════════ */

void test_raw_struct_abi(void) {
    printf("\n=== ABI Direct Memory Access ===\n");

    /* 1. PyLongObject Direct Access */
    PyObject *l = PyLong_FromLong(1073741823); /* 2^30 - 1 (fits perfectly in 1 digit) */
    PyLongObject *pl = (PyLongObject *)l;
    TEST("Direct read of PyLongObject->ob_size");
    CHECK(pl->ob_base.ob_size == 1, "got %zd", pl->ob_base.ob_size);
    TEST("Direct read of PyLongObject->ob_digit[0]");
    CHECK(pl->ob_digit[0] == 1073741823, "got %u", pl->ob_digit[0]);

    /* 2. PyListObject Direct Access */
    PyObject *lst = PyList_New(1);
    PyListObject *plst = (PyListObject *)lst;
    PyObject *val = PyLong_FromLong(42);
    PyList_SetItem(lst, 0, val); /* Steals reference */
    TEST("Direct read of PyListObject->ob_item[0]");
    CHECK(plst->ob_item[0] == val, "pointer mismatch");
    TEST("Direct read of PyListObject->ob_size");
    CHECK(plst->ob_base.ob_size == 1, "got %zd", plst->ob_base.ob_size);

    /* 3. PyASCIIObject Fast-Path Direct Access */
    PyObject *s = PyUnicode_FromString("ABI");
    PyASCIIObject *pascii = (PyASCIIObject *)s;
    TEST("Direct read of PyASCIIObject->length");
    CHECK(pascii->length == 3, "got %zd", pascii->length);

    TEST("Direct read of PyASCIIObject inline string data");
    /* Memory data begins immediately after the 48-byte header */
    char *inline_data = (char *)(pascii + 1);
    CHECK(inline_data[0] == 'A' && inline_data[1] == 'B' && inline_data[2] == 'I',
          "got %c%c%c", inline_data[0], inline_data[1], inline_data[2]);

    Py_DecRef(l);
    Py_DecRef(lst);
    Py_DecRef(s);
}

void test_gc_and_allocator(void) {
    printf("\n=== GC Header & C Allocator ===\n");

    /* 1. PyMem_* Router check */
    TEST("PyMem_Malloc and PyMem_Realloc routing");
    void *mem = PyMem_Malloc(128);
    if (!mem) { FAIL("Malloc failed"); }
    else {
        memset(mem, 0xAA, 128); /* Ensure we own it */
        mem = PyMem_Realloc(mem, 256);
        CHECK(mem != NULL, "Realloc failed");
        PyMem_Free(mem);
    }

    /* 2. The 16-byte GC Offset Test */
    PyObject *lst = PyList_New(0);
    TEST("GC Header reverse pointer math (PyGC_Head*)obj - 1");
    PyGC_Head *gc = ((PyGC_Head *)lst) - 1;

    /* If this offset is wrong, PyObject_GC_Track will write over the list size/items and instantly segfault */
    PyObject_GC_Track(lst);
    PASS(); /* We survived the track */

    PyObject_GC_UnTrack(lst);
    Py_DecRef(lst);
}

void test_containers_api(void) {
    printf("\n=== Native Container APIs ===\n");

    /* Dict */
    PyObject *dict = PyDict_New();
    PyObject *k1 = PyUnicode_FromString("key1");
    PyObject *v1 = PyLong_FromLong(100);

    PyDict_SetItem(dict, k1, v1);
    PyObject *fetched = PyDict_GetItem(dict, k1); /* Borrowed ref */

    TEST("PyDict_SetItem / PyDict_GetItem");
    CHECK(fetched != NULL && PyLong_AsLong(fetched) == 100, "dict access failed");

    /* Set */
    PyObject *set = PySet_New(NULL);
    PySet_Add(set, k1);

    TEST("PySet_New / PySet_Add");
    CHECK(set != NULL, "set operations failed");

    Py_DecRef(dict);
    Py_DecRef(set);
    Py_DecRef(k1);
    Py_DecRef(v1);
}

/* ═══════════════════════════════════════════════════════
 * Extension Method Tests (dlopen + PyArg_ParseTuple + Py_BuildValue)
 * ═══════════════════════════════════════════════════════ */

extern Py_ssize_t PyUnicode_GET_LENGTH(PyObject *obj);

void test_module_loading(void) {
    printf("\n=== Module Loading ===\n");

    void *handle = dlopen("./_testmod.dylib", RTLD_NOW | RTLD_GLOBAL);

    TEST("dlopen(_testmod.dylib) succeeds");
    if (!handle) {
        FAIL("dlopen: %s", dlerror());
        printf("\n  FATAL: Cannot continue without the extension module.\n\n");
        exit(1);
    } else {
        PASS();
    }

    typedef PyObject *(*PyInitFunc)(void);
    PyInitFunc init = (PyInitFunc)dlsym(handle, "PyInit__testmod");

    TEST("dlsym(PyInit__testmod) found");
    if (!init) {
        FAIL("dlsym: %s", dlerror());
        exit(1);
    } else {
        PASS();
    }

    PyObject *module = init();

    TEST("PyInit__testmod() returns non-null");
    CHECK(module != NULL, "returned null");

    TEST("Module has a __dict__");
    PyObject *dict = PyModule_GetDict(module);
    CHECK(dict != NULL, "dict is null");

    TEST("Module dict has 'add' method");
    CHECK(PyDict_GetItemString(dict, "add") != NULL, "not found");

    TEST("Module dict has 'greet' method");
    CHECK(PyDict_GetItemString(dict, "greet") != NULL, "not found");

    TEST("Module dict has 'noop' method");
    CHECK(PyDict_GetItemString(dict, "noop") != NULL, "not found");

    TEST("Module dict has 'strlen' method");
    CHECK(PyDict_GetItemString(dict, "strlen") != NULL, "not found");

    TEST("Module dict has 'make_list' method");
    CHECK(PyDict_GetItemString(dict, "make_list") != NULL, "not found");

    TEST("Module dict has 'sum_list' method");
    CHECK(PyDict_GetItemString(dict, "sum_list") != NULL, "not found");

    TEST("Module dict has 'mixed_return' method");
    CHECK(PyDict_GetItemString(dict, "mixed_return") != NULL, "not found");

    TEST("Module dict has 'pass_through' method");
    CHECK(PyDict_GetItemString(dict, "pass_through") != NULL, "not found");

    TEST("Module dict has '__name__'");
    PyObject *name = PyDict_GetItemString(dict, "__name__");
    CHECK(name != NULL, "not found");

    if (name) {
        TEST("Module __name__ == '_testmod'");
        const char *name_str = PyUnicode_AsUTF8(name);
        CHECK(name_str && strcmp(name_str, "_testmod") == 0,
              "got '%s'", name_str ? name_str : "(null)");
    }
}

void test_add_method(void) {
    printf("\n=== add(a, b) — PyArg_ParseTuple 'ii' ===\n");

    void *handle = dlopen("./_testmod.dylib", RTLD_NOW);
    PyObject *module = ((PyObject*(*)(void))dlsym(handle, "PyInit__testmod"))();
    PyObject *dict = PyModule_GetDict(module);

    PyObject *a3 = PyLong_FromLong(3);
    PyObject *a4 = PyLong_FromLong(4);
    PyObject *args = make_args_2(a3, a4);
    PyObject *result = call_method(dict, "add", args);

    TEST("add(3, 4) returns non-null");
    CHECK(result != NULL, "null");

    if (result) {
        TEST("add(3, 4) == 7");
        CHECK(PyLong_AsLong(result) == 7,
              "got %ld", PyLong_AsLong(result));
        Py_DecRef(result);
    }

    PyObject *a100 = PyLong_FromLong(100);
    PyObject *am50 = PyLong_FromLong(-50);
    PyObject *args2 = make_args_2(a100, am50);
    result = call_method(dict, "add", args2);

    TEST("add(100, -50) == 50");
    CHECK(result && PyLong_AsLong(result) == 50,
          "got %ld", result ? PyLong_AsLong(result) : -1);

    PyObject *z = PyLong_FromLong(0);
    PyObject *args3 = make_args_2(z, z);
    result = call_method(dict, "add", args3);

    TEST("add(0, 0) == 0");
    CHECK(result && PyLong_AsLong(result) == 0,
          "got %ld", result ? PyLong_AsLong(result) : -1);

    Py_DecRef(a3); Py_DecRef(a4); Py_DecRef(args);
    Py_DecRef(a100); Py_DecRef(am50); Py_DecRef(args2);
    Py_DecRef(z); Py_DecRef(args3);
}

void test_multiply_method(void) {
    printf("\n=== multiply(a, b) — PyArg_ParseTuple 'dd' ===\n");

    void *handle = dlopen("./_testmod.dylib", RTLD_NOW);
    PyObject *module = ((PyObject*(*)(void))dlsym(handle, "PyInit__testmod"))();
    PyObject *dict = PyModule_GetDict(module);

    PyObject *d3 = PyFloat_FromDouble(3.0);
    PyObject *d4 = PyFloat_FromDouble(4.5);
    PyObject *args = make_args_2(d3, d4);
    PyObject *result = call_method(dict, "multiply", args);

    TEST("multiply(3.0, 4.5) returns non-null");
    CHECK(result != NULL, "null");

    if (result) {
        double v = PyFloat_AsDouble(result);
        TEST("multiply(3.0, 4.5) == 13.5");
        CHECK(v == 13.5, "got %f", v);
        Py_DecRef(result);
    }

    Py_DecRef(d3); Py_DecRef(d4); Py_DecRef(args);
}

void test_greet_method(void) {
    printf("\n=== greet(name) — PyArg_ParseTuple 's' ===\n");

    void *handle = dlopen("./_testmod.dylib", RTLD_NOW);
    PyObject *module = ((PyObject*(*)(void))dlsym(handle, "PyInit__testmod"))();
    PyObject *dict = PyModule_GetDict(module);

    PyObject *name = PyUnicode_FromString("Rustthon");
    PyObject *args = make_args_1(name);
    PyObject *result = call_method(dict, "greet", args);

    TEST("greet('Rustthon') returns non-null");
    CHECK(result != NULL, "null");

    if (result) {
        const char *s = PyUnicode_AsUTF8(result);
        TEST("greet('Rustthon') == 'Hello, Rustthon!'");
        CHECK(s && strcmp(s, "Hello, Rustthon!") == 0,
              "got '%s'", s ? s : "(null)");
        Py_DecRef(result);
    }

    Py_DecRef(name); Py_DecRef(args);
}

void test_strlen_method(void) {
    printf("\n=== strlen(s) — METH_O ===\n");

    void *handle = dlopen("./_testmod.dylib", RTLD_NOW);
    PyObject *module = ((PyObject*(*)(void))dlsym(handle, "PyInit__testmod"))();
    PyObject *dict = PyModule_GetDict(module);

    PyObject *s = PyUnicode_FromString("hello world");
    PyObject *args = make_args_1(s);
    PyObject *result = call_method(dict, "strlen", args);

    TEST("strlen('hello world') == 11");
    CHECK(result && PyLong_AsLong(result) == 11,
          "got %ld", result ? PyLong_AsLong(result) : -1);

    PyObject *empty = PyUnicode_FromString("");
    PyObject *args2 = make_args_1(empty);
    result = call_method(dict, "strlen", args2);

    TEST("strlen('') == 0");
    CHECK(result && PyLong_AsLong(result) == 0,
          "got %ld", result ? PyLong_AsLong(result) : -1);

    Py_DecRef(s); Py_DecRef(args);
    Py_DecRef(empty); Py_DecRef(args2);
}

void test_noop_method(void) {
    printf("\n=== noop() — METH_NOARGS ===\n");

    void *handle = dlopen("./_testmod.dylib", RTLD_NOW);
    PyObject *module = ((PyObject*(*)(void))dlsym(handle, "PyInit__testmod"))();
    PyObject *dict = PyModule_GetDict(module);

    PyObject *args = make_args_0();
    PyObject *result = call_method(dict, "noop", args);

    TEST("noop() returns non-null");
    CHECK(result != NULL, "null");

    TEST("noop() returns None");
    CHECK(result == _Py_None(), "not None");

    Py_DecRef(args);
}

void test_make_list_method(void) {
    printf("\n=== make_list(n) — PyArg_ParseTuple + list ops ===\n");

    void *handle = dlopen("./_testmod.dylib", RTLD_NOW);
    PyObject *module = ((PyObject*(*)(void))dlsym(handle, "PyInit__testmod"))();
    PyObject *dict = PyModule_GetDict(module);

    PyObject *n5 = PyLong_FromLong(5);
    PyObject *args = make_args_1(n5);
    PyObject *result = call_method(dict, "make_list", args);

    TEST("make_list(5) returns non-null");
    CHECK(result != NULL, "null");

    if (result) {
        TEST("make_list(5) has size 5");
        CHECK(PyList_Size(result) == 5,
              "got %zd", PyList_Size(result));

        TEST("make_list(5)[0] == 0");
        CHECK(PyLong_AsLong(PyList_GetItem(result, 0)) == 0,
              "got %ld", PyLong_AsLong(PyList_GetItem(result, 0)));

        TEST("make_list(5)[4] == 4");
        CHECK(PyLong_AsLong(PyList_GetItem(result, 4)) == 4,
              "got %ld", PyLong_AsLong(PyList_GetItem(result, 4)));

        Py_DecRef(result);
    }

    Py_DecRef(n5); Py_DecRef(args);
}

void test_sum_list_method(void) {
    printf("\n=== sum_list(lst) — METH_O + list iteration ===\n");

    void *handle = dlopen("./_testmod.dylib", RTLD_NOW);
    PyObject *module = ((PyObject*(*)(void))dlsym(handle, "PyInit__testmod"))();
    PyObject *dict = PyModule_GetDict(module);

    PyObject *list = PyList_New(0);
    for (int i = 1; i <= 4; i++) {
        PyObject *item = PyLong_FromLong(i * 10);
        PyList_Append(list, item);
        Py_DecRef(item);
    }

    PyObject *args = make_args_1(list);
    PyObject *result = call_method(dict, "sum_list", args);

    TEST("sum_list([10,20,30,40]) == 100");
    CHECK(result && PyLong_AsLong(result) == 100,
          "got %ld", result ? PyLong_AsLong(result) : -1);

    Py_DecRef(list); Py_DecRef(args);
    if (result) Py_DecRef(result);
}

void test_mixed_return_method(void) {
    printf("\n=== mixed_return(i, d, s) — Py_BuildValue '(ids)' ===\n");

    void *handle = dlopen("./_testmod.dylib", RTLD_NOW);
    PyObject *module = ((PyObject*(*)(void))dlsym(handle, "PyInit__testmod"))();
    PyObject *dict = PyModule_GetDict(module);

    PyObject *i5 = PyLong_FromLong(5);
    PyObject *d3 = PyFloat_FromDouble(3.14);
    PyObject *sHi = PyUnicode_FromString("world");
    PyObject *args = make_args_3(i5, d3, sHi);
    PyObject *result = call_method(dict, "mixed_return", args);

    TEST("mixed_return(5, 3.14, 'world') returns non-null");
    CHECK(result != NULL, "null");

    if (result) {
        TEST("Result is a tuple of size 3");
        CHECK(PyTuple_Size(result) == 3,
              "got %zd", PyTuple_Size(result));

        PyObject *r0 = PyTuple_GetItem(result, 0);
        TEST("tuple[0] == 10 (i*2)");
        CHECK(r0 && PyLong_AsLong(r0) == 10,
              "got %ld", r0 ? PyLong_AsLong(r0) : -1);

        PyObject *r1 = PyTuple_GetItem(result, 1);
        TEST("tuple[1] == 6.28 (d*2.0)");
        CHECK(r1 && PyFloat_AsDouble(r1) == 6.28,
              "got %f", r1 ? PyFloat_AsDouble(r1) : -1.0);

        PyObject *r2 = PyTuple_GetItem(result, 2);
        TEST("tuple[2] == 'got: world'");
        const char *sv = r2 ? PyUnicode_AsUTF8(r2) : NULL;
        CHECK(sv && strcmp(sv, "got: world") == 0,
              "got '%s'", sv ? sv : "(null)");

        Py_DecRef(result);
    }

    Py_DecRef(i5); Py_DecRef(d3); Py_DecRef(sHi); Py_DecRef(args);
}

void test_pass_through_method(void) {
    printf("\n=== pass_through(obj) — ParseTuple 'O' + BuildValue 'O' ===\n");

    void *handle = dlopen("./_testmod.dylib", RTLD_NOW);
    PyObject *module = ((PyObject*(*)(void))dlsym(handle, "PyInit__testmod"))();
    PyObject *dict = PyModule_GetDict(module);

    PyObject *n42 = PyLong_FromLong(42);
    PyObject *args = make_args_1(n42);
    PyObject *result = call_method(dict, "pass_through", args);

    TEST("pass_through(42) returns non-null");
    CHECK(result != NULL, "null");

    if (result) {
        TEST("pass_through(42) == 42");
        CHECK(PyLong_AsLong(result) == 42,
              "got %ld", PyLong_AsLong(result));
    }

    PyObject *none = _Py_None();
    PyObject *args2 = make_args_1(none);
    result = call_method(dict, "pass_through", args2);

    TEST("pass_through(None) == None");
    CHECK(result == none, "not None");

    Py_DecRef(n42); Py_DecRef(args); Py_DecRef(args2);
}

/* ═══════════════════════════════════════════════════════
 *  Main
 * ═══════════════════════════════════════════════════════ */

int main(void) {
    printf("╔══════════════════════════════════════════════════════════╗\n");
    printf("║  Rustthon Phase 3: Ultimate ABI & Extension Driver       ║\n");
    printf("║  Testing direct struct offsets, GC tracking, & memory    ║\n");
    printf("║  + dlopen extension loading, varargs, method dispatch    ║\n");
    printf("╚══════════════════════════════════════════════════════════╝\n");

    Py_Initialize();

    /* ABI & Infrastructure Tests */
    test_raw_struct_abi();
    test_gc_and_allocator();
    test_containers_api();

    /* Extension Module Tests */
    test_module_loading();
    test_add_method();
    test_multiply_method();
    test_greet_method();
    test_strlen_method();
    test_noop_method();
    test_make_list_method();
    test_sum_list_method();
    test_mixed_return_method();
    test_pass_through_method();

    printf("\n═══════════════════════════════════════════════════════════\n");
    printf("  Total: %d  |  ", tests_run);
    if (tests_failed == 0) {
        printf("\033[32mPassed: %d\033[0m  |  Failed: %d\n", tests_passed, tests_failed);
        printf("\n  \033[32m✓ ALL TESTS PASSED — The ABI is bulletproof!\033[0m\n");
    } else {
        printf("Passed: %d  |  \033[31mFailed: %d\033[0m\n", tests_passed, tests_failed);
        printf("\n  \033[31m✗ SOME TESTS FAILED\033[0m\n");
    }
    printf("═══════════════════════════════════════════════════════════\n\n");

    return tests_failed > 0 ? 1 : 0;
}