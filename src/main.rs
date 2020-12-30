use crate::config::Config;
use crate::error::Error;
use getopts::Options;
use std::env;
use std::fs::{create_dir_all, File};
use std::path::{Path, PathBuf};

mod config;
mod error;

fn process(target_arch: Option<String>, target_file: Option<PathBuf>) -> Result<(), Error> {
    let config = Config::new("Cargo.toml")?;

    let rpm_pkg = config.create_rpm_builder(target_arch)?.build()?;

    let default_file_name = Path::new("target").join("generate-rpm").join(format!(
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
    let target_file_name = target_file.unwrap_or(default_file_name);
    if let Some(parent_dir) = target_file_name.parent() {
        if !parent_dir.exists() {
            create_dir_all(parent_dir)?;
        }
    }
    let mut f = File::create(target_file_name)?;
    rpm_pkg.write(&mut f)?;

    Ok(())
}

fn main() {
    let program = env::args().nth(0).unwrap();

    let mut opts = Options::new();
    opts.optopt("a", "arch", "set target arch", "ARCH");
    opts.optopt("o", "output", "set output file", "OUTPUT.rpm");
    opts.optflag("h", "help", "print this help menu");
    let opt_matches = opts.parse(env::args().skip(1)).unwrap_or_else(|err| {
        eprintln!("{}: {}", program, err);
        std::process::exit(1);
    });
    if opt_matches.opt_present("h") {
        println!("{}", opts.usage(&*format!("Usage: {} [options]", program)));
    }
    let target_arch = opt_matches.opt_str("a");
    let target_file = opt_matches.opt_str("o").map(|v| PathBuf::from(v));

    if let Some(_) = std::env::var_os("RUST_BACKTRACE") {
        process(target_arch, target_file).unwrap();
    } else {
        process(target_arch, target_file).unwrap_or_else(|err| {
            eprintln!("{}: {}", program, err);
            std::process::exit(1);
        });
    }
}
