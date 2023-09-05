# cargo-generate-rpm

[Cargo](https://doc.rust-lang.org/cargo/) helper command to generate a binary [RPM package](https://rpm.org/) (.rpm)
from Cargo project.

This command does not depend on `rpmbuild` and generates an RPM package file without a spec file by
using the [`rpm`](https://crates.io/crates/rpm) crate.

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

Upon run `cargo generate-rpm` on your cargo project, a binary RPM package file will be created
in `target/generate-rpm/XXX.rpm`.
You can change the RPM package file location using `-o` option.

In advance, run `cargo run --release` and strip the debug symbols (`strip -s target/release/XXX`), because these are not
run upon `cargo generate-rpm` as of now.

## Configuration

This command generates RPM metadata
from [the `Cargo.toml` file](https://doc.rust-lang.org/cargo/reference/manifest.html):

### `[package.metadata.generate-rpm]` options

* name: the package name. If not present, `package.name` is used.
* version: the package version. If not present, `package.version` is used.
* license: the package license. If not present, `package.license` is used.
* summary: the package summary/description. If not present, `package.description` is used.
* url: the package homepage url. If not present, `package.homepage` is used. If neither present, `package.repository` is
  used.
* assets: (**mandatory**) the array of the files to be included in the package
    * source: the location of that asset in the Rust project. (e.g. `target/release/XXX`)
      Wildcard character `*` is allowed.
    * dest: the install-destination. (e.g. `/usr/bin/XXX`) It shall be a file path or a directory path ending `/`.
      If source contains wildcard character `*`, it must be a directory, not a file path.
    * mode: the permissions as octal string. (e.g. `755` to indicate `-rwxr-xr-x`)
    * config: set true if it is a configuration file.
    * doc: set true if it is a document file.
    * user: the owner of the file.
    * group: the group owner of the file.
* release: optional string of release.
* epoch: optional number of epoch.
* pre_install_script: optional string or file path of pre_install_script.
* pre_uninstall_script: optional string or file path of pre_uninstall_script.
* post_install_script: optional string or file path of post_install_script.
* post_uninstall_script: optional string or file path of post_uninstall_script.
* requires: optional list of Requires
* auto-req: optional string `"no"` to disable the automatic dependency process
* require-sh: optional boolean `false` to omit `/bin/sh` from Requirements
* obsoletes: optional list of Obsoletes
* conflicts: optional list of Conflicts
* provides: optional list of Provides
* vendor: optional string of Vendor

Adding assets such as the binary file, ``.desktop`` file, or icons, shall be written in the following way.

```toml
[package.metadata.generate-rpm]
assets = [
    { source = "target/release/XXX", dest = "/usr/bin/XXX", mode = "755" },
    { source = "<path_relative_to_project_root>/XXX.desktop", dest = "/usr/share/applications/XXX.desktop", mode = "644" },
    { source = "<path_relative_to_project_root>/*/apps/XXX.png", dest = "/usr/share/icons/hicolor/", mode = "644" },
]
```

### `[package.metadata.generate-rpm.{requires,obsoletes,conflicts,provides}]` options

Dependencies such as "requires", "obsoletes", "conflicts", and "provides" shall be written in similar way as
dependencies in Cargo.toml.

```toml
[package.metadata.generate-rpm.requires]
alternative = "*"
filesystem = ">= 3"
```

This example states that the package requires with any versions of `alternative` and all versions of `filesystem` 3.0 or
higher.

Following table lists the version comparisons:

| Comparison               | Meaning                                                          |
|--------------------------|------------------------------------------------------------------|
| `package = "*"`          | A package at any version number                                  |
| `package = "< version"`  | A package with a version number less than version                |
| `package = "<= version"` | A package with a version number less than or equal to version    |
| `package = "= version"`  | A package with a version number equal to version                 |
| `package = "> version"`  | A package with a version number greater than version             |
| `package = ">= version"` | A package with a version number greater than or equal to version |

It is necessary to place a space between version and symbols such as `<`, `<=`, etc...
`package = "version"` is not accepted, instead use `package = "= version"`.

This command automatically determines what shared libraries a package requires.
There may be times when the automatic dependency processing is not desired.
In this case, the package author may set `package.metadata.generate-rpm.auto-req` to `"no"` or
the user who executes this command may specify command line option `--auto-req no`.

* `--auto-req auto`: The following rules are used to determine the preferred automatic dependency process:
    * If `package.metadata.generate-rpm.auto-req` set to `"no"` or `"disabled"`, the process is disabled.
    * If `/usr/lib/rpm/find-requires` exists, it is used (same behaviour as `--auto-req /usr/lib/rpm/find-requires`).
    * Otherwise, builtin procedure is used (same behaviour as `--auto-req builtin`).
* `--auto-req builtin`: the builtin procedure using `ldd` is used.
* `--auto-req /path/to/find-requires`: the specified external program is used. This behavior is the same as the
  original `rpmbuild`.
* `--auto-req no`: the process is disabled.

`/bin/sh` is always added to the package requirements. To disable it, set `package.metadata.generate-rpm.require-sh`
to `false`. You should not do this if you use scripts such as `pre_install_script` or if your assets contain shell
scripts.

### Overwrite configuration

`[package.metadata.generate-rpm]` can be overwritten. The following command line options are used:

* `--metadata-overwrite=TOML_FILE.toml` : Overwrite the `[package.metadata.generate-rpm]` options with the contents of
  the specified TOML file.
* `--metadata-overwrite=TOML_FILE.toml#TOML.PATH` : Overwrites the `[package.metadata.generate-rpm]` options with the
  table specified in the TOML path of the TOML file.
  Only a sequence of bare keys connected by dots is acceptable for the TOML path.
  Path containing quoted keys (such as `metadata."παραλλαγή"`) cannot be acceptable.
* `-s 'toml "text"'` or `--set-metadata='toml "text"'` : Overwrite the `[package.metadata.generate-rpm]` options with
  inline TOML text.
  The argument text --- inline TOML text must be enclosed in quotation marks since it contains spaces.
* `--variant=VARIANT` : Overwrites the `[package.metadata.generate-rpm]` options with the table specified
  in `[package.metadata.generate-rpm.variants.VARIANT]` of the TOML file.
  It is a shortcut to `--metadata-overwrite=path/to/Cargo.toml#package.metadata.generate-rpm.variants.VARIANT`.
  It is intended for providing multiple variants of the metadata in a Cargo.toml and ability for the users to select the
  variant using --variant=name option.

These options can be specified more than once, with the last written one specified being applied.
For example, the arguments -s 'release = "alpha"' `--metadata-overwrite=beta.toml` where beta.toml
contains `release = "beta"` gives `release = "beta"`.

## Advanced Usage

### Workspace

To generate an RPM package from a member of a workspace, execute `cargo generate-rpm` in the workspace directory
with specifying the package (directory path) with option `-p`:

```sh
cargo build --release
strip -s target/release/XXX
cargo generate-rpm -p XXX
```

`[package.metadata.generate-rpm]` options should be written in `XXX/Cargo.toml`.

When the option `-p` specified, first, the asset file `source` shall be treated as a relative path from the current
directory.
If not found, it shall be treated as a relative path from the directory of the package.
If both not found, `cargo generate-rpm` shall fail with an error.

For example, `source = target/bin/XXX` would usually be treated as a relative path from the current directory.
Because all packages in the workspace share a common output directory that is located `target` in workspace directory.

### Cross compilation

This command supports `--target-dir`, `--target`, and `--profile` options like `cargo build`.
Depending on these options, this command changes the RPM package file location and replaces `target/release/` of
the source locations of the assets.

```sh
cargo build --release --target x86_64-unknown-linux-gnu
cargo generate-rpm --target x86_64-unknown-linux-gnu
```

When `--target-dir TARGET-DIR` and `--target x86_64-unknown-linux-gnu` are specified, a binary RPM file will be created
at `TARGET-DIR/x86_64-unknown-linux-gnu/generate-rpm/XXX.rpm` instead of `target/generate-rpm/XXX.rpm`.
In this case, the source of the asset `{ source = "target/release/XXX", dest = "/usr/bin/XXX" }` will be treated as
`TARGET-DIR/x86_64-unknown-linux-gnu/release/XXX` instead of `target/release/XXX`.

You can use `CARGO_BUILD_TARGET` environment variable instead of `--target` option and `CARGO_BUILD_TARGET_DIR` or
`CARGO_TARGET_DIR` instead of `--target-dir`.

Similarly, if using a custom build profile with, for example, `--profile custom` the source of the asset
`{ source = "target/release/XXX" }` will be treated as `target/custom/XXX`.

### Payload compress type

The default payload compress type of the generated RPM file is zstd.
You can specify the payload compress type with `--payload-compress TYPE`: none, gzip, or zstd.

For the legacy system (e.g. centos7), specify legacy compress type explicitly e.g. `--payload-compress none`.

