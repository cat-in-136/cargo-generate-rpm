use crate::error::{ConfigError, Error};
use cargo_toml::Manifest;
use rpm::{Compressor, RPMBuilder, RPMFileOptions};
use std::path::Path;
use std::str::FromStr;
use toml::value::Table;

#[derive(Debug)]
pub struct Config {
    manifest: Manifest,
}

impl Config {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, Error> {
        let manifest = Manifest::from_path(path)?;
        Ok(Self { manifest })
    }

    fn metadata(&self) -> Result<&Table, ConfigError> {
        let pkg = self
            .manifest
            .package
            .as_ref()
            .ok_or(ConfigError::Missing("package"))?;
        let metadata = pkg
            .metadata
            .as_ref()
            .ok_or(ConfigError::Missing("package.metadata"))?
            .as_table()
            .ok_or(ConfigError::WrongType("package.metadata", "table"))?;
        let metadata = metadata
            .iter()
            .find(|(name, _)| name.as_str() == "binary-rpm")
            .ok_or(ConfigError::Missing("package.metadata.binary-rpm"))?
            .1
            .as_table()
            .ok_or(ConfigError::WrongType(
                "package.metadata.binary-rpm",
                "table",
            ))?;
        Ok(metadata)
    }

    fn files(&self) -> Result<Vec<FileInfo>, ConfigError> {
        let metadata = self.metadata()?;
        let assets = metadata
            .get("assets")
            .ok_or(ConfigError::Missing("package.metadata.binary-rpm.assets"))?
            .as_array()
            .ok_or(ConfigError::WrongType(
                "package.metadata.binary-rpm.assets",
                "array",
            ))?;

        let mut files = Vec::with_capacity(assets.len());
        for (idx, value) in assets.iter().enumerate() {
            let table = value
                .as_table()
                .ok_or(ConfigError::AssetFileUndefined(idx, "source"))?;
            let source = table
                .get("source")
                .ok_or(ConfigError::AssetFileUndefined(idx, "source"))?
                .as_str()
                .ok_or(ConfigError::AssetFileWrongType(idx, "source", "string"))?;
            let dest = table
                .get("dest")
                .ok_or(ConfigError::AssetFileUndefined(idx, "dest"))?
                .as_str()
                .ok_or(ConfigError::AssetFileWrongType(idx, "dest", "string"))?;

            let user = if let Some(user) = table.get("user") {
                Some(
                    user.as_str()
                        .ok_or(ConfigError::AssetFileWrongType(idx, "user", "string"))?,
                )
            } else {
                None
            };
            let group = if let Some(group) = table.get("group") {
                Some(
                    group
                        .as_str()
                        .ok_or(ConfigError::AssetFileWrongType(idx, "group", "string"))?,
                )
            } else {
                None
            };
            let mode = if let Some(mode) = table.get("mode") {
                let mode = mode
                    .as_str()
                    .ok_or(ConfigError::AssetFileWrongType(idx, "mode", "string"))?;
                Some(
                    usize::from_str_radix(mode, 8)
                        .map_err(|_| ConfigError::AssetFileWrongType(idx, "mode", "oct-string"))?,
                )
            } else {
                None
            };
            let config = if let Some(is_config) = table.get("config") {
                is_config
                    .as_bool()
                    .ok_or(ConfigError::AssetFileWrongType(idx, "config", "bool"))?
            } else {
                false
            };
            let doc = if let Some(is_doc) = table.get("doc") {
                is_doc
                    .as_bool()
                    .ok_or(ConfigError::AssetFileWrongType(idx, "doc", "bool"))?
            } else {
                false
            };

            files.push(FileInfo {
                source,
                dest,
                user,
                group,
                mode,
                config,
                doc,
            });
        }
        Ok(files)
    }

    pub fn create_rpm_builder(&self, target_arch: &str) -> Result<RPMBuilder, Error> {
        let pkg = self
            .manifest
            .package
            .as_ref()
            .ok_or(ConfigError::Missing("package"))?;
        let name = pkg.name.as_str();
        let version = pkg.version.as_str();
        let license = pkg
            .license
            .as_ref()
            .ok_or(ConfigError::Missing("package.version"))?
            .as_str();
        let desc = pkg
            .description
            .as_ref()
            .ok_or(ConfigError::Missing("package.description"))?
            .as_str();

        let mut builder = RPMBuilder::new(name, version, license, target_arch, desc)
            .compression(Compressor::from_str("gzip").unwrap());
        for file in &self.files()? {
            let options = file.generate_rpm_file_options();
            builder = builder.with_file(file.source, options)?;
        }

        Ok(builder)
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct FileInfo<'a, 'b, 'c, 'd> {
    source: &'a str,
    dest: &'b str,
    user: Option<&'c str>,
    group: Option<&'d str>,
    mode: Option<usize>,
    config: bool,
    doc: bool,
}

impl FileInfo<'_, '_, '_, '_> {
    fn generate_rpm_file_options(&self) -> RPMFileOptions {
        let mut rpm_file_option = RPMFileOptions::new(self.dest);
        if let Some(user) = self.user {
            rpm_file_option = rpm_file_option.user(user);
        }
        if let Some(group) = self.group {
            rpm_file_option = rpm_file_option.group(group);
        }
        if let Some(mode) = self.mode {
            rpm_file_option = rpm_file_option.mode(mode as i32);
        }
        if self.config {
            rpm_file_option = rpm_file_option.is_config();
        }
        if self.doc {
            rpm_file_option = rpm_file_option.is_doc();
        }
        rpm_file_option.into()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_config_new() {
        let config = Config::new("Cargo.toml").unwrap();
        let pkg = config.manifest.package.unwrap();
        assert_eq!(pkg.name, "cargo-binary-rpm");
    }

    #[test]
    fn test_metadata() {
        let config = Config::new("Cargo.toml").unwrap();
        let metadata = config.metadata().unwrap();
        let assets = metadata.get("assets").unwrap();
        assert!(assets.is_array());
    }

    #[test]
    fn test_files() {
        let config = Config::new("Cargo.toml").unwrap();
        let files = config.files().unwrap();
        assert_eq!(
            files,
            vec![
                FileInfo {
                    source: "target/release/cargo-binary-rpm",
                    dest: "/usr/bin/cargo-binary-rpm",
                    user: None,
                    group: None,
                    mode: None,
                    config: false,
                    doc: false
                },
                FileInfo {
                    source: "LICENSE",
                    dest: "/usr/share/doc/cargo-binary-rpm/LICENSE",
                    user: None,
                    group: None,
                    mode: None,
                    config: false,
                    doc: true
                },
                FileInfo {
                    source: "README.md",
                    dest: "/usr/share/doc/cargo-binary-rpm/README.md",
                    user: None,
                    group: None,
                    mode: None,
                    config: false,
                    doc: true
                }
            ]
        );
    }

    // #[test]
    // fn test_config_create_rpm_builder() {
    //     let config = Config::new("Cargo.toml").unwrap();
    //     let builder = config.create_rpm_builder("x86_64").unwrap();
    // }
}
