# Comprehensive test covering Phases 6-10

# ─── Closures ───
def make_counter(start=0):
    count = start
    def increment(n=1):
        nonlocal count
        count = count + n
        return count
    return increment

inc = make_counter(10)
print(inc())     # 11
print(inc(5))    # 16

# ─── Decorators ───
def log_call(func):
    def wrapper(a, b):
        return func(a, b)
    return wrapper

@log_call
def add(a, b):
    return a + b

print(add(3, 4))  # 7

# ─── String methods ───
s = "Hello, World!"
print(s[0:5])              # Hello
print(s.upper())           # HELLO, WORLD!
print(s.lower())           # hello, world!
print(s.replace("World", "Python"))  # Hello, Python!
print(s.find("World"))     # 7
print(s.startswith("Hello"))  # True
print(s.endswith("!"))     # True
print(len(s.split(",")))   # 2

# ─── String formatting ───
name = "Alice"
age = 30
print("Name: %s, Age: %d" % (name, age))  # Name: Alice, Age: 30
print("ha" * 3)  # hahaha

# ─── List methods ───
lst = [3, 1, 4, 1, 5, 9]
lst.append(2)
print(len(lst))  # 7
lst.sort()
print(lst)  # [1, 1, 2, 3, 4, 5, 9]
lst.reverse()
print(lst[0])  # 9

# ─── Dict methods ───
d = {"a": 1, "b": 2, "c": 3}
print(d.get("a"))     # 1
print(d.get("z", 99)) # 99

# ─── Comprehensions ───
squares = [x*x for x in range(6)]
print(squares)  # [0, 1, 4, 9, 16, 25]

# Nested comprehension
flat = [x for row in [[1,2],[3,4],[5,6]] for x in row]
print(flat)  # [1, 2, 3, 4, 5, 6]

# ─── Builtins ───
print(abs(-42))        # 42
print(min(5, 3, 8, 1)) # 1
print(max(5, 3, 8, 1)) # 8
print(sum([1,2,3,4,5])) # 15
print(ord("A"))         # 65
print(chr(97))          # a

# ─── Slicing ───
lst2 = [10, 20, 30, 40, 50]
print(lst2[1:3])   # [20, 30]
print(lst2[:2])    # [10, 20]
print(lst2[3:])    # [40, 50]
print(lst2[-2:])   # [40, 50]

t = (1, 2, 3, 4, 5)
print(t[1:4])   # (2, 3, 4)

print("=== Comprehensive Phase 6-10 tests passed ===")
