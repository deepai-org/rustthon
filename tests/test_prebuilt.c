/*
 * Phase 4: Prebuilt CPython 3.11 Binary Extension Test Driver
 *
 * Tests that Rustthon can load and run PREBUILT .so files from pip wheels
 * — extensions compiled against real CPython 3.11, NOT our own headers.
 *
 * CRITICAL: librustthon.dylib must be loaded with RTLD_GLOBAL | RTLD_LAZY
 * to force all symbols into the flat namespace. Without RTLD_GLOBAL, macOS
 * two-level namespace isolation prevents the prebuilt .so from resolving
 * Rustthon's exported symbols.
 *
 * Build:
 *   cc -o test_prebuilt tests/test_prebuilt.c -ldl
 *
 * Run:
 *   ./test_prebuilt
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <dlfcn.h>
#include <math.h>

typedef intptr_t Py_ssize_t;
typedef struct _object {
    Py_ssize_t ob_refcnt;
    struct _typeobject *ob_type;
} PyObject;

/* ─── Test infrastructure ─── */
static int tests_run = 0;
static int tests_passed = 0;
static int tests_failed = 0;

#define TEST(name) do { tests_run++; printf("  %-55s ", name); } while(0)
#define PASS() do { tests_passed++; printf("\033[32mPASS\033[0m\n"); } while(0)
#define FAIL(fmt, ...) do { tests_failed++; printf("\033[31mFAIL\033[0m  " fmt "\n", ##__VA_ARGS__); } while(0)
#define CHECK(cond, fmt, ...) do { if (cond) { PASS(); } else { FAIL(fmt, ##__VA_ARGS__); } } while(0)

/* ─── Function pointer types (resolved from librustthon at runtime) ─── */
typedef void (*fn_Py_Initialize)(void);
typedef PyObject *(*fn_PyUnicode_FromString)(const char *);
typedef const char *(*fn_PyUnicode_AsUTF8)(PyObject *);
typedef PyObject *(*fn_PyModule_GetDict)(PyObject *);
typedef PyObject *(*fn_PyDict_GetItemString)(PyObject *, const char *);
typedef PyObject *(*fn_PyObject_Call)(PyObject *, PyObject *, PyObject *);
typedef PyObject *(*fn_PyTuple_New)(Py_ssize_t);
typedef int (*fn_PyTuple_SetItem)(PyObject *, Py_ssize_t, PyObject *);
typedef PyObject *(*fn_PyLong_FromLong)(long);
typedef PyObject *(*fn_PyFloat_FromDouble)(double);
typedef long (*fn_PyLong_AsLong)(PyObject *);
typedef double (*fn_PyFloat_AsDouble)(PyObject *);
typedef int (*fn_PyLong_Check)(PyObject *);
typedef int (*fn_PyFloat_Check)(PyObject *);
typedef int (*fn_PyDict_Check)(PyObject *);
typedef int (*fn_PyList_Check)(PyObject *);
typedef PyObject *(*fn_PyDict_New)(void);
typedef int (*fn_PyDict_SetItemString)(PyObject *, const char *, PyObject *);
typedef PyObject *(*fn_PyList_New)(Py_ssize_t);
typedef int (*fn_PyList_Append)(PyObject *, PyObject *);
typedef PyObject *(*fn_PyList_GetItem)(PyObject *, Py_ssize_t);
typedef Py_ssize_t (*fn_PyList_Size)(PyObject *);
typedef PyObject *(*fn_PyDict_GetItem)(PyObject *, PyObject *);
typedef Py_ssize_t (*fn_PyDict_Size)(PyObject *);
typedef int (*fn_PyBool_Check)(PyObject *);
typedef PyObject *(*fn_PyBool_FromLong)(long);
typedef void (*fn_Py_IncRef)(PyObject *);
typedef void (*fn_Py_DecRef)(PyObject *);
typedef PyObject *(*fn_PyErr_Occurred)(void);
typedef void (*fn_PyErr_Clear)(void);
typedef Py_ssize_t (*fn_PyUnicode_GET_LENGTH)(PyObject *);

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
static fn_PyFloat_FromDouble    p_PyFloat_FromDouble;
static fn_PyLong_AsLong         p_PyLong_AsLong;
static fn_PyFloat_AsDouble      p_PyFloat_AsDouble;
static fn_PyLong_Check          p_PyLong_Check;
static fn_PyFloat_Check         p_PyFloat_Check;
static fn_PyDict_Check          p_PyDict_Check;
static fn_PyList_Check          p_PyList_Check;
static fn_PyDict_New            p_PyDict_New;
static fn_PyDict_SetItemString  p_PyDict_SetItemString;
static fn_PyList_New            p_PyList_New;
static fn_PyList_Append         p_PyList_Append;
static fn_PyList_GetItem        p_PyList_GetItem;
static fn_PyList_Size           p_PyList_Size;
static fn_PyDict_GetItem        p_PyDict_GetItem;
static fn_PyDict_Size           p_PyDict_Size;
static fn_PyBool_Check          p_PyBool_Check;
static fn_PyBool_FromLong       p_PyBool_FromLong;
static fn_Py_IncRef             p_Py_IncRef;
static fn_Py_DecRef             p_Py_DecRef;
static fn_PyErr_Occurred        p_PyErr_Occurred;
static fn_PyErr_Clear           p_PyErr_Clear;
static fn_PyUnicode_GET_LENGTH  p_PyUnicode_GET_LENGTH;

