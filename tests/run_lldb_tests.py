#!/usr/bin/env python3
"""
LLDB Test Runner for FerrumPy

This script runs tests that require the LLDB Python environment.
It must be executed from within LLDB or using lldb's Python.

Usage:
    # From LLDB:
    (lldb) script exec(open('tests/run_lldb_tests.py').read())
    
    # Or using lldb Python directly (macOS):
    /Applications/Xcode.app/Contents/SharedFrameworks/LLDB.framework/Resources/Python3/bin/python3 tests/run_lldb_tests.py
"""
import os
import sys

# Add project root to path
PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, PROJECT_ROOT)
sys.path.insert(0, os.path.join(PROJECT_ROOT, "tests"))

try:
    import lldb
except ImportError:
    print("ERROR: This script must be run within LLDB Python environment.")
    print("Try: lldb -o 'script exec(open(\"tests/run_lldb_tests.py\").read())'")
    sys.exit(1)

from test_harness import run_rust_test, compare_summaries, run_ferrumpy_command


def test_string_pretty_printer():
    """Test String formatting."""
    print("Testing String Pretty Printer...", end=" ")
    
    def check(debugger, frame):
        var = frame.FindVariable("simple_string")
        summary = var.GetSummary()
        assert summary is not None, "No summary for simple_string"
        assert "Hello" in summary or "FerrumPy" in summary, f"Unexpected: {summary}"
    
    run_rust_test("", check)
    print("✓")


def test_vec_pretty_printer():
    """Test Vec formatting."""
    print("Testing Vec Pretty Printer...", end=" ")
    
    def check(debugger, frame):
        var = frame.FindVariable("numbers")
        summary = var.GetSummary()
        assert summary is not None, "No summary for numbers"
        assert "[" in summary and "]" in summary, f"Unexpected: {summary}"
    
    run_rust_test("", check)
    print("✓")


def test_option_pretty_printer():
    """Test Option formatting."""
    print("Testing Option Pretty Printer...", end=" ")
    
    def check(debugger, frame):
        some_var = frame.FindVariable("some_value")
        none_var = frame.FindVariable("none_value")
        
        some_summary = some_var.GetSummary()
        none_summary = none_var.GetSummary()
        
        assert some_summary is not None, "No summary for some_value"
        assert none_summary is not None, "No summary for none_value"
        assert "Some" in some_summary, f"Expected Some, got: {some_summary}"
        assert "None" in none_summary, f"Expected None, got: {none_summary}"
    
    run_rust_test("", check)
    print("✓")


def test_result_pretty_printer():
    """Test Result formatting."""
    print("Testing Result Pretty Printer...", end=" ")
    
    def check(debugger, frame):
        ok_var = frame.FindVariable("ok_result")
        err_var = frame.FindVariable("err_result")
        
        ok_summary = ok_var.GetSummary()
        err_summary = err_var.GetSummary()
        
        assert ok_summary is not None, "No summary for ok_result"
        assert err_summary is not None, "No summary for err_result"
        assert "Ok" in ok_summary, f"Expected Ok, got: {ok_summary}"
        assert "Err" in err_summary, f"Expected Err, got: {err_summary}"
    
    run_rust_test("", check)
    print("✓")


def test_arc_rc_pretty_printer():
    """Test Arc/Rc formatting."""
    print("Testing Arc/Rc Pretty Printer...", end=" ")
    
    def check(debugger, frame):
        arc_var = frame.FindVariable("arc_value")
        rc_var = frame.FindVariable("rc_value")
        
        arc_summary = arc_var.GetSummary()
        rc_summary = rc_var.GetSummary()
        
        assert arc_summary is not None, "No summary for arc_value"
        assert rc_summary is not None, "No summary for rc_value"
        assert "Arc" in arc_summary, f"Expected Arc, got: {arc_summary}"
        assert "Rc" in rc_summary, f"Expected Rc, got: {rc_summary}"
    
    run_rust_test("", check)
    print("✓")


def test_ferrumpy_pp_command():
    """Test ferrumpy pp command."""
    print("Testing ferrumpy pp command...", end=" ")
    
    def check(debugger, frame):
        output = run_ferrumpy_command(debugger, "ferrumpy pp simple_string")
        assert "Hello" in output or "FerrumPy" in output, f"Unexpected: {output}"
    
    run_rust_test("", check)
    print("✓")


def run_all_tests():
    """Run all LLDB-dependent tests."""
    print("=" * 60)
    print("FerrumPy LLDB Test Suite")
    print("=" * 60)
    print()
    
    tests = [
        test_string_pretty_printer,
        test_vec_pretty_printer,
        test_option_pretty_printer,
        test_result_pretty_printer,
        test_arc_rc_pretty_printer,
        test_ferrumpy_pp_command,
    ]
    
    passed = 0
    failed = 0
    
    for test in tests:
        try:
            test()
            passed += 1
        except Exception as e:
            print(f"✗ {test.__name__}: {e}")
            failed += 1
    
    print()
    print("=" * 60)
    print(f"Results: {passed} passed, {failed} failed")
    print("=" * 60)
    
    return failed == 0


if __name__ == "__main__":
    success = run_all_tests()
    sys.exit(0 if success else 1)
