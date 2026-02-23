# Test native import of prebuilt ujson C extension from VM Python source
import ujson

# encode
print(ujson.encode("hello"))
print(ujson.encode(42))
print(ujson.encode(3.14))
print(ujson.encode(True))
print(ujson.encode(False))
print(ujson.encode(None))

# decode
print(ujson.decode('"world"'))
print(ujson.decode("99"))

# aliases
print(ujson.dumps("test"))

print("=== native import tests passed ===")
