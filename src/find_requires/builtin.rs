use elf::types::{Class, Machine, ELFCLASS64, EM_FAKE_ALPHA, SHT_GNU_HASH, SHT_HASH};
use elf::ParseError;
use std::io::Error as IoError;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug)]
struct ElfInfo {
    machine: (Class, Machine),
    got_hash: bool,
    got_gnu_hash: bool,
}

impl ElfInfo {
    fn new<P: AsRef<Path>>(path: P) -> Result<Self, ParseError> {
        let file = elf::File::open_path(path)?;

        let machine = (file.ehdr.class, file.ehdr.machine);
        let got_hash = file.sections.iter().any(|s| s.shdr.shtype == SHT_HASH);
        let got_gnu_hash = file.sections.iter().any(|s| s.shdr.shtype == SHT_GNU_HASH);

        Ok(Self {
            machine,
            got_hash,
            got_gnu_hash,
        })
    }

    pub fn marker(&self) -> Option<&'static str> {
        let (class, machine) = self.machine;
        if class == ELFCLASS64 {
            match machine {
                Machine(0x9026) | EM_FAKE_ALPHA => None, // alpha doesn't traditionally have 64bit markers
                _ => Some("(64bit)"),
            }
        } else {
            None
        }
    }
}

#[test]
fn test_elf_info_new() {
    ElfInfo::new("/bin/sh").unwrap();
}

fn find_requires_by_ldd(path: &Path, marker: Option<&str>) -> Vec<String> {
    fn skip_so_name(so_name: &str) -> bool {
        so_name.contains(".so")
            && (so_name.starts_with("ld.")
                || so_name.starts_with("ld-")
                || so_name.starts_with("ld64.")
                || so_name.starts_with("ld64-")
                || so_name.starts_with("lib"))
    };

    let process = Command::new("ldd")
        .arg("-v")
        .arg(path.as_os_str())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let mut s = String::new();
    process.stdout.unwrap().read_to_string(&mut s).unwrap();

    let libraries = s
        .split("\n")
        .take_while(|&line| !line.trim().is_empty())
        .filter_map(|line| line.trim_start().splitn(2, " ").nth(0))
        .filter(|&line| skip_so_name(line));
    let versioned_libraries = s
        .split("\n")
        .skip_while(|&line| !line.contains("Version information:"))
        .skip(1)
        .skip_while(|&line| !line.contains(path.to_str().unwrap()))
        .skip(1)
        .take_while(|&line| line.contains(" => "))
        .filter_map(|line| line.trim_start().splitn(2, " => ").nth(0))
        .filter(|&name| skip_so_name(name))
        .collect::<Vec<_>>();

    let mut requires = Vec::new();
    for name in libraries {
        requires.push(format!("{}(){}", name, marker.unwrap_or_default()));
    }
    for name in versioned_libraries {
        if name.contains(" (") {
            let without_version = format!(
                "{}(){}",
                name.splitn(2, " ").nth(0).unwrap(),
                marker.unwrap_or_default()
            );
            if !requires.contains(&without_version) {
                requires.push(without_version);
            }
        }
        requires.push(format!(
            "{}{}",
            name.replace(" ", ""),
            marker.unwrap_or_default()
        ))
    }
    requires.sort();
    requires
}

fn find_requires_of_elf(path: &Path) -> Option<Vec<String>> {
    ElfInfo::new(&path).ok().and_then(|info| {
        let mut requires = find_requires_by_ldd(&path, info.marker());
        if info.got_gnu_hash && !info.got_hash {
            requires.push("rtld(GNU_HASH)".to_string());
        }
        Some(requires)
    })
}

#[test]
fn test_find_requires_of_elf() {
    let requires = find_requires_of_elf(Path::new("/bin/sh")).unwrap();
    assert!(requires
        .iter()
        .all(|v| v.contains(".so") || v == "rtld(GNU_HASH)"));
}

fn find_requires_of_shebang(path: &Path) -> Option<Vec<String>> {
    (|path: &Path| -> Result<Option<Vec<String>>, std::io::Error> {
        let mut file = std::fs::File::open(path)?;
        let mut shebang = vec![0u8; 2];
        file.read_exact(&mut shebang)?;
        let interpreter = if shebang == [b'#', b'!'] {
            let mut read = BufReader::new(file);
            let mut line = String::new();
            read.read_line(&mut line)?;
            line.trim()
                .splitn(2, |c: char| !c.is_ascii() || c.is_whitespace())
                .nth(0)
                .map(&String::from)
        } else {
            None
        };

        Ok(match interpreter {
            Some(i) if Path::new(&i).exists() => Some(vec![i.to_string()]),
            _ => None,
        })
    })(path)
    .unwrap_or_default()
}

#[test]
fn test_find_requires_of_shebang() {
    find_requires_of_shebang(Path::new("/usr/bin/ldd")).unwrap();
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path)
        .and_then(|metadata| Ok(metadata.mode()))
        .and_then(|mode| Ok(mode & 0o111 != 0))
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
pub(super) fn find_requires<P: AsRef<Path>>(path: &[P]) -> Result<Vec<String>, IoError> {
    let mut requires = Vec::new();
    for p in path {
        if is_executable(p.as_ref()) {
            if let Some(mut vec) = find_requires_of_elf(p.as_ref()) {
                requires.append(&mut vec)
            } else if let Some(mut vec) = find_requires_of_shebang(p.as_ref()) {
                requires.append(&mut vec)
            }
        }
    }
    Ok(requires)
}
