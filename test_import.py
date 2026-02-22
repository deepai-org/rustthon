# Test Python source imports
import mylib

print(mylib.add(3, 4))     # 7
print(mylib.greet("World")) # Hello World
print(mylib.PI)             # 3

# Test from...import
from mylib import add, greet
print(add(10, 20))          # 30
print(greet("Python"))      # Hello Python

print("all import tests passed")
