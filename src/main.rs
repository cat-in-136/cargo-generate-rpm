use crate::auto_req::AutoReqMode;
use crate::config::Config;
use crate::error::Error;
use getopts::Options;
use std::convert::TryFrom;
use std::env;
use std::fs::{create_dir_all, File};
use std::path::{Path, PathBuf};

mod auto_req;
mod config;
mod error;

fn process(
    target_arch: Option<String>,
    target_file: Option<PathBuf>,
    package: Option<String>,
    auto_req_mode: AutoReqMode,
) -> Result<(), Error> {
    let manifest_file_dir = package.map_or(env::current_dir()?, PathBuf::from);
    let manifest_file_path = manifest_file_dir.join("Cargo.toml");
    let config = Config::new(manifest_file_path)?;

    let rpm_pkg = config
        .create_rpm_builder(target_arch, auto_req_mode)?
        .build()?;

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
            create_dir_all(parent_dir)
                .map_err(|err| Error::FileIo(parent_dir.to_path_buf(), err))?;
        }
    }
    let mut f = File::create(&target_file_name)
        .map_err(|err| Error::FileIo(target_file_name.to_path_buf(), err))?;
    rpm_pkg.write(&mut f)?;

    Ok(())
}

fn main() {
    let program = env::args().nth(0).unwrap();

    let mut opts = Options::new();
    opts.optopt("a", "arch", "set target arch", "ARCH");
    opts.optopt("o", "output", "set output file", "OUTPUT.rpm");
    opts.optflag("h", "help", "print this help menu");
    opts.optopt(
        "p",
        "package",
        "set a package name of the workspace",
        "NAME",
    );
    opts.optopt(
        "",
        "auto-req",
        "set automatic dependency processing mode, \
         auto(Default), disabled, builtin, /path/to/find-requires",
        "MODE",
    );
    let opt_matches = opts.parse(env::args().skip(1)).unwrap_or_else(|err| {
        eprintln!("{}: {}", program, err);
        std::process::exit(1);
    });
    if opt_matches.opt_present("h") {
        println!("{}", opts.usage(&*format!("Usage: {} [options]", program)));
    }
    let target_arch = opt_matches.opt_str("a");
    let target_file = opt_matches.opt_str("o").map(|v| PathBuf::from(v));
    let package = opt_matches.opt_str("p");
    let auto_req_mode = AutoReqMode::try_from(
        opt_matches
            .opt_str("auto-req")
            .unwrap_or("auto".to_string()),
    )
    .unwrap_or_else(|err| {
        eprintln!("{}: {}", program, err);
        std::process::exit(1);
    });

    process(target_arch, target_file, package, auto_req_mode).unwrap_or_else(|err| {
        eprintln!("{}: {}", program, err);
        if cfg!(debug_assertions) {
            panic!("{:?}", err);
        }
        std::process::exit(1);
    });
}
