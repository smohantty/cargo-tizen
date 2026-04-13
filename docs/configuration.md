# Configuration

`cargo-tizen` reads two config layers:

1. user config
2. project config

The project file overrides the user file when both set the same value.

Current file locations:

- user config: `~/.config/cargo-tizen/config.toml` on Linux
- project config: `.cargo-tizen.toml` in the workspace root

`gh-release` is stricter than normal commands and reads project config only.

## Starter file

`cargo tizen init` writes:

```toml
[default]
arch = "aarch64"
profile = "mobile"
platform_version = "10.0"

[package]
name = "my-app"
packages = ["my-app"]
```

## Sections

### `[default]`

Supported keys:

- `arch`
- `profile`
- `platform_version`
- `provider`
- `packaging_dir`

Current defaults when omitted:

- `profile = "mobile"`
- `platform_version = "10.0"`
- `provider = "rootstrap"`
- `packaging_dir = "tizen"` for packaging commands

`arch` has no implicit default beyond the auto-selection rules described in [getting-started.md](getting-started.md).

### `[package]`

Supported keys:

- `name`
- `packages`

Use this section to define the packaging group:

```toml
[package]
name = "my-suite"
packages = ["server", "cli"]
```

- `name` controls RPM spec lookup and the release artifact name for `gh-release`.
- `packages` controls which crates are built and staged for RPM packaging and release.
- Empty, duplicate, or path-like package names are rejected.

### `[arch.armv7l]` and `[arch.aarch64]`

Supported keys:

- `rust_target`
- `linker`
- `cc`
- `cxx`
- `ar`
- `tizen_cli_arch`
- `tizen_build_arch`
- `rpm_build_arch`

The code intentionally keeps these architecture mappings separate. Do not collapse them into a single shared value.

Default arch mapping:

| Arch | Rust target | Tizen CLI arch | Tizen build arch | RPM build arch |
|---|---|---|---|---|
| `armv7l` | `armv7-unknown-linux-gnueabi` or inferred hard-float | `arm` | `armel` | `armv7l` |
| `aarch64` | `aarch64-unknown-linux-gnu` | `aarch64` | `aarch64` | `aarch64` |

### `[sdk]`

Supported keys:

- `root`

Use this to pin Tizen Studio when auto-discovery is not enough.

### `[cache]`

Supported keys:

- `root`

Default cache location:

- `~/.cache/cargo-tizen/sysroots` on Linux when `dirs::cache_dir()` is available

### `[tpk]`

Supported keys:

- `sign`

You can set this through the CLI:

```bash
cargo tizen config --sign my_profile
```

When `--sign` is omitted for `tpk` or `install`, the stored value is used if present.

### `[release]`

Supported keys:

- `arches`
- `tag_format`

Example:

```toml
[release]
arches = ["armv7l", "aarch64"]
tag_format = "v{version}"
```

Rules enforced by the current implementation:

- `arches` must not be empty
- every entry must resolve to `armv7l` or `aarch64`
- `tag_format` must contain `{version}`

For backward compatibility, the parser also accepts `gh_release` as an alias for `release`.

## `cargo tizen config`

Current behavior:

- `cargo tizen config --sign <profile>` writes user config
- `cargo tizen config --sign ""` clears `tpk.sign`
- when `--sign` is omitted, the command prints the current merged configuration
- `--show` prints the merged configuration; when combined with `--sign`, shows config after the write
- all config sections are displayed: `[default]`, `[package]`, `[sdk]`, `[cache]`, `[tpk]`, `[release]`, and per-arch overrides
