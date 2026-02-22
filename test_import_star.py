"""Test from X import *"""
results = [0, 0]

def test(name, condition):
    if condition:
        results[0] = results[0] + 1
    else:
        results[1] = results[1] + 1
        print("FAIL: " + name)

# Test 1: from module import * with __all__
from test_import_star_helper import *

test("public_a imported", public_a == 42)
test("public_b imported", public_b == "hello")

# Test 2: from os import * (no __all__, filters _)
from os import *

test("sep imported", sep == "/")
test("linesep imported", linesep == "\n")
test("name imported", name == "posix")

# Summary
print("")
total = results[0] + results[1]
print(str(results[0]) + "/" + str(total) + " import* tests passed")
if results[1] > 0:
    print(str(results[1]) + " FAILED")
else:
    print("All tests passed!")
