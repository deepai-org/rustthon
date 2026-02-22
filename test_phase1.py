# Phase 1: Functions test
def add(a, b):
    return a + b

print(add(3, 4))

def greet(name, greeting="Hello"):
    return greeting + " " + name

print(greet("World"))
print(greet("World", "Hi"))
print(add(add(1, 2), add(3, 4)))

# Phase 2: For loops test
total = 0
for i in range(5):
    total = total + i
print(total)

for x in [10, 20, 30]:
    print(x)

# Nested function calls
def square(x):
    return x * x

def sum_squares(n):
    result = 0
    for i in range(n):
        result = result + square(i)
    return result

print(sum_squares(5))

# Break and continue
found = 0
for i in range(10):
    if i == 7:
        found = i
        break
print(found)
