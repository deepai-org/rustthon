/*
 * Phase 6: Prebuilt pyyaml (Cython extension) Test Driver
 *
 * Tests that Rustthon can load and run the prebuilt _yaml.cpython-311-darwin.so
 * from the PyPI pyyaml 6.0.3 wheel (Cython-generated, compiled against CPython 3.11).
 *
 * Since _yaml needs `import yaml` to work, we pre-create a stub yaml module
 * with all the classes _yaml expects (error types, token types, event types, node types).
 *
 * Build:
 *   cc -o test_pyyaml test_pyyaml/test_pyyaml.c -ldl
 *
 * Run:
 *   YAML_SO=/tmp/cython_wheels/extracted/pyyaml/yaml/_yaml.cpython-311-darwin.so ./test_pyyaml
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

/* ─── Test infrastructure ─── */
static int tests_run = 0;
static int tests_passed = 0;
static int tests_failed = 0;

#define TEST(name) do { tests_run++; printf("  %-55s ", name); } while(0)
#define PASS() do { tests_passed++; printf("\033[32mPASS\033[0m\n"); } while(0)
#define FAIL(fmt, ...) do { tests_failed++; printf("\033[31mFAIL\033[0m  " fmt "\n", ##__VA_ARGS__); } while(0)
#define CHECK(cond, fmt, ...) do { if (cond) { PASS(); } else { FAIL(fmt, ##__VA_ARGS__); } } while(0)

/* ─── Function pointer types ─── */
typedef void (*fn_Py_Initialize)(void);
typedef PyObject *(*fn_PyUnicode_FromString)(const char *);
typedef const char *(*fn_PyUnicode_AsUTF8)(PyObject *);
typedef PyObject *(*fn_PyModule_GetDict)(PyObject *);
typedef PyObject *(*fn_PyModule_NewObject)(PyObject *);
typedef PyObject *(*fn_PyDict_GetItemString)(PyObject *, const char *);
typedef int (*fn_PyDict_SetItemString)(PyObject *, const char *, PyObject *);
typedef PyObject *(*fn_PyObject_Call)(PyObject *, PyObject *, PyObject *);
typedef PyObject *(*fn_PyTuple_New)(Py_ssize_t);
typedef int (*fn_PyTuple_SetItem)(PyObject *, Py_ssize_t, PyObject *);
typedef PyObject *(*fn_PyLong_FromLong)(long);
typedef PyObject *(*fn_PyFloat_FromDouble)(double);
typedef long (*fn_PyLong_AsLong)(PyObject *);
typedef double (*fn_PyFloat_AsDouble)(PyObject *);
typedef PyObject *(*fn_PyDict_New)(void);
typedef PyObject *(*fn_PyList_New)(Py_ssize_t);
typedef int (*fn_PyList_Append)(PyObject *, PyObject *);
typedef Py_ssize_t (*fn_PyList_Size)(PyObject *);
typedef PyObject *(*fn_PyList_GetItem)(PyObject *, Py_ssize_t);
typedef PyObject *(*fn_PyErr_Occurred)(void);
typedef void (*fn_PyErr_Clear)(void);
typedef void (*fn_PyErr_Print)(void);
typedef int (*fn_PyUnicode_Check)(PyObject *);
typedef int (*fn_PyLong_Check)(PyObject *);
typedef int (*fn_PyFloat_Check)(PyObject *);
typedef int (*fn_PyDict_Check)(PyObject *);
typedef int (*fn_PyList_Check)(PyObject *);
typedef int (*fn_PyBool_Check)(PyObject *);
typedef void (*fn_Py_IncRef)(PyObject *);
typedef void (*fn_Py_DecRef)(PyObject *);
typedef PyObject *(*fn_PyObject_GetAttrString)(PyObject *, const char *);
typedef PyObject *(*fn_PyObject_Str)(PyObject *);
typedef PyObject *(*fn_PyErr_NewException)(const char *, PyObject *, PyObject *);
typedef PyObject *(*fn_CreateStubType)(const char *, PyObject *);
typedef void (*fn_register_module)(const char *, PyObject *);

