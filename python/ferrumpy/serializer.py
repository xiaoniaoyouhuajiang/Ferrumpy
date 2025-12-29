"""
FerrumPy Serializer

Converts LLDB SBValue objects to JSON-serializable dictionaries
for transfer to the evcxr REPL environment.
"""

import json
import re
from typing import Any, Dict, List, Optional, Set

try:
    import lldb
except ImportError:
    lldb = None  # For testing outside LLDB


# Type name normalization rules (LLDB -> Rust source)
# Order matters! More specific patterns should come first
TYPE_NORMALIZATION = [
    # Vec with Global allocator (more specific, must come first)
    (r'^alloc::vec::Vec<(.+),\s*alloc::alloc::Global>$', r'Vec<\1>'),
    # Standard types
    (r'^alloc::string::String$', 'String'),
    (r'^alloc::vec::Vec<(.+)>$', r'Vec<\1>'),
    (r'^core::option::Option<(.+)>$', r'Option<\1>'),
    (r'^core::result::Result<(.+),\s*(.+)>$', r'Result<\1, \2>'),
    (r'^alloc::boxed::Box<(.+)>$', r'Box<\1>'),
    (r'^alloc::sync::Arc<(.+),\s*alloc::alloc::Global>$', r'Arc<\1>'),
    (r'^alloc::sync::Arc<(.+)>$', r'Arc<\1>'),
    (r'^alloc::rc::Rc<(.+),\s*alloc::alloc::Global>$', r'Rc<\1>'),
    (r'^alloc::rc::Rc<(.+)>$', r'Rc<\1>'),
    (r'^&str$', '&str'),
    # HashMap with RandomState
    (r'^std::collections::hash::map::HashMap<(.+),\s*(.+),\s*std::hash::random::RandomState>$', r'HashMap<\1, \2>'),
    (r'^std::collections::hash::map::HashMap<(.+)>$', r'HashMap<\1>'),
]

# C/LLDB type to Rust type mapping
C_TO_RUST_TYPES = {
    # Signed integers
    'int': 'i32',
    'signed int': 'i32',
    'long': 'i64',
    'signed long': 'i64',
    'long long': 'i64',
    'signed long long': 'i64',
    'short': 'i16',
    'signed short': 'i16',
    'signed char': 'i8',
    # Unsigned integers
    'unsigned int': 'u32',
    'unsigned long': 'u64',
    'unsigned long long': 'u64',
    'unsigned short': 'u16',
    'unsigned char': 'u8',
    # Floating point
    'float': 'f32',
    'double': 'f64',
    # Boolean
    '_Bool': 'bool',
    # Char (Rust char is 4 bytes, but LLDB shows as char)
    'char': 'char',
}

# Primitive types that can be directly serialized
# Includes both Rust names and C/LLDB names
PRIMITIVE_TYPES = {
    # Rust types
    'i8', 'i16', 'i32', 'i64', 'i128', 'isize',
    'u8', 'u16', 'u32', 'u64', 'u128', 'usize',
    'f32', 'f64',
    'bool', 'char',
    # C/LLDB equivalents
    'char', 'signed char', 'unsigned char',
    'short', 'unsigned short',
    'int', 'unsigned int',
    'long', 'unsigned long',
    'long long', 'unsigned long long',
    'float', 'double',
    '_Bool',
}


def normalize_type_name(lldb_type: str) -> str:
    """Convert LLDB type name to Rust source type name."""
    # First, map C types to Rust types
    if lldb_type in C_TO_RUST_TYPES:
        return C_TO_RUST_TYPES[lldb_type]
    
    # Try each normalization rule in order
    for pattern, replacement in TYPE_NORMALIZATION:
        if re.match(pattern, lldb_type):
            result = re.sub(pattern, replacement, lldb_type)
            # Recursively normalize inner types
            return _normalize_inner_types(result)
    
    # For user types, extract the last component
    # e.g., "my_crate::models::User" -> "User"
    if '::' in lldb_type:
        parts = lldb_type.split('::')
        # Keep generics if present
        last = parts[-1]
        return last
    
    return lldb_type


