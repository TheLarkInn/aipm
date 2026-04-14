---
description: Start a ralph loop that continuously implements features from research/feature-list.json with cargo quality gates after each one. Stops automatically when all features pass and the workspace is green.
argument-hint: "[--max-iterations N]"
allowed-tools: Skill
---

You are setting up a self-sustaining feature implementation loop for this Rust workspace.

## What You're Starting

Each loop iteration will:
1. Run `/aipm-atomic-plugin:implement-feature` to implement one feature from `research/feature-list.json`
2. Validate the workspace with the `cargo-verifier` subagent (cargo build/test/clippy/fmt + 89% coverage)
3. Record failures as new high-priority features, or advance to the next feature on success
4. Stop automatically when all features have `passes: true` AND the workspace is fully green

## Steps

1. Parse `$ARGUMENTS` for `--max-iterations N`. Default to `50` if not provided.

2. Use the `Skill` tool to invoke `ralph-loop` with these exact arguments:
   - **prompt**: `Run /aipm-atomic-plugin:implement-feature to work on the next feature from research/feature-list.json. After implementation, use the cargo-verifier subagent to validate the workspace. Follow the Quality Gate instructions in the implement-feature command.`
   - **completion-promise**: `ALL FEATURES COMPLETE`
   - **max-iterations**: the value parsed above

3. Confirm to the user that the loop has started and remind them:
   - Monitor iteration: `grep '^iteration:' .claude/ralph-loop.local.md`
   - View full state: `head -10 .claude/ralph-loop.local.md`
   - Cancel: `/ralph-loop:cancel-ralph`
