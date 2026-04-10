# cargo-tizen

A Cargo subcommand for cross-building Rust projects for Tizen and packaging them as RPM or TPK.

```bash
cargo tizen build -A armv7l --release
cargo tizen rpm -A armv7l --release
cargo tizen tpk -A armv7l --release
cargo tizen install -A armv7l --release   # TPK only
cargo tizen gh-release --dry-run          # RPM GitHub release pipeline
```

**New to cargo-tizen?** Start with the [Getting Started guide](doc/getting-started.md).

## What to expect

- Package-manager commands below assume a Linux host, which is the documented setup path in this repo. See [doc/linux-install.md](doc/linux-install.md) for the full host setup guide.
- `cargo tizen setup` is optional for normal use. `build`, `rpm`, `tpk`, and `install` prepare sysroots automatically when needed.
- `cargo tizen install` installs TPK packages only.

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

Built-in help is the fastest reference when you already know which command you want:

```bash
cargo tizen --help
cargo tizen <command> --help
```

## Prerequisites

### 1. Tizen SDK and rootstraps

Install [Tizen Studio](https://developer.tizen.org/development/tizen-studio/download), then install rootstrap packages for the profile and platform version you want to target through the SDK Package Manager.

`cargo-tizen` looks for the SDK in this order:

1. `[sdk].root` in `.cargo-tizen.toml`
2. `TIZEN_SDK`
3. parent directory of `sdb` on `PATH`
4. standard install locations such as `~/tizen-studio`

If auto-detection fails, either set `[sdk].root` / `TIZEN_SDK`, or run:

```bash
cargo tizen setup --sdk-root /path/to/tizen-studio
```

`--sdk-root` is a `setup` flag, not a global flag.

### 2. Rust target triples

```bash
rustup target add armv7-unknown-linux-gnueabi armv7-unknown-linux-gnueabihf aarch64-unknown-linux-gnu
```

For `armv7l`, install both ARM Rust targets. With the default rootstrap provider, `cargo-tizen` inspects the selected sysroot headers and chooses:

- `armv7-unknown-linux-gnueabi` when the sysroot exposes `stubs-soft.h`
- `armv7-unknown-linux-gnueabihf` when the sysroot exposes `stubs-hard.h`

### 3. Cross linkers

| Target arch | apt package | Linker binary |
|-------------|-------------|---------------|
| `armv7l` | `gcc-arm-linux-gnueabi` | `arm-linux-gnueabi-gcc` |
| `aarch64` | `gcc-aarch64-linux-gnu` | `aarch64-linux-gnu-gcc` |

```bash
sudo apt install gcc-arm-linux-gnueabi gcc-aarch64-linux-gnu
```

`cargo-tizen` already defaults to `arm-linux-gnueabi-gcc` for `armv7l` and `aarch64-linux-gnu-gcc` for `aarch64`. Add `[arch.*].linker` in `.cargo-tizen.toml` only when you need a different binary name or an explicit path.

### 4. RPM tooling (RPM workflow only)

```bash
# Debian/Ubuntu
sudo apt install rpm

# Fedora/RHEL
sudo dnf install rpm-build

# Arch Linux
sudo pacman -S rpm-tools
```

## Verify your environment

```bash
cargo tizen doctor
cargo tizen fix
```

`doctor` checks the SDK, rootstraps, Rust targets, linkers, packaging layout, and sysroot cache. By default it checks both supported architectures; use `-A` to check only one.

`fix` installs missing Rust targets and prepares missing sysroots. It does not install host packages such as cross linkers, `rpmbuild`, or Tizen Studio.

## Common workflows

### 1. Create project config and packaging scaffolds

```bash
cargo tizen init
```

With no format flags, `init` creates `.cargo-tizen.toml` only.

To create starter packaging files:

```bash
cargo tizen init --rpm
cargo tizen init --tpk
cargo tizen init --rpm --tpk
```

Existing packaging files are left untouched unless you pass `--force`.

### 2. Cross-build

```bash
cargo tizen build -A armv7l
cargo tizen build -A aarch64 --release
```

### 3. Package as RPM

Initialize the scaffold once:

```bash
cargo tizen init --rpm
```

Then edit `tizen/rpm/<package-name>.spec` for your app and build the RPM:

```bash
cargo tizen rpm -A armv7l --release
```

For workspaces with multiple binary crates that should be staged into one RPM, set `.cargo-tizen.toml`:

```toml
[package]
name = "my-project"
packages = ["my-server", "my-cli"]
```

`name` controls spec lookup (`tizen/rpm/my-project.spec`). `packages` controls which crates are built and staged.

### 4. Package as TPK and install on a device

Initialize the scaffold once:

```bash
cargo tizen init --tpk
```

Edit `tizen/tpk/tizen-manifest.xml`, then package:

```bash
cargo tizen tpk -A armv7l --release
```

TPK packaging uses Tizen Studio certificate profiles. You can select one per command:

```bash
cargo tizen tpk -A armv7l --release --sign my_profile
```

Or store a default profile for future `tpk` and `install` commands:

```bash
cargo tizen config --sign my_profile
```

If neither `--sign` nor stored config is set, `cargo-tizen` falls back to the Tizen CLI's default profile selection.

To install on a connected device:

```bash
cargo tizen devices
cargo tizen install -A armv7l --release
```

If multiple devices are connected, pass `-d <device-id>`.

For Samsung TVs, create a Samsung certificate profile in Tizen Studio Certificate Manager and include the TV DUID from `sdb capability | grep duid`.

### 5. Publish a GitHub RPM release

`gh-release` is an RPM release pipeline. It does not publish TPK releases.

Before using it, make sure:

- your working tree is clean
- you are on branch `main`
- your Git remote is `origin`
- `gh` is installed and authenticated
- `.cargo-tizen.toml` defines `[package].name` and `[package].packages`

```bash
cargo tizen gh-release --dry-run
cargo tizen gh-release --bump patch
```

`gh-release` builds the configured packages, packages RPMs, stages them into `<packaging-dir>/rpm/sources`, syncs the spec `Version:` field, commits the release artifacts, tags `v<version>` by default, pushes, and creates or updates the GitHub release with RPM and SHA256 assets. Use `--dry-run` to preview the full plan first.

Optional release defaults live under `[release]`:

```toml
[release]
arches = ["armv7l", "aarch64"]
tag_format = "v{version}"
```

## Packaging layout

By default, packaging files live under `tizen/`:

```text
tizen/
  rpm/
    <package-name>.spec
    sources/                          # optional extra sources for rpmbuild
  tpk/
    tizen-manifest.xml
    reference/
    extra/
```

Files in `rpm/sources/` are copied into `rpmbuild/SOURCES/`, so your spec can reference them as `Source1:`, `Source2:`, and so on. See `templates/reference-projects/rpm-service-app/` for a working example.

The packaging commands do not create missing spec or manifest files on demand. Use `cargo tizen init --rpm` and/or `cargo tizen init --tpk` first.

For a non-standard layout:

```bash
cargo tizen rpm --packaging-dir ./packaging
cargo tizen tpk --packaging-dir ./packaging
cargo tizen install --packaging-dir ./packaging
```

Or persist it in `.cargo-tizen.toml`:

```toml
[default]
packaging_dir = "./packaging"
```

See [doc/packaging-layout.md](doc/packaging-layout.md) for the full layout contract.

## Configuration

`.cargo-tizen.toml` is optional. `cargo tizen init` creates one with defaults, but most users only need a small subset of settings.

Common cases:

Point to a specific SDK:

```toml
[sdk]
root = "/path/to/tizen-studio"
```

Choose which workspace packages are packaged:

```toml
[package]
name = "my-app"
packages = ["my-app"]
```

Set a default architecture or packaging directory:

```toml
[default]
arch = "armv7l"
packaging_dir = "./packaging"
```

Useful advanced notes:

- `profile` and `platform_version` are only worth pinning when your SDK has multiple valid installed rootstrap combinations and you want deterministic selection.
- `[arch.armv7l].linker` and `[arch.aarch64].linker` are overrides. You do not need them when the default linker names already exist on `PATH`.
- `cargo tizen config --sign <profile>` stores a user-level default TPK signing profile in `~/.config/cargo-tizen/config.toml`.

## Architecture selection

When `-A` / `--arch` is omitted, `cargo-tizen` auto-selects:

1. `[default].arch` from `.cargo-tizen.toml`
2. the only configured `[arch.*]` entry, if exactly one exists
3. the architecture of the only connected ready Tizen device
4. otherwise, the command fails and asks you to pass `-A`

## Current limitations

- Custom `[[bin]]` names and multi-bin crates are not supported yet. `cargo-tizen` expects the packaged binary filename to match the selected package name.
- Multi-package workspaces must use `-p/--package` or `[package].packages` when package selection is ambiguous.
- `cargo tizen install` deploys TPK only.

## Commands

| Command | Description |
|---------|-------------|
| `cargo tizen init` | Create project config and optional packaging scaffolds |
| `cargo tizen doctor` | Check SDK, toolchain, sysroot, and packaging readiness |
| `cargo tizen fix` | Install missing Rust targets and prepare missing sysroots |
| `cargo tizen build` | Cross-build the current Rust project for a Tizen target |
| `cargo tizen rpm` | Package the project as an RPM using an existing spec file |
| `cargo tizen tpk` | Package the project as a signed TPK using the Tizen CLI |
| `cargo tizen install` | Build or reuse a TPK and install it on a connected device |
| `cargo tizen devices` | List connected Tizen devices discovered via `sdb` |
| `cargo tizen setup` | Prepare and cache a Tizen sysroot |
| `cargo tizen clean` | Remove build outputs and/or cached sysroots |
| `cargo tizen config` | View or update persistent user settings |
| `cargo tizen gh-release` | Build RPMs and publish a GitHub release |

See [doc/commands.md](doc/commands.md) for full flag reference.

## Troubleshooting

**SDK is missing**
Install Tizen Studio, set `TIZEN_SDK` or `[sdk].root`, then rerun `cargo tizen doctor`.

**Rootstrap is missing**
Install matching rootstrap packages in Tizen Studio Package Manager for your target profile and platform version.

**Linker is missing or unusable**
Install the matching cross compiler, or set `[arch.<arch>].linker` to the correct binary path.

**No device found**
Check `sdb devices` shows the target as `device`. For network targets, run `sdb connect <ip:port>`.

**`gh-release` fails before doing work**
Check for a clean working tree, branch `main`, remote `origin`, and a logged-in `gh` CLI.

## Further documentation

- [Getting started](doc/getting-started.md)
- [Quick reference](doc/quick-reference.md)
- [Full command reference](doc/commands.md)
- [Linux installation](doc/linux-install.md)
- [Tizen SDK setup](doc/install-tizen-sdk.md)
- [Device configuration](doc/configure-device.md)
- [Packaging layout](doc/packaging-layout.md)
- [Packaging model](doc/packaging-model.md)
- [Changelog](CHANGELOG.md)
