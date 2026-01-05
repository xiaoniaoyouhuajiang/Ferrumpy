//! Shared types for FerrumPy REPL testing
//!
//! This module contains user-defined types that will be serialized
//! and restored in the REPL environment.

use std::collections::HashMap;

/// A sample struct for testing pretty printers
#[derive(Debug, Clone)]
pub struct User {
    pub name: String,
    pub age: u32,
    pub email: Option<String>,
}

impl User {
    pub fn new(name: &str, age: u32) -> Self {
        Self {
            name: name.to_string(),
            age,
            email: None,
        }
    }

    pub fn with_email(mut self, email: &str) -> Self {
        self.email = Some(email.to_string());
        self
    }

    pub fn change_age(&mut self, age: u32) {
        self.age = age;
    }

    pub fn greet(&self) -> String {
        format!("Hello, I'm {}!", self.name)
    }
}

/// Nested struct for testing path access
#[derive(Debug, Clone)]
pub struct Config {
    pub database: DatabaseConfig,
    pub users: Vec<User>,
}

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
}

/// Enum with various variant types for testing enum serialization
#[derive(Debug, Clone)]
pub enum Status {
    /// Unit variant
    Active,
    /// Tuple variant with payload
    Pending(u32),
    /// Struct variant with named fields
    Inactive { reason: String },
}

impl Status {
    pub fn description(&self) -> String {
        match self {
            Status::Active => "Active and ready".to_string(),
            Status::Pending(days) => format!("Pending for {} days", days),
            Status::Inactive { reason } => format!("Inactive: {}", reason),
        }
    }
}

/// Enum for testing simple C-like enums
#[derive(Debug, Clone, Copy)]
pub enum Priority {
    Low = 1,
    Medium = 2,
    High = 3,
}

/// Generic container for testing generic type restoration
#[derive(Debug, Clone)]
pub struct Container<T> {
    pub value: T,
    pub metadata: HashMap<String, String>,
}

impl<T> Container<T> {
    pub fn new(value: T) -> Self {
        Self {
            value,
            metadata: HashMap::new(),
        }
    }

    pub fn with_meta(mut self, key: &str, val: &str) -> Self {
        self.metadata.insert(key.to_string(), val.to_string());
        self
    }
}
