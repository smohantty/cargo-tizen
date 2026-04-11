# Packaging Layout

The current implementation expects authored packaging files under a packaging root.

Default packaging root:

- `tizen/`

Override it with:

- `[default].packaging_dir` in `.cargo-tizen.toml`
- `--packaging-dir` on `rpm`, `tpk`, or `install`

## Authored inputs

```text
<packaging-dir>/
  rpm/
    <package-name>.spec
    sources/
  tpk/
    tizen-manifest.xml
    reference/
    extra/
```

Rules enforced by the current code:

- RPM requires `<packaging-dir>/rpm/<package-name>.spec`.
- TPK requires `<packaging-dir>/tpk/tizen-manifest.xml`.
- `rpm/sources/` is optional. Its regular files are copied into `rpmbuild/SOURCES/`.
- `tpk/reference/` and `tpk/extra/` are optional and map to `tizen package -r` and `-e`.
- Legacy manifest locations such as `<workspace>/tizen-manifest.xml` are rejected with a move hint.

## Generated outputs

The implementation writes generated build state under `target/tizen/`.

Current paths:

- Cargo target dir: `target/tizen/<arch>/cargo/<rust-target>/<debug|release>/`
- RPM staging dir: `target/tizen/<arch>/<debug|release>/stage/`
- RPM build tree: `target/tizen/<arch>/<debug|release>/rpmbuild/`
- TPK temporary project and output: `target/tizen/<arch>/<debug|release>/tpk/`

`cargo tizen clean --build` removes generated build state but preserves authored files under `rpm/` and `tpk/`.

## Multi-package RPM

Multi-package RPMs are driven by config, not by implicit Cargo workspace grouping:

```toml
[package]
name = "my-suite"
packages = ["server", "cli"]
```

- `name` selects the spec file name: `<packaging-dir>/rpm/my-suite.spec`
- `packages` selects which built binaries are staged into the RPM

## Current Constraints

- Packaging commands do not generate missing authored files on demand. Use `cargo tizen init --rpm` and/or `cargo tizen init --tpk` first.
- The built binary name must currently match the selected package name.
- Custom `[[bin]]` names and multi-bin crate staging are not implemented.

## Reference Projects

- `templates/reference-projects/rpm-app`
- `templates/reference-projects/rpm-service-app`
- `templates/reference-projects/rpm-multi-package`
- `templates/reference-projects/tpk-service-app`
