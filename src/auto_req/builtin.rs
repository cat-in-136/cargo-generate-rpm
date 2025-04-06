use crate::error::AutoReqError;
use elf::abi::{EM_ALPHA, SHT_GNU_HASH, SHT_HASH};
use elf::endian::AnyEndian;
use elf::file::{Class, FileHeader};
use elf::{ElfStream, ParseError};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug)]
struct ElfInfo {
    machine: (Class, u16),
    got_hash: bool,
    got_gnu_hash: bool,
}

impl ElfInfo {
    fn new<P: AsRef<Path>>(path: P) -> Result<Self, ParseError> {
        let file = File::open(path)?;
        let elf_stream = ElfStream::open_stream(file)?;
        let ehdr: FileHeader<AnyEndian> = elf_stream.ehdr;
        let shdrs = elf_stream.section_headers();

        let machine = (ehdr.class, ehdr.e_machine);
        let got_hash = shdrs.iter().any(|s| s.sh_type == SHT_HASH);
        let got_gnu_hash = shdrs.iter().any(|s| s.sh_type == SHT_GNU_HASH);

        Ok(Self {
            machine,
            got_hash,
            got_gnu_hash,
        })
    }

    pub fn marker(&self) -> Option<&'static str> {
        match self.machine {
            (Class::ELF64, EM_ALPHA) | (Class::ELF64, 0x9026) => None, // alpha doesn't traditionally have 64bit markers
            (Class::ELF64, _) => Some("(64bit)"),
            (Class::ELF32, _) => None,
        }
    }
}

#[test]
fn test_elf_info_new() {
    ElfInfo::new("/bin/sh").unwrap();
}

fn find_requires_by_ldd(
    path: &Path,
    marker: Option<&str>,
) -> Result<BTreeSet<String>, AutoReqError> {
    fn skip_so_name(so_name: &str) -> bool {
        so_name.contains(".so")
            && (so_name.starts_with("ld.")
                || so_name.starts_with("ld-")
                || so_name.starts_with("ld64.")
                || so_name.starts_with("ld64-")
                || so_name.starts_with("lib"))
    }

    let process = Command::new("ldd")
        .arg("-v")
        .arg(path.as_os_str())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| AutoReqError::ProcessError(OsString::from("ldd"), e))?;

    let mut s = String::new();
    process
        .stdout
        .unwrap()
        .read_to_string(&mut s)
        .map_err(|e| AutoReqError::ProcessError(OsString::from("ldd"), e))?;

    let unversioned_libraries = s
        .split('\n')
        .take_while(|&line| !line.trim().is_empty())
        .filter_map(|line| line.trim_start().split(' ').next());
    let versioned_libraries = s
        .split('\n')
        .skip_while(|&line| !line.contains("Version information:"))
        .skip(1)
        .skip_while(|&line| !line.contains(path.to_str().unwrap()))
        .skip(1)
        .take_while(|&line| line.contains(" => "))
        .filter_map(|line| line.trim_start().split(" => ").next());

    let marker = marker.unwrap_or_default();
    let mut requires = BTreeSet::new();
    for name in unversioned_libraries
        .into_iter()
        .chain(versioned_libraries.into_iter())
        .filter(|&name| skip_so_name(name))
    {
        if name.contains(" (") {
            // Insert "unversioned" library name
            requires.insert(format!("{}(){}", name.split(' ').next().unwrap(), marker));
            requires.insert(format!("{}{}", name.replace(' ', ""), marker));
        } else {
            requires.insert(format!("{}(){}", name.replace(' ', ""), marker));
        }
    }
    Ok(requires)
}

fn find_requires_of_elf(path: &Path) -> Result<Option<BTreeSet<String>>, AutoReqError> {
    if let Ok(info) = ElfInfo::new(path) {
        let mut requires = find_requires_by_ldd(path, info.marker())?;
        if info.got_gnu_hash && !info.got_hash {
            requires.insert("rtld(GNU_HASH)".to_string());
        }
        Ok(Some(requires))
    } else {
        Ok(None)
    }
}

#[test]
fn test_find_requires_of_elf() {
    let requires = find_requires_of_elf(Path::new("/bin/sh")).unwrap().unwrap();
    assert!(requires
        .iter()
        .all(|v| v.contains(".so") || v == "rtld(GNU_HASH)"));
    assert!(matches!(find_requires_of_elf(Path::new(file!())), Ok(None)));
}

fn find_require_of_shebang(path: &Path) -> Result<Option<String>, AutoReqError> {
    let interpreter = {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(_) => return Ok(None),
        };
        let mut read = BufReader::new(file);
        let mut shebang = [0u8; 2];
        let shebang_size = match read.read(&mut shebang) {
            Ok(0) => return Ok(None),
            Ok(size) => size,
            Err(_) => return Ok(None),
        };
        if shebang_size == 2 || shebang == [b'#', b'!'] {
            let mut line = String::new();
            match read.read_line(&mut line) {
                Ok(_) => {}
                Err(_) => return Ok(None),
            };
            line.trim()
                .split(|c: char| !c.is_ascii() || c.is_whitespace())
                .next()
                .map(String::from)
        } else {
            None
        }
    };

    Ok(match interpreter {
        Some(i) if Path::new(&i).exists() => Some(i),
        _ => None,
    })
}

#[test]
fn test_find_require_of_shebang() {
    assert!(matches!(
        find_require_of_shebang(Path::new("/usr/bin/ldd")),
        Ok(Some(_))
    ));
    assert!(matches!(
        find_require_of_shebang(Path::new(file!())),
        Ok(None)
    ));
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path)
        .map(|metadata| metadata.mode())
        .map(|mode| mode & 0o111 != 0)
        .unwrap_or_default()
}

#[cfg(unix)]
#[test]
fn test_is_executable() {
    assert!(is_executable(Path::new("/bin/sh")));
    assert!(!is_executable(Path::new(file!())));
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    true
}

/// find requires.
pub(super) fn find_requires<P: AsRef<Path>>(path: &[P]) -> Result<Vec<String>, AutoReqError> {
    let mut requires = Vec::new();
    for p in path.iter().map(|v| v.as_ref()) {
        if is_executable(p) {
            if let Some(elf_requires) = find_requires_of_elf(p)? {
                requires.extend(elf_requires);
            } else if let Some(shebang_require) = find_require_of_shebang(p)? {
                requires.push(shebang_require);
            }
        }
    }
    Ok(requires)
}
