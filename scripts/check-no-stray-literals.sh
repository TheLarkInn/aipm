#!/usr/bin/env bash
#
# Scan crates/ for hand-typed path-string literals that should now flow
# through `libaipm_engine_spec::paths::*` per the engine-api-schema
# source-of-truth refactor (specs/2026-05-04-engine-api-schema-source-of-truth.md).
#
# Reports hits but does not fail CI by default — the cleanup is
# incremental. Set STRICT=1 to fail when any unexpected hit appears.
#
# Usage:
#   scripts/check-no-stray-literals.sh
#   STRICT=1 scripts/check-no-stray-literals.sh

set -euo pipefail

# Tracked literals (Rust string-literal form).
PATTERN='"\.claude-plugin"|"\.github/plugin"|"marketplace\.json"|"marketplace\.toml"|"plugin\.json"|"plugin\.toml"|"aipm\.toml"'

# Files / paths where these literals are intentional and the script should
# stay quiet about them:
#   - libaipm-engine-spec is the source of truth; its data/, src/types.rs,
#     and build.rs all hold the canonical strings on purpose.
#   - tests/ and *test_helpers* are simulation fixtures.
#   - .lock.yml and .md are generated YAML or doc text.
ALLOWLIST_REGEX='crates/libaipm-engine-spec/|/tests/|tests/|test_helpers|\.lock\.yml$|\.md$'

# Run from repo root (script may be called from anywhere).
cd "$(dirname "$0")/.."

hits=$(
    grep -rEn "$PATTERN" crates/ \
        --include='*.rs' \
        --exclude-dir=target \
        --exclude-dir=.git \
        2>/dev/null \
        | grep -vE "$ALLOWLIST_REGEX" \
        | grep -vE '^\s*//' \
        || true
)

if [[ -z "$hits" ]]; then
    echo "OK: no stray path literals in production code (allowlist applied)."
    exit 0
fi

count=$(printf '%s\n' "$hits" | wc -l | tr -d '[:space:]')

cat <<EOF
Stray path-literal hits in production code (excludes tests + libaipm-engine-spec):

$hits

Total: $count occurrence(s).

Each hit is a candidate for replacement by
\`libaipm_engine_spec::paths::*\` constants. The cleanup is incremental
— track progress in research/progress.txt and the per-feature notes on
the engine-api-schema source-of-truth RFC
(specs/2026-05-04-engine-api-schema-source-of-truth.md).
EOF

if [[ "${STRICT:-0}" == "1" ]]; then
    echo
    echo "STRICT=1 set — exiting non-zero."
    exit 1
fi
exit 0
