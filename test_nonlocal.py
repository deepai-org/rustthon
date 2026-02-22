# Test mutable closures (nonlocal)

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

print("=== nonlocal tests passed ===")
