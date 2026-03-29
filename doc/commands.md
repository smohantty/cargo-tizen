# Commands

`cargo-tizen` is used as a Cargo subcommand:

```sh
cargo tizen <command> [options]
```

For the fastest onboarding path, start with the built-in help:

```sh
cargo tizen --help
cargo tizen <command> --help
```

The generated help includes short command descriptions, usage notes, and runnable examples.

Quick start:

```sh
cargo tizen doctor
cargo tizen fix
```

Global options:

- `-v, --verbose`: print detailed progress and diagnostics
- `-q, --quiet`: reduce output to warnings and errors
- `--config <path>`: merge an additional config file after the default user and project config

Command summary:

| Command | Description |
|---|---|
| `setup` | Prepare and cache a Tizen sysroot for cross-compilation |
| `build` | Cross-build the current Rust project for a Tizen target |
| `rpm` | Package the project as an RPM using an existing spec file |
| `tpk` | Package the project as a signed TPK using the Tizen CLI |
| `devices` | List connected Tizen devices discovered via `sdb` |
| `install` | Build or reuse a TPK and install it on a connected device |
| `doctor` | Check SDK, toolchain, sysroot, and packaging readiness |
| `fix` | Install missing Rust targets and prepare missing sysroots |
| `clean` | Remove build outputs and/or cached sysroots |
| `config` | View or update persistent `cargo-tizen` settings |

Packaging format:

- `cargo-tizen` supports both **RPM** and **TPK** packaging.
- If you are coming from `flutter-tizen`, note that it is primarily **TPK** oriented.
- Device workflows use `sdb` similarly to `flutter-tizen` (`devices`, `install`).

## `setup`

Prepare and cache sysroot for one architecture.

```sh
cargo tizen setup [-A <armv7l|aarch64>] [--profile <name>] [--platform-version <ver>] [--provider <rootstrap|repo>] [--sdk-root <path>] [--force]
```

Notes:

- `setup` is optional for normal build/package/run flows.
- `build`/`rpm`/`tpk`/`run` automatically invoke setup if sysroot is not ready.
- Use `setup` when you want to pre-populate cache explicitly.
- If `--profile` and/or `--platform-version` are omitted, installed SDK rootstraps are scanned and a matching installed target is auto-selected.
- If requested profile/platform is not installed, available installed options are printed in the error output.

Examples:

```sh
cargo tizen setup -A armv7l --profile mobile --platform-version 10.0
cargo tizen setup -A aarch64 --sdk-root /opt/tizen-studio
```

## `build`

Cross-build Rust project using cached sysroot.

```sh
cargo tizen build [-A <armv7l|aarch64>] [--release] [--target-dir <path>] [-- <cargo build args>]
```

Examples:

```sh
cargo tizen build -A armv7l
cargo tizen build -A aarch64 --release
cargo tizen build -A armv7l -- --features my_feature
```

On success, `build` prints:
- artifact directory path (`<target-dir>/<rust-target>/<debug|release>`)
- primary binary path when package name can be determined

Architecture auto-selection when `-A` is omitted (`setup`, `build`, `rpm`, `tpk`, `run`):
1. `[default].arch`
2. exactly one configured `[arch.*]` entry
3. exactly one architecture from connected ready Tizen devices
4. otherwise command fails and asks for `-A`

Rust target note:
- Tizen SDK sysroot gives native headers/libs for linking.
- Rust `std` for the target triple still comes from `rustup target add <triple>`.
- Both are needed for cross-builds.
- For `armv7l` with `provider=rootstrap` and no explicit `[arch.armv7l].rust_target`, cargo-tizen infers soft/hard float target from selected rootstrap headers.
- When sysroot has `libssl`/`libcrypto` but no `openssl.pc`, `cargo-tizen` sets `OPENSSL_*` fallback env automatically.

## `rpm`

Generate RPM from built binary (or binaries).

```sh
cargo tizen rpm [-A <armv7l|aarch64>] [-p <package>] [--cargo-release] [--packaging-dir <path>] [--output <dir>] [--no-build]
```

Current behavior:

- Looks for the spec at `<packaging-dir>/rpm/<package-name>.spec`.
- Default packaging root is `<workspace>/tizen`.
- In a multi-package workspace, select the package with `-p/--package` or `[default].package` in `.cargo-tizen.toml`.
- If the spec is missing, the command fails and prints the expected path plus the `--packaging-dir` escape hatch.
- Staging expects the built binary path `<target-dir>/<rust-target>/<profile>/<package-name>`.

**Multi-package RPM:** To bundle multiple binaries from a workspace into a single RPM, set `[rpm].packages` in `.cargo-tizen.toml`:

```toml
[rpm]
packages = ["my-server", "my-cli"]
```

- All listed packages are built and staged into `rpmbuild/SOURCES/`.
- The spec file is looked up by the first package name in the list.
- CLI `-p` overrides to single-package mode even if `[rpm].packages` is set.
- Single-crate projects need no config (auto-detected from `Cargo.toml`).

Examples:

```sh
cargo tizen rpm -A armv7l --cargo-release
cargo tizen rpm -A aarch64 --cargo-release --packaging-dir ./packaging
cargo tizen rpm -A armv7l --no-build
cargo tizen rpm -p my-server   # single-package override
```

