use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

/// A sample struct for testing pretty printers
#[derive(Debug)]
struct User {
    name: String,
    age: u32,
    email: Option<String>,
}

/// Nested struct for testing path access
#[derive(Debug)]
struct Config {
    database: DatabaseConfig,
    users: Vec<User>,
}

impl User {
    pub fn change_age(&mut self, age: u32) {
        self.age = age;
    }
}

#[derive(Debug)]
struct DatabaseConfig {
    host: String,
    port: u16,
}

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

    let arc_value = Arc::new(User {
        name: "Arc User".to_string(),
        age: 30,
        email: Some("arc@example.com".to_string()),
    });

    let rc_value = Rc::new(42);

    let boxed = Box::new("boxed string slice");

    // Nested Vec for testing matrix[i][j] access
    let matrix: Vec<Vec<i32>> = vec![vec![1, 2, 3], vec![4, 5, 6], vec![7, 8, 9]];

    // Fixed size array for testing [T; N] access
    let fixed_array: [i32; 5] = [10, 20, 30, 40, 50];

    let refcell = RefCell::new(vec![1, 2, 3]);

    // Nested structure for path testing
    let config = Config {
        database: DatabaseConfig {
            host: "localhost".to_string(),
            port: 5432,
        },
        users: vec![
            User {
                name: "Alice".to_string(),
                age: 25,
                email: Some("alice@example.com".to_string()),
            },
            User {
                name: "Bob".to_string(),
                age: 30,
                email: None,
            },
        ],
    };

    // Tuple for testing tuple access
    let tuple = ("first", 2, 3.14);

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
}
