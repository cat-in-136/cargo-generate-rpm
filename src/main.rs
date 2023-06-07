extern crate core;

use crate::auto_req::AutoReqMode;
use crate::build_target::BuildTarget;
use crate::config::{Config, ExtraMetadataSource, PayloadCompressType, RpmBuilderConfig};
use crate::error::Error;
use getopts::Options;
use std::convert::TryFrom;
use std::env;
use std::fs::{create_dir_all, File};
use std::path::{Path, PathBuf};

mod auto_req;
mod build_target;
mod config;
mod error;

#[derive(Debug)]
struct CliSetting {
    auto_req_mode: AutoReqMode,
    payload_compress: PayloadCompressType,
    extra_metadata: Vec<ExtraMetadataSource>,
}

fn process(
    build_target: &BuildTarget,
    target_path: Option<PathBuf>,
    package: Option<String>,
    setting: CliSetting,
) -> Result<(), Error> {
    let config = if let Some(p) = package {
        Config::new(
            Path::new(&p),
            Some(Path::new("")),
            setting.extra_metadata.as_slice(),
        )?
    } else {
        Config::new(Path::new(""), None, setting.extra_metadata.as_slice())?
    };

    let rpm_pkg = config
        .create_rpm_builder(RpmBuilderConfig::new(
            build_target,
            setting.auto_req_mode,
            setting.payload_compress,
        ))?
        .build()?;

    let default_file_name = build_target.target_path("generate-rpm").join(format!(
        "{}-{}{}{}.rpm",
        rpm_pkg.metadata.get_name()?,
        rpm_pkg.metadata.get_version()?,
        rpm_pkg
            .metadata
            .get_release()
            .map(|v| format!("-{}", v))
            .unwrap_or_default(),
        rpm_pkg
            .metadata
            .get_arch()
            .map(|v| format!(".{}", v))
            .unwrap_or_default(),
    ));

    let target_file_name = match target_path {
        Some(path) => {
            if path.is_dir() {
                path.join(default_file_name.file_name().unwrap())
            } else {
                path
            }
        }
        None => default_file_name,
    };

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

fn parse_arg() -> Result<(BuildTarget, Option<PathBuf>, Option<String>, CliSetting), Error> {
    let program = env::args().nth(0).unwrap();
    let mut build_target = BuildTarget::default();

    let mut opts = Options::new();
    opts.optopt("a", "arch", "set target arch", "ARCH");
    opts.optopt("o", "output", "set output file or directory", "OUTPUT");
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
    opts.optopt(
        "",
        "profile",
        "Select which build profile to package. Defaults to \"release\".",
        "PROFILE",
    );
    opts.optopt(
        "",
        "payload-compress",
        "Compression type of package payloads. \
        none, gzip, zstd(Default) or xz.",
        "TYPE",
    );
    opts.optmulti(
        "",
        "metadata-overwrite",
        "Overwrite metadata with TOML file. \
        if \"#dotted.key\" suffixed, load \"dotted.key\" table instead of the root table.",
        "TOML_FILE",
    );
    opts.optmulti(
        "s",
        "set-metadata",
        "Overwrite metadata with TOML text.",
        "TOML",
    );
    opts.optopt(
        "",
        "variant",
        "Shortcut to --metadata-overwrite=path/to/Cargo.toml#package.metadata.generate-rpm.variants.VARIANT",
        "VARIANT",
    );

    opts.optflag("h", "help", "print this help menu");
    opts.optflag("V", "version", "print version information");

    let opt_matches = opts.parse(env::args().skip(1)).unwrap_or_else(|err| {
        eprintln!("{}: {}", program, err);
        std::process::exit(1);
    });
    if opt_matches.opt_present("h") {
        println!("{}", opts.usage(&*format!("Usage: {} [options]", program)));
        std::process::exit(0);
    }
    if opt_matches.opt_present("V") {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

    if let Some(target_arch) = opt_matches.opt_str("a") {
        build_target.arch = Some(target_arch);
    }
    let target_path = opt_matches.opt_str("o").map(PathBuf::from);
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
    if let Some(profile) = opt_matches.opt_str("profile") {
        build_target.profile = Some(profile);
    }
    let payload_compress = opt_matches
        .opt_str("payload-compress")
        .map(|v| v.parse::<PayloadCompressType>())
        .unwrap_or(Ok(PayloadCompressType::default()))?;
    let metadata_overwrite = opt_matches.opt_strs_pos("metadata-overwrite");
    let set_metadata = opt_matches.opt_strs_pos("set-metadata");
    let variant = opt_matches.opt_strs_pos("variant");

    let mut extra_metadata = metadata_overwrite
        .iter()
        .map(|(i, v)| {
            let (file, branch) = match v.split_once("#") {
                None => (PathBuf::from(v), None),
                Some((file, branch)) => (PathBuf::from(file), Some(branch.to_string())),
            };
            (*i, ExtraMetadataSource::File(file, branch))
        })
        .chain(
            set_metadata
                .iter()
                .map(|(i, v)| (*i, ExtraMetadataSource::Text(v.to_string()))),
        )
        .chain(variant.iter().map(|(i, v)| {
            let file = Config::create_cargo_toml_path(package.as_ref().unwrap_or(&"".to_string()));
            let branch = String::from("package.metadata.generate-rpm.variants.") + v;
            (*i, ExtraMetadataSource::File(file, Some(branch)))
        }))
        .collect::<Vec<_>>();
    extra_metadata.sort_by_key(|(i, _)| *i);
    let extra_metadata = extra_metadata.iter().map(|(_, v)| v).cloned().collect();
    Ok((
        build_target,
        target_path,
        package,
        CliSetting {
            auto_req_mode,
            payload_compress,
            extra_metadata,
        },
    ))
}

fn main() {
    (|| -> Result<(), Error> {
        let (build_target, target_file, package, setting) = parse_arg()?;
        process(&build_target, target_file, package, setting)?;
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
