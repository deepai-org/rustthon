/*
 * Phase 5: PyO3 Extension Test Driver (bcrypt)
 *
 * Tests that Rustthon can load and run PyO3-generated extensions.
 * bcrypt 4.2.1 is a Rust/PyO3 extension using the stable ABI (abi3).
 *
 * Build:
 *   cc -o test_bcrypt test_bcrypt/test_bcrypt.c -ldl
 *
 * Run:
 *   ./test_bcrypt
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
typedef PyObject *(*fn_PyObject_CallObject)(PyObject *, PyObject *);
typedef PyObject *(*fn_PyObject_GetAttrString)(PyObject *, const char *);
typedef PyObject *(*fn_PyTuple_New)(Py_ssize_t);
typedef int (*fn_PyTuple_SetItem)(PyObject *, Py_ssize_t, PyObject *);
typedef PyObject *(*fn_PyBytes_FromStringAndSize)(const char *, Py_ssize_t);
typedef char *(*fn_PyBytes_AsString)(PyObject *);
typedef Py_ssize_t (*fn_PyBytes_Size)(PyObject *);
typedef PyObject *(*fn_PyLong_FromLong)(long);
typedef long (*fn_PyLong_AsLong)(PyObject *);
typedef void (*fn_Py_IncRef)(PyObject *);
typedef void (*fn_Py_DecRef)(PyObject *);
typedef PyObject *(*fn_PyErr_Occurred)(void);
typedef void (*fn_PyErr_Clear)(void);
typedef void (*fn_PyErr_Print)(void);
typedef int (*fn_PyObject_IsTrue)(PyObject *);

/* Resolved function pointers */
static fn_Py_Initialize         p_Py_Initialize;
static fn_PyUnicode_FromString  p_PyUnicode_FromString;
static fn_PyUnicode_AsUTF8      p_PyUnicode_AsUTF8;
static fn_PyModule_GetDict      p_PyModule_GetDict;
static fn_PyDict_GetItemString  p_PyDict_GetItemString;
static fn_PyObject_Call         p_PyObject_Call;
static fn_PyObject_CallObject   p_PyObject_CallObject;
static fn_PyObject_GetAttrString p_PyObject_GetAttrString;
static fn_PyTuple_New           p_PyTuple_New;
static fn_PyTuple_SetItem       p_PyTuple_SetItem;
static fn_PyBytes_FromStringAndSize p_PyBytes_FromStringAndSize;
static fn_PyBytes_AsString      p_PyBytes_AsString;
static fn_PyBytes_Size          p_PyBytes_Size;
static fn_PyLong_FromLong       p_PyLong_FromLong;
static fn_PyLong_AsLong         p_PyLong_AsLong;
static fn_Py_IncRef             p_Py_IncRef;
static fn_Py_DecRef             p_Py_DecRef;
static fn_PyErr_Occurred        p_PyErr_Occurred;
static fn_PyErr_Clear           p_PyErr_Clear;
static fn_PyErr_Print           p_PyErr_Print;
static fn_PyObject_IsTrue       p_PyObject_IsTrue;

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
    RESOLVE(handle, PyObject_CallObject);
    RESOLVE(handle, PyObject_GetAttrString);
    RESOLVE(handle, PyTuple_New);
    RESOLVE(handle, PyTuple_SetItem);
    RESOLVE(handle, PyBytes_FromStringAndSize);
    RESOLVE(handle, PyBytes_AsString);
    RESOLVE(handle, PyBytes_Size);
    RESOLVE(handle, PyLong_FromLong);
    RESOLVE(handle, PyLong_AsLong);
    RESOLVE(handle, Py_IncRef);
    RESOLVE(handle, Py_DecRef);
    RESOLVE(handle, PyErr_Occurred);
    RESOLVE(handle, PyErr_Clear);
    RESOLVE(handle, PyErr_Print);
    RESOLVE(handle, PyObject_IsTrue);
}

/* Helper: call a function with a single bytes argument */
static PyObject *call_with_bytes(PyObject *func, const char *data, Py_ssize_t len) {
    PyObject *arg = p_PyBytes_FromStringAndSize(data, len);
    if (!arg) return NULL;
    PyObject *args = p_PyTuple_New(1);
    p_PyTuple_SetItem(args, 0, arg);  /* steals ref */
    PyObject *result = p_PyObject_Call(func, args, NULL);
    p_Py_DecRef(args);
    return result;
}

