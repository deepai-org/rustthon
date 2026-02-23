# Test 1: import the simplest yaml submodule first
from yaml.error import YAMLError, Mark
print("Test 1 PASSED: from yaml.error import YAMLError, Mark")

# Test 2: import tokens
from yaml.tokens import Token
print("Test 2 PASSED: from yaml.tokens import Token")

# Test 3: import events
from yaml.events import Event
print("Test 3 PASSED: from yaml.events import Event")

# Test 4: import nodes
from yaml.nodes import Node
print("Test 4 PASSED: from yaml.nodes import Node")

# Test 5: import the full yaml package
import yaml
print("Test 5 PASSED: import yaml")
print("__with_libyaml__:", yaml.__with_libyaml__)
