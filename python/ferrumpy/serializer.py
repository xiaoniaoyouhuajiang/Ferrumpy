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
    """
    Convert LLDB type name to clean Rust source type name.
    
    This function handles:
    1. C-style type names -> Rust types (int -> i32)
    2. Allocator removal (Vec<i32, Global> -> Vec<i32>)
    3. Module path simplification (alloc::vec::Vec -> Vec)
    4. Crate path extraction (my_crate::User -> User)
    5. Recursive normalization of nested generics
    """
    if not lldb_type:
        return lldb_type
    
    # Step 1: Remove all allocator parameters first (before any other processing)
    lldb_type = _remove_allocators(lldb_type)
    
    # Step 2: Map simple C types to Rust types
    if lldb_type in C_TO_RUST_TYPES:
        return C_TO_RUST_TYPES[lldb_type]
    
    # Step 3: Apply normalization rules (module path simplification)
    for pattern, replacement in TYPE_NORMALIZATION:
        if re.match(pattern, lldb_type):
            result = re.sub(pattern, replacement, lldb_type)
            # Recursively normalize inner types
            return _normalize_inner_types(result)
    
    # Step 4: Handle generic types with C types inside
    if '<' in lldb_type:
        return _normalize_generic_type(lldb_type)
    
    # Step 5: Handle C-style arrays (int[5] -> [i32; 5])
    array_match = re.match(r'^(\w+)\[(\d+)\]$', lldb_type)
    if array_match:
        elem_type = array_match.group(1)
        size = array_match.group(2)
        # Normalize element type (e.g., int -> i32)
        if elem_type in C_TO_RUST_TYPES:
            elem_type = C_TO_RUST_TYPES[elem_type]
        return f"[{elem_type}; {size}]"
    
    # Step 6: For unrecognized types with crate paths, extract last component
    if '::' in lldb_type and not lldb_type.startswith('&'):
        return _extract_type_name(lldb_type)
    
    return lldb_type


def _remove_allocators(type_str: str) -> str:
    """
    Remove allocator parameters from type strings.
    
    Examples:
        Vec<i32, alloc::alloc::Global> -> Vec<i32>
        Vec<Vec<i32, alloc::alloc::Global>, alloc::alloc::Global> -> Vec<Vec<i32>>
        HashMap<String, i32, std::hash::random::RandomState> -> HashMap<String, i32>
    """
    # Patterns for allocator/hasher parameters to remove
    allocator_patterns = [
        r',\s*alloc::alloc::Global',
        r',\s*Global',
        r',\s*std::hash::random::RandomState',
        r',\s*std::collections::hash::map::RandomState',
    ]
    
    result = type_str
    for pattern in allocator_patterns:
        result = re.sub(pattern, '', result)
    
    return result


def _normalize_generic_type(type_str: str) -> str:
    """
    Normalize a generic type by processing outer type and inner types separately.
    
    Examples:
        Vec<int> -> Vec<i32>
        Option<unsigned long> -> Option<u64>
        Result<int, alloc::string::String> -> Result<i32, String>
    """
    # Find the outer type name and generic parameters
    match = re.match(r'^([^<]+)<(.+)>$', type_str)
    if not match:
        return type_str
    
    outer = match.group(1)
    inner = match.group(2)
    
    # Normalize outer type (remove module paths)
    outer = _simplify_module_path(outer)
    
    # Split inner types carefully (handling nested generics)
    inner_types = _split_generic_params(inner)
    
    # Normalize each inner type recursively
    normalized_inner = [normalize_type_name(t.strip()) for t in inner_types]
    
    return f"{outer}<{', '.join(normalized_inner)}>"


