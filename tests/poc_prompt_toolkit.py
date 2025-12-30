#!/usr/bin/env python3
"""
PoC: Test prompt_toolkit features in LLDB's Python environment

This script tests:
1. Basic PromptSession creation
2. FileHistory support
3. Custom Completer
4. TTY detection and graceful degradation
5. Key bindings (Ctrl+C, Ctrl+D)

Run: lldb -b -o "command script import /path/to/poc_prompt_toolkit.py"
Or interactively in lldb: script exec(open('poc_prompt_toolkit.py').read())
"""

import sys
import os

def test_imports():
    """Test all required prompt_toolkit imports"""
    print("=" * 50)
    print("Test 1: Checking imports...")
    
    try:
        from prompt_toolkit import PromptSession
        from prompt_toolkit.history import FileHistory, InMemoryHistory
        from prompt_toolkit.completion import Completer, Completion
        from prompt_toolkit.validation import Validator, ValidationError
        from prompt_toolkit.key_binding import KeyBindings
        from prompt_toolkit.patch_stdout import patch_stdout
        print("✓ All prompt_toolkit imports successful")
        return True
    except ImportError as e:
        print(f"✗ Import failed: {e}")
        return False


def test_tty_detection():
    """Test TTY detection for graceful degradation"""
    print("\n" + "=" * 50)
    print("Test 2: TTY Detection...")
    
    is_tty = sys.stdin.isatty() if hasattr(sys.stdin, 'isatty') else False
    stdout_tty = sys.stdout.isatty() if hasattr(sys.stdout, 'isatty') else False
    
    print(f"  stdin.isatty(): {is_tty}")
    print(f"  stdout.isatty(): {stdout_tty}")
    print(f"  TERM: {os.environ.get('TERM', 'not set')}")
    
    if is_tty:
        print("✓ Running in TTY mode - full features available")
    else:
        print("⚠ Running in non-TTY mode - will use fallback input()")
    
    return True


def test_file_history():
    """Test FileHistory write/read"""
    print("\n" + "=" * 50)
    print("Test 3: FileHistory...")
    
    import tempfile
    from prompt_toolkit.history import FileHistory
    
    history_path = os.path.expanduser("~/.cache/ferrumpy/test_history")
    os.makedirs(os.path.dirname(history_path), exist_ok=True)
    
    # Remove existing file to start fresh
    if os.path.exists(history_path):
        os.remove(history_path)
    
    try:
        # Create history and add entries
        history = FileHistory(history_path)
        
        # Store some test entries
        history.store_string("test_entry_1")
        history.store_string("test_entry_2")
        
        # Create new instance and verify persistence
        history2 = FileHistory(history_path)
        entries = list(history2.load_history_strings())
        
        if len(entries) >= 2:
            print(f"✓ FileHistory works (path: {history_path})")
            print(f"  Stored {len(entries)} entries")
            return True
        else:
            print(f"✗ FileHistory entries not persisted (got {len(entries)})")
            return False
            
    except Exception as e:
        print(f"✗ FileHistory failed: {e}")
        return False


def test_completer():
    """Test custom Completer implementation"""
    print("\n" + "=" * 50)
    print("Test 4: Custom Completer...")
    
    from prompt_toolkit.completion import Completer, Completion
    from prompt_toolkit.document import Document
    
    class TestCompleter(Completer):
        def __init__(self, words):
            self.words = words
        
        def get_completions(self, document, complete_event):
            text = document.text_before_cursor
            word = text.split()[-1] if text.split() else ""
            
            for w in self.words:
                if w.startswith(word):
                    yield Completion(w, start_position=-len(word))
    
    try:
        completer = TestCompleter(["numbers", "simple_string", "map", "vec"])
        doc = Document("num")
        completions = list(completer.get_completions(doc, None))
        
        if completions and completions[0].text == "numbers":
            print("✓ Custom Completer works")
            print(f"  'num' -> {[c.text for c in completions]}")
            return True
        else:
            print("✗ Completer returned unexpected results")
            return False
            
    except Exception as e:
        print(f"✗ Completer failed: {e}")
        return False


