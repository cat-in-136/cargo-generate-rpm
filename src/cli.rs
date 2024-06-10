use clap::{
    builder::{PathBufValueParser, PossibleValuesParser, TypedValueParser, ValueParserFactory},
    Arg, Command, Parser, ValueEnum,
};
use std::ffi::OsStr;
use std::path::PathBuf;

/// Wrapper used when the application is executed as Cargo plugin
#[derive(Debug, Parser)]
#[command(name = "cargo")]
#[command(bin_name = "cargo")]
pub enum CargoWrapper {
    GenerateRpm(Cli),
}

/// Arguments of the command line interface
#[derive(Debug, Parser)]
#[command(name = "cargo-generate-rpm")]
#[command(bin_name = "cargo-generate-rpm")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Target arch of generated package.
    #[arg(short, long)]
    pub arch: Option<String>,

    /// Output file or directory.
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Name of a crate in the workspace for which
    /// RPM package will be generated.
    #[arg(short, long)]
    pub package: Option<String>,

    /// Automatic dependency processing mode.
    #[arg(long, default_value = "auto",
        help = "Automatic dependency processing mode. \
        [possible values: auto, disabled, builtin, find-requires, /path/to/find-requires]",
        long_help = color_print::cstr!("Automatic dependency processing mode.\n\n\
        Possible values:\n\
        - <bold>auto</bold>:                   Use the preferred automatic dependency process.\n\
        - <bold>disabled</bold>:               Disable the discovery of dependencies. [alias: no]\n\
        - <bold>builtin</bold>:                Use the builtin procedure based on ldd.\n\
        - <bold>find-requires</bold>:          Use /usr/lib/rpm/find-requires.\n\
        - <bold>/path/to/find-requires</bold>: Use the specified external program."))]
    pub auto_req: AutoReqMode,

    /// Sub-directory name for all generated artifacts. May be
    /// specified with CARGO_BUILD_TARGET environment
    /// variable.
    #[arg(long)]
    pub target: Option<String>,

    /// Directory for all generated artifacts. May be
    /// specified with CARGO_BUILD_TARGET_DIR or
    /// CARGO_TARGET_DIR environment variables.
    #[arg(long)]
    pub target_dir: Option<String>,

    /// Build profile for packaging.
    #[arg(long, default_value = "release")]
    pub profile: String,

    /// Compression type of package payload.
    #[arg(long, default_value = "zstd")]
    pub payload_compress: Compression,

    /// Timestamp in seconds since the UNIX Epoch for clamping
    /// modification time of included files and package build time.
    ///
    /// This value can also be provided using the SOURCE_DATE_EPOCH
    /// enviroment variable.
    #[arg(long)]
    pub source_date: Option<u32>,

    /// Overwrite metadata with TOML file. If "#dotted.key"
    /// suffixed, load "dotted.key" table instead of the root
    /// table.
    #[arg(long, value_delimiter = ',')]
    pub metadata_overwrite: Vec<String>,

    /// Overwrite metadata with TOML text.
    #[arg(short, long, value_delimiter = ',')]
    pub set_metadata: Vec<String>,

    /// Shortcut to --metadata-overwrite=path/to/Cargo.toml#package.metadata.generate-rpm.variants.VARIANT
    #[arg(long, value_delimiter = ',')]
    pub variant: Vec<String>,
}

impl Default for Cli {
    fn default() -> Self {
        Cli::parse_from([""])
    }
}

#[derive(ValueEnum, Clone, Copy, Debug, Default)]
pub enum Compression {
    None,
    Gzip,
    #[default]
    Zstd,
    Xz,
}

impl From<Compression> for rpm::CompressionWithLevel {
    fn from(val: Compression) -> Self {
        let ct = match val {
            Compression::None => rpm::CompressionType::None,
            Compression::Gzip => rpm::CompressionType::Gzip,
            Compression::Zstd => rpm::CompressionType::Zstd,
            Compression::Xz => rpm::CompressionType::Xz,
        };
        ct.into()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AutoReqMode {
    Auto,
    Disabled,
    Builtin,
    FindRequires,
    Script(PathBuf),
}

impl ValueParserFactory for AutoReqMode {
    type Parser = AutoReqModeParser;

    fn value_parser() -> Self::Parser {
        AutoReqModeParser
    }
}

#[derive(Clone, Debug)]
pub struct AutoReqModeParser;

impl TypedValueParser for AutoReqModeParser {
    type Value = AutoReqMode;
    fn parse_ref(
        &self,
        cmd: &Command,
        arg: Option<&Arg>,
        value: &OsStr,
    ) -> Result<Self::Value, clap::Error> {
        const VALUES: [(&str, AutoReqMode); 5] = [
            ("auto", AutoReqMode::Auto),
            ("disabled", AutoReqMode::Disabled),
            ("no", AutoReqMode::Disabled),
            ("builtin", AutoReqMode::Builtin),
            ("find-requires", AutoReqMode::FindRequires),
        ];

        let inner = PossibleValuesParser::new(VALUES.iter().map(|(k, _v)| k));
        match inner.parse_ref(cmd, arg, value) {
            Ok(name) => Ok(VALUES
                .iter()
                .find(|(k, _v)| name.as_str() == (k.as_ref() as &str))
                .unwrap()
                .1
                .clone()),
            Err(e) if e.kind() == clap::error::ErrorKind::InvalidValue => {
                let inner = PathBufValueParser::new();
                match inner.parse_ref(cmd, arg, value) {
                    Ok(v) => Ok(AutoReqMode::Script(v)),
                    Err(e) => Err(e),
                }
            }
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verify_cli() {
        use clap::CommandFactory;
        Cli::command().debug_assert()
    }

    #[test]
    fn test_metadata_overwrite() {
        let args = Cli::try_parse_from([
            "",
            "--metadata-overwrite",
            "TOML_FILE.toml",
            "--metadata-overwrite",
            "TOML_FILE.toml#TOML.PATH",
        ])
        .unwrap();
        assert_eq!(
            args.metadata_overwrite,
            vec!["TOML_FILE.toml", "TOML_FILE.toml#TOML.PATH"]
        );
    }

    #[test]
    fn test_set_metadata() {
        let args = Cli::try_parse_from([
            "",
            "-s",
            "toml \"text1\"",
            "--set-metadata",
            "toml \"text2\"",
        ])
        .unwrap();
        assert_eq!(args.set_metadata, vec!["toml \"text1\"", "toml \"text2\""]);
    }

    #[test]
    fn test_auto_req() {
        let args = Cli::try_parse_from([""]).unwrap();
        assert_eq!(args.auto_req, AutoReqMode::Auto);
        let args = Cli::try_parse_from(["", "--auto-req", "auto"]).unwrap();
        assert_eq!(args.auto_req, AutoReqMode::Auto);
        let args = Cli::try_parse_from(["", "--auto-req", "builtin"]).unwrap();
        assert_eq!(args.auto_req, AutoReqMode::Builtin);
        let args = Cli::try_parse_from(["", "--auto-req", "find-requires"]).unwrap();
        assert_eq!(args.auto_req, AutoReqMode::FindRequires);
        let args = Cli::try_parse_from(["", "--auto-req", "/usr/lib/rpm/find-requires"]).unwrap();
        assert!(
            matches!(args.auto_req, AutoReqMode::Script(v) if v == PathBuf::from("/usr/lib/rpm/find-requires"))
        );
        let args = Cli::try_parse_from(["", "--auto-req", "no"]).unwrap();
        assert_eq!(args.auto_req, AutoReqMode::Disabled);
    }
}
