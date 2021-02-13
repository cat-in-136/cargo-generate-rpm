use std::ffi::OsStr;
use std::io::Error as IoError;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

/// find requires using `find-requires` program located at `script_path`.
pub(super) fn find_requires<P: AsRef<Path>, S: AsRef<OsStr>>(
    path: &[P],
    script_path: S,
) -> Result<Vec<String>, IoError> {
    let process = Command::new(script_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let filenames = path
        .iter()
        .filter_map(|v| v.as_ref().to_str())
        .collect::<Vec<_>>()
        .join("\n");
    process.stdin.unwrap().write_all(filenames.as_bytes())?;

    let mut requires = String::new();
    process.stdout.unwrap().read_to_string(&mut requires)?;

    Ok(requires.trim().split("\n").map(&String::from).collect())
}
