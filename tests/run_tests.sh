#!/bin/bash
#
# FerrumPy Test Runner
#
# Usage:
#   ./tests/run_tests.sh          # Run all tests
#   ./tests/run_tests.sh --python # Run only Python tests
#   ./tests/run_tests.sh --lldb   # Run only LLDB tests

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
RUST_SAMPLE_DIR="$PROJECT_ROOT/tests/rust_sample"

echo "=============================================="
echo "FerrumPy Test Suite"
echo "=============================================="
echo

# Build rust_sample if needed
if [ ! -f "$RUST_SAMPLE_DIR/target/debug/rust_sample" ]; then
    echo "Building rust_sample..."
    (cd "$RUST_SAMPLE_DIR" && cargo build --quiet)
    echo "Built rust_sample"
    echo
fi

# Set up REPL worker path for tests
# In development, the worker binary is in target/release
# This ensures tests can find it regardless of working directory
WORKER_BINARY="$PROJECT_ROOT/target/release/ferrumpy-repl-worker"
if [ -f "$WORKER_BINARY" ]; then
    export FERRUMPY_REPL_WORKER="$WORKER_BINARY"
    echo "Using REPL worker: $FERRUMPY_REPL_WORKER"
else
    echo "Warning: REPL worker not found at $WORKER_BINARY"
    echo "Run 'cargo build --release -p ferrumpy-repl-worker' to build it"
fi
echo
run_python_tests() {
    echo "--- Python Unit Tests (No LLDB) ---"
    echo
    
    python3 << 'PYTHON_SCRIPT'
import sys
sys.path.insert(0, '/Users/wangjiajie/software/FerrumPy')
from python.ferrumpy.path_resolver import tokenize_path, IdentSegment, IndexSegment, DerefSegment

tests = [
    ('foo', [IdentSegment('foo')]),
    ('foo.bar', [IdentSegment('foo'), IdentSegment('bar')]),
    ('foo[0]', [IdentSegment('foo'), IndexSegment(0)]),
    ('foo.bar[1].baz', [IdentSegment('foo'), IdentSegment('bar'), IndexSegment(1), IdentSegment('baz')]),
    ('foo.*', [IdentSegment('foo'), DerefSegment()]),
    ('foo.0', [IdentSegment('foo'), IdentSegment('__0')]),
]

passed = 0
failed = 0
for path, expected in tests:
    result = tokenize_path(path)
    if result == expected:
        print(f'  ✓ tokenize_path("{path}")')
        passed += 1
    else:
        print(f'  ✗ tokenize_path("{path}")')
        print(f'    Expected: {expected}')
        print(f'    Got:      {result}')
        failed += 1

print()
print(f'Tokenizer: {passed}/{passed+failed} passed')
sys.exit(1 if failed > 0 else 0)
PYTHON_SCRIPT

    if [ $? -ne 0 ]; then
        return 1
    fi
    
    echo
    echo "--- Type Normalization Tests ---"
    echo
    python3 "$PROJECT_ROOT/tests/test_type_normalization.py"
    
    echo
    echo "--- Completion API Tests ---"
    echo
    python3 "$PROJECT_ROOT/tests/test_completions.py"

    return $?
}

# ============================================
# LLDB Pretty Printer Tests
# ============================================
run_lldb_tests() {
    echo
    echo "--- LLDB Pretty Printer Tests ---"
    echo
    
    cd "$RUST_SAMPLE_DIR"
    
    # Run LLDB and capture output to temp file (subshell pipe doesn't work on macOS)
    local tmpfile
    tmpfile=$(mktemp)
    lldb -b target/debug/rust_sample \
        -o "command script import /Users/wangjiajie/software/FerrumPy/python/ferrumpy" \
        -o "b main.rs:94" \
        -o "run" \
        -o "ferrumpy pp simple_string" \
        -o "ferrumpy pp numbers" \
        -o "ferrumpy pp some_value" \
        -o "ferrumpy pp none_value" \
        -o "ferrumpy pp ok_result" \
        -o "ferrumpy pp err_result" \
        -o "ferrumpy pp arc_value" \
        -o "ferrumpy pp rc_value" \
        -o "ferrumpy type simple_string" \
        -o "ferrumpy-pp simple_string" \
        -o "ferrumpy pp numbers[0]" \
        -o "ferrumpy pp matrix[0][1]" \
        -o "ferrumpy pp matrix[1][2]" \
        -o "ferrumpy pp fixed_array[2]" \
        -o "kill" \
        -o "quit" &> "$tmpfile"
    local output
    output=$(cat "$tmpfile")
    rm -f "$tmpfile"
    
    # Check each expected output
    local passed=0
    local failed=0
    
    check() {
        if echo "$output" | grep -q "$2"; then
            echo "  ✓ $1"
            passed=$((passed + 1))
        else
            echo "  ✗ $1 (expected: $2)"
            failed=$((failed + 1))
        fi
    }
    
    check "String" '"Hello, FerrumPy!"'
    check "Vec" '\[1, 2, 3, 4, 5\]'
    check "Option Some" 'Some(42)'
    check "Option None" ') None'
    check "Result Ok" 'Ok(100)'
    check "Result Err" 'Err("something went wrong")'
    check "Arc" 'Arc('
    check "Rc" 'Rc(42)'
    # Note: ferrumpy complete requires ferrumpy-server binary (not built)
    check "Type" 'Type: alloc::string::String'
    check "Vec Index" '(int) 1'
    check "Matrix Index" '(int) 2'
    check "Matrix Nested" '(int) 6'
    check "Fixed Array" '(int) 30'
    
    echo
    echo "Tests: $passed/$((passed + failed)) passed"
    
    [ $failed -eq 0 ]
    return $?
}

# ============================================
# REPL Integration Tests
# ============================================
run_repl_tests() {
    echo
    echo "--- REPL Integration Tests ---"
    echo
    
    # Check if expect is available
    if ! command -v expect &> /dev/null; then
        echo "  ⚠ expect not found, skipping interactive REPL tests"
        echo "  Install with: brew install expect"
        return 0
    fi
    
    echo "Starting REPL test (this may take a while for first-time compilation)..."
    echo "Using expect for interactive testing..."
    echo
    
    # Run the expect script
    if expect "$PROJECT_ROOT/tests/test_repl.exp" "$PROJECT_ROOT"; then
        echo
        echo "REPL Tests: PASSED"
        return 0
    else
        echo
        echo "REPL Tests: FAILED"
        return 1
    fi
}

# ============================================
# Main
# ============================================
PYTHON_OK=true
LLDB_OK=true
REPL_OK=true

if [ "$1" != "--lldb" ] && [ "$1" != "--repl" ]; then
    run_python_tests || PYTHON_OK=false
fi

if [ "$1" != "--python" ] && [ "$1" != "--repl" ]; then
    run_lldb_tests || LLDB_OK=false
fi

if [ "$1" == "--repl" ] || [ "$1" == "--all" ] || [ -z "$1" ]; then
    run_repl_tests || REPL_OK=false
fi

echo
echo "=============================================="
if $PYTHON_OK && $LLDB_OK && $REPL_OK; then
    echo "All tests passed!"
else
    echo "Some tests failed."
    exit 1
fi
echo "=============================================="