def test_validator():
    """Test Validator for multi-line input detection"""
    print("\n" + "=" * 50)
    print("Test 5: Validator for multi-line...")
    
    from prompt_toolkit.validation import Validator, ValidationError
    
    class RustFragmentValidator(Validator):
        def validate(self, document):
            text = document.text
            
            # Simple check: unclosed braces/parens mean incomplete
            opens = text.count('{') + text.count('(') + text.count('[')
            closes = text.count('}') + text.count(')') + text.count(']')
            
            if opens > closes:
                raise ValidationError(message="Incomplete input (unclosed braces)")
    
    try:
        validator = RustFragmentValidator()
        
        from prompt_toolkit.document import Document
        
        # Test complete input
        try:
            validator.validate(Document("let x = 42;"))
            complete_ok = True
        except ValidationError:
            complete_ok = False
        
        # Test incomplete input
        try:
            validator.validate(Document("fn foo() {"))
            incomplete_ok = False  # Should have raised
        except ValidationError:
            incomplete_ok = True
        
        if complete_ok and incomplete_ok:
            print("✓ Validator works for multi-line detection")
            return True
        else:
            print(f"✗ Validator behavior unexpected (complete={complete_ok}, incomplete={incomplete_ok})")
            return False
            
    except Exception as e:
        print(f"✗ Validator failed: {e}")
        return False


def test_key_bindings():
    """Test KeyBindings creation"""
    print("\n" + "=" * 50)
    print("Test 6: KeyBindings...")
    
    from prompt_toolkit.key_binding import KeyBindings
    
    try:
        bindings = KeyBindings()
        
        @bindings.add('c-c')
        def handle_ctrl_c(event):
            pass
        
        @bindings.add('c-d')
        def handle_ctrl_d(event):
            pass
        
        print("✓ KeyBindings can be created and configured")
        return True
        
    except Exception as e:
        print(f"✗ KeyBindings failed: {e}")
        return False


def test_prompt_session_creation():
    """Test PromptSession creation (without actually prompting)"""
    print("\n" + "=" * 50)
    print("Test 7: PromptSession creation...")
    
    from prompt_toolkit import PromptSession
    from prompt_toolkit.history import InMemoryHistory
    
    try:
        # Create session with various options
        session = PromptSession(
            message=">> ",
            history=InMemoryHistory(),
            enable_history_search=True,
            # Don't actually prompt, just test creation
        )
        
        print("✓ PromptSession can be created")
        print(f"  message: {session.message}")
        return True
        
    except Exception as e:
        print(f"✗ PromptSession creation failed: {e}")
        return False


def run_all_tests():
    """Run all PoC tests"""
    print("\n" + "=" * 60)
    print("FerrumPy REPL Enhancement - Phase 0 PoC")
    print("Testing prompt_toolkit in LLDB Python environment")
    print("=" * 60)
    
    results = {}
    
    results['imports'] = test_imports()
    results['tty_detection'] = test_tty_detection()
    results['file_history'] = test_file_history()
    results['completer'] = test_completer()
    results['validator'] = test_validator()
    results['key_bindings'] = test_key_bindings()
    results['prompt_session'] = test_prompt_session_creation()
    
    print("\n" + "=" * 60)
    print("Summary:")
    print("=" * 60)
    
    passed = sum(1 for v in results.values() if v)
    total = len(results)
    
    for name, success in results.items():
        status = "✓ PASS" if success else "✗ FAIL"
        print(f"  {name}: {status}")
    
    print(f"\nTotal: {passed}/{total} tests passed")
    
    if passed == total:
        print("\n🎉 All tests passed! prompt_toolkit is ready for FerrumPy REPL.")
    else:
        print("\n⚠ Some tests failed. Review issues above.")
    
    return passed == total


if __name__ == "__main__":
    run_all_tests()
