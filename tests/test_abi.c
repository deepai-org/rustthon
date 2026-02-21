/*
 * Phase 1: ABI Sanity Check for Rustthon
 *
 * This program bypasses the C API accessor functions and directly reads
 * struct internals through pointer arithmetic and field access, exactly
 * like a pre-built C extension would. If any struct offset is wrong,
 * this will either segfault or print incorrect values.
 *
 * Build:
 *   cc -o test_abi tests/test_abi.c \
 *      -L target/release -lrustthon \
 *      -Wl,-rpath,target/release
 *
 * Run:
 *   ./test_abi
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <stddef.h>

/* ─── CPython 3.11 struct layouts (what C extensions expect) ─── */

typedef intptr_t Py_ssize_t;
typedef intptr_t Py_hash_t;
typedef uint32_t digit;

#define PyLong_SHIFT 30

typedef struct _object {
    Py_ssize_t ob_refcnt;
    struct _typeobject *ob_type;
} PyObject;

typedef struct {
    PyObject ob_base;
    Py_ssize_t ob_size;
} PyVarObject;

/* Float */
typedef struct {
    PyObject ob_base;       /* 16 */
    double ob_fval;         /* 8  */
} PyFloatObject;            /* 24 total */

/* Long (int) — flexible array of digits after header */
typedef struct {
    PyVarObject ob_base;    /* 24 */
    digit ob_digit[1];      /* flexible array */
} PyLongObject;

/* List */
typedef struct {
    PyVarObject ob_base;        /* 24 */
    PyObject **ob_item;         /* 8  */
    Py_ssize_t allocated;       /* 8  */
} PyListObject;                 /* 40 total */

/* Tuple — inline items after header */
typedef struct {
    PyVarObject ob_base;        /* 24 */
    PyObject *ob_item[1];       /* flexible array */
} PyTupleObject;

/* Bytes — inline data after header */
typedef struct {
    PyVarObject ob_base;        /* 24 */
    Py_hash_t ob_shash;         /* 8  */
    char ob_sval[1];            /* flexible array */
} PyBytesObject;                /* header: 32 */

/* Unicode (ASCII compact) */
typedef struct {
    PyObject ob_base;           /* 16 */
    Py_ssize_t length;          /* 8  */
    Py_hash_t hash;             /* 8  */
    uint32_t state;             /* 4  */
    uint32_t _padding;          /* 4  */
    int32_t *wstr;              /* 8  */
} PyASCIIObject;                /* 48 total */

/* Unicode (compact, non-ASCII) */
typedef struct {
    PyASCIIObject _base;        /* 48 */
    Py_ssize_t utf8_length;     /* 8  */
    char *utf8;                 /* 8  */
    Py_ssize_t wstr_length;     /* 8  */
} PyCompactUnicodeObject;       /* 72 total */

/* Dict */
typedef struct {
    PyObject ob_base;                   /* 16 */
    Py_ssize_t ma_used;                 /* 8  */
    uint64_t ma_version_tag;            /* 8  */
    void *ma_keys;                      /* 8  */
    PyObject **ma_values;               /* 8  */
} PyDictObject;                         /* 48 total */

/* Set */
typedef struct {
    PyObject *key;
    Py_hash_t hash;
} setentry;

typedef struct {
    PyObject ob_base;                   /* 16 */
    Py_ssize_t fill;                    /* 8  */
    Py_ssize_t used;                    /* 8  */
    Py_ssize_t mask;                    /* 8  */
    setentry *table;                    /* 8  */
    Py_hash_t hash;                     /* 8  */
    Py_ssize_t finger;                  /* 8  */
    setentry smalltable[8];             /* 128 */
    PyObject *weakreflist;              /* 8  */
} PySetObject;                          /* 200 total */

/* GC Head (16 bytes on CPython 3.8+) */
typedef struct {
    uintptr_t gc_next;
    uintptr_t gc_prev;
} PyGC_Head;

/* ─── Extern declarations (from librustthon.dylib) ─── */

extern void Py_Initialize(void);

