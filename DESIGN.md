# cargo-tizen Design

## 1. Overview

`cargo-tizen` is a Cargo subcommand for building Rust projects for Tizen targets and producing RPM/TPK packages.

Primary outcomes:
- Cross-compile Rust binaries for Tizen architectures (`armv7l`, `aarch64`).
- Provision and cache sysroots so subsequent builds are fast and repeatable.
- Package build artifacts into RPMs or TPKs suitable for Tizen deployment.

Invocation model:
- Binary name: `cargo-tizen`
- User command: `cargo tizen ...`

## 1.1 Implementation Status (Current)

Implemented:
- CLI scaffold for `setup`, `build`, `rpm`, `tpk`, `devices`, `run`, `doctor`, `clean`.
- `ArchMap` defaults for Rust target, Tizen CLI arch, Tizen build arch, and RPM build arch.
- Rootstrap-based sysroot provisioning from installed Tizen SDK rootstraps.
- Rootstrap fallback policy for missing `tv-samsung` rootstraps.
- Sysroot cache with metadata, lock, and atomic finalize.
- Centralized `ToolEnv` for build/rpm/tpk subprocess environments.
- Build output isolation under `target/tizen/<arch>/cargo`.
- RPM staging/rpmbuild isolation under `target/tizen/<arch>/<debug|release>/...`.
- TPK staging under `target/tizen/<arch>/<debug|release>/tpk/root` and packaging via `tizen package -t tpk`.
- Device discovery via `sdb devices` with Tizen capability verification.
- Device run flow (`cargo tizen run`) with auto device selection, `sdb install`, and launch.
- Doctor checks for toolchain, SDK detection, rootstrap availability, rust target availability, and cache readiness.

Not implemented yet:
- `repo` provider internals (returns explicit "not implemented" error).
- GBS backend.
- Full integration test suite.
- Multi-bin target selection (RPM/TPK currently stage `<package.name>` binary path).


## 2. Goals and Non-Goals

### 2.1 Goals
- Provide a predictable CLI for setup, build, packaging, and environment checks.
- Support sysroot acquisition/generation and local caching keyed by platform+arch source.
- Use Cargo internally for compilation.
- Produce RPMs via `rpmbuild` and TPKs via Tizen CLI packaging.
- Keep architecture mapping explicit and overridable.

### 2.2 Non-Goals (Phase 1)
- Full Tizen Studio replacement.
- Complete emulation/device deployment workflow.
- Full support for all Tizen profiles/platform variants on day one.
- Automatic patching of third-party crate build scripts for native dependencies.


## 3. Supported Targets and Mapping

`cargo-tizen` uses Tizen-facing architecture names and keeps separate mappings per consumer.

| `-A/--arch` | Rust target | Tizen CLI arch | Tizen build arch | RPM build arch | Typical linker (default) |
|---|---|---|---|---|---|
| `armv7l` | `armv7-unknown-linux-gnueabihf` | `arm` | `armel` | `armv7l` | `arm-linux-gnueabi-gcc` |
| `aarch64` | `aarch64-unknown-linux-gnu` | `aarch64` | `aarch64` | `aarch64` | `aarch64-linux-gnu-gcc` |

Notes:
- Linker/toolchain names vary by environment; defaults are configurable.
- Target mapping defaults are fixed in code and can be overridden through config.


## 4. CLI Specification

## 4.1 Top-Level

```bash
cargo tizen <SUBCOMMAND> [OPTIONS]
```

Global options:
- `-v, --verbose`
- `-q, --quiet`
- `--config <PATH>` (explicit config file override)

## 4.2 `setup`

```bash
cargo tizen setup -A <armv7l|aarch64> [--profile <name>] [--platform-version <ver>] [--provider <rootstrap|repo>] [--sdk-root <path>]
```

Purpose:
- Acquire/generate sysroot.
- Validate sysroot completeness.
- Save to cache for reuse.

Options:
- `-A, --arch`: required.
- `--profile`: default from config (for example `mobile`).
- `--platform-version`: default from config.
- `--provider`: sysroot acquisition strategy.
- `--sdk-root`: per-invocation Tizen SDK root override.
- `--force`: refresh cached entry even if valid.

## 4.3 `build`

```bash
cargo tizen build -A <armv7l|aarch64> [--release] [--target-dir <path>] [-- <cargo_build_args...>]
```

Purpose:
- Resolve sysroot from cache (or fail with setup guidance).
- Execute `cargo build` with target, linker, sysroot, and pkg-config environment.

