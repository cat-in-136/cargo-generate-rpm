use crate::auto_req::AutoReqMode;
use crate::build_target::BuildTarget;
use crate::config::Config;
use crate::error::Error;
use getopts::Options;
use std::convert::TryFrom;
use std::env;
use std::fs::{create_dir_all, File};
use std::path::PathBuf;

mod auto_req;
mod build_target;
mod config;
mod error;
mod file_info;

fn process(
    build_target: &BuildTarget,
    target_file: Option<PathBuf>,
    package: Option<String>,
    auto_req_mode: AutoReqMode,
) -> Result<(), Error> {
    let manifest_file_dir = package.map_or(env::current_dir()?, PathBuf::from);
    let manifest_file_path = manifest_file_dir.join("Cargo.toml");
    let config = Config::new(manifest_file_path)?;

    let rpm_pkg = config
        .create_rpm_builder(build_target, auto_req_mode)?
        .build()?;

    let default_file_name = build_target.target_path("generate-rpm").join(format!(
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

fn parse_arg() -> Result<(BuildTarget, Option<PathBuf>, Option<String>, AutoReqMode), Error> {
    let program = env::args().nth(0).unwrap();
    let mut build_target = BuildTarget::default();

    let mut opts = Options::new();
    opts.optopt("a", "arch", "set target arch", "ARCH");
    opts.optopt("o", "output", "set output file", "OUTPUT.rpm");
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
         auto(Default), no, builtin, /path/to/find-requires",
        "MODE",
    );
    opts.optopt(
        "",
        "target",
        "Sub-directory name for all generated artifacts. \
    May be specified with CARGO_BUILD_TARGET environment variable.",
        "TARGET-TRIPLE",
    );
    opts.optopt(
        "",
        "target-dir",
        "Directory for all generated artifacts. \
    May be specified with CARGO_BUILD_TARGET_DIR or CARGO_TARGET_DIR environment variables.",
        "DIRECTORY",
    );

    opts.optflag("h", "help", "print this help menu");

    let opt_matches = opts.parse(env::args().skip(1)).unwrap_or_else(|err| {
        eprintln!("{}: {}", program, err);
        std::process::exit(1);
    });
    if opt_matches.opt_present("h") {
        println!("{}", opts.usage(&*format!("Usage: {} [options]", program)));
        std::process::exit(0);
    }

    if let Some(target_arch) = opt_matches.opt_str("a") {
        build_target.arch = Some(target_arch);
    }
    let target_file = opt_matches.opt_str("o").map(PathBuf::from);
    let package = opt_matches.opt_str("p");
    let auto_req_mode = AutoReqMode::try_from(
        opt_matches
            .opt_str("auto-req")
            .unwrap_or("auto".to_string()),
    )?;
    if let Some(target) = opt_matches.opt_str("target") {
        build_target.target = Some(target);
    }
    if let Some(target_dir) = opt_matches.opt_str("target-dir") {
        build_target.target_dir = Some(target_dir);
    }

    Ok((build_target, target_file, package, auto_req_mode))
}

fn main() {
    (|| -> Result<(), Error> {
        let (build_target, target_file, package, auto_req_mode) = parse_arg()?;
        process(&build_target, target_file, package, auto_req_mode)?;
        Ok(())
    })()
    .unwrap_or_else(|err| {
        let program = env::args().nth(0).unwrap();
        eprintln!("{}: {}", program, err);
        if cfg!(debug_assertions) {
            panic!("{:?}", err);
        }
        std::process::exit(1);
    });
}
