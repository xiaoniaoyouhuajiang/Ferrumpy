"""
Path Resolver for FerrumPy

Resolves structured path expressions like "user.name[0].len" to LLDB SBValue.

Supported syntax:
- Field access: a.b
- Index access: a[0]
- Tuple field: a.0
- Dereference: a.* (for Box, Arc, Rc, references)
"""

# lldb is only available when running inside LLDB
try:
    import lldb
    _HAS_LLDB = True
except ImportError:
    _HAS_LLDB = False
    lldb = None

import re
from typing import List, Union
from dataclasses import dataclass


class PathResolutionError(Exception):
    """Error during path resolution."""
    pass


@dataclass
class PathSegment:
    """A segment in a path expression."""
    pass


@dataclass
class IdentSegment(PathSegment):
    """Field or variable name."""
    name: str


@dataclass
class IndexSegment(PathSegment):
    """Array/Vec index."""
    index: int


@dataclass
class DerefSegment(PathSegment):
    """Dereference (for Box, Arc, Rc, &, &mut)."""
    pass


def tokenize_path(path: str) -> List[PathSegment]:
    """
    Parse a path string into segments.
    
    Examples:
        "user" -> [IdentSegment("user")]
        "user.name" -> [IdentSegment("user"), IdentSegment("name")]
        "users[0].name" -> [IdentSegment("users"), IndexSegment(0), IdentSegment("name")]
        "box_val.*" -> [IdentSegment("box_val"), DerefSegment()]
    """
    segments: List[PathSegment] = []
    remaining = path.strip()
    
    # First segment must be an identifier
    match = re.match(r'^([a-zA-Z_][a-zA-Z0-9_]*)', remaining)
    if not match:
        raise PathResolutionError(f"Invalid path: expected identifier at start of '{path}'")
    
    segments.append(IdentSegment(match.group(1)))
    remaining = remaining[match.end():]
    
    while remaining:
        # Field access: .name or .0
        if remaining.startswith('.'):
            remaining = remaining[1:]
            
            # Dereference: .*
            if remaining.startswith('*'):
                segments.append(DerefSegment())
                remaining = remaining[1:]
                continue
            
            # Tuple field: .0, .1, etc.
            match = re.match(r'^(\d+)', remaining)
            if match:
                # Treat as field name (LLDB uses __0, __1 for tuple fields)
                segments.append(IdentSegment(f"__{match.group(1)}"))
                remaining = remaining[match.end():]
                continue
            
            # Regular field: .name
            match = re.match(r'^([a-zA-Z_][a-zA-Z0-9_]*)', remaining)
            if match:
                segments.append(IdentSegment(match.group(1)))
                remaining = remaining[match.end():]
                continue
            
            raise PathResolutionError(f"Invalid field access at: .{remaining}")
        
        # Index access: [0]
        elif remaining.startswith('['):
            match = re.match(r'^\[(\d+)\]', remaining)
            if not match:
                raise PathResolutionError(f"Invalid index access at: {remaining}")
            segments.append(IndexSegment(int(match.group(1))))
            remaining = remaining[match.end():]
        
        else:
            raise PathResolutionError(f"Unexpected character at: {remaining}")
    
    return segments


def resolve_path(frame: lldb.SBFrame, path: str) -> lldb.SBValue:
    """
    Resolve a path expression to an SBValue.
    
    Args:
        frame: The current stack frame
        path: Path expression like "user.name[0]"
        
    Returns:
        The resolved SBValue
        
    Raises:
        PathResolutionError: If the path cannot be resolved
    """
    segments = tokenize_path(path)
    
    if not segments:
        raise PathResolutionError("Empty path")
    
    # First segment must be a variable name
    first = segments[0]
    if not isinstance(first, IdentSegment):
        raise PathResolutionError("Path must start with a variable name")
    
    # Find the variable in the frame
    value = frame.FindVariable(first.name)
    if not value.IsValid():
        # Try arguments
        value = frame.FindVariable(first.name)
        if not value.IsValid():
            raise PathResolutionError(f"Variable '{first.name}' not found in current scope")
    
    # Resolve remaining segments
    for segment in segments[1:]:
        value = _resolve_segment(value, segment)
    
    return value


def _resolve_segment(value: lldb.SBValue, segment: PathSegment) -> lldb.SBValue:
    """Resolve a single path segment."""
    
    if isinstance(segment, IdentSegment):
        # Try direct field access
        child = value.GetChildMemberWithName(segment.name)
        if child.IsValid():
            return child
        
        # For smart pointers (Box, Arc, Rc), try to access through __0 or pointer
        # Check if this looks like a smart pointer
        type_name = value.GetType().GetName()
        if _is_smart_pointer_type(type_name):
            # Try dereferencing first
            deref = value.Dereference()
            if deref.IsValid():
                child = deref.GetChildMemberWithName(segment.name)
                if child.IsValid():
                    return child
        
        raise PathResolutionError(
            f"Field '{segment.name}' not found in type '{value.GetType().GetName()}'"
        )
    
    elif isinstance(segment, IndexSegment):
        # Try synthetic children first (for Vec, etc.)
        child = value.GetChildAtIndex(segment.index, lldb.eNoDynamicValues, True)
        if child.IsValid():
            return child
        
        # Try array access
        child = value.GetChildAtIndex(segment.index)
        if child.IsValid():
            return child
        
        raise PathResolutionError(
            f"Index [{segment.index}] out of bounds or not accessible"
        )
    
    elif isinstance(segment, DerefSegment):
        deref = value.Dereference()
        if deref.IsValid():
            return deref
        
        # For smart pointers, try getting the inner value
        type_name = value.GetType().GetName()
        if _is_smart_pointer_type(type_name):
            # Box<T> stores value in __0 or pointer
            inner = value.GetChildAtIndex(0)
            if inner.IsValid():
                return inner
        
        raise PathResolutionError(
            f"Cannot dereference type '{value.GetType().GetName()}'"
        )
    
    else:
        raise PathResolutionError(f"Unknown segment type: {type(segment)}")


def _is_smart_pointer_type(type_name: str) -> bool:
    """Check if a type is a known smart pointer type."""
    smart_pointer_patterns = [
        r'^alloc::boxed::Box<',
        r'^alloc::sync::Arc<',
        r'^alloc::rc::Rc<',
        r'^core::cell::Ref<',
        r'^core::cell::RefMut<',
    ]
    return any(re.match(p, type_name) for p in smart_pointer_patterns)
