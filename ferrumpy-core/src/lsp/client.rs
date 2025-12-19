//! rust-analyzer LSP client
//!
//! Communicates with rust-analyzer subprocess using JSON-RPC over stdio.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::types::{CompletionItem, CompletionKind};

/// JSON-RPC request
#[derive(Debug, Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: u64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

/// JSON-RPC response
#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: Option<u64>,
    result: Option<Value>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

/// rust-analyzer client
pub struct RustAnalyzerClient {
    project_root: PathBuf,
    process: Option<Child>,
    request_id: AtomicU64,
    initialized: bool,
}

impl RustAnalyzerClient {
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
            process: None,
            request_id: AtomicU64::new(1),
            initialized: false,
        }
    }

    /// Start rust-analyzer process and initialize LSP
    pub fn start(&mut self) -> Result<()> {
        if self.process.is_some() {
            return Ok(());
        }

        // Find rust-analyzer binary
        let ra_path = Self::find_rust_analyzer()?;

        // Start process
        let child = Command::new(&ra_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("Failed to start rust-analyzer at {:?}", ra_path))?;

        self.process = Some(child);

        // Send initialize request
        self.send_initialize()?;
        self.initialized = true;

        Ok(())
    }

    /// Find rust-analyzer binary
    fn find_rust_analyzer() -> Result<PathBuf> {
        // Try common locations
        let candidates = [
            "rust-analyzer",
            "/usr/local/bin/rust-analyzer",
            "/opt/homebrew/bin/rust-analyzer",
        ];

        for path in candidates {
            if let Ok(output) = Command::new(path).arg("--version").output() {
                if output.status.success() {
                    return Ok(PathBuf::from(path));
                }
            }
        }

        // Try rustup
        if let Ok(output) = Command::new("rustup")
            .args(["which", "rust-analyzer"])
            .output()
        {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return Ok(PathBuf::from(path));
                }
            }
        }

        anyhow::bail!("rust-analyzer not found. Install with: rustup component add rust-analyzer")
    }

    /// Send initialize request
    fn send_initialize(&mut self) -> Result<()> {
        let init_params = json!({
            "processId": std::process::id(),
            "rootUri": format!("file://{}", self.project_root.display()),
            "capabilities": {
                "textDocument": {
                    "completion": {
                        "completionItem": {
                            "snippetSupport": false,
                            "documentationFormat": ["plaintext"]
                        }
                    }
                }
            }
        });

        let response = self.send_request("initialize", Some(init_params))?;

        if response.error.is_some() {
            anyhow::bail!("Initialize failed: {:?}", response.error);
        }

        // Send initialized notification
        self.send_notification("initialized", Some(json!({})))?;

        Ok(())
    }

    /// Send a JSON-RPC request and wait for response
    fn send_request(&mut self, method: &str, params: Option<Value>) -> Result<JsonRpcResponse> {
        let process = self
            .process
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Process not started"))?;

        let stdin = process
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("No stdin"))?;

        let stdout = process
            .stdout
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("No stdout"))?;

        let id = self.request_id.fetch_add(1, Ordering::SeqCst);

        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        };

        let content = serde_json::to_string(&request)?;
        let header = format!("Content-Length: {}\r\n\r\n", content.len());

        stdin.write_all(header.as_bytes())?;
        stdin.write_all(content.as_bytes())?;
        stdin.flush()?;

        // Read response
        let mut reader = BufReader::new(stdout);
        let mut headers = String::new();
        let mut content_length = 0usize;

        // Read headers
        loop {
            headers.clear();
            reader.read_line(&mut headers)?;

            if headers == "\r\n" {
                break;
            }

            if headers.starts_with("Content-Length:") {
                content_length = headers
                    .trim_start_matches("Content-Length:")
                    .trim()
                    .parse()?;
            }
        }

        // Read body
        let mut body = vec![0u8; content_length];
        std::io::Read::read_exact(&mut reader, &mut body)?;

        let response: JsonRpcResponse = serde_json::from_slice(&body)?;

        Ok(response)
    }

    /// Send a notification (no response expected)
    fn send_notification(&mut self, method: &str, params: Option<Value>) -> Result<()> {
        let process = self
            .process
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Process not started"))?;

        let stdin = process
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("No stdin"))?;

        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        let content = serde_json::to_string(&notification)?;
        let header = format!("Content-Length: {}\r\n\r\n", content.len());

        stdin.write_all(header.as_bytes())?;
        stdin.write_all(content.as_bytes())?;
        stdin.flush()?;

        Ok(())
    }

    /// Open a virtual document for completion analysis
    pub fn open_virtual_document(&mut self, uri: &str, content: &str) -> Result<()> {
        if !self.initialized {
            self.start()?;
        }

        self.send_notification(
            "textDocument/didOpen",
            Some(json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": "rust",
                    "version": 1,
                    "text": content
                }
            })),
        )?;

        Ok(())
    }

    /// Request completions at a position
    pub fn completions(
        &mut self,
        uri: &str,
        line: u32,
        character: u32,
    ) -> Result<Vec<CompletionItem>> {
        if !self.initialized {
            self.start()?;
        }

        let params = json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character }
        });

        let response = self.send_request("textDocument/completion", Some(params))?;

        if let Some(error) = response.error {
            anyhow::bail!(
                "Completion request failed: {} ({})",
                error.message,
                error.code
            );
        }

        let result = response.result.unwrap_or(Value::Null);

        // Parse completion response
        let items = if result.is_array() {
            serde_json::from_value::<Vec<lsp_types::CompletionItem>>(result)?
        } else if result.is_object() {
            let list: lsp_types::CompletionResponse = serde_json::from_value(result)?;
            match list {
                lsp_types::CompletionResponse::Array(items) => items,
                lsp_types::CompletionResponse::List(list) => list.items,
            }
        } else {
            Vec::new()
        };

        // Convert to our types
        let completions = items
            .into_iter()
            .map(|item| CompletionItem {
                label: item.label,
                kind: item
                    .kind
                    .map(CompletionKind::from)
                    .unwrap_or(CompletionKind::Other),
                detail: item.detail,
                documentation: item.documentation.map(|doc| match doc {
                    lsp_types::Documentation::String(s) => s,
                    lsp_types::Documentation::MarkupContent(m) => m.value,
                }),
            })
            .collect();

        Ok(completions)
    }

    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
}

impl Drop for RustAnalyzerClient {
    fn drop(&mut self) {
        if let Some(mut process) = self.process.take() {
            let _ = self.send_notification("shutdown", None);
            let _ = process.wait();
        }
    }
}
