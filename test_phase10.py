# Phase 10: String and List operations

# --- String slicing ---
s = "Hello, World!"
print(s[0:5])      # Hello
print(s[7:])        # World!
print(s[:5])        # Hello
print(s[-6:])       # orld!

# --- String methods ---
print("hello".upper())      # HELLO
print("HELLO".lower())      # hello
print("  hi  ".strip())     # hi
print("hello world".split(" "))  # ['hello', 'world']
print(",".join(["a", "b", "c"]))  # a,b,c
print("hello".startswith("hel"))  # True
print("hello".endswith("llo"))    # True
print("hello".replace("l", "r"))  # herro
print("hello world".find("world")) # 6

# --- String formatting ---
print("x=%d" % 42)           # x=42
print("%s is %d" % ("age", 25))  # age is 25

# --- String multiplication ---
print("ha" * 3)              # hahaha

# --- List methods ---
lst = [3, 1, 4, 1, 5]
lst.append(9)
print(lst)                   # [3, 1, 4, 1, 5, 9]

# --- Builtins ---
print(abs(-5))               # 5
print(min(3, 1, 4))          # 1
print(max(3, 1, 4))          # 4
print(sum([1, 2, 3, 4]))     # 10
print(ord("A"))              # 65
print(chr(65))               # A

print("=== Phase 10 tests passed ===")