Behavior:
- Uses `cargo build --target <triple>`.
- Sets target-specific linker/rustflags.
- Keeps output isolated under Tizen target directory by default.

## 4.4 `rpm`

```bash
cargo tizen rpm -A <armv7l|aarch64> [--cargo-release] [--release <n>] [--spec <path>] [--output <dir>] [--no-build]
```

Purpose:
- Build (unless `--no-build`) then package into RPM.

Behavior:
- Stage artifacts.
- Generate spec if missing.
- Invoke `rpmbuild` with an isolated `_topdir`.
- Emit resulting RPM path(s).

## 4.5 `devices`

```bash
cargo tizen devices [--all]
```

Purpose:
- Discover connected devices from `sdb`.
- Report ready Tizen devices and (optionally) offline/unauthorized entries.

Behavior:
- Parses `sdb devices` output.
- Verifies Tizen targets using `sdb -s <id> capability` (`cpu_arch` check).
- `--all` includes non-ready and non-Tizen entries.

## 4.6 `run`

```bash
cargo tizen run -A <armv7l|aarch64> [-d <device-id>] [--cargo-release] [--manifest <path>] [--output <dir>] [--sign <profile>] [--reference <path>] [--extra-dir <path>] [--no-build] [--tpk <path>] [--app-id <id>]
```

Purpose:
- Build/package (or reuse a prebuilt TPK), install to a device, and launch the app.

Behavior:
- If `--tpk` is omitted, runs the TPK packaging backend first.
- Auto-selects device when exactly one ready Tizen device is connected.
- Requires `-d/--device` when multiple devices are ready.
- Installs with `sdb -s <id> install <tpk>`.
- Launches with:
  - `sdb -s <id> shell app_launcher -e <app_id>` (normal devices)
  - `sdb -s <id> shell 0 execute <app_id>` (secure protocol devices)
- App ID is resolved from:
  1. `--app-id`
  2. manifest `appid`
  3. manifest `package` (fallback)

## 4.7 `doctor`

```bash
cargo tizen doctor [-A <armv7l|aarch64>]
```

Checks:
- Required executables (`cargo`, `rustc`, `rustup`, linker, `rpmbuild`).
- Tizen SDK detection (`TIZEN_SDK`, `sdb`, extension/CLI default locations).
- Rust targets installed.
- Rootstrap availability for selected profile/version/arch.
- Sysroot cache availability and validity.
- Config consistency.

## 4.8 `clean`

```bash
cargo tizen clean [--sysroot] [--build] [--all] [-A <armv7l|aarch64>]
```

Purpose:
- Remove build outputs and/or cached sysroots.

## 4.9 `tpk`

```bash
cargo tizen tpk -A <armv7l|aarch64> [--cargo-release] [--manifest <path>] [--output <dir>] [--sign <profile>] [--reference <path>] [--extra-dir <path>] [--no-build]
```

Purpose:
- Build (unless `--no-build`) then package into TPK via Tizen CLI.

Behavior:
- Stages binary and `tizen-manifest.xml`.
- Invokes `tizen package -t tpk`.
- Emits generated `.tpk` path(s).


## 5. Configuration Model

Config precedence (highest first):
1. CLI flags
2. Project config: `.cargo-tizen.toml`
3. User config: `~/.config/cargo-tizen/config.toml`
4. Built-in defaults

Example:

```toml
[default]
profile = "mobile"
platform_version = "9.0"
provider = "rootstrap"

[arch.armv7l]
rust_target = "armv7-unknown-linux-gnueabihf"
linker = "/opt/tizen/toolchains/armv7l/bin/arm-linux-gnueabi-gcc"
tizen_cli_arch = "arm"
tizen_build_arch = "armel"
rpm_build_arch = "armv7l"

[arch.aarch64]
rust_target = "aarch64-unknown-linux-gnu"
linker = "/opt/tizen/toolchains/aarch64/bin/aarch64-linux-gnu-gcc"
tizen_cli_arch = "aarch64"
tizen_build_arch = "aarch64"
rpm_build_arch = "aarch64"

[sdk]
root = "/home/you/tizen-studio"

[cache]
root = "~/.cache/cargo-tizen/sysroots"

[rpm]
packager = "Your Team <dev@example.com>"
license = "Apache-2.0"
```

Cargo project metadata extension:

```toml
[package.metadata.tizen]
name = "my-app"
release = "1"
summary = "My Tizen app"

[[package.metadata.tizen.install]]
source = "target-binary"
dest = "/usr/bin/my-app"
mode = "0755"
```

