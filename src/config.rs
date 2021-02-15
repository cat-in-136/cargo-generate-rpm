use crate::auto_req::{find_requires, AutoReqMode};
use crate::error::{ConfigError, Error};
use cargo_toml::Error as CargoTomlError;
use cargo_toml::Manifest;
use rpm::{Compressor, Dependency, RPMBuilder, RPMFileOptions};
use std::env::consts::ARCH;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use toml::value::Table;

#[derive(Debug)]
pub struct Config {
    manifest: Manifest,
    path: PathBuf,
}

impl Config {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref().to_path_buf();
        Manifest::from_path(&path)
            .map(|manifest| Config {
                manifest,
                path: path.clone(),
            })
            .map_err(|err| match err {
                CargoTomlError::Io(e) => Error::FileIo(path, e),
                _ => Error::CargoToml(err),
            })
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
            .find(|(name, _)| name.as_str() == "generate-rpm")
            .ok_or(ConfigError::Missing("package.metadata.generate-rpm"))?
            .1
            .as_table()
            .ok_or(ConfigError::WrongType(
                "package.metadata.generate-rpm",
                "table",
            ))?;
        Ok(metadata)
    }

    fn files(&self) -> Result<Vec<FileInfo>, ConfigError> {
        let metadata = self.metadata()?;
        let assets = metadata
            .get("assets")
            .ok_or(ConfigError::Missing("package.metadata.generate-rpm.assets"))?
            .as_array()
            .ok_or(ConfigError::WrongType(
                "package.metadata.generate-rpm.assets",
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
                let mode = usize::from_str_radix(mode, 8)
                    .map_err(|_| ConfigError::AssetFileWrongType(idx, "mode", "oct-string"))?;
                let file_mode = if mode & 0o170000 != 0 {
                    None
                } else if source.ends_with('/') {
                    Some(0o040000) // S_IFDIR
                } else {
                    Some(0o100000) // S_IFREG
                };
                Some(file_mode.unwrap_or_default() | mode)
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

    fn table_to_dependencies(table: &Table) -> Result<Vec<Dependency>, ConfigError> {
        let mut dependencies = Vec::with_capacity(table.len());
        for (key, value) in table {
            let ver = value
                .as_str()
                .ok_or(ConfigError::WrongDependencyVersion(key.clone()))?
                .trim();
            let ver_vec = ver.trim().split_whitespace().collect::<Vec<_>>();
            let dependency = match ver_vec.as_slice() {
                [] | ["*"] => Ok(Dependency::any(key)),
                ["<", ver] => Ok(Dependency::less(key.as_str(), ver.trim())),
                ["<=", ver] => Ok(Dependency::less_eq(key.as_str(), ver.trim())),
                ["=", ver] | [ver] => Ok(Dependency::eq(key.as_str(), ver.trim())),
                [">", ver] => Ok(Dependency::greater(key.as_str(), ver.trim())),
                [">=", ver] => Ok(Dependency::greater_eq(key.as_str(), ver.trim())),
                _ => Err(ConfigError::WrongDependencyVersion(key.clone())),
            }?;
            dependencies.push(dependency);
        }
        Ok(dependencies)
    }

    pub fn create_rpm_builder(
        &self,
        target_arch: Option<String>,
        auto_req_mode: AutoReqMode,
    ) -> Result<RPMBuilder, Error> {
        let metadata = self.metadata()?;
        macro_rules! get_str_from_metadata {
            ($name:expr) => {
                if let Some(val) = metadata.get($name) {
                    Some(val.as_str()
                        .ok_or(ConfigError::WrongType(
                            concat!("package.metadata.generate-rpm.", $name),
                            "string"
                        ))?)
                } else {
                    None
                } as Option<&str>
            }
        };
        macro_rules! get_i64_from_metadata {
            ($name:expr) => {
                if let Some(val) = metadata.get($name) {
                    Some(val.as_integer()
                        .ok_or(ConfigError::WrongType(
                            concat!("package.metadata.generate-rpm.", $name),
                            "integer"
                        ))?)
                } else {
                    None
                } as Option<i64>
            }
        };
        macro_rules! get_table_from_metadata {
            ($name:expr) => {
                if let Some(val) = metadata.get($name) {
                    Some(val.as_table()
                        .ok_or(ConfigError::WrongType(
                            concat!("package.metadata.generate-rpm.", $name),
                            "table"
                        ))?)
                } else {
                    None
                } as Option<&Table>
            }
        };

        let pkg = self
            .manifest
            .package
            .as_ref()
            .ok_or(ConfigError::Missing("package"))?;
        let name = get_str_from_metadata!("name").unwrap_or(pkg.name.as_str());
        let version = get_str_from_metadata!("version").unwrap_or(pkg.version.as_str());
        let license = get_str_from_metadata!("license").unwrap_or(
            pkg.license
                .as_ref()
                .ok_or(ConfigError::Missing("package.license"))?
                .as_str(),
        );
        let arch = target_arch.unwrap_or(
            match ARCH {
                "x86" => "i586",
                "arm" => "armhfp",
                "powerpc" => "ppc",
                "powerpc64" => "ppc64",
                _ => ARCH,
            }
            .to_string(),
        );
        let desc = get_str_from_metadata!("summary").unwrap_or(
            pkg.description
                .as_ref()
                .ok_or(ConfigError::Missing("package.description"))?
                .as_str(),
        );
        let files = self.files()?;

        let mut builder = RPMBuilder::new(name, version, license, arch.as_str(), desc)
            .compression(Compressor::from_str("gzip").unwrap());
        for file in &files {
            let options = file.generate_rpm_file_options();

            let file_source = [
                PathBuf::from(file.source),
                self.path.parent().unwrap().join(file.source),
            ]
            .iter()
            .find(|v| v.exists())
            .ok_or(ConfigError::AssetFileNotFound(file.source.to_string()))?
            .to_owned();

            builder = builder.with_file(file_source, options)?;
        }

        if let Some(release) = get_i64_from_metadata!("release") {
            builder = builder.release(release as u16);
        }
        if let Some(epoch) = get_i64_from_metadata!("epoch") {
            builder = builder.epoch(epoch as i32);
        }

        if let Some(pre_install_script) = get_str_from_metadata!("pre_install_script") {
            builder = builder.pre_install_script(pre_install_script);
        }
        if let Some(pre_uninstall_script) = get_str_from_metadata!("pre_uninstall_script") {
            builder = builder.pre_uninstall_script(pre_uninstall_script);
        }
        if let Some(post_install_script) = get_str_from_metadata!("post_install_script") {
            builder = builder.post_install_script(post_install_script);
        }
        if let Some(post_uninstall_script) = get_str_from_metadata!("post_uninstall_script") {
            builder = builder.post_uninstall_script(post_uninstall_script);
        }

        if let Some(requires) = get_table_from_metadata!("requires") {
            for dependency in Self::table_to_dependencies(requires)? {
                builder = builder.requires(dependency);
            }
        }
        for requires in find_requires(files.iter().map(|v| Path::new(v.source)), auto_req_mode)? {
            builder = builder.requires(Dependency::any(requires));
        }
        if let Some(obsoletes) = get_table_from_metadata!("obsoletes") {
            for dependency in Self::table_to_dependencies(obsoletes)? {
                builder = builder.obsoletes(dependency);
            }
        }
        if let Some(conflicts) = get_table_from_metadata!("conflicts") {
            for dependency in Self::table_to_dependencies(conflicts)? {
                builder = builder.conflicts(dependency);
            }
        }
        if let Some(provides) = get_table_from_metadata!("provides") {
            for dependency in Self::table_to_dependencies(provides)? {
                builder = builder.provides(dependency);
            }
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
    use cargo_toml::Value;

    #[test]
    fn test_config_new() {
        let config = Config::new("Cargo.toml").unwrap();
        let pkg = config.manifest.package.unwrap();
        assert_eq!(pkg.name, "cargo-generate-rpm");

        assert!(matches!(Config::new("not_exist_path/Cargo.toml"),
            Err(Error::FileIo(path, error)) if path == PathBuf::from("not_exist_path/Cargo.toml") && error.kind() == std::io::ErrorKind::NotFound));
        assert!(matches!(
            Config::new("src/error.rs"),
            Err(Error::CargoToml(_))
        ));
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
                    source: "target/release/cargo-generate-rpm",
                    dest: "/usr/bin/cargo-generate-rpm",
                    user: None,
                    group: None,
                    mode: Some(0o0100755),
                    config: false,
                    doc: false
                },
                FileInfo {
                    source: "LICENSE",
                    dest: "/usr/share/doc/cargo-generate-rpm/LICENSE",
                    user: None,
                    group: None,
                    mode: Some(0o0100644),
                    config: false,
                    doc: true
                },
                FileInfo {
                    source: "README.md",
                    dest: "/usr/share/doc/cargo-generate-rpm/README.md",
                    user: None,
                    group: None,
                    mode: Some(0o0100644),
                    config: false,
                    doc: true
                }
            ]
        );
    }

    #[test]
    fn test_table_to_dependencies() {
        let mut table = Table::new();
        [
            ("any1", ""),
            ("any2", "*"),
            ("less", "< 1.0"),
            ("lesseq", "<= 1.0"),
            ("eq", "= 1.0"),
            ("greater", "> 1.0"),
            ("greatereq", "<= 1.0"),
        ]
        .iter()
        .for_each(|(k, v)| {
            table.insert(k.to_string(), Value::String(v.to_string()));
        });
        assert_eq!(Config::table_to_dependencies(&table).unwrap().len(), 7);

        // table.clear();
        table.insert("error".to_string(), Value::Integer(1));
        assert!(matches!(
            Config::table_to_dependencies(&table),
            Err(ConfigError::WrongDependencyVersion(_))
        ));

        table.clear();
        table.insert("error".to_string(), Value::String("!= 1".to_string()));
        assert!(matches!(
            Config::table_to_dependencies(&table),
            Err(ConfigError::WrongDependencyVersion(_))
        ));

        table.clear();
        table.insert("error".to_string(), Value::String("> 1 1".to_string()));
        assert!(matches!(
            Config::table_to_dependencies(&table),
            Err(ConfigError::WrongDependencyVersion(_))
        ));
    }

    #[test]
    fn test_config_create_rpm_builder() {
        let config = Config::new("Cargo.toml").unwrap();
        let builder = config.create_rpm_builder(None, AutoReqMode::Disabled);

        assert!(if Path::new("target/release/cargo-generate-rpm").exists() {
            matches!(builder, Ok(_))
        } else {
            matches!(builder, Err(Error::Config(ConfigError::AssetFileNotFound(path))) if path == "target/release/cargo-generate-rpm")
        });
    }
}
