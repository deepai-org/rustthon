/*
 * Phase 3a: MarkupSafe _speedups Test Driver
 *
 * Loads the real markupsafe _speedups.dylib (compiled from PyPI source)
 * and exercises _escape_inner with various inputs.
 *
 * Build:
 *   cc -o test_markupsafe tests/test_markupsafe.c \
 *      -L target/release -lrustthon -Wl,-rpath,target/release
 *
 * Run:
 *   ./test_markupsafe
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <dlfcn.h>

typedef intptr_t Py_ssize_t;
typedef struct _object {
    Py_ssize_t ob_refcnt;
    struct _typeobject *ob_type;
} PyObject;

/* ─── Extern declarations ─── */
extern void Py_Initialize(void);
extern PyObject *PyUnicode_FromString(const char *s);
extern const char *PyUnicode_AsUTF8(PyObject *obj);
extern Py_ssize_t PyUnicode_GET_LENGTH(PyObject *obj);
extern int PyUnicode_Check(PyObject *obj);
extern PyObject *PyModule_GetDict(PyObject *module);
extern PyObject *PyDict_GetItemString(PyObject *dict, const char *key);
extern PyObject *PyObject_Call(PyObject *callable, PyObject *args, PyObject *kwargs);
extern PyObject *PyTuple_New(Py_ssize_t size);
extern int PyTuple_SetItem(PyObject *tuple, Py_ssize_t i, PyObject *v);
extern PyObject *_Py_None(void);
extern void Py_IncRef(PyObject *o);
extern void Py_DecRef(PyObject *o);

/* ─── Test infrastructure ─── */
static int tests_run = 0;
static int tests_passed = 0;
static int tests_failed = 0;

#define TEST(name) do { tests_run++; printf("  %-55s ", name); } while(0)
#define PASS() do { tests_passed++; printf("\033[32mPASS\033[0m\n"); } while(0)
#define FAIL(fmt, ...) do { tests_failed++; printf("\033[31mFAIL\033[0m  " fmt "\n", ##__VA_ARGS__); } while(0)
#define CHECK(cond, fmt, ...) do { if (cond) { PASS(); } else { FAIL(fmt, ##__VA_ARGS__); } } while(0)

/* Helper: call _escape_inner(s) */
static PyObject *escape_func = NULL;

static PyObject *call_escape(const char *input) {
    PyObject *s = PyUnicode_FromString(input);
    PyObject *args = PyTuple_New(1);
    Py_IncRef(s);
    PyTuple_SetItem(args, 0, s);
    PyObject *result = PyObject_Call(escape_func, args, NULL);
    Py_DecRef(s);
    Py_DecRef(args);
    return result;
}

/* ═══════════════════════════════════════════════════════
 *  Tests
 * ═══════════════════════════════════════════════════════ */

void test_module_loading(void) {
    printf("\n=== MarkupSafe Module Loading ===\n");

    void *handle = dlopen("./_markupsafe_speedups.dylib", RTLD_NOW | RTLD_GLOBAL);

    TEST("dlopen(_markupsafe_speedups.dylib) succeeds");
    if (!handle) {
        FAIL("dlopen: %s", dlerror());
        printf("\n  FATAL: Cannot continue.\n\n");
        exit(1);
    } else {
        PASS();
    }

    typedef PyObject *(*PyInitFunc)(void);
    PyInitFunc init = (PyInitFunc)dlsym(handle, "PyInit__speedups");

    TEST("dlsym(PyInit__speedups) found");
    if (!init) {
        FAIL("dlsym: %s", dlerror());
        exit(1);
    } else {
        PASS();
    }

    PyObject *module = init();

    TEST("PyInit__speedups() returns non-null");
    CHECK(module != NULL, "returned null");

    PyObject *dict = PyModule_GetDict(module);

    TEST("Module has a __dict__");
    CHECK(dict != NULL, "dict is null");

    escape_func = PyDict_GetItemString(dict, "_escape_inner");

    TEST("Module dict has '_escape_inner'");
    CHECK(escape_func != NULL, "not found");
}

void test_no_escaping_needed(void) {
    printf("\n=== Strings That Need No Escaping ===\n");

    PyObject *r;

    r = call_escape("hello world");
    TEST("'hello world' passes through unchanged");
    CHECK(r != NULL && strcmp(PyUnicode_AsUTF8(r), "hello world") == 0,
          "got '%s'", r ? PyUnicode_AsUTF8(r) : "(null)");
    if (r) Py_DecRef(r);

    r = call_escape("");
    TEST("'' (empty) passes through");
    CHECK(r != NULL && strcmp(PyUnicode_AsUTF8(r), "") == 0,
          "got '%s'", r ? PyUnicode_AsUTF8(r) : "(null)");
    if (r) Py_DecRef(r);

    r = call_escape("abc123");
    TEST("'abc123' passes through unchanged");
    CHECK(r != NULL && strcmp(PyUnicode_AsUTF8(r), "abc123") == 0,
          "got '%s'", r ? PyUnicode_AsUTF8(r) : "(null)");
    if (r) Py_DecRef(r);

    r = call_escape("no special chars here");
    TEST("'no special chars here' passes through");
    CHECK(r != NULL && strcmp(PyUnicode_AsUTF8(r), "no special chars here") == 0,
          "got '%s'", r ? PyUnicode_AsUTF8(r) : "(null)");
    if (r) Py_DecRef(r);
}

