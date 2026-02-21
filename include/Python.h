/*
 * Rustthon Python.h — CPython 3.11 compatible header
 *
 * Provides type definitions, macros, and extern declarations that allow
 * C extensions to compile against librustthon.dylib.
 *
 * All struct layouts match CPython 3.11 byte-for-byte, verified by 196 C tests.
 *
 * This header works for BOTH:
 *   1. Extensions compiled against Rustthon (uses _Rustthon_Exc_* functions via macros)
 *   2. Prebuilt extensions compiled against CPython 3.11 (uses DATA symbols directly)
 */

#ifndef Py_PYTHON_H
#define Py_PYTHON_H

#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <assert.h>
#include <math.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ═══════════════════════════════════════════════════════
 *  Basic types
 * ═══════════════════════════════════════════════════════ */

typedef intptr_t        Py_ssize_t;
typedef intptr_t        Py_hash_t;
typedef uint8_t         Py_UCS1;
typedef uint16_t        Py_UCS2;
typedef uint32_t        Py_UCS4;

#define PY_SSIZE_T_MAX  INTPTR_MAX
#define PY_SSIZE_T_MIN  INTPTR_MIN

/* ═══════════════════════════════════════════════════════
 *  Object header (PyObject / PyVarObject)
 * ═══════════════════════════════════════════════════════ */

typedef struct _typeobject PyTypeObject;

typedef struct _object {
    Py_ssize_t ob_refcnt;
    PyTypeObject *ob_type;
} PyObject;

typedef struct {
    PyObject ob_base;
    Py_ssize_t ob_size;
} PyVarObject;

#define PyObject_HEAD       PyObject ob_base;
#define PyObject_VAR_HEAD   PyVarObject ob_base;

#define Py_TYPE(ob)         (((PyObject*)(ob))->ob_type)
#define Py_SIZE(ob)         (((PyVarObject*)(ob))->ob_size)
#define Py_REFCNT(ob)       (((PyObject*)(ob))->ob_refcnt)

#define PyVarObject_HEAD_INIT(type, size) \
    { { 1, (PyTypeObject*)(type) }, (size) }

/* ═══════════════════════════════════════════════════════
 *  Function pointer typedefs (for PyTypeObject)
 * ═══════════════════════════════════════════════════════ */

typedef void (*destructor)(PyObject *);
typedef PyObject *(*getattrfunc)(PyObject *, char *);
typedef int (*setattrfunc)(PyObject *, char *, PyObject *);
typedef PyObject *(*reprfunc)(PyObject *);
typedef Py_hash_t (*hashfunc)(PyObject *);
typedef PyObject *(*ternaryfunc)(PyObject *, PyObject *, PyObject *);
typedef PyObject *(*binaryfunc)(PyObject *, PyObject *);
typedef PyObject *(*unaryfunc)(PyObject *);
typedef int (*inquiry)(PyObject *);
typedef Py_ssize_t (*lenfunc)(PyObject *);
typedef PyObject *(*ssizeargfunc)(PyObject *, Py_ssize_t);
typedef int (*ssizeobjargproc)(PyObject *, Py_ssize_t, PyObject *);
typedef int (*objobjargproc)(PyObject *, PyObject *, PyObject *);
typedef int (*objobjproc)(PyObject *, PyObject *);
typedef PyObject *(*getiterfunc)(PyObject *);
typedef PyObject *(*iternextfunc)(PyObject *);
typedef int (*visitproc)(PyObject *, void *);
typedef int (*traverseproc)(PyObject *, visitproc, void *);
typedef int (*initproc)(PyObject *, PyObject *, PyObject *);
typedef PyObject *(*allocfunc)(PyTypeObject *, Py_ssize_t);
typedef PyObject *(*newfunc)(PyTypeObject *, PyObject *, PyObject *);
typedef void (*freefunc)(void *);
typedef PyObject *(*richcmpfunc)(PyObject *, PyObject *, int);
typedef int (*getbufferproc)(PyObject *, void *, int);
typedef void (*releasebufferproc)(PyObject *, void *);

/* Method suite structs (forward declarations) */
typedef struct PyNumberMethods PyNumberMethods;
typedef struct PySequenceMethods PySequenceMethods;
typedef struct PyMappingMethods PyMappingMethods;
typedef struct PyBufferProcs PyBufferProcs;

/* ═══════════════════════════════════════════════════════
 *  Full PyTypeObject (struct _typeobject) — matches CPython 3.11
 *  This is the actual struct, not a forward declaration.
 * ═══════════════════════════════════════════════════════ */

