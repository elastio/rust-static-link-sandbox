use rusqlite::{Connection};

fn main() {
    let _conn = Connection::open_in_memory().unwrap();

    println!("Hello, world!");
}
