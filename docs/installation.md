# Installation Guide

## Prerequisites

- **Rust toolchain**: 1.70+ (with `cargo`)
- **Python**: 3.9+ (3.13 or 3.14 recommended)
- **LLDB**: 15.0+ with Python scripting support

## Quick Start

### 1. Clone the Repository

```bash
git clone https://github.com/your-username/ferrumpy.git
cd ferrumpy
```

### 2. Build the Rust Components

```bash
# Build all workspace crates
cargo build --release
```

### 3. Install Python Extension (FFI)

For the best performance, install the pyo3 FFI extension:

```bash
# Create a virtual environment with your Python version
python3 -m venv .venv
source .venv/bin/activate

# Install maturin and build the extension
pip install maturin
maturin develop --features python --release
```

### 4. Configure LLDB

Add the following to your `~/.lldbinit`:

```
command script import /path/to/ferrumpy/python/ferrumpy
```

Replace `/path/to/ferrumpy` with your actual installation path.

## Verification

Start LLDB and verify the installation:

```bash
lldb
(lldb) ferrumpy help
```

You should see the FerrumPy help message.

## Troubleshooting

### LLDB Python Version Mismatch

If LLDB uses a different Python version than your venv, rebuild the FFI for that version:

```bash
# Check LLDB's Python version
lldb -o "script import sys; print(sys.version)" -o quit

# Build for that specific Python
/path/to/python3.X -m venv .venvX
source .venvX/bin/activate
pip install maturin
maturin develop --features python
```

### FFI Module Not Found

If the FFI module fails to load, FerrumPy automatically falls back to subprocess communication (JSON-RPC), which works without the FFI extension.

### rust-analyzer Not Found

For completion features, install rust-analyzer:

```bash
rustup component add rust-analyzer
```
