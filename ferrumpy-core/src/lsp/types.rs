//! LSP types for FerrumPy

use serde::{Deserialize, Serialize};

/// Completion item from rust-analyzer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: Option<String>,
    pub documentation: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CompletionKind {
    Field,
    Method,
    Function,
    Variable,
    Struct,
    Enum,
    Module,
    Keyword,
    Snippet,
    Property,
    Constant,
    Other,
}

impl From<lsp_types::CompletionItemKind> for CompletionKind {
    fn from(kind: lsp_types::CompletionItemKind) -> Self {
        match kind {
            lsp_types::CompletionItemKind::FIELD => CompletionKind::Field,
            lsp_types::CompletionItemKind::METHOD => CompletionKind::Method,
            lsp_types::CompletionItemKind::FUNCTION => CompletionKind::Function,
            lsp_types::CompletionItemKind::VARIABLE => CompletionKind::Variable,
            lsp_types::CompletionItemKind::STRUCT => CompletionKind::Struct,
            lsp_types::CompletionItemKind::ENUM => CompletionKind::Enum,
            lsp_types::CompletionItemKind::MODULE => CompletionKind::Module,
            lsp_types::CompletionItemKind::KEYWORD => CompletionKind::Keyword,
            lsp_types::CompletionItemKind::SNIPPET => CompletionKind::Snippet,
            lsp_types::CompletionItemKind::PROPERTY => CompletionKind::Property,
            lsp_types::CompletionItemKind::CONSTANT => CompletionKind::Constant,
            _ => CompletionKind::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completion_kind_serde() {
        let kind = CompletionKind::Field;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, "\"field\"");
    }
}
