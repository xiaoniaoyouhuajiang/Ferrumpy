//! FerrumPy REPL Worker
//!
//! This is a minimal binary that serves as the subprocess for evcxr evaluation.
//! It must call `runtime_hook()` at startup to handle subprocess execution.

use anyhow::Result;

fn main() -> Result<()> {
    // CRITICAL: This must be called at the very start!
    // It checks if we're running as an evcxr subprocess and if so,
    // takes over execution (does not return).
    evcxr::runtime_hook();

    // If we reach here, we're the main process.
    // This binary is not meant to be run directly, but let's handle it gracefully.
    eprintln!("ferrumpy-repl-worker: This binary is meant to be used by ferrumpy internally.");
    eprintln!("Use 'ferrumpy repl' instead.");

    Ok(())
}
