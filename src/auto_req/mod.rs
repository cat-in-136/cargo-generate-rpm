use crate::{cli, error::AutoReqError};
use std::path::{Path, PathBuf};

mod builtin;
mod script;

/// The path to the system default find-requires program
const RPM_FIND_REQUIRES: &str = "/usr/lib/rpm/find-requires";

/// The method to auto-req
#[derive(Debug, PartialEq, Eq)]
pub enum AutoReqMode {
    /// Automatically selected
    Auto,
    /// Disable
    Disabled,
    /// `find-requires` script
    Script(PathBuf),
    /// Builtin
    BuiltIn,
}

impl From<cli::AutoReqMode> for AutoReqMode {
    fn from(value: cli::AutoReqMode) -> Self {
        match value {
            cli::AutoReqMode::Auto => AutoReqMode::Auto,
            cli::AutoReqMode::Disabled => AutoReqMode::Disabled,
            cli::AutoReqMode::Builtin => AutoReqMode::BuiltIn,
            cli::AutoReqMode::FindRequires => AutoReqMode::Script(PathBuf::from(RPM_FIND_REQUIRES)),
            cli::AutoReqMode::Script(path) => AutoReqMode::Script(path),
        }
    }
}

/// Find requires
pub fn find_requires<T: IntoIterator<Item = P>, P: AsRef<Path>>(
    files: T,
    mode: AutoReqMode,
) -> Result<Vec<String>, AutoReqError> {
    match mode {
        AutoReqMode::Auto => {
            if Path::new(RPM_FIND_REQUIRES).exists() {
                find_requires(files, AutoReqMode::Script(PathBuf::from(RPM_FIND_REQUIRES)))
            } else {
                find_requires(files, AutoReqMode::BuiltIn)
            }
        }
        AutoReqMode::Disabled => Ok(Vec::new()),
        AutoReqMode::Script(script) => Ok(script::find_requires(
            files.into_iter().collect::<Vec<_>>().as_slice(),
            script.as_path(),
        )?),
        AutoReqMode::BuiltIn => Ok(builtin::find_requires(
            files.into_iter().collect::<Vec<_>>().as_slice(),
        )?),
    }
}