typedef struct PyMethodDef PyMethodDef;
typedef struct PyMemberDef PyMemberDef;
typedef struct PyGetSetDef PyGetSetDef;

struct _typeobject {
    /* PyVarObject header */
    PyObject ob_base;
    Py_ssize_t ob_size;

    /* Type info */
    const char *tp_name;
    Py_ssize_t tp_basicsize;
    Py_ssize_t tp_itemsize;

    /* Standard methods */
    destructor tp_dealloc;
    Py_ssize_t tp_vectorcall_offset;
    getattrfunc tp_getattr;
    setattrfunc tp_setattr;
    void *tp_as_async;              /* PyAsyncMethods* */
    reprfunc tp_repr;

    /* Method suites */
    PyNumberMethods *tp_as_number;
    PySequenceMethods *tp_as_sequence;
    PyMappingMethods *tp_as_mapping;

    /* More standard ops */
    hashfunc tp_hash;
    ternaryfunc tp_call;
    reprfunc tp_str;
    binaryfunc tp_getattro;
    objobjargproc tp_setattro;

    /* Buffer protocol */
    PyBufferProcs *tp_as_buffer;

    /* Flags */
    unsigned long tp_flags;

    /* Documentation */
    const char *tp_doc;

    /* GC traversal */
    traverseproc tp_traverse;
    inquiry tp_clear;

    /* Rich comparison */
    richcmpfunc tp_richcompare;

    /* Weak reference support */
    Py_ssize_t tp_weaklistoffset;

    /* Iterators */
    getiterfunc tp_iter;
    iternextfunc tp_iternext;

    /* Attribute descriptor / subclassing */
    PyMethodDef *tp_methods;
    PyMemberDef *tp_members;
    PyGetSetDef *tp_getset;
    PyTypeObject *tp_base;
    PyObject *tp_dict;
    ternaryfunc tp_descr_get;
    objobjargproc tp_descr_set;
    Py_ssize_t tp_dictoffset;
    initproc tp_init;
    allocfunc tp_alloc;
    newfunc tp_new;
    freefunc tp_free;
    inquiry tp_is_gc;
    PyObject *tp_bases;
    PyObject *tp_mro;
    PyObject *tp_cache;
    PyObject *tp_subclasses;
    PyObject *tp_weaklist;
    destructor tp_del;
    unsigned int tp_version_tag;
    destructor tp_finalize;
    void *tp_vectorcall;
};

/* ═══════════════════════════════════════════════════════
 *  Reference counting
 * ═══════════════════════════════════════════════════════ */

extern void _Py_Dealloc(PyObject *op);
extern void Py_IncRef(PyObject *op);
extern void Py_DecRef(PyObject *op);

#define Py_INCREF(op)  (++(((PyObject*)(op))->ob_refcnt))

#define Py_DECREF(op) do { \
    PyObject *_py_decref_tmp = (PyObject *)(op); \
    if (--_py_decref_tmp->ob_refcnt == 0) { \
        _Py_Dealloc(_py_decref_tmp); \
    } \
} while (0)

#define Py_XINCREF(op) do { if ((op) != NULL) Py_INCREF(op); } while (0)
#define Py_XDECREF(op) do { if ((op) != NULL) Py_DECREF(op); } while (0)
#define Py_CLEAR(op) do { \
    PyObject *_py_tmp = (PyObject *)(op); \
    if (_py_tmp != NULL) { (op) = NULL; Py_DECREF(_py_tmp); } \
} while (0)

/* ═══════════════════════════════════════════════════════
 *  None singleton
 * ═══════════════════════════════════════════════════════ */

extern PyObject _Py_NoneStruct;
#define Py_None         (&_Py_NoneStruct)
#define Py_RETURN_NONE  do { Py_INCREF(Py_None); return Py_None; } while (0)

/* Function accessor (backward compat for Rustthon-compiled extensions) */
extern PyObject *_Py_None(void);

/* ═══════════════════════════════════════════════════════
 *  Bool singletons — actual static structs
 * ═══════════════════════════════════════════════════════ */

struct _longobject {
    PyVarObject ob_base;
    uint32_t ob_digit[1];
};

extern struct _longobject _Py_TrueStruct;
extern struct _longobject _Py_FalseStruct;
#define Py_True         ((PyObject *)&_Py_TrueStruct)
#define Py_False        ((PyObject *)&_Py_FalseStruct)
#define Py_RETURN_TRUE  do { Py_INCREF(Py_True); return Py_True; } while (0)
#define Py_RETURN_FALSE do { Py_INCREF(Py_False); return Py_False; } while (0)

/* Function accessors (backward compat) */
extern PyObject *_Py_True(void);
extern PyObject *_Py_False(void);

