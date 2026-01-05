#!/usr/bin/env python3
"""
POC: Direct memory reading for Rust types in LLDB

This script tests whether we can extract actual data from Rust types
by directly reading process memory, bypassing SBValue limitations.

Usage in LLDB:
  (lldb) command script import tests/poc_memory_read.py
  (lldb) poc_read tuple
  (lldb) poc_read map
  (lldb) poc_read container
"""

import lldb


def read_str_from_memory(process, ptr_addr, length):
    """Read a string from process memory given pointer and length."""
    if ptr_addr == 0 or length == 0:
        return None
    error = lldb.SBError()
    data = process.ReadMemory(ptr_addr, min(length, 4096), error)
    if error.Fail():
        return f"<read error: {error.GetCString()}>"
    return data.decode('utf-8', errors='replace')


def extract_str_ref(value):
    """Extract &str by reading (data_ptr, length) fat pointer from memory."""
    process = value.GetProcess()
    
    # &str is represented as a fat pointer: (data_ptr, length)
    # Try different child access patterns
    
    # Pattern 1: Direct children with index
    if value.GetNumChildren() >= 2:
        ptr_child = value.GetChildAtIndex(0)
        len_child = value.GetChildAtIndex(1)
        if ptr_child.IsValid() and len_child.IsValid():
            ptr_addr = ptr_child.GetValueAsUnsigned()
            str_len = len_child.GetValueAsUnsigned()
            if ptr_addr and str_len:
                return read_str_from_memory(process, ptr_addr, str_len)
    
    # Pattern 2: Named children (data_ptr, length)
    data_ptr = value.GetChildMemberWithName('data_ptr')
    length = value.GetChildMemberWithName('length')
    if data_ptr.IsValid() and length.IsValid():
        ptr_addr = data_ptr.GetValueAsUnsigned()
        str_len = length.GetValueAsUnsigned()
        if ptr_addr and str_len:
            return read_str_from_memory(process, ptr_addr, str_len)
    
    # Pattern 3: Read raw memory at value's address
    # &str in memory: [ptr: 8 bytes][len: 8 bytes] on 64-bit
    addr = value.GetLoadAddress()
    if addr != lldb.LLDB_INVALID_ADDRESS:
        error = lldb.SBError()
        # Read 16 bytes (ptr + len on 64-bit)
        data = process.ReadMemory(addr, 16, error)
        if not error.Fail() and len(data) == 16:
            import struct
            ptr_addr, str_len = struct.unpack('QQ', data)  # Two 64-bit values
            if ptr_addr and str_len and str_len < 10000:  # Sanity check
                return read_str_from_memory(process, ptr_addr, str_len)
    
    return None


def extract_string(value):
    """Extract String by reading Vec<u8> internals."""
    process = value.GetProcess()
    
    # String contains: vec: Vec<u8>
    # Vec layout: { buf: RawVec { ptr: *const T, cap: usize }, len: usize }
    
    vec = value.GetChildMemberWithName('vec')
    if not vec.IsValid():
        return None
    
    # Try to get length
    len_child = vec.GetChildMemberWithName('len')
    if not len_child.IsValid():
        return None
    str_len = len_child.GetValueAsUnsigned()
    
    if str_len == 0:
        return ""
    
    # Try to get buffer pointer
    buf = vec.GetChildMemberWithName('buf')
    if not buf.IsValid():
        return None
    
    # Navigate: buf -> inner -> ptr -> pointer
    # or: buf -> ptr -> pointer (depending on Rust version)
    ptr_addr = None
    
    # Try various paths to find the pointer
    for path in [
        ['inner', 'ptr', 'pointer'],
        ['ptr', 'pointer'],
        ['inner', 'ptr'],
        ['ptr'],
    ]:
        node = buf
        for step in path:
            node = node.GetChildMemberWithName(step)
            if not node.IsValid():
                break
        if node.IsValid():
            ptr_addr = node.GetValueAsUnsigned()
            if ptr_addr:
                break
    
    # Fallback: try first non-zero child as pointer
    if not ptr_addr:
        for i in range(buf.GetNumChildren()):
            child = buf.GetChildAtIndex(i)
            addr = child.GetValueAsUnsigned()
            if addr and addr > 0x1000:  # Looks like a valid address
                ptr_addr = addr
                break
    
    if ptr_addr:
        return read_str_from_memory(process, ptr_addr, str_len)
    
    return None


