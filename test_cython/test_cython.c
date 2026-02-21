/*
 * Phase 4.5: Cython Extension Test Driver
 *
 * Tests that Rustthon can load and run Cython-generated C extensions.
 * The hello.pyx module was compiled with Cython against Python 3.11 headers.
 *
 * Build:
 *   cc -o test_cython test_cython/test_cython.c -ldl
 *
 * Run:
 *   ./test_cython
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <dlfcn.h>

typedef intptr_t Py_ssize_t;
typedef struct _object {
    Py_ssize_t ob_refcnt;
    struct _typeobject *ob_type;
} PyObject;

/* Test infrastructure */
static int tests_run = 0;
static int tests_passed = 0;
static int tests_failed = 0;

#define TEST(name) do { tests_run++; printf("  %-55s ", name); } while(0)
#define PASS() do { tests_passed++; printf("\033[32mPASS\033[0m\n"); } while(0)
#define FAIL(fmt, ...) do { tests_failed++; printf("\033[31mFAIL\033[0m  " fmt "\n", ##__VA_ARGS__); } while(0)
#define CHECK(cond, fmt, ...) do { if (cond) { PASS(); } else { FAIL(fmt, ##__VA_ARGS__); } } while(0)

/* Function pointer types */
typedef void (*fn_Py_Initialize)(void);
typedef PyObject *(*fn_PyUnicode_FromString)(const char *);
typedef const char *(*fn_PyUnicode_AsUTF8)(PyObject *);
typedef PyObject *(*fn_PyModule_GetDict)(PyObject *);
typedef PyObject *(*fn_PyDict_GetItemString)(PyObject *, const char *);
typedef PyObject *(*fn_PyObject_Call)(PyObject *, PyObject *, PyObject *);
typedef PyObject *(*fn_PyTuple_New)(Py_ssize_t);
typedef int (*fn_PyTuple_SetItem)(PyObject *, Py_ssize_t, PyObject *);
typedef PyObject *(*fn_PyLong_FromLong)(long);
typedef long (*fn_PyLong_AsLong)(PyObject *);
typedef void (*fn_Py_IncRef)(PyObject *);
typedef void (*fn_Py_DecRef)(PyObject *);
typedef PyObject *(*fn_PyErr_Occurred)(void);
typedef void (*fn_PyErr_Clear)(void);
typedef void (*fn_PyErr_Print)(void);

/* Resolved function pointers */
static fn_Py_Initialize         p_Py_Initialize;
static fn_PyUnicode_FromString  p_PyUnicode_FromString;
static fn_PyUnicode_AsUTF8      p_PyUnicode_AsUTF8;
static fn_PyModule_GetDict      p_PyModule_GetDict;
static fn_PyDict_GetItemString  p_PyDict_GetItemString;
static fn_PyObject_Call         p_PyObject_Call;
static fn_PyTuple_New           p_PyTuple_New;
static fn_PyTuple_SetItem       p_PyTuple_SetItem;
static fn_PyLong_FromLong       p_PyLong_FromLong;
static fn_PyLong_AsLong         p_PyLong_AsLong;
static fn_Py_IncRef             p_Py_IncRef;
static fn_Py_DecRef             p_Py_DecRef;
static fn_PyErr_Occurred        p_PyErr_Occurred;
static fn_PyErr_Clear           p_PyErr_Clear;
static fn_PyErr_Print           p_PyErr_Print;

#define RESOLVE(handle, name) do { \
    p_##name = (fn_##name)dlsym(handle, #name); \
    if (!p_##name) { \
        printf("  WARNING: cannot resolve " #name ": %s\n", dlerror()); \
    } \
} while(0)

static void resolve_api(void *handle) {
    RESOLVE(handle, Py_Initialize);
    RESOLVE(handle, PyUnicode_FromString);
    RESOLVE(handle, PyUnicode_AsUTF8);
    RESOLVE(handle, PyModule_GetDict);
    RESOLVE(handle, PyDict_GetItemString);
    RESOLVE(handle, PyObject_Call);
    RESOLVE(handle, PyTuple_New);
    RESOLVE(handle, PyTuple_SetItem);
    RESOLVE(handle, PyLong_FromLong);
    RESOLVE(handle, PyLong_AsLong);
    RESOLVE(handle, Py_IncRef);
    RESOLVE(handle, Py_DecRef);
    RESOLVE(handle, PyErr_Occurred);
    RESOLVE(handle, PyErr_Clear);
    RESOLVE(handle, PyErr_Print);
}

/* Helper: call a function with string arg */
static PyObject *call_str(PyObject *func, const char *arg) {
    PyObject *s = p_PyUnicode_FromString(arg);
    PyObject *args = p_PyTuple_New(1);
    p_Py_IncRef(s);
    p_PyTuple_SetItem(args, 0, s);
    PyObject *result = p_PyObject_Call(func, args, NULL);
    p_Py_DecRef(s);
    p_Py_DecRef(args);
    return result;
}

