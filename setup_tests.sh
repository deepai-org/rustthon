#!/bin/bash
#
# Rustthon Test Environment Setup
#
# Downloads and prepares all external dependencies needed by run_tests.sh.
# Run this once after cloning the repo.
#
# Prerequisites: Python 3.11, pip, cc (Xcode command line tools)
#
# Usage:
#   ./setup_tests.sh
#

set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

RED='\033[31m'
GREEN='\033[32m'
YELLOW='\033[33m'
BOLD='\033[1m'
RESET='\033[0m'

step() { printf "\n${BOLD}${YELLOW}── %s${RESET}\n" "$1"; }
ok()   { printf "  ${GREEN}✓ %s${RESET}\n" "$1"; }
fail() { printf "  ${RED}✗ %s${RESET}\n" "$1"; }

# ─── Step 1: Build librustthon.dylib ───
step "Building librustthon.dylib (release)"
cargo build --release 2>&1 | grep -E "Compiling|Finished|error" || true
if [ ! -f target/release/librustthon.dylib ]; then
    fail "cargo build failed"; exit 1
fi
ok "librustthon.dylib"

# ─── Step 2: Build the thin binary shim ───
step "Building rustthon binary shim"
cc -o rustthon csrc/main.c -ldl
cc -o rustthon_bin csrc/main.c -ldl
ok "rustthon + rustthon_bin"

# ─── Step 3: Create Python 3.11 venv with pip packages ───
step "Setting up .venv311 with pyyaml"
if [ ! -d .venv311 ]; then
    python3.11 -m venv .venv311
    ok "Created .venv311"
else
    ok ".venv311 already exists"
fi
.venv311/bin/pip install --quiet pyyaml==6.0.2 cython markupsafe 2>/dev/null || \
    .venv311/bin/pip install pyyaml cython markupsafe
ok "pyyaml + cython + markupsafe installed"

# ─── Step 4: Download and extract prebuilt wheels ───
step "Downloading prebuilt pip wheels"
PREBUILT_DIR="/tmp/prebuilt_ext"
mkdir -p "$PREBUILT_DIR"

# ujson prebuilt wheel
if [ ! -f "$PREBUILT_DIR/ujson.cpython-311-darwin.so" ]; then
    WHEEL_DIR=$(mktemp -d)
    .venv311/bin/pip download --quiet --no-deps --python-version 3.11 --platform macosx_11_0_arm64 \
        --only-binary :all: ujson==5.10.0 -d "$WHEEL_DIR" 2>/dev/null || \
    .venv311/bin/pip download --no-deps --python-version 3.11 --platform macosx_11_0_arm64 \
        --only-binary :all: "ujson>=5.0" -d "$WHEEL_DIR"
    WHEEL=$(ls "$WHEEL_DIR"/ujson*.whl 2>/dev/null | head -1)
    if [ -n "$WHEEL" ]; then
        unzip -o -q "$WHEEL" "ujson*.so" -d "$PREBUILT_DIR" 2>/dev/null || true
    fi
    rm -rf "$WHEEL_DIR"
fi
if [ -f "$PREBUILT_DIR/ujson.cpython-311-darwin.so" ]; then
    ok "ujson prebuilt .so"
else
    fail "ujson prebuilt .so (download manually)"
fi

# markupsafe prebuilt wheel
if [ ! -d "$PREBUILT_DIR/markupsafe" ]; then
    WHEEL_DIR=$(mktemp -d)
    .venv311/bin/pip download --quiet --no-deps --python-version 3.11 --platform macosx_10_12_universal2 \
        --only-binary :all: MarkupSafe==3.0.2 -d "$WHEEL_DIR" 2>/dev/null || \
    .venv311/bin/pip download --no-deps --python-version 3.11 --platform macosx_10_12_universal2 \
        --only-binary :all: "MarkupSafe>=3.0" -d "$WHEEL_DIR"
    WHEEL=$(ls "$WHEEL_DIR"/MarkupSafe*.whl 2>/dev/null | head -1)
    if [ -n "$WHEEL" ]; then
        unzip -o -q "$WHEEL" "markupsafe/*" -d "$PREBUILT_DIR" 2>/dev/null || true
    fi
    rm -rf "$WHEEL_DIR"
fi
if [ -d "$PREBUILT_DIR/markupsafe" ]; then
    ok "markupsafe prebuilt .so"
else
    fail "markupsafe prebuilt .so (download manually)"
fi

# Copy ujson to site-packages for VM native import tests
cp -f "$PREBUILT_DIR/ujson.cpython-311-darwin.so" .venv311/lib/python3.11/site-packages/ 2>/dev/null || true
ok "ujson .so copied to site-packages"

# ─── Step 5: Compile self-built extensions ───
step "Compiling self-built extensions (markupsafe, ujson)"
LINK="-L target/release -lrustthon -Wl,-rpath,target/release"

