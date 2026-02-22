# Phase 6: Closures and Decorators

# --- Test 1: Simple decorator ---
def double_result(func):
    def wrapper(x):
        return func(x) * 2
    return wrapper

@double_result
def square(x):
    return x * x

print(square(3))  # 18 (3*3=9, *2=18)

# --- Test 2: Read-only closure ---
def make_adder(n):
    def adder(x):
        return x + n
    return adder

add5 = make_adder(5)
print(add5(10))  # 15

# --- Test 3: Closure over loop variable ---
def make_multiplier(factor):
    def multiply(x):
        return x * factor
    return multiply

double = make_multiplier(2)
triple = make_multiplier(3)
print(double(7))   # 14
print(triple(7))   # 21

# --- Test 4: Decorator with arguments (decorator factory) ---
def repeat(n):
    def decorator(func):
        def wrapper(x):
            result = ""
            for i in range(n):
                result = result + func(x)
            return result
        return wrapper
    return decorator

@repeat(3)
def greet(name):
    return "Hi " + name + "! "

print(greet("Bob"))  # Hi Bob! Hi Bob! Hi Bob!

# --- Test 5: Multiple decorators ---
def add_exclaim(func):
    def wrapper(x):
        return func(x) + "!"
    return wrapper

def add_greeting(func):
    def wrapper(x):
        return "Hello, " + func(x)
    return wrapper

@add_greeting
@add_exclaim
def get_name(name):
    return name

print(get_name("Alice"))  # Hello, Alice!

# --- Test 6: Nested closures (3 levels) ---
def level1(a):
    def level2(b):
        def level3(c):
            return a + b + c
        return level3
    return level2

print(level1(1)(2)(3))  # 6

# --- Test 7: Mutable closure (nonlocal) ---
def make_counter():
    count = 0
    def increment():
        nonlocal count
        count = count + 1
        return count
    return increment

c = make_counter()
print(c())  # 1
print(c())  # 2
print(c())  # 3

# Independent counters
c2 = make_counter()
print(c2())  # 1
print(c())   # 4

# --- Test 8: Nonlocal with accumulator ---
def make_accumulator():
    total = 0
    def add(n):
        nonlocal total
        total = total + n
        return total
    return add

acc = make_accumulator()
print(acc(10))  # 10
print(acc(20))  # 30
print(acc(5))   # 35

# --- Test 9: Closure capturing multiple variables ---
def make_linear(slope, intercept):
    def f(x):
        return slope * x + intercept
    return f

line = make_linear(3, 7)
print(line(0))   # 7
print(line(2))   # 13
print(line(10))  # 37

print("=== Phase 6 tests passed ===")