def extract_tuple(value):
    """Extract tuple elements, with special handling for &str."""
    result = []
    type_name = value.GetType().GetName()
    
    num_children = value.GetNumChildren()
    for i in range(num_children):
        child = value.GetChildAtIndex(i)
        child_type = child.GetType().GetName()
        child_name = child.GetName() or f"_{i}"
        
        if child_type == '&str' or 'str' in child_type:
            val = extract_str_ref(child)
        elif 'String' in child_type:
            val = extract_string(child)
        else:
            # Try to get primitive value
            val = child.GetValue()
            if val is None:
                val = child.GetSummary()
        
        result.append({
            "name": child_name,
            "type": child_type,
            "value": val
        })
    
    return {
        "type": type_name,
        "elements": result
    }


def extract_hashmap(value):
    """Attempt to extract HashMap contents."""
    process = value.GetProcess()
    type_name = value.GetType().GetName()
    
    result = {
        "type": type_name,
        "entries": []
    }
    
    # Try GetSummary first
    summary = value.GetSummary()
    if summary:
        result["summary"] = summary
    
    # HashMap internal structure is complex:
    # HashMap { base: hashbrown::HashMap { ... } }
    # The actual data is in a RawTable with complex bucket layout
    
    # Try to find the base/table
    base = value.GetChildMemberWithName('base')
    if base.IsValid():
        table = base.GetChildMemberWithName('table')
        if table.IsValid():
            result["table_info"] = {
                "num_children": table.GetNumChildren(),
                "summary": table.GetSummary()
            }
            
            # Try to find bucket data
            ctrl = table.GetChildMemberWithName('table').GetChildMemberWithName('ctrl')
            if ctrl.IsValid():
                ctrl_addr = ctrl.GetValueAsUnsigned()
                result["ctrl_addr"] = hex(ctrl_addr) if ctrl_addr else None
    
    # Try iterating children directly
    num_children = value.GetNumChildren()
    result["num_children"] = num_children
    
    # Explore structure
    children_info = []
    for i in range(min(num_children, 10)):
        child = value.GetChildAtIndex(i)
        if child.IsValid():
            children_info.append({
                "name": child.GetName(),
                "type": child.GetType().GetName(),
                "value": child.GetValue() or child.GetSummary()
            })
    result["children"] = children_info
    
    return result


def extract_container(value):
    """Extract Container<T> generic type."""
    result = {
        "type": value.GetType().GetName()
    }
    
    # Try to get 'value' field
    val_child = value.GetChildMemberWithName('value')
    if val_child.IsValid():
        result["value"] = val_child.GetValue() or val_child.GetSummary()
    
    # Try to get 'metadata' field
    meta_child = value.GetChildMemberWithName('metadata')
    if meta_child.IsValid():
        meta_type = meta_child.GetType().GetName()
        if 'String' in meta_type:
            result["metadata"] = extract_string(meta_child)
        else:
            result["metadata"] = meta_child.GetValue() or meta_child.GetSummary()
    
    return result


def poc_read(debugger, command, result, internal_dict):
    """POC command to test memory reading for specific variables."""
    target = debugger.GetSelectedTarget()
    process = target.GetProcess()
    thread = process.GetSelectedThread()
    frame = thread.GetSelectedFrame()
    
    var_name = command.strip()
    if not var_name:
        print("Usage: poc_read <variable_name>")
        print("Example: poc_read tuple")
        return
    
    value = frame.FindVariable(var_name)
    if not value.IsValid():
        print(f"Variable '{var_name}' not found")
        return
    
    type_name = value.GetType().GetName()
    print(f"\n=== POC Memory Read for '{var_name}' ===")
    print(f"Type: {type_name}")
    print(f"Address: {hex(value.GetLoadAddress())}")
    print()
    
    # Dispatch based on type
    if type_name.startswith('(') and type_name.endswith(')'):
        result_data = extract_tuple(value)
        print("Tuple extraction:")
        for elem in result_data.get("elements", []):
            print(f"  {elem['name']}: {elem['type']} = {elem['value']}")
    
    elif 'HashMap' in type_name:
        result_data = extract_hashmap(value)
        print("HashMap extraction:")
        print(f"  Summary: {result_data.get('summary')}")
        print(f"  Num children: {result_data.get('num_children')}")
        if result_data.get('children'):
            print("  Children:")
            for c in result_data['children']:
                print(f"    {c['name']}: {c['type']}")
    
    elif 'Container' in type_name:
        result_data = extract_container(value)
        print("Container extraction:")
        print(f"  value: {result_data.get('value')}")
        print(f"  metadata: {result_data.get('metadata')}")
    
    elif type_name == '&str':
        result_data = extract_str_ref(value)
        print(f"&str extraction: {result_data}")
    
    elif 'String' in type_name:
        result_data = extract_string(value)
        print(f"String extraction: {result_data}")
    
    else:
        print(f"Unknown type, raw value: {value.GetValue()}")
        print(f"Summary: {value.GetSummary()}")


