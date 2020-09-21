use crate::config::Config;

mod config;
mod error;

fn main() {
    let config = Config::new("Cargo.toml").unwrap();
    println!("{:?}", config);

    println!("Hello, world!");
}
