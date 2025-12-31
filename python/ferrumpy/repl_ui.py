"""
FerrumPy REPL UI Enhancement with prompt_toolkit

Provides an enhanced REPL experience with:
- Tab completion for Rust code (variables, keywords, methods)
- Multi-line input with smart continuation
- Persistent history
- Ctrl+C handling

Falls back gracefully when prompt_toolkit is unavailable or in non-TTY environments.

Environment variables:
- FERRUMPY_SIMPLE_MODE=1: Force simple mode (no prompt_toolkit)
- TERM=dumb: Also disables prompt_toolkit (for expect scripts)
"""

import sys
import os
from typing import Optional, List, Dict, Any, Callable

# Try to import vendored prompt_toolkit
_HAS_PROMPT_TOOLKIT = False
try:
    # Add vendor directory to path
    _vendor_dir = os.path.join(os.path.dirname(__file__), 'vendor')
    if _vendor_dir not in sys.path:
        sys.path.insert(0, _vendor_dir)
    
    from prompt_toolkit import PromptSession
    from prompt_toolkit.history import FileHistory, InMemoryHistory
    from prompt_toolkit.completion import Completer, Completion
    from prompt_toolkit.validation import Validator, ValidationError
    from prompt_toolkit.key_binding import KeyBindings
    from prompt_toolkit.styles import Style
    from prompt_toolkit.document import Document
    _HAS_PROMPT_TOOLKIT = True
except ImportError:
    pass


def _should_use_enhanced_mode() -> bool:
    """
    Check if enhanced mode should be used.
    
    Returns False if:
    - prompt_toolkit is not available
    - Not in a TTY
    - FERRUMPY_SIMPLE_MODE=1 is set
    - TERM=dumb (common in expect scripts)
    """
    if not _HAS_PROMPT_TOOLKIT:
        return False
    
    # Check environment variables for forced simple mode
    if os.environ.get('FERRUMPY_SIMPLE_MODE', '').lower() in ('1', 'true', 'yes'):
        return False
    
    # Check for dumb terminal (expect scripts often set this)
    if os.environ.get('TERM', '') == 'dumb':
        return False
    
    # Check TTY
    if not (hasattr(sys.stdin, 'isatty') and sys.stdin.isatty()):
        return False
    if not (hasattr(sys.stdout, 'isatty') and sys.stdout.isatty()):
        return False
    
    return True


class RustCompleter(Completer):
    """
    Tab completion for Rust code using evcxr's completion engine.
    
    Falls back to variable name completion if evcxr completions fail.
    """
    
    # Rust keywords for fallback completion
    RUST_KEYWORDS = [
        "let", "mut", "fn", "struct", "impl", "pub", "use", "mod",
        "if", "else", "match", "for", "while", "loop", "break", "continue",
        "return", "true", "false", "self", "Self", "const", "static",
        "trait", "type", "where", "async", "await", "move", "ref",
        "Vec", "String", "Option", "Result", "Some", "None", "Ok", "Err",
    ]
    
    # REPL commands
    COMMANDS = [":q", ":quit", ":exit", ":vars", ":help", ":clear", 
                ":type", ":t", ":dep", ":restart"]
    
    def __init__(self, session=None, snapshot_vars: Optional[List[str]] = None):
        """
        Initialize the completer.
        
        Args:
            session: PyReplSession instance for Rust completions
            snapshot_vars: List of snapshot variable names for fallback completion
        """
        self.session = session
        self.snapshot_vars = snapshot_vars or []
    
    def get_completions(self, document: 'Document', complete_event) -> 'Completion':
        """Get completions for the current document."""
        text = document.text_before_cursor
        
        # Handle command completions
        if text.startswith(':'):
            word = text
            for cmd in self.COMMANDS:
                if cmd.startswith(word):
                    yield Completion(cmd, start_position=-len(word), display_meta="command")
            return
        
        # Try evcxr completions first
        if self.session is not None:
            try:
                result = self.session.completions(document.text, document.cursor_position)
                if result:
                    completions = result.get("completions", [])
                    start_offset = result.get("start_offset")
                    
                    if completions:
                        # Calculate replacement position for prompt_toolkit (relative to cursor)
                        if start_offset is not None and start_offset <= document.cursor_position:
                            start_position = start_offset - document.cursor_position
                        else:
                            start_position = -len(document.get_word_before_cursor())
                        
                        for item in completions:
                            # 'item' is now a dict with 'code', 'label', 'kind', 'detail'
                            code = item["code"]
                            label = item.get("label", code)
                            kind = item.get("kind", "")
                            detail = item.get("detail", "")
                            
                            # Format metadata (shown on the right)
                            display_meta = ""
                            if kind and detail:
                                display_meta = f"{kind}: {detail}"
                            elif kind:
                                display_meta = kind
                            elif detail:
                                display_meta = detail
                                
                            yield Completion(
                                code, 
                                start_position=start_position,
                                display=label,
                                display_meta=display_meta
                            )
                        return
            except Exception:
                pass  # Fall back to simple completion
        
        # Fallback: simple word-based completion
        words = text.split()
        word = words[-1] if words else ""
        
        if not word:
            return
        
        # Combine all completion sources
        all_items = self.snapshot_vars + self.RUST_KEYWORDS
        seen = set()
        
        for item in all_items:
            if item.startswith(word) and item not in seen:
                seen.add(item)
                meta = "variable" if item in self.snapshot_vars else "keyword"
                yield Completion(item, start_position=-len(word), display_meta=meta)