/* Helper: call a function with two int args */
static PyObject *call_int2(PyObject *func, long a, long b) {
    PyObject *args = p_PyTuple_New(2);
    p_PyTuple_SetItem(args, 0, p_PyLong_FromLong(a));
    p_PyTuple_SetItem(args, 1, p_PyLong_FromLong(b));
    PyObject *result = p_PyObject_Call(func, args, NULL);
    p_Py_DecRef(args);
    return result;
}

/* Helper: call a function with one int arg */
static PyObject *call_int1(PyObject *func, long a) {
    PyObject *args = p_PyTuple_New(1);
    p_PyTuple_SetItem(args, 0, p_PyLong_FromLong(a));
    PyObject *result = p_PyObject_Call(func, args, NULL);
    p_Py_DecRef(args);
    return result;
}

int main(int argc, char *argv[]) {
    const char *rustthon_path = getenv("RUSTTHON_LIB");
    if (!rustthon_path) rustthon_path = "target/release/librustthon.dylib";

    const char *cython_so = getenv("CYTHON_SO");
    if (!cython_so) cython_so = "test_cython/hello.cpython-311-darwin.so";

    printf("===========================================================\n");
    printf("  Rustthon Phase 4.5: Cython Extension Test\n");
    printf("  Loading Cython-compiled hello.pyx\n");
    printf("===========================================================\n");

    /* Step 1: Load librustthon.dylib with RTLD_GLOBAL */
    printf("\n=== Loading Rustthon Runtime ===\n");

    TEST("dlopen(librustthon.dylib, RTLD_GLOBAL | RTLD_LAZY)");
    void *rt = dlopen(rustthon_path, RTLD_GLOBAL | RTLD_LAZY);
    if (!rt) {
        FAIL("dlopen: %s", dlerror());
        return 1;
    }
    PASS();

    resolve_api(rt);

    TEST("Py_Initialize resolved");
    CHECK(p_Py_Initialize != NULL, "null");

    printf("\n=== Initializing Rustthon Runtime ===\n");
    TEST("Py_Initialize() succeeds");
    p_Py_Initialize();
    PASS();

    /* Step 2: Load the Cython module */
    printf("\n=== Loading Cython hello module ===\n");

    TEST("dlopen(hello.cpython-311-darwin.so)");
    void *hello = dlopen(cython_so, RTLD_LAZY);
    if (!hello) {
        FAIL("dlopen: %s", dlerror());
        printf("\n  FATAL: Cannot load Cython module.\n");
        printf("  Missing symbols from librustthon?\n\n");
        return 1;
    }
    PASS();

    typedef PyObject *(*PyInitFunc)(void);
    PyInitFunc init = (PyInitFunc)dlsym(hello, "PyInit_hello");

    TEST("dlsym(PyInit_hello) found");
    if (!init) {
        FAIL("dlsym: %s", dlerror());
        return 1;
    }
    PASS();

    TEST("PyInit_hello() returns non-null");
    PyObject *module = init();
    if (!module) {
        if (p_PyErr_Occurred()) {
            printf("  (Python error set)\n");
            p_PyErr_Print();
            p_PyErr_Clear();
        }
        FAIL("returned null");
        return 1;
    }
    PASS();

    /* Step 3: Get module dict and find functions */
    PyObject *dict = p_PyModule_GetDict(module);

    TEST("Module has a __dict__");
    CHECK(dict != NULL, "dict is null");

    PyObject *greet_func = p_PyDict_GetItemString(dict, "greet");
    TEST("Module dict has 'greet'");
    CHECK(greet_func != NULL, "not found");

    PyObject *add_func = p_PyDict_GetItemString(dict, "add");
    TEST("Module dict has 'add'");
    CHECK(add_func != NULL, "not found");

    PyObject *fib_func = p_PyDict_GetItemString(dict, "fibonacci");
    TEST("Module dict has 'fibonacci'");
    CHECK(fib_func != NULL, "not found");

    /* Debug: inspect what PyDict_GetItemString actually returned */
    printf("\n=== Debug: Inspecting returned pointers ===\n");
    printf("  dict         = %p\n", dict);
    printf("  greet_func   = %p\n", greet_func);
    if (greet_func) {
        printf("  greet->ob_refcnt = %zd\n", greet_func->ob_refcnt);
        printf("  greet->ob_type   = %p\n", (void*)greet_func->ob_type);
        if (greet_func->ob_type) {
            struct _typeobject *tp = greet_func->ob_type;
            /* tp_name is the first pointer-sized field after ob_base in our type */
            printf("  greet type name  = %s\n", *(const char **)((char*)tp + sizeof(PyObject) + sizeof(Py_ssize_t)));
        }
    }
    printf("  add_func     = %p\n", add_func);
    if (add_func) {
        printf("  add->ob_refcnt   = %zd\n", add_func->ob_refcnt);
        printf("  add->ob_type     = %p\n", (void*)add_func->ob_type);
    }

    /* Step 4: Test greet() */
    printf("\n=== Testing greet() ===\n");

    if (greet_func) {
        PyObject *r = call_str(greet_func, "World");
        TEST("greet('World') returns non-null");
        if (!r) {
            if (p_PyErr_Occurred()) {
                p_PyErr_Print();
                p_PyErr_Clear();
            }
            FAIL("null result");
        } else {
            PASS();
            const char *s = p_PyUnicode_AsUTF8(r);
            TEST("greet('World') == 'Hello, World! From Cython.'");
            CHECK(s && strcmp(s, "Hello, World! From Cython.") == 0,
                  "got '%s'", s ? s : "(null)");
            p_Py_DecRef(r);
        }

        r = call_str(greet_func, "Rustthon");
        TEST("greet('Rustthon') works");
        if (r) {
            const char *s = p_PyUnicode_AsUTF8(r);
            CHECK(s && strcmp(s, "Hello, Rustthon! From Cython.") == 0,
                  "got '%s'", s ? s : "(null)");
            p_Py_DecRef(r);
        } else {
            FAIL("null result");
            if (p_PyErr_Occurred()) { p_PyErr_Print(); p_PyErr_Clear(); }
        }
    }

    /* Step 5: Test add() */
    printf("\n=== Testing add() ===\n");

    if (add_func) {
        PyObject *r;
        long val;

        r = call_int2(add_func, 2, 3);
        TEST("add(2, 3) == 5");
        if (r) {
            val = p_PyLong_AsLong(r);
            CHECK(val == 5, "got %ld", val);
            p_Py_DecRef(r);
        } else {
            FAIL("null");
            if (p_PyErr_Occurred()) { p_PyErr_Print(); p_PyErr_Clear(); }
        }

        r = call_int2(add_func, -10, 10);
        TEST("add(-10, 10) == 0");
        if (r) {
            val = p_PyLong_AsLong(r);
            CHECK(val == 0, "got %ld", val);
            p_Py_DecRef(r);
        } else {
            FAIL("null");
            if (p_PyErr_Occurred()) { p_PyErr_Print(); p_PyErr_Clear(); }
        }

        r = call_int2(add_func, 100000, 200000);
        TEST("add(100000, 200000) == 300000");
        if (r) {
            val = p_PyLong_AsLong(r);
            CHECK(val == 300000, "got %ld", val);
            p_Py_DecRef(r);
        } else {
            FAIL("null");
            if (p_PyErr_Occurred()) { p_PyErr_Print(); p_PyErr_Clear(); }
        }
    }

    /* Step 6: Test fibonacci() */
    printf("\n=== Testing fibonacci() ===\n");

    if (fib_func) {
        PyObject *r;
        long val;

        r = call_int1(fib_func, 0);
        TEST("fibonacci(0) == 0");
        if (r) {
            val = p_PyLong_AsLong(r);
            CHECK(val == 0, "got %ld", val);
            p_Py_DecRef(r);
        } else {
            FAIL("null");
            if (p_PyErr_Occurred()) { p_PyErr_Print(); p_PyErr_Clear(); }
        }

        r = call_int1(fib_func, 1);
        TEST("fibonacci(1) == 1");
        if (r) {
            val = p_PyLong_AsLong(r);
            CHECK(val == 1, "got %ld", val);
            p_Py_DecRef(r);
        } else {
            FAIL("null");
            if (p_PyErr_Occurred()) { p_PyErr_Print(); p_PyErr_Clear(); }
        }

        r = call_int1(fib_func, 10);
        TEST("fibonacci(10) == 55");
        if (r) {
            val = p_PyLong_AsLong(r);
            CHECK(val == 55, "got %ld", val);
            p_Py_DecRef(r);
        } else {
            FAIL("null");
            if (p_PyErr_Occurred()) { p_PyErr_Print(); p_PyErr_Clear(); }
        }

        r = call_int1(fib_func, 20);
        TEST("fibonacci(20) == 6765");
        if (r) {
            val = p_PyLong_AsLong(r);
            CHECK(val == 6765, "got %ld", val);
            p_Py_DecRef(r);
        } else {
            FAIL("null");
            if (p_PyErr_Occurred()) { p_PyErr_Print(); p_PyErr_Clear(); }
        }
    }

    /* Summary */
    printf("\n===========================================================\n");
    printf("  Total: %d  |  ", tests_run);
    if (tests_failed == 0) {
        printf("\033[32mPassed: %d\033[0m  |  Failed: %d\n", tests_passed, tests_failed);
        printf("\n  \033[32mCython hello world running on Rustthon!\033[0m\n");
    } else {
        printf("Passed: %d  |  \033[31mFailed: %d\033[0m\n", tests_passed, tests_failed);
        printf("\n  \033[31mSome tests failed\033[0m\n");
    }
    printf("===========================================================\n\n");

    return tests_failed > 0 ? 1 : 0;
}
