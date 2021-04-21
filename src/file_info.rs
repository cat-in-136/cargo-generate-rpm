use rpm::RPMFileOptions;
use toml::value::Table;

use crate::build_target::BuildTarget;
use crate::error::ConfigError;
use std::path::{Path, PathBuf};

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct FileInfo<'a, 'b, 'c, 'd> {
    pub source: &'a str,
    pub dest: &'b str,
    pub user: Option<&'c str>,
    pub group: Option<&'d str>,
    pub mode: Option<usize>,
    pub config: bool,
    pub doc: bool,
}

impl FileInfo<'_, '_, '_, '_> {
    pub fn list_from_metadata(metadata: &Table) -> Result<Vec<FileInfo>, ConfigError> {
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
            let mode = Self::get_mode(&table, source, idx)?;
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

    fn get_mode(table: &Table, source: &str, idx: usize) -> Result<Option<usize>, ConfigError> {
        if let Some(mode) = table.get("mode") {
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
            Ok(Some(file_mode.unwrap_or_default() | mode))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn generate_rpm_file_path<P: AsRef<Path>>(
        &self,
        build_target: &BuildTarget,
        parent: P,
    ) -> Result<PathBuf, ConfigError> {
        let source = if let Some(rel_path) = self.source.strip_prefix("target/release/") {
            build_target.target_path("release").join(rel_path)
        } else {
            PathBuf::from(self.source)
        };

        if source.exists() {
            Ok(source)
        } else if source.is_relative() && parent.as_ref().join(source.clone()).exists() {
            Ok(parent.as_ref().join(source))
        } else {
            Err(ConfigError::AssetFileNotFound(self.source.to_string()))
        }
    }

    pub(crate) fn generate_rpm_file_options(&self) -> RPMFileOptions {
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
    use crate::config::Config;

    #[test]
    fn test_list_from_metadata() {
        let config = Config::new("Cargo.toml").unwrap();
        let metadata = config.metadata().unwrap();
        let files = FileInfo::list_from_metadata(&metadata).unwrap();
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
}
