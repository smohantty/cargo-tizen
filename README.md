# cargo-tizen

`cargo-tizen` is a Cargo subcommand for preparing Tizen sysroots, cross-building Rust binaries, packaging them as RPM or TPK, installing TPKs on devices, and publishing RPM GitHub releases.

```bash
cargo tizen init
cargo tizen doctor
cargo tizen fix
cargo tizen build -A armv7l --release
cargo tizen rpm -A armv7l --release
cargo tizen tpk -A armv7l --release
cargo tizen install -A armv7l --release
cargo tizen gh-release --dry-run
```

## Current State

- Supported target architectures are `armv7l` and `aarch64`.
- The working sysroot path is the `rootstrap` provider.
- The `repo` provider exists only as an explicit not-yet-implemented failure path.
- `install` is TPK-only.
- `gh-release` is RPM-only.
- `gh-release` only replaces previously staged RPMs in `rpm/sources/` when they match the same output package and arch.
- Packaging currently expects the built binary name to match the selected package name.

## Install

From a local clone:

```bash
git clone <repo-url> cargo-tizen
cd cargo-tizen
cargo install --path .
```

Verify:

```bash
cargo tizen --help
```

## Fast Path

1. Follow the documented Linux setup in [docs/install-linux.md](docs/install-linux.md).
2. Initialize project files with `cargo tizen init`, `cargo tizen init --rpm`, and/or `cargo tizen init --tpk`.
3. Validate the host with `cargo tizen doctor` and repair common gaps with `cargo tizen fix`.
4. Build, package, or install with the command that matches your workflow.

## Documentation

- [Docs index](docs/README.md)
- [Getting started](docs/getting-started.md)
- [Linux install](docs/install-linux.md)
- [Configuration](docs/configuration.md)
- [Commands](docs/commands.md)
- [Packaging layout](docs/packaging-layout.md)
- [Architecture](docs/architecture.md)
- [Known gaps](docs/known-gaps.md)
- [Agent guide](docs/agents/index.md)

## Typical Workflows

Create config only:

```bash
cargo tizen init
```

Scaffold RPM or TPK packaging files:

```bash
cargo tizen init --rpm
cargo tizen init --tpk
```

Cross-build:

```bash
cargo tizen build -A armv7l --release
```

Package as RPM:

```bash
cargo tizen rpm -A armv7l --release
```

Package as TPK and install:

```bash
cargo tizen tpk -A armv7l --release --sign my_profile
cargo tizen install -A armv7l --release -d <device-id>
```

Preview the RPM GitHub release flow:

```bash
cargo tizen gh-release --dry-run
```

## Configuration Snapshot

`cargo tizen init` writes a starter `.cargo-tizen.toml` like this:

```toml
[default]
arch = "aarch64"
profile = "mobile"
platform_version = "10.0"

[package]
name = "my-app"
packages = ["my-app"]
```

User config is loaded first and project config overrides it. The full schema and defaults live in [docs/configuration.md](docs/configuration.md).

## Notes

- `cargo tizen setup` is optional for normal development. `build`, `rpm`, `tpk`, and `install` prepare sysroots automatically when needed.
- For `armv7l` with the default `rootstrap` provider, the Rust target is inferred from sysroot headers when possible: soft-float uses `armv7-unknown-linux-gnueabi`, hard-float uses `armv7-unknown-linux-gnueabihf`.
- The documented host path in this repository is Linux.
