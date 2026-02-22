"""Test suite for the 5 VM improvements."""

results = [0, 0]  # [passed, failed]

def test(name, condition):
    if condition:
        results[0] = results[0] + 1
    else:
        results[1] = results[1] + 1
        print("FAIL: " + name)

# ─── 2. Multiple Inheritance ───
print("=== Multiple Inheritance ===")

class Animal:
    def speak(self):
        return "..."
    def breathe(self):
        return "inhale-exhale"

class Swimmer:
    def swim(self):
        return "splash"
    def breathe(self):
        return "bubble-bubble"

class Duck(Animal, Swimmer):
    def speak(self):
        return "quack"

d = Duck()
test("MI: derived method override", d.speak() == "quack")
test("MI: inherited from first base (breathe)", d.breathe() == "inhale-exhale")
test("MI: inherited from second base (swim)", d.swim() == "splash")

class Base1:
    x = 10
    def method_a(self):
        return "a"

class Base2:
    y = 20
    def method_b(self):
        return "b"

class Child(Base1, Base2):
    z = 30

c = Child()
test("MI: class var from Base1", c.x == 10)
test("MI: class var from Base2", c.y == 20)
test("MI: own class var", c.z == 30)
test("MI: method from Base1", c.method_a() == "a")
test("MI: method from Base2", c.method_b() == "b")

# Single inheritance still works
class Parent:
    def greet(self):
        return "hello"

class Kid(Parent):
    pass

k = Kid()
test("SI: inherited method", k.greet() == "hello")

# ─── 3. isinstance with tuples ───
print("=== isinstance with tuples ===")

test("isinstance(1, int)", isinstance(1, int))
test("isinstance('a', str)", isinstance("a", str))
test("isinstance(1, (int, str))", isinstance(1, (int, str)))
test("isinstance('a', (int, str))", isinstance("a", (int, str)))
test("not isinstance(1.0, (int, str))", not isinstance(1.0, (int, str)))
test("isinstance([], (list, tuple))", isinstance([], (list, tuple)))
test("isinstance((), (list, tuple))", isinstance((), (list, tuple)))
test("not isinstance({}, (list, tuple))", not isinstance({}, (list, tuple)))
test("isinstance(True, (bool, int))", isinstance(True, (bool, int)))

# RustClass isinstance
test("isinstance with class", isinstance(d, Duck))
test("isinstance with base class", isinstance(d, Animal))
test("isinstance with second base", isinstance(d, Swimmer))
test("not isinstance unrelated", not isinstance(c, Duck))

# ─── 4. Dict/Set Printing ───
print("=== Dict/Set Printing ===")

d1 = {"a": 1, "b": 2}
s1 = str(d1)
# Check dict prints with curly braces and colon
test("dict has curly braces", s1[0] == "{")
test("dict contains colon", ":" in s1)
test("dict contains 'a'", "'a'" in s1)

# Nested structures
nested = {"key": [1, 2, 3]}
s2 = str(nested)
test("nested dict+list", "[1, 2, 3]" in s2)

# Empty dict
empty = {}
test("empty dict repr", str(empty) == "{}")

# ─── 5. re module ───
print("=== re module ===")

import re

# Constants
test("re.IGNORECASE", re.IGNORECASE == 2)
test("re.MULTILINE", re.MULTILINE == 8)
test("re.DOTALL", re.DOTALL == 16)

# re.search
m = re.search(r"\d+", "abc123def")
test("re.search found", m is not None)
test("re.search group(0)", m.group(0) == "123")
test("re.search start()", m.start() == 3)
test("re.search end()", m.end() == 6)

# re.search no match
m2 = re.search(r"\d+", "abcdef")
test("re.search no match", m2 is None)

# re.match
m3 = re.match(r"\d+", "123abc")
test("re.match at start", m3 is not None)
test("re.match group", m3.group(0) == "123")

m4 = re.match(r"\d+", "abc123")
test("re.match not at start", m4 is None)

# re.findall
found = re.findall(r"\d+", "a1b22c333")
test("findall count", len(found) == 3)
test("findall[0]", found[0] == "1")
test("findall[1]", found[1] == "22")
test("findall[2]", found[2] == "333")

# re.sub
result = re.sub(r"\d+", "X", "a1b2c3")
test("re.sub", result == "aXbXcX")

# re.split
parts = re.split(r"[,;]", "a,b;c,d")
test("re.split count", len(parts) == 4)
test("re.split[0]", parts[0] == "a")
test("re.split[3]", parts[3] == "d")

# re.compile
pat = re.compile(r"(\w+)@(\w+)")
m5 = pat.search("user@host")
test("compiled search", m5 is not None)
test("compiled group(0)", m5.group(0) == "user@host")
test("compiled group(1)", m5.group(1) == "user")
test("compiled group(2)", m5.group(2) == "host")

# Pattern .groups()
test("match.groups()", m5.groups() == ("user", "host"))

# Pattern .span()
span = m5.span()
test("match.span()", span == (0, 9))

# Compiled .findall
results2 = pat.findall("a@b c@d")
test("compiled findall", len(results2) == 2)

# Compiled .sub
result2 = pat.sub("REDACTED", "user@host other@place")
test("compiled sub", result2 == "REDACTED REDACTED")

# Compiled .pattern property
test("compiled pattern", pat.pattern == r"(\w+)@(\w+)")

# re.findall with groups returns group contents
results3 = re.findall(r"(\d+)-(\d+)", "12-34 56-78")
test("findall with groups", len(results3) == 2)

# re.escape
escaped = re.escape("hello.world+foo")
test("re.escape dots", "\\." in escaped)

# re.fullmatch
m6 = re.fullmatch(r"\d+", "123")
test("fullmatch match", m6 is not None)
m7 = re.fullmatch(r"\d+", "123abc")
test("fullmatch no match", m7 is None)

# ─── Summary ───
print("")
print("=" * 40)
total = results[0] + results[1]
print(str(results[0]) + "/" + str(total) + " tests passed")
if results[1] > 0:
    print(str(results[1]) + " FAILED")
else:
    print("All tests passed!")
