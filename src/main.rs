use crate::config::Config;
use crate::error::Error;
use std::fs::File;
use std::path::Path;

mod config;
mod error;

fn process() -> Result<(), Error> {
    let config = Config::new("Cargo.toml")?;

    let rpm_pkg = config.create_rpm_builder(None)?.build()?;
    let file_name = Path::new("target").join(format!(
        "{}-{}{}{}.rpm",
        rpm_pkg.metadata.header.get_name()?,
        rpm_pkg.metadata.header.get_version()?,
        rpm_pkg
            .metadata
            .header
            .get_release()
            .map(|v| format!("-{}", v))
            .unwrap_or_default(),
        rpm_pkg
            .metadata
            .header
            .get_arch()
            .map(|v| format!(".{}", v))
            .unwrap_or_default(),
    ));
    let mut f = File::create(file_name).unwrap();
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