/* Singletons resolved from librustthon */
static PyObject *p_Py_None;
static PyObject *p_Py_True;
static PyObject *p_Py_False;

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
    RESOLVE(handle, PyFloat_FromDouble);
    RESOLVE(handle, PyLong_AsLong);
    RESOLVE(handle, PyFloat_AsDouble);
    RESOLVE(handle, PyLong_Check);
    RESOLVE(handle, PyFloat_Check);
    RESOLVE(handle, PyDict_Check);
    RESOLVE(handle, PyList_Check);
    RESOLVE(handle, PyDict_New);
    RESOLVE(handle, PyDict_SetItemString);
    RESOLVE(handle, PyList_New);
    RESOLVE(handle, PyList_Append);
    RESOLVE(handle, PyList_GetItem);
    RESOLVE(handle, PyList_Size);
    RESOLVE(handle, PyDict_GetItem);
    RESOLVE(handle, PyDict_Size);
    RESOLVE(handle, PyBool_Check);
    RESOLVE(handle, PyBool_FromLong);
    RESOLVE(handle, Py_IncRef);
    RESOLVE(handle, Py_DecRef);
    RESOLVE(handle, PyErr_Occurred);
    RESOLVE(handle, PyErr_Clear);
    RESOLVE(handle, PyUnicode_GET_LENGTH);

    /* Singletons — _Py_NoneStruct is a struct, take its address */
    PyObject *none_struct = (PyObject *)dlsym(handle, "_Py_NoneStruct");
    p_Py_None = none_struct;  /* already a pointer to the struct */

    /* _Py_TrueStruct / _Py_FalseStruct are structs too */
    p_Py_True  = (PyObject *)dlsym(handle, "_Py_TrueStruct");
    p_Py_False = (PyObject *)dlsym(handle, "_Py_FalseStruct");
}

/* ═══════════════════════════════════════════════════════════
 *  MARKUPSAFE — Prebuilt _speedups.cpython-311-darwin.so
 * ═══════════════════════════════════════════════════════════ */

static PyObject *ms_escape_func = NULL;

static PyObject *call_escape(const char *input) {
    PyObject *s = p_PyUnicode_FromString(input);
    PyObject *args = p_PyTuple_New(1);
    p_Py_IncRef(s);
    p_PyTuple_SetItem(args, 0, s);
    PyObject *result = p_PyObject_Call(ms_escape_func, args, NULL);
    p_Py_DecRef(s);
    p_Py_DecRef(args);
    return result;
}

