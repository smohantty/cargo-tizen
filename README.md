# cargo-tizen

A Cargo subcommand for cross-building Rust projects for Tizen and packaging them as RPM or TPK.

```
cargo tizen build -A armv7l --release
cargo tizen rpm -A armv7l --release
cargo tizen tpk -A armv7l --release
cargo tizen install -A armv7l --release
```

**New to cargo-tizen?** Start with the [Getting Started guide](doc/getting-started.md).

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

The built-in help is intended to be the fastest onboarding path:

```bash
cargo tizen --help
cargo tizen <command> --help
```

Each command help page includes plain-language descriptions, notes, and examples.

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

Install [Tizen Studio](https://developer.tizen.org/development/tizen-studio/download).

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
whether the current project has the expected RPM spec and TPK manifest layout. The report stays concise and highlights
warnings and errors. If `doctor` reports a missing SDK, missing linker, or other host-tool issue, fix that manually and
rerun `cargo tizen doctor`.

## Quick Start

RPM, TPK, and `install` currently assume the built binary lives at
`<target-dir>/<rust-target>/<debug|release>/<package-name>`. Projects with a custom `[[bin]]` name or multiple
binaries should make sure the packaged binary name matches `[package].name` in `Cargo.toml`.

### Scaffold starter files

```bash
cargo tizen init
```

This creates starter RPM spec and TPK manifest files when they are missing. It only writes `.cargo-tizen.toml` when it
is missing. Use `cargo tizen init --rpm` or `cargo tizen init --tpk` to add packaging scaffolds. Existing packaging
files are left untouched unless you rerun with `--force`.

### Cross-build

```bash
cargo tizen build -A armv7l
cargo tizen build -A aarch64 --release
```

### Package as RPM

```bash
cargo tizen rpm -A armv7l --release
```

For workspaces with multiple binary crates that should be packaged into a single RPM,
list them in `.cargo-tizen.toml`:

```toml
[rpm]
packages = ["my-server", "my-cli"]
```

This builds and stages all listed binaries. The spec file is looked up by the first
package name (`tizen/rpm/my-server.spec`), and packaging inputs are validated
before the build starts. Single-crate projects need no config.

### Package as TPK

```bash
cargo tizen tpk -A armv7l --release
```

This expects an authored manifest at `tizen/tpk/tizen-manifest.xml` and fails
before the build starts if the manifest is missing.

### Install to device

```bash
# List connected devices
cargo tizen devices

# Build, package, and install on device
cargo tizen install -A armv7l --release
```

## Packaging Layout

By default, packaging files live under `tizen/`:

```text
tizen/
  rpm/
    <cargo-package-name>.spec
    sources/                          # optional extra sources for rpmbuild
  tpk/
    tizen-manifest.xml
    reference/
    extra/
```

Files in `rpm/sources/` are copied into `rpmbuild/SOURCES/` so your spec can
reference them as `Source1:`, `Source2:`, etc. Useful for systemd units, env files,
and configs. See `templates/reference-projects/rpm-service-app/` for a working example.

Use `cargo tizen init` to create starter packaging files. The packaging commands themselves do not auto-generate missing
spec or manifest files on demand.

For a non-standard layout, point commands at a different packaging root:

```bash
cargo tizen rpm --packaging-dir ./packaging
cargo tizen tpk --packaging-dir ./packaging
cargo tizen install --packaging-dir ./packaging
```

You can persist that root in `.cargo-tizen.toml`:

```toml
[default]
packaging_dir = "./packaging"
```

See [doc/packaging-layout.md](doc/packaging-layout.md) for the full layout contract and migration notes.

## Architecture Selection

When `-A` / `--arch` is omitted, `cargo-tizen` auto-selects:

1. `[default].arch` from `.cargo-tizen.toml`
2. The only configured `[arch.*]` entry (if exactly one)
3. The architecture of the only connected Tizen device
4. Otherwise, the command fails and asks you to pass `-A`

## Project Configuration

`.cargo-tizen.toml` in your project root is optional. Add it only when you need overrides.

In a multi-package workspace, set `[default].package = "member-name"` if you want
`rpm`, `tpk`, and `install` to package the same member by default without repeating
`-p/--package` on every command.

Minimal (just point to SDK):

```toml
[sdk]
root = "/path/to/tizen-studio"
```

Full example:

```toml
[default]
arch = "armv7l"
package = "my-app"
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

## Packaging Limitations

- The packaged binary name must match `[package].name` in `Cargo.toml`.
- Multi-bin and custom `[[bin]]` names are not yet supported.
- Multi-package workspaces must pick a member with `-p/--package` or `[default].package` in `.cargo-tizen.toml`.
- `cargo tizen install` deploys TPK only (not RPM).

## TPK Signing

TPK packages must be signed to install on Tizen devices.

### 1. Create a security profile

Open Tizen Studio **Tools > Certificate Manager** and create a certificate profile. For development, use the default Tizen distributor certificate.

### 2. Set a default signing profile

```bash
cargo tizen config --sign my_profile
```

This is stored in `~/.config/cargo-tizen/config.toml` and used automatically by `tpk` and `install`. Override per-command with `--sign <profile>`.

### 3. Samsung TV devices

For Samsung TVs, create a **Samsung** type profile in Certificate Manager with your TV's DUID (from `sdb capability | grep duid`).

## Commands

| Command | Description |
|---------|-------------|
| `cargo tizen init` | Create starter config and packaging files for the current project |
| `cargo tizen doctor` | Check SDK, toolchain, sysroot, and packaging readiness |
| `cargo tizen fix` | Install missing Rust targets and prepare missing sysroots |
| `cargo tizen build` | Cross-build the current Rust project for a Tizen target |
| `cargo tizen rpm` | Package the project as an RPM using an existing spec file |
| `cargo tizen tpk` | Package the project as a signed TPK using the Tizen CLI |
| `cargo tizen install` | Build or reuse a TPK and install it on a connected device |
| `cargo tizen devices` | List connected Tizen devices discovered via `sdb` |
| `cargo tizen setup` | Prepare and cache a Tizen sysroot for cross-compilation |
| `cargo tizen clean` | Remove build outputs and/or cached sysroots |
| `cargo tizen config` | View or update persistent `cargo-tizen` settings |

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

- [Getting started](doc/getting-started.md)
- [Quick reference](doc/quick-reference.md)
- [Full command reference](doc/commands.md)
- [Linux installation](doc/linux-install.md)
- [Tizen SDK setup](doc/install-tizen-sdk.md)
- [Device configuration](doc/configure-device.md)
- [Packaging layout](doc/packaging-layout.md)
- [Packaging model](doc/packaging-model.md)
- [Changelog](CHANGELOG.md)
