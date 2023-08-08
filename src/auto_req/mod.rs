use crate::{cli, error::AutoReqError};
use std::path::{Path, PathBuf};

mod builtin;
mod script;

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

impl From<&Option<cli::AutoReqMode>> for AutoReqMode {
    fn from(value: &Option<cli::AutoReqMode>) -> Self {
        use cli::AutoReqMode as M;
        use AutoReqMode::*;

        match value {
            None => Auto,
            Some(M::Disabled) => Disabled,
            Some(M::Builtin) => BuiltIn,
            Some(M::Script(path)) => Script(path.into()),
            Some(M::FindRequires) => Script(PathBuf::from(RPM_FIND_REQUIRES)),
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
