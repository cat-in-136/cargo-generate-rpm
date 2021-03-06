use cargo_toml::Error as CargoTomlError;
use rpm::RPMError;
use std::ffi::OsString;
use std::io::Error as IoError;
use std::path::PathBuf;

#[derive(thiserror::Error, Debug, Clone)]
pub enum ConfigError {
    #[error("Missing field: {0}")]
    Missing(&'static str),
    #[error("Field {0} must be {1}")]
    WrongType(&'static str, &'static str),
    #[error("{1} of {0}-th asset is undefined")]
    AssetFileUndefined(usize, &'static str),
    #[error("{1} of {0}-th asset must be {2}")]
    AssetFileWrongType(usize, &'static str, &'static str),
    #[error("Asset file not found: {0}")]
    AssetFileNotFound(String),
    #[error("Invalid dependency version specified for {0}")]
    WrongDependencyVersion(String),
}

#[derive(thiserror::Error, Debug)]
pub enum AutoReqError {
    #[error("Wrong auto-req mode")]
    WrongMode,
    #[error("Failed to execute `{}`: {1}", .0.clone().into_string().unwrap_or_default())]
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
    #[error(transparent)]
    AutoReq(#[from] AutoReqError),
    #[error(transparent)]
    Rpm(#[from] RPMError),
    #[error("{1}: {0}")]
    FileIo(PathBuf, #[source] IoError),
    #[error(transparent)]
    Io(#[from] IoError),
}