def poc_expr(debugger, command, result, internal_dict):
    """POC command to test EvaluateExpression for extracting variable data."""
    target = debugger.GetSelectedTarget()
    process = target.GetProcess()
    thread = process.GetSelectedThread()
    frame = thread.GetSelectedFrame()
    
    var_name = command.strip()
    if not var_name:
        print("Usage: poc_expr <variable_name>")
        print("Example: poc_expr map")
        return
    
    print(f"\n=== POC EvaluateExpression for '{var_name}' ===\n")
    
    # Test 1: Simple Debug format
    print("Test 1: Debug format with format!()")
    expr1 = f'format!("{{:?}}", {var_name})'
    result1 = frame.EvaluateExpression(expr1)
    if result1.GetError().Success():
        print(f"  Result: {result1.GetSummary()}")
    else:
        print(f"  Error: {result1.GetError().GetCString()}")
    
    # Test 2: Try with explicit type annotation
    print("\nTest 2: Direct variable access")
    result2 = frame.EvaluateExpression(var_name)
    if result2.GetError().Success():
        print(f"  Type: {result2.GetType().GetName()}")
        print(f"  Value: {result2.GetValue()}")
        print(f"  Summary: {result2.GetSummary()}")
        print(f"  NumChildren: {result2.GetNumChildren()}")
    else:
        print(f"  Error: {result2.GetError().GetCString()}")
    
    # Test 3: Try iterating HashMap entries
    if 'map' in var_name.lower():
        print("\nTest 3: HashMap iteration attempt")
        # Try to get length
        expr_len = f'{var_name}.len()'
        result_len = frame.EvaluateExpression(expr_len)
        if result_len.GetError().Success():
            print(f"  len(): {result_len.GetValue()}")
        else:
            print(f"  len() error: {result_len.GetError().GetCString()}")
        
        # Try to get keys (requires iterator, may not work)
        expr_keys = f'{var_name}.keys().collect::<Vec<_>>()'
        result_keys = frame.EvaluateExpression(expr_keys)
        if result_keys.GetError().Success():
            print(f"  keys(): {result_keys.GetSummary()}")
        else:
            print(f"  keys() error: {result_keys.GetError().GetCString()}")
        
        # Try direct index access
        print("\n  Trying direct key access:")
        for key in ['"one"', '"two"', '"three"']:
            expr_get = f'{var_name}.get({key})'
            result_get = frame.EvaluateExpression(expr_get)
            if result_get.GetError().Success():
                print(f"    get({key}): {result_get.GetSummary()}")
            else:
                print(f"    get({key}) error: {result_get.GetError().GetCString()}")
    
    # Test 4: For Container, try field access
    if 'container' in var_name.lower():
        print("\nTest 4: Container field access")
        for field in ['value', 'metadata']:
            expr_field = f'{var_name}.{field}'
            result_field = frame.EvaluateExpression(expr_field)
            if result_field.GetError().Success():
                print(f"  .{field}: {result_field.GetSummary() or result_field.GetValue()}")
            else:
                print(f"  .{field} error: {result_field.GetError().GetCString()}")


def __lldb_init_module(debugger, internal_dict):
    debugger.HandleCommand('command script add -f poc_memory_read.poc_read poc_read')
    debugger.HandleCommand('command script add -f poc_memory_read.poc_expr poc_expr')
    print("POC commands loaded. Use: poc_read <var_name>, poc_expr <var_name>")