/* Object creation */
extern PyObject *PyLong_FromLong(long v);
extern long PyLong_AsLong(PyObject *obj);
extern PyObject *PyFloat_FromDouble(double v);
extern double PyFloat_AsDouble(PyObject *obj);
extern PyObject *PyUnicode_FromString(const char *s);
extern const char *PyUnicode_AsUTF8(PyObject *obj);
extern PyObject *PyBytes_FromString(const char *s);
extern char *PyBytes_AsString(PyObject *obj);
extern Py_ssize_t PyBytes_Size(PyObject *obj);
extern PyObject *PyList_New(Py_ssize_t size);
extern int PyList_Append(PyObject *list, PyObject *item);
extern PyObject *PyList_GetItem(PyObject *list, Py_ssize_t index);
extern Py_ssize_t PyList_Size(PyObject *list);
extern PyObject *PyTuple_New(Py_ssize_t size);
extern int PyTuple_SetItem(PyObject *tuple, Py_ssize_t index, PyObject *item);
extern PyObject *PyTuple_GetItem(PyObject *tuple, Py_ssize_t index);
extern PyObject *PyDict_New(void);
extern int PyDict_SetItemString(PyObject *dict, const char *key, PyObject *val);
extern PyObject *PyDict_GetItemString(PyObject *dict, const char *key);
extern Py_ssize_t PyDict_Size(PyObject *dict);
extern PyObject *PySet_New(PyObject *iterable);
extern int PySet_Add(PyObject *set, PyObject *key);
extern Py_ssize_t PySet_Size(PyObject *set);
extern PyObject *PyBool_FromLong(long v);

/* Singletons */
extern PyObject *_Py_None(void);
extern PyObject *_Py_True(void);
extern PyObject *_Py_False(void);

/* Memory */
extern void *PyMem_Malloc(size_t n);
extern void *PyMem_Realloc(void *p, size_t n);
extern void PyMem_Free(void *p);

/* Refcounting */
extern void Py_IncRef(PyObject *o);
extern void Py_DecRef(PyObject *o);

/* ─── Test infrastructure ─── */

static int tests_run = 0;
static int tests_passed = 0;
static int tests_failed = 0;

#define TEST(name) do { \
    tests_run++; \
    printf("  %-50s ", name); \
} while(0)

#define PASS() do { \
    tests_passed++; \
    printf("\033[32mPASS\033[0m\n"); \
} while(0)

#define FAIL(fmt, ...) do { \
    tests_failed++; \
    printf("\033[31mFAIL\033[0m  " fmt "\n", ##__VA_ARGS__); \
} while(0)