static void test_markupsafe_loading(const char *so_path) {
    printf("\n=== MarkupSafe Prebuilt: Module Loading ===\n");

    void *handle = dlopen(so_path, RTLD_LAZY);

    TEST("dlopen(prebuilt _speedups.so) succeeds");
    if (!handle) {
        FAIL("dlopen: %s", dlerror());
        printf("\n  FATAL: Cannot load markupsafe. Skipping markupsafe tests.\n\n");
        return;
    }
    PASS();

    typedef PyObject *(*PyInitFunc)(void);
    PyInitFunc init = (PyInitFunc)dlsym(handle, "PyInit__speedups");

    TEST("dlsym(PyInit__speedups) found");
    if (!init) {
        FAIL("dlsym: %s", dlerror());
        return;
    }
    PASS();

    PyObject *module = init();

    TEST("PyInit__speedups() returns non-null");
    if (!module) {
        if (p_PyErr_Occurred()) {
            printf("  (Python error set)\n");
            p_PyErr_Clear();
        }
        FAIL("returned null");
        return;
    }
    PASS();

    PyObject *dict = p_PyModule_GetDict(module);

    TEST("Module has a __dict__");
    CHECK(dict != NULL, "dict is null");

    ms_escape_func = p_PyDict_GetItemString(dict, "_escape_inner");

    TEST("Module dict has '_escape_inner'");
    CHECK(ms_escape_func != NULL, "not found");
}

static void test_markupsafe_no_escaping(void) {
    if (!ms_escape_func) return;
    printf("\n=== MarkupSafe Prebuilt: No Escaping Needed ===\n");

    PyObject *r;

    r = call_escape("hello world");
    TEST("'hello world' passes through unchanged");
    CHECK(r != NULL && strcmp(p_PyUnicode_AsUTF8(r), "hello world") == 0,
          "got '%s'", r ? p_PyUnicode_AsUTF8(r) : "(null)");
    if (r) p_Py_DecRef(r);

    r = call_escape("");
    TEST("'' (empty) passes through");
    CHECK(r != NULL && strcmp(p_PyUnicode_AsUTF8(r), "") == 0,
          "got '%s'", r ? p_PyUnicode_AsUTF8(r) : "(null)");
    if (r) p_Py_DecRef(r);

    r = call_escape("abc123");
    TEST("'abc123' passes through unchanged");
    CHECK(r != NULL && strcmp(p_PyUnicode_AsUTF8(r), "abc123") == 0,
          "got '%s'", r ? p_PyUnicode_AsUTF8(r) : "(null)");
    if (r) p_Py_DecRef(r);

    r = call_escape("no special chars here");
    TEST("'no special chars here' passes through");
    CHECK(r != NULL && strcmp(p_PyUnicode_AsUTF8(r), "no special chars here") == 0,
          "got '%s'", r ? p_PyUnicode_AsUTF8(r) : "(null)");
    if (r) p_Py_DecRef(r);
}

static void test_markupsafe_escaping(void) {
    if (!ms_escape_func) return;
    printf("\n=== MarkupSafe Prebuilt: HTML Entity Escaping ===\n");

    PyObject *r;

    r = call_escape("<script>");
    TEST("'<script>' escapes to '&lt;script&gt;'");
    CHECK(r != NULL && strcmp(p_PyUnicode_AsUTF8(r), "&lt;script&gt;") == 0,
          "got '%s'", r ? p_PyUnicode_AsUTF8(r) : "(null)");
    if (r) p_Py_DecRef(r);

    r = call_escape("a&b");
    TEST("'a&b' escapes to 'a&amp;b'");
    CHECK(r != NULL && strcmp(p_PyUnicode_AsUTF8(r), "a&amp;b") == 0,
          "got '%s'", r ? p_PyUnicode_AsUTF8(r) : "(null)");
    if (r) p_Py_DecRef(r);

    r = call_escape("say \"hello\"");
    TEST("double quotes escape to &#34;");
    CHECK(r != NULL && strcmp(p_PyUnicode_AsUTF8(r), "say &#34;hello&#34;") == 0,
          "got '%s'", r ? p_PyUnicode_AsUTF8(r) : "(null)");
    if (r) p_Py_DecRef(r);

    r = call_escape("it's");
    TEST("single quote escapes to &#39;");
    CHECK(r != NULL && strcmp(p_PyUnicode_AsUTF8(r), "it&#39;s") == 0,
          "got '%s'", r ? p_PyUnicode_AsUTF8(r) : "(null)");
    if (r) p_Py_DecRef(r);

    r = call_escape("<b>\"Tom & Jerry's\"</b>");
    TEST("All 5 special chars escaped together");
    CHECK(r != NULL && strcmp(p_PyUnicode_AsUTF8(r),
          "&lt;b&gt;&#34;Tom &amp; Jerry&#39;s&#34;&lt;/b&gt;") == 0,
          "got '%s'", r ? p_PyUnicode_AsUTF8(r) : "(null)");
    if (r) p_Py_DecRef(r);
}

