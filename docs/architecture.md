# Architecture

This document describes the current implementation, not an aspirational future design.

## Scope

`cargo-tizen` is a Cargo subcommand that:

- prepares and caches Tizen sysroots
- cross-builds Rust binaries for `armv7l` and `aarch64`
- packages outputs as RPM or TPK
- lists devices and installs TPKs through `sdb`
- publishes RPM GitHub releases through `gh-release`

## Runtime Model

At startup the binary:

1. strips the leading `tizen` token when invoked as `cargo tizen ...`
2. parses clap arguments from `src/cli.rs`
3. loads config from user config and then `.cargo-tizen.toml`
4. builds `AppContext`
5. dispatches to the selected command handler

The main dispatch lives in `src/main.rs`.

## Code Map

- CLI definitions: `src/cli.rs`
- Config model and merge order: `src/config.rs`
- Command context: `src/context.rs`
- Architecture mapping and auto-selection: `src/arch.rs`, `src/arch_detect.rs`, `src/rust_target.rs`
- SDK discovery: `src/sdk.rs`
- Toolchain env injection: `src/tool_env.rs`
- Init scaffolding: `src/init_cmd.rs`
- Package selection: `src/package_select.rs`
- Packaging layout checks: `src/packaging.rs`
- Sysroot orchestration and cache: `src/sysroot/*`
- Build runner: `src/cargo_runner.rs`
- RPM pipeline: `src/rpm/*`
- TPK pipeline: `src/tpk.rs`
- Device discovery and install: `src/device.rs`, `src/install_cmd.rs`
- Diagnostics and repair: `src/doctor.rs`, `src/fix.rs`
- Cleanup: `src/clean.rs`
- GitHub release flow: `src/gh_release.rs`

## Stable Command Surface

The current top-level commands are:

- `init`
- `doctor`
- `fix`
- `build`
- `rpm`
- `tpk`
- `install`
- `devices`
- `setup`
- `clean`
- `config`
- `gh-release`

When the command surface changes, update `README.md`, `docs/commands.md`, `docs/getting-started.md`, `docs/configuration.md`, and this file in the same patch.

## Behavioral Contracts

### Config merge order

- User config is loaded first.
- Project config overrides it.
- `gh-release` is intentionally different and reads project config only.

### Architecture mapping

Keep the current per-consumer mapping split:

- Rust target
- Tizen CLI arch
- Tizen `build-native` arch
- RPM build arch

`armv7l` and `aarch64` are the only supported logical architectures.

### Architecture auto-selection

When `--arch` is omitted, the current selection order is:

1. `[default].arch`
2. the only configured `[arch.*]` entry
3. the architecture of the only connected ready Tizen device
4. otherwise fail and ask for `-A`

`install` has one extra rule before the generic fallback: it prefers the resolved target device architecture.

### Sysroot cache

Cache entry layout:

```text
<cache>/<profile>/<platform-version>/<arch>/<provider>/<fingerprint>/
```

Important invariants:

- dotted fingerprint names must survive lock and temp path generation
- `rootstrap` is the working provider
- `repo` exists only as a placeholder that fails explicitly
- `build`, `rpm`, `tpk`, and `install` auto-prepare missing sysroots

### Packaging

Package resolution order is:

1. explicit CLI `-p`
2. `.cargo-tizen.toml [package].packages`
3. root Cargo package name when available

Current constraints:

- RPM spec lookup uses `.cargo-tizen.toml [package].name` when present, otherwise the resolved package name
- RPM and TPK packaging both expect a built binary named after the selected package
- custom `[[bin]]` names and multi-bin crate staging are not implemented
- multi-package grouping is supported for RPM only

### Build outputs

Default Cargo target dir:

```text
target/tizen/<arch>/cargo
```

Generated RPM and TPK work trees stay under arch- and profile-segmented directories in `target/tizen/`.

### Release flow

`gh-release` is an RPM-specific workflow with these enforced preconditions:

- clean working tree
- current branch `main`
- remote `origin`
- authenticated `gh`
- project config with `[package].name` and `[package].packages`
- one shared release version across the selected crates; the version may live in per-crate manifests, `[workspace.package].version`, or both, and `--bump` updates each contributing manifest path

The flow:

1. optionally bump version
2. sync the RPM spec `Version:` field
3. build release binaries
4. package RPMs
5. stage RPMs into `<packaging-dir>/rpm/sources/`, replacing only previously staged RPMs for the same output package+arch
6. generate SHA256 sidecars
7. commit artifacts
8. tag and push
9. create or update the GitHub release

## Test Coverage

The current unit tests cover:

- CLI parsing
- config parsing and merge behavior
- architecture mapping and auto-detection
- package selection rules
- packaging layout resolution
- sysroot cache naming
- rootstrap selection
- build context rendering
- RPM build helper logic
- TPK manifest parsing and command rendering
- release version and tag handling

End-to-end packaging still depends on real SDK, rootstraps, toolchains, and devices. Those flows are not covered in CI today.

## Documentation Rule

This repository treats `docs/` as the source of truth. Agent-facing files should route back into these docs instead of duplicating them.