def _split_generic_params(params: str) -> List[str]:
    """
    Split generic parameters respecting nested angle brackets.
    
    Examples:
        "i32, String" -> ["i32", "String"]
        "Vec<i32>, Option<String>" -> ["Vec<i32>", "Option<String>"]
    """
    result = []
    current = ""
    depth = 0
    
    for char in params:
        if char == '<':
            depth += 1
            current += char
        elif char == '>':
            depth -= 1
            current += char
        elif char == ',' and depth == 0:
            result.append(current.strip())
            current = ""
        else:
            current += char
    
    if current.strip():
        result.append(current.strip())
    
    return result


def _simplify_module_path(type_name: str) -> str:
    """
    Simplify module paths to just the type name.
    
    Examples:
        alloc::vec::Vec -> Vec
        alloc::string::String -> String
        core::option::Option -> Option
        std::collections::HashMap -> HashMap
    """
    known_types = {
        'alloc::vec::Vec': 'Vec',
        'alloc::string::String': 'String',
        'core::option::Option': 'Option',
        'core::result::Result': 'Result',
        'alloc::boxed::Box': 'Box',
        'alloc::sync::Arc': 'Arc',
        'alloc::rc::Rc': 'Rc',
        'std::collections::hash::map::HashMap': 'HashMap',
        'std::collections::HashMap': 'HashMap',
        'std::cell::RefCell': 'RefCell',
        'std::cell::Cell': 'Cell',
    }
    
    if type_name in known_types:
        return known_types[type_name]
    
    # For unknown types, extract last component
    if '::' in type_name:
        return type_name.split('::')[-1]
    
    return type_name


def _extract_type_name(full_path: str) -> str:
    """
    Extract the type name from a full crate path.
    
    Examples:
        rust_sample::User -> User
        my_crate::models::Config -> Config
        alloc::string::String -> String
    """
    if not '::' in full_path:
        return full_path
    
    # Handle generics: my_crate::Wrapper<T> -> Wrapper<T>
    if '<' in full_path:
        match = re.match(r'^([^<]+)<(.+)>$', full_path)
        if match:
            outer = match.group(1).split('::')[-1]
            inner = match.group(2)
            # Recursively normalize inner types
            inner_normalized = normalize_type_name(inner)
            return f"{outer}<{inner_normalized}>"
    
    return full_path.split('::')[-1]