static void test_markupsafe_edge_cases(void) {
    if (!ms_escape_func) return;
    printf("\n=== MarkupSafe Prebuilt: Edge Cases ===\n");

    PyObject *r;

    r = call_escape("<>&'\"");
    TEST("'<>&\\'\"' — all specials");
    CHECK(r != NULL && strcmp(p_PyUnicode_AsUTF8(r), "&lt;&gt;&amp;&#39;&#34;") == 0,
          "got '%s'", r ? p_PyUnicode_AsUTF8(r) : "(null)");
    if (r) p_Py_DecRef(r);

    r = call_escape("<");
    TEST("'<' → '&lt;'");
    CHECK(r != NULL && strcmp(p_PyUnicode_AsUTF8(r), "&lt;") == 0,
          "got '%s'", r ? p_PyUnicode_AsUTF8(r) : "(null)");
    if (r) p_Py_DecRef(r);

    r = call_escape("aaaaaaaaaaaaaaaaaaaaaaaaaaaa<");
    TEST("Long string with '<' at end");
    CHECK(r != NULL && strcmp(p_PyUnicode_AsUTF8(r), "aaaaaaaaaaaaaaaaaaaaaaaaaaaa&lt;") == 0,
          "got '%s'", r ? p_PyUnicode_AsUTF8(r) : "(null)");
    if (r) p_Py_DecRef(r);

    r = call_escape("&start");
    TEST("'&start' → '&amp;start'");
    CHECK(r != NULL && strcmp(p_PyUnicode_AsUTF8(r), "&amp;start") == 0,
          "got '%s'", r ? p_PyUnicode_AsUTF8(r) : "(null)");
    if (r) p_Py_DecRef(r);
}

/* ═══════════════════════════════════════════════════════════
 *  UJSON — Prebuilt ujson.cpython-311-darwin.so
 * ═══════════════════════════════════════════════════════════ */

static PyObject *uj_encode_func = NULL;
static PyObject *uj_decode_func = NULL;

static const char *uj_encode(PyObject *obj) {
    PyObject *args = p_PyTuple_New(1);
    p_Py_IncRef(obj);
    p_PyTuple_SetItem(args, 0, obj);
    PyObject *result = p_PyObject_Call(uj_encode_func, args, NULL);
    p_Py_DecRef(args);
    if (!result) {
        if (p_PyErr_Occurred()) p_PyErr_Clear();
        return NULL;
    }
    return p_PyUnicode_AsUTF8(result);
}

static PyObject *uj_decode(const char *json) {
    PyObject *s = p_PyUnicode_FromString(json);
    PyObject *args = p_PyTuple_New(1);
    p_Py_IncRef(s);
    p_PyTuple_SetItem(args, 0, s);
    PyObject *result = p_PyObject_Call(uj_decode_func, args, NULL);
    p_Py_DecRef(s);
    p_Py_DecRef(args);
    if (!result && p_PyErr_Occurred()) p_PyErr_Clear();
    return result;
}

