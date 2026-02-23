# Comprehensive yaml.safe_load tests
import yaml

# Test 1: Simple key-value
r = yaml.safe_load("hello: world")
print("1:", r)

# Test 2: Multiple key-values
r = yaml.safe_load("a: 1\nb: 2\nc: 3")
print("2:", r)

# Test 3: Simple scalar
r = yaml.safe_load("hello")
print("3:", r)

# Test 4: Integer
r = yaml.safe_load("42")
print("4:", r)

# Test 5: Boolean
r = yaml.safe_load("true")
print("5:", r)

# Test 6: Null
r = yaml.safe_load("null")
print("6:", r)

# Test 7: List
r = yaml.safe_load("- a\n- b\n- c")
print("7:", r)

# Test 8: Nested dict
r = yaml.safe_load("outer:\n  inner: value")
print("8:", r)

# Test 9: Float
r = yaml.safe_load("3.14")
print("9:", r)

# Test 10: Empty doc
r = yaml.safe_load("")
print("10:", r)

print("done")
