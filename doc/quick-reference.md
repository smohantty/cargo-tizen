# Quick Reference

## Commands

| Command | Usage | Description |
|---------|-------|-------------|
| `init` | `cargo tizen init [--rpm] [--tpk] [-p PKG] [--force]` | Scaffold config and packaging files |
| `doctor` | `cargo tizen doctor [-A ARCH]` | Check prerequisites |
| `fix` | `cargo tizen fix [-A ARCH]` | Install missing Rust targets and sysroots |
| `setup` | `cargo tizen setup [-A ARCH] [--profile P] [--platform-version V]` | Prepare sysroot cache |
| `build` | `cargo tizen build [-A ARCH] [--release] [-- CARGO_ARGS]` | Cross-build |
| `rpm` | `cargo tizen rpm [-A ARCH] [-p PKG] [--release] [--no-build]` | Package as RPM |
| `tpk` | `cargo tizen tpk [-A ARCH] [--release] [--sign PROF] [--no-build]` | Package as TPK |
| `install` | `cargo tizen install [-A ARCH] [-d DEV] [--release] [--sign PROF]` | Build + install TPK on device |
| `devices` | `cargo tizen devices [--all]` | List connected Tizen devices |
| `clean` | `cargo tizen clean [--build] [--sysroot] [--all] [-A ARCH]` | Remove build outputs or caches |
| `config` | `cargo tizen config [--sign PROF] [--show]` | View or set user-level settings |

## Architectures

| `-A` flag | Rust target | Cross-compiler package | Linker binary |
|-----------|-------------|----------------------|---------------|
| `armv7l` | `armv7-unknown-linux-gnueabi` | `gcc-arm-linux-gnueabi` | `arm-linux-gnueabi-gcc` |
| `aarch64` | `aarch64-unknown-linux-gnu` | `gcc-aarch64-linux-gnu` | `aarch64-linux-gnu-gcc` |

## Config files

| File | Scope | Location |
|------|-------|----------|
| `.cargo-tizen.toml` | Project | `<workspace-root>/.cargo-tizen.toml` |
| `config.toml` | User | `~/.config/cargo-tizen/config.toml` |

Precedence: CLI flags > project config > user config > built-in defaults.

## Output directories

```text
target/tizen/<arch>/
  cargo/<rust-target>/<debug|release>/    # build output (binaries)
  <debug|release>/
    stage/                                # staged binaries for RPM
    rpmbuild/{BUILD,RPMS,SOURCES,SPECS}   # RPM build tree
    tpk/root/                             # TPK staging root
    tpk/out/                              # generated .tpk files
```

## Packaging layout

```text
tizen/                           # default packaging root
  rpm/
    <package-name>.spec          # required for cargo tizen rpm
    sources/                     # optional extra RPM sources
  tpk/
    tizen-manifest.xml           # required for cargo tizen tpk
    reference/                   # optional, passed to tizen package -r
    extra/                       # optional, passed to tizen package -e
```

Override with `--packaging-dir` or `[default].packaging_dir` in `.cargo-tizen.toml`.

## Common workflows

```sh
# First-time setup
cargo tizen init --rpm          # or --tpk
cargo tizen doctor
cargo tizen fix

# Build and package RPM
cargo tizen build -A armv7l --release
cargo tizen rpm -A armv7l --release

# Build, package, and install TPK
cargo tizen install -A armv7l --release

# Multi-package RPM (set [rpm].packages in .cargo-tizen.toml)
cargo tizen rpm -A armv7l --release
```