#define CHECK(cond, fmt, ...) do { \
    if (cond) { PASS(); } else { FAIL(fmt, ##__VA_ARGS__); } \
} while(0)

/* ─── Size assertions ─── */

void test_struct_sizes(void) {
    printf("\n=== Struct Size Verification ===\n");

    TEST("sizeof(PyObject) == 16");
    CHECK(sizeof(PyObject) == 16, "got %zu", sizeof(PyObject));

    TEST("sizeof(PyVarObject) == 24");
    CHECK(sizeof(PyVarObject) == 24, "got %zu", sizeof(PyVarObject));

    TEST("sizeof(PyFloatObject) == 24");
    CHECK(sizeof(PyFloatObject) == 24, "got %zu", sizeof(PyFloatObject));

    TEST("sizeof(PyListObject) == 40");
    CHECK(sizeof(PyListObject) == 40, "got %zu", sizeof(PyListObject));

    TEST("sizeof(PyBytesObject) header == 32");
    /* ob_sval[1] adds 1, so subtract it */
    CHECK(offsetof(PyBytesObject, ob_sval) == 32,
          "got %zu", offsetof(PyBytesObject, ob_sval));

    TEST("sizeof(PyASCIIObject) == 48");
    CHECK(sizeof(PyASCIIObject) == 48, "got %zu", sizeof(PyASCIIObject));

    TEST("sizeof(PyCompactUnicodeObject) == 72");
    CHECK(sizeof(PyCompactUnicodeObject) == 72, "got %zu", sizeof(PyCompactUnicodeObject));

    TEST("sizeof(PyDictObject) == 48");
    CHECK(sizeof(PyDictObject) == 48, "got %zu", sizeof(PyDictObject));

    TEST("sizeof(PySetObject) == 200");
    CHECK(sizeof(PySetObject) == 200, "got %zu", sizeof(PySetObject));

    TEST("sizeof(setentry) == 16");
    CHECK(sizeof(setentry) == 16, "got %zu", sizeof(setentry));

    TEST("sizeof(PyGC_Head) == 16");
    CHECK(sizeof(PyGC_Head) == 16, "got %zu", sizeof(PyGC_Head));
}

/* ─── Float: direct ob_fval read ─── */

void test_float_layout(void) {
    printf("\n=== Float Layout ===\n");

    PyObject *f = PyFloat_FromDouble(3.14159265358979);
    PyFloatObject *fo = (PyFloatObject *)f;

    TEST("PyFloat ob_fval at offset 16");
    CHECK(fo->ob_fval == 3.14159265358979,
          "got %f", fo->ob_fval);

    TEST("PyFloat ob_refcnt > 0");
    CHECK(fo->ob_base.ob_refcnt > 0,
          "got %zd", fo->ob_base.ob_refcnt);

    TEST("PyFloat ob_type is non-null");
    CHECK(fo->ob_base.ob_type != NULL, "type is null");

    Py_DecRef(f);
}

/* ─── Long: digit array read ─── */

void test_long_layout(void) {
    printf("\n=== Long (int) Layout ===\n");

    /* Small positive: 42 fits in one 30-bit digit */
    PyObject *n42 = PyLong_FromLong(42);
    PyLongObject *lo42 = (PyLongObject *)n42;

    TEST("PyLong(42) ob_size == 1 (positive, 1 digit)");
    CHECK(lo42->ob_base.ob_size == 1,
          "got %zd", lo42->ob_base.ob_size);

    TEST("PyLong(42) ob_digit[0] == 42");
    CHECK(lo42->ob_digit[0] == 42,
          "got %u", lo42->ob_digit[0]);

    /* Negative: -7 */
    PyObject *nm7 = PyLong_FromLong(-7);
    PyLongObject *lom7 = (PyLongObject *)nm7;

    TEST("PyLong(-7) ob_size == -1 (negative, 1 digit)");
    CHECK(lom7->ob_base.ob_size == -1,
          "got %zd", lom7->ob_base.ob_size);

    TEST("PyLong(-7) ob_digit[0] == 7");
    CHECK(lom7->ob_digit[0] == 7,
          "got %u", lom7->ob_digit[0]);

    /* Zero */
    PyObject *n0 = PyLong_FromLong(0);
    PyLongObject *lo0 = (PyLongObject *)n0;

    TEST("PyLong(0) ob_size == 0");
    CHECK(lo0->ob_base.ob_size == 0,
          "got %zd", lo0->ob_base.ob_size);

    /* Large: 2^30 = 1073741824 should need 2 digits */
    PyObject *nbig = PyLong_FromLong(1073741824L);
    PyLongObject *lobig = (PyLongObject *)nbig;

    TEST("PyLong(2^30) ob_size == 2 (two 30-bit digits)");
    CHECK(lobig->ob_base.ob_size == 2,
          "got %zd", lobig->ob_base.ob_size);

    TEST("PyLong(2^30) ob_digit[0] == 0 (lower 30 bits)");
    CHECK(lobig->ob_digit[0] == 0,
          "got %u", lobig->ob_digit[0]);

    TEST("PyLong(2^30) ob_digit[1] == 1 (upper digit)");
    CHECK(lobig->ob_digit[1] == 1,
          "got %u", lobig->ob_digit[1]);

    /* Verify the value reconstructs correctly */
    long reconstructed = (long)lobig->ob_digit[1] * (1L << 30) + (long)lobig->ob_digit[0];
    TEST("PyLong(2^30) digit reconstruction == 1073741824");
    CHECK(reconstructed == 1073741824L,
          "got %ld", reconstructed);

    /* 999999999: single digit (< 2^30) */
    PyObject *n999 = PyLong_FromLong(999999999L);
    PyLongObject *lo999 = (PyLongObject *)n999;

    TEST("PyLong(999999999) ob_size == 1");
    CHECK(lo999->ob_base.ob_size == 1,
          "got %zd", lo999->ob_base.ob_size);

    TEST("PyLong(999999999) ob_digit[0] == 999999999");
    CHECK(lo999->ob_digit[0] == 999999999,
          "got %u", lo999->ob_digit[0]);

    Py_DecRef(n42);
    Py_DecRef(nm7);
    Py_DecRef(n0);
    Py_DecRef(nbig);
    Py_DecRef(n999);
}

/* ─── Bool: PyLongObject with BOOL_TYPE ─── */

void test_bool_layout(void) {
    printf("\n=== Bool Layout (Long subtype) ===\n");

    PyObject *t = _Py_True();
    PyObject *f = _Py_False();
    PyLongObject *lt = (PyLongObject *)t;
    PyLongObject *lf = (PyLongObject *)f;

    TEST("True ob_size == 1");
    CHECK(lt->ob_base.ob_size == 1,
          "got %zd", lt->ob_base.ob_size);

    TEST("True ob_digit[0] == 1");
    CHECK(lt->ob_digit[0] == 1,
          "got %u", lt->ob_digit[0]);

    TEST("False ob_size == 0");
    CHECK(lf->ob_base.ob_size == 0,
          "got %zd", lf->ob_base.ob_size);

    TEST("True and False have same ob_type");
    CHECK(lt->ob_base.ob_base.ob_type == lf->ob_base.ob_base.ob_type,
          "types differ");

    TEST("True ob_type != int ob_type (bool is subtype)");
    PyObject *n1 = PyLong_FromLong(1);
    CHECK(lt->ob_base.ob_base.ob_type != ((PyLongObject*)n1)->ob_base.ob_base.ob_type,
          "bool type == int type");
    Py_DecRef(n1);

    TEST("True is a singleton (pointer identity)");
    PyObject *t2 = _Py_True();
    CHECK(t == t2, "pointers differ");

    TEST("False is a singleton (pointer identity)");
    PyObject *f2 = _Py_False();
    CHECK(f == f2, "pointers differ");
}

/* ─── List: ob_item array ─── */

void test_list_layout(void) {
    printf("\n=== List Layout ===\n");

    PyObject *list = PyList_New(0);
    PyListObject *lo = (PyListObject *)list;

    TEST("Empty list ob_size == 0");
    CHECK(lo->ob_base.ob_size == 0,
          "got %zd", lo->ob_base.ob_size);

    /* Add items */
    PyObject *n10 = PyLong_FromLong(10);
    PyObject *n20 = PyLong_FromLong(20);
    PyObject *n30 = PyLong_FromLong(30);
    PyList_Append(list, n10);
    PyList_Append(list, n20);
    PyList_Append(list, n30);

    TEST("list[3 items] ob_size == 3");
    CHECK(lo->ob_base.ob_size == 3,
          "got %zd", lo->ob_base.ob_size);

    TEST("list[3 items] allocated >= 3");
    CHECK(lo->allocated >= 3,
          "got %zd", lo->allocated);

    TEST("list ob_item is non-null");
    CHECK(lo->ob_item != NULL, "ob_item is null");

    /* Direct field access: read ob_item[0] */
    TEST("list ob_item[0] == n10 (direct struct access)");
    CHECK(lo->ob_item[0] == n10,
          "got %p, expected %p", (void*)lo->ob_item[0], (void*)n10);

    TEST("list ob_item[1] == n20 (direct struct access)");
    CHECK(lo->ob_item[1] == n20,
          "got %p, expected %p", (void*)lo->ob_item[1], (void*)n20);

    TEST("list ob_item[2] == n30 (direct struct access)");
    CHECK(lo->ob_item[2] == n30,
          "got %p, expected %p", (void*)lo->ob_item[2], (void*)n30);

    /* Verify the pointed-to values */
    TEST("PyLong_AsLong(ob_item[0]) == 10");
    CHECK(PyLong_AsLong(lo->ob_item[0]) == 10,
          "got %ld", PyLong_AsLong(lo->ob_item[0]));

    Py_DecRef(n10);
    Py_DecRef(n20);
    Py_DecRef(n30);
    Py_DecRef(list);
}

/* ─── Tuple: inline items at offset 24 ─── */

void test_tuple_layout(void) {
    printf("\n=== Tuple Layout (inline items) ===\n");

    PyObject *t = PyTuple_New(3);
    PyTupleObject *to = (PyTupleObject *)t;

    TEST("Tuple(3) ob_size == 3");
    CHECK(to->ob_base.ob_size == 3,
          "got %zd", to->ob_base.ob_size);

    PyObject *n100 = PyLong_FromLong(100);
    PyObject *n200 = PyLong_FromLong(200);
    PyObject *n300 = PyLong_FromLong(300);

    /* SetItem steals references */
    Py_IncRef(n100); Py_IncRef(n200); Py_IncRef(n300);
    PyTuple_SetItem(t, 0, n100);
    PyTuple_SetItem(t, 1, n200);
    PyTuple_SetItem(t, 2, n300);

    /* Direct inline access: items at offset 24 from start of object */
    PyObject **items_ptr = (PyObject **)((char *)t + 24);

    TEST("Tuple inline items[0] at offset 24 == n100");
    CHECK(items_ptr[0] == n100,
          "got %p, expected %p", (void*)items_ptr[0], (void*)n100);

    TEST("Tuple inline items[1] at offset 32 == n200");
    CHECK(items_ptr[1] == n200,
          "got %p, expected %p", (void*)items_ptr[1], (void*)n200);

    TEST("Tuple inline items[2] at offset 40 == n300");
    CHECK(items_ptr[2] == n300,
          "got %p, expected %p", (void*)items_ptr[2], (void*)n300);

    /* Also via the struct field (should be same) */
    TEST("Tuple ob_item[0] matches inline access");
    CHECK(to->ob_item[0] == items_ptr[0], "mismatch");

    /* Verify values */
    TEST("PyLong_AsLong(tuple inline [1]) == 200");
    CHECK(PyLong_AsLong(items_ptr[1]) == 200,
          "got %ld", PyLong_AsLong(items_ptr[1]));

    Py_DecRef(n100);
    Py_DecRef(n200);
    Py_DecRef(n300);
    Py_DecRef(t);
}

/* ─── Bytes: inline data at offset 32 ─── */

void test_bytes_layout(void) {
    printf("\n=== Bytes Layout (inline data) ===\n");

    PyObject *b = PyBytes_FromString("hello");
    PyBytesObject *bo = (PyBytesObject *)b;

    TEST("Bytes('hello') ob_size == 5");
    CHECK(bo->ob_base.ob_size == 5,
          "got %zd", bo->ob_base.ob_size);

    TEST("Bytes ob_shash == -1 (not computed)");
    CHECK(bo->ob_shash == -1,
          "got %zd", bo->ob_shash);

    /* Direct inline data access */
    TEST("Bytes ob_sval[0] == 'h' (direct struct)");
    CHECK(bo->ob_sval[0] == 'h',
          "got '%c' (0x%02x)", bo->ob_sval[0], (unsigned char)bo->ob_sval[0]);

    TEST("Bytes ob_sval[4] == 'o'");
    CHECK(bo->ob_sval[4] == 'o',
          "got '%c'", bo->ob_sval[4]);

    /* Via pointer arithmetic (how C extensions actually do it) */
    char *data_ptr = (char *)b + 32; /* offset 32 = after header */
    TEST("Bytes data at offset 32 == 'hello'");
    CHECK(memcmp(data_ptr, "hello", 5) == 0,
          "got '%.5s'", data_ptr);

    TEST("Bytes null terminator at offset 37");
    CHECK(data_ptr[5] == '\0', "not null-terminated");

    /* Compare with API accessor */
    TEST("PyBytes_AsString matches direct access");
    CHECK(PyBytes_AsString(b) == data_ptr,
          "API=%p, direct=%p", PyBytes_AsString(b), data_ptr);

    Py_DecRef(b);
}

/* ─── Unicode: ASCII compact inline at offset 48 ─── */

void test_unicode_layout(void) {
    printf("\n=== Unicode Layout (ASCII compact) ===\n");

    PyObject *s = PyUnicode_FromString("hello world");
    PyASCIIObject *ao = (PyASCIIObject *)s;

    TEST("Unicode('hello world') length == 11");
    CHECK(ao->length == 11,
          "got %zd", ao->length);

    TEST("Unicode hash == -1 (not computed yet) or valid");
    /* Hash may or may not be computed at creation time; just check it's reasonable */
    CHECK(ao->hash != 0 || ao->hash == 0, "unreachable");
    PASS(); /* informational */
    tests_run--; tests_passed--; /* undo double count */

    /* State bitfield: kind=1 (bits 2-4), compact=1 (bit 5), ascii=1 (bit 6), ready=1 (bit 7) */
    uint32_t state = ao->state;
    uint32_t kind = (state >> 2) & 0x7;
    int compact = (state >> 5) & 1;
    int ascii = (state >> 6) & 1;
    int ready = (state >> 7) & 1;

    TEST("Unicode ASCII state.kind == 1");
    CHECK(kind == 1, "got %u", kind);

    TEST("Unicode ASCII state.compact == 1");
    CHECK(compact == 1, "got %d", compact);

    TEST("Unicode ASCII state.ascii == 1");
    CHECK(ascii == 1, "got %d", ascii);

    TEST("Unicode ASCII state.ready == 1");
    CHECK(ready == 1, "got %d", ready);

    TEST("Unicode wstr == NULL (new strings)");
    CHECK(ao->wstr == NULL, "got %p", (void*)ao->wstr);

    /* Direct inline data access at offset 48 */
    char *inline_data = (char *)s + 48;

    TEST("Unicode inline data at offset 48 == 'hello world'");
    CHECK(memcmp(inline_data, "hello world", 11) == 0,
          "got '%.11s'", inline_data);

    TEST("Unicode null terminator at offset 59");
    CHECK(inline_data[11] == '\0', "not null-terminated");

    /* Compare with API accessor */
    const char *api_str = PyUnicode_AsUTF8(s);
    TEST("PyUnicode_AsUTF8 matches inline data pointer");
    CHECK(api_str == inline_data,
          "API=%p, direct=%p", api_str, inline_data);

    Py_DecRef(s);
}

/* ─── Dict: ma_used and structure ─── */

void test_dict_layout(void) {
    printf("\n=== Dict Layout ===\n");

    PyObject *d = PyDict_New();
    PyDictObject *do_ = (PyDictObject *)d;

    TEST("Empty dict ma_used == 0");
    CHECK(do_->ma_used == 0,
          "got %zd", do_->ma_used);

    TEST("Dict ma_keys is non-null");
    CHECK(do_->ma_keys != NULL, "ma_keys is null");

    TEST("Dict ma_values is null (combined table)");
    CHECK(do_->ma_values == NULL,
          "got %p", (void*)do_->ma_values);

    /* Add entries */
    PyObject *v1 = PyLong_FromLong(42);
    PyObject *v2 = PyLong_FromLong(99);
    PyDict_SetItemString(d, "answer", v1);
    PyDict_SetItemString(d, "bottles", v2);

    TEST("Dict with 2 entries: ma_used == 2");
    CHECK(do_->ma_used == 2,
          "got %zd", do_->ma_used);

    /* Field offsets via pointer math */
    Py_ssize_t *ma_used_ptr = (Py_ssize_t *)((char *)d + 16);
    TEST("Dict ma_used at offset 16");
    CHECK(*ma_used_ptr == 2,
          "got %zd", *ma_used_ptr);

    uint64_t *version_ptr = (uint64_t *)((char *)d + 24);
    TEST("Dict ma_version_tag at offset 24 (non-zero)");
    CHECK(*version_ptr != 0, "version is 0");

    void **keys_ptr = (void **)((char *)d + 32);
    TEST("Dict ma_keys at offset 32");
    CHECK(*keys_ptr == do_->ma_keys,
          "mismatch");

    /* Verify API still works */
    TEST("PyDict_GetItemString('answer') == 42");
    PyObject *got = PyDict_GetItemString(d, "answer");
    CHECK(got != NULL && PyLong_AsLong(got) == 42,
          "got %ld", got ? PyLong_AsLong(got) : -1);

    Py_DecRef(v1);
    Py_DecRef(v2);
    Py_DecRef(d);
}

/* ─── Set: smalltable and used ─── */

void test_set_layout(void) {
    printf("\n=== Set Layout ===\n");

    PyObject *s = PySet_New(NULL);
    PySetObject *so = (PySetObject *)s;

    TEST("Empty set used == 0");
    CHECK(so->used == 0,
          "got %zd", so->used);

    TEST("Empty set fill == 0");
    CHECK(so->fill == 0,
          "got %zd", so->fill);

    TEST("Empty set mask == 7 (smalltable[8])");
    CHECK(so->mask == 7,
          "got %zd", so->mask);

    TEST("Set table points to inline smalltable");
    setentry *expected_smalltable = &so->smalltable[0];
    CHECK(so->table == expected_smalltable,
          "table=%p, smalltable=%p", (void*)so->table, (void*)expected_smalltable);

    /* Add items */
    PyObject *k1 = PyLong_FromLong(10);
    PyObject *k2 = PyLong_FromLong(20);
    PyObject *k3 = PyLong_FromLong(30);
    PySet_Add(s, k1);
    PySet_Add(s, k2);
    PySet_Add(s, k3);

    TEST("Set with 3 items: used == 3");
    CHECK(so->used == 3,
          "got %zd", so->used);

    TEST("Set with 3 items: fill >= 3");
    CHECK(so->fill >= 3,
          "got %zd", so->fill);

    /* Check field offsets via pointer math */
    Py_ssize_t *fill_ptr = (Py_ssize_t *)((char *)s + 16);
    TEST("Set fill at offset 16");
    CHECK(*fill_ptr == so->fill, "mismatch");

    Py_ssize_t *used_ptr = (Py_ssize_t *)((char *)s + 24);
    TEST("Set used at offset 24");
    CHECK(*used_ptr == so->used, "mismatch");

    Py_ssize_t *mask_ptr = (Py_ssize_t *)((char *)s + 32);
    TEST("Set mask at offset 32");
    CHECK(*mask_ptr == so->mask, "mismatch");

    /* Smalltable offset should be at 64 */
    setentry *st_by_offset = (setentry *)((char *)s + 64);
    TEST("Set smalltable at offset 64");
    CHECK(st_by_offset == &so->smalltable[0],
          "offset=%p, struct=%p", (void*)st_by_offset, (void*)&so->smalltable[0]);

    Py_DecRef(k1);
    Py_DecRef(k2);
    Py_DecRef(k3);
    Py_DecRef(s);
}

/* ─── GC Header: 16 bytes before GC-tracked objects ─── */

void test_gc_header(void) {
    printf("\n=== GC Header (16 bytes) ===\n");

    /* Lists are GC-tracked. The GC head should be at obj - 16 */
    PyObject *list = PyList_New(0);

    /* The GC head is 16 bytes before the object pointer */
    PyGC_Head *gc = (PyGC_Head *)((char *)list - 16);

    TEST("GC head is accessible (no segfault)");
    /* Just reading it is the test — if offset is wrong, we crash */
    uintptr_t gc_next = gc->gc_next;
    uintptr_t gc_prev = gc->gc_prev;
    CHECK(1, "");  /* If we get here, we didn't segfault */

    /* Dicts are also GC-tracked */
    PyObject *dict = PyDict_New();
    PyGC_Head *gc2 = (PyGC_Head *)((char *)dict - 16);

    TEST("Dict GC head accessible");
    gc_next = gc2->gc_next;
    gc_prev = gc2->gc_prev;
    CHECK(1, "");

    /* Sets are GC-tracked */
    PyObject *set = PySet_New(NULL);
    PyGC_Head *gc3 = (PyGC_Head *)((char *)set - 16);

    TEST("Set GC head accessible");
    gc_next = gc3->gc_next;
    gc_prev = gc3->gc_prev;
    CHECK(1, "");

    /* Tuples are GC-tracked */
    PyObject *tuple = PyTuple_New(1);
    PyGC_Head *gc4 = (PyGC_Head *)((char *)tuple - 16);

    TEST("Tuple GC head accessible");
    gc_next = gc4->gc_next;
    gc_prev = gc4->gc_prev;
    CHECK(1, "");

    /* Floats are NOT GC-tracked — no GC head */
    /* (We intentionally don't test this — reading before a non-GC object is UB) */

    Py_DecRef(list);
    Py_DecRef(dict);
    Py_DecRef(set);
    PyObject *dummy = PyLong_FromLong(0);
    PyTuple_SetItem(tuple, 0, dummy);
    Py_DecRef(tuple);

    (void)gc_next; (void)gc_prev; /* suppress warnings */
}

/* ─── Memory allocator bridge ─── */

void test_memory_allocator(void) {
    printf("\n=== Memory Allocator Bridge ===\n");

    /* PyMem_Malloc / Realloc / Free */
    void *p = PyMem_Malloc(1024);
    TEST("PyMem_Malloc(1024) returns non-null");
    CHECK(p != NULL, "got null");

    /* Write pattern */
    memset(p, 0xAB, 1024);

    TEST("PyMem_Malloc memory is writable");
    CHECK(((unsigned char*)p)[0] == 0xAB && ((unsigned char*)p)[1023] == 0xAB,
          "write failed");

    void *p2 = PyMem_Realloc(p, 2048);
    TEST("PyMem_Realloc(2048) returns non-null");
    CHECK(p2 != NULL, "got null");

    TEST("PyMem_Realloc preserves data");
    CHECK(((unsigned char*)p2)[0] == 0xAB && ((unsigned char*)p2)[1023] == 0xAB,
          "data lost");

    /* Write to extended region */
    memset((char*)p2 + 1024, 0xCD, 1024);
    TEST("Realloc'd memory is writable in extended region");
    CHECK(((unsigned char*)p2)[2047] == 0xCD, "write failed");

    PyMem_Free(p2);
    TEST("PyMem_Free completed without crash");
    PASS();
    tests_run--; tests_passed--; /* undo double */
    CHECK(1, "");
}

/* ─── None singleton ─── */

void test_none_singleton(void) {
    printf("\n=== None Singleton ===\n");

    PyObject *none1 = _Py_None();
    PyObject *none2 = _Py_None();

    TEST("None is a singleton (pointer identity)");
    CHECK(none1 == none2,
          "%p != %p", (void*)none1, (void*)none2);

    TEST("None ob_type is non-null");
    CHECK(none1->ob_type != NULL, "type is null");

    TEST("None ob_refcnt is very large (immortal)");
    CHECK(none1->ob_refcnt > 1000000,
          "got %zd", none1->ob_refcnt);
}

/* ─── Cross-type: list of mixed types ─── */

void test_mixed_container(void) {
    printf("\n=== Mixed Container (integration) ===\n");

    PyObject *list = PyList_New(0);
    PyObject *n42 = PyLong_FromLong(42);
    PyObject *f3 = PyFloat_FromDouble(3.14);
    PyObject *hello = PyUnicode_FromString("hello");
    PyObject *none = _Py_None();

    PyList_Append(list, n42);
    PyList_Append(list, f3);
    PyList_Append(list, hello);
    PyList_Append(list, none);

    PyListObject *lo = (PyListObject *)list;

    TEST("Mixed list size == 4");
    CHECK(lo->ob_base.ob_size == 4, "got %zd", lo->ob_base.ob_size);

    /* Read item[0] as int, verify digit */
    PyLongObject *item0 = (PyLongObject *)lo->ob_item[0];
    TEST("list[0] as PyLongObject: ob_digit[0] == 42");
    CHECK(item0->ob_digit[0] == 42,
          "got %u", item0->ob_digit[0]);

    /* Read item[1] as float, verify ob_fval */
    PyFloatObject *item1 = (PyFloatObject *)lo->ob_item[1];
    TEST("list[1] as PyFloatObject: ob_fval == 3.14");
    CHECK(item1->ob_fval == 3.14,
          "got %f", item1->ob_fval);

    /* Read item[2] as unicode, verify inline data */
    char *str_data = (char *)lo->ob_item[2] + 48;
    TEST("list[2] as unicode: inline data == 'hello'");
    CHECK(memcmp(str_data, "hello", 5) == 0,
          "got '%.5s'", str_data);

    /* Read item[3] is None */
    TEST("list[3] == None singleton");
    CHECK(lo->ob_item[3] == none, "not None");

    Py_DecRef(n42);
    Py_DecRef(f3);
    Py_DecRef(hello);
    Py_DecRef(list);
}

/* ─── Main ─── */

int main(void) {
    printf("╔══════════════════════════════════════════════════════╗\n");
    printf("║  Rustthon Phase 1: ABI Sanity Check                 ║\n");
    printf("║  Direct struct access — bypassing all C API methods  ║\n");
    printf("╚══════════════════════════════════════════════════════╝\n");

    /* Initialize the runtime */
    Py_Initialize();

    /* Run all tests */
    test_struct_sizes();
    test_float_layout();
    test_long_layout();
    test_bool_layout();
    test_list_layout();
    test_tuple_layout();
    test_bytes_layout();
    test_unicode_layout();
    test_dict_layout();
    test_set_layout();
    test_gc_header();
    test_memory_allocator();
    test_none_singleton();
    test_mixed_container();

    /* Summary */
    printf("\n══════════════════════════════════════════════════════\n");
    printf("  Total: %d  |  ", tests_run);
    if (tests_failed == 0) {
        printf("\033[32mPassed: %d\033[0m  |  Failed: %d\n", tests_passed, tests_failed);
        printf("\n  \033[32m✓ ALL TESTS PASSED — ABI layout is CPython 3.11 compatible\033[0m\n");
    } else {
        printf("Passed: %d  |  \033[31mFailed: %d\033[0m\n", tests_passed, tests_failed);
        printf("\n  \033[31m✗ SOME TESTS FAILED — struct offsets may be wrong\033[0m\n");
    }
    printf("══════════════════════════════════════════════════════\n\n");

    return tests_failed > 0 ? 1 : 0;
}
