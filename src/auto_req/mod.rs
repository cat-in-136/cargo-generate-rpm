use crate::error::AutoReqError;
use std::convert::TryFrom;
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

impl TryFrom<String> for AutoReqMode {
    type Error = AutoReqError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "auto" | "" => Ok(AutoReqMode::Auto),
            "no" | "disabled" => Ok(AutoReqMode::Disabled),
            "builtin" => Ok(AutoReqMode::BuiltIn),
            "find-requires" => Ok(AutoReqMode::Script(PathBuf::from(RPM_FIND_REQUIRES))),
            v if Path::new(v).exists() => Ok(AutoReqMode::Script(PathBuf::from(v))),
            _ => Err(AutoReqError::WrongMode),
        }
    }
}

#[test]
pub fn test_try_from_for_auto_req_mode() {
    for (text, auto_req_mode) in &[
        ("auto", AutoReqMode::Auto),
        ("", AutoReqMode::Auto),
        ("no", AutoReqMode::Disabled),
        ("disabled", AutoReqMode::Disabled),
        (
            "find-requires",
            AutoReqMode::Script(PathBuf::from(RPM_FIND_REQUIRES)),
        ),
        (file!(), AutoReqMode::Script(PathBuf::from(file!()))),
    ] {
        assert_eq!(
            AutoReqMode::try_from(text.to_string()).unwrap(),
            *auto_req_mode
        );
    }

    assert!(matches!(
        AutoReqMode::try_from("invalid-value".to_string()),
        Err(AutoReqError::WrongMode)
    ));
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
