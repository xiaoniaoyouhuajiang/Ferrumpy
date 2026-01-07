#!/bin/bash
#
# FerrumPy Installation Script
#
# Installs FerrumPy for use with LLDB on systems where pip install
# may not work (externally-managed Python environments).
#
# Prerequisites: curl, unzip, python3
#
# Usage:
#   curl -sSL https://raw.githubusercontent.com/xiaoniaoyouhuajiang/Ferrumpy/main/scripts/install.sh | bash
#

set -e

INSTALL_DIR="$HOME/.local/lib/ferrumpy"
LLDBINIT="$HOME/.lldbinit"
PYPI_API="https://pypi.org/pypi/ferrumpy/json"

echo "=== FerrumPy Installer ==="
echo

# Detect platform
OS=$(uname -s)
ARCH=$(uname -m)

echo "Detected: $OS / $ARCH"

# Map to wheel platform suffix
case "$OS" in
    Linux)
        case "$ARCH" in
            x86_64)
                PLATFORM_PATTERN="manylinux.*x86_64"
                ;;
            aarch64|arm64)
                PLATFORM_PATTERN="manylinux.*aarch64"
                ;;
            *)
                echo "Error: Unsupported architecture: $ARCH"
                exit 1
                ;;
        esac
        ;;
    Darwin)
        case "$ARCH" in
            arm64)
                PLATFORM_PATTERN="macosx.*arm64"
                ;;
            x86_64)
                PLATFORM_PATTERN="macosx.*x86_64"
                ;;
            *)
                echo "Error: Unsupported architecture: $ARCH"
                exit 1
                ;;
        esac
        ;;
    *)
        echo "Error: Unsupported OS: $OS"
        exit 1
        ;;
esac

# Fetch wheel URL from PyPI
echo "Fetching latest version from PyPI..."
WHEEL_INFO=$(curl -sL "$PYPI_API" | python3 -c "
import json, sys, re
data = json.load(sys.stdin)
version = data['info']['version']
pattern = re.compile(r'$PLATFORM_PATTERN')
for url_info in data.get('urls', []):
    filename = url_info.get('filename', '')
    if pattern.search(filename) and filename.endswith('.whl'):
        print(f\"{url_info['url']}|{filename}|{version}\")
        break
")

if [ -z "$WHEEL_INFO" ]; then
    echo "Error: Could not find wheel for platform: $PLATFORM_PATTERN"
    echo "Please check https://pypi.org/project/ferrumpy/#files for available wheels."
    exit 1
fi

# Parse wheel info
WHEEL_URL=$(echo "$WHEEL_INFO" | cut -d'|' -f1)
WHEEL_FILENAME=$(echo "$WHEEL_INFO" | cut -d'|' -f2)
VERSION=$(echo "$WHEEL_INFO" | cut -d'|' -f3)

echo "Version: $VERSION"
echo "Wheel: $WHEEL_FILENAME"
echo

# Create install directory (or clean existing for upgrade)
echo "Installing to: $INSTALL_DIR"
if [ -d "$INSTALL_DIR" ]; then
    echo "Cleaning previous installation..."
    rm -rf "$INSTALL_DIR"
fi
mkdir -p "$INSTALL_DIR"

# Download wheel
TEMP_WHEEL=$(mktemp)
echo "Downloading..."
curl -sL "$WHEEL_URL" -o "$TEMP_WHEEL"

# Extract wheel (it's a zip file)
echo "Extracting..."
unzip -q -o "$TEMP_WHEEL" -d "$INSTALL_DIR"
rm "$TEMP_WHEEL"

FERRUMPY_PATH="$INSTALL_DIR/ferrumpy"

if [ ! -d "$FERRUMPY_PATH" ]; then
    echo "Error: Installation failed - ferrumpy directory not found"
    exit 1
fi

echo "Installed ferrumpy to: $FERRUMPY_PATH"
echo

# Configure LLDB
LLDB_CMD="command script import $FERRUMPY_PATH"

echo "Configuring LLDB..."
if [ -f "$LLDBINIT" ] && grep -q "ferrumpy" "$LLDBINIT"; then
    echo "Note: ~/.lldbinit already contains ferrumpy configuration."
    echo "Current config may need updating. New command:"
    echo "  $LLDB_CMD"
else
    echo "$LLDB_CMD" >> "$LLDBINIT"
    echo "Added to ~/.lldbinit"
fi

# Configure environment for repl-worker
# Look for worker in data directory using the version we downloaded
WORKER_PATH=""
DATA_DIR="$INSTALL_DIR/ferrumpy-${VERSION}.data/scripts"
if [ -d "$DATA_DIR" ] && [ -f "$DATA_DIR/ferrumpy-repl-worker" ]; then
    WORKER_PATH="$DATA_DIR/ferrumpy-repl-worker"
fi

# Fallback: search for any .data directory
if [ -z "$WORKER_PATH" ]; then
    DATA_DIR=$(find "$INSTALL_DIR" -maxdepth 1 -type d -name "*.data" | head -1)
    if [ -n "$DATA_DIR" ] && [ -f "$DATA_DIR/scripts/ferrumpy-repl-worker" ]; then
        WORKER_PATH="$DATA_DIR/scripts/ferrumpy-repl-worker"
    fi
fi

if [ -n "$WORKER_PATH" ]; then
    chmod +x "$WORKER_PATH"
    echo
    echo "REPL worker found at: $WORKER_PATH"

    # Determine shell config file
    SHELL_CONFIG=""
    if [ -n "$ZSH_VERSION" ] || [ -f "$HOME/.zshrc" ]; then
        SHELL_CONFIG="$HOME/.zshrc"
    elif [ -f "$HOME/.bashrc" ]; then
        SHELL_CONFIG="$HOME/.bashrc"
    elif [ -f "$HOME/.bash_profile" ]; then
        SHELL_CONFIG="$HOME/.bash_profile"
    fi

    EXPORT_LINE="export FERRUMPY_REPL_WORKER=\"$WORKER_PATH\""

    if [ -n "$SHELL_CONFIG" ]; then
        if grep -q "FERRUMPY_REPL_WORKER" "$SHELL_CONFIG" 2>/dev/null; then
            echo "Note: $SHELL_CONFIG already contains FERRUMPY_REPL_WORKER."
            echo "You may need to update it manually to:"
            echo "  $EXPORT_LINE"
        else
            echo "" >> "$SHELL_CONFIG"
            echo "# FerrumPy REPL worker" >> "$SHELL_CONFIG"
            echo "$EXPORT_LINE" >> "$SHELL_CONFIG"
            echo "Added FERRUMPY_REPL_WORKER to $SHELL_CONFIG"
            echo
            echo "Run 'source $SHELL_CONFIG' or restart your terminal to apply."
        fi
    else
        echo "Could not detect shell config file."
        echo "Please add this manually to your shell config:"
        echo "  $EXPORT_LINE"
    fi
fi

echo
echo "=== Installation Complete ==="
echo
echo "Start LLDB and run: ferrumpy help"
echo
