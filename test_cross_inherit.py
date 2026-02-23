from yaml.error import YAMLError
print("YAMLError type:", type(YAMLError))

# Define a class that uses YAMLError (imported from another module) as base
class RepresenterError(YAMLError):
    pass

print("RepresenterError OK")

# Now test method inheritance
class Base:
    def greet(self):
        return "hello"

class Child(Base):
    pass

Child.greet(None)
print("Child.greet(None) OK:", Child.greet)