Note:
- `package.metadata.tizen` is currently documented as planned schema; the current implementation does not consume this metadata yet.


## 6. Sysroot Provider and Cache

## 6.1 Provider Interface

Trait-level contract:
- `kind() -> ProviderKind`
- `fingerprint(request) -> String`
- `prepare(request, sysroot_dir) -> Result<()>`

Provider implementations:
- `rootstrap`: resolve installed SDK rootstraps, apply fallback policy, and materialize sysroot cache entries.
- `repo`: currently placeholder and returns explicit "not implemented" error.

## 6.2 Cache Layout

Default root:
- `~/.cache/cargo-tizen/sysroots`

Current path:
- `<cache_root>/<profile>/<platform_version>/<arch>/<provider>/<fingerprint>/`

Files:
- `sysroot/` (headers/libs/crt/pkgconfig)
- `meta.json` (created timestamp, arch/profile/platform/provider/fingerprint)
- `state` (currently `ready` is used)
- `.lock` (advisory lock for concurrent setup)

## 6.3 Cache Key

Current key dimensions:
- provider
- profile
- platform version
- architecture
- provider fingerprint (rootstrap candidate IDs and fallback tuple)

Cache reuse policy:
- Reuse only `ready` entries.
- If validation fails, mark invalid and rebuild.
- `--force` bypasses reuse and refreshes.

## 6.4 Validation Rules

Current minimum checks:
- C runtime objects exist (`crt1.o` or `Scrt1.o`, plus `crti.o`).
- `libc.so` exists.
- include directory exists (`usr/include`).
- library directory exists (`usr/lib` or `usr/lib64` or `lib` or `lib64`).
- pkg-config directory exists (`usr/lib/pkgconfig` or `usr/lib64/pkgconfig` or `usr/share/pkgconfig`).

## 6.5 Rootstrap Resolution Policy

SDK discovery order:
- `setup --sdk-root`
- `[sdk].root` in config
- `TIZEN_SDK` environment variable
- parent of detected `sdb` path
- `~/.tizen-extension-platform/server/sdktools/data`
- `~/tizen-studio`

Profile normalization policy:
- `common` + `>= 8.0` -> `tizen`
- `common` + `< 8.0` -> `iot-headed`
- `tv` -> `tv-samsung`
- `mobile` + `>= 8.0` -> `tizen`

Fallback policy:
- if `tv-samsung-<version>-<type>.core` is missing:
  - fallback to `tizen-<version>-<type>.core` for `>= 8.0`
  - fallback to `iot-headed-<version>-<type>.core` for `< 8.0`


## 7. Build Pipeline

Given `cargo tizen build -A armv7l`:
1. Load and merge configuration.
2. Map arch -> Rust target, Tizen CLI arch, Tizen build arch, and RPM build arch.
3. Ensure sysroot exists and validates.
4. Ensure Rust target is installed (with `rustup target add` guidance when missing).
5. Resolve toolchain binaries (linker, cc, cxx, ar) from config, PATH, or SDK tool directories.
6. Construct execution environment via `ToolEnv`:
   - `CC_<target>`
   - `CXX_<target>`
   - `AR_<target>`
   - `CARGO_TARGET_<TRIPLE>_LINKER`
   - `CARGO_TARGET_<TRIPLE>_RUSTFLAGS`
   - `PKG_CONFIG_SYSROOT_DIR`
   - `PKG_CONFIG_LIBDIR`
   - `PKG_CONFIG_ALLOW_CROSS=1`
   - `PATH` augmentation with SDK/toolchain directories
   - `USER_CPP_OPTS=-std=c++17`
7. Run `cargo build --target <triple> --target-dir target/tizen/<arch>/cargo ...`.
8. Stream build output and return command success/failure.

Implementation detail:
- Prefer ephemeral `CARGO_TARGET_<TRIPLE>_*` env overrides over mutating user `.cargo/config.toml`.


## 8. RPM Packaging Pipeline

Given `cargo tizen rpm -A aarch64`:
1. Run build phase unless `--no-build`.
2. Create staging root: `target/tizen/<arch>/<debug|release>/stage/`.
3. Copy built binary to `/usr/bin/<package_name>` in staging.
4. Resolve spec:
   - use `--spec` if provided
   - otherwise generate via built-in minimal spec renderer
5. Prepare rpmbuild tree:
   - `target/tizen/<arch>/<debug|release>/rpmbuild/{BUILD,RPMS,SOURCES,SPECS,SRPMS}`
