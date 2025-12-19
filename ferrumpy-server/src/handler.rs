//! Request handler for ferrumpy-server

use ferrumpy_core::{Request, Response};
use ferrumpy_core::lsp::{RustAnalyzerClient, CompletionItem, CompletionKind};
use ferrumpy_core::expr::{parse_expr, Evaluator, Value};
use tracing::{info, debug, warn};

pub struct Handler {
    ra_client: Option<RustAnalyzerClient>,
    project_root: Option<String>,
}

impl Handler {
    pub fn new() -> Self {
        Self {
            ra_client: None,
            project_root: None,
        }
    }
    
    pub fn handle(&mut self, request: &Request) -> Response {
        match request {
            Request::Initialize { project_root } => {
                self.handle_initialize(project_root)
            }
            Request::Complete { frame, input, cursor } => {
                self.handle_complete(frame, input, *cursor)
            }
            Request::TypeInfo { frame, expr } => {
                self.handle_type_info(frame, expr)
            }
            Request::Eval { frame, expr } => {
                self.handle_eval(frame, expr)
            }
            Request::Hover { frame, path } => {
                self.handle_hover(frame, path)
            }
            Request::Shutdown => {
                info!("Shutdown requested");
                Response::success()
            }
        }
    }
    
    fn handle_initialize(&mut self, project_root: &str) -> Response {
        info!("Initializing for project: {}", project_root);
        
        self.project_root = Some(project_root.to_string());
        
        // Create rust-analyzer client
        let mut client = RustAnalyzerClient::new(project_root);
        
        // Try to start rust-analyzer
        match client.start() {
            Ok(()) => {
                info!("rust-analyzer started successfully");
                self.ra_client = Some(client);
            }
            Err(e) => {
                warn!("Failed to start rust-analyzer: {}. Falling back to basic completions.", e);
                // Keep client anyway for basic functionality
                self.ra_client = Some(RustAnalyzerClient::new(project_root));
            }
        }
        
        Response::success()
    }
    
    fn handle_complete(
        &mut self,
        frame: &ferrumpy_core::protocol::FrameInfo,
        input: &str,
        cursor: usize,
    ) -> Response {
        debug!("Complete request: input={}, cursor={}", input, cursor);
        
        let mut completions = Vec::new();
        
        // Try rust-analyzer first if available and input contains '.'
        if input.ends_with('.') {
            // Take the client out to avoid borrow issues
            if let Some(mut ra) = self.ra_client.take() {
                if ra.is_initialized() {
                    // Generate virtual scope for RA
                    let virtual_content = Self::generate_virtual_scope_static(frame);
                    let uri = "file:///tmp/__ferrumpy_scope.rs";
                    
                    if ra.open_virtual_document(uri, &virtual_content).is_ok() {
                        let lines: Vec<&str> = virtual_content.lines().collect();
                        let line = lines.len().saturating_sub(1) as u32;
                        let character = lines.last().map(|l| l.len()).unwrap_or(0) as u32;
                        
                        if let Ok(items) = ra.completions(uri, line, character) {
                            if !items.is_empty() {
                                self.ra_client = Some(ra);
                                return Response::completions(items);
                            }
                        }
                    }
                }
                // Put it back
                self.ra_client = Some(ra);
            }
            
            // Fallback: suggest based on type info from locals
            let var_name = input.trim_end_matches('.');
            for local in &frame.locals {
                if local.name == var_name {
                    completions.push(CompletionItem {
                        label: format!("/* {} has no field info available */", local.rust_type),
                        kind: CompletionKind::Field,
                        detail: Some(format!("Type: {}", local.rust_type)),
                        documentation: Some("Use 'ferrumpy type' to see fields".to_string()),
                    });
                }
            }
        } else {
            // Suggest local variables matching prefix
            for local in &frame.locals {
                if local.name.starts_with(input) {
                    completions.push(CompletionItem {
                        label: local.name.clone(),
                        kind: CompletionKind::Variable,
                        detail: Some(local.rust_type.clone()),
                        documentation: None,
                    });
                }
            }
        }
        
        Response::completions(completions)
    }
    