def _normalize_inner_types(type_str: str) -> str:
    """Recursively normalize types inside generics."""
    # Replace C types in generic parameters
    for c_type, rust_type in C_TO_RUST_TYPES.items():
        # Match as whole word to avoid partial replacements
        type_str = re.sub(rf'\b{re.escape(c_type)}\b', rust_type, type_str)
    return type_str


def is_primitive(type_name: str) -> bool:
    """Check if type is a primitive that can be directly serialized."""
    # Strip any references
    clean = type_name.lstrip('&').strip()
    return clean in PRIMITIVE_TYPES


def serialize_frame(frame) -> Dict[str, Any]:
    """
    Serialize all local variables in a frame to JSON.
    
    Returns:
        {
            "variables": {
                "user": { ... serialized value ... },
                "items": { ... },
            },
            "types": {
                "user": "User",
                "items": "Vec<i32>",
            }
        }
    """
    if frame is None or not frame.IsValid():
        return {"variables": {}, "types": {}}
    
    variables = {}
    types = {}
    visited = set()  # Track visited addresses to avoid cycles
    
    for var in frame.GetVariables(True, True, False, True):
        if not var.IsValid():
            continue
        
        name = var.GetName()
        if name is None or name.startswith('$'):
            continue
        
        try:
            value = value_to_json(var, visited)
            variables[name] = value
            types[name] = normalize_type_name(var.GetType().GetName())
        except Exception as e:
            # Skip variables that can't be serialized
            variables[name] = {"__error__": str(e)}
            types[name] = "?"
    
    return {"variables": variables, "types": types}


def value_to_json(value, visited: Optional[Set[int]] = None, depth: int = 0) -> Any:
    """
    Convert an LLDB SBValue to a JSON-serializable value.
    """
    if visited is None:
        visited = set()
    
    if not value.IsValid():
        return None
    
    # Limit recursion depth
    if depth > 20:
        return {"__truncated__": "max depth"}
    
    type_name = value.GetType().GetName()
    
    # Check for cycles only on heap pointers (not stack locals)
    # Heap addresses are typically very high, stack addresses are lower
    addr = value.GetLoadAddress()
    # Only track if it looks like a heap pointer (has ptr child or is smart pointer type)
    is_ptr_type = any(p in type_name for p in ['Arc<', 'Rc<', 'Box<', '*const', '*mut'])
    if is_ptr_type and addr != 0:
        if addr in visited:
            return {"__cycle__": f"0x{addr:x}"}
        visited.add(addr)
    
    # Handle primitives
    if is_primitive(type_name):
        return _serialize_primitive(value)
    
    # Handle String
    if 'String' in type_name and 'alloc::string::String' in type_name:
        return _serialize_string(value)
    
    # Handle &str
    if type_name == '&str':
        return _serialize_str_ref(value)
    
    # Handle Vec
    if 'Vec<' in type_name:
        return _serialize_vec(value, visited, depth)
    
    # Handle Option
    if 'Option<' in type_name:
        return _serialize_option(value, visited, depth)
    
    # Handle Result
    if 'Result<' in type_name:
        return _serialize_result(value, visited, depth)
    
    # Handle Box/Arc/Rc (smart pointers)
    if any(p in type_name for p in ['Box<', 'Arc<', 'Rc<']):
        return _serialize_smart_pointer(value, visited, depth)
    
    # Default: serialize as struct
    return _serialize_struct(value, visited, depth)


