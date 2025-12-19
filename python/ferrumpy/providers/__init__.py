"""
FerrumPy Pretty Print Providers (v2)

Simplified approach that relies more on LLDB's built-in capabilities
and avoids manual memory reading when possible.
"""

import lldb
import re
from typing import Optional, Callable, Dict

# Registry of type patterns to formatter functions
_FORMATTERS: Dict[str, Callable[[lldb.SBValue, bool], str]] = {}


def register_providers(debugger: lldb.SBDebugger):
    """Register type formatters with LLDB."""
    pass


def format_value(value: lldb.SBValue, expand: bool = False, depth: int = 0) -> str:
    """
    Format an SBValue using the appropriate pretty printer.
    """
    if not value.IsValid():
        return "<invalid>"
    
    type_name = value.GetType().GetName()
    
    # Try to find a matching formatter
    for pattern, formatter in _FORMATTERS.items():
        if re.match(pattern, type_name):
            try:
                return formatter(value, expand)
            except Exception as e:
                # Fallback to default if formatter fails
                return _format_default(value, expand, depth)
    
    # Default formatting
    return _format_default(value, expand, depth)


def _format_default(value: lldb.SBValue, expand: bool, depth: int) -> str:
    """Default formatter for unknown types."""
    # Try LLDB's summary first - it often works well
    summary = value.GetSummary()
    if summary:
        return summary
    
    # Try value representation
    val = value.GetValue()
    if val:
        return val
    
    # For compound types, iterate children
    num_children = value.GetNumChildren()
    if num_children > 0:
        if expand:
            return _format_struct_expanded(value, depth)
        else:
            return _format_struct_compact(value)
    
    # Last resort: str() representation
    return str(value)


def _format_struct_compact(value: lldb.SBValue) -> str:
    """Format a struct/enum compactly."""
    type_name = value.GetType().GetName()
    short_name = type_name.split("::")[-1]
    
    num_children = value.GetNumChildren()
    if num_children == 0:
        return short_name
    
    parts = []
    for i in range(min(num_children, 3)):
        child = value.GetChildAtIndex(i)
        if not child.IsValid():
            continue
        child_name = child.GetName() or f"_{i}"
        # Use summary or value, fallback to "..."
        child_val = child.GetSummary() or child.GetValue() or "..."
        parts.append(f"{child_name}: {child_val}")
    
    if num_children > 3:
        parts.append("...")
    
    return f"{short_name} {{ {', '.join(parts)} }}"


def _format_struct_expanded(value: lldb.SBValue, depth: int) -> str:
    """Format a struct/enum with full details."""
    type_name = value.GetType().GetName()
    indent = "  " * depth
    child_indent = "  " * (depth + 1)
    
    lines = [f"{type_name} {{"]
    
    num_children = value.GetNumChildren()
    for i in range(min(num_children, 20)):  # Limit children
        child = value.GetChildAtIndex(i)
        if not child.IsValid():
            continue
        child_name = child.GetName() or f"_{i}"
        child_val = format_value(child, expand=True, depth=depth + 1)
        lines.append(f"{child_indent}{child_name}: {child_val},")
    
    if num_children > 20:
        lines.append(f"{child_indent}... ({num_children - 20} more)")
    
    lines.append(f"{indent}}}")
    return "\n".join(lines)


# =============================================================================
# String Formatters - Using LLDB's summary when available
# =============================================================================