class RustValidator(Validator):
    """
    Validate input for multi-line continuation.
    
    Checks for unclosed braces, parentheses, strings, etc.
    Allows double-newline to force submission (escape hatch).
    """
    
    def __init__(self, session=None):
        self.session = session

    def validate(self, document: 'Document'):
        """Validate the document, raising ValidationError if incomplete."""
        text = document.text
        
        # Empty is valid
        if not text.strip():
            return
        
        # Double newline forces submit (escape hatch, like evcxr_repl)
        if text.endswith('\n\n'):
            return
        
        # Use Rust-side lexical scanner for accurate validation
        if self.session is not None:
            try:
                validity = self.session.fragment_validity(text)
                if validity == "Incomplete":
                    raise ValidationError(
                        message="Incomplete input: waiting for more code (or double Enter to force)",
                        cursor_position=len(text)
                    )
                elif validity == "Invalid":
                    # We don't necessarily block invalid code (the REPL execution will show error)
                    # but we can provide feedback here if we wanted to.
                    pass
            except Exception:
                # Fallback to simple brace counting if Rust call fails
                self._fallback_validate(text)
        else:
            self._fallback_validate(text)

    def _fallback_validate(self, text):
        # Check brace balance (simplified fallback)
        opens = text.count('{') + text.count('(') + text.count('[')
        closes = text.count('}') + text.count(')') + text.count(']')
        
        if opens > closes:
            raise ValidationError(
                message=f"Incomplete input: unclosed braces ({opens} open, {closes} close)",
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
            )


def create_enhanced_repl(
    session,
    snapshot_vars: Optional[Dict[str, str]] = None,
    history_file: Optional[str] = None,
) -> Optional['PromptSession']:
    """
    Create an enhanced REPL session with prompt_toolkit.
    
    Args:
        session: PyReplSession instance
        snapshot_vars: Dict of variable name -> type name
        history_file: Path to history file (default: ~/.cache/ferrumpy/repl_history)
    
    Returns:
        PromptSession if successful, None if unavailable
    """
    if not _should_use_enhanced_mode():
        return None
    
    # History
    if history_file is None:
        cache_dir = os.path.expanduser("~/.cache/ferrumpy")
        os.makedirs(cache_dir, exist_ok=True)
        history_file = os.path.join(cache_dir, "repl_history")
    
    try:
        history = FileHistory(history_file)
    except Exception:
        history = InMemoryHistory()
    
    # Completer
    var_names = list(snapshot_vars.keys()) if snapshot_vars else []
    completer = RustCompleter(session=session, snapshot_vars=var_names)
    
    # Validator for multi-line
    validator = RustValidator(session=session)
    
    # Key bindings
    bindings = KeyBindings()
    ctrl_c_count = [0]
    
    @bindings.add('c-c')
    def handle_ctrl_c(event):
        """Handle Ctrl+C - clear line on first press, show message on second."""
        ctrl_c_count[0] += 1
        if ctrl_c_count[0] >= 2:
            event.app.output.write("\n(Use :q to exit)\n")
            ctrl_c_count[0] = 0
        else:
            event.current_buffer.reset()

    @bindings.add('enter')
    def handle_enter(event):
        """
        Smart Enter handling:
        1. If input is valid/complete, submit it.
        2. If input is incomplete (e.g. unclosed brace), insert a newline.
        3. If ends with double newline, force submit (escape hatch).
        """
        buffer = event.current_buffer
        text = buffer.text
        
        # Escape hatch: double newline forces submission
        if text.endswith('\n'):
            buffer.validate_and_handle()
            return

        # Check validity
        try:
            validator.validate(buffer.document)
            # Valid/Complete -> Submit
            buffer.validate_and_handle()
        except ValidationError:
            # Incomplete -> Newline
            buffer.newline()
    
    # Style
    style = Style.from_dict({
        'prompt': '#00aa00 bold',
        'continuation': '#888888',
    })
    
    # Create session
    try:
        prompt_session = PromptSession(
            history=history,
            completer=completer,
            validator=validator,
            validate_while_typing=False,
            multiline=True,
            prompt_continuation=lambda width, line_number, is_soft_wrap: '... ',
            key_bindings=bindings,
            enable_history_search=True,
            style=style,
            mouse_support=False,
        )
        return prompt_session
    except Exception:
        return None