6. Invoke:
   - `rpmbuild -bb <spec> --target <rpm_arch> --define "_topdir <...>"`
   - with SDK-aware PATH augmentation
7. Emit generated package path(s).

Default `rpm_arch` mapping:
- `armv7l` -> `armv7l` (config-overridable)
- `aarch64` -> `aarch64` (config-overridable)

Spec generation minimum fields:
- `Name`, `Version`, `Release`, `Summary`, `License`
- `%description`
- `%files`

## 8.1 TPK Packaging Pipeline

Given `cargo tizen tpk -A aarch64`:
1. Run build phase unless `--no-build`.
2. Create staging root: `target/tizen/<arch>/<debug|release>/tpk/root/`.
3. Stage application binary to `bin/<package_name>`.
4. Stage `tizen-manifest.xml` from:
   - `--manifest` if provided
   - `<workspace>/tizen-manifest.xml`
   - `<workspace>/tizen/tizen-manifest.xml`
5. Invoke:
   - `tizen package -t tpk -o <output_dir> -- <staging_root>`
   - optional `-s <sign_profile>`
   - optional `-r <reference>`
   - optional `-e <extra_dir>`
6. Emit generated `.tpk` artifact path(s).

Notes:
- TPK backend depends on Tizen CLI availability from detected SDK or PATH.
- Unlike flutter-tizen, this backend packages Rust binary layout directly from Cargo outputs.

## 8.2 Device Run Pipeline

Given `cargo tizen run -A armv7l`:
1. Discover devices via `sdb devices`.
2. Keep entries with state `device`.
3. Verify Tizen targets with `sdb -s <id> capability` (`cpu_arch` must exist).
4. Select target device:
   - single ready device -> auto-select
   - multiple ready devices -> require `-d`
5. Determine package:
   - use `--tpk` if provided
   - otherwise run TPK packaging flow
6. Resolve app ID (`--app-id` > manifest `appid` > manifest `package`).
7. Install using `sdb -s <id> install <tpk>` (with flutter-tizen-like failure string checks).
8. Launch app with `app_launcher -e` or secure protocol `0 execute`.


## 9. Error Model and Diagnostics

Error classes:
- `ConfigError`: invalid/missing config values.
- `ToolMissing`: required external command not found.
- `SysrootProvisionError`: provider failed to prepare sysroot.
- `SysrootValidationError`: missing/invalid sysroot contents.
- `BuildError`: cargo or linker failure.
- `PackagingError`: spec or rpmbuild failure.
- `DeployError`: device discovery/install/launch failure.

Diagnostics principles:
- Show failing command and exit code.
- Show exact cache/config path involved.
- Include one actionable fix suggestion.
- For SDK/rootstrap failures, include remediation text based on detected SDK flavor (CLI vs extension).

Example:
- `ToolMissing: rpmbuild not found. Install rpm-build package and retry.`


## 10. Security and Reproducibility (Planned Hardening)

- Current:
  - cache reuse is explicit (`ready` state + validation)
  - cache refresh is explicit via `setup --force`
- Planned:
  - provider input verification using checksums/signatures where available
  - richer provenance in `meta.json` (URLs/digest/tool versions)
  - explicit offline policy flags


## 11. Concurrency and Robustness

- Use per-cache-key lock files to prevent duplicate setup work.
- Write into temp dir then atomically rename to final cache path.
- Never treat partially written sysroot as valid.
- Stale lock cleanup heuristics are not implemented yet.


## 12. Testing Strategy (Current + Planned)

### 12.1 Current Unit Tests
- Arch mapping stability.
- Rootstrap profile mapping/fallback candidate generation.
- Cache sibling suffix/temp naming with dotted fingerprints.

### 12.2 Planned Unit Tests
- Arch mapping logic.
- Config precedence resolution.
- Cache key generation and path resolution.
- Spec template rendering.

### 12.3 Planned Integration Tests
- `setup` populates cache and creates metadata/state.
- repeated `build` hits cache.
- `rpm` creates expected package naming and arch.
- corrupted cache entry is detected and rebuilt.

### 12.4 Planned End-to-End Matrix
- `armv7l`, `aarch64`
- debug + release
- clean cache + warm cache scenarios

Use fixtures/mocks for provider paths where real Tizen artifacts are unavailable in CI.


## 13. Project Structure Proposal

