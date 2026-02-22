# Phase 3: Exception Handling tests

# Test 1: Basic try/except
try:
    x = 1 / 0
except:
    print("caught division by zero")

# Test 2: try/except with else
try:
    x = 42
except:
    print("ERROR: should not get here")
else:
    print("no exception, x =", x)

# Test 3: try/finally
result = "before"
try:
    result = "inside try"
finally:
    print("finally ran, result =", result)

# Test 4: Raise and catch
try:
    raise ValueError("bad value")
except:
    print("caught ValueError")

# Test 5: Nested try/except
try:
    try:
        x = 1 / 0
    except:
        print("inner caught")
        raise ValueError("re-raised")
except:
    print("outer caught")

# Test 6: try/except/finally
try:
    x = 10
except:
    print("ERROR: should not catch")
finally:
    print("finally after no exception")

# Test 7: try with exception in body + finally
try:
    x = 1 / 0
except:
    print("caught in except")
finally:
    print("finally after except")

print("all exception tests passed")