    #[allow(dead_code)]
    fn try_ra_completions(
        &self,
        ra: &mut RustAnalyzerClient,
        frame: &ferrumpy_core::protocol::FrameInfo,
        _input: &str,
    ) -> Option<Vec<CompletionItem>> {
        // Generate virtual scope file
        let virtual_content = Self::generate_virtual_scope_static(frame);
        let uri = "file:///tmp/__ferrumpy_scope.rs";
        
        // Open virtual document
        if let Err(e) = ra.open_virtual_document(uri, &virtual_content) {
            debug!("Failed to open virtual document: {}", e);
            return None;
        }
        
        // Calculate cursor position (end of file)
        let lines: Vec<&str> = virtual_content.lines().collect();
        let line = lines.len().saturating_sub(1) as u32;
        let character = lines.last().map(|l| l.len()).unwrap_or(0) as u32;
        
        // Request completions
        match ra.completions(uri, line, character) {
            Ok(items) => {
                debug!("Got {} completions from rust-analyzer", items.len());
                Some(items)
            }
            Err(e) => {
                debug!("Completion request failed: {}", e);
                None
            }
        }
    }
    
    fn generate_virtual_scope_static(frame: &ferrumpy_core::protocol::FrameInfo) -> String {
        let mut code = String::new();
        
        // Add a function scope with local variable declarations
        code.push_str("fn __ferrumpy_scope() {\n");
        
        for local in &frame.locals {
            // Declare variables with their types
            // Note: We use `todo!()` as placeholder since we don't have actual values
            code.push_str(&format!("    let {}: {} = todo!();\n", local.name, local.rust_type));
        }
        
        // Add cursor position marker
        code.push_str("    // Cursor here\n");
        code.push_str("}\n");
        
        code
    }
    
    fn handle_type_info(
        &self,
        frame: &ferrumpy_core::protocol::FrameInfo,
        expr: &str,
    ) -> Response {
        debug!("Type info request: expr={}", expr);
        
        // Simple lookup in locals
        for local in &frame.locals {
            if local.name == expr {
                return Response::TypeInfo {
                    type_name: local.rust_type.clone(),
                };
            }
        }
        
        Response::error(format!("Unknown expression: {}", expr))
    }
    
    fn handle_eval(
        &self,
        frame: &ferrumpy_core::protocol::FrameInfo,
        expr_str: &str,
    ) -> Response {
        debug!("Eval request: expr={}", expr_str);
        
        // Parse expression
        let ast = match parse_expr(expr_str) {
            Ok(ast) => ast,
            Err(e) => return Response::error(e.to_string()),
        };
        
        // Build evaluator with variables from frame
        let mut evaluator = Evaluator::new();
        
        // Add local variables to evaluator
        // Note: Currently we only support primitive types
        for local in &frame.locals {
            if let Some(value) = self.parse_variable_value(&local.rust_type, &local.value) {
                evaluator.set_variable(&local.name, value);
            }
        }
        
        // Evaluate
        match evaluator.eval(&ast) {
            Ok(value) => {
                Response::eval_result(
                    value.to_string(),
                    value.type_name(),
                )
            }
            Err(e) => Response::error(e.to_string()),
        }
    }
    
    /// Parse a variable value string to Value
    /// This is a simplified parser for common types
    fn parse_variable_value(&self, type_name: &str, value_str: &str) -> Option<Value> {
        let type_name = type_name.trim();
        let value_str = value_str.trim();
        
        match type_name {
            "i8" => value_str.parse().ok().map(Value::I8),
            "i16" => value_str.parse().ok().map(Value::I16),
            "i32" => value_str.parse().ok().map(Value::I32),
            "i64" => value_str.parse().ok().map(Value::I64),
            "i128" => value_str.parse().ok().map(Value::I128),
            "isize" => value_str.parse().ok().map(Value::Isize),
            "u8" => value_str.parse().ok().map(Value::U8),
            "u16" => value_str.parse().ok().map(Value::U16),
            "u32" => value_str.parse().ok().map(Value::U32),
            "u64" => value_str.parse().ok().map(Value::U64),
            "u128" => value_str.parse().ok().map(Value::U128),
            "usize" => value_str.parse().ok().map(Value::Usize),
            "f32" => value_str.parse().ok().map(Value::F32),
            "f64" => value_str.parse().ok().map(Value::F64),
            "bool" => value_str.parse().ok().map(Value::Bool),
            _ => None, // Complex types not yet supported
        }
    }
    
    fn handle_hover(
        &self,
        _frame: &ferrumpy_core::protocol::FrameInfo,
        path: &str,
    ) -> Response {
        debug!("Hover request: path={}", path);
        
        // TODO: Use rust-analyzer for hover info
        Response::Hover { content: None }
    }
}

