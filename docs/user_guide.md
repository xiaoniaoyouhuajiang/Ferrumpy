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

---

## REPL (Interactive Rust Evaluation)

### `ferrumpy repl`

Start an interactive Rust REPL with access to variables from the current debug context.

```
(lldb) ferrumpy repl
[FerrumPy] Cache enabled (512MB)
Snapshot loaded with 5 variables: user: User, items: Vec<i32>, ...
>> 
```

### Features

#### ✅ Full Rust Syntax Support
- Define functions, structs, impl blocks
- Closures with captures
- Control flow (if/else, match, loops)
- Iterators and method chaining

```rust
>> fn sum_vec(v: &[i32]) -> i32 { v.iter().sum() }
>> sum_vec(&vec![1, 2, 3])
6
```

#### ✅ Snapshot Variables
Access variables from the current debug frame:

```rust
>> numbers.len()        // Access Vec from debug context
5
>> config.database      // Access struct fields
"localhost"
```

#### ✅ Multi-line Input
Unclosed braces automatically continue to the next line:

```rust
>> fn factorial(n: u64) -> u64 {
..     if n <= 1 { 1 } else { n * factorial(n - 1) }
.. }
>> factorial(10)
3628800
```

#### ✅ Code Completion (Enhanced Mode)
When `prompt_toolkit` is installed, Tab completion is available.

### REPL Commands

| Command | Description |
|---------|-------------|
| `:q` | Quit REPL |
| `:vars` | Show captured variables |
| `:help` | Show help |
| `Ctrl+C` | Interrupt execution |
| `Ctrl+C` ×2 | Force quit |

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `FERRUMPY_SIMPLE_MODE` | `0` | Set `1` to force simple mode (no prompt_toolkit) |
| `FERRUMPY_SNAPSHOT_ITEMS` | `0` | Set `1` to enable item-level export (experimental) |

---

## Known Limitations

### 1. `println!()` Output May Not Display

**Issue**: Output from `println!()` may not appear in the REPL.

**Cause**: The REPL subprocess's stdout is not fully routed to the terminal UI to prevent blocking issues.

**Workaround**: Use `format!()` and return the value instead:

```rust
// ❌ May not display
fn print_vec<T: std::fmt::Debug>(vec: Vec<T>) {
    println!("{:?}", vec);
}
print_vec(items);  // No output

// ✅ Always works
fn format_vec<T: std::fmt::Debug>(vec: Vec<T>) -> String {
    format!("{:?}", vec)
}
format_vec(items)  // Displays: [1, 2, 3]
```

### 2. Background Thread Output

**Issue**: Output from detached background threads may not display.

**Cause**: Async output arrives after eval() returns and is not captured.

**Workaround**: Use `join()` to wait for thread completion:

```rust
>> let h = std::thread::spawn(|| { println!("done"); });
>> h.join().unwrap();  // Wait for output
```

### 3. REPL Functions Cannot Access Snapshot Variables (Default Mode)

**Issue**: User-defined functions cannot access snapshot variables:

```rust
>> fn double_vec() -> Vec<i32> {
       numbers.iter().map(|x| x * 2).collect()  // Error: numbers not in scope
   }
```

**Workaround**: Use closures or pass variables as parameters:

```rust
>> let doubled = numbers.iter().map(|x| x * 2).collect::<Vec<_>>();
```

**Alternative**: Enable experimental item-level export:
```bash
export FERRUMPY_SNAPSHOT_ITEMS=1
```
Then access via function syntax: `numbers()` instead of `numbers`.

### 4. Some Types Fallback to `serde_json::Value`

**Issue**: Complex types like `HashMap`, `Arc`, `Rc` may appear as `serde_json::Value`.

**Cause**: LLDB cannot always deserialize internal memory layout of these types.

**Workaround**: Access fields/methods directly in REPL rather than serialized form.

---

## Troubleshooting

### REPL Freezes or Doesn't Respond

1. Press `Ctrl+C` twice to force exit
2. If still frozen, kill the LLDB process

### "Worker not found" Error

Ensure the worker binary is built:
```bash
cargo build --release
export FERRUMPY_REPL_WORKER=$(pwd)/target/release/ferrumpy-repl-worker
```

### Type Mismatch in Snapshot

If a variable shows incorrect type, it may be due to LLDB symbol mismatch. Rebuild your Rust project with debug symbols:
```bash
cargo build  # Debug build has full symbols
```
