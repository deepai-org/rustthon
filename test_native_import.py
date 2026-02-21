import ujson

# Test native import: ujson loaded from prebuilt .so via VM's import machinery

# encode string
print(ujson.encode("hello"))

# encode int
print(ujson.encode(42))

# encode float
print(ujson.encode(3.14))

# encode bools
print(ujson.encode(True))
print(ujson.encode(False))

# encode None
print(ujson.encode(None))

# decode
print(ujson.decode('"world"'))
print(ujson.decode("99"))

# aliases
print(ujson.dumps("test"))
