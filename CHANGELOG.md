# Changelog

All notable changes to `cargo-tizen` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [0.1.0] - 2026-04-01

Initial internal release.

### Added

**CLI**
- `cargo tizen init` — scaffold starter `.cargo-tizen.toml`, RPM spec, and TPK manifest files with `--rpm`, `--tpk`, and `--force` flags
- `cargo tizen setup` — prepare and cache Tizen sysroots with auto-selection of profile and platform version
- `cargo tizen build` — cross-build Rust projects with automatic sysroot provisioning and environment injection
- `cargo tizen rpm` — package built binaries as RPM using authored spec files
- `cargo tizen tpk` — package built binaries as signed TPK using Tizen CLI
- `cargo tizen install` — build, package, and install TPK on a connected Tizen device
- `cargo tizen devices` — list connected Tizen devices with capability verification
- `cargo tizen doctor` — check SDK, toolchain, sysroot, and packaging readiness with actionable fix suggestions
- `cargo tizen fix` — install missing Rust targets and prepare missing sysroots automatically
- `cargo tizen clean` — remove build outputs and/or cached sysroots
- `cargo tizen config` — view or set persistent user-level settings (TPK signing profile)
- Built-in help with examples and notes on every subcommand

**Build**
- Automatic sysroot provisioning from installed Tizen SDK rootstraps
- Rootstrap fallback policy for missing `tv-samsung` rootstraps
- ABI detection for `armv7l` (soft-float vs hard-float) from rootstrap headers
- Cross-compiler sanity probe to fail fast on broken toolchains
- OpenSSL fallback environment when sysroot has libs but no `.pc` files
- glibc `__has_include` compatibility flags for cross-compilation
- Build output isolation under `target/tizen/<arch>/cargo`

**Packaging**
- Multi-package RPM support via `[rpm].packages` config
- Extra RPM source files from `<packaging-dir>/rpm/sources/`
- Auto-generated `rpmrc` for cross-architecture RPM builds
- TPK signing profile support via `--sign` flag and `cargo tizen config --sign`
- TPK reference and extra directory support
- Default package selection for workspace packaging via `[default].package`
- Packaging input validation before build (fail fast on missing spec/manifest)

**Device**
- Device discovery via `sdb devices` with Tizen capability verification
- Auto-selection when exactly one ready device is connected
- Network device support via `sdb connect`

**Docs**
- Getting started guide with 3 paths (existing+RPM, existing+TPK, fresh project)
- Full command reference with flag descriptions and examples
- Quick reference card for daily use
- Tizen SDK installation guide
- Linux installation guide
- Device configuration guide
- Packaging layout documentation
- Reference projects: rpm-app, rpm-service-app, rpm-multi-package, tpk-service-app
