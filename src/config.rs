use std::path::{Path, PathBuf};
use std::str::FromStr;

use cargo_toml::Error as CargoTomlError;
use cargo_toml::Manifest;
use rpm::{Compressor, Dependency, RPMBuilder};
use toml::value::Table;

use crate::auto_req::{find_requires, AutoReqMode};
use crate::build_target::BuildTarget;
use crate::error::{ConfigError, Error};
use crate::file_info::FileInfo;

#[derive(Debug)]
pub struct RpmBuilderConfig<'a, 'b> {
    build_target: &'a BuildTarget,
    auto_req_mode: AutoReqMode,
    payload_compress: &'b str,
}

impl<'a, 'b> RpmBuilderConfig<'a, 'b> {
    pub fn new(
        build_target: &'a BuildTarget,
        auto_req_mode: AutoReqMode,
        payload_compress: &'b str,
    ) -> RpmBuilderConfig<'a, 'b> {
        RpmBuilderConfig {
            build_target,
            auto_req_mode,
            payload_compress,
        }
    }
}

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

    pub(crate) fn metadata(&self) -> Result<&Table, ConfigError> {
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
                ["=", ver] => Ok(Dependency::eq(key.as_str(), ver.trim())),
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
        rpm_builder_config: RpmBuilderConfig,
    ) -> Result<RPMBuilder, Error> {
        let metadata = self.metadata()?;

        macro_rules! get_from_metadata {
            ($name:literal, {$($pattern:pat => $conv:expr),*}, $type_name:literal) => {
                if let Some(val) = metadata.get($name) {
                    use toml::value::Value::*;
                    match val {
                        $($pattern => $conv,)*
                        _ => {
                            Err(
                                ConfigError::WrongType(
                                    concat!("package.metadata.generate-rpm.", $name),
                                    $type_name
                                )
                            )
                        }
                    }?
                } else {
                    None
                }
            }
        }

        macro_rules! get_str_from_metadata {
            ($name:expr) => {
                get_from_metadata!($name, {
                    String(val) => Ok(Some(val.as_str()))
                }, "string") as Option<&str>
            }
        }
        macro_rules! get_i64_from_metadata {
            ($name:expr) => {
                get_from_metadata!($name, {
                    Integer(val) => Ok(Some(*val))
                }, "integer") as Option<i64>
            }
        }
        macro_rules! get_str_or_i64_from_metadata {
            ($name:expr) => {
                get_from_metadata!($name, {
                    Integer(val) => Ok(Some(val.to_string())),
                    String(val) => Ok(Some(val.clone()))
                }, "string or integer") as Option<String>
            }
        }
        macro_rules! get_table_from_metadata {
            ($name:expr) => {
                get_from_metadata!($name, {
                    Table(val) => Ok(Some(val))
                }, "table") as Option<&Table>
            }
        }

        let pkg = self
            .manifest
            .package
            .as_ref()
            .ok_or(ConfigError::Missing("package"))?;
        let name = get_str_from_metadata!("name").unwrap_or_else(|| pkg.name.as_str());
        let version = get_str_from_metadata!("version").unwrap_or_else(|| pkg.version.as_str());
        let license = get_str_from_metadata!("license")
            .or_else(|| pkg.license.as_ref().map(|v| v.as_ref()))
            .ok_or(ConfigError::Missing("package.license"))?;
        let arch = rpm_builder_config.build_target.binary_arch();
        let desc = get_str_from_metadata!("summary")
            .or_else(|| pkg.description.as_ref().map(|v| v.as_ref()))
            .ok_or(ConfigError::Missing("package.description"))?;
        let files = FileInfo::list_from_metadata(&metadata)?;
        let parent = self.path.parent().unwrap();

        let mut builder = RPMBuilder::new(name, version, license, arch.as_str(), desc)
            .compression(Compressor::from_str(rpm_builder_config.payload_compress)?);
        for file in &files {
            let file_source =
                file.generate_rpm_file_path(rpm_builder_config.build_target, parent)?;
            let options = file.generate_rpm_file_options();
            builder = builder.with_file(file_source, options)?;
        }

        if let Some(release) = get_str_or_i64_from_metadata!("release") {
            builder = builder.release(release);
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
        let auto_req = if rpm_builder_config.auto_req_mode == AutoReqMode::Auto
            && matches!(
                get_str_from_metadata!("auto-req"),
                Some("no") | Some("disabled")
            ) {
            AutoReqMode::Disabled
        } else {
            rpm_builder_config.auto_req_mode
        };
        for requires in find_requires(files.iter().map(|v| Path::new(v.source)), auto_req)? {
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

#[cfg(test)]
mod test {
    use cargo_toml::Value;

    use super::*;

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
    fn test_table_to_dependencies() {
        fn dependency_to_u8_slice(dep: &Dependency) -> &[u8] {
            unsafe { std::mem::transmute_copy(dep) }
        }

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

        assert_eq!(
            Config::table_to_dependencies(&table)
                .unwrap()
                .iter()
                .map(&dependency_to_u8_slice)
                .collect::<Vec<_>>(),
            vec![
                dependency_to_u8_slice(&Dependency::any("any1")),
                dependency_to_u8_slice(&Dependency::any("any2")),
                dependency_to_u8_slice(&Dependency::eq("eq", "1.0")),
                dependency_to_u8_slice(&Dependency::greater("greater", "1.0")),
                dependency_to_u8_slice(&Dependency::greater_eq("greatereq", "1.0")),
                dependency_to_u8_slice(&Dependency::less("less", "1.0")),
                dependency_to_u8_slice(&Dependency::less_eq("lesseq", "1.0")),
            ]
        );

        // table.clear();
        table.insert("error".to_string(), Value::Integer(1));
        assert!(matches!(
            Config::table_to_dependencies(&table),
            Err(ConfigError::WrongDependencyVersion(_))
        ));

        table.clear();
        table.insert("error".to_string(), Value::String("1".to_string()));
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
        let builder = config.create_rpm_builder(&BuildTarget::default(), AutoReqMode::Disabled);

        assert!(if Path::new("target/release/cargo-generate-rpm").exists() {
            matches!(builder, Ok(_))
        } else {
            matches!(builder, Err(Error::Config(ConfigError::AssetFileNotFound(path))) if path == "target/release/cargo-generate-rpm")
        });
    }
}
