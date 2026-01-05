# User Guide

## Overview

FerrumPy enhances Rust debugging in LLDB with:

- **Pretty Printers**: Human-readable display of Rust types (String, Vec, Option, Result, etc.)
- **Expression Evaluation**: Evaluate Rust expressions with strict type checking
- **Tab Completion**: Native completion for variable paths

## Commands

### `ferrumpy help`

Display available commands and usage information.

### `ferrumpy locals`

Pretty print all local variables in the current frame.

```
(lldb) ferrumpy locals
user: User { name: "Alice", age: 30 }
items: Vec<i32>[1, 2, 3]
```

### `ferrumpy args`

Pretty print function arguments.

### `ferrumpy pp <path>`

Pretty print a specific variable or field path.

```
(lldb) ferrumpy pp user.name
(String) "Alice"

(lldb) ferrumpy pp items[0]
(i32) 1
```

### `ferrumpy-pp <path>` (with Tab completion)

Same as `ferrumpy pp`, but with native Tab completion:

```
(lldb) ferrumpy-pp us<Tab>
(lldb) ferrumpy-pp user
(lldb) ferrumpy-pp user.<Tab>
user.name    user.age
```

### `ferrumpy eval <expr>`

Evaluate a Rust expression.

```
(lldb) ferrumpy eval 10 + 5
(i32) 15

(lldb) ferrumpy eval x * 2
(i32) 84

(lldb) ferrumpy eval count > 10
(bool) true
```

**Supported operations:**
- Arithmetic: `+`, `-`, `*`, `/`, `%`
- Comparison: `==`, `!=`, `<`, `<=`, `>`, `>=`
- Logical: `&&`, `||`, `!`
- Bitwise: `&`, `|`, `^`, `<<`, `>>`

**Not yet supported:**
- Function calls: `foo()`
- Method calls: `x.len()`
- Field access in expressions (use `ferrumpy pp` instead)

### `ferrumpy repl`

Start an embedded Rust REPL (evcxr) with access to current variables from the debug session.

```
(lldb) ferrumpy repl
Snapshot loaded with 5 items. Access: user(), items(), config(), ...
>> 
```

**Variable Access (Important):**
Variables captured from the debugger are exposed as **accessor functions**. You must call them with `()` to access their values. This ensures variables remain available throughout the REPL session without being moved.

```rust
// ✅ Correct: Access as function call
>> user()
User { name: "Alice", age: 30 }

// ❌ Incorrect: Direct variable access
>> user
error[E0425]: cannot find ...

// Accessing fields and methods works naturally on the result
>> user().name
"Alice"

>> items().len()
3
```

**Getting Ownership:**
The accessor functions return a reference (or a cloned value for copy types). To get an owned value (e.g., to pass to a function that takes ownership), use `.clone()`:

```rust
// Create a local variable with ownership
>> let my_user = user().clone();
>> process_user(my_user); // OK: moves my_user
```

### `ferrumpy type <expr>`

Display type information for a variable.

```
(lldb) ferrumpy type user
Type: User
Size: 56 bytes
Fields:
  name: String
  age: i32
```

## Pretty Printer Types

FerrumPy provides enhanced display for:

| Type | Display |
|------|---------|
| `String` | `"content"` |
| `&str` | `"content"` |
| `Vec<T>` | `Vec<T>[elem1, elem2, ...]` |
| `Option<T>` | `Some(value)` or `None` |
| `Result<T, E>` | `Ok(value)` or `Err(error)` |
| `Box<T>` | `Box<T> → inner` |
| `Rc<T>` | `Rc<T>(count) → inner` |
| `Arc<T>` | `Arc<T>(count) → inner` |
| `HashMap<K, V>` | `HashMap { key: value, ... }` |

## Options

### `--raw`

Show raw LLDB output instead of pretty printing.

```
(lldb) ferrumpy pp user --raw
```

### `--expand`

Expand internal structure details.

```
(lldb) ferrumpy pp user --expand
```

## Tips

1. **Use `ferrumpy-pp` for interactive exploration** - Tab completion makes it easy to navigate complex structures.

2. **Use `ferrumpy eval` for calculations** - Evaluate expressions with proper Rust type semantics.

3. **Combine with breakpoints** - Set a breakpoint and use `ferrumpy locals` to inspect state.
