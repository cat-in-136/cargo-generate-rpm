use cargo_toml::Error as CargoTomlError;
use std::error::Error as StdError;
use std::ffi::OsString;
use std::fmt::{Debug, Display, Formatter};
use std::io::Error as IoError;
use std::path::PathBuf;
use toml::de::Error as TomlDeError;

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq, Hash)]
pub enum DottedBareKeyLexError {
    #[error("invalid key-joint character `.'")]
    InvalidDotChar,
    #[error("invalid character `{0}' and quoted key is not supported")]
    QuotedKey(char),
    #[error("invalid character `{0}'")]
    InvalidChar(char),
}

#[derive(thiserror::Error, Debug, Clone)]
pub enum ConfigError {
    #[error("Missing field: {0}")]
    Missing(String),
    #[error("Field {0} must be {1}")]
    WrongType(String, &'static str),
    #[error("Invalid Glob at {0}: {1}")]
    AssetGlobInvalid(usize, &'static str),
    #[error("Glob at {0}-th asset found {1} which doesn't appear to be in {2}")]
    AssetGlobPathInvalid(usize, String, String),
    #[error("Failed reading {0}-th asset")]
    AssetReadFailed(usize),
    #[error("{1} of {0}-th asset is undefined")]
    AssetFileUndefined(usize, &'static str),
    #[error("{1} of {0}-th asset must be {2}")]
    AssetFileWrongType(usize, &'static str, &'static str),
    #[error("Asset file not found: {0}")]
    AssetFileNotFound(PathBuf),
    #[error("Invalid dependency version specified for {0}")]
    WrongDependencyVersion(String),
    #[error("Invalid branch path `{0}'")]
    WrongBranchPathOfToml(String, #[source] DottedBareKeyLexError),
    #[error("Branch `{0}' not found")]
    BranchPathNotFoundInToml(String),
    #[error("Field {1} for file {0} has the following error: {2}")]
    AssetFileRpm(usize, &'static str, #[source] std::rc::Rc<rpm::Error>),
}

#[derive(thiserror::Error, Debug)]
pub struct FileAnnotatedError<E: StdError + Display>(pub Option<PathBuf>, #[source] pub E);

impl<E: StdError + Display> Display for FileAnnotatedError<E> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            None => Display::fmt(&self.1, f),
            Some(path) => write!(f, "{}: {}", path.as_path().display(), self.1),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum AutoReqError {
    #[error("Failed to execute `{file}`: {1}", file = .0.clone().into_string().unwrap_or_default())]
    ProcessError(OsString, #[source] IoError),
    #[error(transparent)]
    Io(#[from] IoError),
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Cargo.toml: {0}")]
    CargoToml(#[from] CargoTomlError),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("Invalid value of environment variable {0}: {1}")]
    #[allow(clippy::enum_variant_names)] // Allow bad terminology for compatibility
    EnvError(&'static str, String),
    #[error(transparent)]
    ParseTomlFile(#[from] FileAnnotatedError<TomlDeError>),
    #[error(transparent)]
    ExtraConfig(#[from] FileAnnotatedError<ConfigError>),
    #[error(transparent)]
    AutoReq(#[from] AutoReqError),
    #[error(transparent)]
    Rpm(#[from] rpm::Error),
    #[error("{1}: {0}")]
    FileIo(PathBuf, #[source] IoError),
    #[error(transparent)]
    Io(#[from] IoError),
}
