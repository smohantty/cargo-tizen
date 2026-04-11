# CLAUDE.md

Compatibility note for Claude-family agents.

This repository now uses `AGENTS.md` as the root agent entry point and `docs/` as the system of record.

Start here:

- `AGENTS.md`
- `docs/README.md`
- `docs/architecture.md`
- `docs/commands.md`
- `docs/configuration.md`
- `docs/known-gaps.md`

Hard rules:

- Keep docs grounded in the current code and tests.
- Update docs in the same patch as behavior changes.
- Do not hide current limitations such as the unimplemented `repo` provider or binary-name packaging constraints.
- Run `./scripts/check-agent-docs.sh`, `cargo fmt`, `cargo check`, and `cargo test` before finishing.
