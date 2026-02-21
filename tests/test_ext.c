/*
 * Phase 3: A proper C extension module for Rustthon
 *
 * This follows the exact same pattern that every CPython C extension uses:
 *   - PyModuleDef with method table
 *   - PyInit_<name> entry point
 *   - Methods using PyArg_ParseTuple / Py_BuildValue
 *   - Methods using METH_O, METH_NOARGS
 *
 * Build as shared library:
 *   cc -shared -o _testmod.dylib tests/test_ext.c \
 *      -L target/release -lrustthon -Wl,-rpath,target/release
 *
 * Then test with the driver program (test_ext_driver.c).
 */

#include <stdio.h>
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <stdlib.h>

/* ─── Minimal CPython type declarations ─── */

typedef intptr_t Py_ssize_t;

typedef struct _object {
    Py_ssize_t ob_refcnt;
    struct _typeobject *ob_type;
} PyObject;

typedef PyObject *(*PyCFunction)(PyObject *, PyObject *);

typedef struct {
    const char *ml_name;
    PyCFunction ml_meth;
    int ml_flags;
    const char *ml_doc;
} PyMethodDef;

typedef struct {
    PyObject ob_base;
    PyObject *(*m_init)(void);
    Py_ssize_t m_index;
    PyObject *m_copy;
} PyModuleDef_Base;

typedef struct {
    int slot;
    void *value;
} PyModuleDef_Slot;

typedef struct {
    PyModuleDef_Base m_base;
    const char *m_name;
    const char *m_doc;
    Py_ssize_t m_size;
    PyMethodDef *m_methods;
    PyModuleDef_Slot *m_slots;
    int (*m_traverse)(PyObject *, void *, void *);
    int (*m_clear)(PyObject *);
    void (*m_free)(void *);
} PyModuleDef;

#define METH_VARARGS  0x0001
#define METH_KEYWORDS 0x0002
#define METH_NOARGS   0x0004
#define METH_O        0x0008

#define PyModuleDef_HEAD_INIT { {1, NULL}, NULL, 0, NULL }

/* ─── Extern declarations ─── */

extern PyObject *PyModule_Create2(PyModuleDef *def, int api_version);
extern int PyArg_ParseTuple(PyObject *args, const char *format, ...);
extern PyObject *Py_BuildValue(const char *format, ...);

extern PyObject *PyLong_FromLong(long v);
extern long PyLong_AsLong(PyObject *obj);
extern PyObject *PyFloat_FromDouble(double v);
extern double PyFloat_AsDouble(PyObject *obj);
extern PyObject *PyUnicode_FromString(const char *s);
extern const char *PyUnicode_AsUTF8(PyObject *obj);
extern Py_ssize_t PyUnicode_GET_LENGTH(PyObject *obj);

extern PyObject *PyList_New(Py_ssize_t size);
extern int PyList_Append(PyObject *list, PyObject *item);
extern Py_ssize_t PyList_Size(PyObject *list);
extern PyObject *PyList_GetItem(PyObject *list, Py_ssize_t i);

extern PyObject *PyTuple_New(Py_ssize_t size);
extern int PyTuple_SetItem(PyObject *tuple, Py_ssize_t i, PyObject *v);

extern PyObject *_Py_None(void);
extern void Py_IncRef(PyObject *o);
extern void Py_DecRef(PyObject *o);

#define Py_RETURN_NONE do { PyObject *n = _Py_None(); Py_IncRef(n); return n; } while(0)

/* ═══════════════════════════════════════════════════════
 *  Module methods
 * ═══════════════════════════════════════════════════════ */

/* add(a, b) → a + b (integers) */
static PyObject *testmod_add(PyObject *self, PyObject *args) {
    int a, b;
    if (!PyArg_ParseTuple(args, "ii", &a, &b))
        return NULL;
    return Py_BuildValue("i", a + b);
}

/* multiply(a, b) → a * b (doubles) */
static PyObject *testmod_multiply(PyObject *self, PyObject *args) {
    double a, b;
    if (!PyArg_ParseTuple(args, "dd", &a, &b))
        return NULL;
    return Py_BuildValue("d", a * b);
}

