# cargo-tizen

A Cargo subcommand for cross-building Rust projects for Tizen and packaging them as RPM or TPK.

```
cargo tizen build -A armv7l --release
cargo tizen rpm -A armv7l --cargo-release
cargo tizen tpk -A armv7l --cargo-release
cargo tizen run -A armv7l --cargo-release
```

## Install

```bash
cargo install --git https://github.com/nickalready/cargo-tizen
```

Or from a local clone:

```bash
cargo install --path .
```

Verify:

```bash
cargo tizen --help
```

## Prerequisites

### 1. Rust cross-compilation targets

```bash
rustup target add armv7-unknown-linux-gnueabi armv7-unknown-linux-gnueabihf aarch64-unknown-linux-gnu
```

For `armv7l`, install both ARM Rust targets. With the default rootstrap provider, `cargo-tizen`
inspects the selected rootstrap headers and uses:

- `armv7-unknown-linux-gnueabi` when the sysroot exposes `stubs-soft.h`
- `armv7-unknown-linux-gnueabihf` when the sysroot exposes `stubs-hard.h`

### 2. C cross-compilers (linkers)

| Target arch | apt package | Linker binary |
|-------------|-------------|---------------|
| `armv7l` | `gcc-arm-linux-gnueabi` | `arm-linux-gnueabi-gcc` |
| `aarch64` | `gcc-aarch64-linux-gnu` | `aarch64-linux-gnu-gcc` |

```bash
sudo apt install gcc-arm-linux-gnueabi gcc-aarch64-linux-gnu
```

### 3. Tizen SDK

