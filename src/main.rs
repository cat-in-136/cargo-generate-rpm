use crate::{build_target::BuildTarget, config::BuilderConfig};
use clap::Parser;
use cli::{CargoWrapper, Cli};
use std::{
    fs,
    path::{Path, PathBuf},
};

mod auto_req;
mod build_target;
mod cli;
mod config;
mod error;

use config::{Config, ExtraMetadataSource};
use error::Error;

fn collect_metadata(args: &Cli) -> Vec<config::ExtraMetadataSource> {
    args.metadata_overwrite
        .iter()
        .map(|v| {
            let (file, branch) = match v.split_once('#') {
                None => (PathBuf::from(v), None),
                Some((file, branch)) => (PathBuf::from(file), Some(branch.to_string())),
            };
            ExtraMetadataSource::File(file, branch)
        })
        .chain(
            args.set_metadata
                .iter()
                .map(|v| ExtraMetadataSource::Text(v.to_string())),
        )
        .chain(args.variant.iter().map(|v| {
            let file = match &args.package {
                Some(package) => Config::create_cargo_toml_path(package),
                None => Config::create_cargo_toml_path(""),
            };
            let branch = String::from("package.metadata.generate-rpm.variants.") + v;
            ExtraMetadataSource::File(file, Some(branch))
        }))
        .collect::<Vec<_>>()
}

fn main() -> Result<(), Error> {
    let mut args = std::env::args();
    let args = if let [Some("cargo"), Some("generate-rpm")] =
        [args.next().as_deref(), args.next().as_deref()]
    {
        let CargoWrapper::GenerateRpm(args) = CargoWrapper::parse();
        args
    } else {
        Cli::parse()
    };

    let build_target = BuildTarget::new(&args);
    let extra_metadata = collect_metadata(&args);

    let config = if let Some(p) = &args.package {
        Config::new(Path::new(p), Some(Path::new("")), &extra_metadata)?
    } else {
        Config::new(Path::new(""), None, &extra_metadata)?
    };
    let rpm_pkg = config
        .create_rpm_builder(BuilderConfig::new(&build_target, &args))?
        .build()?;

    let pkg_name = rpm_pkg.metadata.get_name()?;
    let pkg_version = rpm_pkg.metadata.get_version()?;
    let pkg_release = rpm_pkg
        .metadata
        .get_release()
        .map(|v| format!("-{}", v))
        .unwrap_or_default();
    let pkg_arch = rpm_pkg
        .metadata
        .get_arch()
        .map(|v| format!(".{}", v))
        .unwrap_or_default();
    let file_name = format!("{pkg_name}-{pkg_version}{pkg_release}{pkg_arch}.rpm");

    let target_file_name = match args.target.map(PathBuf::from) {
        Some(path) if path.is_dir() => path.join(file_name),
        Some(path) => path,
        None => build_target.target_path("generate-rpm").join(file_name),
    };

    if let Some(parent_dir) = target_file_name.parent() {
        if !parent_dir.exists() {
            fs::create_dir_all(parent_dir)
                .map_err(|err| Error::FileIo(parent_dir.to_path_buf(), err))?;
        }
    }
    let mut f = fs::File::create(&target_file_name)
        .map_err(|err| Error::FileIo(target_file_name.to_path_buf(), err))?;
    rpm_pkg.write(&mut f)?;

    Ok(())
}