void test_html_escaping(void) {
    printf("\n=== HTML Entity Escaping ===\n");

    PyObject *r;

    /* < and > */
    r = call_escape("<script>");
    TEST("'<script>' escapes to '&lt;script&gt;'");
    CHECK(r != NULL && strcmp(PyUnicode_AsUTF8(r), "&lt;script&gt;") == 0,
          "got '%s'", r ? PyUnicode_AsUTF8(r) : "(null)");
    if (r) Py_DecRef(r);

    /* & */
    r = call_escape("a&b");
    TEST("'a&b' escapes to 'a&amp;b'");
    CHECK(r != NULL && strcmp(PyUnicode_AsUTF8(r), "a&amp;b") == 0,
          "got '%s'", r ? PyUnicode_AsUTF8(r) : "(null)");
    if (r) Py_DecRef(r);

    /* " (double quote) → &#34; */
    r = call_escape("say \"hello\"");
    TEST("double quotes escape to &#34;");
    CHECK(r != NULL && strcmp(PyUnicode_AsUTF8(r), "say &#34;hello&#34;") == 0,
          "got '%s'", r ? PyUnicode_AsUTF8(r) : "(null)");
    if (r) Py_DecRef(r);

    /* ' (single quote) → &#39; */
    r = call_escape("it's");
    TEST("single quote escapes to &#39;");
    CHECK(r != NULL && strcmp(PyUnicode_AsUTF8(r), "it&#39;s") == 0,
          "got '%s'", r ? PyUnicode_AsUTF8(r) : "(null)");
    if (r) Py_DecRef(r);

    /* All special chars at once */
    r = call_escape("<b>\"Tom & Jerry's\"</b>");
    TEST("All 5 special chars escaped together");
    CHECK(r != NULL && strcmp(PyUnicode_AsUTF8(r),
          "&lt;b&gt;&#34;Tom &amp; Jerry&#39;s&#34;&lt;/b&gt;") == 0,
          "got '%s'", r ? PyUnicode_AsUTF8(r) : "(null)");
    if (r) Py_DecRef(r);
}

void test_edge_cases(void) {
    printf("\n=== Edge Cases ===\n");

    PyObject *r;

    /* Only special chars */
    r = call_escape("<>&'\"");
    TEST("'<>&\\'\"' — all specials");
    CHECK(r != NULL && strcmp(PyUnicode_AsUTF8(r), "&lt;&gt;&amp;&#39;&#34;") == 0,
          "got '%s'", r ? PyUnicode_AsUTF8(r) : "(null)");
    if (r) Py_DecRef(r);

    /* Single special char */
    r = call_escape("<");
    TEST("'<' → '&lt;'");
    CHECK(r != NULL && strcmp(PyUnicode_AsUTF8(r), "&lt;") == 0,
          "got '%s'", r ? PyUnicode_AsUTF8(r) : "(null)");
    if (r) Py_DecRef(r);

    /* Long string with one special char at the end */
    r = call_escape("aaaaaaaaaaaaaaaaaaaaaaaaaaaa<");
    TEST("Long string with '<' at end");
    CHECK(r != NULL && strcmp(PyUnicode_AsUTF8(r), "aaaaaaaaaaaaaaaaaaaaaaaaaaaa&lt;") == 0,
          "got '%s'", r ? PyUnicode_AsUTF8(r) : "(null)");
    if (r) Py_DecRef(r);

    /* Special char at start */
    r = call_escape("&start");
    TEST("'&start' → '&amp;start'");
    CHECK(r != NULL && strcmp(PyUnicode_AsUTF8(r), "&amp;start") == 0,
          "got '%s'", r ? PyUnicode_AsUTF8(r) : "(null)");
    if (r) Py_DecRef(r);
}

int main(void) {
    printf("╔══════════════════════════════════════════════════════════╗\n");
    printf("║  Rustthon Phase 3a: MarkupSafe _speedups (from PyPI)    ║\n");
    printf("║  Real-world C extension compiled against Python.h       ║\n");
    printf("║  PEP 393 Unicode macros + PyUnicode_New + Py_DECREF     ║\n");
    printf("╚══════════════════════════════════════════════════════════╝\n");

    Py_Initialize();

    test_module_loading();
    test_no_escaping_needed();
    test_html_escaping();
    test_edge_cases();

    printf("\n═══════════════════════════════════════════════════════════\n");
    printf("  Total: %d  |  ", tests_run);
    if (tests_failed == 0) {
        printf("\033[32mPassed: %d\033[0m  |  Failed: %d\n", tests_passed, tests_failed);
        printf("\n  \033[32m✓ ALL TESTS PASSED — MarkupSafe works on Rustthon!\033[0m\n");
    } else {
        printf("Passed: %d  |  \033[31mFailed: %d\033[0m\n", tests_passed, tests_failed);
        printf("\n  \033[31m✗ SOME TESTS FAILED\033[0m\n");
    }
    printf("═══════════════════════════════════════════════════════════\n\n");

    return tests_failed > 0 ? 1 : 0;
}
