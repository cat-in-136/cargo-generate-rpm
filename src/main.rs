use crate::config::Config;

mod config;
mod error;

fn main() {
    let config = Config::new("Cargo.toml").unwrap();
    let _builder = config.create_rpm_builder("x86_64").unwrap();

    println!("Hello, world!");
}