def _format_string(value: lldb.SBValue, expand: bool) -> str:
    """Format alloc::string::String."""
    # First try LLDB's built-in summary
    summary = value.GetSummary()
    if summary:
        if expand:
            # Try to get length info
            vec = value.GetChildMemberWithName("vec")
            if vec.IsValid():
                len_child = vec.GetChildMemberWithName("len")
                if len_child.IsValid():
                    length = len_child.GetValueAsUnsigned()
                    return f"{summary}  (len={length})"
        return summary
    
    # Fallback: try to read via children
    vec = value.GetChildMemberWithName("vec")
    if not vec.IsValid():
        return _format_default(value, expand, 0)
    
    len_child = vec.GetChildMemberWithName("len")
    if not len_child.IsValid():
        return _format_default(value, expand, 0)
    
    length = len_child.GetValueAsUnsigned()
    
    if length == 0:
        return '""'
    
    # Try to get the buffer pointer and read
    buf = vec.GetChildMemberWithName("buf")
    if not buf.IsValid():
        return f'"<{length} bytes>"'
    
    # Navigate to the pointer - layout varies by Rust version
    ptr = _find_pointer_in_buf(buf)
    if not ptr:
        return f'"<{length} bytes>"'
    
    ptr_addr = ptr.GetValueAsUnsigned()
    if ptr_addr == 0:
        return '""'
    
    # Read string data
    error = lldb.SBError()
    process = value.GetProcess()
    max_len = min(length, 256)  # Limit read size
    data = process.ReadMemory(ptr_addr, max_len, error)
    
    if error.Fail():
        return f'"<error reading {length} bytes>"'
    
    try:
        text = data.decode('utf-8')
        text = _escape_string(text)
        if length > 256:
            text += "..."
        if expand:
            return f'"{text}"  (len={length})'
        return f'"{text}"'
    except UnicodeDecodeError:
        return f'"<invalid UTF-8, {length} bytes>"'


def _find_pointer_in_buf(buf: lldb.SBValue) -> Optional[lldb.SBValue]:
    """
    Navigate through RawVec/Unique/NonNull to find the actual data pointer.
    
    TODO: [DWARF-DRIVEN] This function uses hardcoded path patterns which may break
    with different Rust versions. Future versions should use DWARF type information
    to dynamically discover the pointer field regardless of internal structure changes.
    See: references/ferrumpy_technical_spec.md - DWARF-driven type matching.
    
    Known structure variations:
    - Rust 1.70+: buf.inner.ptr.pointer.pointer
    - Rust 1.60+: buf.ptr.pointer.pointer  
    - Older: buf.ptr.pointer
    """
    # Define known path patterns for different Rust versions
    # Each pattern is a list of field names to traverse
    POINTER_PATTERNS = [
        # Rust 1.70+ with RawVecInner
        ["inner", "ptr", "pointer", "pointer"],
        # Rust 1.60+ without RawVecInner
        ["ptr", "pointer", "pointer"],
        # Older versions or simpler wrappers
        ["ptr", "pointer"],
        # Direct pointer (rare)
        ["pointer"],
    ]
    
    for pattern in POINTER_PATTERNS:
        result = _navigate_path(buf, pattern)
        if result is not None and result.IsValid():
            # Verify it looks like a pointer (has non-zero address for non-empty strings)
            return result
    
    return None


def _navigate_path(value: lldb.SBValue, path: list) -> Optional[lldb.SBValue]:
    """
    Navigate a path of field names through an SBValue.
    
    Args:
        value: Starting SBValue
        path: List of field names to traverse
        
    Returns:
        The final SBValue if all fields exist, None otherwise
    """
    current = value
    for field_name in path:
        if not current.IsValid():
            return None
        child = current.GetChildMemberWithName(field_name)
        if not child.IsValid():
            return None
        current = child
    return current


def _escape_string(text: str) -> str:
    """Escape special characters for display."""
    return (text
        .replace('\\', '\\\\')
        .replace('"', '\\"')
        .replace('\n', '\\n')
        .replace('\r', '\\r')
        .replace('\t', '\\t'))


# =============================================================================
# Vec Formatter
# =============================================================================

