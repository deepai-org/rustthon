# Debug: simple raise inside except
try:
    try:
        x = 1 / 0
    except:
        print("inner caught")
        raise ValueError("re-raised")
except:
    print("outer caught")
