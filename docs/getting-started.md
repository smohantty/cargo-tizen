# Getting Started

This guide assumes a Linux host and an existing Rust project.

## 1. Install prerequisites

Follow [install-linux.md](install-linux.md) first. The short version is:

- install Rust and `rustup`
- install cross linkers for `armv7l` and/or `aarch64`
- install Tizen Studio and the rootstraps you plan to target
- install `rpm` or `rpm-build` if you need RPM packaging

## 2. Initialize project files

Create project config only:

```bash
cargo tizen init
```

Create RPM or TPK scaffolding:

```bash
cargo tizen init --rpm
cargo tizen init --tpk
cargo tizen init --rpm --tpk
```

With no format flags, `init` writes `.cargo-tizen.toml` only.

## 3. Verify the environment

```bash
cargo tizen doctor
cargo tizen fix
```

- `doctor` reports SDK discovery, host tools, rootstrap coverage, packaging files, rust targets, and sysroot readiness.
- `fix` installs missing Rust targets and prepares missing sysroots. It does not install host packages such as Tizen Studio, cross compilers, or `rpmbuild`.

## 4. Build

```bash
cargo tizen build -A armv7l --release
```

If `-A` is omitted, `cargo-tizen` tries:

1. `[default].arch`
2. the only configured `[arch.*]` entry
3. the architecture of the only connected ready Tizen device
4. otherwise it fails and asks you to choose

## 5. Package as RPM

Generate the scaffold once:

```bash
cargo tizen init --rpm
```

Edit `tizen/rpm/<package-name>.spec`, then package:

```bash
cargo tizen rpm -A armv7l --release
```

For a workspace or multi-package RPM, define the packaging group in `.cargo-tizen.toml`:

```toml
[package]
name = "my-suite"
packages = ["server", "cli"]
```

- `name` controls RPM spec lookup.
- `packages` controls which crates are built and staged into the RPM.

## 6. Package as TPK

Generate the scaffold once:

```bash
cargo tizen init --tpk
```

Edit `tizen/tpk/tizen-manifest.xml`, then package:

```bash
cargo tizen tpk -A armv7l --release --sign my_profile
```

You can also store a default signing profile:

```bash
cargo tizen config --sign my_profile
```

If neither `--sign` nor stored config is set, the Tizen CLI default profile selection is used.

## 7. Install on a device

List devices:

```bash
cargo tizen devices
```

Install a TPK:

```bash
cargo tizen install -A armv7l --release -d <device-id>
```

Notes:

- `install` is TPK-only.
- If `--tpk` is omitted, `install` packages a TPK first.
- If `--arch` is omitted, `install` prefers the target device architecture when it can detect one.

## 8. Preview the GitHub RPM release flow

```bash
cargo tizen gh-release --dry-run
```

`gh-release` requires:

- a clean working tree
- current branch `main`
- remote `origin`
- authenticated `gh`
- project config with `[package].name` and `[package].packages`
- one shared release version across the selected crates, whether it comes from crate-local `version = "..."` fields, `[workspace.package].version`, or both

When it stages release RPMs into `rpm/sources/`, it only replaces older RPMs for the same output package+arch. Other authored files there stay intact.

See [commands.md](commands.md) and [architecture.md](architecture.md) for the exact contract.
