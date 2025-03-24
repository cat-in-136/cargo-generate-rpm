use crate::error::AutoReqError;
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};

/// find requires using `find-requires` program located at `script_path`.
pub(super) fn find_requires<P: AsRef<Path>, S: AsRef<OsStr>>(
    path: &[P],
    script_path: S,
) -> Result<Vec<String>, AutoReqError> {
    let process = Command::new(&script_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| AutoReqError::ProcessError(script_path.as_ref().to_os_string(), e))?;

    let filenames = path
        .iter()
        .filter_map(|v| v.as_ref().to_str())
        .collect::<Vec<_>>()
        .join("\n");
    process
        .stdin
        .unwrap()
        .write_all(filenames.as_bytes())
        .map_err(|e| AutoReqError::ProcessError(script_path.as_ref().to_os_string(), e))?;

    let mut requires = Vec::new();
    let reader = BufReader::new(process.stdout.unwrap());

    for line in reader.lines() {
        match line {
            Ok(content) if content == "" => (), // ignore empty line
            Ok(content) => requires.push(content),
            Err(e) => {
                return Err(AutoReqError::ProcessError(
                    script_path.as_ref().to_os_string(),
                    e,
                ))
            }
        }
    }

    Ok(requires)
}

#[test]
fn test_find_requires() {
    assert_eq!(
        find_requires(&[file!()], "/bin/cat").unwrap(),
        vec![file!().to_string()]
    );
    assert!(matches!(
        find_requires(&[file!()], "not-exist"),
        Err(AutoReqError::ProcessError(_, _))
    ));

    // empty dependencies shall return empty vector
    assert!(find_requires(&[file!()], "/bin/false").unwrap().is_empty());
}
