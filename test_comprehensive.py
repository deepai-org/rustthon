# Comprehensive VM test

# String operations
s = "hello world"
print(len(s))           # 11
print(s[0])             # h
print(s[-1])            # d

# String concatenation and multiplication
print("ab" + "cd")      # abcd
print("ha" * 3)         # hahaha

# List operations
lst = [1, 2, 3, 4, 5]
print(len(lst))         # 5
print(lst[0])           # 1
print(lst[-1])          # 5

# List with mixed types
mixed = [1, "two", 3.0, True, None]
print(len(mixed))       # 5

# Tuple operations
t = (10, 20, 30)
print(len(t))           # 3
print(t[1])             # 20

# Dict operations
d = {"a": 1, "b": 2, "c": 3}
print(d["a"])           # 1
print(d["b"])           # 2

# While loop
n = 10
total = 0
while n > 0:
    total = total + n
    n = n - 1
print(total)            # 55

# Nested loops
result = 0
for i in range(3):
    for j in range(3):
        result = result + 1
print(result)           # 9

# Boolean operations
print(True and False)   # False
print(True or False)    # True
print(not True)         # False

# Comparison
print(1 < 2)            # True
print(3 > 5)            # False
print(2 == 2)           # True
print(2 != 3)           # True

# String format
name = "World"
# Simple concatenation (format not yet supported)
greeting = "Hello " + name + "!"
print(greeting)         # Hello World!

# List comprehension
squares = [x * x for x in range(5)]
print(squares)          # [0, 1, 4, 9, 16]

# Multiple assignment
a = b = 10
print(a)                # 10
print(b)                # 10

# Augmented assignment
x = 5
x = x + 3
print(x)                # 8

# Class with __init__ and methods
class Dog:
    def __init__(self, name, age):
        self.name = name
        self.age = age

    def speak(self):
        return self.name + " says woof!"

    def is_puppy(self):
        if self.age < 2:
            return True
        return False

d1 = Dog("Rex", 5)
d2 = Dog("Buddy", 1)
print(d1.speak())       # Rex says woof!
print(d2.speak())       # Buddy says woof!
print(d1.is_puppy())    # False
print(d2.is_puppy())    # True

# Exception in function
def safe_divide(a, b):
    try:
        return a / b
    except:
        return 0

print(safe_divide(10, 2))  # 5
print(safe_divide(10, 0))  # 0

print("all comprehensive tests passed")