extern PyObject *PyBool_FromLong(long v);
extern int PyBool_Check(PyObject *obj);
extern PyTypeObject PyBool_Type;

/* ═══════════════════════════════════════════════════════
 *  Long (int)
 * ═══════════════════════════════════════════════════════ */

typedef struct _longobject PyLongObject;

extern PyObject *PyLong_FromLong(long v);
extern PyObject *PyLong_FromLongLong(long long v);
extern PyObject *PyLong_FromSsize_t(Py_ssize_t v);
extern PyObject *PyLong_FromUnsignedLong(unsigned long v);
extern PyObject *PyLong_FromUnsignedLongLong(unsigned long long v);
extern PyObject *PyLong_FromDouble(double v);
extern PyObject *PyLong_FromSize_t(size_t v);
extern PyObject *PyLong_FromString(const char *str, char **pend, int base);
extern long PyLong_AsLong(PyObject *obj);
extern long long PyLong_AsLongLong(PyObject *obj);
extern unsigned long long PyLong_AsUnsignedLongLong(PyObject *obj);
extern Py_ssize_t PyLong_AsSsize_t(PyObject *obj);
extern double PyLong_AsDouble(PyObject *obj);
extern int PyLong_Check(PyObject *obj);
extern PyTypeObject PyLong_Type;

/* ═══════════════════════════════════════════════════════
 *  Float
 * ═══════════════════════════════════════════════════════ */

typedef struct {
    PyObject ob_base;
    double ob_fval;
} PyFloatObject;

extern PyObject *PyFloat_FromDouble(double v);
extern double PyFloat_AsDouble(PyObject *obj);
extern int PyFloat_Check(PyObject *obj);
#define PyFloat_AS_DOUBLE(op) (((PyFloatObject*)(op))->ob_fval)
extern PyTypeObject PyFloat_Type;

/* ═══════════════════════════════════════════════════════
 *  Unicode (PEP 393 — Flexible String Representation)
 * ═══════════════════════════════════════════════════════ */

typedef struct {
    PyObject ob_base;           /* 16 */
    Py_ssize_t length;          /* 8  */
    Py_hash_t hash;             /* 8  */
    uint32_t state;             /* 4  (bitfield: interned:2, kind:3, compact:1, ascii:1, ready:1) */
    uint32_t _padding;          /* 4  */
    void *wstr;                 /* 8  */
} PyASCIIObject;               /* 48 bytes total */

typedef struct {
    PyASCIIObject _base;        /* 48 */
    Py_ssize_t utf8_length;     /* 8  */
    char *utf8;                 /* 8  */
    Py_ssize_t wstr_length;     /* 8  */
} PyCompactUnicodeObject;      /* 72 bytes total */

/* PyUnicodeObject is the same as PyCompactUnicodeObject for compact strings */
typedef PyCompactUnicodeObject PyUnicodeObject;

/* String kind constants */
#define PyUnicode_1BYTE_KIND    1
#define PyUnicode_2BYTE_KIND    2
#define PyUnicode_4BYTE_KIND    4

/* State bitfield access macros.
 * Layout: interned(bits 0-1), kind(bits 2-4), compact(bit 5), ascii(bit 6), ready(bit 7) */
#define PyUnicode_KIND(op) \
    ((unsigned int)(((PyASCIIObject*)(op))->state >> 2) & 7)

#define PyUnicode_IS_ASCII(op) \
    ((((PyASCIIObject*)(op))->state >> 6) & 1)

#define PyUnicode_IS_COMPACT(op) \
    ((((PyASCIIObject*)(op))->state >> 5) & 1)

#define PyUnicode_IS_COMPACT_ASCII(op) \
    (PyUnicode_IS_ASCII(op) && PyUnicode_IS_COMPACT(op))

#define PyUnicode_IS_READY(op) \
    ((((PyASCIIObject*)(op))->state >> 7) & 1)

/* PyUnicode_READY is a no-op for compact strings (always ready). Returns 0 on success. */
#define PyUnicode_READY(op)     0

/* Data access macros — the core of PEP 393. */
#define _PyUnicode_COMPACT_DATA(op) \
    (PyUnicode_IS_ASCII(op) \
     ? (void*)((PyASCIIObject*)(op) + 1) \
     : (void*)((PyCompactUnicodeObject*)(op) + 1))

#define PyUnicode_DATA(op)          _PyUnicode_COMPACT_DATA(op)
#define PyUnicode_1BYTE_DATA(op)    ((Py_UCS1*)_PyUnicode_COMPACT_DATA(op))
#define PyUnicode_2BYTE_DATA(op)    ((Py_UCS2*)_PyUnicode_COMPACT_DATA(op))
#define PyUnicode_4BYTE_DATA(op)    ((Py_UCS4*)_PyUnicode_COMPACT_DATA(op))

