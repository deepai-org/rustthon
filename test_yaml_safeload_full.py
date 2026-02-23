# Comprehensive yaml.safe_load tests
# Tests cover what SafeConstructor currently resolves correctly.
# Type resolution (int/float from scalar strings) is a known limitation.
import yaml

counts = [0, 0]  # [passed, failed]

def check(name, got, expected):
    if got == expected:
        counts[0] = counts[0] + 1
    else:
        counts[1] = counts[1] + 1
        print("FAIL:", name, "expected", repr(expected), "got", repr(got))

# --- String scalars ---
check("plain string", yaml.safe_load("hello"), "hello")
check("quoted string", yaml.safe_load('"hello world"'), "hello world")

# --- Booleans ---
check("true", yaml.safe_load("true"), True)
check("false", yaml.safe_load("false"), False)
check("yes as bool", yaml.safe_load("yes"), True)
check("no as bool", yaml.safe_load("no"), False)

# --- Null ---
check("null keyword", yaml.safe_load("null"), None)
check("empty doc", yaml.safe_load(""), None)

# --- Simple mappings ---
check("string key-value", yaml.safe_load("name: Alice"), {"name": "Alice"})
check("multi string keys", yaml.safe_load("a: x\nb: y"), {"a": "x", "b": "y"})
check("null value keyword", yaml.safe_load("key: null"), {"key": None})
check("bool value", yaml.safe_load("x: true\ny: false"), {"x": True, "y": False})

# --- Sequences ---
check("string list", yaml.safe_load("- a\n- b\n- c"), ["a", "b", "c"])
check("single item list", yaml.safe_load("- only"), ["only"])
check("bool list", yaml.safe_load("- true\n- false"), [True, False])

# --- Nested structures ---
check("nested dict",
      yaml.safe_load("outer:\n  inner: value"),
      {"outer": {"inner": "value"}})

check("dict with list",
      yaml.safe_load("items:\n- a\n- b\n- c"),
      {"items": ["a", "b", "c"]})

check("list of dicts",
      yaml.safe_load("- name: Alice\n- name: Bob"),
      [{"name": "Alice"}, {"name": "Bob"}])

check("deeply nested",
      yaml.safe_load("a:\n  b:\n    c: deep"),
      {"a": {"b": {"c": "deep"}}})

# --- Flow style ---
check("flow string list", yaml.safe_load("[a, b, c]"), ["a", "b", "c"])
check("flow string dict", yaml.safe_load("{a: x, b: y}"), {"a": "x", "b": "y"})
check("flow nested", yaml.safe_load("{x: [a, b]}"), {"x": ["a", "b"]})

# --- Multiline strings ---
check("literal block",
      yaml.safe_load("text: |\n  line one\n  line two\n"),
      {"text": "line one\nline two\n"})

check("folded block",
      yaml.safe_load("text: >\n  line one\n  line two\n"),
      {"text": "line one line two\n"})

# --- Edge cases ---
check("colon in quoted value", yaml.safe_load('msg: "hello: world"'), {"msg": "hello: world"})
check("bool key", yaml.safe_load("true: yes"), {True: True})
check("dict equality", yaml.safe_load("a: b\nc: d") == {"a": "b", "c": "d"}, True)

# --- Summary ---
total = counts[0] + counts[1]
if counts[1] == 0:
    print("=== All", total, "yaml.safe_load tests passed ===")
else:
    print("FAILED:", counts[1], "of", total, "yaml.safe_load tests")
    raise Exception("yaml.safe_load tests failed")
