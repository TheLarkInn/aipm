---
description: >
  Periodic reverse binary analysis of AI engine CLIs (claude, copilot-cli, opencode, etc.).
  Downloads each engine binary, uses a parallel LLM agent per engine to read the minified source,
  validates plugin-related APIs, and produces an updated schema file and a changelog document.
  Runs weekly and opens a PR when findings differ from the previously committed schema.
on:
  schedule: weekly
  workflow_dispatch:
timeout-minutes: 45
permissions:
  contents: read
  issues: read
  pull-requests: read
tools:
  github:
    toolsets: [default]
  web-fetch:
  bash: true
network:
  allowed: [defaults, node]
checkout:
  fetch: ["*"]
  fetch-depth: 0
safe-outputs:
  create-pull-request:
    max: 1
    draft: false
    auto-merge: false
    labels: [automation, analysis]
  push-to-pull-request-branch:
    target: "*"
    title-prefix: "[reverse-binary-analysis]"
    if-no-changes: ignore
  noop:
    report-as-issue: false
---

# Reverse Binary Analysis

You are an expert AI agent performing **reverse binary analysis** of AI engine CLI runtimes to
track their plugin APIs and keep aipm's detection, migration, and lint rules up to date.

## Objectives (per issue #132)

- Download each configured AI engine CLI binary.
- Spawn **parallel analysis** (one agent thread per engine) that reads the minified/bundled source
  code and validates all APIs relevant to:
  - `marketplace.json` / `plugin.json`
  - `.claude/settings.json` and `.claude/` folder conventions
  - `.github/copilot/` folder conventions
  - `.vscode/`, `.opencode/`, `codex/`, `gemini-cli/` conventions
  - Skills, commands, agents, LSPs, MCPs, output styles, local settings, plugin attributes
  - Engine detection and discovery logic
  - Size limits logic
  - Every known rule
- Produce suggestions for:
  - Adaptor/detector fixes in this codebase
  - New unit test cases
  - Behavior variants to handle
- Update (or create) `research/engine-api-schema.json` — a canonical schema of all discovered APIs.
- Update (or create) `research/engine-api-changelog.md` — a versioned changelog that records every
  API change, when it was first observed, and the engine version at that time.
- Open a PR when the schema or changelog has changed.

## Engine Configuration

The list of engines to analyze is read from `research/engine-api-schema.json` under the
`"engines"` key if that file already exists.  If the file does not exist yet, bootstrap with these
defaults:

```json
{
  "engines": [
    { "name": "claude",      "source": "npm", "package": "@anthropic-ai/claude-code" },
    { "name": "copilot-cli", "source": "npm", "package": "@github/copilot-cli"       }
  ]
}
```

## Step-by-step Instructions

### 1 — Read existing schema and changelog

Read `research/engine-api-schema.json` and `research/engine-api-changelog.md` if they exist.
Note the `engines` list and the `versions` map (engine → last-seen version).  These will be used
to detect changes.

### 2 — Download each engine CLI

For each engine in the configuration, install the package into a temporary directory:

```bash
mkdir -p /tmp/rba-engines
cd /tmp/rba-engines
```

**npm engines** — install without running postinstall scripts to keep the sandbox clean:

```bash
npm install --prefix /tmp/rba-engines/<engine-name> --ignore-scripts <npm-package>@latest
```

After installation, capture the installed version:

```bash
npm list --prefix /tmp/rba-engines/<engine-name> --depth 0 --json 2>/dev/null \
  | jq -r '.dependencies | to_entries[0].value.version'
```

If a download fails, log the error in the changelog and skip that engine for this run.

### 3 — Locate entry-point and bundled source

For each installed package, find the main entry file and any bundled/minified JS files:

```bash
find /tmp/rba-engines/<engine-name>/node_modules/<package-path> \
  -name "*.js" -not -path "*/node_modules/*/node_modules/*" \
  | head -50
```

Read the `package.json` of the package to identify the `"main"` field.

### 4 — Parallel per-engine API extraction

For **each engine**, perform all of the following analysis steps.  Work through the engines
concurrently where possible to stay within the 45-minute timeout.

#### 4a — Read and extract API surface