#define PyUnicode_GET_LENGTH(op) \
    (((PyASCIIObject*)(op))->length)

/* Read a single character from the string at index `i` */
#define PyUnicode_READ(kind, data, index) \
    ((Py_UCS4)( \
        (kind) == PyUnicode_1BYTE_KIND ? ((Py_UCS1*)(data))[(index)] : \
        (kind) == PyUnicode_2BYTE_KIND ? ((Py_UCS2*)(data))[(index)] : \
                                         ((Py_UCS4*)(data))[(index)] \
    ))

/* Write a single character */
#define PyUnicode_WRITE(kind, data, index, value) do { \
    if ((kind) == PyUnicode_1BYTE_KIND) ((Py_UCS1*)(data))[(index)] = (Py_UCS1)(value); \
    else if ((kind) == PyUnicode_2BYTE_KIND) ((Py_UCS2*)(data))[(index)] = (Py_UCS2)(value); \
    else ((Py_UCS4*)(data))[(index)] = (Py_UCS4)(value); \
} while (0)

extern PyObject *PyUnicode_New(Py_ssize_t size, Py_UCS4 maxchar);
extern PyObject *PyUnicode_FromString(const char *s);
extern PyObject *PyUnicode_FromStringAndSize(const char *s, Py_ssize_t size);
extern PyObject *PyUnicode_FromKindAndData(int kind, const void *buffer, Py_ssize_t size);
extern PyObject *PyUnicode_FromFormat(const char *format, ...);
extern const char *PyUnicode_AsUTF8(PyObject *obj);
extern const char *PyUnicode_AsUTF8AndSize(PyObject *obj, Py_ssize_t *size);
extern PyObject *PyUnicode_AsEncodedString(PyObject *obj, const char *encoding, const char *errors);
extern PyObject *PyUnicode_DecodeUTF8(const char *s, Py_ssize_t size, const char *errors);
extern Py_ssize_t PyUnicode_GetLength(PyObject *obj);
extern int PyUnicode_Check(PyObject *obj);
extern PyObject *PyUnicode_Concat(PyObject *left, PyObject *right);
extern PyObject *PyUnicode_Join(PyObject *sep, PyObject *seq);
extern int PyUnicode_CompareWithASCIIString(PyObject *obj, const char *string);
extern void PyUnicode_InternInPlace(PyObject **p);
extern PyObject *PyUnicode_InternFromString(const char *s);
extern int _PyUnicode_Ready(PyObject *op);
extern PyTypeObject PyUnicode_Type;

/* ═══════════════════════════════════════════════════════
 *  Bytes
 * ═══════════════════════════════════════════════════════ */

typedef struct {
    PyVarObject ob_base;        /* 24 */
    Py_hash_t ob_shash;         /* 8  */
    char ob_sval[1];            /* flexible array */
} PyBytesObject;               /* Header: 32 bytes */

extern PyObject *PyBytes_FromString(const char *s);
extern PyObject *PyBytes_FromStringAndSize(const char *s, Py_ssize_t size);
extern char *PyBytes_AsString(PyObject *obj);
extern int PyBytes_AsStringAndSize(PyObject *obj, char **s, Py_ssize_t *len);
extern Py_ssize_t PyBytes_Size(PyObject *obj);
extern int PyBytes_Check(PyObject *obj);
#define PyBytes_GET_SIZE(op)    (Py_SIZE(op))
#define PyBytes_AS_STRING(op)   (((PyBytesObject*)(op))->ob_sval)
extern PyTypeObject PyBytes_Type;

/* ═══════════════════════════════════════════════════════
 *  ByteArray (stub — just enough for type checking)
 * ═══════════════════════════════════════════════════════ */

extern int PyByteArray_Check(PyObject *obj);

/* ═══════════════════════════════════════════════════════
 *  List
 * ═══════════════════════════════════════════════════════ */

typedef struct {
    PyVarObject ob_base;            /* 24 */
    PyObject **ob_item;             /* 8  */
    Py_ssize_t allocated;           /* 8  */
} PyListObject;                    /* 40 bytes */