```text
src/
  main.rs
  cli.rs
  config.rs
  arch.rs
  sdk.rs
  tool_env.rs
  doctor.rs
  cargo_runner.rs
  tpk.rs
  sysroot/
    mod.rs
    cache.rs
    validate.rs
    provider.rs
    rootstrap.rs
    repo.rs
  rpm/
    mod.rs
    spec.rs
    stage.rs
    rpmbuild.rs
templates/
  tizen.spec.hbs
tests/
  integration_setup.rs
  integration_build.rs
  integration_rpm.rs
```


## 14. Delivery Plan

## Milestone 0: Tooling Validation
- Validate arch/linker/sysroot assumptions on one sample app.
- Freeze default mapping and minimum dependency list.

## Milestone 1: Skeleton
- Create CLI commands and config loading.
- Add logging/error framework.

## Milestone 2: Setup + Cache
- Implement provider abstraction and rootstrap provider.
- Implement cache state, locking, and validation.

## Milestone 3: Build
- Integrate cargo invocation with env injection.
- Verify cross-compiled binaries for both arches.

## Milestone 4: RPM
- Add staging + spec generation + rpmbuild invocation.
- Produce installable RPM artifacts.

## Milestone 5: Hardening
- Add integration tests and failure-path diagnostics.
- Improve reproducibility metadata and offline behavior.


## 15. Open Decisions

- Should `setup` auto-install missing Rust targets (`rustup target add`) or only suggest commands?
- Should `repo` provider be implemented next or replaced by a GBS-first provider?
- When should `package.metadata.tizen` be wired into staging/spec generation?
- Whether to support a `gbs` backend in Phase 1 or defer to Phase 2.


## 16. First Implementation Slice (Recommended)

Smallest useful vertical slice:
1. `cargo tizen setup -A <arch>` with local cache + validation stub.
2. `cargo tizen build -A <arch>` using fixed linker/sysroot from config.
3. `cargo tizen rpm -A <arch>` with generated minimal spec and `rpmbuild`.
4. `cargo tizen tpk -A <arch>` with staged binary + `tizen-manifest.xml` packaging.

This slice is enough to prove the full workflow and then iterate on provider quality and packaging richness.


## 17. Flutter-Tizen Findings and Adaptation

This section captures concrete patterns observed in `flutter-tizen` and how to adapt them to `cargo-tizen`.

Observed patterns:
- Rootstrap selection is deterministic and derived from profile + API version + arch type (`device`, `device64`, `emulator`, `emulator64`).
- Rootstrap lookup is filesystem-driven under SDK platform directories and fails fast with actionable install guidance.
- Architecture naming is normalized per consumer:
  - one mapping for Tizen CLI (`arm`, `aarch64`, `x86`, `x86_64`)
  - one mapping for native builder/rpm naming (`armel`, `aarch64`, `i586`, `x86_64`)
- Native build is delegated to `tizen build-native` with explicit:
  - configuration (`-C`)
  - arch (`-a`)
  - compiler (`-c`)
  - rootstrap (`-r`)
  - macro predefines (`-d`)
  - extra options (`-e`)
- Build tool environments are injected centrally (`PATH`, `USER_CPP_OPTS`) rather than ad-hoc per command.
- Doctor checks are strict and include package-level remediation instructions.
- Build outputs are isolated by target arch and build config.
- Device discovery is based on `sdb devices` + capability probing.
- Install path uses `sdb install` with retry/failure-string guardrails.
- Launch path uses profile-aware commands (`app_launcher -e` vs secure `0 execute`).

Adaptation for `cargo-tizen`:
- Implemented: `ArchMap` with explicit consumer-specific names (`rust_target`, `tizen_cli_arch`, `tizen_build_arch`, `rpm_build_arch`).
- Implemented: `RootstrapResolver` with deterministic IDs from profile + version + arch type.
- Implemented: fallback policy for missing `tv-samsung` rootstrap.
- Implemented: `doctor` remediation templates by SDK flavor.
- Implemented: `ToolEnv` for PATH/compiler/pkg-config/sysroot environment construction.
- Implemented: output segmentation by arch and build profile.
- Implemented: `devices` command with capability-based filtering and diagnostics.
- Implemented: `run` command with auto-select or explicit `-d`, install, and launch.
- Pending: `repo` provider internals.

Implementation note:
- `flutter-tizen` is app/TPK-focused and invokes Tizen native builders directly.
- `cargo-tizen` remains Cargo-first for Rust compilation and supports both RPM and TPK packaging backends while borrowing rootstrap resolution, arch normalization, and diagnostics quality patterns.
