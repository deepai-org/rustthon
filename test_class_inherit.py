class A:
    x = 42
    def hello(self):
        return "hello"

class B(A):
    y = 99

print("B.x:", B.x)
print("B.y:", B.y)
