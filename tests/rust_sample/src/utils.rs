//! Utility functions for FerrumPy REPL testing
//!
//! These functions can be used to test function invocation in REPL
//! (requires manual pasting or companion lib enhancement).

/// Simple greeting function
pub fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}

/// Add two numbers
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

/// Calculate factorial
pub fn factorial(n: u32) -> u32 {
    if n <= 1 {
        1
    } else {
        n * factorial(n - 1)
    }
}

/// Check if a string is a palindrome
pub fn is_palindrome(s: &str) -> bool {
    let chars: Vec<char> = s.chars().filter(|c| c.is_alphanumeric()).collect();
    let lower: String = chars.iter().map(|c| c.to_ascii_lowercase()).collect();
    lower == lower.chars().rev().collect::<String>()
}