Read the bundled source files (they may be minified; do your best to interpret them).

Specifically look for and extract:

- **manifest / plugin fields**: all keys expected in `marketplace.json`, `plugin.json`,
  `plugin-manifest.json`, or equivalent.
- **settings file paths**: any file paths the engine reads from disk at startup or during a session
  (e.g. `.claude/settings.json`, `.github/copilot-instructions.md`).
- **folder conventions**: all directories the engine scans or mounts
  (`.claude/`, `.github/copilot/`, `.vscode/`, `.opencode/`, `codex/`, `gemini-cli/`, etc.).
- **skill / command / agent registration**: how skills, slash commands, or sub-agents are declared
  and discovered.
- **LSP and MCP configuration**: how Language Server Protocols and Model Context Protocol servers
  are configured.
- **output styles**: structured output formats supported by the engine.
- **size limits**: any hard-coded file size, token, or payload limits.
- **detection heuristics**: logic used to detect whether a repo uses this engine.
- **discovery algorithm**: how plugins, skills, or extensions are discovered.
- **every rule or validation**: any validation or lint rules baked into the engine.

#### 4b — Compare to existing schema

Diff the extracted surface against the previously recorded schema for this engine.  Identify:

- **Added** fields, paths, or behaviours (new since last run).
- **Removed** fields, paths, or behaviours (no longer present).
- **Changed** fields (e.g. renamed keys, changed defaults, changed size limits).

#### 4c — Generate suggestions

Based on the diff, suggest concrete changes needed in this codebase:

1. **Adaptor/detector fixes** — paths in `crates/libaipm/` that may need updating.
2. **New unit test cases** — specific scenarios that should be tested.
3. **Behaviour variants** — edge cases or new features to handle.

### 5 — Update `research/engine-api-schema.json`

Merge all extracted API surfaces into the schema file.  Structure:

```jsonc
{
  "generated_at": "<ISO-8601 timestamp>",
  "engines": [
    { "name": "<engine>", "source": "npm", "package": "<package>" }
    // ...
  ],
  "versions": {
    "<engine>": "<installed-version>"
  },
  "apis": {
    "<engine>": {
      "manifest_fields": [ ... ],
      "settings_paths": [ ... ],
      "folder_conventions": [ ... ],
      "skill_registration": { ... },
      "lsp_config": { ... },
      "mcp_config": { ... },
      "output_styles": [ ... ],
      "size_limits": { ... },
      "detection_heuristics": [ ... ],
      "discovery_algorithm": [ ... ],
      "rules": [ ... ]
    }
  },
  "suggestions": {
    "<engine>": {
      "adaptor_fixes": [ ... ],
      "test_cases": [ ... ],
      "behaviour_variants": [ ... ]
    }
  }
}
```

### 6 — Update `research/engine-api-changelog.md`

Prepend a new entry at the top of the changelog (or create the file) with:

```markdown
## <ISO-8601 date> — <engine> v<new-version>

| Field | Change |
|-------|--------|
| `<field>` | Added / Removed / Changed (was: `<old>`, now: `<new>`) |
```

If a version has not changed since the last run, still record the run date and note
"no API changes detected".

If this is the first run, record the baseline versions and note "initial schema established".

### 7 — Check whether anything changed

Compare the new schema with the committed version using:

```bash
git diff --stat research/engine-api-schema.json research/engine-api-changelog.md
```

- If **nothing changed**, call the `noop` safe output:
  > "Reverse binary analysis complete — no API changes detected for any engine.
  > Versions analyzed: \<engine\>=\<version\>, ..."
  Stop here.

### 8 — Open a Pull Request

Use `push-to-pull-request-branch` to push the updated files to a branch named
`reverse-binary-analysis/<date>` (e.g. `reverse-binary-analysis/2026-04-30`).

Then use `create-pull-request` with:

- **Title**: `[reverse-binary-analysis] API schema update <date>`
- **Body** that includes:
  1. **Summary table**: engine → old version → new version.
  2. **API changes**: full diff for each engine (added / removed / changed fields).
  3. **Suggestions**: adaptor/detector fixes, new test cases, behaviour variants.
  4. **Links**: reference to issue #132.

Label the PR with `automation` and `analysis`.
