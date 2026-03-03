# Packaging Model (vs flutter-tizen)

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

RPM flow:

1. Resolve rootstrap and prepare sysroot cache.
2. Cross-build Rust binary with Cargo.
3. Stage files.
4. Generate spec (or use provided spec).
5. Build RPM via `rpmbuild`.

TPK flow:

1. Resolve rootstrap and prepare sysroot cache.
2. Cross-build Rust binary with Cargo.
3. Stage binary + `tizen-manifest.xml`.
4. Invoke `tizen package -t tpk`.

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
