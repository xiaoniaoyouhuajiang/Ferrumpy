//! JSON-RPC Protocol definitions
//!
//! Defines the communication protocol between Python bridge and ferrumpy-server.

use crate::dwarf::VariableInfo;
use crate::lsp::CompletionItem;
use serde::{Deserialize, Serialize};

/// Frame information from LLDB
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameInfo {
    /// Function name
    pub function: String,
    /// Source file path
    pub file: Option<String>,
    /// Line number
    pub line: Option<u32>,
    /// Local variables
    pub locals: Vec<VariableInfo>,
}

/// Request from Python to ferrumpy-server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum Request {
    /// Initialize the server for a project
    #[serde(rename = "initialize")]
    Initialize { project_root: String },

    /// Request completions
    #[serde(rename = "complete")]
    Complete {
        frame: FrameInfo,
        input: String,
        cursor: usize,
    },

    /// Request type information
    #[serde(rename = "type")]
    TypeInfo { frame: FrameInfo, expr: String },

    /// Evaluate an expression
    #[serde(rename = "eval")]
    Eval { frame: FrameInfo, expr: String },

    /// Request hover documentation
    #[serde(rename = "hover")]
    Hover { frame: FrameInfo, path: String },

    /// Shutdown the server
    #[serde(rename = "shutdown")]
    Shutdown,
}

/// Response from ferrumpy-server to Python
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Response {
    Completions { completions: Vec<CompletionItem> },
    TypeInfo { type_name: String },
    EvalResult { value: String, value_type: String },
    Hover { content: Option<String> },
    Success { ok: bool },
    Error { error: String },
}

impl Response {
    pub fn success() -> Self {
        Response::Success { ok: true }
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Response::Error { error: msg.into() }
    }

    pub fn completions(items: Vec<CompletionItem>) -> Self {
        Response::Completions { completions: items }
    }

    pub fn eval_result(value: impl Into<String>, value_type: impl Into<String>) -> Self {
        Response::EvalResult {
            value: value.into(),
            value_type: value_type.into(),
        }
    }
}

/// JSON-RPC message wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcMessage<T> {
    pub jsonrpc: String,
    pub id: Option<u64>,
    #[serde(flatten)]
    pub content: T,
}

impl<T> RpcMessage<T> {
    pub fn new(id: u64, content: T) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: Some(id),
            content,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialize() {
        let req = Request::Complete {
            frame: FrameInfo {
                function: "main".to_string(),
                file: Some("/path/to/file.rs".to_string()),
                line: Some(42),
                locals: vec![],
            },
            input: "user.".to_string(),
            cursor: 5,
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"method\":\"complete\""));
    }

    #[test]
    fn test_response_serialize() {
        let resp = Response::completions(vec![CompletionItem {
            label: "name".to_string(),
            kind: crate::lsp::CompletionKind::Field,
            detail: Some("String".to_string()),
            documentation: None,
        }]);

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"label\":\"name\""));
    }
}
