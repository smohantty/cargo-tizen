# CLAUDE.md

This file defines repository-specific rules and context for future coding agents working on `cargo-tizen`.

## Workflow default

For any feature, refactor, or bug fix request in this repo, default to gstack best practices:

1. Run `/autoplan` before making any code changes.
2. Implement only after the gstack review plan is established.
3. Run the appropriate gstack review and test loop before finishing.
4. Prepare for `/ship` if the work is ready.

Do not substitute an ad hoc workflow.

Review must not rely only on the same coding agent that did the work, and plan review must follow the same separation rule as code review.

- The primary planning agent must not be the final reviewer of its own plan.
- A fresh-context subagent from the same coding agent may be used as an additional reviewer, but it does not satisfy the review requirement by itself.
- `/autoplan` (or an equivalent explicit plan-review flow) must use at least one independent review agent that is a different coding agent from the primary planner.
- Example: if Claude is the planner, review must be done by Codex. If Codex is the planner, review must be done by Claude.
- The implementing agent must not be the final reviewer of its own patch.
- A fresh-context subagent from the same coding agent may be used as an additional reviewer, but it does not satisfy the review requirement by itself.
- Code review must use at least one independent review agent that is a different coding agent from the primary coder.
- Example: if Claude is the coder, review must be done by Codex. If Codex is the coder, review must be done by Claude.
- Prefer using more than one review agent for both plan review and code review on non-trivial changes.
- If a different coding agent is unavailable for either plan review or code review, say so explicitly and stop to ask the user before treating the work as reviewed.

If the git tree is dirty, the scope changes, or a required gstack step cannot be followed cleanly, stop and ask the user before proceeding.

## Project intent

`cargo-tizen` is a Cargo subcommand (`cargo tizen ...`) for:
- preparing/caching Tizen sysroots
- cross-building Rust binaries for Tizen targets
- packaging outputs as RPM and TPK

Current architecture is intentionally pragmatic and CLI-first.

## Upstream baseline

This project is an adaptation of design patterns from:
- https://github.com/flutter-tizen/flutter-tizen
- https://github.com/flutter-tizen/flutter-tizen/tree/master/doc

When changing SDK/rootstrap/device workflows, check upstream behavior first and update docs/design if divergence is intentional.

## Command surface (must stay stable unless explicitly changed)

- `cargo tizen setup`
- `cargo tizen build`
- `cargo tizen rpm`
- `cargo tizen tpk`
- `cargo tizen devices`
- `cargo tizen install`
- `cargo tizen doctor`
- `cargo tizen clean`
- `cargo tizen config`

If flags/semantics change, update:
- `README.md`
- `doc/commands.md`
- `DESIGN.md`

in the same patch.

## Core implementation map

- CLI + dispatch: `src/main.rs`, `src/cli.rs`
- Config: `src/config.rs`
- Arch mapping: `src/arch.rs`
- SDK discovery: `src/sdk.rs`
- Tool env injection: `src/tool_env.rs`
- Sysroot/cache/providers: `src/sysroot/*`
- Build runner: `src/cargo_runner.rs`
- Device discovery/install: `src/device.rs`, `src/install_cmd.rs`
- RPM backend: `src/rpm/*`
- TPK backend: `src/tpk.rs`
- Config command: `src/config_cmd.rs`
- Diagnostics: `src/doctor.rs`

## Critical invariants

1. Cache key/path behavior
- Cache entry path format: `<cache>/<profile>/<platform>/<arch>/<provider>/<fingerprint>/`
- Lock/temp paths must preserve dotted fingerprint names.
- Do not use `Path::set_extension` for sibling lock/temp naming on cache entry paths.

2. Arch mapping separation
- Keep explicit per-consumer mappings (`rust_target`, `tizen_cli_arch`, `tizen_build_arch`, `rpm_build_arch`).
- Avoid collapsing to a single arch name.

3. Rootstrap resolution policy
- Respect current normalization/fallback rules in `src/sysroot/rootstrap.rs`.
- Keep SDK discovery order consistent with docs/design.

4. Build output isolation
- Cargo target output defaults under `target/tizen/<arch>/cargo`.
- RPM and TPK artifacts stay under arch/profile-segmented directories.

5. Backend status
- `rootstrap` provider is functional.
- `repo` provider is intentionally unimplemented; keep failure explicit unless implementing fully.

## Documentation synchronization rule

`DESIGN.md` is treated as a living implementation contract.

Whenever behavior changes:
- update `DESIGN.md` sections describing that behavior
- keep `README.md` and `doc/*` examples aligned with actual CLI flags and outputs
- avoid documenting not-yet-implemented behavior without clearly marking it as planned/pending

## Validation checklist before finishing changes

Run:

```bash
cargo fmt
cargo check
cargo test
```

For CLI changes, also run:

```bash
cargo run -- --help
cargo run -- <subcommand> --help
```

## Known limitations (do not hide)

- `repo` sysroot provider is not implemented.
- `package.metadata.tizen` schema is documented but not wired into staging/spec generation yet.
- Workspace/member packaging requires explicit `--package` / `--bin` when selection is ambiguous.
- End-to-end packaging tests require real Tizen SDK/rootstraps and host toolchain availability.