def _format_vec(value: lldb.SBValue, expand: bool) -> str:
    """
    Format alloc::vec::Vec<T>.
    
    TODO: [DWARF-DRIVEN] This function uses hardcoded structure assumptions.
    Future versions should use DWARF type information for reliable element access.
    """
    # Get length
    len_child = value.GetChildMemberWithName("len")
    if not len_child.IsValid():
        return _format_default(value, expand, 0)
    
    length = len_child.GetValueAsUnsigned()
    
    # Get capacity if available (try multiple paths)
    cap = 0
    buf = value.GetChildMemberWithName("buf")
    if buf.IsValid():
        # Try direct cap
        cap_child = buf.GetChildMemberWithName("cap")
        if cap_child.IsValid():
            cap = cap_child.GetValueAsUnsigned()
        else:
            # Try inner.cap (Rust 1.70+)
            inner = buf.GetChildMemberWithName("inner")
            if inner.IsValid():
                cap_child = inner.GetChildMemberWithName("cap")
                if cap_child.IsValid():
                    # cap might be wrapped in a struct with __0
                    inner_cap = cap_child.GetChildMemberWithName("__0")
                    if inner_cap.IsValid():
                        cap = inner_cap.GetValueAsUnsigned()
                    else:
                        cap = cap_child.GetValueAsUnsigned()
    
    if length == 0:
        if expand:
            return f"[]  (len=0, cap={cap})"
        return "[]"
    
    # Get element type from Vec<T>
    vec_type = value.GetType()
    elem_type = vec_type.GetTemplateArgumentType(0)
    if not elem_type.IsValid():
        return f"[?]  (len={length})"
    
    elem_size = elem_type.GetByteSize()
    
    # Get data pointer using multi-path navigation
    ptr = _find_pointer_in_buf(buf)
    if ptr is None or not ptr.IsValid():
        return f"[... {length} elements]"
    
    ptr_addr = ptr.GetValueAsUnsigned()
    if ptr_addr == 0:
        return "[]"
    
    # Format elements by reading from memory
    elements = []
    max_display = 10 if expand else 5
    
    for i in range(min(length, max_display)):
        addr = ptr_addr + i * elem_size
        elem = value.CreateValueFromAddress(f"[{i}]", addr, elem_type)
        if elem.IsValid():
            elem_str = format_value(elem, expand=False, depth=0)
            elements.append(elem_str)
        else:
            elements.append("?")
    
    if length > max_display:
        elements.append(f"... ({length - max_display} more)")
    
    result = f"[{', '.join(elements)}]"
    if expand:
        result += f"  (len={length}, cap={cap})"
    
    return result


# =============================================================================
# Option/Result Formatters - Simplified
# =============================================================================

def _format_option(value: lldb.SBValue, expand: bool) -> str:
    """Format core::option::Option<T>."""
    type_name = value.GetType().GetName()
    
    # Check type name for variant (when LLDB resolves to specific variant)
    if type_name.endswith("::None"):
        return "None"
    
    if type_name.endswith("::Some"):
        inner = _get_enum_payload(value, 1)  # Some is variant 1
        if inner.IsValid():
            inner_str = format_value(inner, expand=expand)
            return f"Some({inner_str})"
        return "Some(?)"
    
    # Check for $variants$ structure (common in LLDB for Rust enums)
    variants = value.GetChildMemberWithName("$variants$")
    if variants.IsValid():
        # For Option: $variant$1 contains Some value
        variant1 = variants.GetChildMemberWithName("$variant$1")
        if variant1.IsValid():
            discr = variant1.GetChildMemberWithName("$discr$")
            if discr.IsValid():
                discr_val = discr.GetValueAsUnsigned()
                if discr_val == 0:
                    return "None"
                else:
                    # Get the actual value from value.__0
                    val_wrapper = variant1.GetChildMemberWithName("value")
                    if val_wrapper.IsValid():
                        inner = val_wrapper.GetChildMemberWithName("__0")
                        if inner.IsValid():
                            return f"Some({format_value(inner, expand=expand)})"
                    return "Some(?)"
    
    # For simple discriminant at top level
    discr = value.GetChildMemberWithName("$discr$")
    if discr.IsValid():
        discr_val = discr.GetValueAsUnsigned()
        if discr_val == 0:
            return "None"
        inner = _get_enum_payload_simple(value)
        if inner.IsValid():
            return f"Some({format_value(inner, expand=expand)})"
        return "Some(?)"
    
    # Try LLDB summary
    summary = value.GetSummary()
    if summary:
        return summary
    
    return _format_default(value, expand, 0)


def _get_enum_payload_simple(value: lldb.SBValue) -> lldb.SBValue:
    """Get payload from simple enum structure (no $variants$)."""
    # Try 'value' child with __0
    payload = value.GetChildMemberWithName("value")
    if payload.IsValid():
        inner = payload.GetChildMemberWithName("__0")
        if inner.IsValid():
            return inner
        inner = payload.GetChildAtIndex(0)
        if inner.IsValid():
            return inner
        return payload
    
    # Try __0 directly
    inner = value.GetChildMemberWithName("__0")
    if inner.IsValid():
        return inner
    
    return lldb.SBValue()


