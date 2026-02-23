# Test generator isinstance check
import types

def gen_func():
    yield 42
    yield 99

g = gen_func()
print("1: generator:", g)
print("2: type(g):", type(g))
print("3: types.GeneratorType:", types.GeneratorType)
print("4: isinstance check:", isinstance(g, types.GeneratorType))
print("5: next(g):", next(g))
print("6: next(g):", next(g))