static void test_ujson_loading(const char *so_path) {
    printf("\n=== ujson Prebuilt: Module Loading ===\n");

    void *handle = dlopen(so_path, RTLD_LAZY);

    TEST("dlopen(prebuilt ujson.so) succeeds");
    if (!handle) {
        FAIL("dlopen: %s", dlerror());
        printf("\n  FATAL: Cannot load ujson. Skipping ujson tests.\n\n");
        return;
    }
    PASS();

    typedef PyObject *(*PyInitFunc)(void);
    PyInitFunc init = (PyInitFunc)dlsym(handle, "PyInit_ujson");

    TEST("dlsym(PyInit_ujson) found");
    if (!init) {
        FAIL("dlsym: %s", dlerror());
        return;
    }
    PASS();

    PyObject *module = init();

    TEST("PyInit_ujson() returns non-null");
    if (!module) {
        if (p_PyErr_Occurred()) {
            printf("  (Python error set)\n");
            p_PyErr_Clear();
        }
        FAIL("returned null");
        return;
    }
    PASS();

    PyObject *dict = p_PyModule_GetDict(module);

    TEST("Module has a __dict__");
    CHECK(dict != NULL, "dict is null");

    uj_encode_func = p_PyDict_GetItemString(dict, "encode");
    TEST("Module dict has 'encode'");
    CHECK(uj_encode_func != NULL, "not found");

    uj_decode_func = p_PyDict_GetItemString(dict, "decode");
    TEST("Module dict has 'decode'");
    CHECK(uj_decode_func != NULL, "not found");
}

static void test_ujson_encode_primitives(void) {
    if (!uj_encode_func) return;
    printf("\n=== ujson Prebuilt: Encode Primitives ===\n");

    const char *r;

    /* Integers */
    r = uj_encode(p_PyLong_FromLong(0));
    TEST("encode(0) → '0'");
    CHECK(r && strcmp(r, "0") == 0, "got '%s'", r ? r : "(null)");

    r = uj_encode(p_PyLong_FromLong(42));
    TEST("encode(42) → '42'");
    CHECK(r && strcmp(r, "42") == 0, "got '%s'", r ? r : "(null)");

    r = uj_encode(p_PyLong_FromLong(-1));
    TEST("encode(-1) → '-1'");
    CHECK(r && strcmp(r, "-1") == 0, "got '%s'", r ? r : "(null)");

    r = uj_encode(p_PyLong_FromLong(999999));
    TEST("encode(999999) → '999999'");
    CHECK(r && strcmp(r, "999999") == 0, "got '%s'", r ? r : "(null)");

    /* Floats */
    r = uj_encode(p_PyFloat_FromDouble(3.14));
    TEST("encode(3.14) starts with '3.14'");
    CHECK(r && strncmp(r, "3.14", 4) == 0, "got '%s'", r ? r : "(null)");

    r = uj_encode(p_PyFloat_FromDouble(0.0));
    TEST("encode(0.0) → '0.0'");
    CHECK(r && strcmp(r, "0.0") == 0, "got '%s'", r ? r : "(null)");

    /* Strings */
    PyObject *s = p_PyUnicode_FromString("hello");
    r = uj_encode(s);
    TEST("encode('hello') → '\"hello\"'");
    CHECK(r && strcmp(r, "\"hello\"") == 0, "got '%s'", r ? r : "(null)");

    /* Booleans */
    r = uj_encode(p_Py_True);
    TEST("encode(True) → 'true'");
    CHECK(r && strcmp(r, "true") == 0, "got '%s'", r ? r : "(null)");

    r = uj_encode(p_Py_False);
    TEST("encode(False) → 'false'");
    CHECK(r && strcmp(r, "false") == 0, "got '%s'", r ? r : "(null)");

    /* None */
    r = uj_encode(p_Py_None);
    TEST("encode(None) → 'null'");
    CHECK(r && strcmp(r, "null") == 0, "got '%s'", r ? r : "(null)");
}

