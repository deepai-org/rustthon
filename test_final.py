# Final comprehensive test covering all VM features

# ─── Basic types ───
print(42)              # int
print(3.14)            # float
print("hello")         # str
print(True)            # bool
print(None)            # None
print([1, 2, 3])       # list
print((4, 5, 6))       # tuple

# ─── Arithmetic ───
print(10 + 3)          # 13
print(10 - 3)          # 7
print(10 * 3)          # 30
print(10 / 3)          # 3
print(10 // 3)         # 3
print(10 % 3)          # 1
print(2 ** 10)         # 1024
print(-5)              # -5

# ─── String operations ───
s = "Hello, World!"
print(len(s))          # 13
print(s[0])            # H
print(s[-1])           # !
print("Hello" + " " + "World")  # Hello World

# ─── List operations ───
lst = [10, 20, 30, 40, 50]
print(len(lst))        # 5
print(lst[2])          # 30
print(lst[-2])         # 40
lst[1] = 99
print(lst[1])          # 99

# ─── Dict operations ───
d = {"name": "Alice", "age": 30}
print(d["name"])       # Alice
d["city"] = "NYC"
print(d["city"])       # NYC

# ─── Control flow ───
x = 10
if x > 5:
    print("big")       # big
elif x > 0:
    print("small")
else:
    print("zero or neg")

# While loop
n = 5
fact = 1
while n > 1:
    fact = fact * n
    n = n - 1
print(fact)            # 120

# For loop with range
total = 0
for i in range(10):
    total += i
print(total)           # 45

# For with list
words = ["foo", "bar", "baz"]
for w in words:
    print(w)

# ─── Functions ───
def fib(n):
    if n <= 1:
        return n
    return fib(n - 1) + fib(n - 2)

print(fib(10))         # 55

# Default args
def power(base, exp=2):
    result = 1
    for i in range(exp):
        result = result * base
    return result

print(power(3))        # 9
print(power(2, 10))    # 1024

# ─── Nested functions ───
def outer(x):
    def inner(y):
        return x + y
    return inner

f = outer(10)
print(f(5))            # 15

# ─── Classes ───
class Animal:
    def __init__(self, name, sound):
        self.name = name
        self.sound = sound

    def speak(self):
        return self.name + " says " + self.sound

cat = Animal("Cat", "meow")
dog = Animal("Dog", "woof")
print(cat.speak())     # Cat says meow
print(dog.speak())     # Dog says woof

# ─── Exceptions ───
def safe_div(a, b):
    try:
        return a / b
    except:
        return -1

print(safe_div(10, 2))  # 5
print(safe_div(10, 0))  # -1

# ─── List comprehension ───
evens = [x for x in range(10) if x % 2 == 0]
print(evens)            # [0, 2, 4, 6, 8]

# ─── Imports ───
import mylib
print(mylib.add(100, 200))  # 300

# ─── in operator ───
print("abc" in "xabcy")     # True
print(3 in [1, 2, 3, 4])    # True
print(5 in [1, 2, 3, 4])    # False

# ─── Truthiness ───
print(bool(0))          # False
print(bool(1))          # True
print(bool(""))         # False
print(bool("hi"))       # True
print(bool([]))         # False
print(bool([1]))        # True

# ─── Augmented assignment ───
x = 10
x += 5
print(x)               # 15
x -= 3
print(x)               # 12
x *= 2
print(x)               # 24

# ─── isinstance ───
print(isinstance(42, int))     # True
print(isinstance("hi", str))   # True
print(isinstance(3.14, float)) # True

print("=== ALL TESTS PASSED ===")
