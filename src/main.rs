use crate::{build_target::BuildTarget, config::BuilderConfig};
use cli::Cli;
use std::{
    fs,
    path::{Path, PathBuf},
};

mod auto_req;
mod build_target;
mod cli;
mod config;
mod error;

use config::Config;
use error::Error;

fn determine_output_dir(
    output: Option<&PathBuf>,
    file_name: &str,
    build_target: BuildTarget,
) -> PathBuf {
    match output.as_ref().map(PathBuf::from) {
        Some(path) if path.is_dir() => path.join(file_name),
        Some(path) => path,
        None => build_target.target_path("generate-rpm").join(file_name),
    }
}

fn run() -> Result<(), Error> {
    let (args, matches) = Cli::get_matches_and_try_parse().unwrap_or_else(|e| e.exit());

    let build_target = BuildTarget::new(&args);
    let extra_metadata = args.extra_metadata(&matches);

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

    let target_file_name = determine_output_dir(args.output.as_ref(), &file_name, build_target);

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

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Test the three cases of determining the output file name:
    // 1. Output is a directory
    // 2. Output is a file
    // 3. Output is not specified
    #[test]
    fn test_output_is_dir() {
        let tempdir = tempfile::tempdir().unwrap();
        let pathbufbinding = &tempdir.path().to_path_buf();

        let output = Some(pathbufbinding);
        let file_name = "test.rpm";
        let build_target = BuildTarget::new(&crate::cli::Cli::default());

        let target_file_name = determine_output_dir(output, file_name, build_target);
        assert_eq!(target_file_name, tempdir.path().join("test.rpm"));
    }
    #[test]
    fn test_output_is_file() {
        let tempdir = tempfile::tempdir().unwrap();
        let pathbufbinding = &tempdir.path().to_path_buf();
        let temppath = pathbufbinding.join("foo.rpm");

        let output = Some(&temppath);
        let file_name = "test.rpm";
        let build_target = BuildTarget::new(&crate::cli::Cli::default());

        let target_file_name = determine_output_dir(output, file_name, build_target);
        assert_eq!(target_file_name, temppath);
    }

    #[test]
    fn test_no_output_specified() {
        let output = None;
        let file_name = "test.rpm";
        let build_target = BuildTarget::new(&crate::cli::Cli::default());

        let target_file_name = determine_output_dir(output, file_name, build_target);
        assert_eq!(
            target_file_name,
            PathBuf::from("target/generate-rpm/test.rpm")
        );
    }
}
