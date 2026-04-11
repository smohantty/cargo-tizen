# Agent Guide

Use this directory as the agent-specific entry point. The source of truth still lives in the rest of `docs/`.

## Read In This Order

1. [`../../AGENTS.md`](../../AGENTS.md)
2. [`../README.md`](../README.md)
3. [`../architecture.md`](../architecture.md)
4. [`../commands.md`](../commands.md)
5. [`../configuration.md`](../configuration.md)
6. [`../packaging-layout.md`](../packaging-layout.md)
7. [`../known-gaps.md`](../known-gaps.md)
8. [`workflows.md`](workflows.md)

## What Lives Where

- `../architecture.md`: current implementation contract and code map
- `../commands.md`: command surface and user-visible behavior
- `../configuration.md`: config keys, defaults, and merge order
- `../packaging-layout.md`: authored packaging files and generated work trees
- `../known-gaps.md`: unsupported behavior that should not be documented as working
- `workflows.md`: change-type checklists and validation

## Maintenance Rules

- Keep `AGENTS.md` short.
- Add detail to `docs/`, not to `AGENTS.md`.
- Delete redundant docs instead of carrying both old and new copies.
- When behavior changes, update the relevant docs in the same patch.
- Run `./scripts/check-agent-docs.sh` after doc or CLI edits.
