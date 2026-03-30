# Packaging Model (vs flutter-tizen)

## Upstream links

- Repo: https://github.com/flutter-tizen/flutter-tizen
- Docs: https://github.com/flutter-tizen/flutter-tizen/tree/master/doc

This document exists so `cargo-tizen` can be kept aligned with upstream `flutter-tizen` design changes where relevant to Tizen SDK/rootstrap/device behavior.

## What flutter-tizen does

`flutter-tizen` primarily produces **TPK** packages for apps.

Observed flow:

1. Build native app with Tizen CLI/rootstrap settings.
2. Produce `.tpk` artifact.
3. Run `tizen package -t tpk` for packaging/signing.

Notes:

- Their docs and command set are TPK-centric.
- `.rpm` appears in their code as a temporary file name in one run/debug path, not as the main app packaging output.

## What cargo-tizen does

`cargo-tizen` supports both backends:

- **RPM backend** (`cargo tizen rpm`)
- **TPK backend** (`cargo tizen tpk`)
- **Device install flow** (`cargo tizen devices`, `cargo tizen install`)

RPM flow:

1. Resolve rootstrap and prepare sysroot cache.
2. Validate the authored spec and optional `rpm/sources/` layout.
3. Cross-build Rust binary with Cargo.
4. Stage files.
5. Build RPM via `rpmbuild`.

TPK flow:

1. Resolve rootstrap and prepare sysroot cache.
2. Validate the authored manifest and optional TPK directories from the packaging layout.
3. Cross-build Rust binary with Cargo.
4. Stage binary + authored `tizen-manifest.xml` from the packaging layout.
5. Invoke `tizen package -t tpk`.

Current packaging layout:

- Default packaging root: `<workspace>/tizen`
- RPM spec: `<packaging-root>/rpm/<cargo-package-name>.spec`
- TPK manifest: `<packaging-root>/tpk/tizen-manifest.xml`
- Optional TPK reference dir: `<packaging-root>/tpk/reference`
- Optional TPK extra dir: `<packaging-root>/tpk/extra`

If the required packaging files are missing, `cargo-tizen` fails with a gap message instead of generating placeholder files.

Run flow:

1. Discover devices via `sdb devices`.
2. Filter ready Tizen devices via `sdb capability`.
3. Install TPK with `sdb install`.
4. Launch app via `app_launcher -e` (or secure protocol command).

## Similar techniques adopted from flutter-tizen

Even though package format differs (`TPK` vs `RPM`), these techniques are aligned:

- deterministic rootstrap resolution by profile/version/arch
- fallback policy for unavailable profile-specific rootstraps
- separate arch naming per consumer (toolchain vs CLI vs package)
- centralized environment injection for native/cross tooling
- actionable doctor diagnostics for missing SDK/packages

## Why keep both in cargo-tizen

Project goals include Rust-native packaging for Tizen in environments that need either RPM or TPK distribution.  
`cargo-tizen` keeps both backends while reusing proven rootstrap/toolchain setup patterns from `flutter-tizen`.
