use glob::glob;
use rpm::RPMFileOptions;
use toml::value::Table;

use crate::build_target::BuildTarget;
use crate::error::ConfigError;
use std::path::{Path, PathBuf};
use toml::Value;

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct FileInfo<'c, 'd> {
    pub source: String,
    pub dest: String,
    pub user: Option<&'c str>,
    pub group: Option<&'d str>,
    pub mode: Option<usize>,
    pub config: bool,
    pub doc: bool,
}

impl FileInfo<'_, '_> {
    pub fn new(assets: &[Value]) -> Result<Vec<FileInfo>, ConfigError> {
        let mut files = Vec::with_capacity(assets.len());
        for (idx, value) in assets.iter().enumerate() {
            let table = value
                .as_table()
                .ok_or(ConfigError::AssetFileUndefined(idx, "source"))?;
            let source = table
                .get("source")
                .ok_or(ConfigError::AssetFileUndefined(idx, "source"))?
                .as_str()
                .ok_or(ConfigError::AssetFileWrongType(idx, "source", "string"))?
                .to_owned();
            let dest = table
                .get("dest")
                .ok_or(ConfigError::AssetFileUndefined(idx, "dest"))?
                .as_str()
                .ok_or(ConfigError::AssetFileWrongType(idx, "dest", "string"))?
                .to_owned();

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
            let mode = Self::get_mode(&table, &source, idx)?;
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

            if source.contains('*') {
                let base = _get_base_from_glob(&source);
                for path in glob(&source).map_err(|e| ConfigError::AssetGlobInvalid(idx, e.msg))? {
                    let file = path.map_err(|_| ConfigError::AssetReadFailed(idx))?;
                    if file.is_dir() {
                        continue;
                    }
                    let rel_path = file.strip_prefix(&base).map_err(|_| {
                        ConfigError::AssetGlobPathInvalid(
                            idx,
                            file.to_str().unwrap().to_owned(),
                            base.to_str().unwrap().to_owned(),
                        )
                    })?;
                    let dest_path = Path::new(&dest).join(rel_path);
                    let src = file.to_str().unwrap().to_owned();
                    let dst = dest_path.to_str().unwrap().to_owned();
                    files.push(FileInfo {
                        source: src,
                        dest: dst,
                        user,
                        group,
                        mode,
                        config,
                        doc,
                    })
                }
            } else {
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
            PathBuf::from(&self.source)
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
        let mut rpm_file_option = RPMFileOptions::new(&self.dest);
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

fn _get_base_from_glob(glob: &'_ str) -> PathBuf {
    let base = match glob.split_once('*') {
        Some((before, _)) => before,
        None => glob,
    };

    let base_path = Path::new(base);
    let out_path = if base_path.is_dir() {
        base_path
    } else if let Some(parent) = base_path.parent() {
        parent
    } else {
        base_path
    };

    out_path.into()
}

#[cfg(test)]
mod test {
    use super::*;
    use cargo_toml::Manifest;
    use std::fs::File;

    #[test]
    fn test_get_base_from_glob() {
        let tests = &[
            ("*", PathBuf::from("")),
            ("src/auto_req/*.rs", PathBuf::from("src/auto_req")),
            ("src/not_a_directory/*.rs", PathBuf::from("src")),
            ("*.things", PathBuf::from("")),
            ("src/auto_req", PathBuf::from("src/auto_req")), // shouldn't currently happen as we detect '*' in the string, but test the code path anyway
        ];

        for test in tests {
            let out = _get_base_from_glob(test.0);
            assert_eq!(out, test.1);
        }
    }

    #[test]
    fn test_new() {
        let manifest = Manifest::from_path("Cargo.toml").unwrap();
        let metadata = manifest.package.unwrap().metadata.unwrap();
        let metadata = metadata
            .as_table()
            .unwrap()
            .get("generate-rpm")
            .unwrap()
            .as_table()
            .unwrap();
        let assets = metadata.get("assets").and_then(|v| v.as_array()).unwrap();
        let files = FileInfo::new(assets.as_slice()).unwrap();
        assert_eq!(
            files,
            vec![
                FileInfo {
                    source: "target/release/cargo-generate-rpm".into(),
                    dest: "/usr/bin/cargo-generate-rpm".into(),
                    user: None,
                    group: None,
                    mode: Some(0o0100755),
                    config: false,
                    doc: false
                },
                FileInfo {
                    source: "LICENSE".into(),
                    dest: "/usr/share/doc/cargo-generate-rpm/LICENSE".into(),
                    user: None,
                    group: None,
                    mode: Some(0o0100644),
                    config: false,
                    doc: true
                },
                FileInfo {
                    source: "README.md".into(),
                    dest: "/usr/share/doc/cargo-generate-rpm/README.md".into(),
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
    fn test_generate_rpm_file_path() {
        let tempdir = tempfile::tempdir().unwrap();
        let target = BuildTarget::default();
        let file_info = FileInfo {
            source: "README.md".into(),
            dest: "/usr/share/doc/cargo-generate-rpm/README.md".into(),
            user: None,
            group: None,
            mode: None,
            config: false,
            doc: true,
        };
        assert_eq!(
            file_info.generate_rpm_file_path(&target, &tempdir).unwrap(),
            PathBuf::from("README.md")
        );

        let file_info = FileInfo {
            source: "not-exist-file".into(),
            dest: "/usr/share/doc/cargo-generate-rpm/not-exist-file".into(),
            user: None,
            group: None,
            mode: None,
            config: false,
            doc: true,
        };
        assert!(matches!(
        file_info.generate_rpm_file_path(&target, &tempdir),
        Err(ConfigError::AssetFileNotFound(v)) if v == "not-exist-file"
        ));

        std::fs::create_dir_all(tempdir.path().join("target/release")).unwrap();
        File::create(tempdir.path().join("target/release/foobar")).unwrap();
        let file_info = FileInfo {
            source: "target/release/foobar".into(),
            dest: "/usr/bin/foobar".into(),
            user: None,
            group: None,
            mode: None,
            config: false,
            doc: false,
        };
        assert_eq!(
            file_info.generate_rpm_file_path(&target, &tempdir).unwrap(),
            tempdir.path().join("target/release/foobar")
        );

        let target = BuildTarget {
            target_dir: Some(
                tempdir
                    .path()
                    .join("target")
                    .as_os_str()
                    .to_str()
                    .unwrap()
                    .to_string(),
            ),
            target: None,
            ..Default::default()
        };
        assert_eq!(
            file_info.generate_rpm_file_path(&target, ".").unwrap(),
            tempdir.path().join("target/release/foobar")
        );

        std::fs::create_dir_all(tempdir.path().join("target/foobarbaz/release")).unwrap();
        File::create(tempdir.path().join("target/foobarbaz/release/foobarbaz")).unwrap();
        let file_info = FileInfo {
            source: "target/release/foobarbaz".into(),
            dest: "/usr/bin/foobarbaz".into(),
            user: None,
            group: None,
            mode: None,
            config: false,
            doc: false,
        };
        let target = BuildTarget {
            target_dir: Some(
                tempdir
                    .path()
                    .join("target")
                    .as_os_str()
                    .to_str()
                    .unwrap()
                    .to_string(),
            ),
            target: Some("foobarbaz".to_string()),
            ..Default::default()
        };
        assert_eq!(
            file_info.generate_rpm_file_path(&target, ".").unwrap(),
            tempdir.path().join("target/foobarbaz/release/foobarbaz")
        );
    }
}
