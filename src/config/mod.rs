use std::path::{Path, PathBuf};

use cargo_toml::Error as CargoTomlError;
use cargo_toml::Manifest;
use rpm::Dependency;
use toml::value::Table;

use crate::auto_req::{find_requires, AutoReqMode};
use crate::build_target::BuildTarget;
use crate::cli::Cli;
use crate::error::{ConfigError, Error};
use file_info::FileInfo;
use metadata::{CompoundMetadataConfig, ExtraMetaData, MetadataConfig, TomlValueHelper};

mod file_info;
mod metadata;

#[derive(Debug, Clone)]
pub enum ExtraMetadataSource {
    File(PathBuf, Option<String>),
    Text(String),
}

#[derive(Debug)]
pub struct BuilderConfig<'a> {
    build_target: &'a BuildTarget,
    args: &'a Cli,
}

impl<'a> BuilderConfig<'a> {
    pub fn new(build_target: &'a BuildTarget, args: &'a Cli) -> BuilderConfig<'a> {
        BuilderConfig { build_target, args }
    }
}

#[derive(Debug)]
pub struct Config {
    manifest: Manifest,
    manifest_path: PathBuf,
    extra_metadata: Vec<ExtraMetaData>,
}

impl Config {
    pub fn new(
        project_base_path: &Path,
        workspace_base_path: Option<&Path>,
        extra_metadata: &[ExtraMetadataSource],
    ) -> Result<Self, Error> {
        let manifest_path = Self::create_cargo_toml_path(project_base_path);

        let manifest = if let Some(p) = workspace_base_path {
            // HACK when workspace used, manifest is generated from slice directly instead of
            // `from_path_with_metadata`. Because it call `inherit_workspace`, which yields an error
            // in case `edition.workspace = true` specified in the project manifest file.
            // TODO future fix when https://gitlab.com/crates.rs/cargo_toml/-/issues/20 fixed

            let cargo_toml_content = std::fs::read(&manifest_path)
                .map_err(|e| Error::FileIo(manifest_path.clone(), e))?;
            let mut manifest = Manifest::from_slice_with_metadata(&cargo_toml_content)?;

            let workspace_manifest_path = Self::create_cargo_toml_path(p);
            let workspace_manifest =
                Manifest::from_path(&workspace_manifest_path).map_err(|err| match err {
                    CargoTomlError::Io(e) => {
                        Error::FileIo(workspace_manifest_path.to_path_buf(), e)
                    }
                    _ => Error::CargoToml(err),
                })?;
            manifest.complete_from_path_and_workspace(
                manifest_path.as_path(),
                Some((&workspace_manifest, p)),
            )?;
            manifest
        } else {
            Manifest::from_path(&manifest_path).map_err(|err| match err {
                CargoTomlError::Io(e) => Error::FileIo(manifest_path.to_path_buf(), e),
                _ => Error::CargoToml(err),
            })?
        };

        let extra_metadata = extra_metadata
            .iter()
            .map(ExtraMetaData::new)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Config {
            manifest,
            manifest_path,
            extra_metadata,
        })
    }

    pub(crate) fn create_cargo_toml_path<P: AsRef<Path>>(base_path: P) -> PathBuf {
        base_path.as_ref().join("Cargo.toml")
    }

    fn table_to_dependencies(table: &Table) -> Result<Vec<Dependency>, ConfigError> {
        let mut dependencies = Vec::with_capacity(table.len());
        for (key, value) in table {
            let ver = value
                .as_str()
                .ok_or(ConfigError::WrongDependencyVersion(key.clone()))?
                .trim();
            let ver_vec = ver.split_whitespace().collect::<Vec<_>>();
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

    pub fn create_rpm_builder(&self, cfg: BuilderConfig) -> Result<rpm::PackageBuilder, Error> {
        let mut metadata_config = Vec::new();
        metadata_config.push(MetadataConfig::new_from_manifest(&self.manifest)?);
        for v in &self.extra_metadata {
            metadata_config.push(MetadataConfig::new_from_extra_metadata(v));
        }
        let metadata = CompoundMetadataConfig::new(metadata_config.as_slice());

        let pkg = self
            .manifest
            .package
            .as_ref()
            .ok_or(ConfigError::Missing("package".to_string()))?;
        let name = metadata.get_str("name")?.unwrap_or(pkg.name.as_str());
        let version = match metadata.get_str("version")? {
            Some(v) => v,
            None => pkg.version.get()?,
        };
        let license = match (metadata.get_str("license")?, pkg.license.as_ref()) {
            (Some(v), _) => v,
            (None, None) => Err(ConfigError::Missing("package.license".to_string()))?,
            (None, Some(v)) => v.get()?,
        };
        let arch = cfg.build_target.binary_arch();
        let desc = match (
            metadata.get_str("summary")?,
            metadata.get_str("description")?,
            pkg.description.as_ref(),
        ) {
            (Some(v), _, _) => v,
            (None, Some(v), _) => v,
            (None, None, Some(v)) => v.get()?,
            (None, None, None) => Err(ConfigError::Missing("package.description".to_string()))?,
        };
        let assets = metadata
            .get_array("assets")?
            .ok_or(ConfigError::Missing("package.assets".to_string()))?;
        let files = FileInfo::new(assets)?;
        let parent = self.manifest_path.parent().unwrap();

        let mut builder = rpm::PackageBuilder::new(name, version, license, arch.as_str(), desc)
            .compression(cfg.args.payload_compress);
        builder = if let Some(t) = cfg.args.source_date {
            builder.source_date(t)
        } else if let Ok(t) = std::env::var("SOURCE_DATE_EPOCH") {
            let t = t
                .parse::<u32>()
                .map_err(|err| Error::EnvError("SOURCE_DATE_EPOCH", err.to_string()))?;
            builder.source_date(t)
        } else {
            builder
        };

        let mut expanded_file_paths = vec![];
        for (idx, file) in files.iter().enumerate() {
            let entries = file.generate_rpm_file_entry(cfg.build_target, parent, idx)?;
            for (file_source, options) in entries {
                expanded_file_paths.push(file_source.clone());
                builder = builder.with_file(file_source, options)?;
            }
        }

        if let Some(release) = metadata.get_string_or_i64("release")? {
            builder = builder.release(release);
        }
        if let Some(epoch) = metadata.get_i64("epoch")? {
            builder = builder.epoch(epoch as u32);
        }

        if let Some(pre_install_script) = metadata.get_str("pre_install_script")? {
            let scriptlet = metadata.get_scriptlet(
                "pre_install_script",
                load_script_if_path(pre_install_script, parent, cfg.build_target)?,
            )?;

            if let Some(scriptlet) = scriptlet {
                builder = builder.pre_install_script(scriptlet);
            }
        }

        if let Some(pre_uninstall_script) = metadata.get_str("pre_uninstall_script")? {
            let scriptlet = metadata.get_scriptlet(
                "pre_uninstall_script",
                load_script_if_path(pre_uninstall_script, parent, cfg.build_target)?,
            )?;

            if let Some(scriptlet) = scriptlet {
                builder = builder.pre_uninstall_script(scriptlet);
            }
        }

        if let Some(post_install_script) = metadata.get_str("post_install_script")? {
            let scriptlet = metadata.get_scriptlet(
                "post_install_script",
                load_script_if_path(post_install_script, parent, cfg.build_target)?,
            )?;

            if let Some(scriptlet) = scriptlet {
                builder = builder.post_install_script(scriptlet);
            }
        }

        if let Some(post_uninstall_script) = metadata.get_str("post_uninstall_script")? {
            let scriptlet = metadata.get_scriptlet(
                "post_uninstall_script",
                load_script_if_path(post_uninstall_script, parent, cfg.build_target)?,
            )?;

            if let Some(scriptlet) = scriptlet {
                builder = builder.post_uninstall_script(scriptlet);
            }
        }

        if let Some(pre_trans_script) = metadata.get_str("pre_trans_script")? {
            let scriptlet = metadata.get_scriptlet(
                "pre_trans_script",
                load_script_if_path(pre_trans_script, parent, cfg.build_target)?,
            )?;

            if let Some(scriptlet) = scriptlet {
                builder = builder.pre_trans_script(scriptlet);
            }
        }

        if let Some(post_trans_script) = metadata.get_str("post_trans_script")? {
            let scriptlet = metadata.get_scriptlet(
                "post_trans_script",
                load_script_if_path(post_trans_script, parent, cfg.build_target)?,
            )?;

            if let Some(scriptlet) = scriptlet {
                builder = builder.post_trans_script(scriptlet);
            }
        }

        if let Some(pre_untrans_script) = metadata.get_str("pre_untrans_script")? {
            let scriptlet = metadata.get_scriptlet(
                "pre_untrans_script",
                load_script_if_path(pre_untrans_script, parent, cfg.build_target)?,
            )?;

            if let Some(scriptlet) = scriptlet {
                builder = builder.pre_untrans_script(scriptlet);
            }
        }

        if let Some(post_untrans_script) = metadata.get_str("post_untrans_script")? {
            let scriptlet = metadata.get_scriptlet(
                "post_untrans_script",
                load_script_if_path(post_untrans_script, parent, cfg.build_target)?,
            )?;

            if let Some(scriptlet) = scriptlet {
                builder = builder.post_untrans_script(scriptlet);
            }
        }

        if let Some(url) = match (
            metadata.get_str("url")?,
            pkg.homepage.as_ref(),
            pkg.repository.as_ref(),
        ) {
            (Some(v), _, _) => Some(v),
            (None, Some(v), _) => Some(v.get()?.as_str()),
            (None, None, Some(v)) => Some(v.get()?.as_str()),
            (None, None, None) => None,
        } {
            builder = builder.url(url);
        }

        if let Some(vendor) = metadata.get_str("vendor")? {
            builder = builder.vendor(vendor);
        }

        if metadata.get_bool("require-sh")?.unwrap_or(true) {
            builder = builder.requires(Dependency::any("/bin/sh".to_string()));
        }

        if let Some(requires) = metadata.get_table("requires")? {
            for dependency in Self::table_to_dependencies(requires)? {
                builder = builder.requires(dependency);
            }
        }

        let meta_aut_req = metadata.get_str("auto-req")?;
        let auto_req = match (&cfg.args.auto_req, meta_aut_req) {
            (None, Some("no" | "disabled")) => AutoReqMode::Disabled,
            (None, _) => AutoReqMode::Auto,
            (Some(v), _) => AutoReqMode::from(v.clone()),
        };

        for requires in find_requires(expanded_file_paths, auto_req)? {
            builder = builder.requires(Dependency::any(requires));
        }
        if let Some(obsoletes) = metadata.get_table("obsoletes")? {
            for dependency in Self::table_to_dependencies(obsoletes)? {
                builder = builder.obsoletes(dependency);
            }
        }
        if let Some(conflicts) = metadata.get_table("conflicts")? {
            for dependency in Self::table_to_dependencies(conflicts)? {
                builder = builder.conflicts(dependency);
            }
        }
        if let Some(provides) = metadata.get_table("provides")? {
            for dependency in Self::table_to_dependencies(provides)? {
                builder = builder.provides(dependency);
            }
        }

        Ok(builder)
    }
}

pub(crate) fn load_script_if_path<P: AsRef<Path>>(
    asset: &str,
    parent: P,
    build_target: &BuildTarget,
) -> std::io::Result<String> {
    let relpath = file_info::get_asset_rel_path(asset, build_target);

    if Path::new(&relpath).exists() {
        return std::fs::read_to_string(relpath);
    } else if let Some(p) = parent.as_ref().join(&relpath).to_str() {
        if Path::new(&p).exists() {
            return std::fs::read_to_string(p);
        }
    }

    Ok(asset.to_string())
}

#[cfg(test)]
mod test {
    use cargo_toml::Value;

    use super::*;

    #[test]
    fn test_config_new() {
        let config = Config::new(Path::new("."), None, &[]).unwrap();
        let pkg = config.manifest.package.unwrap();
        assert_eq!(pkg.name, "cargo-generate-rpm");

        assert!(matches!(Config::new(Path::new("not_exist_dir"), None, &[]),
            Err(Error::FileIo(path, error)) if path == PathBuf::from("not_exist_dir/Cargo.toml") && error.kind() == std::io::ErrorKind::NotFound));
        assert!(
            matches!(Config::new(Path::new(""), Some(Path::new("not_exist_dir")), &[]),
            Err(Error::FileIo(path, error)) if path == PathBuf::from("not_exist_dir/Cargo.toml") && error.kind() == std::io::ErrorKind::NotFound)
        );
    }

    #[test]
    fn test_config_new_with_workspace() {
        let tempdir = tempfile::tempdir().unwrap();

        let workspace_dir = tempdir.path().join("workspace");
        let project_dir = workspace_dir.join("bar");

        std::fs::create_dir(&workspace_dir).unwrap();
        std::fs::write(
            workspace_dir.join("Cargo.toml"),
            r#"
[workspace]
members = ["bar"]

[workspace.package]
version = "1.2.3"
authors = ["Nice Folks"]
description = "A short description of my package"
documentation = "https://example.com/bar"
        "#,
        )
        .unwrap();
        std::fs::create_dir(&project_dir).unwrap();
        std::fs::write(
            project_dir.join("Cargo.toml"),
            r#"
[package]
name = "bar"
version.workspace = true
authors.workspace = true
description.workspace = true
documentation.workspace = true
        "#,
        )
        .unwrap();

        let config =
            Config::new(project_dir.as_path(), Some(workspace_dir.as_path()), &[]).unwrap();
        let pkg = config.manifest.package.unwrap();
        assert_eq!(pkg.name, "bar");
        assert_eq!(pkg.version.get().unwrap(), "1.2.3");

        assert!(
            matches!(Config::new(Path::new("not_exist_dir"), Some(workspace_dir.as_path()), &[]),
            Err(Error::FileIo(path, error)) if path == PathBuf::from("not_exist_dir/Cargo.toml") && error.kind() == std::io::ErrorKind::NotFound)
        );
        assert!(
            matches!(Config::new(project_dir.as_path(), Some(Path::new("not_exist_dir")), &[]),
            Err(Error::FileIo(path, error)) if path == PathBuf::from("not_exist_dir/Cargo.toml") && error.kind() == std::io::ErrorKind::NotFound)
        );
    }

    #[test]
    fn test_new() {
        let config = Config::new(Path::new(""), None, &[]).unwrap();
        assert_eq!(config.manifest.package.unwrap().name, "cargo-generate-rpm");
        assert_eq!(config.manifest_path, PathBuf::from("Cargo.toml"));
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

        assert!(Config::table_to_dependencies(&table).is_ok());

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
        let config = Config::new(Path::new("."), None, &[]).unwrap();
        let args = crate::cli::Cli {
            ..Default::default()
        };
        let target = BuildTarget::new(&args);
        let cfg = BuilderConfig::new(&target, &args);
        let builder = config.create_rpm_builder(cfg);

        assert!(if Path::new("target/release/cargo-generate-rpm").exists() {
            builder.is_ok()
        } else {
            matches!(builder, Err(Error::Config(ConfigError::AssetFileNotFound(path))) if path.to_str() == Some("target/release/cargo-generate-rpm"))
        });
    }
}