def _normalize_inner_types(type_str: str) -> str:
    """
    Recursively normalize types inside generics.
    
    This handles C types in generic parameters:
        Vec<int> -> Vec<i32>
        Option<unsigned long> -> Option<u64>
    """
    # If this is a generic type, parse and normalize recursively
    if '<' in type_str:
        return _normalize_generic_type(type_str)
    
    # Replace C types
    if type_str in C_TO_RUST_TYPES:
        return C_TO_RUST_TYPES[type_str]
    
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
    
    # Handle tuples (type name starts with '(')
    if type_name.startswith('(') and type_name.endswith(')'):
        return _serialize_tuple(value, visited, depth)
    
    # Handle fixed arrays (type contains '[' and ']' like 'int[5]' or '[i32; 5]')
    if ('[' in type_name and ']' in type_name) or type_name.startswith('['):
        return _serialize_fixed_array(value, visited, depth)
    
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
    """Serialize an Option<T> with __ferrumpy_kind__ metadata."""
    type_name = value.GetType().GetName()
    
    # Check for explicit None variant in type name
    if type_name.endswith('::None'):
        return {
            "__ferrumpy_kind__": "option",
            "__variant__": "None",
            "__inner__": None
        }
    
    # Check for explicit Some variant in type name
    if type_name.endswith('::Some'):
        inner = value.GetChildAtIndex(0)
        inner_value = value_to_json(inner, visited, depth+1) if inner.IsValid() else None
        return {
            "__ferrumpy_kind__": "option",
            "__variant__": "Some",
            "__inner__": inner_value
        }
    
    # Handle LLDB's $variants$ structure (common for niche-optimized enums)
    # Structure: $variants$ -> $variant$0 (None) or $variant$1 (Some)
    # Each variant has $discr$ (discriminant) and value
    variants = value.GetChildMemberWithName('$variants$')
    if variants.IsValid():
        # Check $variant$1 for Some (discriminant = 1)
        variant1 = variants.GetChildMemberWithName('$variant$1')
        if variant1.IsValid():
            discr = variant1.GetChildMemberWithName('$discr$')
            if discr.IsValid() and discr.GetValueAsUnsigned(0) == 1:
                # It's Some - extract the inner value
                val_child = variant1.GetChildMemberWithName('value')
                if val_child.IsValid():
                    # The actual value is in __0 or first child
                    inner = val_child.GetChildMemberWithName('__0')
                    if not inner.IsValid():
                        inner = val_child.GetChildAtIndex(0)
                    if inner.IsValid():
                        inner_value = value_to_json(inner, visited, depth+1)
                        return {
                            "__ferrumpy_kind__": "option",
                            "__variant__": "Some",
                            "__inner__": inner_value
                        }
        
        # Check $variant$0 for None (discriminant = 0)
        variant0 = variants.GetChildMemberWithName('$variant$0')
        if variant0.IsValid():
            discr = variant0.GetChildMemberWithName('$discr$')
            if discr.IsValid() and discr.GetValueAsUnsigned(1) == 0:
                return {
                    "__ferrumpy_kind__": "option",
                    "__variant__": "None",
                    "__inner__": None
                }
    
    # Fallback: check for direct child that looks like inner value
    num_children = value.GetNumChildren()
    for i in range(num_children):
        child = value.GetChildAtIndex(i)
        name = child.GetName() or ""
        # Skip internal LLDB fields
        if name.startswith('$'):
            continue
        if 'Some' in name or name == '__0' or name == '0':
            inner_value = value_to_json(child, visited, depth+1)
            return {
                "__ferrumpy_kind__": "option",
                "__variant__": "Some",
                "__inner__": inner_value
            }
    
    # Last resort: use summary
    summary = value.GetSummary()
    if summary:
        if 'Some' in summary:
            # Try to parse value from summary like "Some(42)"
            return {
                "__ferrumpy_kind__": "option",
                "__variant__": "Some",
                "__summary__": summary
            }
        if 'None' in summary:
            return {
                "__ferrumpy_kind__": "option",
                "__variant__": "None",
                "__inner__": None
            }
    
    # Unknown
    return {
        "__ferrumpy_kind__": "option",
        "__variant__": "unknown",
        "__summary__": summary or ""
    }

def _serialize_result(value, visited: Set[int], depth: int = 0) -> Dict[str, Any]:
    """Serialize a Result<T, E> with __ferrumpy_kind__ metadata."""
    type_name = value.GetType().GetName()
    
    if type_name.endswith('::Ok'):
        inner = value.GetChildAtIndex(0)
        inner_value = value_to_json(inner, visited, depth+1) if inner.IsValid() else None
        return {
            "__ferrumpy_kind__": "result",
            "__variant__": "Ok",
            "__inner__": inner_value
        }
    
    if type_name.endswith('::Err'):
        inner = value.GetChildAtIndex(0)
        inner_value = value_to_json(inner, visited, depth+1) if inner.IsValid() else None
        return {
            "__ferrumpy_kind__": "result",
            "__variant__": "Err",
            "__inner__": inner_value
        }
    
    # Try to determine variant from structure
    summary = value.GetSummary()
    if summary:
        if 'Ok' in summary:
            return {
                "__ferrumpy_kind__": "result",
                "__variant__": "Ok",
                "__summary__": summary
            }
        if 'Err' in summary:
            return {
                "__ferrumpy_kind__": "result",
                "__variant__": "Err",
                "__summary__": summary
            }
    
    return {
        "__ferrumpy_kind__": "result",
        "__variant__": "unknown",
        "__summary__": summary or ""
    }


