# Install for Linux

Linux is the documented host path in this repository.

## Rust

Install Rust and `rustup`:

```bash
curl https://sh.rustup.rs -sSf | sh
```

Install target triples:

```bash
rustup target add armv7-unknown-linux-gnueabi armv7-unknown-linux-gnueabihf aarch64-unknown-linux-gnu
```

For `armv7l`, keep both ARM targets installed. With the default `rootstrap` provider, `cargo-tizen` picks soft-float or hard-float based on the selected sysroot headers.

## Cross linkers

```bash
sudo apt install gcc-arm-linux-gnueabi gcc-aarch64-linux-gnu
```

Default linker names:

- `armv7l`: `arm-linux-gnueabi-gcc`
- `aarch64`: `aarch64-linux-gnu-gcc`

Override them only when your toolchain uses different names or paths.

## RPM tooling

Install this only if you need RPM packaging:

```bash
sudo apt install rpm
```

## Tizen Studio and rootstraps

Install [Tizen Studio](https://developer.tizen.org/development/tizen-studio/download), then install the rootstrap packages for the profile and platform version you want to target.

SDK discovery order in the current code:

1. `[sdk].root` from config or `--sdk-root` on `setup`
2. `TIZEN_SDK`
3. parent directory of `sdb` on `PATH`
4. standard install locations such as `~/tizen-studio`

If discovery fails, either set `[sdk].root` in `.cargo-tizen.toml`, set `TIZEN_SDK`, or run:

```bash
cargo tizen setup --sdk-root /path/to/tizen-studio
```

## Verify

```bash
cargo tizen doctor
cargo tizen fix
```

## Device access

For device workflows:

```bash
sdb devices
sdb connect <ip:port>
cargo tizen devices --all
```

`cargo-tizen` uses `sdb` to discover devices and install TPKs.
