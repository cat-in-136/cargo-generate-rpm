use std::env::consts::ARCH;
use std::path::{Path, PathBuf};

use crate::cli::Cli;

#[derive(Debug, Clone)]
pub struct BuildTarget {
    target_dir: Option<String>,
    target: Option<String>,
    profile: String,
    arch: Option<String>,
}

impl BuildTarget {
    pub fn new(args: &Cli) -> Self {
        Self {
            target_dir: args.target_dir.clone(),
            target: args.target.clone(),
            profile: args.profile.clone(),
            arch: args.arch.clone(),
        }
    }

    pub fn profile(&self) -> &str {
        self.profile.as_str()
    }

    pub fn build_target_path(&self) -> PathBuf {
        if let Some(target_dir) = &self.target_dir {
            PathBuf::from(&target_dir)
        } else {
            let target_build_dir = std::env::var("CARGO_BUILD_TARGET_DIR")
                .or_else(|_| std::env::var("CARGO_TARGET_DIR"))
                .unwrap_or("target".to_string());
            PathBuf::from(&target_build_dir)
        }
    }

    pub fn target_path<P: AsRef<Path>>(&self, dir_name: P) -> PathBuf {
        let mut path = self.build_target_path();
        if let Some(target) = &self.target {
            path = path.join(target)
        }
        path.join(dir_name)
    }

    pub fn binary_arch(&self) -> String {
        if let Some(arch) = &self.arch {
            arch.clone()
        } else {
            let arch = self
                .target
                .as_ref()
                .and_then(|v| v.split('-').next())
                .unwrap_or(ARCH);

            match arch {
                "x86" => "i586",
                "arm" => "armhfp",
                "powerpc" => "ppc",
                "powerpc64" => "ppc64",
                _ => arch,
            }
            .to_string()
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_build_target_path() {
        let args = crate::cli::Cli::default();
        let target = BuildTarget::new(&args);
        assert_eq!(target.build_target_path(), PathBuf::from("target"));

        let target = BuildTarget {
            target_dir: Some("/tmp/foobar/target".to_string()),
            ..target
        };
        assert_eq!(
            target.build_target_path(),
            PathBuf::from("/tmp/foobar/target")
        );
    }

    #[test]
    fn test_target_path() {
        let args = crate::cli::Cli::default();
        let default_target = BuildTarget::new(&args);
        assert_eq!(
            default_target.target_path("release"),
            PathBuf::from("target/release")
        );

        let target = BuildTarget {
            target: Some("x86_64-unknown-linux-gnu".to_string()),
            ..default_target.clone()
        };
        assert_eq!(
            target.target_path("release"),
            PathBuf::from("target/x86_64-unknown-linux-gnu/release")
        );

        let target = BuildTarget {
            target_dir: Some("/tmp/foobar/target".to_string()),
            ..default_target.clone()
        };
        assert_eq!(
            target.target_path("debug"),
            PathBuf::from("/tmp/foobar/target/debug")
        );

        let target = BuildTarget {
            target_dir: Some("/tmp/foobar/target".to_string()),
            target: Some("x86_64-unknown-linux-gnu".to_string()),
            ..default_target
        };
        assert_eq!(
            target.target_path("debug"),
            PathBuf::from("/tmp/foobar/target/x86_64-unknown-linux-gnu/debug")
        );
    }
}
