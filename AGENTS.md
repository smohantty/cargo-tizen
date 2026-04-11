# AGENTS.md

This repository keeps its source of truth in `docs/`. Keep this file short and use it as the root index for coding agents.

## Start Here

- `docs/README.md`: documentation map and maintenance rules
- `docs/architecture.md`: current architecture and behavioral contract
- `docs/commands.md`: command surface and command-specific notes
- `docs/configuration.md`: merged config model, defaults, and overrides
- `docs/packaging-layout.md`: authored packaging inputs and generated outputs
- `docs/known-gaps.md`: explicit unsupported or unfinished behavior
- `docs/agents/index.md`: agent-specific navigation
- `docs/agents/workflows.md`: task checklists and validation

## Working Rules

- Keep docs grounded in `src/*`, clap help, and tests. Do not document planned behavior as if it exists.
- Update docs in the same patch as behavior changes.
- Prefer deleting stale docs over layering a new doc on top of them.
- Keep `AGENTS.md` concise; move detail into `docs/`.
- `CLAUDE.md` is a compatibility shim and should point back here.
- `DESIGN.md` is a compatibility pointer to `docs/architecture.md`.

## Command Surface

- `cargo tizen init`
- `cargo tizen doctor`
- `cargo tizen fix`
- `cargo tizen build`
- `cargo tizen rpm`
- `cargo tizen tpk`
- `cargo tizen install`
- `cargo tizen devices`
- `cargo tizen setup`
- `cargo tizen clean`
- `cargo tizen config`
- `cargo tizen gh-release`

If command names, flags, defaults, or semantics change, update:

- `README.md`
- `docs/commands.md`
- `docs/getting-started.md`
- `docs/configuration.md` when config or defaults changed
- `docs/architecture.md`

## Core Code Map

- CLI and dispatch: `src/main.rs`, `src/cli.rs`
- Config loading and merge order: `src/config.rs`, `src/config_cmd.rs`
- Architecture selection: `src/arch.rs`, `src/arch_detect.rs`, `src/rust_target.rs`
- SDK and tool resolution: `src/sdk.rs`, `src/tool_env.rs`
- Sysroot setup and cache: `src/sysroot/*`
- Cross-build execution: `src/cargo_runner.rs`
- Package selection and layout: `src/package_select.rs`, `src/packaging.rs`
- RPM pipeline: `src/rpm/*`
- TPK pipeline and install: `src/tpk.rs`, `src/install_cmd.rs`, `src/device.rs`
- Diagnostics and cleanup: `src/doctor.rs`, `src/fix.rs`, `src/clean.rs`
- GitHub RPM release flow: `src/gh_release.rs`

## Non-Negotiable Behavior

- Sysroot cache entries live at `<cache>/<profile>/<platform-version>/<arch>/<provider>/<fingerprint>/`.
- Lock and temp paths must preserve dotted fingerprint names.
- Keep separate mappings for `rust_target`, `tizen_cli_arch`, `tizen_build_arch`, and `rpm_build_arch`.
- `rootstrap` is the working sysroot provider; `repo` must keep failing explicitly until implemented.
- Package resolution order is CLI `-p`, then `.cargo-tizen.toml [package].packages`, then the root Cargo package when possible.
- Packaging currently expects the built binary name to match the selected package name.
- `install` is TPK-only.
- `gh-release` is RPM-only and requires a clean tree on `main` with remote `origin` and authenticated `gh`.

## Validation

- `./scripts/check-agent-docs.sh`
- `cargo fmt`
- `cargo check`
- `cargo test`
- `cargo run -- --help`
- `cargo run -- <command> --help` for any command you changed
