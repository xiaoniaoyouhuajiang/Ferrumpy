"""
FerrumPy Vendor Directory

This directory contains vendored third-party Python packages to ensure
they are available in LLDB's Python environment without requiring
additional installation.

Vendored packages:
- prompt_toolkit (3.0.52): Terminal UI toolkit for enhanced REPL experience
- wcwidth (0.2.14): Terminal width calculation (dependency of prompt_toolkit)
"""

import os
import sys

# Add vendor directory to Python path for imports
_vendor_dir = os.path.dirname(os.path.abspath(__file__))
if _vendor_dir not in sys.path:
    sys.path.insert(0, _vendor_dir)
