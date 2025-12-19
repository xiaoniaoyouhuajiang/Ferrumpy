"""
FerrumPy - A Python-like debugging experience for Rust

This module provides LLDB extensions for better Rust debugging:
- Pretty Printers for common Rust types
- Structured path access (a.b[0].c)
- Enhanced locals/args display
"""

__version__ = "0.1.0"

# lldb module is only available when running inside LLDB
try:
    import lldb
    _HAS_LLDB = True
except ImportError:
    _HAS_LLDB = False
    lldb = None

# These imports work without lldb for standalone testing
from . import path_resolver

# These require lldb
if _HAS_LLDB:
    from . import commands
    from . import providers


def __lldb_init_module(debugger, internal_dict: dict):
    """Called by LLDB when the module is loaded."""
    if not _HAS_LLDB:
        print("ERROR: FerrumPy requires LLDB environment")
        return
    commands.register_commands(debugger)
    providers.register_providers(debugger)
    print(f"FerrumPy v{__version__} loaded. Type 'ferrumpy help' for usage.")
