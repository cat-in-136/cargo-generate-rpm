use crate::config::Config;
use crate::error::Error;
use std::fs::File;

mod config;
mod error;

fn process() -> Result<(), Error> {
    let config = Config::new("Cargo.toml")?;

    let rpm_pkg = config.create_rpm_builder("x86_64")?.build()?;
    let mut f = File::create("target/package.rpm").unwrap(); // TODO get package name
    rpm_pkg.write(&mut f)?;

    Ok(())
}

fn main() {
    if let Some(_) = std::env::var_os("RUST_BACKTRACE") {
        process().unwrap();
    } else {
        let result = process();
        if let Err(err) = result {
            eprintln!("cargo-binary-rpm: {}", err);
            std::process::exit(1);
        }
    }
}
