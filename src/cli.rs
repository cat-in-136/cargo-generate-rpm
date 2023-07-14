use clap::{builder::PossibleValue, Parser, ValueEnum};
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
    #[arg(long)]
    pub auto_req: Option<AutoReqMode>,

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

#[derive(Clone, Debug)]
pub enum AutoReqMode {
    Disabled,
    Builtin,
    FindRequires,
    Script(String),
}

static AUTO_REQ_VARIANTS: &[AutoReqMode] = &[
    AutoReqMode::Disabled,
    AutoReqMode::Builtin,
    AutoReqMode::FindRequires,
    AutoReqMode::Script(String::new()),
];

impl ValueEnum for AutoReqMode {
    fn value_variants<'a>() -> &'a [Self] {
        AUTO_REQ_VARIANTS
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        use AutoReqMode::*;

        let val = match self {
            Disabled => {
                PossibleValue::new("disabled").help("Disable automatic discovery of dependencies")
            }
            Builtin => {
                PossibleValue::new("builtin").help("Use the builtin procedure based on ldd.")
            }
            FindRequires => PossibleValue::new("find-requires")
                .help("Use the external program specified in RPM_FIND_REQUIRES."),
            _ => PossibleValue::new("/path/to/find-requires")
                .help("Use the specified external program."),
        };
        Some(val)
    }

    // Provided method
    fn from_str(input: &str, ignore_case: bool) -> Result<Self, String> {
        let lowercase = String::from(input).to_lowercase();
        let val = if ignore_case { &lowercase } else { input };
        Ok(match val {
            "disabled" => Self::Disabled,
            "builtin" => Self::Builtin,
            "find-requires" => Self::FindRequires,
            _ => Self::Script(input.into()),
        })
    }
}