/* greet(name) → "Hello, <name>!" */
static PyObject *testmod_greet(PyObject *self, PyObject *args) {
    const char *name;
    if (!PyArg_ParseTuple(args, "s", &name))
        return NULL;

    char buf[256];
    snprintf(buf, sizeof(buf), "Hello, %s!", name);
    return PyUnicode_FromString(buf);
}

/* strlen(s) → length of string (uses METH_O) */
static PyObject *testmod_strlen(PyObject *self, PyObject *arg) {
    const char *s = PyUnicode_AsUTF8(arg);
    if (!s) return NULL;
    Py_ssize_t len = PyUnicode_GET_LENGTH(arg);
    return PyLong_FromLong((long)len);
}

/* noop() → None (uses METH_NOARGS) */
static PyObject *testmod_noop(PyObject *self, PyObject *unused) {
    Py_RETURN_NONE;
}

/* make_list(n) → [0, 1, 2, ..., n-1] */
static PyObject *testmod_make_list(PyObject *self, PyObject *args) {
    int n;
    if (!PyArg_ParseTuple(args, "i", &n))
        return NULL;

    PyObject *list = PyList_New(0);
    for (int i = 0; i < n; i++) {
        PyObject *item = PyLong_FromLong(i);
        PyList_Append(list, item);
        Py_DecRef(item);
    }
    return list;
}

/* sum_list(lst) → sum of all ints in list (uses METH_O) */
static PyObject *testmod_sum_list(PyObject *self, PyObject *list) {
    Py_ssize_t n = PyList_Size(list);
    long total = 0;
    for (Py_ssize_t i = 0; i < n; i++) {
        PyObject *item = PyList_GetItem(list, i);
        total += PyLong_AsLong(item);
    }
    return PyLong_FromLong(total);
}

/* mixed_return(i, d, s) → (i*2, d*2.0, "got: <s>") — tests Py_BuildValue with tuple */
static PyObject *testmod_mixed_return(PyObject *self, PyObject *args) {
    int i;
    double d;
    const char *s;
    if (!PyArg_ParseTuple(args, "ids", &i, &d, &s))
        return NULL;

    char buf[256];
    snprintf(buf, sizeof(buf), "got: %s", s);
    return Py_BuildValue("(ids)", i * 2, d * 2.0, buf);
}

/* pass_through(obj) → obj (tests O format in ParseTuple and BuildValue) */
static PyObject *testmod_pass_through(PyObject *self, PyObject *args) {
    PyObject *obj;
    if (!PyArg_ParseTuple(args, "O", &obj))
        return NULL;
    return Py_BuildValue("O", obj);
}

/* ═══════════════════════════════════════════════════════
 *  Module definition
 * ═══════════════════════════════════════════════════════ */

static PyMethodDef TestModMethods[] = {
    {"add",          testmod_add,          METH_VARARGS, "Add two integers"},
    {"multiply",     testmod_multiply,     METH_VARARGS, "Multiply two doubles"},
    {"greet",        testmod_greet,        METH_VARARGS, "Greet by name"},
    {"strlen",       (PyCFunction)testmod_strlen,  METH_O,       "Get string length"},
    {"noop",         testmod_noop,         METH_NOARGS,  "Do nothing"},
    {"make_list",    testmod_make_list,    METH_VARARGS, "Make a list [0..n)"},
    {"sum_list",     (PyCFunction)testmod_sum_list, METH_O,       "Sum a list of ints"},
    {"mixed_return", testmod_mixed_return, METH_VARARGS, "Return mixed tuple"},
    {"pass_through", testmod_pass_through, METH_VARARGS, "Pass object through"},
    {NULL, NULL, 0, NULL}  /* sentinel */
};

static PyModuleDef testmod_module = {
    PyModuleDef_HEAD_INIT,
    "_testmod",                      /* m_name */
    "Test extension module",         /* m_doc */
    -1,                              /* m_size */
    TestModMethods,                  /* m_methods */
    NULL,                            /* m_slots */
    NULL,                            /* m_traverse */
    NULL,                            /* m_clear */
    NULL,                            /* m_free */
};

/* The entry point — called by dlopen/dlsym */
PyObject *PyInit__testmod(void) {
    return PyModule_Create2(&testmod_module, 1013);
}
