# Packaging Layout

`cargo-tizen` now expects authored packaging files in a standard source layout. It does not generate missing spec or manifest files for you.

## Standard layout

By default the packaging root is `<workspace>/tizen`:

```text
tizen/
  rpm/
    <cargo-package-name>.spec
    sources/      # optional, contents copied to rpmbuild SOURCES/
  tpk/
    tizen-manifest.xml
    reference/    # optional, passed to `tizen package -r`
    extra/        # optional, passed to `tizen package -e`
```

You can point to a non-standard root with:

```sh
cargo tizen rpm --packaging-dir /path/to/packaging
cargo tizen tpk --packaging-dir /path/to/packaging
cargo tizen install --packaging-dir /path/to/packaging
```

You can also persist that root in `.cargo-tizen.toml`:

```toml
[default]
packaging_dir = "./packaging"
```

## Command expectations

- `cargo tizen rpm` looks for `<packaging-dir>/rpm/<cargo-package-name>.spec`.
- `cargo tizen rpm` also checks for an optional `<packaging-dir>/rpm/sources/` directory. If present, all regular files inside are copied into `rpmbuild/SOURCES/` alongside the binary. Use this for systemd units, environment files, configs, or any other non-binary sources your spec references as `Source1:`, `Source2:`, etc. Dotfiles are skipped and symlinks are rejected.
- `cargo tizen tpk` looks for `<packaging-dir>/tpk/tizen-manifest.xml`.
- `cargo tizen install` is TPK-only. When `--tpk` is omitted, it uses the same TPK packaging layout as `cargo tizen tpk`.

If the expected file is missing, the command fails and prints the exact path it expected plus the `--packaging-dir` escape hatch.

## Migration note

Older versions accepted:

- `<workspace>/tizen-manifest.xml`
- `<workspace>/tizen/tizen-manifest.xml`

Those locations are no longer loaded automatically. Move the manifest to `<packaging-dir>/tpk/tizen-manifest.xml`.

## Current gaps

- The tool packages the binary named after `[package].name`. Multi-bin and renamed-bin packaging are not implemented yet.
- Multi-package workspaces must select a member with `-p/--package` or `[default].package` in `.cargo-tizen.toml`.
- `doctor` reports packaging readiness, but it does not create or repair packaging files.
- `clean` removes build outputs under `target/`; it does not remove source packaging files under the packaging root.

## Reference projects

The repo includes example Cargo projects that also act as regression fixtures:

- `templates/reference-projects/rpm-app` — minimal binary-only RPM
- `templates/reference-projects/rpm-service-app` — RPM with extra sources (systemd unit, env file)
- `templates/reference-projects/tpk-service-app`