def _serialize_smart_pointer(value, visited: Set[int], depth: int = 0) -> Any:
    """Serialize Box/Arc/Rc with __ferrumpy_kind__ metadata."""
    type_name = value.GetType().GetName()
    
    # Determine pointer kind
    if 'Arc<' in type_name:
        kind = "arc"
    elif 'Rc<' in type_name:
        kind = "rc"
    elif 'Box<' in type_name:
        kind = "box"
    else:
        kind = "ptr"
    
    # Try to get the inner value
    inner_value = None
    
    # Common field for Arc/Rc in some LLDB versions
    ptr = value.GetChildMemberWithName('ptr')
    if ptr.IsValid():
        pointer = ptr.GetChildMemberWithName('pointer')
        if pointer.IsValid():
            inner = pointer.Dereference()
            if inner.IsValid():
                # Some versions have 'data' or 'value' field
                for field_name in ['data', 'value']:
                    data = inner.GetChildMemberWithName(field_name)
                    if data.IsValid():
                        inner_value = value_to_json(data, visited, depth+1)
                        break
                
                if inner_value is None:
                    # If it has 'strong' and 'weak' fields, it's the control block
                    # The value might be in a 'value' field alongside them
                    inner_value = value_to_json(inner, visited, depth+1)
    
    # Proactive check for 'value' field if it's a known smart pointer type
    if inner_value is None or (isinstance(inner_value, dict) and 'strong' in inner_value):
        # Look for 'value' child directly
        val_child = value.GetChildMemberWithName('value')
        if not val_child.IsValid():
            # Try to find any child named 'value' or 'data'
            for i in range(value.GetNumChildren()):
                child = value.GetChildAtIndex(i)
                if child.GetName() in ['value', 'data']:
                    val_child = child
                    break
        
        if val_child.IsValid():
            # If the child itself is the control block (contains strong/weak)
            # we need to go deeper
            if val_child.GetChildMemberWithName('strong').IsValid():
                real_val = val_child.GetChildMemberWithName('value')
                if real_val.IsValid():
                    inner_value = value_to_json(real_val, visited, depth+1)
            else:
                inner_value = value_to_json(val_child, visited, depth+1)

    # Fallback: try first child
    if inner_value is None and value.GetNumChildren() > 0:
        child = value.GetChildAtIndex(0)
        deref = child.Dereference()
        if deref.IsValid():
            inner_value = value_to_json(deref, visited, depth+1)
        else:
            # If dereference fails, maybe the child is the value (e.g. Box in some cases)
            inner_value = value_to_json(child, visited, depth+1)
    
    # Final check: if we still have 'strong'/'weak' in the result, it's failed to extract
    if isinstance(inner_value, dict) and ('strong' in inner_value or 'weak' in inner_value):
        # Try to extract 'value' from it if present
        if 'value' in inner_value:
            inner_value = inner_value['value']

    return {
        "__ferrumpy_kind__": kind,
        "__inner__": inner_value
    }


def _serialize_tuple(value, visited: Set[int], depth: int = 0) -> Dict[str, Any]:
    """Serialize a tuple with __ferrumpy_kind__ metadata."""
    elements = []
    type_name = value.GetType().GetName()
    
    num_children = value.GetNumChildren()
    for i in range(num_children):
        child = value.GetChildAtIndex(i)
        if child.IsValid():
            elements.append(value_to_json(child, visited, depth+1))
        else:
            elements.append(None)
    
    return {
        "__ferrumpy_kind__": "tuple",
        "__elements__": elements,
        "__type__": type_name
    }


def _serialize_fixed_array(value, visited: Set[int], depth: int = 0) -> Dict[str, Any]:
    """Serialize a fixed-size array with __ferrumpy_kind__ metadata."""
    elements = []
    type_name = value.GetType().GetName()
    
    num_children = value.GetNumChildren()
    for i in range(min(num_children, 100)):  # Limit for performance
        child = value.GetChildAtIndex(i)
        if child.IsValid():
            elements.append(value_to_json(child, visited, depth+1))
        else:
            elements.append(None)
    
    return {
        "__ferrumpy_kind__": "array",
        "__elements__": elements,
        "__type__": type_name,
        "__length__": num_children
    }


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
