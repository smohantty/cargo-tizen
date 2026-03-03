# Install and Configure Tizen SDK

`cargo-tizen` uses the installed Tizen SDK rootstraps as sysroot input for cross-compilation.

## Recommended SDK source

Install the latest Tizen SDK / VS Code Extension for Tizen from:

- https://samsungtizenos.com/tools-download/

Reference docs:

- https://samsungtizenos.com/docs/sdktools/

## Install required packages

In Tizen Package Manager, install Native CLI and Native App Development packages for your target platform/profile.

At minimum, ensure you have rootstraps for the platform version and profile/arch you build against.

Typical examples:

- `iot-headed` / `mobile` / `tizen` profile rootstraps for `arm` and/or `aarch64`
- platform versions such as `6.0`, `8.0`, `9.0` based on your target

## Configure SDK path for cargo-tizen

Choose one:

1. Environment variable:

```sh
export TIZEN_SDK=/path/to/tizen-sdk
```

2. Project config (`.cargo-tizen.toml`):

```toml
[sdk]
root = "/path/to/tizen-sdk"
```

3. One-time setup flag:

```sh
cargo tizen setup -A armv7l --sdk-root /path/to/tizen-sdk
```

## Verify SDK and rootstrap availability

```sh
cargo tizen doctor -A armv7l
cargo tizen doctor -A aarch64
```

## Rootstrap resolution behavior

`cargo-tizen` resolves rootstraps from:

```text
<sdk>/platforms/tizen-<platform-version>/<profile>/rootstraps/<profile>-<platform-version>-<type>.core
```

Profile normalization:

- `common` + `< 8.0` -> `iot-headed`
- `common` + `>= 8.0` -> `tizen`
- `mobile` + `>= 8.0` -> `tizen`
- `tv` -> `tv-samsung`

Fallback:

- If `tv-samsung` rootstrap is missing, fallback is:
  - `tizen` for `>= 8.0`
  - `iot-headed` for `< 8.0`

## Troubleshooting

If `doctor` shows:

- `unable to locate Tizen SDK`
  - set `TIZEN_SDK` or `[sdk].root`
- `rootstrap ... could not be found`
  - install matching Native App Development/rootstrap packages for your chosen profile/version/arch
- `linker not found`
  - install target toolchain package or set `[arch.<name>].linker` explicitly
