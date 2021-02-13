use std::io::Error as IoError;
use std::path::{Iter, Path, PathBuf};

mod script;

const RPM_FIND_REQUIRES: &str = "/usr/lib/rpm/find-requires";

/// The method to find requires
#[derive(Debug)]
pub enum FindRequiresMode {
    /// Automatically selected
    Auto,
    /// `find-requires` script
    Script(PathBuf),
}

/// Find requires
pub fn find_requires<P: AsRef<Path>>(
    files: &[P],
    mode: FindRequiresMode,
) -> Result<Vec<String>, IoError> {
    match mode {
        FindRequiresMode::Auto => {
            if Path::new(RPM_FIND_REQUIRES).exists() {
                script::find_requires(files, RPM_FIND_REQUIRES)
            } else {
                Ok(Default::default())
            }
        }
        FindRequiresMode::Script(script_path) => {
            script::find_requires(files, script_path.as_path())
        }
    }
}
