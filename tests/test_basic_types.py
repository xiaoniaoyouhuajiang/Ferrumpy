"""
Tests for basic Rust types Pretty Printers.

These tests verify that FerrumPy correctly formats common Rust types.
"""
import pytest
import sys
import os

# Add project root to path for imports
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


class TestPathTokenizer:
    """Test path tokenizer without LLDB (can run anywhere)."""
    
    def test_simple_field(self):
        from python.ferrumpy.path_resolver import tokenize_path
        assert tokenize_path("foo") == [("field", "foo")]
    
    def test_nested_fields(self):
        from python.ferrumpy.path_resolver import tokenize_path
        assert tokenize_path("foo.bar.baz") == [
            ("field", "foo"), ("field", "bar"), ("field", "baz")
        ]
    
    def test_index_access(self):
        from python.ferrumpy.path_resolver import tokenize_path
        assert tokenize_path("arr[0]") == [("field", "arr"), ("index", "0")]
        assert tokenize_path("arr[42]") == [("field", "arr"), ("index", "42")]
    
    def test_mixed_access(self):
        from python.ferrumpy.path_resolver import tokenize_path
        assert tokenize_path("users[0].name") == [
            ("field", "users"), ("index", "0"), ("field", "name")
        ]
    
    def test_deref(self):
        from python.ferrumpy.path_resolver import tokenize_path
        assert tokenize_path("ptr.*") == [("field", "ptr"), ("deref", None)]
    
    def test_tuple_field(self):
        from python.ferrumpy.path_resolver import tokenize_path
        assert tokenize_path("tuple.0") == [("field", "tuple"), ("tuple", "0")]
        assert tokenize_path("tuple.1") == [("field", "tuple"), ("tuple", "1")]
    
    def test_complex_path(self):
        from python.ferrumpy.path_resolver import tokenize_path
        assert tokenize_path("config.users[0].name.*") == [
            ("field", "config"),
            ("field", "users"),
            ("index", "0"),
            ("field", "name"),
            ("deref", None),
        ]


# =============================================================================
# LLDB-dependent tests (require running in LLDB Python environment)
# =============================================================================

# These tests are marked to skip if LLDB is not available
try:
    import lldb
    HAS_LLDB = True
except ImportError:
    HAS_LLDB = False


@pytest.mark.skipif(not HAS_LLDB, reason="LLDB not available")
class TestPrettyPrintersInLLDB:
    """
    Tests that require LLDB environment.
    
    To run these tests:
        python -c "import lldb; exec(open('tests/run_lldb_tests.py').read())"
    
    Or use the test runner script.
    """
    
    def test_string_formatting(self):
        from test_harness import run_rust_test, compare_summaries
        
        def check(debugger, frame):
            failures = compare_summaries(frame, {
                "simple_string": '"Hello, FerrumPy!"',
            })
            assert not failures, failures
        
        run_rust_test("", check)
    
    def test_vec_formatting(self):
        from test_harness import run_rust_test, compare_summaries
        
        def check(debugger, frame):
            var = frame.FindVariable("numbers")
            summary = var.GetSummary()
            # Check contains expected elements
            assert "1" in summary and "5" in summary
        
        run_rust_test("", check)
    
    def test_option_formatting(self):
        from test_harness import run_rust_test, compare_summaries
        
        def check(debugger, frame):
            some_var = frame.FindVariable("some_value")
            none_var = frame.FindVariable("none_value")
            
            some_summary = some_var.GetSummary()
            none_summary = none_var.GetSummary()
            
            assert "Some" in some_summary and "42" in some_summary
            assert "None" in none_summary
        
        run_rust_test("", check)


if __name__ == "__main__":
    # Run tokenizer tests (no LLDB required)
    pytest.main([__file__, "-v", "-k", "TestPathTokenizer"])
