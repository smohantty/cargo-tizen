# Known Gaps

These are current implementation boundaries, not hidden roadmap items.

- The `repo` sysroot provider is not implemented. It creates a placeholder working directory and then fails explicitly.
- Packaging expects each entry in `[package].packages` to match the Cargo-built binary name (i.e., the crate's `[package].name` in its own `Cargo.toml`). The separate `[package].name` in `.cargo-tizen.toml` controls RPM spec lookup and `gh-release` artifact naming only — it does not remap binary names during staging.
- Custom `[[bin]]` names (where the binary output differs from the crate name) and crates that produce multiple binaries are not handled by staging. Each `packages` entry must resolve to exactly one binary with the same name.
- Multi-package grouping is supported for RPM packaging, not TPK packaging.
- `install` is TPK-only.
- `gh-release` is RPM-only.
- End-to-end packaging validation still depends on real Tizen SDK, rootstraps, host toolchains, and devices.
- `package.metadata.tizen` is not wired into the current packaging pipeline. The live contract is authored files under the packaging root.
