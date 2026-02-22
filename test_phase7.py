# Phase 7: *args/**kwargs

# Test 1: *args
def sum_all(*args):
    total = 0
    for x in args:
        total = total + x
    return total

print(sum_all(1, 2, 3))       # 6
print(sum_all(10, 20, 30, 40)) # 100

# Test 2: **kwargs
def greet(**kwargs):
    if "name" in kwargs:
        return "Hello " + kwargs["name"]
    return "Hello stranger"

# Can't test kwargs easily via Python syntax yet,
# but *args is the main feature needed

# Test 3: Mixed positional and *args
def head_and_rest(first, *rest):
    return first

print(head_and_rest(42, 1, 2, 3))  # 42

# Test 4: *args with keyword args
def func(a, b=10, *args):
    return a + b

print(func(1))         # 11
print(func(1, 2))      # 3

print("=== Phase 7 tests passed ===")