static void test_ujson_encode_containers(void) {
    if (!uj_encode_func) return;
    printf("\n=== ujson Prebuilt: Encode Containers ===\n");

    const char *r;

    /* Empty list */
    r = uj_encode(p_PyList_New(0));
    TEST("encode([]) → '[]'");
    CHECK(r && strcmp(r, "[]") == 0, "got '%s'", r ? r : "(null)");

    /* List with ints */
    PyObject *list = p_PyList_New(0);
    p_PyList_Append(list, p_PyLong_FromLong(1));
    p_PyList_Append(list, p_PyLong_FromLong(2));
    p_PyList_Append(list, p_PyLong_FromLong(3));
    r = uj_encode(list);
    TEST("encode([1,2,3]) → '[1,2,3]'");
    CHECK(r && strcmp(r, "[1,2,3]") == 0, "got '%s'", r ? r : "(null)");

    /* Empty dict */
    r = uj_encode(p_PyDict_New());
    TEST("encode({}) → '{}'");
    CHECK(r && strcmp(r, "{}") == 0, "got '%s'", r ? r : "(null)");

    /* Dict with string keys */
    PyObject *d = p_PyDict_New();
    p_PyDict_SetItemString(d, "a", p_PyLong_FromLong(1));
    r = uj_encode(d);
    TEST("encode({'a': 1}) → '{\"a\":1}'");
    CHECK(r && strcmp(r, "{\"a\":1}") == 0, "got '%s'", r ? r : "(null)");

    /* Nested */
    PyObject *outer = p_PyDict_New();
    PyObject *inner_list = p_PyList_New(0);
    p_PyList_Append(inner_list, p_PyLong_FromLong(1));
    p_PyList_Append(inner_list, p_PyLong_FromLong(2));
    p_PyDict_SetItemString(outer, "nums", inner_list);
    r = uj_encode(outer);
    TEST("encode({'nums': [1,2]}) → nested JSON");
    CHECK(r && strcmp(r, "{\"nums\":[1,2]}") == 0, "got '%s'", r ? r : "(null)");
}

static void test_ujson_decode_primitives(void) {
    if (!uj_decode_func) return;
    printf("\n=== ujson Prebuilt: Decode Primitives ===\n");

    PyObject *r;

    /* Integers */
    r = uj_decode("42");
    TEST("decode('42') → int 42");
    CHECK(r && p_PyLong_Check(r) && p_PyLong_AsLong(r) == 42,
          "got %ld", r ? p_PyLong_AsLong(r) : -999);

    r = uj_decode("0");
    TEST("decode('0') → int 0");
    CHECK(r && p_PyLong_Check(r) && p_PyLong_AsLong(r) == 0,
          "got %ld", r ? p_PyLong_AsLong(r) : -999);

    r = uj_decode("-100");
    TEST("decode('-100') → int -100");
    CHECK(r && p_PyLong_Check(r) && p_PyLong_AsLong(r) == -100,
          "got %ld", r ? p_PyLong_AsLong(r) : -999);

    /* Floats */
    r = uj_decode("3.14");
    TEST("decode('3.14') → float ~3.14");
    CHECK(r && p_PyFloat_Check(r) && fabs(p_PyFloat_AsDouble(r) - 3.14) < 0.001,
          "got %f", r ? p_PyFloat_AsDouble(r) : -999.0);

    /* Strings */
    r = uj_decode("\"hello\"");
    TEST("decode('\"hello\"') → str 'hello'");
    CHECK(r && strcmp(p_PyUnicode_AsUTF8(r), "hello") == 0,
          "got '%s'", r ? p_PyUnicode_AsUTF8(r) : "(null)");

    /* Booleans */
    r = uj_decode("true");
    TEST("decode('true') → True");
    CHECK(r && r == p_Py_True, "not True");

    r = uj_decode("false");
    TEST("decode('false') → False");
    CHECK(r && r == p_Py_False, "not False");

    /* Null */
    r = uj_decode("null");
    TEST("decode('null') → None");
    CHECK(r && r == p_Py_None, "not None");
}

