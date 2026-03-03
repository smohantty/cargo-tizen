# Install for Linux

This guide installs `cargo-tizen` on Linux and prepares your host for cross-building Rust apps for Tizen.

## System requirements

- Linux x64
- Rust toolchain (`cargo`, `rustc`, `rustup`)
- Tizen SDK (see [install-tizen-sdk.md](install-tizen-sdk.md))
- `rpmbuild` (usually from `rpm-build`)
- common tools: `bash`, `curl`, `git`, `make`, `which`

## 1. Install Rust

If Rust is not installed:

```sh
curl https://sh.rustup.rs -sSf | sh
source "$HOME/.cargo/env"
```

Verify:

```sh
cargo --version
rustc --version
rustup --version
```

## 2. Install RPM tooling

Ubuntu/Debian:

```sh
sudo apt update
sudo apt install -y rpm
```

Fedora/RHEL/CentOS:

```sh
sudo dnf install -y rpm-build
```

Verify:

```sh
rpmbuild --version
```

## 3. Install Tizen SDK

Follow [install-tizen-sdk.md](install-tizen-sdk.md), then set one of:

- `TIZEN_SDK=/path/to/tizen-sdk`
- project config: `[sdk].root = "/path/to/tizen-sdk"`

## 4. Build and install cargo-tizen

From this repository:

```sh
cargo build
cargo test
cargo install --path .
```

Install location notes:

- `cargo install --path .` installs `cargo-tizen` into `$CARGO_HOME/bin`.
- If `CARGO_HOME` is not set, this is typically `~/.cargo/bin`.
- Cargo finds subcommands by executable name on `PATH`, so `cargo-tizen` enables `cargo tizen`.

Verify:

```sh
cargo tizen --help
```

## 5. Validate environment

```sh
cargo tizen doctor -A armv7l
cargo tizen doctor -A aarch64
```

If doctor reports missing SDK/rootstrap packages, see [install-tizen-sdk.md](install-tizen-sdk.md).

Configuration note:

- `.cargo-tizen.toml` is optional.
- Start with defaults and add config only when you need overrides.

## 6. Configure device connection (for `cargo tizen run`)

See [configure-device.md](configure-device.md), then verify:

```sh
cargo tizen devices --all
```
