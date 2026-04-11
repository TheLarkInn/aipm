---
date: 2026-04-10 19:53:02 UTC
researcher: Claude
git_commit: 2a5103be452b6b722afee133f12f4e4ea8a46610
branch: main
repository: aipm
topic: "VS Code extension launch failure — cargo not found in task shell"
tags: [research, vscode-aipm, extension, debugging, launch-config, tasks, cargo, PATH]
status: complete
last_updated: 2026-04-10
last_updated_by: Claude
---

# Research: VS Code Extension Launch Failure

## Research Question
Why does "Launch Extension with Fixture Folder" fail with `cargo: command not found` (exit code 127)?

## Summary

The launch configuration `"Launch Extension with Fixture Folder"` uses `preLaunchTask: "build-and-compile"`, which runs two sub-tasks in parallel: `build-aipm-debug` (runs `cargo build -p aipm`) and `compile-extension` (runs `npm run compile`). The TypeScript compilation succeeds, but the `cargo` command is not found in the shell environment that VS Code spawns for the task.

**Root cause:** VS Code tasks execute in a non-login, non-interactive shell (`/bin/bash -c '...'`). In this environment, neither `~/.bashrc` nor `~/.profile` are sourced, so the `. "$HOME/.cargo/env"` line that adds `~/.cargo/bin` to `PATH` never runs. The terminal window PATH (which includes `/home/codespace/.cargo/bin`) is not inherited by task processes.

## Detailed Findings

### Launch Configuration Chain

The launch config at [`.vscode/launch.json:27-44`](https://github.com/TheLarkInn/aipm/blob/2a5103be452b6b722afee133f12f4e4ea8a46610/.vscode/launch.json#L27-L44):

```
"Launch Extension with Fixture Folder"
  └── preLaunchTask: "build-and-compile"
        ├── "build-aipm-debug"     → cargo build -p aipm        ❌ FAILS
        └── "compile-extension"    → npm run compile             ✅ SUCCEEDS
```

The `"build-and-compile"` compound task ([`.vscode/tasks.json:66-74`](https://github.com/TheLarkInn/aipm/blob/2a5103be452b6b722afee133f12f4e4ea8a46610/.vscode/tasks.json#L66-L74)) runs both dependencies with `dependsOrder: "parallel"`. Since `build-aipm-debug` fails, the entire pre-launch task fails and the Extension Development Host never starts.

### The `build-aipm-debug` Task

Defined at [`.vscode/tasks.json:41-49`](https://github.com/TheLarkInn/aipm/blob/2a5103be452b6b722afee133f12f4e4ea8a46610/.vscode/tasks.json#L41-L49):

```json
{
  "label": "build-aipm-debug",
  "type": "shell",
  "command": "cargo build -p aipm",
  "group": "build",
  "presentation": { "reveal": "silent", "panel": "shared" },
  "problemMatcher": "$rustc"
}
```

- No `options.env` override to inject `PATH`
- No `options.shell` override to force a login shell
- VS Code runs this as `/bin/bash -c 'cargo build -p aipm'`, which does not source profile files

### Where `cargo` Lives

- Binary location: `/home/codespace/.cargo/bin/cargo`
- PATH setup: `~/.cargo/env` (sourced by `~/.bashrc` and `~/.profile`)
- The integrated terminal works because it starts an interactive shell that sources `~/.bashrc`
- Task processes are non-interactive and skip profile sourcing

### The Extension Itself

[`vscode-aipm/src/extension.ts:17`](https://github.com/TheLarkInn/aipm/blob/2a5103be452b6b722afee133f12f4e4ea8a46610/vscode-aipm/src/extension.ts#L17):

```typescript
const aipmPath = process.env['AIPM_PATH'] ?? config.get<string>('path', 'aipm');
```

The launch config sets `AIPM_PATH` to `${workspaceFolder}/target/debug/aipm` via the `env` block. This means the extension itself would find the binary if it launched — the failure happens *before* the extension activates, during the pre-launch task.

### Current State of the Binary

The `aipm` debug binary already exists at `/workspaces/aipm/target/debug/aipm` (52 MB, built previously). The `cargo build` task would succeed if `cargo` were findable, and it would be a fast no-op rebuild.

### Fixture Directory

The fixture folder at `/workspaces/aipm/fixtures/extension-test/` exists and contains:
- `aipm.toml` — workspace manifest
- `.ai/` — AI plugin directory
- `.claude/` — Claude configuration directory
- `WHAT_SHOULD_FAIL.md` — test file for expected lint failures

## Three Ways to Fix the Launch Failure

### Option A: Add PATH to the task's environment (targeted fix)

In `.vscode/tasks.json`, add an `options.env` block to the `build-aipm-debug` task:

```json
{
  "label": "build-aipm-debug",
  "type": "shell",
  "command": "cargo build -p aipm",
  "options": {
    "env": {
      "PATH": "${env:HOME}/.cargo/bin:${env:PATH}"
    }
  },
  ...
}
```

This injects `~/.cargo/bin` into the task's PATH without relying on profile sourcing.

### Option B: Source cargo env in the command itself

Change the task command to source the env first:

```json
{
  "label": "build-aipm-debug",
  "type": "shell",
  "command": ". \"$HOME/.cargo/env\" && cargo build -p aipm",
  ...
}
```

### Option C: Use an absolute path to cargo

```json
{
  "label": "build-aipm-debug",
  "type": "shell",
  "command": "${env:HOME}/.cargo/bin/cargo build -p aipm",
  ...
}
```

### The Other Launch Config Works Differently

The simpler `"Launch Extension (Extension Development Host)"` config ([`.vscode/launch.json:11-22`](https://github.com/TheLarkInn/aipm/blob/2a5103be452b6b722afee133f12f4e4ea8a46610/.vscode/launch.json#L11-L22)) uses `preLaunchTask: "compile-extension"` (TypeScript only) — it skips the cargo build entirely, so it wouldn't hit this error.

## Code References
- `.vscode/launch.json:27-44` — "Launch Extension with Fixture Folder" configuration
- `.vscode/launch.json:36` — `preLaunchTask: "build-and-compile"`
- `.vscode/tasks.json:41-49` — `build-aipm-debug` task definition
- `.vscode/tasks.json:66-74` — `build-and-compile` compound task
- `vscode-aipm/src/extension.ts:17` — `AIPM_PATH` env var fallback
- `vscode-aipm/package.json` — Extension manifest (v0.1.0)
- `/home/codespace/.cargo/env` — Cargo PATH setup script
- `/home/codespace/.bashrc` — Sources `.cargo/env` (interactive shells only)

## Architecture Documentation

The extension development setup has two tiers:
1. **TypeScript compile** (`npm run compile` in `vscode-aipm/`) — builds the VS Code extension client
2. **Rust build** (`cargo build -p aipm`) — builds the LSP server binary that the extension spawns

The extension acts as an LSP client that launches `aipm lsp` as a child process using stdio transport. The launch config for the fixture folder sets `AIPM_PATH` so the extension finds the debug binary, but the pre-launch cargo task needs `cargo` to build it first.

## Historical Context (from research/)
- `research/docs/2026-04-10-377-vscode-support-aipm-lint.md` — Original research for VS Code integration (issue #377)

## Open Questions
- Should VS Code workspace settings (`.vscode/settings.json`) be added to pre-configure `aipm.path` for development?
- Should the `build-and-compile` task be made more resilient (e.g., skip cargo build if binary exists)?