static void test_ujson_decode_containers(void) {
    if (!uj_decode_func) return;
    printf("\n=== ujson Prebuilt: Decode Containers ===\n");

    PyObject *r;

    /* Empty list */
    r = uj_decode("[]");
    TEST("decode('[]') → empty list");
    CHECK(r && p_PyList_Check(r) && p_PyList_Size(r) == 0,
          "size=%ld", r ? p_PyList_Size(r) : -1);

    /* List of ints */
    r = uj_decode("[1,2,3]");
    TEST("decode('[1,2,3]') → list of 3");
    CHECK(r && p_PyList_Check(r) && p_PyList_Size(r) == 3,
          "size=%ld", r ? p_PyList_Size(r) : -1);

    if (r && p_PyList_Check(r) && p_PyList_Size(r) == 3) {
        TEST("  [0]=1, [1]=2, [2]=3");
        CHECK(p_PyLong_AsLong(p_PyList_GetItem(r, 0)) == 1 &&
              p_PyLong_AsLong(p_PyList_GetItem(r, 1)) == 2 &&
              p_PyLong_AsLong(p_PyList_GetItem(r, 2)) == 3,
              "wrong values");
    }

    /* Empty object */
    r = uj_decode("{}");
    TEST("decode('{}') → empty dict");
    CHECK(r && p_PyDict_Check(r) && p_PyDict_Size(r) == 0,
          "size=%ld", r ? p_PyDict_Size(r) : -1);

    /* Object with values */
    r = uj_decode("{\"x\":10,\"y\":20}");
    TEST("decode('{\"x\":10,\"y\":20}') → dict of 2");
    CHECK(r && p_PyDict_Check(r) && p_PyDict_Size(r) == 2,
          "size=%ld", r ? p_PyDict_Size(r) : -1);

    if (r && p_PyDict_Check(r)) {
        PyObject *key_x = p_PyUnicode_FromString("x");
        PyObject *val_x = p_PyDict_GetItem(r, key_x);
        TEST("  d['x'] == 10");
        CHECK(val_x && p_PyLong_AsLong(val_x) == 10,
              "got %ld", val_x ? p_PyLong_AsLong(val_x) : -999);
    }

    /* Nested */
    r = uj_decode("{\"items\":[1,2],\"ok\":true}");
    TEST("decode nested object → dict of 2");
    CHECK(r && p_PyDict_Check(r) && p_PyDict_Size(r) == 2,
          "size=%ld", r ? p_PyDict_Size(r) : -1);
}

static void test_ujson_roundtrip(void) {
    if (!uj_encode_func || !uj_decode_func) return;
    printf("\n=== ujson Prebuilt: Encode/Decode Roundtrip ===\n");

    const char *json;
    PyObject *r;

    /* int roundtrip */
    PyObject *val = p_PyLong_FromLong(12345);
    json = uj_encode(val);
    r = uj_decode(json);
    TEST("roundtrip int 12345");
    CHECK(r && p_PyLong_AsLong(r) == 12345, "got %ld", r ? p_PyLong_AsLong(r) : -999);

    /* string roundtrip */
    val = p_PyUnicode_FromString("hello world");
    json = uj_encode(val);
    r = uj_decode(json);
    TEST("roundtrip string 'hello world'");
    CHECK(r && strcmp(p_PyUnicode_AsUTF8(r), "hello world") == 0,
          "got '%s'", r ? p_PyUnicode_AsUTF8(r) : "(null)");

    /* list roundtrip */
    PyObject *list = p_PyList_New(0);
    p_PyList_Append(list, p_PyLong_FromLong(10));
    p_PyList_Append(list, p_PyLong_FromLong(20));
    p_PyList_Append(list, p_PyLong_FromLong(30));
    json = uj_encode(list);
    r = uj_decode(json);
    TEST("roundtrip [10,20,30]");
    CHECK(r && p_PyList_Check(r) && p_PyList_Size(r) == 3 &&
          p_PyLong_AsLong(p_PyList_GetItem(r, 1)) == 20,
          "failed");

    /* bool roundtrip */
    json = uj_encode(p_Py_True);
    r = uj_decode(json);
    TEST("roundtrip True");
    CHECK(r == p_Py_True, "not True");

    json = uj_encode(p_Py_None);
    r = uj_decode(json);
    TEST("roundtrip None");
    CHECK(r == p_Py_None, "not None");
}

/* ═══════════════════════════════════════════════════════════
 *  MAIN
 * ═══════════════════════════════════════════════════════════ */