/* Module init function type */
typedef PyObject *(*fn_PyInit)(void);

/* Resolved function pointers */
static fn_Py_Initialize         p_Py_Initialize;
static fn_PyUnicode_FromString  p_PyUnicode_FromString;
static fn_PyUnicode_AsUTF8      p_PyUnicode_AsUTF8;
static fn_PyModule_GetDict      p_PyModule_GetDict;
static fn_PyModule_NewObject    p_PyModule_NewObject;
static fn_PyDict_GetItemString  p_PyDict_GetItemString;
static fn_PyDict_SetItemString  p_PyDict_SetItemString;
static fn_PyObject_Call         p_PyObject_Call;
static fn_PyTuple_New           p_PyTuple_New;
static fn_PyTuple_SetItem       p_PyTuple_SetItem;
static fn_PyLong_FromLong       p_PyLong_FromLong;
static fn_PyFloat_FromDouble    p_PyFloat_FromDouble;
static fn_PyLong_AsLong         p_PyLong_AsLong;
static fn_PyFloat_AsDouble      p_PyFloat_AsDouble;
static fn_PyDict_New            p_PyDict_New;
static fn_PyList_New            p_PyList_New;
static fn_PyList_Append         p_PyList_Append;
static fn_PyList_Size           p_PyList_Size;
static fn_PyList_GetItem        p_PyList_GetItem;
static fn_PyErr_Occurred        p_PyErr_Occurred;
static fn_PyErr_Clear           p_PyErr_Clear;
static fn_PyErr_Print           p_PyErr_Print;
static fn_PyUnicode_Check       p_PyUnicode_Check;
static fn_PyLong_Check          p_PyLong_Check;
static fn_PyFloat_Check         p_PyFloat_Check;
static fn_PyDict_Check          p_PyDict_Check;
static fn_PyList_Check          p_PyList_Check;
static fn_PyBool_Check          p_PyBool_Check;
static fn_Py_IncRef             p_Py_IncRef;
static fn_Py_DecRef             p_Py_DecRef;
static fn_PyObject_GetAttrString p_PyObject_GetAttrString;
static fn_PyObject_Str          p_PyObject_Str;
static fn_PyErr_NewException    p_PyErr_NewException;
static fn_CreateStubType        p_CreateStubType;

/* Module registration (we need to call PyImport_AddModule or register directly) */
static fn_register_module       p_PyImport_AddModule;

#define RESOLVE(handle, name) do { \
    p_##name = (fn_##name)dlsym(handle, #name); \
    if (!p_##name) { \
        fprintf(stderr, "Failed to resolve " #name ": %s\n", dlerror()); \
        return 1; \
    } \
} while(0)

/* ─── Helper: create exception type ─── */
static PyObject *make_exc(const char *name, PyObject *base) {
    return p_PyErr_NewException(name, base, NULL);
}

/* ─── Helper: create stub type (non-exception, supports instance creation + setattr) ─── */
static PyObject *make_type(const char *name, PyObject *base) {
    return p_CreateStubType(name, base);
}

/* ─── Helper: add to module dict ─── */
static void add_to(PyObject *dict, const char *attr, PyObject *obj) {
    if (obj) {
        p_PyDict_SetItemString(dict, attr, obj);
    } else {
        fprintf(stderr, "  WARNING: Failed to create stub for %s\n", attr);
    }
}

/*
 * Create a stub "yaml" package with submodules containing all the classes
 * that _yaml expects.
 *
 * Cython's __Pyx_ImportDottedModule("yaml.error") does:
 *   1. PyImport_GetModule("yaml.error") → check module registry
 *   2. Walk-parts: import("yaml") then getattr(yaml, "error")
 *
 * So we need both:
 *   - Submodule objects registered as "yaml.error", "yaml.tokens", etc.
 *   - Submodules as attributes on the yaml module (yaml.error = <module>)
 *   - Types in BOTH the submodule dict AND the main yaml dict
 */
