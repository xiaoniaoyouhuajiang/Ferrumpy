# FerrumPy

A Python-like debugging experience for Rust, built on top of LLDB.

## Features

- **Pretty Printers**: Human-readable display of Rust types (String, Vec, HashMap, etc.)
- **Structured Access**: Navigate Rust data structures with `a.b[0].c` syntax
- **Rust-aware Console** (WIP): IDE-level completions and type hints

## Installation

```bash
# Add to your ~/.lldbinit
command script import /path/to/ferrumpy/python/ferrumpy
```

## Usage

In LLDB:
```
(lldb) ferrumpy locals      # Pretty print all local variables
(lldb) ferrumpy pp user     # Pretty print specific variable
(lldb) ferrumpy pp user.name[0]  # Structured path access
```

## Project Structure

```
ferrumpy/
├── python/ferrumpy/     # LLDB Python scripts
│   ├── __init__.py      # LLDB command registration
│   ├── commands.py      # Command implementations
│   ├── path_resolver.py # Structured path access
│   └── providers/       # Pretty Printer providers
├── tests/rust_sample/   # Test Rust programs
└── references/          # Design documents
```

## License

MIT
