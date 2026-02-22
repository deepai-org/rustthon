# Phase 4: Class Definitions tests

class Point:
    def __init__(self, x, y):
        self.x = x
        self.y = y

    def magnitude(self):
        return self.x * self.x + self.y * self.y

p = Point(3, 4)
print(p.x)              # 3
print(p.y)              # 4
print(p.magnitude())    # 25

# Test multiple instances
p2 = Point(1, 2)
print(p2.x)             # 1
print(p2.magnitude())   # 5

# Test class with method calling another method
class Counter:
    def __init__(self):
        self.count = 0

    def inc(self):
        self.count = self.count + 1

    def get(self):
        return self.count

c = Counter()
c.inc()
c.inc()
c.inc()
print(c.get())          # 3

print("all class tests passed")