extern PyObject *PyList_New(Py_ssize_t size);
extern PyObject *PyList_GetItem(PyObject *list, Py_ssize_t i);
extern int PyList_SetItem(PyObject *list, Py_ssize_t i, PyObject *item);
extern int PyList_Append(PyObject *list, PyObject *item);
extern int PyList_Insert(PyObject *list, Py_ssize_t i, PyObject *item);
extern Py_ssize_t PyList_Size(PyObject *list);
extern PyObject *PyList_GetSlice(PyObject *list, Py_ssize_t low, Py_ssize_t high);
extern PyObject *PyList_AsTuple(PyObject *list);
extern int PyList_Sort(PyObject *list);
extern int PyList_Reverse(PyObject *list);
extern int PyList_Check(PyObject *obj);

#define PyList_GET_ITEM(op, i)  (((PyListObject*)(op))->ob_item[(i)])
#define PyList_SET_ITEM(op, i, v) (((PyListObject*)(op))->ob_item[(i)] = (v))
#define PyList_GET_SIZE(op)     (Py_SIZE(op))

extern PyTypeObject PyList_Type;

/* ═══════════════════════════════════════════════════════
 *  Tuple
 * ═══════════════════════════════════════════════════════ */

typedef struct {
    PyVarObject ob_base;            /* 24 */
    PyObject *ob_item[1];           /* flexible inline array */
} PyTupleObject;                   /* 24 + 8*N */

extern PyObject *PyTuple_New(Py_ssize_t size);
extern PyObject *PyTuple_GetItem(PyObject *tuple, Py_ssize_t i);
extern int PyTuple_SetItem(PyObject *tuple, Py_ssize_t i, PyObject *item);
extern Py_ssize_t PyTuple_Size(PyObject *tuple);
extern PyObject *PyTuple_GetSlice(PyObject *tuple, Py_ssize_t low, Py_ssize_t high);
extern PyObject *PyTuple_Pack(Py_ssize_t n, ...);
extern int PyTuple_Check(PyObject *obj);

#define PyTuple_GET_ITEM(op, i) (((PyTupleObject*)(op))->ob_item[(i)])
#define PyTuple_SET_ITEM(op, i, v) (((PyTupleObject*)(op))->ob_item[(i)] = (v))
#define PyTuple_GET_SIZE(op)    (Py_SIZE(op))

extern PyTypeObject PyTuple_Type;

/* ═══════════════════════════════════════════════════════
 *  Dict
 * ═══════════════════════════════════════════════════════ */

extern PyObject *PyDict_New(void);
extern int PyDict_SetItem(PyObject *p, PyObject *key, PyObject *val);
extern int PyDict_SetItemString(PyObject *p, const char *key, PyObject *val);
extern PyObject *PyDict_GetItem(PyObject *p, PyObject *key);
extern PyObject *PyDict_GetItemString(PyObject *p, const char *key);
extern PyObject *PyDict_GetItemWithError(PyObject *p, PyObject *key);
extern int PyDict_DelItem(PyObject *p, PyObject *key);
extern int PyDict_DelItemString(PyObject *p, const char *key);
extern int PyDict_Contains(PyObject *p, PyObject *key);
extern PyObject *PyDict_Keys(PyObject *p);
extern PyObject *PyDict_Values(PyObject *p);
extern PyObject *PyDict_Items(PyObject *p);
extern int PyDict_Next(PyObject *p, Py_ssize_t *ppos, PyObject **pkey, PyObject **pvalue);
extern void PyDict_Clear(PyObject *p);
extern PyObject *PyDict_Copy(PyObject *p);
extern int PyDict_Update(PyObject *a, PyObject *b);
extern int PyDict_Merge(PyObject *a, PyObject *b, int override);
extern Py_ssize_t PyDict_Size(PyObject *p);
extern int PyDict_Check(PyObject *obj);
extern PyTypeObject PyDict_Type;

/* ═══════════════════════════════════════════════════════
 *  Set
 * ═══════════════════════════════════════════════════════ */

extern PyObject *PySet_New(PyObject *iterable);
extern PyObject *PyFrozenSet_New(PyObject *iterable);
extern int PySet_Add(PyObject *set, PyObject *key);
extern int PySet_Discard(PyObject *set, PyObject *key);
extern int PySet_Contains(PyObject *set, PyObject *key);
extern Py_ssize_t PySet_Size(PyObject *set);
extern int PySet_Clear(PyObject *set);
extern int PySet_Check(PyObject *obj);
extern PyTypeObject PySet_Type;

/* ═══════════════════════════════════════════════════════
 *  Number protocol
 * ═══════════════════════════════════════════════════════ */

extern PyObject *PyNumber_ToBase(PyObject *n, int base);

/* ═══════════════════════════════════════════════════════
 *  Object protocol
 * ═══════════════════════════════════════════════════════ */

