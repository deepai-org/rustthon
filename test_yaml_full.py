# Incremental yaml functional tests — find where things break

import yaml
print("1: import yaml OK, __with_libyaml__:", yaml.__with_libyaml__)

# Test: direct CParser creation (same as C test driver does)
from yaml._yaml import CParser
print("2: imported CParser:", CParser)

parser = CParser("hello: world\n")
print("3: CParser('hello: world') created:", parser)

event = parser.get_event()
print("4: first event:", event)