# markupsafe
if [ ! -f _markupsafe_speedups.dylib ]; then
    MARKUPSAFE_SRC=$(.venv311/bin/python3 -c "import markupsafe; import os; print(os.path.dirname(markupsafe.__file__))" 2>/dev/null || echo "")
    if [ -n "$MARKUPSAFE_SRC" ] && [ -f "$MARKUPSAFE_SRC/_speedups.c" ]; then
        cc -shared -fPIC -o _markupsafe_speedups.dylib \
            "$MARKUPSAFE_SRC/_speedups.c" \
            -I include $LINK 2>/dev/null && ok "markupsafe self-built" || fail "markupsafe self-built"
    else
        fail "markupsafe source not found (pip install markupsafe into .venv311)"
    fi
else
    ok "markupsafe self-built (already exists)"
fi

# ujson — requires ujson source which has C++ code, skip if complex
if [ -f _ujson.dylib ]; then
    ok "ujson self-built (already exists)"
else
    fail "ujson self-built (requires manual compilation — see readme)"
fi

# ─── Step 6: Compile Cython hello extension ───
step "Compiling Cython hello extension"
if [ ! -f test_cython/hello.cpython-311-darwin.so ]; then
    if [ -f test_cython/hello.c ]; then
        PYINC=$(.venv311/bin/python3 -c "import sysconfig; print(sysconfig.get_path('include'))" 2>/dev/null || echo "")
        cc -shared -fPIC -DCYTHON_COMPRESS_STRINGS=0 \
            -o test_cython/hello.cpython-311-darwin.so \
            test_cython/hello.c \
            ${PYINC:+-I "$PYINC"} -I include $LINK \
            2>/dev/null && ok "hello.cpython-311-darwin.so" || fail "hello.cpython-311-darwin.so"
    else
        fail "test_cython/hello.c not found"
    fi
else
    ok "hello.cpython-311-darwin.so (already exists)"
fi

# ─── Step 7: Extract pyyaml C extension ───
step "Setting up pyyaml C extension (_yaml.so)"
YAML_SO="/tmp/cython_wheels/extracted/pyyaml/yaml/_yaml.cpython-311-darwin.so"
if [ ! -f "$YAML_SO" ]; then
    mkdir -p /tmp/cython_wheels/extracted/pyyaml/yaml
    VENV_YAML=".venv311/lib/python3.11/site-packages/_yaml"
    # Try to find _yaml .so from the installed pyyaml
    YAML_REAL=$(find .venv311 -name "_yaml*.so" 2>/dev/null | head -1)
    if [ -n "$YAML_REAL" ]; then
        cp "$YAML_REAL" "$YAML_SO"
        ok "_yaml.so extracted"
    else
        # Download pyyaml wheel and extract
        WHEEL_DIR=$(mktemp -d)
        .venv311/bin/pip download --quiet --no-deps --python-version 3.11 --platform macosx_11_0_arm64 \
            --only-binary :all: PyYAML==6.0.2 -d "$WHEEL_DIR" 2>/dev/null || true
        WHEEL=$(ls "$WHEEL_DIR"/PyYAML*.whl 2>/dev/null | head -1)
        if [ -n "$WHEEL" ]; then
            unzip -o -q "$WHEEL" "*_yaml*" -d /tmp/cython_wheels/extracted/pyyaml/ 2>/dev/null || true
        fi
        rm -rf "$WHEEL_DIR"
        if [ -f "$YAML_SO" ]; then
            ok "_yaml.so extracted from wheel"
        else
            fail "_yaml.so (download pyyaml wheel manually)"
        fi
    fi
else
    ok "_yaml.so (already exists)"
fi

# ─── Step 8: bcrypt extension ───
step "Checking bcrypt extension"
if [ -f test_bcrypt/bcrypt_pkg/bcrypt/_bcrypt.abi3.so ]; then
    ok "_bcrypt.abi3.so (already exists)"
else
    mkdir -p test_bcrypt/bcrypt_pkg/bcrypt
    WHEEL_DIR=$(mktemp -d)
    .venv311/bin/pip download --quiet --no-deps --python-version 3.11 --platform macosx_10_12_universal2 \
        --only-binary :all: bcrypt -d "$WHEEL_DIR" 2>/dev/null || true
    WHEEL=$(ls "$WHEEL_DIR"/bcrypt*.whl 2>/dev/null | head -1)
    if [ -n "$WHEEL" ]; then
        unzip -o -q "$WHEEL" "bcrypt/*" -d test_bcrypt/bcrypt_pkg/ 2>/dev/null || true
    fi
    rm -rf "$WHEEL_DIR"
    if [ -f test_bcrypt/bcrypt_pkg/bcrypt/_bcrypt.abi3.so ]; then
        ok "_bcrypt.abi3.so extracted"
    else
        fail "_bcrypt.abi3.so (download manually)"
    fi
fi

# ─── Done ───
printf "\n${BOLD}${GREEN}Setup complete.${RESET}\n"
printf "Run ${BOLD}./run_tests.sh${RESET} to execute all 33 test suites.\n\n"
