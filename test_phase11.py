# Phase 11: Standard Library Stubs & Additional Builtins

# --- Additional builtins ---

# enumerate
result = []
for i, v in enumerate(["a", "b", "c"]):
    result.append((i, v))
print(result)  # [(0, 'a'), (1, 'b'), (2, 'c')]

# zip
pairs = list(zip([1, 2, 3], ["a", "b", "c"]))
print(pairs)  # [(1, 'a'), (2, 'b'), (3, 'c')]

# sorted
print(sorted([3, 1, 4, 1, 5]))  # [1, 1, 3, 4, 5]

# reversed
print(list(reversed([1, 2, 3])))  # [3, 2, 1]

# hasattr / getattr / setattr
class Obj:
    def __init__(self):
        self.x = 10

o = Obj()
print(hasattr(o, "x"))     # True
print(hasattr(o, "y"))     # False
print(getattr(o, "x"))     # 10
print(getattr(o, "y", 42)) # 42

# repr
print(repr(42))       # 42
print(repr("hello"))  # 'hello'
print(repr([1, 2]))   # [1, 2]

# id (just check it returns an int)
x = [1, 2, 3]
print(id(x) > 0)  # True

# hex, oct, bin
print(hex(255))    # 0xff
print(oct(8))      # 0o10
print(bin(10))     # 0b1010

# --- sys module ---
import sys
print(sys.platform)         # darwin
print(len(sys.path) >= 0)   # True
print(sys.maxsize > 0)      # True

# --- os module ---
import os
print(os.sep)               # /
print(os.getcwd() != "")    # True

# --- os.path module ---
import os.path
print(os.path.join("a", "b"))  # a/b
print(os.path.exists("."))     # True

print("=== Phase 11 tests passed ===")
