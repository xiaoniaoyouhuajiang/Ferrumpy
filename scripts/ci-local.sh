#!/bin/bash
#
# Local CI Validation Script
# Run this before pushing to verify all CI checks will pass
#

set -e  # Exit on first error

echo "=============================================="
echo "Local CI Validation"
echo "=============================================="
echo

# 1. Rust Formatting
echo "📝 Checking Rust formatting..."
cargo fmt --all -- --check
echo "✅ Rust formatting OK"
echo

# 2. Clippy
echo "🔍 Running Clippy..."
cargo clippy --all-targets --all-features -- -D warnings
echo "✅ Clippy OK"
echo

# 3. Rust Tests
echo "🧪 Running Rust tests..."
cargo test -p ferrumpy-core
echo "✅ Rust tests OK"
echo

# 4. Python Linting
echo "🐍 Checking Python with ruff..."
if ! command -v ruff &> /dev/null; then
    echo "Installing ruff..."
    pip install ruff
fi
ruff check python/ tests/
echo "✅ Python linting OK"
echo

# 5. Python Tests
echo "🧪 Running Python tests..."
python3 tests/test_type_normalization.py
echo "✅ Python tests OK"
echo

# 6. Build repl-worker binary
echo "🔧 Building repl-worker binary..."
cargo build --release -p ferrumpy-repl-worker
mkdir -p data/scripts
cp target/release/ferrumpy-repl-worker data/scripts/
echo "✅ repl-worker binary OK"
echo

# 7. Build wheel
echo "🔨 Building wheel..."
if ! command -v maturin &> /dev/null; then
    echo "Installing maturin..."
    pip install maturin
fi
maturin build --release
echo "✅ Wheel build OK"
echo

# 7. Integration Tests (optional, takes time)
read -p "Run full integration tests? (y/N) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo "🔗 Running integration tests..."
    ./tests/run_tests.sh
    echo "✅ Integration tests OK"
    echo
fi

echo "=============================================="
echo "✅ All CI checks passed!"
echo "=============================================="
echo
echo "Ready to push to GitHub."
