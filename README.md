# FerrumPy

A powerful debugging REPL for Rust, built on LLDB. Debug Rust with the ease of an interactive Python session.

## Features

- **🎯 Interactive REPL**: Full-featured Rust REPL powered by evcxr
- **📸 Snapshot Variables**: Capture debugged process variables and use them in REPL
- **🎨 Pretty Printers**: Human-readable display of Rust types (String, Vec, Option, Result, etc.)
- **🔍 Path Navigation**: Access nested data with `a.b[0].c` syntax
- **⚡ Smart Type Restoration**: Automatic type inference with manual `restore!` macro for complex types

## Installation

### Via pip (Recommended)

```bash
pip install ferrumpy
```

The package uses Python's Stable ABI (abi3) and is compatible with Python 3.9+.

### Manual Setup

```bash
# Clone the repository
git clone https://github.com/xiaoniaoyouhuajiang/Ferrumpy.git
cd Ferrumpy

# Build and install
maturin develop --release

# Add to your ~/.lldbinit
echo 'command script import <path-to-repo>/python/ferrumpy' >> ~/.lldbinit
```

## Quick Start

```rust
// your_program.rs
fn main() {
    let numbers = vec![1, 2, 3, 4, 5];
    let message = "Hello, FerrumPy!";
    println!("Set breakpoint here");  // <-- breakpoint
}
```

```bash
# In LLDB
(lldb) b main.rs:4
(lldb) run
(lldb) ferrumpy repl  # Start interactive REPL

>> numbers().iter().sum::<i32>()
15
>> message().to_uppercase()
"HELLO, FERRUMPY!"
>> let doubled: Vec<i32> = numbers().iter().map(|x| x * 2).collect();
>> doubled
[2, 4, 6, 8, 10]
```

## Usage

### Pretty Printing

```lldb
(lldb) ferrumpy locals           # Pretty print all local variables
(lldb) ferrumpy pp user          # Pretty print specific variable
(lldb) ferrumpy pp user.name[0]  # Navigate nested structures
(lldb) ferrumpy type user        # Show type information
```

### REPL Mode

```lldb
(lldb) ferrumpy repl

# Variables accessed as functions
>> simple_string()
"Hello, FerrumPy!"

# Full Rust expressions supported
>> numbers().iter().filter(|x| x % 2 == 0).collect::<Vec<_>>()
[2, 4]

# Define new functions and types
>> fn double(x: i32) -> i32 { x * 2 }
>> double(21)
42
```

### Type Restoration (Advanced)

For complex types that can't be automatically restored, use the `restore!` macro:

```rust
>> tuple()  // Returns JSON representation
{"__elements__": ["first", 2, 3.14], ...}

>> let t = restore!(tuple, (&str, i32, f64));
>> t.0
"first"
```

## Supported Types

| Type Category | Examples | Support |
|---------------|----------|---------|
| Primitives | `i32`, `f64`, `bool`, `char` | ✅ Full |
| Strings | `String`, `&str` | ✅ Full |
| Collections | `Vec<T>`, Arrays | ✅ Full |
| Smart Pointers | `Box`, `Rc`, `Arc` | ✅ Full |
| Options/Results | `Option<T>`, `Result<T, E>` | ✅ Full |
| Tuples | `(T1, T2, ...)` | ✅ Full |
| Enums | All variants | ✅ Full |
| Structs | User-defined | ✅ Full |
| HashMap | `HashMap<K, V>` | ⚠️ Limited* |

\* HashMap serialization is not supported in REPL snapshots. Create them manually in REPL if needed.

## Project Structure

```
ferrumpy/
├── python/ferrumpy/        # LLDB Python integration
│   ├── commands.py         # LLDB commands (pp, repl, etc.)
│   ├── serializer.py       # Variable serialization with memory reading
│   └── providers/          # Pretty printer providers
├── ferrumpy-core/          # Rust core library
│   ├── src/repl/          # REPL session management
│   ├── src/expr/          # Expression parsing
│   └── src/libgen/        # Snapshot library generation
├── tests/                  # Test suite
│   ├── run_tests.sh       # Test runner
│   ├── test_repl.exp      # REPL integration tests
│   └── rust_sample/       # Test Rust project
└── docs/                   # Documentation
```

## Requirements

- **macOS**: 11.0+ (arm64 or x86_64)
- **Linux**: x86_64 (LLDB required)
- **Python**: 3.9+
- **Rust**: 1.70+ (for building from source)
- **LLDB**: System LLDB (ships with Xcode on macOS)

## Contributing

Contributions welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## Documentation

- [User Guide](docs/user_guide.md) - Detailed usage examples
- [Installation Guide](docs/installation.md) - Platform-specific setup
- [Architecture](docs/architecture.md) - Design decisions

## License

MIT License - see [LICENSE](LICENSE) for details.

## Acknowledgments

Built on top of:
- [evcxr](https://github.com/google/evcxr) - Rust REPL engine
- [PyO3](https://github.com/PyO3/pyo3) - Rust-Python bindings
- LLDB Python API
