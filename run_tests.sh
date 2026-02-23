#!/bin/bash
#
# Rustthon Comprehensive Test Suite
#
# Builds librustthon.dylib, compiles all test drivers, and runs every test
# phase. Exit code 0 = all pass, 1 = some failures.
#
# Usage:
#   ./run_tests.sh          # run all tests
#   ./run_tests.sh --quick  # skip cargo build (assume already built)
#

set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

# ── Colors ──
RED='\033[31m'
GREEN='\033[32m'
YELLOW='\033[33m'
CYAN='\033[36m'
BOLD='\033[1m'
RESET='\033[0m'

TOTAL_SUITES=0
PASSED_SUITES=0
FAILED_SUITES=0
FAILED_NAMES=""

run_suite() {
    local name="$1"
    local binary="$2"
    shift 2
    local env_vars=("$@")

    TOTAL_SUITES=$((TOTAL_SUITES + 1))
    printf "${CYAN}${BOLD}[%d] %s${RESET}\n" "$TOTAL_SUITES" "$name"

    # Run the test binary, capturing output
    local output
    local exit_code=0
    if [ ${#env_vars[@]} -gt 0 ]; then
        output=$(env "${env_vars[@]}" "$binary" 2>&1) || exit_code=$?
    else
        output=$("$binary" 2>&1) || exit_code=$?
    fi

    # Extract the results line (look for PASS/FAIL counts)
    local results
    results=$(echo "$output" | grep -iE '(results:|Total:|PASS|ALL.*PASS)' | tail -3 || true)

    if [ $exit_code -eq 0 ]; then
        PASSED_SUITES=$((PASSED_SUITES + 1))
        # Show condensed results
        echo "$results" | while IFS= read -r line; do
            printf "    %s\n" "$line"
        done
        printf "    ${GREEN}SUITE PASSED${RESET}\n\n"
    else
        FAILED_SUITES=$((FAILED_SUITES + 1))
        FAILED_NAMES="$FAILED_NAMES  - $name\n"
        # Show full output on failure
        echo "$output" | tail -20
        printf "    ${RED}SUITE FAILED (exit code %d)${RESET}\n\n" "$exit_code"
    fi
}

# ═══════════════════════════════════════════════════════════
printf "${BOLD}═══════════════════════════════════════════════════════════${RESET}\n"
printf "${BOLD}  Rustthon Test Suite${RESET}\n"
printf "${BOLD}═══════════════════════════════════════════════════════════${RESET}\n\n"

# ── Step 0: Build librustthon.dylib ──
if [ "${1:-}" != "--quick" ]; then
    printf "${YELLOW}Building librustthon.dylib (release)...${RESET}\n"
    cargo build --release 2>&1 | grep -E "Compiling|Finished|error" || true
    if [ ! -f target/release/librustthon.dylib ]; then
        printf "${RED}FATAL: cargo build failed — no librustthon.dylib${RESET}\n"
        exit 1
    fi
    printf "${GREEN}Build complete.${RESET}\n\n"
else
    printf "${YELLOW}Skipping build (--quick mode)${RESET}\n\n"
fi

# ── Step 1: Compile all test drivers ──
printf "${YELLOW}Compiling test drivers...${RESET}\n"

LINK_FLAGS="-L target/release -lrustthon -Wl,-rpath,target/release"

compile() {
    local src="$1"
    local out="$2"
    shift 2
    local extra_flags="$*"
    printf "  %-45s" "$src -> $out"
    if cc -o "$out" "$src" $extra_flags 2>/dev/null; then
        printf "${GREEN}OK${RESET}\n"
    else
        printf "${RED}FAIL${RESET}\n"
        return 1
    fi
}

# Phase 1: ABI
compile tests/test_abi.c           test_abi           $LINK_FLAGS

# Phase 2: GC torture
compile tests/test_gc_torture.c    test_gc_torture    $LINK_FLAGS

# Phase 3: C extension module
cc -shared -fPIC -o _testmod.dylib tests/test_ext.c $LINK_FLAGS 2>/dev/null || true
compile tests/test_ext_driver.c    test_ext_driver    $LINK_FLAGS

# Phase 3a: markupsafe (compiled against Rustthon)
compile tests/test_markupsafe.c    test_markupsafe    $LINK_FLAGS

# Phase 3b: ujson (compiled against Rustthon)
compile tests/test_ujson.c         test_ujson         $LINK_FLAGS

# Phase 4: Prebuilt CPython 3.11 wheels
compile tests/test_prebuilt.c      test_prebuilt      "-ldl"

# Phase 4.5: Cython
compile test_cython/test_cython.c  test_cython_bin    "-ldl"

# Phase 5: PyO3 bcrypt
compile test_bcrypt/test_bcrypt.c  test_bcrypt_bin    "-ldl"

# Phase 6: pyyaml (Cython)
compile test_pyyaml/test_pyyaml.c test_pyyaml_bin    "-ldl"

# Thin binary shim (for native import tests)
compile csrc/main.c               rustthon_bin       "-ldl"

printf "\n"

# ── Step 2: Run all test suites ──
printf "${BOLD}Running tests...${RESET}\n\n"

# Phase 1: ABI layout validation
run_suite "Phase 1: ABI Struct Layouts" ./test_abi

# Phase 2: GC & Memory
run_suite "Phase 2: GC & Memory Torture" ./test_gc_torture

# Phase 3: C extension module
run_suite "Phase 3: Custom C Extension Module" ./test_ext_driver

# Phase 3a: markupsafe (compiled against Rustthon headers)
run_suite "Phase 3a: markupsafe (Rustthon-compiled)" ./test_markupsafe

# Phase 3b: ujson (compiled against Rustthon headers) — requires manually compiled _ujson.dylib
if [ -f _ujson.dylib ]; then
    run_suite "Phase 3b: ujson (Rustthon-compiled)" ./test_ujson
else
    printf "${YELLOW}[--] Phase 3b: ujson (Rustthon-compiled) — SKIPPED (no _ujson.dylib)${RESET}\n\n"
fi

# Phase 4: Prebuilt CPython 3.11 wheels
run_suite "Phase 4: Prebuilt CPython 3.11 Wheels" ./test_prebuilt

# Phase 4.5: Cython extension
run_suite "Phase 4.5: Cython Extension" ./test_cython_bin

# Phase 5: PyO3 bcrypt
run_suite "Phase 5: PyO3 bcrypt" ./test_bcrypt_bin

# Phase 6: pyyaml (Cython)
run_suite "Phase 6: pyyaml (Cython)" ./test_pyyaml_bin

# ── Step 3: Run VM Python tests ──
printf "${BOLD}Running VM Python tests...${RESET}\n\n"

run_vm_test() {
    local name="$1"
    local script="$2"

    TOTAL_SUITES=$((TOTAL_SUITES + 1))
    printf "${CYAN}${BOLD}[%d] %s${RESET}\n" "$TOTAL_SUITES" "$name"

    local output
    local exit_code=0
    output=$(./rustthon_bin "$script" 2>&1) || exit_code=$?

    # Check for Traceback (the definitive error indicator from the VM)
    local errors
    errors=$(echo "$output" | grep -v '^\[' | grep 'Traceback (most recent call last)' || true)

    if [ $exit_code -eq 0 ] && [ -z "$errors" ]; then
        PASSED_SUITES=$((PASSED_SUITES + 1))
        local last_line
        last_line=$(echo "$output" | grep -v '^\[' | tail -1)
        printf "    %s\n" "$last_line"
        printf "    ${GREEN}SUITE PASSED${RESET}\n\n"
    else
        FAILED_SUITES=$((FAILED_SUITES + 1))
        FAILED_NAMES="$FAILED_NAMES  - $name\n"
        echo "$output" | grep -v '^\[' | tail -10
        printf "    ${RED}SUITE FAILED (exit code %d)${RESET}\n\n" "$exit_code"
    fi
}

# VM basics
run_vm_test "VM: Functions & Defaults"        test_phase1.py
run_vm_test "VM: Exception Handling"          test_phase3.py
run_vm_test "VM: Classes & __init__"          test_phase4.py
run_vm_test "VM: Closures & Decorators"       test_phase6.py
run_vm_test "VM: *args/**kwargs"              test_phase7.py
run_vm_test "VM: Comprehensions"              test_phase8.py
run_vm_test "VM: Generators"                  test_phase9.py
run_vm_test "VM: String & List Methods"       test_phase10.py
run_vm_test "VM: Stdlib Stubs & Builtins"     test_phase11.py
run_vm_test "VM: Comprehensive Types"         test_final.py
run_vm_test "VM: Nonlocal Closures"           test_nonlocal.py
run_vm_test "VM: Dict Iteration"              test_dict_iter.py
run_vm_test "VM: Generator isinstance"        test_gen_isinstance.py

# Classes & inheritance
run_vm_test "VM: Class Inheritance"           test_class_inherit.py
run_vm_test "VM: Cross-Module Inheritance"    test_cross_inherit.py
run_vm_test "VM: super()"                     test_super.py
run_vm_test "VM: Multiple Inheritance + re"   test_vm_improvements.py

# Imports
run_vm_test "VM: Python Source Imports"       test_import.py
run_vm_test "VM: import * with __all__"       test_import_star.py
run_vm_test "VM: import collections.abc"      test_import_collections.py

# Native C extension imports from VM
run_vm_test "VM: import ujson (prebuilt)"     test_native_import.py

# YAML integration
run_vm_test "VM: import yaml"                 test_yaml_import.py
run_vm_test "VM: yaml CParser Events"         test_yaml_full.py
run_vm_test "VM: yaml.safe_load"              test_yaml_safeload_full.py

# ═══════════════════════════════════════════════════════════
printf "${BOLD}═══════════════════════════════════════════════════════════${RESET}\n"
printf "${BOLD}  FINAL RESULTS: %d/%d suites passed${RESET}" "$PASSED_SUITES" "$TOTAL_SUITES"

if [ $FAILED_SUITES -gt 0 ]; then
    printf "  ${RED}(%d FAILED)${RESET}" "$FAILED_SUITES"
fi
printf "\n"

if [ $FAILED_SUITES -eq 0 ]; then
    printf "${GREEN}${BOLD}  ALL SUITES PASSED${RESET}\n"
else
    printf "${RED}${BOLD}  Failed suites:${RESET}\n"
    printf "$FAILED_NAMES"
fi
printf "${BOLD}═══════════════════════════════════════════════════════════${RESET}\n"

exit $FAILED_SUITES
