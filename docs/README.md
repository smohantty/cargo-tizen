# Documentation

`docs/` is the version-controlled system of record for `cargo-tizen`.

Start here based on what you need:

- [Getting started](getting-started.md): first-run path for an existing Rust project
- [Linux install](install-linux.md): documented host setup path
- [Configuration](configuration.md): config files, defaults, merge order, and overrides
- [Commands](commands.md): command surface and command-specific notes
- [Packaging layout](packaging-layout.md): authored packaging inputs and generated outputs
- [Architecture](architecture.md): implementation map and behavioral contract
- [Known gaps](known-gaps.md): explicit unsupported or unfinished behavior
- [Agent guide](agents/index.md): agent-specific navigation and checklists

Maintenance rules:

- Update docs in the same patch as behavior changes.
- Keep docs grounded in code, clap help, and tests.
- Delete stale or redundant docs instead of keeping multiple competing versions.
- Run `./scripts/check-agent-docs.sh` before finishing doc or CLI changes.
