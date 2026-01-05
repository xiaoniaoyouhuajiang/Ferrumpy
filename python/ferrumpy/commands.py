"""
FerrumPy LLDB Commands with Tab Completion Support

Provides the 'ferrumpy' command with subcommands and native Tab completion.
Uses LLDB's ParsedCommand API for completion support.
"""

import shlex

import lldb

from .path_resolver import PathResolutionError, resolve_path
from .providers import format_value


def register_commands(debugger: lldb.SBDebugger):
    """Register ferrumpy commands with LLDB."""
    # Register pp command with variable-path completion (native LLDB completion)
    # -C variable-path enables Tab completion for variable paths like "user.name"
    debugger.HandleCommand(
        'command script add -C variable-path -f ferrumpy.commands.ferrumpy_pp_command ferrumpy-pp'
    )

    # Register main ferrumpy command (function-based, for subcommands)
    debugger.HandleCommand(
        'command script add -f ferrumpy.commands.ferrumpy_command ferrumpy'
    )


class FerrumPyPPCommand:
    """
    Pretty print command with Tab completion.

    Usage: ferrumpy-pp <path>

    Tab completion works for variable names and field paths.
    """

    def __init__(self, debugger, internal_dict):
        self.debugger = debugger

    def __call__(self, debugger, command, exe_ctx, result):
        """Execute the pp command."""
        args = shlex.split(command) if command else []

        if not args:
            result.SetError("Usage: ferrumpy-pp <path>")
            return

        frame = exe_ctx.GetFrame()
        if not frame.IsValid():
            result.SetError("No valid frame selected. Are you stopped at a breakpoint?")
            return

        path = args[0]
        expand = "--expand" in args

        try:
            value = resolve_path(frame, path)
            formatted = format_value(value, expand=expand)
            type_name = value.GetType().GetName()
            result.AppendMessage(f"({type_name}) {formatted}")
        except PathResolutionError as e:
            result.SetError(str(e))

    def get_short_help(self):
        return "Pretty print a Rust variable or path expression with Tab completion"

    def get_long_help(self):
        return """
Usage: ferrumpy-pp <path> [--expand]

Pretty print a Rust variable or path expression.

Arguments:
    <path>      Variable path (e.g., user, user.name, arr[0].field)

Options:
    --expand    Show expanded internal structure

Examples:
    ferrumpy-pp my_string
    ferrumpy-pp config.database.host
    ferrumpy-pp users[0].name --expand

Note: Use Tab for variable name completion.
"""

    def get_flags(self):
        """Return command flags (for LLDB command parsing)."""
        return 0

    def handle_completion(self, current_line, cursor_pos, exe_ctx):
        """
        Handle Tab completion for the command.

        This method is called when user presses Tab.
        Returns a dict with 'values' and optionally 'descriptions'.
        """
        # Extract the text being completed
        # current_line is the full line, e.g., "ferrumpy-pp us"
        parts = current_line.split()
        if len(parts) < 2:
            # No argument yet, complete variable names
            prefix = ""
        else:
            prefix = parts[-1]

        frame = exe_ctx.GetFrame()
        if not frame.IsValid():
            return {"values": []}

        completions = []
        descriptions = []

        if "." in prefix:
            # Field completion
            base_path = prefix.rsplit(".", 1)[0]
            partial_field = prefix.rsplit(".", 1)[1] if "." in prefix else ""

            try:
                value = resolve_path(frame, base_path)
                type_obj = value.GetType()

                # Get fields
                for i in range(type_obj.GetNumberOfFields()):
                    field = type_obj.GetFieldAtIndex(i)
                    if field.IsValid():
                        field_name = field.GetName()
                        if field_name and field_name.startswith(partial_field):
                            full_path = f"{base_path}.{field_name}"
                            completions.append(full_path)
                            descriptions.append(field.GetType().GetName())
            except Exception:
                pass
        else:
            # Variable name completion
            variables = frame.GetVariables(True, True, False, True)
            for var in variables:
                name = var.GetName()
                if name and name.startswith(prefix):
                    completions.append(name)
                    descriptions.append(var.GetType().GetName())

        return {
            "values": completions,
            "descriptions": descriptions
        }


