# cargo-tizen

`cargo-tizen` is a Cargo subcommand for:
- provisioning and caching per-arch Tizen sysroots
- cross-building Rust binaries for Tizen architectures
- generating RPM packages from Rust build outputs
- generating TPK packages via Tizen CLI from Rust build outputs

## Status

Implemented:
- Commands: `setup`, `build`, `rpm`, `tpk`, `devices`, `run`, `doctor`, `fix`, `clean`
- Sysroot cache with metadata and locking
- Rootstrap-based provider with profile fallback policy
- SDK auto-discovery
- RPM generation via `rpmbuild`
- TPK generation via `tizen package -t tpk`

Known gap:
- `repo` provider is not implemented yet (only `rootstrap` is functional)
- RPM/TPK staging currently expects the built binary name to match `[package].name`

## Upstream Reference

`cargo-tizen` is an adaptation of design patterns from `flutter-tizen` for Tizen SDK/rootstrap/device workflows, applied to Rust/Cargo build and packaging.

- Upstream repo: https://github.com/flutter-tizen/flutter-tizen
- Upstream docs: https://github.com/flutter-tizen/flutter-tizen/tree/master/doc

When upstream design/commands around rootstrap resolution, Tizen SDK integration, device discovery (`sdb`), install, or launch change, review and sync relevant behavior/docs in this repository.

## Docs

- Linux installation: [doc/linux-install.md](doc/linux-install.md)
- Tizen SDK setup: [doc/install-tizen-sdk.md](doc/install-tizen-sdk.md)
- Device setup: [doc/configure-device.md](doc/configure-device.md)
- Command reference: [doc/commands.md](doc/commands.md)
- Packaging model vs `flutter-tizen`: [doc/packaging-model.md](doc/packaging-model.md)

## Prerequisites

Required tools:
- Rust toolchain (`cargo`, `rustc`, `rustup`)
- Rust target stdlib for each Tizen arch (`armv7l` uses `armv7-unknown-linux-gnueabi` or `armv7-unknown-linux-gnueabihf` based on rootstrap ABI; `aarch64` uses `aarch64-unknown-linux-gnu`)
- `rpmbuild` (usually from `rpm-build`, required only for `cargo tizen rpm`)
- Tizen SDK with Native CLI and matching rootstrap packages for your target/profile/version

Important:
- Tizen SDK sysroot provides native headers/libs for cross-linking.
- Rust targets from `rustup target add` provide Rust `std`/`core` artifacts for `rustc`.
- Both are required for cross-builds.

SDK detection order:
1. `setup --sdk-root <path>`
2. `[sdk].root` in `.cargo-tizen.toml`
3. `TIZEN_SDK` environment variable
4. parent of detected `sdb`
5. default locations (`~/.tizen-extension-platform/...`, `~/tizen-studio`)

## Build This Project

From this repository:

```bash
cargo build
cargo test
```

Release build:

```bash
cargo build --release
```

## Install This Tool

Install globally so `cargo tizen ...` works in any Rust project:

```bash
cargo install --path .
```

What this does:
- builds `cargo-tizen` and installs it to Cargo's bin directory
- install location is `$CARGO_HOME/bin` (or `~/.cargo/bin` when `CARGO_HOME` is not set)
- `cargo tizen ...` works because Cargo discovers `cargo-tizen` from `PATH`

After install, verify:

```bash
cargo tizen --help
```

## Use In Any Rust Project

### 1. Optional: add project config

`.cargo-tizen.toml` is optional.

- If SDK/toolchains are discoverable and defaults fit your target, you can run without this file.
- Add this file only when you need overrides (SDK path, profile/version, linker/toolchain mapping, arch mapping).

Minimal config (recommended):

```toml
[sdk]
root = "/path/to/tizen-studio"
```

Advanced/full override example:

Create `.cargo-tizen.toml` in the target Rust project:

```toml
[default]
arch = "armv7l"
profile = "mobile"
platform_version = "10.0"
provider = "rootstrap"

[sdk]
root = "/path/to/tizen-studio"

[arch.armv7l]
rust_target = "armv7-unknown-linux-gnueabi"
linker = "arm-linux-gnueabi-gcc"
tizen_cli_arch = "arm"
tizen_build_arch = "armel"
rpm_build_arch = "armv7l"

[arch.aarch64]
rust_target = "aarch64-unknown-linux-gnu"
linker = "aarch64-linux-gnu-gcc"
tizen_cli_arch = "aarch64"
tizen_build_arch = "aarch64"
rpm_build_arch = "aarch64"
```

### 2. Validate toolchain and SDK

```bash
cargo tizen doctor
cargo tizen doctor -A armv7l
cargo tizen doctor -A aarch64
```

Notes:
- `cargo tizen doctor` (without `-A`) checks both `armv7l` and `aarch64`.
- `cargo tizen doctor -A <arch>` checks a specific architecture.
- doctor prints installed SDK `--platform-version/--profile` options per arch, marks `[selected]` target used by default, and shows `[cached]` vs `[not-cached]`.

### 2.1 Fix missing Rust targets

Install missing Rust targets and prepare missing sysroots automatically:

```bash
cargo tizen fix
```

If `rpmbuild` is missing, `fix` prints a distro-specific install hint (warning only; required for RPM packaging).

Install missing target for one architecture:

```bash
cargo tizen fix -A armv7l
```

Architecture selection when `-A/--arch` is omitted (`setup`, `build`, `rpm`, `tpk`, `run`):
1. `[default].arch`
2. exactly one configured `[arch.*]` entry
3. exactly one architecture from connected ready Tizen devices
4. otherwise command fails and asks for `-A`

