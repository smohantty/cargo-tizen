# Commands

Built-in help is the fastest reference:

```bash
cargo tizen --help
cargo tizen <command> --help
```

## Summary

| Command | Purpose |
|---|---|
| `init` | Create project config and optional packaging scaffolds |
| `doctor` | Check SDK, toolchain, packaging, and sysroot readiness |
| `fix` | Install missing Rust targets and prepare missing sysroots |
| `build` | Cross-build the current Rust project for Tizen |
| `rpm` | Package the project as an RPM using an authored spec file |
| `tpk` | Package the project as a signed TPK using the Tizen CLI |
| `install` | Build or reuse a TPK and install it on a device |
| `devices` | List devices discovered through `sdb` |
| `setup` | Prepare and cache a sysroot explicitly |
| `clean` | Remove generated build outputs and/or cached sysroots |
| `config` | View or update persistent user settings |
| `gh-release` | Build RPMs and publish a GitHub release |

## `init`

Use:

```bash
cargo tizen init [--rpm] [--tpk] [-p <package>] [--force]
```

Current behavior:

- always writes `.cargo-tizen.toml` when it is missing
- with no format flags, creates config only
- `--rpm` writes `<packaging-dir>/rpm/<package-name>.spec`
- `--tpk` writes `<packaging-dir>/tpk/tizen-manifest.xml`
- existing scaffold files are skipped unless `--force` is passed

## `doctor`

Use:

```bash
cargo tizen doctor [-A <armv7l|aarch64>]
```

Checks:

- host tools such as `cargo`, `rustc`, `rustup`, and optional `rpmbuild`
- Tizen SDK discovery and CLI location
- packaging file presence
- rootstrap coverage when `provider=rootstrap`
- linker, Rust target, rootstrap selection, sysroot cache, and C compiler sanity

## `fix`

Use:

```bash
cargo tizen fix [-A <armv7l|aarch64>]
```

Current behavior:

- installs missing Rust targets through `rustup target add`
- prepares missing sysroots by calling the normal setup flow
- warns about missing `rpmbuild` but does not install host packages

## `build`

Use:

```bash
cargo tizen build [-A <armv7l|aarch64>] [--release] [--target-dir <path>] [-- <cargo args>...]
```

Current behavior:

- resolves the target architecture, sysroot, toolchain, and Rust target
- auto-prepares the sysroot when it is missing
- writes Cargo outputs under `target/tizen/<arch>/cargo` by default
- forwards extra Cargo arguments after `--`

## `rpm`

Use:

```bash
cargo tizen rpm [-A <armv7l|aarch64>] [-p <package>] [--release] [--packaging-dir <path>] [--output <dir>] [--no-build]
```

Current behavior:

- requires an authored spec at `<packaging-dir>/rpm/<package-name>.spec`
- uses `.cargo-tizen.toml [package].packages` for multi-package RPMs when present
- uses `-p` as a single-package override even when config lists multiple packages
- stages built binaries under `target/tizen/<arch>/<profile>/stage/usr/bin`
- copies extra files from `<packaging-dir>/rpm/sources/` into `rpmbuild/SOURCES/`

The packaged binary name must currently match each selected package name.

## `tpk`

Use:

```bash
cargo tizen tpk [-A <armv7l|aarch64>] [-p <package>] [--release] [--no-build] [--packaging-dir <path>] [--output <dir>] [--sign <profile>]
```

Current behavior:

- requires an authored manifest at `<packaging-dir>/tpk/tizen-manifest.xml`
- creates a temporary native Tizen project, injects the Rust binary, and runs `tizen package`
- uses `--sign` first, then `[tpk].sign`, then the Tizen CLI default profile selection
- supports optional `<packaging-dir>/tpk/reference/` and `<packaging-dir>/tpk/extra/`

Current limitation:

- the source binary still has to exist at `<target-dir>/<rust-target>/<profile>/<package-name>`

## `install`

Use:

```bash
cargo tizen install [-A <armv7l|aarch64>] [-p <package>] [-d <device-id>] [--release] [--no-build] [--packaging-dir <path>] [--output <dir>] [--sign <profile>] [--tpk <path>]
```

Current behavior:

- resolves a ready Tizen device through `sdb`
- if `--tpk` is provided, installs that file directly
- otherwise packages a TPK first and installs the first generated artifact
- when `--arch` is omitted, prefers the target device architecture when available

`install` is TPK-only.

## `devices`

Use:

```bash
cargo tizen devices [--all]
```

Current behavior:

- runs `sdb devices`
- enriches ready devices with capability queries when possible
- by default prints only ready Tizen devices
- `--all` includes offline, unauthorized, and non-Tizen entries

## `setup`

Use:

```bash
cargo tizen setup [-A <armv7l|aarch64>] [--profile <name>] [--platform-version <ver>] [--provider <rootstrap|repo>] [--sdk-root <path>] [--force]
```

Current behavior:

- explicitly prepares a sysroot cache entry
- is optional for normal development because build and packaging commands auto-prepare missing sysroots
- stores cache entries under `<cache>/<profile>/<platform-version>/<arch>/<provider>/<fingerprint>/`

Provider status:

- `rootstrap`: working path
- `repo`: placeholder that fails explicitly

## `clean`

Use:

```bash
cargo tizen clean [--build] [--sysroot] [--all] [-A <armv7l|aarch64>]
```

Current behavior:

- with no flags, behaves like `--build`
- `--build` removes `target/tizen` outputs and generated per-arch packaging artifacts
- `--sysroot` removes cached sysroots
- `--all` removes both

## `config`

Use:

```bash
cargo tizen config [--show] [--sign <profile>]
```

Current behavior:

- `--sign` updates user config
- `--sign ""` clears the stored signing profile
- with no `--sign`, the command prints the current merged configuration

## `gh-release`

Use:

```bash
cargo tizen gh-release [-A <armv7l|aarch64>...] [--bump <major|minor|patch>] [--dry-run] [--yes]
```

Current behavior:

- reads project config only
- requires `[package].name` and `[package].packages`
- defaults release architectures from `[release].arches` or both supported arches
- validates a clean working tree, branch `main`, remote `origin`, and authenticated `gh`
- builds release binaries, packages RPMs with `--no-build`, stages them into `<packaging-dir>/rpm/sources/`, replaces previously staged RPMs for the same output package+arch only, syncs the spec `Version:` field, commits artifacts, tags, pushes, and creates or updates the GitHub release
- uploads RPM and `.sha256` sidecar assets

Current limitation:

- all configured release packages must resolve to one shared version source
