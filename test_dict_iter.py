# Test dict iteration
d = {0: "zero", 1: "one", 2: "two"}
print("1: dict:", d)

print("2: iterating with for key in d:")
for key in d:
    print("  key:", key, "val:", d[key])

print("3: iterating with for key in d.keys():")
for key in d.keys():
    print("  key:", key)

# Test with string keys
d2 = {"a": 1, "b": 2}
print("4: string dict:", d2)
for key in d2:
    print("  key:", key)

# Test with int keys specifically
d3 = {}
d3[0] = "hello"
print("5: d3:", d3)
for key in d3:
    print("  key:", key)

print("6: done")
