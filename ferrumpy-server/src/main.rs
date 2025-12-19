//! FerrumPy Server
//!
//! JSON-RPC server that bridges Python LLDB scripts with Rust functionality.
//! Communicates via stdin/stdout for easy subprocess management.

use std::io::{self, BufRead, Write};
use anyhow::Result;
use tracing::{info, error, debug};
use ferrumpy_core::{Request, Response};

mod handler;

fn main() -> Result<()> {
    // Initialize logging to stderr (stdout is for JSON-RPC)
    tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .init();
    
    info!("ferrumpy-server starting...");
    
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    
    let mut handler = handler::Handler::new();
    
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                error!("Failed to read line: {}", e);
                continue;
            }
        };
        
        if line.is_empty() {
            continue;
        }
        
        debug!("Received: {}", line);
        
        // Parse JSON-RPC request
        let response = match serde_json::from_str::<ferrumpy_core::protocol::RpcMessage<Request>>(&line) {
            Ok(msg) => {
                let result = handler.handle(&msg.content);
                ferrumpy_core::protocol::RpcMessage::new(msg.id.unwrap_or(0), result)
            }
            Err(e) => {
                ferrumpy_core::protocol::RpcMessage::new(0, Response::error(format!("Parse error: {}", e)))
            }
        };
        
        // Send response
        let response_json = serde_json::to_string(&response)?;
        debug!("Sending: {}", response_json);
        writeln!(stdout, "{}", response_json)?;
        stdout.flush()?;
    }
    
    info!("ferrumpy-server shutting down");
    Ok(())
}
