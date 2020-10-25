# cargo-generate-rpm

[Cargo](https://doc.rust-lang.org/cargo/) helper command to generate a binary [RPM package](https://rpm.org/) (.rpm) from Cargo project.

This command does not depend on `rpmbuild` and generates an RPM package file without a spec file by using [rpm-rs](https://crates.io/crates/rpm-rs).

![Rust](https://github.com/cat-in-136/cargo-generate-rpm/workflows/Rust/badge.svg)
[![cargo-generate-rpm at crates.io](https://img.shields.io/crates/v/cargo-generate-rpm.svg)](https://crates.io/crates/cargo-generate-rpm)

## Install

```sh
cargo install cargo-generate-rpm
```

## Usage

```sh
cargo build --release
strip -s target/release/XXX
cargo generate-rpm
```

Upon run `cargo generate-rpm` on your cargo project, a binary RPM package file will be created in `target/generate-rpm/XXX.rpm`.
You can change the RPM package file location using `-o` option.

In advance, run `cargo run --release` and strip the debug symbols (`strip -s target/release/XXX`), because these are not run upon `cargo generate-rpm` as of now.

## Configuration

This command obtains RPM metadata from [the `Cargo.toml` file](https://doc.rust-lang.org/cargo/reference/manifest.html):

### `[package.metadata.generate-rpm]` options

* name: the package name. If not present, `package.name` is used.
* version: the package version. If not present, `package.version` is used.
* license: the package license. If not present, `package.license` is used.
* summary: the package summary/description. If not present, `package.description` is used.
* assets: the array of the files to be included in the package
  * source: the location of that asset in the Rust project. (e.g. `target/release/XXX`)
  * dest: the install-destination. (e.g. `/usr/bin/XXX`)
  * mode: the permissions as octal string. (e.g. `755` to indicate `-rwxr-xr-x`)
  * config: set true if it is a configuration file.
  * doc: set true if it is a document file.
* release: optional string of release.
* epoch: optional number of epoch.
* pre_install_script: optional string of pre_install_script.
* pre_uninstall_script: optional string of pre_uninstall_script.
* post_install_script: optional string of post_install_script.
* post_uninstall_script: optional string of post_uninstall_script.
