# Test native import of prebuilt ujson C extension from VM Python source
import ujson

counts = [0, 0]  # [passed, failed]

def check(name, got, expected):
    if got == expected:
        counts[0] = counts[0] + 1
    else:
        counts[1] = counts[1] + 1
        print("FAIL:", name, "expected", repr(expected), "got", repr(got))

# --- Encode scalars ---
check("encode string", ujson.encode("hello"), '"hello"')
check("encode int", ujson.encode(42), "42")
check("encode float", ujson.encode(3.14), "3.14")
check("encode True", ujson.encode(True), "true")
check("encode False", ujson.encode(False), "false")
check("encode None", ujson.encode(None), "null")
check("encode zero", ujson.encode(0), "0")
check("encode negative", ujson.encode(-7), "-7")
check("encode empty string", ujson.encode(""), '""')

# --- Encode containers ---
check("encode list", ujson.encode([1, 2, 3]), "[1,2,3]")
check("encode empty list", ujson.encode([]), "[]")
check("encode nested list", ujson.encode([[1], [2]]), "[[1],[2]]")
check("encode dict", ujson.encode({"a": 1}), '{"a":1}')
check("encode empty dict", ujson.encode({}), "{}")
check("encode mixed list", ujson.encode([1, "two", None, True]), '[1,"two",null,true]')

# --- Decode scalars ---
check("decode string", ujson.decode('"world"'), "world")
check("decode int", ujson.decode("99"), 99)
check("decode float", ujson.decode("2.718"), 2.718)
check("decode true", ujson.decode("true"), True)
check("decode false", ujson.decode("false"), False)
check("decode null", ujson.decode("null"), None)
check("decode negative", ujson.decode("-42"), -42)
check("decode zero", ujson.decode("0"), 0)

# --- Decode containers ---
check("decode list", ujson.decode("[1,2,3]"), [1, 2, 3])
check("decode empty list", ujson.decode("[]"), [])
check("decode dict", ujson.decode('{"x":10}'), {"x": 10})
check("decode empty dict", ujson.decode("{}"), {})
check("decode nested", ujson.decode('{"a":[1,2]}'), {"a": [1, 2]})

# --- Round-trip ---
data = {"name": "rustthon", "version": 1, "features": ["ujson", "yaml"], "ok": True}
check("round-trip dict", ujson.decode(ujson.encode(data)), data)

data2 = [1, "hello", None, False, 3.14, [99]]
check("round-trip list", ujson.decode(ujson.encode(data2)), data2)

# --- Aliases ---
check("dumps alias", ujson.dumps("test"), '"test"')
check("loads alias", ujson.loads("123"), 123)

# --- Large values ---
check("encode large int", ujson.encode(999999999), "999999999")

# --- Summary ---
total = counts[0] + counts[1]
if counts[1] == 0:
    print("=== All", total, "ujson tests passed ===")
else:
    print("FAILED:", counts[1], "of", total, "ujson tests")
    raise Exception("ujson tests failed")