/* Helper: call a function with two bytes arguments */
static PyObject *call_with_two_bytes(PyObject *func, const char *d1, Py_ssize_t l1,
                                     const char *d2, Py_ssize_t l2) {
    PyObject *a1 = p_PyBytes_FromStringAndSize(d1, l1);
    PyObject *a2 = p_PyBytes_FromStringAndSize(d2, l2);
    if (!a1 || !a2) return NULL;
    PyObject *args = p_PyTuple_New(2);
    p_PyTuple_SetItem(args, 0, a1);
    p_PyTuple_SetItem(args, 1, a2);
    PyObject *result = p_PyObject_Call(func, args, NULL);
    p_Py_DecRef(args);
    return result;
}

int main(void) {
    printf("=== Phase 5: PyO3 bcrypt on Rustthon ===\n\n");

    /* Load Rustthon runtime (RTLD_GLOBAL so bcrypt finds Python symbols) */
    printf("[1] Loading Rustthon runtime...\n");
    void *rt = dlopen("target/release/librustthon.dylib", RTLD_NOW | RTLD_GLOBAL);
    if (!rt) {
        printf("FATAL: Cannot load librustthon.dylib: %s\n", dlerror());
        return 1;
    }
    printf("  librustthon.dylib loaded at %p\n", rt);

    resolve_api(rt);

    /* Initialize Python runtime */
    printf("[2] Initializing Python runtime...\n");
    p_Py_Initialize();
    printf("  Py_Initialize() done\n\n");

    /* Load bcrypt extension */
    printf("[3] Loading bcrypt._bcrypt (PyO3/abi3)...\n");

    /* dlopen the extension */
    void *ext = dlopen("test_bcrypt/bcrypt_pkg/bcrypt/_bcrypt.abi3.so",
                        RTLD_NOW | RTLD_GLOBAL);
    if (!ext) {
        printf("FATAL: Cannot load _bcrypt.abi3.so: %s\n", dlerror());
        return 1;
    }
    printf("  _bcrypt.abi3.so loaded at %p\n", ext);

    /* Call PyInit__bcrypt */
    typedef PyObject *(*PyInitFunc)(void);
    PyInitFunc init = (PyInitFunc)dlsym(ext, "PyInit__bcrypt");
    if (!init) {
        printf("FATAL: Cannot find PyInit__bcrypt: %s\n", dlerror());
        return 1;
    }
    printf("  PyInit__bcrypt found at %p\n", (void *)init);

    PyObject *module = init();
    if (!module) {
        printf("FATAL: PyInit__bcrypt returned NULL\n");
        if (p_PyErr_Occurred()) p_PyErr_Print();
        return 1;
    }
    printf("  Module created at %p\n\n", (void *)module);

    /* Get module dict */
    PyObject *dict = p_PyModule_GetDict(module);
    if (!dict) {
        printf("FATAL: PyModule_GetDict returned NULL\n");
        return 1;
    }

    printf("[4] Running bcrypt tests...\n\n");

    /* ── Test 1: Module loads successfully ── */
    TEST("Module _bcrypt loads");
    CHECK(module != NULL, "module is NULL");

    /* ── Test 2: Module has expected attributes ── */
    TEST("Module has __version_ex__ attribute");
    PyObject *version = p_PyDict_GetItemString(dict, "__version_ex__");
    if (version) {
        const char *vs = p_PyUnicode_AsUTF8(version);
        printf("  (version=%s) ", vs ? vs : "???");
        PASS();
    } else {
        /* Try via getattr */
        version = p_PyObject_GetAttrString(module, "__version_ex__");
        if (version) {
            const char *vs = p_PyUnicode_AsUTF8(version);
            printf("  (version=%s) ", vs ? vs : "???");
            PASS();
            p_Py_DecRef(version);
        } else {
            FAIL("attribute not found");
            if (p_PyErr_Occurred()) p_PyErr_Clear();
        }
    }

    /* ── Test 3: gensalt() ── */
    TEST("gensalt() returns bytes");
    PyObject *fn_gensalt = p_PyDict_GetItemString(dict, "gensalt");
    if (!fn_gensalt) fn_gensalt = p_PyObject_GetAttrString(module, "gensalt");
    if (fn_gensalt) {
        PyObject *empty_args = p_PyTuple_New(0);
        PyObject *salt = p_PyObject_Call(fn_gensalt, empty_args, NULL);
        p_Py_DecRef(empty_args);
        if (salt) {
            char *salt_str = p_PyBytes_AsString(salt);
            Py_ssize_t salt_len = p_PyBytes_Size(salt);
            CHECK(salt_str != NULL && salt_len == 29,
                  "expected 29-byte salt, got %zd", salt_len);
            if (salt_str) {
                printf("  salt=%.29s\n", salt_str);
            }
            p_Py_DecRef(salt);
        } else {
            FAIL("gensalt() returned NULL");
            if (p_PyErr_Occurred()) p_PyErr_Print();
        }
    } else {
        FAIL("gensalt not found in module");
        if (p_PyErr_Occurred()) p_PyErr_Clear();
    }

    /* ── Test 4: gensalt(rounds=12) ── */
    TEST("gensalt(rounds=12) respects rounds");
    if (fn_gensalt) {
        PyObject *args = p_PyTuple_New(1);
        p_PyTuple_SetItem(args, 0, p_PyLong_FromLong(12));
        PyObject *salt12 = p_PyObject_Call(fn_gensalt, args, NULL);
        p_Py_DecRef(args);
        if (salt12) {
            char *s = p_PyBytes_AsString(salt12);
            /* bcrypt salt format: $2b$12$... */
            CHECK(s && strncmp(s, "$2b$12$", 7) == 0,
                  "expected $2b$12$ prefix, got %.10s", s ? s : "NULL");
            p_Py_DecRef(salt12);
        } else {
            FAIL("returned NULL");
            if (p_PyErr_Occurred()) p_PyErr_Print();
        }
    } else {
        FAIL("gensalt not found");
    }

    /* ── Test 5: hashpw() ── */
    TEST("hashpw(password, salt) returns hash");
    PyObject *fn_hashpw = p_PyDict_GetItemString(dict, "hashpw");
    if (!fn_hashpw) fn_hashpw = p_PyObject_GetAttrString(module, "hashpw");
    if (fn_hashpw) {
        /* Use a known salt for reproducibility */
        const char *password = "supersecret";
        const char *salt = "$2b$12$WApznUPhDubN0oeveSXHp.";
        PyObject *hash = call_with_two_bytes(fn_hashpw,
            password, strlen(password), salt, strlen(salt));
        if (hash) {
            char *h = p_PyBytes_AsString(hash);
            Py_ssize_t hlen = p_PyBytes_Size(hash);
            CHECK(h && hlen == 60, "expected 60-byte hash, got %zd", hlen);
            if (h) printf("  hash=%s\n", h);
            p_Py_DecRef(hash);
        } else {
            FAIL("hashpw returned NULL");
            if (p_PyErr_Occurred()) p_PyErr_Print();
        }
    } else {
        FAIL("hashpw not found");
        if (p_PyErr_Occurred()) p_PyErr_Clear();
    }

    /* ── Test 6: checkpw() with correct password ── */
    TEST("checkpw(correct_password, hash) returns True");
    PyObject *fn_checkpw = p_PyDict_GetItemString(dict, "checkpw");
    if (!fn_checkpw) fn_checkpw = p_PyObject_GetAttrString(module, "checkpw");
    if (fn_checkpw && fn_hashpw) {
        /* First hash a known password */
        const char *pw = "rustthon_rocks";
        /* Generate a salt */
        PyObject *empty_args = p_PyTuple_New(0);
        PyObject *salt = p_PyObject_Call(fn_gensalt, empty_args, NULL);
        p_Py_DecRef(empty_args);
        if (salt) {
            char *salt_s = p_PyBytes_AsString(salt);
            /* Hash the password */
            PyObject *hashed = call_with_two_bytes(fn_hashpw,
                pw, strlen(pw), salt_s, p_PyBytes_Size(salt));
            if (hashed) {
                char *hash_s = p_PyBytes_AsString(hashed);
                /* Check correct password */
                PyObject *result = call_with_two_bytes(fn_checkpw,
                    pw, strlen(pw), hash_s, p_PyBytes_Size(hashed));
                if (result) {
                    int is_true = p_PyObject_IsTrue(result);
                    CHECK(is_true == 1, "expected True, got %d", is_true);
                    p_Py_DecRef(result);
                } else {
                    FAIL("checkpw returned NULL");
                    if (p_PyErr_Occurred()) p_PyErr_Print();
                }
                p_Py_DecRef(hashed);
            } else {
                FAIL("hashpw returned NULL");
                if (p_PyErr_Occurred()) p_PyErr_Print();
            }
            p_Py_DecRef(salt);
        } else {
            FAIL("gensalt returned NULL");
            if (p_PyErr_Occurred()) p_PyErr_Print();
        }
    } else {
        FAIL("checkpw or hashpw not found");
    }

    /* ── Test 7: checkpw() with wrong password ── */
    TEST("checkpw(wrong_password, hash) returns False");
    if (fn_checkpw && fn_hashpw) {
        const char *pw = "correct_password";
        const char *wrong = "wrong_password";
        PyObject *empty_args = p_PyTuple_New(0);
        PyObject *salt = p_PyObject_Call(fn_gensalt, empty_args, NULL);
        p_Py_DecRef(empty_args);
        if (salt) {
            char *salt_s = p_PyBytes_AsString(salt);
            PyObject *hashed = call_with_two_bytes(fn_hashpw,
                pw, strlen(pw), salt_s, p_PyBytes_Size(salt));
            if (hashed) {
                char *hash_s = p_PyBytes_AsString(hashed);
                PyObject *result = call_with_two_bytes(fn_checkpw,
                    wrong, strlen(wrong), hash_s, p_PyBytes_Size(hashed));
                if (result) {
                    int is_false = p_PyObject_IsTrue(result);
                    CHECK(is_false == 0, "expected False, got %d", is_false);
                    p_Py_DecRef(result);
                } else {
                    FAIL("checkpw returned NULL");
                    if (p_PyErr_Occurred()) p_PyErr_Print();
                }
                p_Py_DecRef(hashed);
            } else {
                FAIL("hashpw returned NULL");
                if (p_PyErr_Occurred()) p_PyErr_Print();
            }
            p_Py_DecRef(salt);
        } else {
            FAIL("gensalt returned NULL");
            if (p_PyErr_Occurred()) p_PyErr_Print();
        }
    } else {
        FAIL("functions not found");
    }

    /* ── Test 8: hashpw deterministic with same salt ── */
    TEST("hashpw() is deterministic with same salt");
    if (fn_hashpw) {
        const char *pw = "test123";
        const char *fixed_salt = "$2b$12$WApznUPhDubN0oeveSXHp.";
        PyObject *h1 = call_with_two_bytes(fn_hashpw, pw, strlen(pw),
            fixed_salt, strlen(fixed_salt));
        PyObject *h2 = call_with_two_bytes(fn_hashpw, pw, strlen(pw),
            fixed_salt, strlen(fixed_salt));
        if (h1 && h2) {
            char *s1 = p_PyBytes_AsString(h1);
            char *s2 = p_PyBytes_AsString(h2);
            CHECK(s1 && s2 && strcmp(s1, s2) == 0,
                  "hashes differ: %s vs %s", s1 ? s1 : "NULL", s2 ? s2 : "NULL");
            p_Py_DecRef(h1);
            p_Py_DecRef(h2);
        } else {
            FAIL("hashpw returned NULL");
            if (p_PyErr_Occurred()) p_PyErr_Print();
        }
    } else {
        FAIL("hashpw not found");
    }

    /* ── Test 9: gensalt produces unique salts ── */
    TEST("gensalt() produces unique salts each call");
    if (fn_gensalt) {
        PyObject *ea = p_PyTuple_New(0);
        PyObject *s1 = p_PyObject_Call(fn_gensalt, ea, NULL);
        PyObject *s2 = p_PyObject_Call(fn_gensalt, ea, NULL);
        p_Py_DecRef(ea);
        if (s1 && s2) {
            char *ss1 = p_PyBytes_AsString(s1);
            char *ss2 = p_PyBytes_AsString(s2);
            CHECK(ss1 && ss2 && strcmp(ss1, ss2) != 0,
                  "salts are identical: %s", ss1 ? ss1 : "NULL");
            p_Py_DecRef(s1);
            p_Py_DecRef(s2);
        } else {
            FAIL("gensalt returned NULL");
            if (p_PyErr_Occurred()) p_PyErr_Print();
        }
    } else {
        FAIL("gensalt not found");
    }

    /* ── Test 10: Module metadata strings ── */
    TEST("Module has __title__ attribute");
    {
        PyObject *title = p_PyDict_GetItemString(dict, "__title__");
        if (!title) title = p_PyObject_GetAttrString(module, "__title__");
        if (title) {
            const char *ts = p_PyUnicode_AsUTF8(title);
            CHECK(ts && strcmp(ts, "bcrypt") == 0,
                  "expected 'bcrypt', got '%s'", ts ? ts : "NULL");
        } else {
            FAIL("not found");
            if (p_PyErr_Occurred()) p_PyErr_Clear();
        }
    }

    /* ── Summary ── */
    printf("\n══════════════════════════════════════════════════════════════\n");
    printf("  PyO3 bcrypt results: %d/%d PASS", tests_passed, tests_run);
    if (tests_failed > 0) {
        printf(", \033[31m%d FAIL\033[0m", tests_failed);
    }
    printf("\n══════════════════════════════════════════════════════════════\n");

    if (tests_failed == 0) {
        printf("\n  bcrypt password hashing on Rustthon! \n\n");
    }

    return tests_failed > 0 ? 1 : 0;
}
