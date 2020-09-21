use cargo_toml::Error as CargoTomlError;
use rpm::RPMError;
use std::error::Error as StdErr;
use std::fmt;
use std::fmt::Debug;

#[derive(Debug, Clone)]
pub enum ConfigError {
    Missing(&'static str),
    WrongType(&'static str, &'static str),
    AssetFileUndefined(usize, &'static str),
    AssetFileWrongType(usize, &'static str, &'static str),
}

impl StdErr for ConfigError {
    fn source(&self) -> Option<&(dyn StdErr + 'static)> {
        None
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Missing(field) => f.write_fmt(format_args!("Missing field: {}", field)),
            ConfigError::WrongType(field, value_type) => {
                f.write_fmt(format_args!("Field {} must be {}", field, value_type))
            }
            ConfigError::AssetFileUndefined(idx, field) => {
                f.write_fmt(format_args!("{} of {}-th asset is undefined", field, idx))
            }
            ConfigError::AssetFileWrongType(idx, field, value_type) => f.write_fmt(format_args!(
                "{} of {}-th asset must be {}",
                field, idx, value_type
            )),
        }
    }
}

#[derive(Debug)]
pub enum Error {
    CargoToml(CargoTomlError),
    Config(ConfigError),
    Rpm(RPMError),
}

impl StdErr for Error {
    fn source(&self) -> Option<&(dyn StdErr + 'static)> {
        match self {
            Error::CargoToml(err) => Some(err),
            Error::Config(err) => Some(err),
            Error::Rpm(err) => Some(err),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::CargoToml(ref err) => fmt::Display::fmt(&err, f),
            Error::Config(ref err) => fmt::Display::fmt(&err, f),
            Error::Rpm(ref err) => fmt::Display::fmt(&err, f),
        }
    }
}

impl From<CargoTomlError> for Error {
    fn from(err: CargoTomlError) -> Self {
        Error::CargoToml(err)
    }
}

impl From<ConfigError> for Error {
    fn from(err: ConfigError) -> Self {
        Error::Config(err)
    }
}

impl From<RPMError> for Error {
    fn from(err: RPMError) -> Self {
        Error::Rpm(err)
    }
}