extern PyObject *PyObject_Repr(PyObject *obj);
extern PyObject *PyObject_Str(PyObject *obj);
extern Py_hash_t PyObject_Hash(PyObject *obj);
extern PyObject *PyObject_RichCompare(PyObject *v, PyObject *w, int op);
extern int PyObject_RichCompareBool(PyObject *v, PyObject *w, int op);
extern int PyObject_IsTrue(PyObject *obj);
extern int PyObject_Not(PyObject *obj);
extern PyObject *PyObject_Type(PyObject *obj);
extern int PyObject_TypeCheck(PyObject *obj, PyTypeObject *tp);
extern int PyObject_HasAttrString(PyObject *obj, const char *name);
extern PyObject *PyObject_GetAttrString(PyObject *obj, const char *name);
extern int PyObject_SetAttrString(PyObject *obj, const char *name, PyObject *value);
extern PyObject *PyObject_GetAttr(PyObject *obj, PyObject *name);
extern int PyObject_SetAttr(PyObject *obj, PyObject *name, PyObject *value);
extern PyObject *PyObject_GetItem(PyObject *obj, PyObject *key);
extern int PyObject_SetItem(PyObject *obj, PyObject *key, PyObject *value);
extern Py_ssize_t PyObject_Length(PyObject *obj);
extern Py_ssize_t PyObject_Size(PyObject *obj);
extern PyObject *PyObject_GetIter(PyObject *obj);
extern PyObject *PyIter_Next(PyObject *iter);
extern int PyCallable_Check(PyObject *obj);
extern PyObject *PyObject_Call(PyObject *callable, PyObject *args, PyObject *kwargs);
extern PyObject *PyObject_CallObject(PyObject *callable, PyObject *args);
extern PyObject *PyObject_CallNoArgs(PyObject *callable);
extern PyObject *PyObject_CallOneArg(PyObject *callable, PyObject *arg);
extern PyObject *PyObject_CallMethod(PyObject *obj, const char *name, const char *format, ...);
extern PyObject *PyObject_CallFunctionObjArgs(PyObject *callable, ...);
extern int PyObject_IsInstance(PyObject *inst, PyObject *cls);
extern int PyIter_Check(PyObject *obj);
extern PyObject *PyObject_GenericGetAttr(PyObject *obj, PyObject *name);
extern int PyObject_GenericSetAttr(PyObject *obj, PyObject *name, PyObject *value);

/* Rich comparison constants */
#define Py_LT   0
#define Py_LE   1
#define Py_EQ   2
#define Py_NE   3
#define Py_GT   4
#define Py_GE   5

/* ═══════════════════════════════════════════════════════
 *  Type object
 * ═══════════════════════════════════════════════════════ */

typedef PyObject *(*PyCFunction)(PyObject *, PyObject *);
typedef PyObject *(*PyCFunctionWithKeywords)(PyObject *, PyObject *, PyObject *);

struct PyMethodDef {
    const char *ml_name;
    PyCFunction ml_meth;
    int ml_flags;
    const char *ml_doc;
};

/* Method flags */
#define METH_VARARGS    0x0001
#define METH_KEYWORDS   0x0002
#define METH_NOARGS     0x0004
#define METH_O          0x0008

/* Type flags */
#define Py_TPFLAGS_DEFAULT              (1UL << 0)
#define Py_TPFLAGS_READY                (1UL << 12)
#define Py_TPFLAGS_HAVE_GC             (1UL << 14)
#define Py_TPFLAGS_LONG_SUBCLASS       (1UL << 24)
#define Py_TPFLAGS_LIST_SUBCLASS       (1UL << 25)
#define Py_TPFLAGS_TUPLE_SUBCLASS      (1UL << 26)
#define Py_TPFLAGS_BYTES_SUBCLASS      (1UL << 27)
#define Py_TPFLAGS_UNICODE_SUBCLASS    (1UL << 28)
#define Py_TPFLAGS_DICT_SUBCLASS       (1UL << 29)
#define Py_TPFLAGS_BASETYPE            (1UL << 10)
#define Py_TPFLAGS_HAVE_VECTORCALL     (1UL << 11)

extern int PyType_IsSubtype(PyTypeObject *a, PyTypeObject *b);
extern int PyType_Ready(PyTypeObject *tp);
extern PyObject *PyType_GenericNew(PyTypeObject *tp, PyObject *args, PyObject *kwargs);
extern PyObject *PyType_GenericAlloc(PyTypeObject *tp, Py_ssize_t nitems);

/* Metaclass and base type */
extern PyTypeObject PyType_Type;
extern PyTypeObject PyBaseObject_Type;

/* ═══════════════════════════════════════════════════════
 *  Module definition
 * ═══════════════════════════════════════════════════════ */

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

