#!/usr/bin/env python3
"""
Interactive PoC: Test prompt_toolkit REPL experience in LLDB

This creates a minimal REPL with:
- Syntax-aware multi-line input
- Tab completion
- History persistence
- Ctrl+C / Ctrl+D handling

Run in LLDB:
  script exec(open('/path/to/poc_interactive_repl.py').read())
  test_interactive_repl()
"""

import sys
import os

def test_interactive_repl():
    """Launch an interactive REPL demo with prompt_toolkit"""
    
    # Check if we're in a TTY
    if not (hasattr(sys.stdin, 'isatty') and sys.stdin.isatty()):
        print("⚠ Not in a TTY environment. Interactive test skipped.")
        print("  Run this in an interactive LLDB session.")
        return
    
    try:
        from prompt_toolkit import PromptSession
        from prompt_toolkit.history import FileHistory
        from prompt_toolkit.completion import Completer, Completion
        from prompt_toolkit.validation import Validator, ValidationError
        from prompt_toolkit.key_binding import KeyBindings
        from prompt_toolkit.styles import Style
    except ImportError as e:
        print(f"✗ Import failed: {e}")
        print("  Install with: python3.14 -m pip install --user --break-system-packages prompt_toolkit")
        return
    
    # --- Custom Completer ---
    class SnapshotCompleter(Completer):
        """Simulates completing snapshot variable names and Rust keywords"""
        
        SNAPSHOT_VARS = [
            "numbers", "simple_string", "map", "ok_result", 
            "some_value", "none_value", "config", "arc_value"
        ]
        RUST_KEYWORDS = [
            "let", "mut", "fn", "struct", "impl", "pub", "use",
            "if", "else", "match", "for", "while", "loop", "break", "continue",
            "return", "true", "false", "self", "Self"
        ]
        COMMANDS = [":q", ":quit", ":vars", ":help", ":clear", ":type"]
        
        def get_completions(self, document, complete_event):
            text = document.text_before_cursor
            
            # Get current word
            words = text.split()
            word = words[-1] if words else ""
            
            # Skip if empty or whitespace after cursor
            if not word:
                return
            
            # Commands start with :
            if word.startswith(':'):
                for cmd in self.COMMANDS:
                    if cmd.startswith(word):
                        yield Completion(cmd, start_position=-len(word), 
                                         display_meta="command")
                return
            
            # Variables and keywords
            all_items = self.SNAPSHOT_VARS + self.RUST_KEYWORDS
            for item in sorted(set(all_items)):
                if item.startswith(word.lower()) or item.startswith(word):
                    meta = "variable" if item in self.SNAPSHOT_VARS else "keyword"
                    yield Completion(item, start_position=-len(word),
                                     display_meta=meta)
    
    # --- Multi-line Validator ---
    class RustValidator(Validator):
        """Check if input is complete (balanced braces/parens)"""
        
        def validate(self, document):
            text = document.text.strip()
            
            # Empty is valid
            if not text:
                return
            
            # Double newline forces submit (escape hatch)
            if text.endswith('\n\n'):
                return
            
            # Check brace balance
            opens = text.count('{') + text.count('(') + text.count('[')
            closes = text.count('}') + text.count(')') + text.count(']')
            
            if opens > closes:
                raise ValidationError(
                    message=f"Unclosed braces ({opens} open, {closes} close)",
                    cursor_position=len(text)
                )
            
            # Check for unclosed string
            in_string = False
            escape_next = False
            for char in text:
                if escape_next:
                    escape_next = False
                    continue
                if char == '\\':
                    escape_next = True
                elif char == '"' and not escape_next:
                    in_string = not in_string
            
            if in_string:
                raise ValidationError(
                    message="Unclosed string literal",
                    cursor_position=len(text)
                )
    
    # --- Key Bindings ---
    bindings = KeyBindings()
    ctrl_c_count = [0]
    
    @bindings.add('c-c')
    def handle_ctrl_c(event):
        ctrl_c_count[0] += 1
        if ctrl_c_count[0] >= 2:
            print("\n(Use :q to exit)")
            ctrl_c_count[0] = 0
        else:
            # Clear current line
            event.current_buffer.reset()
            print("\n(Ctrl+C to clear, type :q to exit)")
    
    # --- Style ---
    style = Style.from_dict({
        'prompt': '#00aa00 bold',
        'continuation': '#888888',
    })
    
    # --- Session ---
    history_path = os.path.expanduser("~/.cache/ferrumpy/poc_history")
    os.makedirs(os.path.dirname(history_path), exist_ok=True)
    
    session = PromptSession(
        history=FileHistory(history_path),
        completer=SnapshotCompleter(),
        validator=RustValidator(),
        validate_while_typing=False,  # Only validate on Enter
        multiline=True,
        prompt_continuation=lambda width, line_number, is_soft_wrap: '... ',
        key_bindings=bindings,
        enable_history_search=True,
        style=style,
    )
    
    print("=" * 60)
    print("FerrumPy REPL PoC - prompt_toolkit Demo")
    print("=" * 60)
    print("Features being tested:")
    print("  • Tab completion (try: num<Tab>, :h<Tab>)")
    print("  • Multi-line input (try: fn foo() {<Enter>)")
    print("  • History (Up/Down arrows, Ctrl+R to search)")
    print("  • Ctrl+C clears line, :q exits")
    print("=" * 60)
    print()
    
    while True:
        try:
            text = session.prompt(">> ", 
                                  rprompt="[PoC]",
                                  mouse_support=False)
            
            text = text.strip()
            
            if not text:
                continue
            
            if text in (":q", ":quit", ":exit"):
                print("Exiting PoC REPL.")
                break
            
            if text == ":vars":
                print("Snapshot variables: numbers, simple_string, map, ok_result, ...")
                continue
            
            if text == ":help":
                print("Commands: :q, :vars, :help, :clear")
                print("Try typing Rust expressions!")
                continue
            
            if text == ":clear":
                os.system('clear')
                continue
            
            # Simulate eval
            print(f"[Would eval]: {text}")
            ctrl_c_count[0] = 0
            
        except KeyboardInterrupt:
            print("\n(Ctrl+C)")
            continue
        except EOFError:
            print("\n(Ctrl+D - exiting)")
            break
    
    print("\n✓ Interactive PoC completed successfully!")


if __name__ == "__main__":
    test_interactive_repl()
