# hello.pyx - Simple Cython hello world
def greet(name):
    """Return a greeting string."""
    return f"Hello, {name}! From Cython."

def add(int a, int b):
    """Add two integers with C-level speed."""
    return a + b

def fibonacci(int n):
    """Compute nth fibonacci number."""
    cdef int a = 0, b = 1, i
    for i in range(n):
        a, b = b, a + b
    return a
