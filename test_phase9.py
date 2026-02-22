# Phase 9: Generators

# Test 1: Simple generator
def count_up(n):
    i = 0
    while i < n:
        yield i
        i = i + 1

for x in count_up(5):
    print(x)  # 0 1 2 3 4

# Test 2: Generator with list conversion
def squares_gen(n):
    for i in range(n):
        yield i * i

result = list(squares_gen(5))
print(result)  # [0, 1, 4, 9, 16]

# Test 3: Generator as filter
def evens(iterable):
    for x in iterable:
        if x % 2 == 0:
            yield x

result = list(evens(range(10)))
print(result)  # [0, 2, 4, 6, 8]

print("=== Phase 9 tests passed ===")
