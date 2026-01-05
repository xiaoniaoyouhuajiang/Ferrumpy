use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

// Import types from our library modules
mod types;
mod utils;

use types::{Config, Container, DatabaseConfig, Priority, Status, User};

fn main() {
    // Test various Rust types
    let simple_string = String::from("Hello, FerrumPy!");
    let empty_string = String::new();

    let numbers: Vec<i32> = vec![1, 2, 3, 4, 5];
    let empty_vec: Vec<i32> = vec![];

    let some_value: Option<i32> = Some(42);
    let none_value: Option<i32> = None;

    let ok_result: Result<i32, String> = Ok(100);
    let err_result: Result<i32, String> = Err("something went wrong".to_string());

    let mut map: HashMap<String, i32> = HashMap::new();
    map.insert("one".to_string(), 1);
    map.insert("two".to_string(), 2);
    map.insert("three".to_string(), 3);

    let arc_value = Arc::new(User::new("Arc User", 30).with_email("arc@example.com"));

    let rc_value = Rc::new(42);

    let boxed = Box::new("boxed string slice");

    // Nested Vec for testing matrix[i][j] access
    let matrix: Vec<Vec<i32>> = vec![vec![1, 2, 3], vec![4, 5, 6], vec![7, 8, 9]];

    // Fixed size array for testing [T; N] access
    let fixed_array: [i32; 5] = [10, 20, 30, 40, 50];

    let refcell = RefCell::new(vec![1, 2, 3]);

    // Nested structure for path testing (using types from types.rs)
    let config = Config {
        database: DatabaseConfig {
            host: "localhost".to_string(),
            port: 5432,
        },
        users: vec![
            User::new("Alice", 25).with_email("alice@example.com"),
            User::new("Bob", 30),
        ],
    };

    // Tuple for testing tuple access
    let tuple = ("first", 2, 3.14);

    // ===== NEW: Enum test cases =====

    // Unit variant
    let status_active = Status::Active;

    // Tuple variant with payload
    let status_pending = Status::Pending(7);

    // Struct variant with named fields
    let status_inactive = Status::Inactive {
        reason: "on vacation".to_string(),
    };

    // C-like enum (simple discriminant)
    let priority_low = Priority::Low;
    let priority_high = Priority::High;

    // Generic container type
    let container = Container::new(42).with_meta("source", "test");

    // Set a breakpoint here to test pretty printers
    println!("=== FerrumPy Test Program ===");
    println!("Set a breakpoint on this line to test pretty printers");

    // Use all variables to prevent optimization
    println!("simple_string: {:?}", simple_string);
    println!("empty_string: {:?}", empty_string);
    println!("numbers: {:?}", numbers);
    println!("empty_vec: {:?}", empty_vec);
    println!("some_value: {:?}", some_value);
    println!("none_value: {:?}", none_value);
    println!("ok_result: {:?}", ok_result);
    println!("err_result: {:?}", err_result);
    println!("map: {:?}", map);
    println!("arc_value: {:?}", arc_value);
    println!("rc_value: {:?}", rc_value);
    println!("boxed: {:?}", boxed);
    println!("refcell: {:?}", refcell);
    println!("matrix: {:?}", matrix);
    println!("fixed_array: {:?}", fixed_array);
    println!("config: {:?}", config);
    println!("tuple: {:?}", tuple);

    // Print enum values
    println!("status_active: {:?}", status_active);
    println!("status_pending: {:?}", status_pending);
    println!("status_inactive: {:?}", status_inactive);
    println!("priority_low: {:?}", priority_low);
    println!("priority_high: {:?}", priority_high);
    println!("container: {:?}", container);
}
