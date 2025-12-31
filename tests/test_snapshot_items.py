#!/usr/bin/env python3
"""Test item-level snapshot export feature."""

import subprocess
import sys

# Test script for item-level snapshot export
test_script = """
set pagination off
set confirm off

file tests/rust_sample/target/debug/rust_sample
command script import {}/python/ferrumpy
b main.rs:94
run

# Test item-level export
ferrumpy repl

# Try to access variable from user function
fn get_first_number() -> Option<&'static i32> {{
    numbers().first()
}}

get_first_number()

:q
quit
""".format(sys.argv[1] if len(sys.argv) > 1 else ".")

# Run LLDB with the test script
proc = subprocess.Popen(
    ['lldb'],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    text=True
)

stdout, stderr = proc.communicate(test_script, timeout=30)

print("=== STDOUT ===")
print(stdout)

if stderr:
    print("\n=== STDERR ===")
    print(stderr)

# Check for success indicators
if "items. Access:" in stdout:
    print("\n✅ Item- snapshot export is working!")
    if "Some" in stdout and "get_first_number()" in stdout:
        print("✅ User function can access snapshot variables!")
        sys.exit(0)
    else:
        print("⚠️  Function call might have failed")
        sys.exit(1)
else:
    print("\n❌ Item-level export not detected")
    sys.exit(1)