def _serialize_primitive(value) -> Any:
    """Serialize a primitive type."""
    type_name = value.GetType().GetName()
    
    # Try to get the value directly
    val_str = value.GetValue()
    if val_str is None:
        return None
    
    # Parse based on type
    if type_name in ('bool', '_Bool'):
        return val_str.lower() == 'true'
    elif type_name == 'char':
        # Remove quotes if present
        return val_str.strip("'")
    elif type_name in ('f32', 'f64', 'float', 'double'):
        return float(val_str)
    else:
        # Integer types - try to parse
        try:
            return int(val_str, 0)  # 0 allows hex/octal
        except ValueError:
            # Maybe it's a float in disguise
            try:
                return float(val_str)
            except:
                return val_str



def _serialize_string(value) -> str:
    """Serialize a Rust String."""
    # Try LLDB summary first
    summary = value.GetSummary()
    if summary:
        # Remove surrounding quotes
        return summary.strip('"')
    
    # Fallback: read from memory
    vec = value.GetChildMemberWithName('vec')
    if not vec.IsValid():
        return ""
    
    len_child = vec.GetChildMemberWithName('len')
    if not len_child.IsValid():
        return ""
    
    length = len_child.GetValueAsUnsigned()
    if length == 0:
        return ""
    
    # Get data pointer
    buf = vec.GetChildMemberWithName('buf')
    if not buf.IsValid():
        return f"<String len={length}>"
    
    # Try to read the actual bytes
    try:
        from .providers import _find_pointer_in_buf
        ptr = _find_pointer_in_buf(buf)
        if ptr and ptr.IsValid():
            ptr_addr = ptr.GetValueAsUnsigned()
            if ptr_addr:
                error = lldb.SBError()
                data = value.GetProcess().ReadMemory(ptr_addr, min(length, 1024), error)
                if not error.Fail():
                    return data.decode('utf-8', errors='replace')
    except:
        pass
    
    return f"<String len={length}>"


def _serialize_str_ref(value) -> str:
    """Serialize a &str reference."""
    summary = value.GetSummary()
    if summary:
        return summary.strip('"')
    return "<&str>"


def _serialize_vec(value, visited: Set[int], depth: int = 0) -> List[Any]:
    """Serialize a Vec<T>."""
    len_child = value.GetChildMemberWithName('len')
    if not len_child.IsValid():
        return []
    
    length = len_child.GetValueAsUnsigned()
    if length == 0:
        return []
    
    # Get element type
    vec_type = value.GetType()
    elem_type = vec_type.GetTemplateArgumentType(0)
    if not elem_type.IsValid():
        return [f"<{length} elements>"]
    
    elem_size = elem_type.GetByteSize()
    
    # Get data pointer
    buf = value.GetChildMemberWithName('buf')
    if not buf.IsValid():
        return [f"<{length} elements>"]
    
    try:
        from .providers import _find_pointer_in_buf
        ptr = _find_pointer_in_buf(buf)
        if not ptr or not ptr.IsValid():
            return [f"<{length} elements>"]
        
        ptr_addr = ptr.GetValueAsUnsigned()
        if ptr_addr == 0:
            return []
        
        # Serialize elements (limit to 100 for performance)
        elements = []
        max_elements = min(length, 100)
        
        for i in range(max_elements):
            addr = ptr_addr + i * elem_size
            elem = value.CreateValueFromAddress(f"[{i}]", addr, elem_type)
            elements.append(value_to_json(elem, visited, depth+1))
        
        if length > max_elements:
            elements.append(f"... ({length - max_elements} more)")
        
        return elements
    except:
        return [f"<{length} elements>"]


