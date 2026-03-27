# Configure Tizen Device Connectivity

`cargo-tizen run` and `cargo-tizen devices` use `sdb` for device discovery, install, and launch.

## 1. Ensure `sdb` is available

If Tizen SDK is installed in a default location, `cargo-tizen` can auto-detect it.  
You can also set:

```sh
export TIZEN_SDK=/path/to/tizen-sdk
```

Verify:

```sh
sdb version
```

## 2. Connect a device

For network-connected devices/emulators:

```sh
sdb connect <ip:port>
```

Example:

```sh
sdb connect 192.168.0.101:26101
```

For USB-connected devices, ensure the device appears in `sdb devices`.

## 3. Verify device status

```sh
sdb devices
cargo tizen devices --all
```

Expected ready status is `device`.

## 4. Install app on device

```sh
cargo tizen install -A armv7l --cargo-release
```

If multiple devices are connected:

```sh
cargo tizen install -A armv7l -d <device-id> --cargo-release
```

If your packaging files live outside the default `tizen/` layout:

```sh
cargo tizen install -A armv7l --cargo-release --packaging-dir ./packaging
```

