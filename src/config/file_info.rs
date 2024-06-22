use glob::glob;
use toml::value::Table;

use crate::build_target::BuildTarget;
use crate::error::ConfigError;
use std::path::{Path, PathBuf};
use toml::Value;

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct FileInfo<'a, 'b, 'c, 'd, 'e> {
    pub source: &'a str,
    pub dest: &'b str,
    pub user: Option<&'c str>,
    pub group: Option<&'d str>,
    pub mode: Option<usize>,
    pub config: bool,
    pub config_noreplace: bool,
    pub doc: bool,
    pub caps: Option<&'e str>,
}

impl FileInfo<'_, '_, '_, '_, '_> {
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
            let mode = Self::get_mode(table, source, idx)?;
            let caps = if let Some(caps) = table.get("caps") {
                Some(
                    caps.as_str()
                        .ok_or(ConfigError::AssetFileWrongType(idx, "caps", "string"))?,
                )
            } else {
                None
            };
            let (config, config_noreplace, _config_missingok) = match table.get("config") {
                Some(Value::Boolean(v)) => (*v, false, false),
                Some(Value::String(v)) if v.eq("noreplace") => (false, true, false),
                //Some(Value::String(v)) if v.eq("missingok") => (false, false, true),
                None => (false, false, false),
                _ => {
                    return Err(ConfigError::AssetFileWrongType(
                        idx,
                        "config",
                        "bool or \"noreplace\"",
                    ))
                } //_ => return Err(ConfigError::AssetFileWrongType(idx, "config", "bool or \"noreplace\" or \"missingok\"")),
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
                config_noreplace,
                doc,
                caps,
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

    fn generate_expanded_path<P: AsRef<Path>>(
        &self,
        build_target: &BuildTarget,
        parent: P,
        idx: usize,
    ) -> Result<Vec<(PathBuf, String)>, ConfigError> {
        let source = get_asset_rel_path(self.source, build_target);

        let expanded = expand_glob(source.as_str(), self.dest, idx)?;
        if !expanded.is_empty() {
            return Ok(expanded);
        }

        if let Some(src) = parent.as_ref().join(&source).to_str() {
            let expanded = expand_glob(src, self.dest, idx)?;
            if !expanded.is_empty() {
                return Ok(expanded);
            }
        }

        Err(ConfigError::AssetFileNotFound(PathBuf::from(source)))
    }

    fn generate_rpm_file_options<T: ToString>(
        &self,
        dest: T,
        idx: usize,
    ) -> Result<rpm::FileOptions, ConfigError> {
        let mut rpm_file_option = rpm::FileOptions::new(dest.to_string());
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
        if self.config_noreplace {
            rpm_file_option = rpm_file_option.is_config_noreplace();
        }
        if self.doc {
            rpm_file_option = rpm_file_option.is_doc();
        }
        if let Some(caps) = self.caps {
            rpm_file_option = rpm_file_option
                .caps(caps)
                .map_err(|err| ConfigError::AssetFileRpm(idx, "caps", err.into()))?;
        }
        Ok(rpm_file_option.into())
    }

    pub(crate) fn generate_rpm_file_entry<P: AsRef<Path>>(
        &self,
        build_target: &BuildTarget,
        parent: P,
        idx: usize,
    ) -> Result<Vec<(PathBuf, rpm::FileOptions)>, ConfigError> {
        self.generate_expanded_path(build_target, parent, idx)?
            .iter()
            .map(|(src, dst)| {
                self.generate_rpm_file_options(dst, idx)
                    .map(|v| (src.clone(), v))
            })
            .collect::<Result<Vec<_>, _>>()
    }
}