def _serialize_option(value, visited: Set[int], depth: int = 0) -> Optional[Any]:
    """Serialize an Option<T>."""
    type_name = value.GetType().GetName()
    
    # Check for None variant
    if type_name.endswith('::None'):
        return None
    
    if type_name.endswith('::Some'):
        # Get the inner value
        inner = value.GetChildAtIndex(0)
        if inner.IsValid():
            return value_to_json(inner, visited, depth+1)
        return None
    
    # Check discriminant
    variants = value.GetChildMemberWithName('$variants$')
    if variants.IsValid():
        # Modern niche-optimized Option
        discr = value.GetChildMemberWithName('$discr$')
        if discr.IsValid():
            discr_val = discr.GetValueAsUnsigned()
            if discr_val == 0:
                return None
    
    # Try to get Some value
    for i in range(value.GetNumChildren()):
        child = value.GetChildAtIndex(i)
        name = child.GetName() or ""
        if 'Some' in name or '__0' in name:
            return value_to_json(child, visited, depth+1)
    
    # Fallback: check if it looks like None
    summary = value.GetSummary()
    if summary and 'None' in summary:
        return None
    
    return {"__option__": "unknown"}


def _serialize_result(value, visited: Set[int], depth: int = 0) -> Dict[str, Any]:
    """Serialize a Result<T, E>."""
    type_name = value.GetType().GetName()
    
    if type_name.endswith('::Ok'):
        inner = value.GetChildAtIndex(0)
        return {"Ok": value_to_json(inner, visited, depth+1) if inner.IsValid() else None}
    
    if type_name.endswith('::Err'):
        inner = value.GetChildAtIndex(0)
        return {"Err": value_to_json(inner, visited, depth+1) if inner.IsValid() else None}
    
    # Try to determine variant from structure
    summary = value.GetSummary()
    if summary:
        if 'Ok' in summary:
            return {"Ok": summary}
        if 'Err' in summary:
            return {"Err": summary}
    
    return {"__result__": "unknown"}


def _serialize_smart_pointer(value, visited: Set[int], depth: int = 0) -> Any:
    """Serialize Box/Arc/Rc by dereferencing."""
    # Try to get the inner value
    ptr = value.GetChildMemberWithName('ptr')
    if ptr.IsValid():
        pointer = ptr.GetChildMemberWithName('pointer')
        if pointer.IsValid():
            inner = pointer.Dereference()
            if inner.IsValid():
                data = inner.GetChildMemberWithName('data')
                if data.IsValid():
                    return value_to_json(data, visited, depth+1)
                return value_to_json(inner, visited, depth+1)
    
    # Fallback: try first child
    if value.GetNumChildren() > 0:
        child = value.GetChildAtIndex(0)
        deref = child.Dereference()
        if deref.IsValid():
            return value_to_json(deref, visited, depth+1)
    
    return {"__ptr__": "opaque"}


def _serialize_struct(value, visited: Set[int], depth: int = 0) -> Dict[str, Any]:
    """Serialize a struct as a dictionary of fields."""
    result = {}
    
    num_children = value.GetNumChildren()
    for i in range(min(num_children, 50)):  # Limit fields
        child = value.GetChildAtIndex(i)
        if not child.IsValid():
            continue
        
        name = child.GetName()
        if name is None:
            name = f"_{i}"
        
        # Skip internal fields
        if name.startswith('$'):
            continue
        
        try:
            result[name] = value_to_json(child, visited, depth+1)
        except Exception as e:
            result[name] = {"__error__": str(e)}
    
    if num_children > 50:
        result["__truncated__"] = f"{num_children - 50} more fields"
    
    return result


def to_json_string(data: Any, indent: int = 2) -> str:
    """Convert serialized data to JSON string."""
    return json.dumps(data, indent=indent, ensure_ascii=False, default=str)


# For testing
if __name__ == "__main__":
    # Test type normalization
    tests = [
        ("alloc::string::String", "String"),
        ("alloc::vec::Vec<i32>", "Vec<i32>"),
        ("alloc::vec::Vec<i32, alloc::alloc::Global>", "Vec<i32>"),
        ("core::option::Option<i32>", "Option<i32>"),
        ("rust_sample::User", "User"),
        ("my_crate::models::Config", "Config"),
    ]
    
    for input_type, expected in tests:
        result = normalize_type_name(input_type)
        status = "✓" if result == expected else "✗"
        print(f"{status} {input_type} -> {result} (expected: {expected})")
