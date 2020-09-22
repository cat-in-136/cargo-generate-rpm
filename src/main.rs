use crate::config::Config;
use crate::error::Error;
use getopts::Options;
use std::env;
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
    let program = env::args().nth(0).unwrap();

    let mut opts = Options::new();
    opts.optflag("h", "help", "print this help menu");
    let opt_matches = opts.parse(env::args().skip(1)).unwrap_or_else(|err| {
        eprintln!("{}: {}", program, err);
        std::process::exit(1);
    });
    if opt_matches.opt_present("h") {
        println!("{}", opts.usage(&*format!("Usage: {} [options]", program)));
    }

    if let Some(_) = std::env::var_os("RUST_BACKTRACE") {
        process().unwrap();
    } else {
        process().unwrap_or_else(|err| {
            eprintln!("{}: {}", program, err);
            std::process::exit(1);
        });
    }
}