### 3. Prepare sysroot cache

```bash
cargo tizen setup -A armv7l
cargo tizen setup -A aarch64
```

Note:
- `setup` is optional for normal builds.
- `build`/`rpm`/`tpk`/`run` automatically run setup when sysroot is missing or invalid.
- use `setup` mainly to pre-warm cache ahead of time.
- when `--profile` and/or `--platform-version` are omitted, `cargo-tizen` selects from installed rootstraps in your SDK for the selected arch.
- when an unavailable profile/platform is requested, `cargo-tizen` prints installed options like `--platform-version <ver> --profile <name>`.

Force refresh:

```bash
cargo tizen setup -A armv7l --force
```

### 4. Cross-build Rust binaries

Debug build:

```bash
cargo tizen build -A armv7l
```

Release build:

```bash
cargo tizen build -A aarch64 --release
```

### 5. Generate RPM

Build + package in release profile:

```bash
cargo tizen rpm -A armv7l --cargo-release
```

Use a custom RPM release field:

```bash
cargo tizen rpm -A aarch64 --cargo-release --release 3
```

Use existing build outputs:

```bash
cargo tizen rpm -A armv7l --no-build
```

### 6. Generate TPK

Build + package as TPK (requires `tizen-manifest.xml`):

```bash
cargo tizen tpk -A armv7l --cargo-release --manifest ./tizen/tizen-manifest.xml
```

Use existing build outputs:

```bash
cargo tizen tpk -A aarch64 --no-build --manifest ./tizen-manifest.xml
```

Sign with a Tizen security profile:

```bash
cargo tizen tpk -A armv7l --cargo-release --manifest ./tizen-manifest.xml --sign my_profile
```

### 7. Run On Device

List connected devices:

```bash
cargo tizen devices --all
```

Build/package/install/launch on an auto-selected device:

```bash
cargo tizen run -A armv7l --cargo-release --manifest ./tizen-manifest.xml
```

Use a specific device ID:

```bash
cargo tizen run -A aarch64 -d 192.168.0.101:26101 --cargo-release --manifest ./tizen-manifest.xml
```

Install and launch a prebuilt TPK:

```bash
cargo tizen run -A armv7l -d <device-id> --tpk ./build/app.tpk --app-id org.example.app
```

## Architecture Mapping Defaults

| CLI arch | Rust target | Tizen CLI arch | Tizen build arch | RPM arch |
|---|---|---|---|---|
| `armv7l` | `armv7-unknown-linux-gnueabi` | `arm` | `armel` | `armv7l` |
| `aarch64` | `aarch64-unknown-linux-gnu` | `aarch64` | `aarch64` | `aarch64` |

Notes:
- For `armv7l` with `provider=rootstrap` and no explicit `[arch.armv7l].rust_target`, cargo-tizen infers soft/hard float Rust target from selected rootstrap headers.

## Output Layout

Cargo build output:
- `target/tizen/<arch>/cargo/<rust-target>/<debug|release>/`

Staging and RPM output:
- `target/tizen/<arch>/<debug|release>/stage/`
- `target/tizen/<arch>/<debug|release>/rpmbuild/`

TPK output:
- `target/tizen/<arch>/<debug|release>/tpk/root/`
- `target/tizen/<arch>/<debug|release>/tpk/out/`

## Command Reference

- `cargo tizen setup [-A <armv7l|aarch64>] [--profile] [--platform-version] [--provider] [--sdk-root] [--force]`
- `cargo tizen build [-A <armv7l|aarch64>] [--release] [--target-dir <path>] [-- <cargo build args>]`
- `cargo tizen rpm [-A <armv7l|aarch64>] [--cargo-release] [--release <n>] [--spec <path>] [--output <dir>] [--no-build]`
- `cargo tizen tpk [-A <armv7l|aarch64>] [--cargo-release] [--manifest <path>] [--output <dir>] [--sign <profile>] [--reference <path>] [--extra-dir <path>] [--no-build]`
- `cargo tizen devices [--all]`
- `cargo tizen run [-A <armv7l|aarch64>] [-d <device-id>] [--cargo-release] [--manifest <path>] [--output <dir>] [--sign <profile>] [--reference <path>] [--extra-dir <path>] [--no-build] [--tpk <path>] [--app-id <id>]`
- `cargo tizen doctor [-A <armv7l|aarch64>]`
- `cargo tizen fix [-A <armv7l|aarch64>]`
- `cargo tizen clean [--sysroot] [--build] [--all] [-A <armv7l|aarch64>]`

## Troubleshooting

If `doctor` says SDK is missing:
- install Tizen SDK / VS Code Extension for Tizen
- set `TIZEN_SDK` or `[sdk].root`
- rerun `cargo tizen doctor -A <arch>`

If `setup` fails with rootstrap missing:
- install matching Native App Development/rootstrap packages for your profile and platform version
- rerun `cargo tizen setup -A <arch>`

If build fails early with "compiler is unusable" and `/root/.dibs/...` in stderr:
- your selected cross-compiler has broken built-in include paths
- configure `[arch.<arch>].linker`, `[arch.<arch>].cc`, and `[arch.<arch>].cxx` to a working Tizen GCC/Clang path
- rerun `cargo tizen doctor -A <arch>` and then `cargo tizen build`

If `rpmbuild` is missing:
- install your distro package providing `rpmbuild` (commonly `rpm-build`)

If no device is found:
- verify `sdb devices` shows your target as `device`
- connect network device with `sdb connect <ip:port>`
- rerun `cargo tizen devices --all`