def ferrumpy_pp_command(
    debugger: lldb.SBDebugger,
    command: str,
    result: lldb.SBCommandReturnObject,
    internal_dict: dict
):
    """
    Pretty print command with native Tab completion.

    This function is registered with -C variable-path for LLDB native completion.
    """
    args = shlex.split(command) if command else []

    if not args:
        result.SetError("Usage: ferrumpy-pp <path> [--expand] [--deep] [--addr]")
        return

    target = debugger.GetSelectedTarget()
    process = target.GetProcess()
    thread = process.GetSelectedThread()
    frame = thread.GetSelectedFrame()

    if not frame.IsValid():
        result.SetError("No valid frame selected. Are you stopped at a breakpoint?")
        return

    # Extract flags
    path = args[0]
    expand = "--expand" in args
    deep = "--deep" in args
    show_addr = "--addr" in args

    try:
        value = resolve_path(frame, path)
        from .providers import FormatOptions
        options = FormatOptions(expand=expand, deep=deep, show_addr=show_addr)
        formatted = format_value(value, options=options)
        type_name = value.GetType().GetName()
        result.AppendMessage(f"({type_name}) {formatted}")
    except PathResolutionError as e:
        result.SetError(str(e))


# =============================================================================
# Original function-based command (kept for ferrumpy subcommands)
# =============================================================================

def ferrumpy_command(
    debugger: lldb.SBDebugger,
    command: str,
    result: lldb.SBCommandReturnObject,
    internal_dict: dict
):
    """Main ferrumpy command handler."""
    args = shlex.split(command)

    if not args:
        args = ["help"]

    subcommand = args[0]
    subargs = args[1:]

    target = debugger.GetSelectedTarget()
    process = target.GetProcess()
    thread = process.GetSelectedThread()
    frame = thread.GetSelectedFrame()

    if not frame.IsValid():
        result.SetError("No valid frame selected. Are you stopped at a breakpoint?")
        return

    if subcommand == "help":
        _cmd_help(result)
    elif subcommand == "locals":
        _cmd_locals(frame, result, subargs)
    elif subcommand == "pp":
        _cmd_pp(frame, result, subargs)
    elif subcommand == "args":
        _cmd_args(frame, result)
    elif subcommand == "complete":
        _cmd_complete(frame, result, subargs)
    elif subcommand == "type":
        _cmd_type(frame, result, subargs)
    elif subcommand == "eval":
        _cmd_eval(frame, result, subargs)
    elif subcommand == "repl":
        _cmd_repl(frame, result, subargs, debugger)
    else:
        result.SetError(f"Unknown subcommand: {subcommand}. Try 'ferrumpy help'")


def _cmd_help(result: lldb.SBCommandReturnObject):
    """Show help message."""
    help_text = """
FerrumPy - Rust Debugging Enhanced

USAGE:
    ferrumpy <subcommand> [options]
    ferrumpy-pp <path>          (with Tab completion!)

SUBCOMMANDS:
    locals              Pretty print all local variables
    args                Pretty print function arguments
    pp <path>           Pretty print a variable or path expression
    complete <prefix>   Get completions for a path prefix
    type <expr>         Get type info for an expression
    eval <expr>         Evaluate an expression (Rust syntax)
    repl [project_path] Start interactive REPL with current variables
    help                Show this help message

TAB COMPLETION:
    Use 'ferrumpy-pp' for native Tab completion:
    ferrumpy-pp use<Tab>  → ferrumpy-pp user
    ferrumpy-pp user.<Tab> → shows fields

OPTIONS:
    --raw               Show raw LLDB output instead of pretty print
    --expand            Expand internal structure details

EXAMPLES:
    ferrumpy locals
    ferrumpy pp my_vec
    ferrumpy-pp config.database.host  (with Tab completion)
"""
    result.AppendMessage(help_text.strip())


def _cmd_locals(
    frame: lldb.SBFrame,
    result: lldb.SBCommandReturnObject,
    args: list
):
    """Pretty print all local variables."""
    show_raw = "--raw" in args
    expand = "--expand" in args

    variables = frame.GetVariables(
        True,   # arguments
        True,   # locals
        False,  # statics
        True    # in_scope_only
    )

    if variables.GetSize() == 0:
        result.AppendMessage("No local variables in current scope.")
        return

    output_lines = []
    for var in variables:
        name = var.GetName()
        if show_raw:
            output_lines.append(f"{name} = {var}")
        else:
            formatted = format_value(var, expand=expand)
            output_lines.append(f"{name} = {formatted}")

    result.AppendMessage("\n".join(output_lines))


def _cmd_args(frame: lldb.SBFrame, result: lldb.SBCommandReturnObject):
    """Pretty print function arguments."""
    variables = frame.GetVariables(
        True,   # arguments
        False,  # locals
        False,  # statics
        True    # in_scope_only
    )

    if variables.GetSize() == 0:
        result.AppendMessage("No arguments for current function.")
        return

    output_lines = []
    for var in variables:
        name = var.GetName()
        formatted = format_value(var)
        output_lines.append(f"{name} = {formatted}")

    result.AppendMessage("\n".join(output_lines))