Install either [Tizen Studio](https://developer.tizen.org/development/tizen-studio/download) or the [VS Code Extension for Tizen](https://marketplace.visualstudio.com/items?itemName=nickelready.nickelready-vscode-tizen).

Then install rootstrap packages for your target profile and platform version through the SDK Package Manager.

`cargo-tizen` finds the SDK automatically from:

1. `[sdk].root` in `.cargo-tizen.toml`
2. `TIZEN_SDK` environment variable
3. Parent directory of `sdb` on `PATH`
4. Default locations (`~/tizen-studio`, `~/.tizen-extension-platform/...`)

If auto-detection fails, either set `[sdk].root` / `TIZEN_SDK`, or run `cargo tizen setup --sdk-root /path/to/sdk`
to override the SDK location while preparing the sysroot cache. `--sdk-root` is a `setup` flag, not a global flag.

### 4. rpmbuild (only for RPM packaging)

```bash
# Debian/Ubuntu
sudo apt install rpm

# Fedora/RHEL
sudo dnf install rpm-build

# Arch Linux
sudo pacman -S rpm-tools
```

### Verify everything

```bash
cargo tizen doctor
```

This checks all tools, SDK, rootstraps, linkers, Rust targets, and sysroot cache. To repair missing Rust targets and
sysroots:

```bash
cargo tizen fix
```

`cargo tizen fix` can install missing Rust targets and prepare missing sysroots. `cargo tizen doctor` also reports
whether the current project has the expected RPM spec and TPK manifest layout. If `doctor` reports a missing SDK,
missing linker, or other host-tool issue, fix that manually and rerun `cargo tizen doctor`.

## Quick Start

RPM, TPK, and `run` currently assume the built binary lives at
`<target-dir>/<rust-target>/<debug|release>/<package-name>`. Projects with a custom `[[bin]]` name or multiple
binaries should make sure the packaged binary name matches `[package].name` in `Cargo.toml`.

### Cross-build

```bash
cargo tizen build -A armv7l
cargo tizen build -A aarch64 --release
```

### Package as RPM

```bash
cargo tizen rpm -A armv7l --cargo-release
```

### Package as TPK

```bash
cargo tizen tpk -A armv7l --cargo-release
```

This expects an authored manifest at `tizen/tpk/tizen-manifest.xml`.

### Deploy to device

```bash
# List connected devices
cargo tizen devices

# Build, package, install, and launch
cargo tizen run -A armv7l --cargo-release
```

## Packaging Layout

By default, packaging files live under `tizen/`:

```text
tizen/
  rpm/
    <cargo-package-name>.spec
  tpk/
    tizen-manifest.xml
    reference/
    extra/
```

`cargo-tizen` does not auto-generate missing spec or manifest files.

For a non-standard layout, point commands at a different packaging root:

```bash
cargo tizen rpm --packaging-dir ./packaging
cargo tizen tpk --packaging-dir ./packaging
cargo tizen run --packaging-dir ./packaging
```

You can persist that root in `.cargo-tizen.toml`:

```toml
[default]
packaging_dir = "./packaging"
```

Reference projects live in:

- `templates/reference-projects/rpm-app`
- `templates/reference-projects/tpk-service-app`

See [doc/packaging-layout.md](doc/packaging-layout.md) for the full layout contract and migration notes.

## Architecture Selection

When `-A` / `--arch` is omitted, `cargo-tizen` auto-selects:

1. `[default].arch` from `.cargo-tizen.toml`
2. The only configured `[arch.*]` entry (if exactly one)
3. The architecture of the only connected Tizen device
4. Otherwise, the command fails and asks you to pass `-A`

## Project Configuration

`.cargo-tizen.toml` in your project root is optional. Add it only when you need overrides.

Minimal (just point to SDK):

```toml
[sdk]
root = "/path/to/tizen-studio"
```

Full example:

```toml
[default]
arch = "armv7l"
profile = "mobile"
platform_version = "10.0"
packaging_dir = "./packaging"

[sdk]
root = "/path/to/tizen-studio"

[arch.armv7l]
linker = "arm-linux-gnueabi-gcc"

[arch.aarch64]
linker = "aarch64-linux-gnu-gcc"
```

## Current Packaging Gaps

- Packaging assumes the built binary name matches `[package].name`.
- Multi-bin and renamed-bin packaging are not implemented yet.
- Workspace/member packaging is not implemented yet. Run packaging commands from a concrete package crate.
- `run` is TPK-only.

## TPK Signing

TPK packages must be signed to install on Tizen devices.

### 1. Create a security profile

Open Tizen Studio **Tools > Certificate Manager** and create a certificate profile. For development, use the default Tizen distributor certificate.

### 2. Set a default signing profile

```bash
cargo tizen config --sign my_profile
```

This is stored in `~/.config/cargo-tizen/config.toml` and used automatically by `tpk` and `run`. Override per-command with `--sign <profile>`.

### 3. Samsung TV devices

For Samsung TVs, create a **Samsung** type profile in Certificate Manager with your TV's DUID (from `sdb capability | grep duid`).

## Commands

| Command | Description |
|---------|-------------|
| `cargo tizen build` | Cross-build Rust project |
| `cargo tizen rpm` | Build and package as RPM |
| `cargo tizen tpk` | Build and package as TPK |
| `cargo tizen run` | Build, package, install, and launch on device |
| `cargo tizen devices` | List connected Tizen devices |
| `cargo tizen setup` | Pre-populate sysroot cache |
| `cargo tizen doctor` | Check toolchain and SDK readiness |
| `cargo tizen fix` | Auto-fix missing Rust targets and sysroots |
| `cargo tizen clean` | Remove build outputs and/or cached sysroots |
| `cargo tizen config` | View or set persistent configuration |

See [doc/commands.md](doc/commands.md) for full flag reference.

## Troubleshooting

**Doctor says SDK is missing:**
Install Tizen SDK, set `TIZEN_SDK` or `[sdk].root`, or rerun `cargo tizen setup --sdk-root /path/to/tizen-studio`,
then rerun `cargo tizen doctor`.

**Setup fails with rootstrap missing:**
Install matching rootstrap packages in Tizen SDK Package Manager for your target profile and platform version.

**Build fails with "compiler is unusable":**
Your cross-compiler has broken include paths. Configure `[arch.<arch>].linker` / `[arch.<arch>].cc` in `.cargo-tizen.toml` to a working toolchain path.

**No device found:**
Check `sdb devices` shows your target as `device`. For network devices: `sdb connect <ip:port>`.

## Further Documentation

- [Linux installation](doc/linux-install.md)
- [Tizen SDK setup](doc/install-tizen-sdk.md)
- [Device configuration](doc/configure-device.md)
- [Full command reference](doc/commands.md)
- [Packaging layout](doc/packaging-layout.md)
- [Packaging model](doc/packaging-model.md)

## Development

Build and test from this repository:

```bash
cargo build
cargo test
```

Upstream reference: [flutter-tizen](https://github.com/flutter-tizen/flutter-tizen) — `cargo-tizen` adapts its SDK/rootstrap/device workflow patterns for Rust.