typedef struct PyModuleDef {
    PyModuleDef_Base m_base;
    const char *m_name;
    const char *m_doc;
    Py_ssize_t m_size;
    PyMethodDef *m_methods;
    PyModuleDef_Slot *m_slots;
    int (*m_traverse)(PyObject *, visitproc, void *);
    int (*m_clear)(PyObject *);
    void (*m_free)(void *);
} PyModuleDef;

#define PyModuleDef_HEAD_INIT { { 1, NULL }, NULL, 0, NULL }

/* Py_VISIT — used in tp_traverse / m_traverse callbacks */
#define Py_VISIT(op) do { \
    if (op) { \
        int vret = visit((PyObject *)(op), arg); \
        if (vret) return vret; \
    } \
} while (0)

extern PyObject *PyModule_Create2(PyModuleDef *def, int api_version);
#define PyModule_Create(def) PyModule_Create2((def), 1013)
extern PyObject *PyModuleDef_Init(PyModuleDef *def);
extern PyObject *PyModule_GetDict(PyObject *module);
extern const char *PyModule_GetName(PyObject *module);
extern PyObject *PyModule_GetNameObject(PyObject *module);
extern int PyModule_AddObject(PyObject *module, const char *name, PyObject *value);
extern int PyModule_AddIntConstant(PyObject *module, const char *name, long value);
extern int PyModule_AddStringConstant(PyObject *module, const char *name, const char *value);
extern int PyModule_Check(PyObject *obj);
extern void *PyModule_GetState(PyObject *module);
extern PyObject *PyState_FindModule(PyModuleDef *def);

/* PyMODINIT_FUNC — module initialization function return type */
#define PyMODINIT_FUNC  __attribute__((visibility("default"))) PyObject*

/* ═══════════════════════════════════════════════════════
 *  Error handling
 * ═══════════════════════════════════════════════════════ */

extern void PyErr_SetString(PyObject *type, const char *message);
extern void PyErr_SetObject(PyObject *type, PyObject *value);
extern PyObject *PyErr_Occurred(void);
extern void PyErr_Clear(void);
extern void PyErr_Fetch(PyObject **type, PyObject **value, PyObject **traceback);
extern void PyErr_Restore(PyObject *type, PyObject *value, PyObject *traceback);
extern void PyErr_NormalizeException(PyObject **type, PyObject **value, PyObject **traceback);
extern void PyErr_SetNone(PyObject *type);
extern int PyErr_ExceptionMatches(PyObject *exc);
extern int PyErr_GivenExceptionMatches(PyObject *err, PyObject *exc);
extern PyObject *PyErr_Format(PyObject *type, const char *format, ...);
extern int PyErr_BadArgument(void);
extern PyObject *PyErr_NoMemory(void);
extern PyObject *PyErr_NewException(const char *name, PyObject *base, PyObject *dict);

/* Exception type singletons — DATA symbols (PyObject* pointers).
 * Prebuilt extensions reference these as: extern PyObject *PyExc_TypeError;
 * Rustthon-compiled extensions can also use the _Rustthon_Exc_* functions via macros. */

extern PyObject *PyExc_BaseException;
extern PyObject *PyExc_Exception;
extern PyObject *PyExc_TypeError;
extern PyObject *PyExc_ValueError;
extern PyObject *PyExc_OverflowError;
extern PyObject *PyExc_RuntimeError;
extern PyObject *PyExc_KeyError;
extern PyObject *PyExc_IndexError;
extern PyObject *PyExc_AttributeError;
extern PyObject *PyExc_StopIteration;
extern PyObject *PyExc_MemoryError;
extern PyObject *PyExc_SystemError;
extern PyObject *PyExc_OSError;
extern PyObject *PyExc_IOError;
extern PyObject *PyExc_NotImplementedError;
extern PyObject *PyExc_UnicodeDecodeError;
extern PyObject *PyExc_UnicodeEncodeError;
extern PyObject *PyExc_UnicodeError;
extern PyObject *PyExc_LookupError;
extern PyObject *PyExc_ArithmeticError;

/* Backward-compat function accessors for Rustthon-compiled extensions */
extern PyObject *_Rustthon_Exc_TypeError(void);
extern PyObject *_Rustthon_Exc_ValueError(void);
extern PyObject *_Rustthon_Exc_OverflowError(void);
extern PyObject *_Rustthon_Exc_RuntimeError(void);
extern PyObject *_Rustthon_Exc_KeyError(void);
extern PyObject *_Rustthon_Exc_IndexError(void);
extern PyObject *_Rustthon_Exc_AttributeError(void);
extern PyObject *_Rustthon_Exc_StopIteration(void);
extern PyObject *_Rustthon_Exc_MemoryError(void);