def _cmd_pp(
    frame: lldb.SBFrame,
    result: lldb.SBCommandReturnObject,
    args: list
):
    """Pretty print a specific path expression."""
    if not args:
        result.SetError("Usage: ferrumpy pp <path> [--expand] [--deep] [--addr]")
        return

    # Extract flags
    show_raw = "--raw" in args
    expand = "--expand" in args
    deep = "--deep" in args
    show_addr = "--addr" in args
    path_args = [a for a in args if not a.startswith("--")]

    if not path_args:
        result.SetError("Usage: ferrumpy pp <path> [--expand] [--deep] [--addr]")
        return

    path = path_args[0]

    try:
        value = resolve_path(frame, path)

        if show_raw:
            result.AppendMessage(str(value))
        else:
            from .providers import FormatOptions
            options = FormatOptions(expand=expand, deep=deep, show_addr=show_addr)
            formatted = format_value(value, options=options)
            type_name = value.GetType().GetName()
            result.AppendMessage(f"({type_name}) {formatted}")

    except PathResolutionError as e:
        result.SetError(str(e))


def _cmd_complete(
    frame: lldb.SBFrame,
    result: lldb.SBCommandReturnObject,
    args: list
):
    """Get completions for a path prefix."""
    if not args:
        result.SetError("Usage: ferrumpy complete <prefix>")
        return

    prefix = args[0]

    try:
        from . import bridge

        # Get frame info for server
        frame_info = bridge.frame_to_info(frame)

        # Find project root (look for Cargo.toml)
        import os
        line_entry = frame.GetLineEntry()
        if line_entry.IsValid():
            file_spec = line_entry.GetFileSpec()
            if file_spec.IsValid():
                source_dir = os.path.dirname(str(file_spec))
                # Walk up to find Cargo.toml
                current = source_dir
                while current and current != "/":
                    if os.path.exists(os.path.join(current, "Cargo.toml")):
                        break
                    current = os.path.dirname(current)

                if current and current != "/":
                    conn = bridge.get_connection()
                    if conn.initialize(current):
                        completions = conn.complete(frame_info, prefix, len(prefix))
                        if completions:
                            for c in completions:
                                label = c.get("label", "")
                                detail = c.get("detail", "")
                                if detail:
                                    result.AppendMessage(f"{label}: {detail}")
                                else:
                                    result.AppendMessage(label)
                            return

        # Fallback: show local variables matching prefix
        variables = frame.GetVariables(True, True, False, True)
        matches = []
        for var in variables:
            name = var.GetName()
            if name and name.startswith(prefix):
                type_name = var.GetType().GetName()
                matches.append(f"{name}: {type_name}")

        if matches:
            for m in matches:
                result.AppendMessage(m)
        else:
            result.AppendMessage(f"No completions for '{prefix}'")

    except Exception as e:
        result.SetError(f"Completion error: {e}")


def _cmd_type(
    frame: lldb.SBFrame,
    result: lldb.SBCommandReturnObject,
    args: list
):
    """Get type information for an expression."""
    if not args:
        result.SetError("Usage: ferrumpy type <expr>")
        return

    expr = args[0]

    try:
        value = resolve_path(frame, expr)
        type_obj = value.GetType()

        result.AppendMessage(f"Type: {type_obj.GetName()}")
        result.AppendMessage(f"Size: {type_obj.GetByteSize()} bytes")

        # Show fields for structs
        num_fields = type_obj.GetNumberOfFields()
        if num_fields > 0:
            result.AppendMessage("Fields:")
            for i in range(num_fields):
                field = type_obj.GetFieldAtIndex(i)
                if field.IsValid():
                    field_name = field.GetName() or f"[{i}]"
                    field_type = field.GetType().GetName()
                    result.AppendMessage(f"  {field_name}: {field_type}")

    except PathResolutionError as e:
        result.SetError(str(e))


