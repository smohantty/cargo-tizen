# Packaging Layout

`cargo-tizen` expects packaging files in a standard source layout. Use `cargo tizen init` to scaffold starter files, then edit them to match your app before packaging.

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
- `cargo tizen init` creates `.cargo-tizen.toml` by default. Add `--rpm` and/or `--tpk` when you want starter packaging files in the standard layout. Existing packaging files are skipped unless `--force` is passed.

If the expected file is missing, the command fails and prints the exact path it expected plus the `--packaging-dir` escape hatch.

## Migration note

Older versions accepted:

- `<workspace>/tizen-manifest.xml`
- `<workspace>/tizen/tizen-manifest.xml`

Those locations are no longer loaded automatically. Move the manifest to `<packaging-dir>/tpk/tizen-manifest.xml`.

## Multi-package RPM

Workspaces with multiple binary crates can bundle all binaries into a single RPM.
Set `[package].name` and `[package].packages` in `.cargo-tizen.toml`:

```toml
[package]
name = "my-project"
packages = ["my-server", "my-cli"]
```

`name` controls the spec filename lookup (`<packaging-dir>/rpm/my-project.spec`).
When omitted, it defaults to the first entry in `packages`. The `packages` list
controls which crates are built and staged. The spec must reference all staged
binaries as sources. CLI `-p` overrides to single-package mode.

Single-crate projects get both fields set to the crate name by `cargo tizen init`.

## Current gaps

- The tool packages the binary named after `[package].name`. Custom `[[bin]]` names are not supported.
- `doctor` reports packaging readiness, but it does not create or repair packaging files.
- `clean` removes build outputs under `target/`; it does not remove source packaging files under the packaging root.

## Reference projects

The repo includes example Cargo projects that also act as regression fixtures:

- `templates/reference-projects/rpm-app` — minimal binary-only RPM
- `templates/reference-projects/rpm-service-app` — RPM with extra sources (systemd unit, env file)
- `templates/reference-projects/rpm-multi-package` — workspace with 2 binary crates + 1 library, multi-binary RPM via `[package].packages`
- `templates/reference-projects/tpk-service-app`
