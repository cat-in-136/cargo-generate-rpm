use std::env::consts::ARCH;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct BuildTarget {
    pub target_dir: Option<String>,
    pub target: Option<String>,
    pub arch: Option<String>,
}

impl BuildTarget {
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
                .and_then(|v| v.splitn(2, "-").nth(0))
                .unwrap_or_else(|| ARCH);

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
        let target = BuildTarget::default();
        assert_eq!(target.build_target_path(), PathBuf::from("target"));

        let target = BuildTarget {
            target_dir: Some("/tmp/foobar/target".to_string()),
            ..Default::default()
        };
        assert_eq!(
            target.build_target_path(),
            PathBuf::from("/tmp/foobar/target")
        );
    }

    #[test]
    fn test_target_path() {
        let target = BuildTarget::default();
        assert_eq!(
            target.target_path("release"),
            PathBuf::from("target/release")
        );

        let target = BuildTarget {
            target: Some("x86_64-unknown-linux-gnu".to_string()),
            ..Default::default()
        };
        assert_eq!(
            target.target_path("release"),
            PathBuf::from("target/x86_64-unknown-linux-gnu/release")
        );

        let target = BuildTarget {
            target_dir: Some("/tmp/foobar/target".to_string()),
            ..Default::default()
        };
        assert_eq!(
            target.target_path("debug"),
            PathBuf::from("/tmp/foobar/target/debug")
        );

        let target = BuildTarget {
            target_dir: Some("/tmp/foobar/target".to_string()),
            target: Some("x86_64-unknown-linux-gnu".to_string()),
            ..Default::default()
        };
        assert_eq!(
            target.target_path("debug"),
            PathBuf::from("/tmp/foobar/target/x86_64-unknown-linux-gnu/debug")
        );
    }
}
