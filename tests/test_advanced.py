# ─── Arithmetic stress test ───
print(2 ** 10)
print(100 // 7)
print(100 % 7)
print(-42)
print(3.14 * 2)

# ─── String operations ───
greeting = "Hello" + " " + "World"
print(greeting)

# ─── Nested conditionals ───
a = 15
if a > 20:
    print("big")
else:
    if a > 10:
        print("medium")
    else:
        print("small")

# ─── Complex while loop ───
total = 0
n = 1
while n <= 100:
    total = total + n
    n = n + 1
print(total)

# ─── List building with range ───
squares = []
i = 0
while i < 10:
    squares = squares + [i * i]
    i = i + 1
print(squares)

# ─── Tuple ───
point = (10, 20, 30)
print(point)

# ─── Dict ───
person = {"name": "Alice", "age": 30}
print(person)

# ─── Comparisons ───
print(1 < 2)
print(5 >= 5)
print("abc" == "abc")
print("abc" != "def")

# ─── Boolean logic ───
print(True and True)
print(True and False)
print(False or True)
print(not False)

# ─── Nested expressions ───
result = ((10 + 5) * 3 - 15) // 10
print(result)

# ─── Mixed types ───
print(len("hello"))
print(len([1, 2, 3]))
print(type(42))
print(type("hi"))
print(type(True))

print("Advanced tests passed!")
