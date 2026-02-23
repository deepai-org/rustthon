# Test super() basic functionality
class A:
    def greet(self):
        return "hello from A"

    def info(self):
        return "info from A"

class B(A):
    def greet(self):
        base = super().greet()
        return base + " and B"

class C(B):
    def greet(self):
        base = super().greet()
        return base + " and C"

# Test
a = A()
print("1:", a.greet())

b = B()
print("2:", b.greet())

c = C()
print("3:", c.greet())

print("4:", b.info())
print("5:", c.info())