/* ═══════════════════════════════════════════════════════
 *  Recursion guard (stubs — prevent extensions from crashing)
 * ═══════════════════════════════════════════════════════ */

#define Py_EnterRecursiveCall(where) (0)
#define Py_LeaveRecursiveCall()      do {} while (0)

/* ═══════════════════════════════════════════════════════
 *  Memory management
 * ═══════════════════════════════════════════════════════ */

extern void *PyMem_Malloc(size_t size);
extern void *PyMem_Calloc(size_t nelem, size_t elsize);
extern void *PyMem_Realloc(void *ptr, size_t new_size);
extern void PyMem_Free(void *ptr);

extern void *PyObject_Malloc(size_t size);
extern void *PyObject_Calloc(size_t nelem, size_t elsize);
extern void *PyObject_Realloc(void *ptr, size_t new_size);
extern void PyObject_Free(void *ptr);

extern PyObject *PyObject_Init(PyObject *op, PyTypeObject *tp);

/* ═══════════════════════════════════════════════════════
 *  GC
 * ═══════════════════════════════════════════════════════ */

typedef struct {
    uintptr_t gc_next;
    uintptr_t gc_prev;
} PyGC_Head;

extern void PyObject_GC_Track(void *op);
extern void PyObject_GC_UnTrack(void *op);
extern void PyObject_GC_Del(void *op);

/* ═══════════════════════════════════════════════════════
 *  Argument parsing (implemented in C via csrc/varargs.c)
 * ═══════════════════════════════════════════════════════ */

extern int PyArg_ParseTuple(PyObject *args, const char *format, ...);
extern int PyArg_ParseTupleAndKeywords(PyObject *args, PyObject *kwargs,
                                       const char *format, char **kwlist, ...);
extern int PyArg_UnpackTuple(PyObject *args, const char *funcname,
                             Py_ssize_t min, Py_ssize_t max, ...);
extern PyObject *Py_BuildValue(const char *format, ...);

/* ═══════════════════════════════════════════════════════
 *  Initialization
 * ═══════════════════════════════════════════════════════ */

extern void Py_Initialize(void);
extern void Py_InitializeEx(int initsigs);
extern void Py_Finalize(void);
extern int Py_IsInitialized(void);
extern const char *Py_GetVersion(void);

/* ═══════════════════════════════════════════════════════
 *  Import
 * ═══════════════════════════════════════════════════════ */

extern PyObject *PyImport_ImportModule(const char *name);
extern PyObject *PyImport_Import(PyObject *name);

/* ═══════════════════════════════════════════════════════
 *  CFunction
 * ═══════════════════════════════════════════════════════ */

extern PyObject *PyCFunction_NewEx(PyMethodDef *ml, PyObject *self, PyObject *module);
extern PyObject *PyCFunction_New(PyMethodDef *ml, PyObject *self);

/* ═══════════════════════════════════════════════════════
 *  Miscellaneous
 * ═══════════════════════════════════════════════════════ */

extern int Py_IsNone(PyObject *obj);

/* NaN and Infinity constants */
#define Py_NAN      ((double)NAN)
#define Py_HUGE_VAL ((double)HUGE_VAL)

/* GIL state */
typedef int PyGILState_STATE;
extern PyGILState_STATE PyGILState_Ensure(void);
extern void PyGILState_Release(PyGILState_STATE state);

/* Buffer protocol */
typedef struct {
    void *buf;
    PyObject *obj;
    Py_ssize_t len;
    Py_ssize_t itemsize;
    int readonly;
    int ndim;
    char *format;
    Py_ssize_t *shape;
    Py_ssize_t *strides;
    Py_ssize_t *suboffsets;
    void *internal;
} Py_buffer;

#define PyBUF_SIMPLE        0
#define PyBUF_WRITABLE      0x0001
#define PyBUF_FORMAT        0x0004
#define PyBUF_ND            0x0008
#define PyBUF_STRIDES       (0x0010 | PyBUF_ND)
#define PyBUF_C_CONTIGUOUS  (0x0020 | PyBUF_STRIDES)
#define PyBUF_F_CONTIGUOUS  (0x0040 | PyBUF_STRIDES)
#define PyBUF_ANY_CONTIGUOUS (0x0080 | PyBUF_STRIDES)

extern int PyObject_GetBuffer(PyObject *obj, Py_buffer *view, int flags);
extern void PyBuffer_Release(Py_buffer *view);

#ifdef __cplusplus
}
#endif

#endif /* Py_PYTHON_H */
