#!/usr/bin/env python3
"""
LLDB script to inspect Rust variable structure.
Run with: lldb target -o "script execfile('inspect_var.py')"
Or use: (lldb) script execfile('tests/inspect_var.py')
"""

def inspect_variable(var, depth=0, max_depth=3):
    """Recursively inspect LLDB variable structure."""
    indent = "  " * depth
    if not var.IsValid():
        print(f"{indent}(invalid)")
        return
    
    name = var.GetName() or "(unnamed)"
    type_name = var.GetType().GetName()
    value = var.GetValue()
    summary = var.GetSummary()
    num_children = var.GetNumChildren()
    
    print(f"{indent}'{name}':")
    print(f"{indent}  type: {type_name}")
    print(f"{indent}  value: {value}")
    print(f"{indent}  summary: {summary}")
    print(f"{indent}  children: {num_children}")
    
    if depth < max_depth and num_children > 0:
        for i in range(min(num_children, 5)):  # Limit to 5 children
            child = var.GetChildAtIndex(i)
            inspect_variable(child, depth + 1, max_depth)


def inspect_frame_variables(debugger, command, result, internal_dict):
    """LLDB command to inspect all variables in current frame."""
    target = debugger.GetSelectedTarget()
    process = target.GetProcess()
    thread = process.GetSelectedThread()
    frame = thread.GetSelectedFrame()
    
    if not frame.IsValid():
        print("No valid frame selected")
        return
    
    print("=" * 60)
    print("Inspecting frame variables")
    print("=" * 60)
    
    # Get specific variables by name
    for var_name in ["some_value", "none_value", "ok_result", "err_result", "map"]:
        var = frame.FindVariable(var_name)
        if var.IsValid():
            print(f"\n=== {var_name} ===")
            inspect_variable(var, 0, 3)
        else:
            print(f"\n=== {var_name} === (not found)")


def __lldb_init_module(debugger, internal_dict):
    """Register the command when module is imported."""
    debugger.HandleCommand('command script add -f inspect_var.inspect_frame_variables inspect_vars')
    print("Loaded 'inspect_vars' command. Run at breakpoint to inspect variables.")