int main(int argc, char *argv[]) {
    /* Default paths — override with env vars or arguments */
    const char *rustthon_path = getenv("RUSTTHON_LIB");
    if (!rustthon_path) rustthon_path = "target/release/librustthon.dylib";

    const char *ms_path = getenv("MARKUPSAFE_SO");
    if (!ms_path) ms_path = "/tmp/prebuilt_ext/markupsafe/_speedups.cpython-311-darwin.so";

    const char *uj_path = getenv("UJSON_SO");
    if (!uj_path) uj_path = "/tmp/prebuilt_ext/ujson.cpython-311-darwin.so";

    printf("╔══════════════════════════════════════════════════════════╗\n");
    printf("║  Rustthon Phase 4: Prebuilt CPython 3.11 Extensions     ║\n");
    printf("║  Loading REAL pip wheel .so files (not our compilation)  ║\n");
    printf("║  Two-level namespace: RTLD_GLOBAL on librustthon.dylib  ║\n");
    printf("╚══════════════════════════════════════════════════════════╝\n");

    /* ─── Step 1: Load librustthon.dylib with RTLD_GLOBAL ─── */
    printf("\n=== Loading Rustthon Runtime ===\n");

    TEST("dlopen(librustthon.dylib, RTLD_GLOBAL | RTLD_LAZY)");
    void *rt = dlopen(rustthon_path, RTLD_GLOBAL | RTLD_LAZY);
    if (!rt) {
        FAIL("dlopen: %s", dlerror());
        printf("\n  FATAL: Cannot load librustthon.dylib\n");
        printf("  Make sure to run: cargo build --release\n\n");
        return 1;
    }
    PASS();

    /* ─── Step 2: Resolve Rustthon API ─── */
    resolve_api(rt);

    TEST("Py_Initialize resolved");
    CHECK(p_Py_Initialize != NULL, "null");

    TEST("_Py_NoneStruct resolved");
    CHECK(p_Py_None != NULL, "null");

    TEST("_Py_TrueStruct resolved");
    CHECK(p_Py_True != NULL, "null");

    TEST("_Py_FalseStruct resolved");
    CHECK(p_Py_False != NULL, "null");

    /* ─── Step 3: Initialize Rustthon ─── */
    printf("\n=== Initializing Rustthon Runtime ===\n");
    TEST("Py_Initialize() succeeds");
    p_Py_Initialize();
    PASS();

    /* Verify singletons are valid after init */
    TEST("None singleton has non-null ob_type");
    CHECK(p_Py_None->ob_type != NULL, "ob_type is null");

    TEST("True singleton has non-null ob_type");
    CHECK(p_Py_True->ob_type != NULL, "ob_type is null");

    TEST("False singleton has non-null ob_type");
    CHECK(p_Py_False->ob_type != NULL, "ob_type is null");

    /* ─── Step 4: Test prebuilt markupsafe ─── */
    test_markupsafe_loading(ms_path);
    test_markupsafe_no_escaping();
    test_markupsafe_escaping();
    test_markupsafe_edge_cases();

    /* ─── Step 5: Test prebuilt ujson ─── */
    test_ujson_loading(uj_path);
    test_ujson_encode_primitives();
    test_ujson_encode_containers();
    test_ujson_decode_primitives();
    test_ujson_decode_containers();
    test_ujson_roundtrip();

    /* ─── Summary ─── */
    printf("\n═══════════════════════════════════════════════════════════\n");
    printf("  Total: %d  |  ", tests_run);
    if (tests_failed == 0) {
        printf("\033[32mPassed: %d\033[0m  |  Failed: %d\n", tests_passed, tests_failed);
        printf("\n  \033[32m✓ ALL TESTS PASSED — Prebuilt wheels work on Rustthon!\033[0m\n");
    } else {
        printf("Passed: %d  |  \033[31mFailed: %d\033[0m\n", tests_passed, tests_failed);
        printf("\n  \033[31m✗ SOME TESTS FAILED\033[0m\n");
    }
    printf("═══════════════════════════════════════════════════════════\n\n");

    return tests_failed > 0 ? 1 : 0;
}
