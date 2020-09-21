use crate::config::Config;
use std::fs::File;

mod config;
mod error;

fn main() {
    let config = Config::new("Cargo.toml").unwrap();

    let rpm_pkg = config.create_rpm_builder("x86_64").unwrap()
        .build().unwrap();
    let mut f = File::create("target/package.rpm").unwrap(); // TODO get package name
    rpm_pkg.write(&mut f).unwrap();
}
