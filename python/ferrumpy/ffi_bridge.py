"""
FFI Bridge to ferrumpy-core (pyo3)

Direct Python binding to Rust expression evaluator, replacing subprocess communication.
"""
from typing import Dict, Optional, Any

# Try to import pyo3 module
_CORE_MODULE = None

def _get_core():
    """Lazy load ferrumpy_core module."""
    global _CORE_MODULE
    if _CORE_MODULE is not None:
        return _CORE_MODULE
    
    try:
        from . import ferrumpy_core
        _CORE_MODULE = ferrumpy_core
        return ferrumpy_core
    except ImportError:
        # FFI not available, return None
        return None


def eval_expression_ffi(expr: str, variables: Dict[str, Dict]) -> Optional[Dict]:
    """
    Evaluate expression using pyo3 FFI.
    
    Args:
        expr: Rust expression string
        variables: Dict of variable name -> {"type": type_name, "value": value_str}
    
    Returns:
        Dict with "value" and "type" keys, or None if FFI not available
    """
    core = _get_core()
    if core is None:
        return None
    
    try:
        result = core.eval_expression(expr, variables)
        return {"value": result["value"], "value_type": result["type"]}
    except ValueError as e:
        return {"error": str(e)}
    except RuntimeError as e:
        return {"error": str(e)}
    except Exception as e:
        return {"error": f"FFI error: {e}"}


def parse_expression_ffi(expr: str) -> Optional[str]:
    """
    Parse expression to AST JSON using pyo3 FFI.
    
    Returns:
        JSON string of AST, or None if FFI not available
    """
    core = _get_core()
    if core is None:
        return None
    
    try:
        return core.parse_expression(expr)
    except Exception:
        return None


def is_ffi_available() -> bool:
    """Check if FFI module is available."""
    return _get_core() is not None
