# Commands

`cargo-tizen` is used as a Cargo subcommand:

```sh
cargo tizen <command> [options]
```

Global options:

- `-v, --verbose`
- `-q, --quiet`
- `--config <path>`

Packaging format:

- `cargo-tizen` supports both **RPM** and **TPK** packaging.
- If you are coming from `flutter-tizen`, note that it is primarily **TPK** oriented.
- Device workflows use `sdb` similarly to `flutter-tizen` (`devices`, `run`).

## `setup`

Prepare and cache sysroot for one architecture.

```sh
cargo tizen setup -A <armv7l|aarch64> [--profile <name>] [--platform-version <ver>] [--provider <rootstrap|repo>] [--sdk-root <path>] [--force]
```

Notes:

- `setup` is optional for normal build/package/run flows.
- `build`/`rpm`/`tpk`/`run` automatically invoke setup if sysroot is not ready.
- Use `setup` when you want to pre-populate cache explicitly.

Examples:

```sh
cargo tizen setup -A armv7l --profile mobile --platform-version 9.0
cargo tizen setup -A aarch64 --sdk-root /opt/tizen-studio
```

## `build`

Cross-build Rust project using cached sysroot.

```sh
cargo tizen build -A <armv7l|aarch64> [--release] [--target-dir <path>] [-- <cargo build args>]
```

Examples:

```sh
cargo tizen build -A armv7l
cargo tizen build -A aarch64 --release
cargo tizen build -A armv7l -- --features my_feature
```

## `rpm`

Generate RPM from built binary.

```sh
cargo tizen rpm -A <armv7l|aarch64> [--cargo-release] [--release <n>] [--spec <path>] [--output <dir>] [--no-build]
```

Current behavior:

- Staging expects the built binary path `<target-dir>/<rust-target>/<profile>/<package-name>`.

Examples:

```sh
cargo tizen rpm -A armv7l --cargo-release
cargo tizen rpm -A aarch64 --cargo-release --release 3
cargo tizen rpm -A armv7l --no-build
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

## `tpk`

Package as TPK using Tizen CLI.

```sh
cargo tizen tpk -A <armv7l|aarch64> [--cargo-release] [--manifest <path>] [--output <dir>] [--sign <profile>] [--reference <path>] [--extra-dir <path>] [--no-build]
```

Notes:

- Requires `tizen-manifest.xml`.
- If `--manifest` is omitted, lookup order is:
  - `<workspace>/tizen-manifest.xml`
  - `<workspace>/tizen/tizen-manifest.xml`
- Staging expects the built binary path `<target-dir>/<rust-target>/<profile>/<package-name>`.

Examples:

```sh
cargo tizen tpk -A armv7l --cargo-release --manifest ./tizen-manifest.xml
cargo tizen tpk -A aarch64 --no-build --manifest ./tizen/tizen-manifest.xml
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

## `run`

Package, install, and launch on a connected device.

```sh
cargo tizen run -A <armv7l|aarch64> [-d <device-id>] [--cargo-release] [--manifest <path>] [--output <dir>] [--sign <profile>] [--reference <path>] [--extra-dir <path>] [--no-build] [--tpk <path>] [--app-id <id>]
```

Behavior:

- If `--tpk` is omitted, `cargo-tizen` builds/packages a TPK first.
- If one ready device exists, it is auto-selected.
- If multiple ready devices exist, `-d/--device` is required.
- Install uses `sdb -s <id> install <tpk>`.
- Launch uses:
  - `sdb -s <id> shell app_launcher -e <app_id>` (normal)
  - `sdb -s <id> shell 0 execute <app_id>` (secure protocol devices)

Examples:

```sh
cargo tizen run -A armv7l --cargo-release --manifest ./tizen-manifest.xml
cargo tizen run -A aarch64 -d 192.168.0.101:26101 --cargo-release --manifest ./tizen-manifest.xml
cargo tizen run -A armv7l --tpk ./build/app.tpk --app-id org.example.app -d <device-id>
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

## Output directories

Build outputs:

- `target/tizen/<arch>/cargo/<rust-target>/<debug|release>/`

Packaging outputs:

- `target/tizen/<arch>/<debug|release>/stage/`
- `target/tizen/<arch>/<debug|release>/rpmbuild/`
- `target/tizen/<arch>/<debug|release>/tpk/root/`
- `target/tizen/<arch>/<debug|release>/tpk/out/`
