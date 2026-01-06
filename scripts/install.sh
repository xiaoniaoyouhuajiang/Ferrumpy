#!/bin/bash
#
# FerrumPy Installation Script
#
# This script installs FerrumPy for use with LLDB on systems where
# the system Python is externally-managed (e.g., Debian, Ubuntu 23.04+).
#
# Usage:
#   curl -sSL https://raw.githubusercontent.com/xiaoniaoyouhuajiang/Ferrumpy/main/scripts/install.sh | bash
#
# Or locally:
#   ./scripts/install.sh
#

set -e

FERRUMPY_VERSION="0.1.1"
INSTALL_DIR="$HOME/.local/lib/ferrumpy"
LLDBINIT="$HOME/.lldbinit"

echo "=== FerrumPy Installer ==="
echo

# Detect platform
OS=$(uname -s)
ARCH=$(uname -m)

case "$OS" in
    Linux)
        case "$ARCH" in
            x86_64)
                WHEEL_PLATFORM="manylinux_2_17_x86_64.manylinux2014_x86_64"
                ;;
            aarch64)
                WHEEL_PLATFORM="manylinux_2_17_aarch64.manylinux2014_aarch64"
                ;;
            *)
                echo "Unsupported architecture: $ARCH"
                exit 1
                ;;
        esac
        ;;
    Darwin)
        case "$ARCH" in
            arm64)
                WHEEL_PLATFORM="macosx_11_0_arm64"
                ;;
            x86_64)
                WHEEL_PLATFORM="macosx_10_12_x86_64"
                ;;
            *)
                echo "Unsupported architecture: $ARCH"
                exit 1
                ;;
        esac
        ;;
    *)
        echo "Unsupported OS: $OS"
        exit 1
        ;;
esac

WHEEL_NAME="ferrumpy-${FERRUMPY_VERSION}-cp39-abi3-${WHEEL_PLATFORM}.whl"
WHEEL_URL="https://files.pythonhosted.org/packages/cp39/${WHEEL_NAME:0:2}/$WHEEL_NAME"

# Try alternative URL structure
PYPI_JSON_URL="https://pypi.org/pypi/ferrumpy/${FERRUMPY_VERSION}/json"

echo "Detecting wheel URL from PyPI..."
WHEEL_URL=$(curl -sL "$PYPI_JSON_URL" | python3 -c "
import json, sys
data = json.load(sys.stdin)
for url_info in data.get('urls', []):
    filename = url_info.get('filename', '')
    if '$WHEEL_PLATFORM' in filename or filename.endswith('.whl'):
        print(url_info['url'])
        break
" 2>/dev/null || echo "")

if [ -z "$WHEEL_URL" ]; then
    echo "Could not find wheel for platform: $WHEEL_PLATFORM"
    echo "Please download manually from: https://pypi.org/project/ferrumpy/#files"
    exit 1
fi

echo "Platform: $OS / $ARCH"
echo "Wheel: $(basename $WHEEL_URL)"
echo "Install directory: $INSTALL_DIR"
echo

# Create install directory
mkdir -p "$INSTALL_DIR"

# Download wheel
echo "Downloading wheel..."
TEMP_WHEEL=$(mktemp)
curl -sL "$WHEEL_URL" -o "$TEMP_WHEEL"

# Extract wheel (it's just a zip file)
echo "Extracting..."
cd "$INSTALL_DIR"
unzip -q -o "$TEMP_WHEEL"
rm "$TEMP_WHEEL"

# Find ferrumpy package directory
FERRUMPY_PATH="$INSTALL_DIR/ferrumpy"

if [ ! -d "$FERRUMPY_PATH" ]; then
    echo "Error: ferrumpy package not found after extraction"
    exit 1
fi

echo "Installed to: $FERRUMPY_PATH"

# Configure LLDB
LLDB_CMD="command script import $FERRUMPY_PATH"

if grep -q "ferrumpy" "$LLDBINIT" 2>/dev/null; then
    echo
    echo "Note: ~/.lldbinit already contains ferrumpy configuration."
    echo "You may want to update it to:"
    echo "  $LLDB_CMD"
else
    echo
    read -p "Add FerrumPy to ~/.lldbinit? [Y/n] " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Nn]$ ]]; then
        echo "$LLDB_CMD" >> "$LLDBINIT"
        echo "Added to ~/.lldbinit"
    else
        echo "Skipped. Add this line manually to ~/.lldbinit:"
        echo "  $LLDB_CMD"
    fi
fi

echo
echo "=== Installation Complete ==="
echo
echo "Start LLDB and use: ferrumpy help"
