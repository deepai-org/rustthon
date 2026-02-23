print("1: testing import collections.abc...")
try:
    import collections.abc
    print("2: imported:", collections.abc)
except Exception as e:
    print("2: failed:", e)

print("3: testing from collections.abc import Hashable...")
try:
    from collections.abc import Hashable
    print("4: Hashable:", Hashable)
except Exception as e:
    print("4: failed:", e)

print("5: done")