def _get_enum_payload(value: lldb.SBValue, variant_index: int) -> lldb.SBValue:
    """Get payload from Rust enum with $variants$ structure."""
    variants = value.GetChildMemberWithName("$variants$")
    if variants.IsValid():
        variant = variants.GetChildMemberWithName(f"$variant${variant_index}")
        if variant.IsValid():
            val_wrapper = variant.GetChildMemberWithName("value")
            if val_wrapper.IsValid():
                inner = val_wrapper.GetChildMemberWithName("__0")
                if inner.IsValid():
                    return inner
                return val_wrapper.GetChildAtIndex(0) if val_wrapper.GetNumChildren() > 0 else val_wrapper
    
    # Fallback to simple extraction
    return _get_enum_payload_simple(value)


def _format_result(value: lldb.SBValue, expand: bool) -> str:
    """Format core::result::Result<T, E>."""
    type_name = value.GetType().GetName()
    
    # Check type name for variant
    if type_name.endswith("::Ok"):
        inner = _get_enum_payload(value, 0)
        if inner.IsValid():
            return f"Ok({format_value(inner, expand=expand)})"
        return "Ok(?)"
    
    if type_name.endswith("::Err"):
        inner = _get_enum_payload(value, 1)
        if inner.IsValid():
            return f"Err({format_value(inner, expand=expand)})"
        return "Err(?)"
    
    # Check for $variants$ structure (niche optimization)
    variants = value.GetChildMemberWithName("$variants$")
    if variants.IsValid():
        # For niche-optimized Result<T, E> with E=String:
        # The discriminant stores String's capacity in the high bits when Ok,
        # or String's length when Err. We detect this by:
        # - High $discr$ value (>= 2^63) typically means Ok
        # - Lower $discr$ value means Err (and equals String length)
        # This is a heuristic and may not work for all Result types
        
        variant0 = variants.GetChildMemberWithName("$variant$0")
        variant_err = variants.GetChildMemberWithName("$variant$")
        
        if variant0.IsValid():
            discr = variant0.GetChildMemberWithName("$discr$")
            if discr.IsValid():
                discr_val = discr.GetValueAsUnsigned()
                
                # Heuristic: if discr >= 2^63, it's storing capacity (Ok)
                # if discr is a small positive number, it's storing len (Err)
                NICHE_THRESHOLD = 1 << 62  # Conservative threshold
                
                if discr_val >= NICHE_THRESHOLD:
                    # This is Ok variant - value is in $variant$0.value.__0
                    val_wrapper = variant0.GetChildMemberWithName("value")
                    if val_wrapper.IsValid():
                        inner = val_wrapper.GetChildMemberWithName("__0")
                        if inner.IsValid():
                            return f"Ok({format_value(inner, expand=expand)})"
                    return "Ok(?)"
                else:
                    # This is Err variant - value is in $variant$.value.__0
                    if variant_err.IsValid():
                        val_wrapper = variant_err.GetChildMemberWithName("value")
                        if val_wrapper.IsValid():
                            inner = val_wrapper.GetChildMemberWithName("__0")
                            if inner.IsValid():
                                return f"Err({format_value(inner, expand=expand)})"
                    # Fallback: try $variant$1
                    variant1 = variants.GetChildMemberWithName("$variant$1")
                    if variant1.IsValid():
                        val_wrapper = variant1.GetChildMemberWithName("value")
                        if val_wrapper.IsValid():
                            inner = val_wrapper.GetChildMemberWithName("__0")
                            if inner.IsValid():
                                return f"Err({format_value(inner, expand=expand)})"
                    return "Err(?)"
    
    # For simple discriminant at top level
    discr = value.GetChildMemberWithName("$discr$")
    if discr.IsValid():
        discr_val = discr.GetValueAsUnsigned()
        inner = _get_enum_payload_simple(value)
        if discr_val == 0:
            if inner.IsValid():
                return f"Ok({format_value(inner, expand=expand)})"
            return "Ok(?)"
        else:
            if inner.IsValid():
                return f"Err({format_value(inner, expand=expand)})"
            return "Err(?)"
    
    # Use summary if available
    summary = value.GetSummary()
    if summary:
        return summary
    
    return _format_default(value, expand, 0)


# =============================================================================
# Smart Pointer Formatters
# =============================================================================

