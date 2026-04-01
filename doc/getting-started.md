# Getting Started with cargo-tizen

This guide walks you through building and deploying a Rust application on a Tizen device. Pick the path that matches your situation.

## Prerequisites (all paths)

Before starting, ensure you have:

1. **Rust toolchain** — `cargo`, `rustc`, `rustup`
2. **Tizen SDK** — see [install-tizen-sdk.md](install-tizen-sdk.md)
3. **Cross-compiler** — `gcc-arm-linux-gnueabi` (armv7l) or `gcc-aarch64-linux-gnu` (aarch64)
4. **cargo-tizen** installed:

```sh
cd /path/to/cargo-tizen
cargo install --path .
```

5. **Rust cross-compilation targets**:

```sh
rustup target add armv7-unknown-linux-gnueabi armv7-unknown-linux-gnueabihf aarch64-unknown-linux-gnu
```

Run `cargo tizen doctor` to check everything at once. Run `cargo tizen fix` to auto-install missing Rust targets and prepare sysroots.

---

## Path A: Existing Rust project, need RPM

Use this path when you have a Rust binary crate and need to produce an RPM for Tizen platform deployment.

### Step 1: Initialize packaging

```sh
cd /path/to/your-rust-project
cargo tizen init --rpm
```

This creates `.cargo-tizen.toml` (if missing) and `tizen/rpm/<package-name>.spec`.

**If this step fails:**
- "failed to determine package name" → pass `-p <member>` for workspace projects
- directory permission errors → check write access to your project root

### Step 2: Edit the spec file

Open `tizen/rpm/<package-name>.spec` and verify:
- `Name`, `Version`, `Summary` match your app
- `%install` section has the correct binary install path
- `%files` lists all files your app needs on the device

### Step 3: Check prerequisites

```sh
cargo tizen doctor
```

**If doctor reports issues:**
- "unable to locate Tizen SDK" → set `TIZEN_SDK=/path/to/sdk` or add `[sdk].root` to `.cargo-tizen.toml`
- "rootstrap could not be found" → install rootstrap packages in SDK Package Manager
- "linker not found" → `sudo apt install gcc-arm-linux-gnueabi` (armv7l) or `gcc-aarch64-linux-gnu` (aarch64)
- "rpmbuild not found" → `sudo apt install rpm`
- "Rust target not installed" → run `cargo tizen fix`

### Step 4: Cross-build

```sh
cargo tizen build -A armv7l --release
```

Replace `armv7l` with `aarch64` if your target device uses a 64-bit ARM processor.

**If build fails:**
- "compiler is unusable" → your cross-compiler has broken include paths. Set `[arch.<arch>].linker` in `.cargo-tizen.toml`
- sysroot resolution error → run `cargo tizen setup -A armv7l` separately to diagnose

### Step 5: Package as RPM

```sh
cargo tizen rpm -A armv7l --release
```

The RPM file path is printed on success.

**If packaging fails:**
- "spec missing" → run `cargo tizen init --rpm`
- rpmbuild errors → check your spec file syntax

### Step 6: Deploy

Install the RPM on your Tizen device using `sdb` or your deployment tool:

```sh
sdb push <path-to-rpm> /tmp/
sdb shell rpm -i /tmp/<package>.rpm
```

---

## Path B: Existing Rust project, need TPK

Use this path when you have a Rust binary crate and need to produce a signed TPK for Tizen app deployment.

### Step 1: Initialize packaging

```sh
cd /path/to/your-rust-project
cargo tizen init --tpk
```

This creates `.cargo-tizen.toml` (if missing) and `tizen/tpk/tizen-manifest.xml`.

### Step 2: Set up signing

Create a certificate profile in Tizen Studio (Tools > Certificate Manager), then:

```sh
cargo tizen config --sign my_profile
```

**If you skip this:** TPK packaging will fail with a signing error. You need a signing profile.

### Step 3: Edit the manifest

Open `tizen/tpk/tizen-manifest.xml` and verify:
- `package` attribute has your app ID (e.g., `org.example.myapp`)
- `appid` matches the package attribute
- `exec` matches your binary name (must match `[package].name` in `Cargo.toml`)
- `profile` and `api-version` match your target device

### Step 4: Check prerequisites

```sh
cargo tizen doctor
```

See Path A, Step 3 for troubleshooting doctor output.

### Step 5: Cross-build

```sh
cargo tizen build -A armv7l --release
```

**If build fails:** see Path A, Step 4 troubleshooting.

### Step 6: Package as TPK

```sh
cargo tizen tpk -A armv7l --release
```

**If packaging fails:**
- "manifest missing" → run `cargo tizen init --tpk`
- signing error → check your signing profile with `cargo tizen config --show`

### Step 7: Install on device

```sh
cargo tizen install -A armv7l --release
```

If multiple devices are connected, specify one:

```sh
cargo tizen devices
cargo tizen install -A armv7l --release -d <device-id>
```

**If install fails:**
- "no device found" → check `sdb devices`, run `sdb connect <ip:port>` for network devices
- see [configure-device.md](configure-device.md) for device setup

---

## Path C: Starting a new Rust project for Tizen

Use this path when starting from scratch.

### Step 1: Create the Rust project

```sh
cargo new my-tizen-app
cd my-tizen-app
```

### Step 2: Initialize cargo-tizen

For RPM packaging:

```sh
cargo tizen init --rpm
```

For TPK packaging:

```sh
cargo tizen init --tpk
```

For both:

```sh
cargo tizen init --rpm --tpk
```

### Step 3: Check and fix prerequisites

```sh
cargo tizen doctor
cargo tizen fix
```

`fix` installs missing Rust targets and prepares sysroot caches automatically.

**If doctor reports issues:** see Path A, Step 3 for troubleshooting.

### Step 4: Edit packaging files

- RPM: edit `tizen/rpm/my-tizen-app.spec` (see Path A, Step 2)
- TPK: edit `tizen/tpk/tizen-manifest.xml` (see Path B, Step 3)
- TPK signing: run `cargo tizen config --sign my_profile` (see Path B, Step 2)

### Step 5: Build and package

For RPM:

```sh
cargo tizen build -A armv7l --release
cargo tizen rpm -A armv7l --release
```

For TPK:

```sh
cargo tizen build -A armv7l --release
cargo tizen tpk -A armv7l --release
cargo tizen install -A armv7l --release
```

---

## General troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| "unable to locate Tizen SDK" | SDK not found | Set `TIZEN_SDK` env var or `[sdk].root` in `.cargo-tizen.toml` |
| "rootstrap could not be found" | Missing SDK packages | Install rootstrap packages in SDK Package Manager |
| "linker not found" | Missing cross-compiler | `sudo apt install gcc-arm-linux-gnueabi` or `gcc-aarch64-linux-gnu` |
| "compiler is unusable" | Broken cross-compiler paths | Set `[arch.<arch>].linker` in `.cargo-tizen.toml` |
| "rpmbuild not found" | Missing RPM tooling | `sudo apt install rpm` |
| "spec missing" | No RPM spec file | Run `cargo tizen init --rpm` |
| "manifest missing" | No TPK manifest | Run `cargo tizen init --tpk` |
| "no device found" | Device not connected | Check `sdb devices`, run `sdb connect <ip:port>` |
| Build fails during sysroot | Sysroot issue | Run `cargo tizen setup -A <arch>` to diagnose |

## What's next

- See [commands.md](commands.md) for the full command reference
- See [quick-reference.md](quick-reference.md) for a one-page cheat sheet
- See [packaging-layout.md](packaging-layout.md) for the packaging file layout
- Run `cargo tizen <command> --help` for command-specific help