static int create_yaml_stubs(void *rh) {
    /* Get exception base class */
    PyObject **pExc_Exception = (PyObject **)dlsym(rh, "PyExc_Exception");
    if (!pExc_Exception || !*pExc_Exception) {
        fprintf(stderr, "Failed to get PyExc_Exception\n");
        return -1;
    }
    PyObject *exc = *pExc_Exception;

    /* Resolve PyImport_AddModule */
    typedef PyObject *(*fn_AddModule)(const char *);
    fn_AddModule add_mod = (fn_AddModule)dlsym(rh, "PyImport_AddModule");
    if (!add_mod) {
        fprintf(stderr, "Failed to resolve PyImport_AddModule\n");
        return -1;
    }

    /* Create main yaml module and submodules */
    PyObject *yaml_mod = add_mod("yaml");
    if (!yaml_mod) { fprintf(stderr, "Failed to create yaml\n"); return -1; }
    PyObject *yaml_dict = p_PyModule_GetDict(yaml_mod);

    /* Create submodules — must also be registered in module registry */
    PyObject *error_mod = add_mod("yaml.error");
    PyObject *tokens_mod = add_mod("yaml.tokens");
    PyObject *events_mod = add_mod("yaml.events");
    PyObject *nodes_mod = add_mod("yaml.nodes");
    PyObject *scanner_mod = add_mod("yaml.scanner");
    PyObject *parser_mod = add_mod("yaml.parser");
    PyObject *reader_mod = add_mod("yaml.reader");
    PyObject *emitter_mod = add_mod("yaml.emitter");
    PyObject *serializer_mod = add_mod("yaml.serializer");
    PyObject *representer_mod = add_mod("yaml.representer");
    PyObject *composer_mod = add_mod("yaml.composer");
    PyObject *constructor_mod = add_mod("yaml.constructor");

    /* Add submodules as attributes of yaml package */
    add_to(yaml_dict, "error", error_mod);
    add_to(yaml_dict, "tokens", tokens_mod);
    add_to(yaml_dict, "events", events_mod);
    add_to(yaml_dict, "nodes", nodes_mod);
    add_to(yaml_dict, "scanner", scanner_mod);
    add_to(yaml_dict, "parser", parser_mod);
    add_to(yaml_dict, "reader", reader_mod);
    add_to(yaml_dict, "emitter", emitter_mod);
    add_to(yaml_dict, "serializer", serializer_mod);
    add_to(yaml_dict, "representer", representer_mod);
    add_to(yaml_dict, "composer", composer_mod);
    add_to(yaml_dict, "constructor", constructor_mod);

    PyObject *err_dict = p_PyModule_GetDict(error_mod);
    PyObject *tok_dict = p_PyModule_GetDict(tokens_mod);
    PyObject *evt_dict = p_PyModule_GetDict(events_mod);
    PyObject *nod_dict = p_PyModule_GetDict(nodes_mod);

    /* ─── Error types (Exception subclasses) ─── */
    PyObject *yaml_error = make_exc("yaml.error.YAMLError", exc);
    PyObject *marked_error = make_exc("yaml.error.MarkedYAMLError", yaml_error);
    /* Add to both yaml and yaml.error */
    add_to(yaml_dict, "YAMLError", yaml_error);
    add_to(yaml_dict, "MarkedYAMLError", marked_error);
    add_to(err_dict, "YAMLError", yaml_error);
    add_to(err_dict, "MarkedYAMLError", marked_error);

    /* Error subclasses — add to yaml dict, submodule dicts, and yaml.error */
    PyObject *scanner_err = make_exc("yaml.scanner.ScannerError", marked_error);
    PyObject *parser_err = make_exc("yaml.parser.ParserError", marked_error);
    PyObject *reader_err = make_exc("yaml.reader.ReaderError", yaml_error);
    PyObject *emitter_err = make_exc("yaml.emitter.EmitterError", yaml_error);
    PyObject *serializer_err = make_exc("yaml.serializer.SerializerError", yaml_error);
    PyObject *representer_err = make_exc("yaml.representer.RepresenterError", yaml_error);
    PyObject *composer_err = make_exc("yaml.composer.ComposerError", marked_error);
    PyObject *constructor_err = make_exc("yaml.constructor.ConstructorError", marked_error);

    add_to(yaml_dict, "ScannerError", scanner_err);
    add_to(yaml_dict, "ParserError", parser_err);
    add_to(yaml_dict, "ReaderError", reader_err);
    add_to(yaml_dict, "EmitterError", emitter_err);
    add_to(yaml_dict, "SerializerError", serializer_err);
    add_to(yaml_dict, "RepresenterError", representer_err);
    add_to(yaml_dict, "ComposerError", composer_err);
    add_to(yaml_dict, "ConstructorError", constructor_err);

    add_to(p_PyModule_GetDict(scanner_mod), "ScannerError", scanner_err);
    add_to(p_PyModule_GetDict(parser_mod), "ParserError", parser_err);
    add_to(p_PyModule_GetDict(reader_mod), "ReaderError", reader_err);
    add_to(p_PyModule_GetDict(emitter_mod), "EmitterError", emitter_err);
    add_to(p_PyModule_GetDict(serializer_mod), "SerializerError", serializer_err);
    add_to(p_PyModule_GetDict(representer_mod), "RepresenterError", representer_err);
    add_to(p_PyModule_GetDict(composer_mod), "ComposerError", composer_err);
    add_to(p_PyModule_GetDict(constructor_mod), "ConstructorError", constructor_err);

    /* ─── Token types (object subclasses) ─── */
    PyObject *token = make_type("yaml.tokens.Token", NULL);
    add_to(yaml_dict, "Token", token);
    add_to(tok_dict, "Token", token);
    add_to(tok_dict, "DirectiveToken", make_type("yaml.tokens.DirectiveToken", token));
    add_to(tok_dict, "DocumentStartToken", make_type("yaml.tokens.DocumentStartToken", token));
    add_to(tok_dict, "DocumentEndToken", make_type("yaml.tokens.DocumentEndToken", token));
    add_to(tok_dict, "StreamStartToken", make_type("yaml.tokens.StreamStartToken", token));
    add_to(tok_dict, "StreamEndToken", make_type("yaml.tokens.StreamEndToken", token));
    add_to(tok_dict, "BlockSequenceStartToken", make_type("yaml.tokens.BlockSequenceStartToken", token));
    add_to(tok_dict, "BlockMappingStartToken", make_type("yaml.tokens.BlockMappingStartToken", token));
    add_to(tok_dict, "BlockEndToken", make_type("yaml.tokens.BlockEndToken", token));
    add_to(tok_dict, "FlowSequenceStartToken", make_type("yaml.tokens.FlowSequenceStartToken", token));
    add_to(tok_dict, "FlowMappingStartToken", make_type("yaml.tokens.FlowMappingStartToken", token));
    add_to(tok_dict, "FlowSequenceEndToken", make_type("yaml.tokens.FlowSequenceEndToken", token));
    add_to(tok_dict, "FlowMappingEndToken", make_type("yaml.tokens.FlowMappingEndToken", token));
    add_to(tok_dict, "KeyToken", make_type("yaml.tokens.KeyToken", token));
    add_to(tok_dict, "ValueToken", make_type("yaml.tokens.ValueToken", token));
    add_to(tok_dict, "BlockEntryToken", make_type("yaml.tokens.BlockEntryToken", token));
    add_to(tok_dict, "FlowEntryToken", make_type("yaml.tokens.FlowEntryToken", token));
    add_to(tok_dict, "AliasToken", make_type("yaml.tokens.AliasToken", token));
    add_to(tok_dict, "AnchorToken", make_type("yaml.tokens.AnchorToken", token));
    add_to(tok_dict, "TagToken", make_type("yaml.tokens.TagToken", token));
    add_to(tok_dict, "ScalarToken", make_type("yaml.tokens.ScalarToken", token));
    /* Also mirror token types into yaml dict */
    add_to(yaml_dict, "DirectiveToken", p_PyDict_GetItemString(tok_dict, "DirectiveToken"));
    add_to(yaml_dict, "DocumentStartToken", p_PyDict_GetItemString(tok_dict, "DocumentStartToken"));
    add_to(yaml_dict, "DocumentEndToken", p_PyDict_GetItemString(tok_dict, "DocumentEndToken"));
    add_to(yaml_dict, "StreamStartToken", p_PyDict_GetItemString(tok_dict, "StreamStartToken"));
    add_to(yaml_dict, "StreamEndToken", p_PyDict_GetItemString(tok_dict, "StreamEndToken"));
    add_to(yaml_dict, "BlockSequenceStartToken", p_PyDict_GetItemString(tok_dict, "BlockSequenceStartToken"));
    add_to(yaml_dict, "BlockMappingStartToken", p_PyDict_GetItemString(tok_dict, "BlockMappingStartToken"));
    add_to(yaml_dict, "BlockEndToken", p_PyDict_GetItemString(tok_dict, "BlockEndToken"));
    add_to(yaml_dict, "FlowSequenceStartToken", p_PyDict_GetItemString(tok_dict, "FlowSequenceStartToken"));
    add_to(yaml_dict, "FlowMappingStartToken", p_PyDict_GetItemString(tok_dict, "FlowMappingStartToken"));
    add_to(yaml_dict, "FlowSequenceEndToken", p_PyDict_GetItemString(tok_dict, "FlowSequenceEndToken"));
    add_to(yaml_dict, "FlowMappingEndToken", p_PyDict_GetItemString(tok_dict, "FlowMappingEndToken"));
    add_to(yaml_dict, "KeyToken", p_PyDict_GetItemString(tok_dict, "KeyToken"));
    add_to(yaml_dict, "ValueToken", p_PyDict_GetItemString(tok_dict, "ValueToken"));
    add_to(yaml_dict, "BlockEntryToken", p_PyDict_GetItemString(tok_dict, "BlockEntryToken"));
    add_to(yaml_dict, "FlowEntryToken", p_PyDict_GetItemString(tok_dict, "FlowEntryToken"));
    add_to(yaml_dict, "AliasToken", p_PyDict_GetItemString(tok_dict, "AliasToken"));
    add_to(yaml_dict, "AnchorToken", p_PyDict_GetItemString(tok_dict, "AnchorToken"));
    add_to(yaml_dict, "TagToken", p_PyDict_GetItemString(tok_dict, "TagToken"));
    add_to(yaml_dict, "ScalarToken", p_PyDict_GetItemString(tok_dict, "ScalarToken"));

    /* ─── Event types (object subclasses) ─── */
    PyObject *event = make_type("yaml.events.Event", NULL);
    PyObject *node_event = make_type("yaml.events.NodeEvent", event);
    PyObject *collection_start = make_type("yaml.events.CollectionStartEvent", node_event);
    PyObject *collection_end = make_type("yaml.events.CollectionEndEvent", event);
    add_to(yaml_dict, "Event", event);
    add_to(yaml_dict, "NodeEvent", node_event);
    add_to(yaml_dict, "CollectionStartEvent", collection_start);
    add_to(yaml_dict, "CollectionEndEvent", collection_end);
    add_to(evt_dict, "Event", event);
    add_to(evt_dict, "NodeEvent", node_event);
    add_to(evt_dict, "CollectionStartEvent", collection_start);
    add_to(evt_dict, "CollectionEndEvent", collection_end);

    PyObject *stream_start_e = make_type("yaml.events.StreamStartEvent", event);
    PyObject *stream_end_e = make_type("yaml.events.StreamEndEvent", event);
    PyObject *doc_start_e = make_type("yaml.events.DocumentStartEvent", event);
    PyObject *doc_end_e = make_type("yaml.events.DocumentEndEvent", event);
    PyObject *alias_e = make_type("yaml.events.AliasEvent", node_event);
    PyObject *scalar_e = make_type("yaml.events.ScalarEvent", node_event);
    PyObject *seq_start_e = make_type("yaml.events.SequenceStartEvent", collection_start);
    PyObject *seq_end_e = make_type("yaml.events.SequenceEndEvent", collection_end);
    PyObject *map_start_e = make_type("yaml.events.MappingStartEvent", collection_start);
    PyObject *map_end_e = make_type("yaml.events.MappingEndEvent", collection_end);

    add_to(evt_dict, "StreamStartEvent", stream_start_e);
    add_to(evt_dict, "StreamEndEvent", stream_end_e);
    add_to(evt_dict, "DocumentStartEvent", doc_start_e);
    add_to(evt_dict, "DocumentEndEvent", doc_end_e);
    add_to(evt_dict, "AliasEvent", alias_e);
    add_to(evt_dict, "ScalarEvent", scalar_e);
    add_to(evt_dict, "SequenceStartEvent", seq_start_e);
    add_to(evt_dict, "SequenceEndEvent", seq_end_e);
    add_to(evt_dict, "MappingStartEvent", map_start_e);
    add_to(evt_dict, "MappingEndEvent", map_end_e);

    add_to(yaml_dict, "StreamStartEvent", stream_start_e);
    add_to(yaml_dict, "StreamEndEvent", stream_end_e);
    add_to(yaml_dict, "DocumentStartEvent", doc_start_e);
    add_to(yaml_dict, "DocumentEndEvent", doc_end_e);
    add_to(yaml_dict, "AliasEvent", alias_e);
    add_to(yaml_dict, "ScalarEvent", scalar_e);
    add_to(yaml_dict, "SequenceStartEvent", seq_start_e);
    add_to(yaml_dict, "SequenceEndEvent", seq_end_e);
    add_to(yaml_dict, "MappingStartEvent", map_start_e);
    add_to(yaml_dict, "MappingEndEvent", map_end_e);

    /* ─── Node types (object subclasses) ─── */
    PyObject *node = make_type("yaml.nodes.Node", NULL);
    PyObject *scalar_n = make_type("yaml.nodes.ScalarNode", node);
    PyObject *seq_n = make_type("yaml.nodes.SequenceNode", node);
    PyObject *map_n = make_type("yaml.nodes.MappingNode", node);
    add_to(yaml_dict, "Node", node);
    add_to(yaml_dict, "ScalarNode", scalar_n);
    add_to(yaml_dict, "SequenceNode", seq_n);
    add_to(yaml_dict, "MappingNode", map_n);
    add_to(nod_dict, "Node", node);
    add_to(nod_dict, "ScalarNode", scalar_n);
    add_to(nod_dict, "SequenceNode", seq_n);
    add_to(nod_dict, "MappingNode", map_n);

    /* ─── Mark type (from yaml.error) ─── */
    PyObject *mark = make_type("yaml.error.Mark", NULL);
    add_to(yaml_dict, "Mark", mark);
    add_to(err_dict, "Mark", mark);

    /* ─── __version__ ─── */
    PyObject *ver = p_PyUnicode_FromString("6.0.3");
    add_to(yaml_dict, "__version__", ver);

    return 0;
}