## `doctor`

Validate host/toolchain/SDK/sysroot readiness.

```sh
cargo tizen doctor [-A <armv7l|aarch64>]
```

Examples:

```sh
cargo tizen doctor
cargo tizen doctor -A armv7l
```

Notes:

- `cargo tizen doctor` checks both `armv7l` and `aarch64`.
- `cargo tizen doctor -A <arch>` checks one architecture.
- `doctor` reports the current packaging root plus missing/present RPM spec and TPK manifest files for the active project.
- For rootstrap provider, doctor reports installed SDK coverage grouped by `--platform-version/--profile` with supported architecture summary.
- Default doctor output is concise; use `cargo tizen -v doctor` for detailed per-check path output.
- Missing `rpmbuild` is reported as a warning (it is required only for `cargo tizen rpm`).

## `fix`

Install missing Rust targets and prepare missing sysroots used for cross-builds.

```sh
cargo tizen fix [-A <armv7l|aarch64>]
```

Behavior:

- Without `-A`, checks both `armv7l` and `aarch64` target triples and installs missing ones via `rustup target add`.
- With `-A`, checks and installs only the selected architecture target triple.
- Also ensures sysroot cache exists for each selected architecture (runs `setup` defaults when missing).
- If `rpmbuild` is missing, prints a non-failing warning with distro-specific install hint (needed only for `cargo tizen rpm`); install command is highlighted in terminal output.

Examples:

```sh
cargo tizen fix
cargo tizen fix -A armv7l
```

## `tpk`

Package as TPK using Tizen CLI.

```sh
cargo tizen tpk [-A <armv7l|aarch64>] [--cargo-release] [--packaging-dir <path>] [--output <dir>] [--sign <profile>] [--no-build]
```

Notes:

- Looks for the manifest at `<packaging-dir>/tpk/tizen-manifest.xml`.
- Default packaging root is `<workspace>/tizen`.
- In a multi-package workspace, select the package with `-p/--package` or `[default].package` in `.cargo-tizen.toml`.
- Optional directories:
  - `<packaging-dir>/tpk/reference` maps to `tizen package -r`
  - `<packaging-dir>/tpk/extra` maps to `tizen package -e`
- If the manifest is missing, the command fails and prints the expected path plus the `--packaging-dir` escape hatch.
- Staging expects the built binary path `<target-dir>/<rust-target>/<profile>/<package-name>`.

Examples:

```sh
cargo tizen tpk -A armv7l --cargo-release
cargo tizen tpk -A aarch64 --no-build --packaging-dir ./packaging
```

## `devices`

List connected devices discovered via `sdb`.

```sh
cargo tizen devices [--all]
```

Notes:

- By default, output focuses on ready Tizen devices.
- `--all` includes offline/unauthorized/non-Tizen entries parsed from `sdb devices`.

Examples:

```sh
cargo tizen devices
cargo tizen devices --all
```

## `install`

Build, package, and install a TPK on a connected device.

```sh
cargo tizen install [-A <armv7l|aarch64>] [-d <device-id>] [--cargo-release] [--packaging-dir <path>] [--output <dir>] [--sign <profile>] [--no-build] [--tpk <path>]
```

Behavior:

- `install` is TPK-only.
- If `--tpk` is omitted, `cargo-tizen` builds/packages a TPK first using the same packaging layout as `cargo tizen tpk`.
- If one ready device exists, it is auto-selected.
- If multiple ready devices exist, `-d/--device` is required.
- Installs with `sdb -s <id> install <tpk>`.

Examples:

```sh
cargo tizen install -A armv7l --cargo-release
cargo tizen install -A aarch64 -d 192.168.0.101:26101 --cargo-release --packaging-dir ./packaging
cargo tizen install -A armv7l --tpk ./build/app.tpk -d <device-id>
```

## `clean`

Remove build outputs and/or cached sysroots.

```sh
cargo tizen clean [--build] [--sysroot] [--all] [-A <armv7l|aarch64>]
```

Examples:

```sh
cargo tizen clean --build
cargo tizen clean --sysroot -A aarch64
cargo tizen clean --all
```

## `config`

View or set persistent user-level configuration values.

```sh
cargo tizen config [--sign <profile>] [--show]
```

Notes:

- Settings are stored in `~/.config/cargo-tizen/config.toml`.
- `--sign <profile>` sets the default TPK signing profile used by `tpk` and `install` when `--sign` is not passed on the command line.
- `--sign ""` (empty string) clears the stored signing profile.
- `--show` (or no flags) prints current configuration values.

Examples:

```sh
cargo tizen config --sign my_profile
cargo tizen config --show
cargo tizen config
```

## Output directories

Build outputs:

- `target/tizen/<arch>/cargo/<rust-target>/<debug|release>/`

Packaging outputs:

- `target/tizen/<arch>/<debug|release>/stage/`
- `target/tizen/<arch>/<debug|release>/rpmbuild/`
- `target/tizen/<arch>/<debug|release>/tpk/root/`
- `target/tizen/<arch>/<debug|release>/tpk/out/`