def run_enhanced_repl(
    session,
    snapshot_data: Dict[str, Any],
    eval_callback: Callable[[str], str],
    output_callback: Callable[[str], None],
    error_callback: Callable[[str], None],
):
    """
    Run the enhanced REPL loop.
    
    Args:
        session: PyReplSession instance
        snapshot_data: Dict with 'types' -> {name: type_name}
        eval_callback: Function to evaluate code, returns result string
        output_callback: Function to display output
        error_callback: Function to display errors
    
    Returns:
        True if loop completed normally, False if fallback needed
    """
    snapshot_vars = snapshot_data.get('types', {})
    
    prompt_session = create_enhanced_repl(session, snapshot_vars)
    
    if prompt_session is None:
        return False  # Need fallback
    
    output_callback("=" * 50)
    output_callback("FerrumPy REPL - Enhanced Mode (prompt_toolkit)")
    output_callback("  • Tab completion enabled")
    output_callback("  • Multi-line input (unclosed braces continue)")
    output_callback("  • History search: Ctrl+R")
    output_callback("  • Commands: :q (quit), :vars, :help")
    output_callback("  • Interrupt: Ctrl+C (twice to exit)")
    output_callback("=" * 50 + "\n")
    
    last_interrupt_time = None
    INTERRUPT_TIMEOUT = 2.0  # 2 seconds window for second Ctrl+C
    
    while True:
        try:
            text = prompt_session.prompt(
                ">> ",
                rprompt="[rust]",
            )
            
            # Reset interrupt timer on successful input
            last_interrupt_time = None
            
            text = text.strip()
            
            if not text:
                continue
            
            # Handle commands
            if text in (':q', ':quit', ':exit'):
                output_callback("Exiting REPL...")
                break
            
            if text == ':vars':
                output_callback("Variables:")
                for name, type_name in snapshot_vars.items():
                    output_callback(f"  {name}: {type_name}")
                continue
            
            if text == ':clear':
                # Only clear screen, not REPL state or snapshot variables
                import os
                os.system('clear' if os.name != 'nt' else 'cls')
                continue
            
            if text == ':help':
                output_callback("Commands:")
                output_callback("  :q, :quit, :exit  - Exit REPL")
                output_callback("  :vars             - Show captured variables")
                output_callback("  :help             - Show this help")
                output_callback("  :clear            - Clear screen")
                output_callback("  :type <expr>      - Show type of expression")
                output_callback("\nRust Evaluation:")
                output_callback("  Type any Rust expression to evaluate")
                output_callback("  Multi-line: open braces auto-continue")
                output_callback("  Force submit: press Enter twice")
                output_callback("\nInterrupt:")
                output_callback("  Ctrl+C once      - Stop running code")
                output_callback("  Ctrl+C twice     - Exit REPL")
                continue
            
            # Evaluate Rust code
            try:
                result = eval_callback(text)
                if result:
                    output_callback(result)
            except Exception as e:
                error_callback(str(e))
        
        except KeyboardInterrupt:
            import time
            current_time = time.time()
            
            # Check if this is a second Ctrl+C within timeout
            if last_interrupt_time and (current_time - last_interrupt_time) < INTERRUPT_TIMEOUT:
                output_callback("\n\n(Second Ctrl+C - exiting REPL)")
                break
            
            # First Ctrl+C: interrupt running code
            last_interrupt_time = current_time
            output_callback("\nKeyboardInterrupt")
            output_callback("(Press Ctrl+C again within 2 seconds to exit)")
            
            # Try to interrupt any running evaluation
            try:
                repl_session._session.interrupt()
                output_callback("Evaluation interrupted.")
            except Exception as e:
                output_callback(f"Failed to interrupt: {e}")
            
            continue
        except EOFError:
            output_callback("\n(Ctrl+D - exiting)")
            break
    
    return True


def is_prompt_toolkit_available() -> bool:
    """Check if prompt_toolkit is available and enhanced mode should be used."""
    return _should_use_enhanced_mode()

