"""
FerrumPy Test Harness

Provides utilities for automated testing of Pretty Printers using LLDB Python API.
Inspired by rust-prettifier-for-lldb's test_harness.py.

Usage:
    pytest tests/  # Run all tests
"""
import os
import subprocess
import textwrap
from typing import Any, Callable, Dict, Optional

# Try to import lldb - will fail outside of LLDB context
try:
    import lldb
    HAS_LLDB = True
except ImportError:
    HAS_LLDB = False
    lldb = None

# Project paths
PACKAGE_ROOT_PATH = os.path.abspath(
    os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
)
FERRUMPY_PATH = os.path.join(PACKAGE_ROOT_PATH, "python", "ferrumpy")
TEST_CRATE_PATH = os.path.join(PACKAGE_ROOT_PATH, "tests", "rust_sample")


def run_rust_test(
    rust_src: str,
    test_code: Callable[["lldb.SBDebugger", "lldb.SBFrame"], None],
    temp_dir: Optional[Any] = None,
):
    """
    Build and run a Rust test program, then execute test_code at breakpoint.
    
    Args:
        rust_src: Rust code to compile and test (will be wrapped in main())
        test_code: Callback function(debugger, frame) to run at breakpoint
        temp_dir: Optional temp directory (pytest fixture) for isolated builds
    """
    if not HAS_LLDB:
        raise RuntimeError("This test must be run within LLDB Python environment")
    
    # Use temp_dir if provided, otherwise use test_crate
    if temp_dir:
        project_dir = str(temp_dir)
        src_path = os.path.join(project_dir, "src", "main.rs")
        os.makedirs(os.path.dirname(src_path), exist_ok=True)
        
        # Create Cargo.toml
        cargo_toml_path = os.path.join(project_dir, "Cargo.toml")
        with open(cargo_toml_path, "w") as f:
            f.write(textwrap.dedent("""
                [package]
                name = "ferrumpy_test"
                version = "0.1.0"
                edition = "2021"
            """))
        
        # Prepare Rust source
        rust_src = textwrap.indent(textwrap.dedent(rust_src), "    ")
        rust_src += "\n    let _ = 0 + 0;  // breakpoint line"
        rust_src = "fn main() {\n" + rust_src + "\n}\n"
        
        with open(src_path, "w") as f:
            f.write(rust_src)
        
        # Build
        target_dir = os.path.join(PACKAGE_ROOT_PATH, "target", "test_builds")
        result = subprocess.run(
            ["cargo", "build", "--target-dir", target_dir],
            stderr=subprocess.PIPE,
            stdout=subprocess.PIPE,
            cwd=project_dir
        )
        
        if result.returncode != 0:
            raise RuntimeError(f"Cargo build failed: {result.stderr.decode('utf-8')}")
        
        binary_path = os.path.join(target_dir, "debug", "ferrumpy_test")
        breakpoint_line = len(rust_src.splitlines()) - 1
        src_file = "main.rs"
    else:
        # Use existing rust_sample
        binary_path = os.path.join(TEST_CRATE_PATH, "target", "debug", "rust_sample")
        if not os.path.exists(binary_path):
            raise RuntimeError(f"Test binary not found: {binary_path}. Run 'cargo build' in tests/rust_sample first.")
        breakpoint_line = 82  # Hardcoded for rust_sample
        src_file = "main.rs"
    
    # Create LLDB debugger
    debugger = lldb.SBDebugger.Create()
    debugger.SetAsync(False)
    
    try:
        target = debugger.CreateTargetWithFileAndArch(binary_path, lldb.LLDB_ARCH_DEFAULT)
        assert target.IsValid(), f"Failed to create target: {binary_path}"
        
        breakpoint = target.BreakpointCreateByLocation(src_file, breakpoint_line)
        assert breakpoint.num_locations >= 1, f"Failed to set breakpoint at {src_file}:{breakpoint_line}"
        
        process = target.LaunchSimple(None, None, ".")
        assert process.IsValid(), "Failed to launch process"
        
        thread = process.GetThreadAtIndex(0)
        frame = thread.GetFrameAtIndex(0)
        assert frame.IsValid(), "Failed to get frame"
        
        # Import ferrumpy
        repl = debugger.GetCommandInterpreter()
        res = lldb.SBCommandReturnObject()
        repl.HandleCommand(f"command script import {FERRUMPY_PATH}", res)
        assert res.Succeeded(), f"Failed to import ferrumpy: {res.GetError()}"
        
        # Run test code
        test_code(debugger, frame)
        
    finally:
        lldb.SBDebugger.Destroy(debugger)


def compare_summaries(frame: "lldb.SBFrame", expected: Dict[str, str]) -> Dict[str, str]:
    """
    Compare variable summaries against expected values.
    
    Returns dict of failures: {var_name: "expected X but got Y"}
    """
    failures = {}
    for name, expected_summary in expected.items():
        var = frame.FindVariable(name)
        if not var.IsValid():
            failures[name] = f"Variable not found"
            continue
        
        actual = var.GetSummary()
        if actual is None:
            actual = var.GetValue()
        
        if actual != expected_summary:
            failures[name] = f"expected '{expected_summary}' but got '{actual}'"
    
    return failures


def expect_summaries(expected: Dict[str, str], temp_dir: Optional[Any] = None):
    """
    Test that variables have expected summaries using rust_sample.
    
    For use with existing rust_sample test program.
    """
    def test_fn(debugger, frame):
        failures = compare_summaries(frame, expected)
        if failures:
            msg = "\n".join(f"  {k}: {v}" for k, v in failures.items())
            raise AssertionError(f"Summary mismatches:\n{msg}")
    
    run_rust_test("", test_fn, temp_dir=None)


def run_ferrumpy_command(debugger: "lldb.SBDebugger", command: str) -> str:
    """Run a ferrumpy command and return output."""
    repl = debugger.GetCommandInterpreter()
    res = lldb.SBCommandReturnObject()
    repl.HandleCommand(command, res)
    if not res.Succeeded():
        raise RuntimeError(f"Command failed: {res.GetError()}")
    return res.GetOutput()


# =============================================================================
# Standalone test utilities (no LLDB required)
# =============================================================================

def test_path_tokenizer():
    """Test the path tokenizer without LLDB."""
    from python.ferrumpy.path_resolver import tokenize_path
    
    test_cases = [
        ("foo", [("field", "foo")]),
        ("foo.bar", [("field", "foo"), ("field", "bar")]),
        ("foo[0]", [("field", "foo"), ("index", "0")]),
        ("foo.bar[1].baz", [("field", "foo"), ("field", "bar"), ("index", "1"), ("field", "baz")]),
        ("foo.*", [("field", "foo"), ("deref", None)]),
        ("foo.0", [("field", "foo"), ("tuple", "0")]),
    ]
    
    failures = []
    for path, expected in test_cases:
        result = tokenize_path(path)
        if result != expected:
            failures.append(f"  tokenize_path('{path}'): expected {expected}, got {result}")
    
    if failures:
        raise AssertionError("Tokenizer failures:\n" + "\n".join(failures))
    
    return len(test_cases)