def _format_box(value: lldb.SBValue, expand: bool) -> str:
    """Format alloc::boxed::Box<T>."""
    # Try to dereference
    inner = value.Dereference()
    if inner.IsValid():
        inner_str = format_value(inner, expand=expand)
        return f"Box({inner_str})"
    
    # Try first child
    inner = value.GetChildAtIndex(0)
    if inner.IsValid():
        deref = inner.Dereference()
        if deref.IsValid():
            return f"Box({format_value(deref, expand=expand)})"
        return f"Box({format_value(inner, expand=expand)})"
    
    return "Box(?)"


def _format_arc(value: lldb.SBValue, expand: bool) -> str:
    """Format alloc::sync::Arc<T>."""
    # Try to get ptr -> NonNull -> pointer -> ArcInner
    ptr = value.GetChildMemberWithName("ptr")
    if not ptr.IsValid():
        ptr = value.GetChildAtIndex(0)
    
    if ptr.IsValid():
        # Navigate to actual data
        pointer = ptr.GetChildMemberWithName("pointer")
        if pointer.IsValid():
            inner = pointer.Dereference()
            if inner.IsValid():
                data = inner.GetChildMemberWithName("data")
                strong = inner.GetChildMemberWithName("strong")
                weak = inner.GetChildMemberWithName("weak")
                
                if data.IsValid():
                    data_str = format_value(data, expand=expand)
                    if expand and strong.IsValid():
                        s = strong.GetValueAsUnsigned()
                        w = weak.GetValueAsUnsigned() if weak.IsValid() else 0
                        return f"Arc({data_str})  (strong={s}, weak={w})"
                    return f"Arc({data_str})"
    
    # Fallback: show summary or default
    summary = value.GetSummary()
    if summary:
        return f"Arc({summary})"
    
    return "Arc(...)"


def _format_rc(value: lldb.SBValue, expand: bool) -> str:
    """Format alloc::rc::Rc<T>."""
    # Similar to Arc but different structure
    ptr = value.GetChildMemberWithName("ptr")
    if not ptr.IsValid():
        ptr = value.GetChildAtIndex(0)
    
    if ptr.IsValid():
        pointer = ptr.GetChildMemberWithName("pointer")
        if pointer.IsValid():
            inner = pointer.Dereference()
            if inner.IsValid():
                value_child = inner.GetChildMemberWithName("value")
                strong = inner.GetChildMemberWithName("strong")
                
                if value_child.IsValid():
                    data_str = format_value(value_child, expand=expand)
                    if expand and strong.IsValid():
                        s = strong.GetValueAsUnsigned()
                        return f"Rc({data_str})  (strong={s})"
                    return f"Rc({data_str})"
    
    summary = value.GetSummary()
    if summary:
        return f"Rc({summary})"
    
    return "Rc(...)"


# =============================================================================
# HashMap Formatter
# =============================================================================

def _format_hashmap(value: lldb.SBValue, expand: bool) -> str:
    """Format HashMap - just show count for now."""
    # Try to get length from various possible paths
    base = value.GetChildMemberWithName("base")
    if base.IsValid():
        table = base.GetChildMemberWithName("table")
        if table.IsValid():
            items = table.GetChildMemberWithName("items")
            if items.IsValid():
                count = items.GetValueAsUnsigned()
                if count == 0:
                    return "{}"
                return f"{{...}}  (len={count})"
    
    # Alternative layout
    table = value.GetChildMemberWithName("table")
    if table.IsValid():
        items = table.GetChildMemberWithName("items")
        if items.IsValid():
            count = items.GetValueAsUnsigned()
            return f"{{...}}  (len={count})"
    
    return "{...}"


# =============================================================================
# Register Formatters
# =============================================================================

_FORMATTERS = {
    r'^alloc::string::String$': _format_string,
    r'^&str$': _format_string,  # Try same formatter
    r'^alloc::vec::Vec<.+>$': _format_vec,
    r'^core::option::Option<.+>': _format_option,
    r'^core::result::Result<.+>': _format_result,
    r'^std::collections::hash::map::HashMap<.+>': _format_hashmap,
    r'^hashbrown::map::HashMap<.+>': _format_hashmap,
    r'^alloc::sync::Arc<.+>$': _format_arc,
    r'^alloc::rc::Rc<.+>$': _format_rc,
    r'^alloc::boxed::Box<.+>$': _format_box,
}
