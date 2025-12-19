#!/bin/bash
# Quick test script for FerrumPy

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo "=== Building test Rust program ==="
cd "$SCRIPT_DIR/rust_sample"
cargo build

echo ""
echo "=== Test program built successfully ==="
echo ""
echo "To test FerrumPy, run:"
echo ""
echo "  lldb target/debug/rust_sample"
echo ""
echo "Then in LLDB:"
echo "  (lldb) command script import $PROJECT_ROOT/python/ferrumpy"
echo "  (lldb) b main.rs:80"
echo "  (lldb) run"
echo "  (lldb) ferrumpy locals"
echo "  (lldb) ferrumpy pp simple_string"
echo "  (lldb) ferrumpy pp config.users[0].name"
echo ""
