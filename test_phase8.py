# Phase 8: Comprehensions

# Test 1: Basic list comprehension
squares = [x * x for x in range(5)]
print(squares)  # [0, 1, 4, 9, 16]

# Test 2: List comprehension with filter
evens = [x for x in range(10) if x % 2 == 0]
print(evens)  # [0, 2, 4, 6, 8]

# Test 3: String processing via list comp
words = ["hello", "world", "foo"]
lengths = [len(w) for w in words]
print(lengths)  # [5, 5, 3]

# Test 4: Comprehension with function call
def double(x):
    return x * 2

doubled = [double(x) for x in range(5)]
print(doubled)  # [0, 2, 4, 6, 8]

# Test 5: Nested comprehension
matrix = [[1, 2, 3], [4, 5, 6], [7, 8, 9]]
flat = [x for row in matrix for x in row]
print(flat)  # [1, 2, 3, 4, 5, 6, 7, 8, 9]

print("=== Phase 8 tests passed ===")
