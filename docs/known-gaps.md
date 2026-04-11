# Known Gaps

These are current implementation boundaries, not hidden roadmap items.

- The `repo` sysroot provider is not implemented. It creates a placeholder working directory and then fails explicitly.
- Packaging expects the built binary name to match the selected package name.
- Custom `[[bin]]` names and multi-bin crate staging are not implemented.
- Multi-package grouping is supported for RPM packaging, not TPK packaging.
- `install` is TPK-only.
- `gh-release` is RPM-only.
- End-to-end packaging validation still depends on real Tizen SDK, rootstraps, host toolchains, and devices.
- `package.metadata.tizen` is not wired into the current packaging pipeline. The live contract is authored files under the packaging root.