def _cmd_eval(
    frame: lldb.SBFrame,
    result: lldb.SBCommandReturnObject,
    args: list
):
    """Evaluate an expression using Rust syntax."""
    if not args:
        result.SetError("Usage: ferrumpy eval <expr>")
        result.AppendMessage("Examples:")
        result.AppendMessage("  ferrumpy eval 10 + 5")
        result.AppendMessage("  ferrumpy eval x * 2")
        result.AppendMessage("  ferrumpy eval a == b")
        return

    # Join all args as the expression (to handle spaces)
    expr = " ".join(args)

    try:
        # Try FFI first (faster, no subprocess)
        from . import ffi_bridge
        if ffi_bridge.is_ffi_available():
            # Build variables dict from frame locals
            variables = {}
            frame_vars = frame.GetVariables(True, True, False, True)
            for i in range(frame_vars.GetSize()):
                var = frame_vars.GetValueAtIndex(i)
                if var.IsValid():
                    name = var.GetName()
                    type_name = var.GetType().GetName()
                    value = var.GetValue() or ""
                    # Map to simplified types
                    from . import bridge
                    rust_type = bridge._simplify_type_name(type_name)
                    variables[name] = {"type": rust_type, "value": value}

            eval_result = ffi_bridge.eval_expression_ffi(expr, variables)
            if eval_result:
                if "error" in eval_result:
                    result.SetError(eval_result["error"])
                else:
                    value = eval_result.get("value", "")
                    value_type = eval_result.get("value_type", "")
                    result.AppendMessage(f"({value_type}) {value}")
                return

        # Fallback to subprocess (JSON-RPC)
        from . import bridge
        frame_info = bridge.frame_to_info(frame)

        import os
        line_entry = frame.GetLineEntry()
        project_root = None
        if line_entry.IsValid():
            file_spec = line_entry.GetFileSpec()
            if file_spec.IsValid():
                source_dir = os.path.dirname(str(file_spec))
                current = source_dir
                while current and current != "/":
                    if os.path.exists(os.path.join(current, "Cargo.toml")):
                        project_root = current
                        break
                    current = os.path.dirname(current)

        if project_root:
            conn = bridge.get_connection()
            if conn.initialize(project_root):
                eval_result = conn.eval(frame_info, expr)

                if eval_result:
                    if "error" in eval_result:
                        result.SetError(eval_result["error"])
                    else:
                        value = eval_result.get("value", "")
                        value_type = eval_result.get("value_type", "")
                        result.AppendMessage(f"({value_type}) {value}")
                    return

        # Final fallback: use LLDB's expression evaluator
        result.AppendMessage("(Note: using LLDB fallback)")
        sbval = frame.EvaluateExpression(expr)
        if sbval.IsValid():
            result.AppendMessage(str(sbval))
        else:
            result.SetError(f"Failed to evaluate: {expr}")

    except Exception as e:
        result.SetError(f"Eval error: {e}")


