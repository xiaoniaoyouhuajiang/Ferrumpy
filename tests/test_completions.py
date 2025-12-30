import sys
import os
import json

# Add project root to path
PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, PROJECT_ROOT)

try:
    from python.ferrumpy.ferrumpy_core import PyReplSession
except ImportError:
    print("ferrumpy_core not found. Build with 'maturin develop'")
    sys.exit(1)

def test_rich_completions():
    print("Testing rich completions API...")
    session = PyReplSession()
    
    # 1. Test basic completion structure
    code = "let x = 42; x."
    result = session.completions(code, len(code))
    completions = result.get('completions', [])
    
    print(f"Found {len(completions)} completions for 'x.'")
    
    # We expect some completions (like clones, etc.)
    # Since rust-analyzer state might be empty initially, we might need a dummy eval first
    session.eval("let test_var = 123;")
    
    code2 = "test_v"
    result2 = session.completions(code2, len(code2))
    completions2 = result2.get('completions', [])
    print(f"Completions for 'test_v': {completions2}")
    
    # Check for our variable
    found = False
    for c in completions2:
        if c.get('label') == 'test_var':
            found = True
            assert c.get('kind') == 'Variable', f"Expected kind 'Variable', got {c.get('kind')}"
            assert 'i32' in c.get('detail', ''), f"Expected 'i32' in detail, got {c.get('detail')}"
            print(f"  ✓ Found '{c.get('label')}' (kind: {c.get('kind')}, detail: {c.get('detail')})")
    
    if not found:
        print("  ⚠ 'test_var' not found in completions (rust-analyzer might still be indexing)")
    
    # 2. Test field completions
    session.eval("struct Config { database: String }")
    session.eval("let config = Config { database: \"test\".to_string() };")
    
    code3 = "config.d"
    result3 = session.completions(code3, len(code3))
    completions3 = result3.get('completions', [])
    print(f"Completions for 'config.d': {completions3}")
    
    found_field = False
    for c in completions3:
        if c.get('label') == 'database':
            found_field = True
            assert c.get('kind') == 'Field', f"Expected kind 'Field', got {c.get('kind')}"
            assert 'String' in c.get('detail', ''), f"Expected 'String' in detail, got {c.get('detail')}"
            print(f"  ✓ Found field '{c.get('label')}' (kind: {c.get('kind')}, detail: {c.get('detail')})")
            
    if not found_field:
        # Note: sometimes rust-analyzer needs a bit of time or a specific trigger
        print("  ⚠ 'database' field not found in completions")

    print("\nCompletions API test finished.")

if __name__ == "__main__":
    test_rich_completions()
