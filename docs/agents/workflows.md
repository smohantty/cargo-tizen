# Agent Workflows

Use these checklists when making changes.

## CLI Changes

Update:

- `src/cli.rs`
- `docs/commands.md`
- `docs/getting-started.md` when the common workflow changes
- `docs/architecture.md`
- `README.md` when the top-level user story changes

Validate:

- `cargo run -- --help`
- `cargo run -- <command> --help`
- `cargo test`
- `./scripts/check-agent-docs.sh`

## Config Changes

Update:

- `src/config.rs`
- `src/config_cmd.rs` if display or persistence changed
- `docs/configuration.md`
- `docs/architecture.md`
- `docs/commands.md` when a command interface changed

Validate:

- `cargo test`
- `cargo run -- config --help`
- `./scripts/check-agent-docs.sh`

## Sysroot or SDK Changes

Update:

- `src/sdk.rs`
- `src/sysroot/*`
- `src/rust_target.rs` if target inference changed
- `docs/install-linux.md`
- `docs/configuration.md`
- `docs/architecture.md`
- `docs/known-gaps.md` if provider status changed

Validate:

- `cargo test`
- `cargo run -- setup --help`
- `cargo run -- doctor --help`
- `cargo run -- fix --help`
- `./scripts/check-agent-docs.sh`

## RPM, TPK, or Install Changes

Update:

- `src/package_select.rs`
- `src/packaging.rs`
- `src/rpm/*` and/or `src/tpk.rs`
- `src/install_cmd.rs` and `src/device.rs` when device behavior changed
- `docs/commands.md`
- `docs/packaging-layout.md`
- `docs/getting-started.md`
- `docs/known-gaps.md` when limitations changed
- `docs/architecture.md`

Validate:

- `cargo test`
- `cargo run -- rpm --help`
- `cargo run -- tpk --help`
- `cargo run -- install --help`
- `cargo run -- devices --help`
- `./scripts/check-agent-docs.sh`

## Release Flow Changes

Update:

- `src/gh_release.rs`
- `docs/commands.md`
- `docs/configuration.md`
- `docs/architecture.md`
- `docs/known-gaps.md` if boundaries changed
- `README.md` when the public workflow changed

Validate:

- `cargo test`
- `cargo run -- gh-release --help`
- `./scripts/check-agent-docs.sh`

## Docs-Only Changes

Requirements:

- docs must describe current code, not intent
- remove or merge redundant pages instead of leaving parallel versions
- keep links and file names aligned with `docs/`

Validate:

- `./scripts/check-agent-docs.sh`