def _cmd_repl(
    frame: lldb.SBFrame,
    result: lldb.SBCommandReturnObject,
    args: list,
    debugger: lldb.SBDebugger
):
    """Start an interactive REPL with captured variables."""
    from . import repl_ui
    from .repl import EmbeddedReplSession
    from .serializer import serialize_frame

    # Check for --test flag (non-interactive mode for testing)
    test_mode = "--test" in args
    # Check for --simple flag (force simple mode without prompt_toolkit)
    simple_mode = "--simple" in args

    result.AppendMessage("Starting FerrumPy Embedded REPL...")
    result.AppendMessage("Capturing variables from current frame...")

    # Serialize current frame for display
    data = serialize_frame(frame)
    num_vars = len(data.get('variables', {}))
    result.AppendMessage(f"Captured {num_vars} variables")

    # Show available variables
    result.AppendMessage("\nVariables available:")
    for name, type_name in list(data.get('types', {}).items())[:10]:
        result.AppendMessage(f"  {name}: {type_name}")
    if len(data.get('types', {})) > 10:
        result.AppendMessage(f"  ... and {len(data.get('types', {})) - 10} more")

    # Create embedded REPL session
    try:
        session = EmbeddedReplSession(frame)
        result.AppendMessage("\nInitializing REPL engine...")
        init_result = session.initialize()
        result.AppendMessage(f"REPL ready. {init_result}")

        # In test mode, exit immediately after initialization
        if test_mode:
            result.AppendMessage("\n" + "=" * 50)
            result.AppendMessage("FerrumPy REPL - Type Rust expressions")
            result.AppendMessage("Commands: :q (quit), :vars (show variables)")
            result.AppendMessage("=" * 50 + "\n")
            result.AppendMessage("Test mode: REPL initialized successfully, exiting.")
            result.AppendMessage("REPL session ended.")
            return

        # Try enhanced mode with prompt_toolkit
        if not simple_mode and repl_ui.is_prompt_toolkit_available():
            # Use enhanced REPL
            def eval_callback(code: str) -> str:
                return session.eval(code)

            def output_callback(msg: str):
                print(msg)

            def error_callback(msg: str):
                print(_format_error(msg))

            success = repl_ui.run_enhanced_repl(
                session=session._session,  # Get underlying PyReplSession
                snapshot_data=data,
                eval_callback=eval_callback,
                output_callback=output_callback,
                error_callback=error_callback,
            )

            if success:
                result.AppendMessage("REPL session ended.")
                return
            # Fall through to simple mode if enhanced mode failed

        # Simple mode (fallback)
        result.AppendMessage("\n" + "=" * 50)
        result.AppendMessage("FerrumPy REPL - Simple Mode")
        result.AppendMessage("Commands: :q (quit), :vars (show variables)")
        result.AppendMessage("Tip: Install prompt_toolkit for enhanced features")
        result.AppendMessage("=" * 50 + "\n")

        # Simple interactive loop
        buffer = []
        prev_line_empty = False
        last_interrupt_time = None
        INTERRUPT_TIMEOUT = 2.0  # 2 seconds window for second Ctrl+C

        while True:
            try:
                # Show continuation prompt if buffer not empty
                prompt = ".. " if buffer else ">> "
                print(prompt, end='', flush=True)
                line = input()
                # Reset interrupt timer on successful input
                last_interrupt_time = None
            except KeyboardInterrupt:
                import time
                current_time = time.time()

                # Check if this is a second Ctrl+C within timeout
                if last_interrupt_time and (current_time - last_interrupt_time) < INTERRUPT_TIMEOUT:
                    result.AppendMessage("\n\nExiting REPL...")
                    break

                # First Ctrl+C: interrupt running code
                last_interrupt_time = current_time
                result.AppendMessage("\nKeyboardInterrupt")
                result.AppendMessage("(Press Ctrl+C again within 2 seconds to exit)")

                # Try to interrupt any running evaluation
                try:
                    session._session.interrupt()
                    result.AppendMessage("Evaluation interrupted.")
                except Exception as e:
                    result.AppendMessage(f"Failed to interrupt: {e}")

                # Clear buffer and reset state
                buffer = []
                prev_line_empty = False
                continue
            except EOFError:
                result.AppendMessage("\nExiting REPL...")
                break

            # Special case: handle single line commands even in buffer
            stripped_line = line.strip()
            if not buffer and stripped_line in (':q', ':quit', ':exit'):
                result.AppendMessage("Exiting REPL...")
                break
            if not buffer and stripped_line == ':vars':
                result.AppendMessage("Variables:")
                for name, type_name in data.get('types', {}).items():
                    result.AppendMessage(f"  {name}: {type_name}")
                continue
            if not buffer and stripped_line == ':help':
                result.AppendMessage("Commands:")
                result.AppendMessage("  :q, :quit, :exit  - Exit REPL")
                result.AppendMessage("  :vars             - Show captured variables")
                result.AppendMessage("  :help             - Show this help")
                continue

            buffer.append(line)
            full_code = "\n".join(buffer)

            # Check if code is complete
            try:
                # Force submit on consecutive empty lines
                current_line_empty = line == ""
                if current_line_empty and prev_line_empty and len(buffer) > 1:
                    validity = "Valid"  # Force submit
                else:
                    validity = session._session.fragment_validity(full_code)
                prev_line_empty = current_line_empty
            except Exception:
                validity = "Valid"  # Fallback

            if validity == "Incomplete":
                continue

            # Reset buffer and state for next command
            buffer = []
            prev_line_empty = False
            if not full_code.strip():
                continue

            # Evaluate Rust code
            try:
                eval_result = session.eval(full_code)
                if eval_result:
                    print(eval_result)
            except Exception as e:
                print(_format_error(str(e)))

        result.AppendMessage("REPL session ended.")

    except Exception as e:
        result.SetError(f"Failed to create REPL: {e}")


def _format_error(error_msg: str) -> str:
    """Format error message for better readability."""
    lines = str(error_msg).split('\n')
    formatted = []
    for line in lines[:15]:  # Limit to first 15 lines
        if 'error' in line.lower():
            formatted.append(f"\033[91m{line}\033[0m")  # Red
        elif 'warning' in line.lower():
            formatted.append(f"\033[93m{line}\033[0m")  # Yellow
        elif line.strip().startswith('-->') or line.strip().startswith('|'):
            formatted.append(f"\033[90m{line}\033[0m")  # Gray
        else:
            formatted.append(line)
    if len(lines) > 15:
        formatted.append(f"\033[90m... ({len(lines) - 15} more lines)\033[0m")
    return '\n'.join(formatted)



def _find_cargo_project(start_path: str) -> str:
    """Find Cargo.toml by walking up the directory tree."""
    import os
    current = start_path
    for _ in range(10):  # Max 10 levels up
        cargo_toml = os.path.join(current, "Cargo.toml")
        if os.path.exists(cargo_toml):
            return current
        parent = os.path.dirname(current)
        if parent == current:
            break
        current = parent
    return None