int main(int argc, char **argv) {
    /* ─── Load librustthon ─── */
    const char *rustthon_lib = getenv("RUSTTHON_LIB");
    if (!rustthon_lib)
        rustthon_lib = "target/release/librustthon.dylib";

    void *rh = dlopen(rustthon_lib, RTLD_NOW | RTLD_GLOBAL);
    if (!rh) {
        fprintf(stderr, "Failed to load librustthon: %s\n", dlerror());
        return 1;
    }

    /* Resolve Rustthon API functions */
    RESOLVE(rh, Py_Initialize);
    RESOLVE(rh, PyUnicode_FromString);
    RESOLVE(rh, PyUnicode_AsUTF8);
    RESOLVE(rh, PyModule_GetDict);
    RESOLVE(rh, PyModule_NewObject);
    RESOLVE(rh, PyDict_GetItemString);
    RESOLVE(rh, PyDict_SetItemString);
    RESOLVE(rh, PyObject_Call);
    RESOLVE(rh, PyTuple_New);
    RESOLVE(rh, PyTuple_SetItem);
    RESOLVE(rh, PyLong_FromLong);
    RESOLVE(rh, PyFloat_FromDouble);
    RESOLVE(rh, PyLong_AsLong);
    RESOLVE(rh, PyFloat_AsDouble);
    RESOLVE(rh, PyDict_New);
    RESOLVE(rh, PyList_New);
    RESOLVE(rh, PyList_Append);
    RESOLVE(rh, PyList_Size);
    RESOLVE(rh, PyList_GetItem);
    RESOLVE(rh, PyErr_Occurred);
    RESOLVE(rh, PyErr_Clear);
    RESOLVE(rh, PyErr_Print);
    RESOLVE(rh, PyUnicode_Check);
    RESOLVE(rh, PyLong_Check);
    RESOLVE(rh, PyFloat_Check);
    RESOLVE(rh, PyDict_Check);
    RESOLVE(rh, PyList_Check);
    RESOLVE(rh, PyBool_Check);
    RESOLVE(rh, Py_IncRef);
    RESOLVE(rh, Py_DecRef);
    RESOLVE(rh, PyObject_GetAttrString);
    RESOLVE(rh, PyObject_Str);
    RESOLVE(rh, PyErr_NewException);

    /* Resolve our stub type helper */
    p_CreateStubType = (fn_CreateStubType)dlsym(rh, "_Rustthon_CreateStubType");
    if (!p_CreateStubType) {
        fprintf(stderr, "Failed to resolve _Rustthon_CreateStubType: %s\n", dlerror());
        return 1;
    }

    /* Initialize the Rustthon runtime */
    p_Py_Initialize();

    /* Create yaml stub module BEFORE loading _yaml */
    if (create_yaml_stubs(rh) != 0) {
        fprintf(stderr, "Failed to create yaml stubs\n");
        return 1;
    }

    /* ─── Load prebuilt _yaml extension ─── */
    const char *yaml_so = getenv("YAML_SO");
    if (!yaml_so)
        yaml_so = "/tmp/cython_wheels/extracted/pyyaml/yaml/_yaml.cpython-311-darwin.so";

    void *yh = dlopen(yaml_so, RTLD_NOW | RTLD_GLOBAL);
    if (!yh) {
        fprintf(stderr, "Failed to load _yaml.so: %s\n", dlerror());
        return 1;
    }

    fn_PyInit pyinit_yaml = (fn_PyInit)dlsym(yh, "PyInit__yaml");
    if (!pyinit_yaml) {
        fprintf(stderr, "No PyInit__yaml: %s\n", dlerror());
        return 1;
    }

    printf("\n\033[36m\033[1mPhase 6: Prebuilt pyyaml (Cython)\033[0m\n");

    /* ─── Test 1: Module initializes ─── */
    TEST("_yaml module initializes");
    PyObject *yaml_mod = pyinit_yaml();
    if (!yaml_mod) {
        FAIL("PyInit__yaml returned NULL");
        if (p_PyErr_Occurred()) p_PyErr_Print();
        goto done;
    }
    PASS();

    /* ─── Test 2: Module has get_version_string ─── */
    TEST("Module has get_version_string");
    PyObject *mod_dict = p_PyModule_GetDict(yaml_mod);
    PyObject *get_version_string = mod_dict ? p_PyDict_GetItemString(mod_dict, "get_version_string") : NULL;
    CHECK(get_version_string != NULL, "get_version_string not found in module dict");

    /* ─── Test 3: Call get_version_string ─── */
    if (get_version_string) {
        TEST("get_version_string() returns a string");
        PyObject *empty_args = p_PyTuple_New(0);
        PyObject *version = p_PyObject_Call(get_version_string, empty_args, NULL);
        p_Py_DecRef(empty_args);
        if (version && p_PyUnicode_Check(version)) {
            const char *v = p_PyUnicode_AsUTF8(version);
            printf("  [version=%s] ", v ? v : "NULL");
            CHECK(v != NULL && strlen(v) > 0, "empty version string");
        } else {
            FAIL("returned non-string or NULL");
            if (p_PyErr_Occurred()) p_PyErr_Print();
        }
    }

    /* ─── Test 4: Module has get_version ─── */
    TEST("Module has get_version");
    PyObject *get_version = mod_dict ? p_PyDict_GetItemString(mod_dict, "get_version") : NULL;
    CHECK(get_version != NULL, "get_version not found");

    /* ─── Test 5: Call get_version → tuple ─── */
    if (get_version) {
        TEST("get_version() returns a tuple");
        PyObject *empty_args = p_PyTuple_New(0);
        PyObject *ver_tuple = p_PyObject_Call(get_version, empty_args, NULL);
        p_Py_DecRef(empty_args);
        CHECK(ver_tuple != NULL, "returned NULL");
        if (!ver_tuple && p_PyErr_Occurred()) {
            p_PyErr_Print();
        }
    }

    /* ─── Test 6: Module has CParser and CEmitter types ─── */
    TEST("Module has CParser type");
    PyObject *cparser = mod_dict ? p_PyDict_GetItemString(mod_dict, "CParser") : NULL;
    CHECK(cparser != NULL, "CParser not found");

    TEST("Module has CEmitter type");
    PyObject *cemitter = mod_dict ? p_PyDict_GetItemString(mod_dict, "CEmitter") : NULL;
    CHECK(cemitter != NULL, "CEmitter not found");

    /* ─── Test 7: Module has Mark type ─── */
    TEST("Module has Mark type");
    PyObject *mark = mod_dict ? p_PyDict_GetItemString(mod_dict, "Mark") : NULL;
    CHECK(mark != NULL, "Mark not found");

    /* ─── Test 8: Module has error types ─── */
    TEST("Module has ScannerError");
    CHECK(p_PyDict_GetItemString(mod_dict, "ScannerError") != NULL, "not found");

    TEST("Module has ParserError");
    CHECK(p_PyDict_GetItemString(mod_dict, "ParserError") != NULL, "not found");

    TEST("Module has ReaderError");
    CHECK(p_PyDict_GetItemString(mod_dict, "ReaderError") != NULL, "not found");

    /* ─── Test 9: Create a CParser instance ─── */
    TEST("CParser can be instantiated with a string");
    if (cparser) {
        PyObject *yaml_str = p_PyUnicode_FromString("key: value\n");
        PyObject *args = p_PyTuple_New(1);
        p_Py_IncRef(yaml_str);
        p_PyTuple_SetItem(args, 0, yaml_str);
        PyObject *parser = p_PyObject_Call(cparser, args, NULL);
        p_Py_DecRef(args);
        p_Py_DecRef(yaml_str);
        if (parser) {
            PASS();
            /* Test 10: Parser has get_event method */
            TEST("CParser has get_event method");
            PyObject *get_event = p_PyObject_GetAttrString(parser, "get_event");
            CHECK(get_event != NULL, "get_event not found");

            if (get_event) {
                /* Test 11: Call get_event — first event should be StreamStartEvent */
                TEST("get_event() returns an event");
                PyObject *empty = p_PyTuple_New(0);
                PyObject *event = p_PyObject_Call(get_event, empty, NULL);
                p_Py_DecRef(empty);
                if (event) {
                    PASS();
                } else {
                    FAIL("returned NULL");
                    if (p_PyErr_Occurred()) p_PyErr_Print();
                }
                p_Py_DecRef(get_event);
            }
            p_Py_DecRef(parser);
        } else {
            FAIL("CParser() returned NULL");
            PyObject *err = p_PyErr_Occurred();
            if (err) {
                /* Try to get error message */
                PyObject *err_str = p_PyObject_Str(err);
                if (err_str) {
                    const char *msg = p_PyUnicode_AsUTF8(err_str);
                    fprintf(stderr, "  Error type: %s\n", msg ? msg : "(null)");
                }
                p_PyErr_Print();
                p_PyErr_Clear();
            } else {
                fprintf(stderr, "  No error set after CParser() returned NULL\n");
            }
        }
    }

done:
    printf("\n  Total: %d  |  \033[32mPassed: %d\033[0m  |  Failed: %d\n", tests_run, tests_passed, tests_failed);
    if (tests_failed == 0) {
        printf("  \033[32m✓ ALL TESTS PASSED — pyyaml (Cython) works on Rustthon!\033[0m\n");
    }

    return tests_failed > 0 ? 1 : 0;
}
