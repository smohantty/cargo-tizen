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

## `setup`

Prepare and cache sysroot for one architecture.

```sh
cargo tizen setup -A <armv7l|aarch64> [--profile <name>] [--platform-version <ver>] [--provider <rootstrap|repo>] [--sdk-root <path>] [--force]
```

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
