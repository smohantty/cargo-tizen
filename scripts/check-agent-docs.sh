#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

required_files=(
    AGENTS.md
    CLAUDE.md
    README.md
    DESIGN.md
    docs/README.md
    docs/getting-started.md
    docs/install-linux.md
    docs/configuration.md
    docs/commands.md
    docs/packaging-layout.md
    docs/architecture.md
    docs/known-gaps.md
    docs/agents/index.md
    docs/agents/workflows.md
    templates/README.md
)

for file in "${required_files[@]}"; do
    [[ -f "$file" ]] || fail "missing required doc: $file"
done

agents_lines="$(wc -l < AGENTS.md | tr -d ' ')"
(( agents_lines <= 120 )) || fail "AGENTS.md should stay concise (<= 120 lines, found $agents_lines)"

grep -q 'AGENTS.md' CLAUDE.md || fail "CLAUDE.md must point to AGENTS.md"
grep -q 'docs/architecture.md' DESIGN.md || fail "DESIGN.md must point to docs/architecture.md"
grep -q 'workflows.md' docs/agents/index.md || fail "docs/agents/index.md must link to workflows.md"
grep -q 'known-gaps.md' docs/agents/index.md || fail "docs/agents/index.md must link to known-gaps.md"

help_output="$(cargo run -- --help 2>/dev/null)"
commands="$(
    printf '%s\n' "$help_output" |
        awk '
            /^Commands:/ { in_commands = 1; next }
            in_commands && NF == 0 { exit }
            in_commands { print $1 }
        '
)"

[[ -n "$commands" ]] || fail "failed to extract commands from cargo tizen --help"

while IFS= read -r command; do
    [[ -n "$command" ]] || continue
    [[ "$command" == "help" ]] && continue
    grep -Fqx "## \`$command\`" docs/commands.md || fail "docs/commands.md is missing section for command: $command"
done <<< "$commands"