fn get_base_from_glob(glob: &'_ str) -> PathBuf {
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

pub(crate) fn get_asset_rel_path(asset: &str, build_target: &BuildTarget) -> String {
    let dir_name = match build_target.profile() {
        "dev" => "debug",
        p => p,
    };
    asset
        .strip_prefix("target/release/")
        .or_else(|| asset.strip_prefix(&format!("target/{dir_name}/")))
        .and_then(|rel_path| {
            build_target
                .target_path(dir_name)
                .join(rel_path)
                .to_str()
                .map(|v| v.to_string())
        })
        .unwrap_or(asset.to_string())
}

fn expand_glob(
    source: &str,
    dest: &str,
    idx: usize,
) -> Result<Vec<(PathBuf, String)>, ConfigError> {
    let mut vec = Vec::new();
    if source.contains('*') {
        let base = get_base_from_glob(source);
        for path in glob(source).map_err(|e| ConfigError::AssetGlobInvalid(idx, e.msg))? {
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
            let dst = dest_path.to_str().unwrap().to_owned();

            vec.push((file, dst));
        }
    } else if Path::new(source).exists() {
        let file = PathBuf::from(source);
        let dst = match file.file_name().map(|v| v.to_str()) {
            Some(Some(filename)) if dest.ends_with('/') => dest.to_string() + filename,
            _ => dest.to_string(),
        };

        vec.push((file, dst));
    }

    Ok(vec)
}

#[cfg(test)]
mod test {
    use super::*;
    use cargo_toml::Manifest;
    use std::fs::File;

    #[test]
    fn test_get_base_from_glob() {
        let toml_dir = "../".to_string()
            + std::env::current_dir()
                .unwrap()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap();
        let toml_ptn = toml_dir.to_string() + "/*.toml";

        let tests = &[
            ("*", PathBuf::from("")),
            ("src/auto_req/*.rs", PathBuf::from("src/auto_req")),
            ("src/not_a_directory/*.rs", PathBuf::from("src")),
            ("*.things", PathBuf::from("")),
            (toml_ptn.as_str(), PathBuf::from(toml_dir)),
            ("src/auto_req", PathBuf::from("src/auto_req")), // shouldn't currently happen as we detect '*' in the string, but test the code path anyway
        ];

        for test in tests {
            let out = get_base_from_glob(test.0);
            assert_eq!(
                out, test.1,
                "get_base_from_glob({0:?}) shall equal to {1:?}",
                test.0, test.1
            );
        }
    }

    #[test]
    fn test_new() {
        let manifest = Manifest::from_path("./Cargo.toml").unwrap();
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
                    source: "target/release/cargo-generate-rpm",
                    dest: "/usr/bin/cargo-generate-rpm",
                    user: None,
                    group: None,
                    mode: Some(0o0100755),
                    config: false,
                    config_noreplace: false,
                    doc: false,
                    caps: None,
                },
                FileInfo {
                    source: "LICENSE",
                    dest: "/usr/share/doc/cargo-generate-rpm/LICENSE",
                    user: None,
                    group: None,
                    mode: Some(0o0100644),
                    config: false,
                    config_noreplace: false,
                    doc: true,
                    caps: None,
                },
                FileInfo {
                    source: "README.md",
                    dest: "/usr/share/doc/cargo-generate-rpm/README.md",
                    user: None,
                    group: None,
                    mode: Some(0o0100644),
                    config: false,
                    config_noreplace: false,
                    doc: true,
                    caps: None,
                },
            ]
        );
    }

    #[test]
    fn test_generate_rpm_file_path() {
        let tempdir = tempfile::tempdir().unwrap();
        let args = crate::cli::Cli::default();
        let target = BuildTarget::new(&args);
        let file_info = FileInfo {
            source: "README.md",
            dest: "/usr/share/doc/cargo-generate-rpm/README.md",
            user: None,
            group: None,
            mode: None,
            config: false,
            config_noreplace: false,
            doc: true,
            caps: Some("cap_sys_admin=pe"),
        };
        let expanded = file_info
            .generate_expanded_path(&target, &tempdir, 0)
            .unwrap();
        assert_eq!(
            expanded
                .iter()
                .map(|(src, dst)| { (src.as_path().to_str(), dst) })
                .collect::<Vec<_>>(),
            vec![(Some(file_info.source), &file_info.dest.to_string())]
        );

        let file_info = FileInfo {
            source: "not-exist-file",
            dest: "/usr/share/doc/cargo-generate-rpm/not-exist-file",
            user: None,
            group: None,
            mode: None,
            config: false,
            config_noreplace: false,
            doc: true,
            caps: None,
        };
        assert!(
            matches!(file_info.generate_expanded_path(&target, &tempdir, 0),
                   Err(ConfigError::AssetFileNotFound(v)) if v == PathBuf::from( "not-exist-file"))
        );

        std::fs::create_dir_all(tempdir.path().join("target/release")).unwrap();
        File::create(tempdir.path().join("target/release/foobar")).unwrap();
        let file_info = FileInfo {
            source: "target/release/foobar",
            dest: "/usr/bin/foobar",
            user: None,
            group: None,
            mode: None,
            config: false,
            config_noreplace: false,
            doc: false,
            caps: None,
        };
        let expanded = file_info
            .generate_expanded_path(&target, &tempdir, 0)
            .unwrap();
        assert_eq!(
            expanded
                .iter()
                .map(|(src, dst)| { (src.as_path().to_str(), dst) })
                .collect::<Vec<_>>(),
            vec![(
                Some(
                    tempdir
                        .path()
                        .join("target/release/foobar")
                        .to_str()
                        .unwrap()
                ),
                &file_info.dest.to_string()
            )]
        );

        let args = crate::cli::Cli {
            target_dir: Some(
                tempdir
                    .path()
                    .join("target")
                    .as_os_str()
                    .to_str()
                    .unwrap()
                    .to_string(),
            ),
            ..Default::default()
        };
        let target = BuildTarget::new(&args);
        let expanded = file_info
            .generate_expanded_path(&target, &tempdir, 0)
            .unwrap();
        assert_eq!(
            expanded
                .iter()
                .map(|(src, dst)| { (src.as_path().to_str(), dst) })
                .collect::<Vec<_>>(),
            vec![(
                Some(
                    tempdir
                        .path()
                        .join("target/release/foobar")
                        .to_str()
                        .unwrap()
                ),
                &file_info.dest.to_string()
            )]
        );

        std::fs::create_dir_all(tempdir.path().join("target/target-triple/my-profile")).unwrap();
        File::create(
            tempdir
                .path()
                .join("target/target-triple/my-profile/my-bin"),
        )
        .unwrap();
        let file_info = FileInfo {
            source: "target/release/my-bin",
            dest: "/usr/bin/my-bin",
            user: None,
            group: None,
            mode: None,
            config: false,
            config_noreplace: false,
            doc: false,
            caps: None,
        };
        let args = crate::cli::Cli {
            target_dir: Some(
                tempdir
                    .path()
                    .join("target")
                    .as_os_str()
                    .to_str()
                    .unwrap()
                    .to_string(),
            ),
            target: Some("target-triple".to_string()),
            profile: "my-profile".to_string(),
            ..Default::default()
        };
        let target = BuildTarget::new(&args);
        let expanded = file_info
            .generate_expanded_path(&target, &tempdir, 0)
            .unwrap();
        assert_eq!(
            expanded
                .iter()
                .map(|(src, dst)| { (src.as_path().to_str(), dst) })
                .collect::<Vec<_>>(),
            vec![(
                Some(
                    tempdir
                        .path()
                        .join("target/target-triple/my-profile/my-bin")
                        .to_str()
                        .unwrap()
                ),
                &file_info.dest.to_string()
            )]
        );
    }

    #[test]
    fn test_expand_glob() {
        assert_eq!(
            expand_glob("*.md", "/usr/share/doc/cargo-generate-rpm/", 0).unwrap(),
            vec![(
                PathBuf::from("README.md"),
                "/usr/share/doc/cargo-generate-rpm/README.md".into()
            )]
        );

        assert_eq!(
            expand_glob("*-not-exist-glob", "/usr/share/doc/cargo-generate-rpm/", 0).unwrap(),
            vec![]
        );

        assert_eq!(
            expand_glob(
                "README.md",
                "/usr/share/doc/cargo-generate-rpm/README.md",
                2
            )
            .unwrap(),
            vec![(
                PathBuf::from("README.md"),
                "/usr/share/doc/cargo-generate-rpm/README.md".into()
            )]
        );

        assert_eq!(
            expand_glob(
                "README.md",
                "/usr/share/doc/cargo-generate-rpm/", // specifying directory
                0
            )
            .unwrap(),
            vec![(
                PathBuf::from("README.md"),
                "/usr/share/doc/cargo-generate-rpm/README.md".into()
            )]
        );
    }
}
