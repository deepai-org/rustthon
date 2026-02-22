# Advanced VM tests

# Test 'in' operator
print(1 in [1, 2, 3])     # True
print(4 in [1, 2, 3])     # False
print("h" in "hello")      # True
print("z" in "hello")      # False

# Test 'not in'
print(4 not in [1, 2, 3])  # True

# Test while with continue
total = 0
i = 0
while i < 10:
    i = i + 1
    if i % 2 == 0:
        continue
    total = total + i
print(total)               # 25 (1+3+5+7+9)

# Test nested function with closure-like behavior
def make_adder(n):
    def adder(x):
        return x + n
    return adder

add5 = make_adder(5)
print(add5(10))            # 15

# Test global-scope variable passing
VALUE = 42
def get_value():
    return VALUE
print(get_value())         # 42

# Test multiple return values (tuple)
def swap(a, b):
    return b, a

result = swap(1, 2)
print(result)              # (2, 1)

# Test unpack
x, y = swap(10, 20)
print(x)                   # 20
print(y)                   # 10

# Test isinstance (basic, will be limited)
print(isinstance(42, int))     # True

# Test dict comprehension
# d = {x: x*x for x in range(4)}
# print(d)

# Test for/else
for i in range(5):
    if i == 10:
        break
else:
    print("loop completed")  # loop completed

# Test None comparisons
x = None
print(x is None)             # True
print(x is not None)         # False

print("all advanced tests passed")
