#!/usr/bin/env python3
"""
Cache Prewarming Script for FerrumPy REPL

This script pre-compiles common dependencies (serde, serde_json) to speed up
first-time REPL usage. Run this after installing FerrumPy.

Usage:
    python -m ferrumpy.warm_cache
    # or
    ferrumpy warm-cache
"""

import sys
import time


def warm_cache():
    """Prewarm the evcxr cache with common dependencies."""
    print("[FerrumPy] Prewarming cache...")
    print("[FerrumPy] This will take 30-60 seconds on first run.")

    start = time.time()

    try:
        from ferrumpy import ferrumpy_core
    except ImportError:
        try:
            import ferrumpy_core
        except ImportError:
            print("[FerrumPy] Error: ferrumpy_core not found. Is FerrumPy installed?")
            return 1

    try:
        # Create a REPL session - this enables cache automatically
        print("[FerrumPy] Creating REPL session...")
        session = ferrumpy_core.PyReplSession()

        # Add serde dependencies (these are cached after first compile)
        print("[FerrumPy] Compiling serde...")
        session.eval(':dep serde = { version = "1", features = ["derive"] }')

        print("[FerrumPy] Compiling serde_json...")
        session.eval(':dep serde_json = "1"')

        # Verify they work
        print("[FerrumPy] Verifying compilation...")
        session.eval('use serde::{Serialize, Deserialize};')
        session.eval('let _: serde_json::Value = serde_json::json!({"test": 1});')

        elapsed = time.time() - start
        print(f"[FerrumPy] Cache warmed successfully in {elapsed:.1f}s")
        print("[FerrumPy] Future REPL starts will be much faster!")
        return 0

    except Exception as e:
        print(f"[FerrumPy] Error during cache warming: {e}")
        return 1


def main():
    sys.exit(warm_cache())


if __name__ == "__main__":
    main()
