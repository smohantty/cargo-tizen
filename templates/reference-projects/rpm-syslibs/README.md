# hello-syslibs

Reference project that links against **platform system libraries** from the
Tizen SDK sysroot instead of bundling them statically.

## What it tests

- `openssl` crate linking against sysroot `libssl.so.3` / `libcrypto.so.3`
- `rusqlite` crate linking against sysroot `libsqlite3.so`
- Correct `PKG_CONFIG_*` and `--sysroot` propagation by cargo-tizen

## Build

```bash
cargo tizen build -A aarch64
cargo tizen rpm  -A aarch64
```

## Why this matters

Many Rust crates default to vendored/bundled builds for OpenSSL and SQLite.
Tizen provides these as platform libraries (guaranteed present on device),
so linking against the system versions produces smaller binaries and picks
up platform security updates automatically.

The key is to **not** enable the `vendored` (openssl) or `bundled` (rusqlite)
features — cargo-tizen sets up the sysroot environment so `pkg-config` and
the linker find the platform copies.

## Expected runtime output

```
openssl: OpenSSL 3.0.16 ...
sqlite: version=3.50.2, roundtrip=hello from syslibs
```
