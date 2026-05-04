---
date: 2026-05-02 13:44:44 UTC
researcher: Sean Larkin
git_commit: 0f4e837c0e3ba30ad34827197fd54c0c6a9a7348
branch: main
repository: aipm
topic: "Removal scope for the `<engine>-instructions.md` classifier branch in unified discovery"
tags: [research, discovery, lint, instruction-files, issue-725, removal, copilot-instructions, claude-instructions, agents-instructions, gemini-instructions]
status: complete
last_updated: 2026-05-02
last_updated_by: Sean Larkin
---

# Research: Removing the `<engine>-instructions.md` Classifier Branch

## Research Question

> The unified discovery pipeline now recognises `copilot-instructions.md`,
> `claude-instructions.md`, `agents-instructions.md`, and `gemini-instructions.md`
> as instruction files (alongside the existing `CLAUDE.md`, `AGENTS.md`, etc.
> names). This entire feature should be deleted and was misinterpreted. There is
> no `<engine>-instructions.md` that any engine picks up. This is a bug and this
> needs to be removed from the codebase.

Two scopes were confirmed by the user (`AskUserQuestion` 2026-05-02):

1. **Verify no engine reads these names** — establish the load-bearing fact that
   the deletion rests on, with cited engine documentation.
2. **Map the removal blast radius** — find every source/test/spec/doc/changelog/
   research site that references the pattern and what removing each entails.

The user explicitly excluded "distinguish what stays vs. goes" and "trace the
misinterpretation back to spec" as research objectives. This document still
identifies the legitimate parts that must NOT be removed (`INSTRUCTION_FILENAMES`
exact-name table and `*.instructions.md` suffix) because they share a file with
the target branch and an implementer needs to know where the cut line is.

---

## Summary (TL;DR)

1. **The user is correct on the load-bearing fact.** Of the four prefixes hard-
   coded at
   [`crates/libaipm/src/discovery/instruction.rs:33`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/discovery/instruction.rs#L33)
   in `ENGINE_INSTRUCTION_PREFIXES = &["copilot", "claude", "agents", "gemini"]`,
   three correspond to filenames **no engine reads**:
   - `claude-instructions.md` — Claude Code reads `CLAUDE.md` only.
   - `gemini-instructions.md` — Gemini CLI reads `GEMINI.md` only.
   - `agents-instructions.md` — the AGENTS.md spec defines `AGENTS.md` only.

   The fourth name, `copilot-instructions.md`, IS recognized by GitHub Copilot —
   but only at exactly two paths: `.github/copilot-instructions.md` (repo) and
   `$HOME/.copilot/copilot-instructions.md` (user). Copilot does NOT read it
   from `.github/copilot/copilot-instructions.md` (the nested path the test at
   [`instruction.rs:218-228`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/discovery/instruction.rs#L218-L228)
   asserts on), nor from `.claude/`, `.ai/`, `.gemini/`, or arbitrary roots —
   exactly the locations the aipm classifier accepts.

2. **The pattern is contained to one branch in one classifier function.** The
   removable surface in `instruction.rs` is:
   - `ENGINE_INSTRUCTION_PREFIXES` constant
     ([line 33](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/discovery/instruction.rs#L33))
   - `matches_engine_instructions` helper
     ([lines 74-79](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/discovery/instruction.rs#L74-L79))
   - The third disjunct in `is_instruction_filename`
     ([line 69](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/discovery/instruction.rs#L69))
   - Module-doc lines 11-13 and the doc comment at line 32
   - Case-C unit tests
     ([lines 143-171, 180-214, plus 218-228](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/discovery/instruction.rs#L143-L171))

   The first two branches of `is_instruction_filename` (the `INSTRUCTION_FILENAMES`
   table and the `*.instructions.md` suffix) MUST stay — they correspond to real
   engine conventions and are independent of the engine-prefix logic.

3. **Blast radius beyond `instruction.rs` is moderate but mechanical.**
   - **6 source files** carry test fixtures or doc comments referencing
     `.github/copilot/copilot-instructions.md`: `discovery/{mod,classify,types,
     walker}.rs`, `lint/mod.rs`, plus `instruction.rs` itself.
   - **3 test files** assert on the pattern: `crates/libaipm/tests/bdd.rs`
     (step `given_copilot_instructions_file_exists`),
     `crates/aipm/tests/issue_725_e2e.rs` (the "1 instruction" count assertion
     across four tests), and `tests/features/guardrails/quality.feature` (the
     `Lint flags oversized .github/copilot/copilot-instructions.md` scenario).
   - **2 spec files** mandate the feature: most importantly
     `specs/2026-05-01-unified-discovery-and-copilot-skill-detection.md` (G7
     acceptance criterion at line 109, full Section 5.4 at lines 415-450).
   - **2 user-facing docs** advertise it: `CHANGELOG.md` line 10,
     `docs/rules/source/misplaced-features.md` line 62.
   - **2 in-flight research files** track it: `research/feature-list.json`
     (items 5, 7, 8, 16, 17, 25 reference it) and `research/progress.txt`
     (multiple log entries).

4. **The customer issue #725 still has a real lint-side bug — but only for one
   path, not a family.** The
   [`2026-05-01-github-copilot-skills-migrate-lint-silent-failure.md`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/research/docs/2026-05-01-github-copilot-skills-migrate-lint-silent-failure.md)
   research doc focuses on `SKILL.md` discovery; its mention of
   `copilot-instructions.md` is incidental. The real Copilot-recognized path is
   `.github/copilot-instructions.md` (no `copilot/` segment). The
   [`2026-03-28-copilot-cli-source-code-analysis.md`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/research/docs/2026-03-28-copilot-cli-source-code-analysis.md)
   binary analysis confirms this path-exact. So removing the `<engine>-
   instructions.md` family does not lose any real engine-recognized
   functionality — but the implementer should consciously choose what (if
   anything) replaces it for the bare `.github/copilot-instructions.md` path,
   which today is matched ONLY via the engine-prefix branch (it is not in
   `INSTRUCTION_FILENAMES` and does not end in `.instructions.md`).

5. **The introducing spec was self-contradictory.** The same spec
   (`specs/2026-05-01-unified-discovery-and-copilot-skill-detection.md`)
   that mandates G7 also cites Copilot's docs that the path is bare
   `.github/copilot-instructions.md`. The leap from "support
   `.github/copilot-instructions.md`" to "support `<engine>-instructions.md`
   for any of `{copilot,claude,agents,gemini}` from any source root" is the
   misinterpretation the user is calling out.

---

## Detailed Findings

### 1. Engine-Documentation Verification (Load-Bearing Facts)

The aipm classifier
([`instruction.rs:33`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/discovery/instruction.rs#L33))
accepts `<engine>-instructions.md` for `<engine> ∈ {copilot, claude, agents,
gemini}`, from any source root the discovery walker descends into (`.claude/`,
`.github/`, `.ai/`, `.gemini/`, project root, plus arbitrary nested paths under
those — see the test fixtures in §3 below). Cross-referenced against each
engine's official docs:

#### Claude Code (Anthropic)

**Source**: [How Claude remembers your project — code.claude.com/docs/en/memory](https://code.claude.com/docs/en/memory)

> "**CLAUDE.md files**: instructions you write to give Claude persistent context"
>
> "A project CLAUDE.md can be stored in either `./CLAUDE.md` or `./.claude/CLAUDE.md`."
>
> "Claude Code reads CLAUDE.md files by walking up the directory tree from your
> current working directory, checking each directory along the way for `CLAUDE.md`
> and `CLAUDE.local.md` files."
>
> "Claude Code reads `CLAUDE.md`, not `AGENTS.md`. If your repository already
> uses `AGENTS.md` for other coding agents, create a `CLAUDE.md` that imports
> it…"

The discovery walk explicitly enumerates only `CLAUDE.md`, `CLAUDE.local.md`,
and `.claude/rules/*.md`. **`claude-instructions.md` is not mentioned anywhere in
the memory docs.** ✅ Invented.

#### GitHub Copilot (the only real-but-misplaced case)

**Sources**:
- [Adding repository custom instructions for GitHub Copilot — docs.github.com](https://docs.github.com/en/copilot/customizing-copilot/adding-repository-custom-instructions-for-github-copilot)
- [Adding custom instructions for GitHub Copilot CLI — docs.github.com](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-custom-instructions)
- [Copilot coding agent now supports AGENTS.md — GitHub Changelog](https://github.blog/changelog/2025-08-28-copilot-coding-agent-now-supports-agents-md-custom-instructions/)

> "These are specified in a `copilot-instructions.md` file in the `.github`
> directory of the repository."
>
> "In the root of your repository, create a file named
> `.github/copilot-instructions.md`."
>
> "You can specify instructions within your own home directory, by creating a
> file at `$HOME/.copilot/copilot-instructions.md`."

`copilot-instructions.md` is recognized at exactly two paths:

1. `<repo>/.github/copilot-instructions.md`
2. `$HOME/.copilot/copilot-instructions.md`

It is NOT recognized at:
- `.github/copilot/copilot-instructions.md` (the path tested by the e2e fixture
  at
  [`crates/aipm/tests/issue_725_e2e.rs:55-57`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/aipm/tests/issue_725_e2e.rs#L55-L57)
  and the unit test at
  [`instruction.rs:220-225`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/discovery/instruction.rs#L220-L225)).
- `.claude/copilot-instructions.md`, `.ai/copilot-instructions.md`,
  `.gemini/copilot-instructions.md`, or arbitrary roots.

Path-specific Copilot instructions use a different shape entirely
(`<NAME>.instructions.md` inside `.github/instructions/`), which is already
handled by the legitimate `*.instructions.md` suffix branch and is **not** part
of the engine-prefix family being removed.

✅ Real but misplaced — the classifier treats the file as engine-recognized in
locations Copilot does not actually scan.

#### Gemini CLI (Google)

**Sources**:
- [Provide Context with GEMINI.md Files — google-gemini.github.io](https://google-gemini.github.io/gemini-cli/docs/cli/gemini-md.html)
- [google-gemini/gemini-cli — GitHub](https://github.com/google-gemini/gemini-cli)

> "Context files, which use the default name `GEMINI.md`, are a powerful
> feature for providing instructional context to the Gemini model."
>
> "While `GEMINI.md` is the default filename, you can configure this in your
> `settings.json` file. To specify a different name or a list of names, use the
> `context.fileName` property."

Default is strictly `GEMINI.md`. The only way to make Gemini CLI read a
different name is a user-configured `context.fileName` override. **The aipm
classifier does not consult any user setting** — it hard-codes the
`gemini-instructions.md` name unconditionally. ✅ Invented.

#### AGENTS.md spec

**Source**: [agents.md](https://agents.md/)

> "Create an AGENTS.md file at the root of the repository."
>
> "Place another AGENTS.md inside each package" (for monorepos).

The spec defines exactly one filename: `AGENTS.md`. The phrase
"agents-instructions.md" appears nowhere. The legacy migration shim
(`mv AGENT.md AGENTS.md`) does not endorse alternative filenames. ✅ Invented.

#### Verification summary table

| Engine prefix | Real engine name | Real path(s) | Aipm classifier accepts at | Verdict |
|---|---|---|---|---|
| `claude-` | `CLAUDE.md` | `./CLAUDE.md`, `./.claude/CLAUDE.md`, `~/.claude/CLAUDE.md` | any source root, any nested path | **Invented** |
| `copilot-` | `copilot-instructions.md` | `./.github/copilot-instructions.md`, `$HOME/.copilot/copilot-instructions.md` | any source root, any nested path | **Real but misplaced** |
| `agents-` | `AGENTS.md` | repo root, per-package roots | any source root, any nested path | **Invented** |
| `gemini-` | `GEMINI.md` | `~/.gemini/GEMINI.md`, workspace walk | any source root, any nested path | **Invented** |

### 2. The Pattern in Code

#### 2.1 Implementation home — `crates/libaipm/src/discovery/instruction.rs`

The complete feature surface (read the file at
[`instruction.rs`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/discovery/instruction.rs)):

| Line(s) | Symbol / text | Role |
|---|---|---|
| 11-13 | Module-doc enum item 3 listing `<engine>-instructions.md` and naming all four filenames | DELETE |
| 32 | Doc comment "Engine prefixes accepted in the `<engine>-instructions.md` shape." | DELETE |
| 33 | `const ENGINE_INSTRUCTION_PREFIXES: &[&str] = &["copilot", "claude", "agents", "gemini"];` | DELETE |
| 69 | `\|\| matches_engine_instructions(file_name_lower)` — third disjunct in `is_instruction_filename` | DELETE |
| 72-73 | Doc comment for `matches_engine_instructions` | DELETE |
| 74-79 | `fn matches_engine_instructions(...) -> bool` body | DELETE |
| 143 | Comment `// --- Case C: <engine>-instructions.md (the #725 fix) ---` | DELETE |
| 145-150 | `copilot_instructions_md_matches_issue_725` test | DELETE |
| 152-155 | `claude_instructions_md_matches` test | DELETE |
| 157-160 | `agents_instructions_md_matches` test | DELETE |
| 162-165 | `gemini_instructions_md_matches` test | DELETE |
| 167-171 | `engine_instructions_md_case_insensitive` test | DELETE |
| 180-184 | `instructions_copilot_md_wrong_order_no_match` (negative test guarding the deleted branch) | DELETE |
| 186-191 | `unknown_engine_prefix_no_match` (negative test for `cursor-instructions.md`, references `ENGINE_INSTRUCTION_PREFIXES`) | DELETE |
| 193-197 | `copilot_tools_md_does_not_match` (negative test for the deleted branch) | DELETE |
| 199-203 | `copilot_instructions_md_with_extra_suffix_no_match` (negative test for the deleted branch) | DELETE |
| 210-214 | `just_dash_instructions_md_no_match` (negative test for `-instructions.md`, only meaningful with the deleted branch) | DELETE |
| 218-228 | `classify_returns_path_unchanged` test — uses `copilot-instructions.md` as its sample input. **Rewrite** to use a `INSTRUCTION_FILENAMES` filename (e.g. `CLAUDE.md`) so the structural assertion remains | EDIT |

The legitimate parts (must stay):

| Line(s) | Symbol / text | Reason to keep |
|---|---|---|
| 5-7 | Module doc enumerating Case 1 (`INSTRUCTION_FILENAMES`) | Real engine convention |
| 8-9 | Module doc enumerating Case 2 (`*.instructions.md`) | Real Copilot/VS Code convention |
| 26-30 | `pub const INSTRUCTION_FILENAMES: &[&str] = &["claude.md", "agents.md", "copilot.md", "instructions.md", "gemini.md"];` | Engines DO read `CLAUDE.md`, `AGENTS.md`, `GEMINI.md`, etc. |
| 67-68 | First two disjuncts in `is_instruction_filename` (`INSTRUCTION_FILENAMES.contains` and `ends_with(".instructions.md")`) | Match real engine behavior |
| 92-141 | Case A and Case B unit tests | Cover the legitimate branches |

After the cut, `is_instruction_filename` should reduce to two OR'd branches and
the file should drop from ~237 lines to roughly ~140.

#### 2.2 Other source files referencing `copilot-instructions.md`

These are not implementation — they are test fixtures or doc-comment examples
that bake the path `.github/copilot/copilot-instructions.md` into structural
tests of the unified discovery pipeline. After removing the engine-prefix
branch, the file would no longer classify as `Instructions`, so each of these
fixtures must be either removed or replaced with a filename that still
classifies (e.g., `CLAUDE.md` or `my-thing.instructions.md`).

| File | Line(s) | What it does |
|---|---|---|
| `crates/libaipm/src/discovery/mod.rs` | 122-136 (test `discover_unified_finds_issue_725_tree`), 149-164 (test `discover_finds_copilot_instructions_md`) | Writes `.github/copilot/copilot-instructions.md` to a tempdir and asserts the unified `discover()` finds it. The second test exists solely to assert the engine-prefix branch's effect; **delete it**. The first test should be edited to drop the `copilot-instructions.md` line and keep the SKILL.md assertions (the legitimate #725 fix). |
| `crates/libaipm/src/discovery/walker.rs` | 209-223 (test `issue_725_tree_visible_to_walker`) | Asserts `walker::walk` returns the file path. Walker is layer-below classification, so this test passes regardless of classifier behavior. **Edit** to drop the `copilot-instructions.md` line, or rename the test to focus on skill-tree visibility. |
| `crates/libaipm/src/discovery/classify.rs` | 122-130 (test `copilot_instructions_md_classified_as_instructions`), 162-175 (test `issue_725_full_tree_dispatch`) | `copilot_instructions_md_classified_as_instructions` is the dispatcher-side mirror of the `instruction.rs` Case-C tests. **Delete** the first; **edit** the second to drop the `copilot-instructions.md` assertion line. |
| `crates/libaipm/src/discovery/types.rs` | 169 (test `discovered_feature_clone_and_eq`) | Uses `.github/copilot/copilot-instructions.md` as a sample path in a Clone/Eq round-trip test. **Edit** to use any other path string — the value is incidental. |
| `crates/libaipm/src/lint/mod.rs` | 1583-1628 (test `lint_unified_finds_issue_725_skills_and_instructions`), 1630-1644+ (test `lint_outcome_carries_scan_counts_and_dirs`) | These tests assert that the lint command sees the instruction file via the unified pipeline and that `counts.instructions == 1`. **Edit** the fixture writer (`write_issue_725_tree`) to drop the `copilot-instructions.md` line, and update the count assertions to drop the `+1`. The skills-side assertions remain. |

#### 2.3 Where `FeatureKind::Instructions` is consumed (downstream effect of the cut)

Three call sites in production code (verified from
[`scan_report.rs:83`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/discovery/scan_report.rs#L83),
[`lint/mod.rs:115`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/lint/mod.rs#L115),
[`lint/rules/mod.rs:149-168`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/lint/rules/mod.rs#L149-L168)):

1. `scan_report::counts` increments `counts.instructions` for each
   `FeatureKind::Instructions`. After the cut, files literally named
   `copilot-instructions.md` (etc.) at non-canonical paths stop being counted.
2. `lint::run_rules_for_feature` exempts `Instructions` from
   `source/misplaced-features`. After the cut, those files would either be
   skipped entirely (no `FeatureKind` assigned, classification falls through to
   `tracing::debug!` at
   [`discovery/mod.rs:93`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/discovery/mod.rs#L93))
   or, if they fall under a marketplace source root, be flagged as misplaced —
   but in practice they sit in `.github/copilot/` which is not an `.ai/`
   subroot, so they are silently skipped.
3. `lint/rules/mod.rs::quality_rules_for_kind` selects the
   `instructions/oversized` rule for `Instructions` features. After the cut, an
   oversized `.github/copilot/copilot-instructions.md` would no longer trigger
   the rule. **This is the load-bearing behavior the spec's G7 acceptance
   criterion was protecting** — and per §1 above, that criterion was based on a
   misreading of Copilot's path requirement.

### 3. Test Infrastructure (Blast Radius)

#### 3.1 BDD step definition — `crates/libaipm/tests/bdd.rs`

| Line(s) | Item | Action |
|---|---|---|
| 791-807 | `#[given]` step `given_copilot_instructions_file_exists` (registered with `expr = "a copilot instructions file with {int} lines exists in {string}"`). Writes `.github/copilot/copilot-instructions.md` and sets `world.unified_discovery = true`. | **Delete** the step entirely. |
| 62, 118 | The `unified_discovery: bool` field on `World` (set by the deleted step) | If no other step references it, **delete** the field. Verify with grep before removal. |

#### 3.2 BDD scenario — `tests/features/guardrails/quality.feature`

| Line(s) | Item | Action |
|---|---|---|
| 51-54 | Comment block describing the issue #725 layout context | **Edit** — drop the `copilot-instructions.md` reference; keep the `.github/copilot/skills/` rationale (still valid for the SKILL.md side of #725). |
| 64-69 | Scenario `Lint flags oversized .github/copilot/copilot-instructions.md` (uses the deleted step from §3.1) | **Delete** the entire scenario. |

#### 3.3 E2E binary tests — `crates/aipm/tests/issue_725_e2e.rs`

The full file is 180 lines and four tests; all four currently rely on the
`.github/copilot/copilot-instructions.md` fixture and the resulting
`"1 instruction"` count in stderr.

| Lines | Item | Action |
|---|---|---|
| 5 | Doc comment naming the fixture path | **Edit** to drop the instruction-file mention. |
| 37 | Tree diagram `\`-- copilot-instructions.md` | **Delete** the line from the comment tree. |
| 39-58 | `build_issue_725_fixture` — writes 3 SKILL.md skills AND `copilot-instructions.md` | **Edit** to drop the `copilot-instructions.md` write at lines 55-57. |
| 78-81, 116-119, 175-178 | Three assertions on stderr containing `"matched 3 skills, 1 instruction"` | **Edit** to `"matched 3 skills"` (the exact format depends on `format_counts` — the rendering may collapse to `"matched 3 skills"` cleanly when `counts.instructions == 0`; verify with a dry run). |

The skill-side migrate fix from #725 (the legitimate part) remains covered by
these tests after the edits.

#### 3.4 Discovery-module unit tests already enumerated in §2

The unit tests in `instruction.rs`, `classify.rs`, `mod.rs`, `walker.rs`,
`types.rs`, and `lint/mod.rs` are itemized in §2.1 and §2.2 — those are
delete/edit calls per file:line.

### 4. Specs (Authoritative Source of the Misinterpretation)

#### 4.1 `specs/2026-05-01-unified-discovery-and-copilot-skill-detection.md`

This is the spec that introduced the feature. Removing the implementation
without correcting the spec leaves a stale acceptance contract.

| Line(s) | Content | Action |
|---|---|---|
| 16 | Executive-summary prose: "the accompanying `.github/copilot/copilot-instructions.md` is invisible to **lint**…" — the misreading of the Copilot doc | **Edit** — replace with a corrected reading: Copilot reads only `.github/copilot-instructions.md` (no nested `copilot/` segment); the lint-side gap was a misinterpretation. |
| 76-83 | Customer fixture diagram including `└── copilot-instructions.md` | **Edit** the tree to drop the `copilot-instructions.md` line. |
| 90 | Why-it-fails item 2: "Lint misses the instructions file…" | **Edit** to mark this point as withdrawn / based on a misreading. |
| 109 | Functional Goal **G7**: "`INSTRUCTION_FILENAMES` recognition is extended to match `<engine>-instructions.md`…" | **Delete** the goal entirely. |
| 213 | Module-layout comment: "`instruction.rs // INSTRUCTION_FILENAMES + the new <engine>-instructions.md regex`" | **Edit** to drop the `+ the new <engine>-instructions.md regex` clause. |
| 415-450 | Section **5.4 Instruction-file recognition (the `copilot-instructions.md` fix)** — full pseudocode for the regex and the three-case enumeration | **Delete** the entire section, OR rewrite to a two-case enumeration (table + suffix). |
| 632 | Decision-matrix row asserting Alternative A "doesn't address lint's `copilot-instructions.md` gap" | **Edit** to remove the citation as a strike against Alternative A. |
| 685 | Test-plan bullet: "covers `copilot-instructions.md`, `claude-instructions.md`, …" | **Edit** to drop the engine-prefix names; keep the table + suffix coverage. |
| 700 | Integration tree including `└── copilot-instructions.md` | **Edit** to drop the line. |
| 709 | "add a scenario for `copilot-instructions.md` triggering the `instructions_oversized` rule" | **Delete** this planned scenario. |
| 710 | "add a step `given_copilot_instructions_file_exists`" | **Delete** this planned step. |

#### 4.2 `specs/2026-04-11-lint-instructions-oversized.md`

| Line(s) | Content | Action |
|---|---|---|
| 36 | Prose: "Copilot CLI reads `CLAUDE.md`, `AGENTS.md`, and `GEMINI.md` in addition to its own `copilot-instructions.md`" | **No edit needed** — the prose is factually correct (it cites Copilot CLI's real file conventions) and does NOT advocate the `<engine>-instructions.md` pattern. |

### 5. User-Facing Docs

#### 5.1 `CHANGELOG.md`

| Line(s) | Content | Action |
|---|---|---|
| 10 | "**`aipm lint` now recognises `<engine>-instructions.md` files** — `copilot-instructions.md`, `claude-instructions.md`, `agents-instructions.md`, and `gemini-instructions.md` are all classified as instruction files. Closes the second silent-drop case from issue #725." | **Delete** the bullet. The bullet sits under `## [Unreleased] → ### Fixed`; if the section is now empty, also remove the heading. |
| 9 | Sibling bullet about `aipm migrate` finding skills under `.github/copilot/skills/` (the LEGITIMATE part of #725) | **Keep** — this is the SKILL.md fix the unified discovery refactor actually delivered. |

#### 5.2 `docs/rules/source/misplaced-features.md`

| Line | Content | Action |
|---|---|---|
| 62 | Tree-diagram example: `copilot-instructions.md   # ✅ exempt — *.instructions.md pattern` | **Edit** — the comment is misleading: `copilot-instructions.md` does NOT match the `*.instructions.md` suffix (it has no `.` before `instructions`). It only matched via the engine-prefix branch. Either delete the line or replace it with `my-thing.instructions.md` to keep the documentation accurate. |

### 6. In-Flight Research (`research/`)

These two files track the unified-discovery refactor in progress. They are
in-flight work logs, not historical archives; their accuracy matters for the
researcher who picks up the loop.

#### 6.1 `research/feature-list.json`

| Line(s) | Content | Action |
|---|---|---|
| 65 | Feature 5 description: "Instruction classifier with new `<engine>-instructions.md` regex (closes the `copilot-instructions.md` silent-drop gap)" | **Edit** — strike the engine-prefix half, keep the table + suffix scope. |
| 68 | Implementation note: "Implement `<engine>-instructions.md` detection via `str::strip_suffix` + `matches!`" | **Delete** the note. |
| 70-71 | Match logic mentioning `matches_engine_instructions`; long unit-test enumeration for the four prefixes | **Edit** — remove the engine-prefix unit-test enumeration. |
| 120 | Feature 7 unit-test description: "legacy drops `copilot-instructions.md` while unified finds it" | **Delete** — premise is invalid. |
| 133 | Feature 8 unit-test description: "`copilot-instructions.md` dropped (the actual #725 lint-side bug, fix lives in unified path)" | **Delete** — premise is invalid. |
| 239 | Feature 16 BDD-scenario description: "oversized `.github/copilot/copilot-instructions.md`" | **Delete** the scenario item. |
| 252 | Feature 17 e2e-test description listing `copilot-instructions.md` in the fixture | **Edit** to drop the file from the listed fixture. |
| 270 | Changelog-fragment description: "`<engine>-instructions.md` recognition" | **Delete** the fragment item. |

#### 6.2 `research/progress.txt`

| Line(s) | Content | Action |
|---|---|---|
| 99-101 | List of three classifier cases including Case C `<engine>-instructions.md` | **Edit** to two cases. |
| 135 | Testing summary mentioning `copilot-instructions.md` | **Edit** to drop the reference. |
| 173-179 | Post-implementation note that "the actual lint-side silent drop in #725 is the `copilot-instructions.md` filename" | **Edit** — append a 2026-05-02 correction noting the misinterpretation, OR delete the original note. |
| 195-206 | Documented divergence on project-root `CLAUDE.md` | **Keep** — separate concern. |
| 371 | Bullet "quality.feature: 2 new scenarios for skills under `.github/copilot/skills/` … and oversized `.github/copilot/copilot-instructions.md`" | **Edit** to drop the second scenario. |
| 411 | Plan for `### Fixed` CHANGELOG entry "`<engine>-instructions.md` filename recognition" | **Delete** — already in CHANGELOG; reconsider per §5.1. |
| 471 | "find `.github/copilot/copilot-instructions.md` when the user opts in via `AIPM_UNIFIED_DISCOVERY=1`" | **Edit** to drop the example file. |

### 7. Workflows / Config

#### 7.1 `.github/workflows/reverse-binary-analysis.md`

| Line | Content | Action |
|---|---|---|
| 142 | Prose example: "`.claude/settings.json`, `.github/copilot-instructions.md`" — illustrating what the binary analysis workflow looks for | **Keep** — this references the REAL Copilot path (`.github/copilot-instructions.md`, no nested `copilot/`), which the workflow correctly cites as a known engine convention. Not affected by the removal. |

---

## Code References

### Removable surface (the cut)

- `crates/libaipm/src/discovery/instruction.rs:11-13` — module-doc Case 3 enumeration
- `crates/libaipm/src/discovery/instruction.rs:32-33` — `ENGINE_INSTRUCTION_PREFIXES` const + doc
- `crates/libaipm/src/discovery/instruction.rs:69` — third disjunct in `is_instruction_filename`
- `crates/libaipm/src/discovery/instruction.rs:72-79` — `matches_engine_instructions` helper
- `crates/libaipm/src/discovery/instruction.rs:143-228` — Case-C unit tests + structural test using the deleted branch

### Test fixture sites needing edits/deletions

- `crates/libaipm/src/discovery/mod.rs:122-136, 149-164`
- `crates/libaipm/src/discovery/walker.rs:209-223`
- `crates/libaipm/src/discovery/classify.rs:122-130, 162-175`
- `crates/libaipm/src/discovery/types.rs:169`
- `crates/libaipm/src/lint/mod.rs:1583-1644+`
- `crates/libaipm/tests/bdd.rs:791-807` (and possibly the `unified_discovery: bool` field at lines 62, 118)
- `crates/aipm/tests/issue_725_e2e.rs:5, 37, 39-58, 78-81, 116-119, 175-178`
- `tests/features/guardrails/quality.feature:51-54, 64-69`

### Doc / spec / research / changelog edits

- `CHANGELOG.md:10`
- `docs/rules/source/misplaced-features.md:62`
- `specs/2026-05-01-unified-discovery-and-copilot-skill-detection.md:16, 76-83, 90, 109, 213, 415-450, 632, 685, 700, 709-710`
- `research/feature-list.json:65, 68, 70-71, 120, 133, 239, 252, 270`
- `research/progress.txt:99-101, 135, 173-179, 371, 411, 471`

### MUST stay (do NOT delete)

- `crates/libaipm/src/discovery/instruction.rs:5-7` — module-doc Case 1 (`INSTRUCTION_FILENAMES`)
- `crates/libaipm/src/discovery/instruction.rs:8-9` — module-doc Case 2 (`*.instructions.md`)
- `crates/libaipm/src/discovery/instruction.rs:26-30` — `pub const INSTRUCTION_FILENAMES`
- `crates/libaipm/src/discovery/instruction.rs:67-68` — first two disjuncts in `is_instruction_filename`
- `crates/libaipm/src/discovery/instruction.rs:92-141` — Case A and Case B unit tests

---

## Architecture Documentation

### Discovery-pipeline data flow (current state)

For `.github/copilot/copilot-instructions.md` today:

1. `lint::lint(opts, fs)` →
   [`lint/mod.rs:135-197`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/lint/mod.rs#L135-L197)
   builds `DiscoverOptions` and calls `discovery::discover`.
2. `discovery::discover` →
   [`discovery/mod.rs:75-98`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/discovery/mod.rs#L75-L98)
   calls `walker::walk`, then iterates the returned files and calls
   `classify::classify` per file.
3. `walker::walk` →
   [`discovery/walker.rs:53-114`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/discovery/walker.rs#L53-L114)
   — `ignore::WalkBuilder` with `hidden(false)`, `git_ignore(true)`, `SKIP_DIRS`
   filter. Returns every file path.
4. `classify::classify` →
   [`discovery/classify.rs:41-74`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/discovery/classify.rs#L41-L74)
   — calls `source::infer_engine_root` to determine `(engine, source_root)`,
   then `instruction::classify` with that engine. Instruction precedence runs
   BEFORE the layout-matchers (line 47-53 comment).
5. `instruction::classify` (the file in question) returns
   `Some(DiscoveredFeature { kind: Instructions, engine: Copilot, layout: Canonical, source_root: ".github", feature_dir: None, path })`
   via the engine-prefix branch.
6. Downstream:
   `lint/mod.rs:171-173` runs `run_rules_for_feature` per feature →
   [`lint/rules/mod.rs:149-168`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/lint/rules/mod.rs#L149-L168)
   selects `instructions_oversized::Oversized` for `Kind::Instructions`.

### Data flow after the cut

For the same path, step 5 returns `None` (none of the three branches matches —
`copilot-instructions.md` is not in `INSTRUCTION_FILENAMES` and does not end
with `.instructions.md`). The dispatcher's other layout-matchers also do not
match (`match_agent` requires `agents/` ancestor, `match_skill` requires
`SKILL.md` filename, etc.), so the dispatcher returns `None` and
`discovery::discover` logs a `tracing::debug!` at
[`discovery/mod.rs:93`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/discovery/mod.rs#L93)
and skips the file. The lint command emits no diagnostics for it; the scan
summary reports 0 instructions for the customer's fixture.

This is the correct behavior — Copilot does not read the file at this path, so
aipm has no engine-policy to enforce.

### What the implementer should consciously consider during the cut

- **The bare path `.github/copilot-instructions.md`** (no nested `copilot/`)
  IS a real Copilot convention. It does not match `INSTRUCTION_FILENAMES`
  (the table is lowercase `copilot.md`, not `copilot-instructions.md`), and it
  does not end with `.instructions.md`. After the cut, it will be skipped
  silently. If the project wants to honor this single real path, the cleanest
  approach is to add `"copilot-instructions.md"` to `INSTRUCTION_FILENAMES` (a
  one-line change to a static table) and gate it on
  `infer_engine_root(...) == Engine::Copilot && source_root.ends_with(".github")`.
  That preserves the legitimate part without resurrecting the engine-prefix
  family.
- **The `instructions_oversized` rule** is the only consumer of
  `FeatureKind::Instructions` and currently runs on every classified
  instruction file regardless of path. Whether this rule should run on the
  bare `.github/copilot-instructions.md` path is a separate policy question
  the implementer should answer explicitly rather than inherit by accident.

---

## Historical Context (from `research/`)

Bibliographic survey of relevant prior research:

### Tickets

- `research/tickets/2026-04-11-185-prevent-long-instructions-files.md` —
  Defines the `instructions/oversized` rule. Treats
  `.github/copilot-instructions.md` as a fixed path (not a wildcarded family).
  Documents the `INSTRUCTION_FILENAMES` table seed. **Stance:** documents the
  legitimate cases; not the source of the engine-prefix family.
- `research/tickets/2026-04-11-426-dogfood-aipm-lint.md` — Describes
  `FeatureKind::Instructions` as a downstream consumer pattern. **Stance:**
  documents the architecture; no opinion on filename matching.
- `research/tickets/2026-05-01-510-aipm-toml-engines.md` — Cites
  `instructions/oversized` as the configurable-rule pattern for new rules.
  **Stance:** orthogonal — no opinion on instruction filename matching.

### Docs

- `research/docs/2026-03-16-claude-code-defaults.md` — Catalogs Claude Code's
  filename convention (`CLAUDE.md` only). **Stance:** evidence file for the
  deletion (no `claude-instructions.md` mentioned anywhere).
- `research/docs/2026-03-28-copilot-cli-source-code-analysis.md` — Reverse-
  engineers the Copilot CLI binary; Section 6 enumerates the exact filenames
  the binary reads, including `.github/copilot-instructions.md` (bare, no
  nested `copilot/`). **Stance:** primary evidence file for the deletion;
  proves the path constraint.
- `research/docs/2026-03-10-microsoft-apm-analysis.md` — Documents that apm
  generates `AGENTS.md` / `CLAUDE.md`, not `<engine>-instructions.md`.
  **Stance:** corroborating evidence.
- `research/docs/2026-04-01-migrate-file-discovery-classification.md` —
  Migrate-side discovery research. **Stance:** orthogonal.
- `research/docs/2026-05-01-github-copilot-skills-migrate-lint-silent-failure.md`
  — Primary research for issue #725. Focuses on `SKILL.md` discovery, not
  `copilot-instructions.md`. **Stance:** ambiguous — its incidental mentions
  of `copilot-instructions.md` were the seed the spec misread, but the doc
  itself does not advocate the engine-prefix family.
- `research/docs/2026-05-01-engine-tool-references.md` — Tool-name catalogs
  (engine *tools*, not engine *instruction filenames*). **Stance:** not
  relevant to this deletion.

### Root research files (in-flight work logs)

- `research/feature-list.json` — Argues for the engine-prefix pattern (items 5,
  7, 8, 16, 17, 25). Needs edits per §6.1.
- `research/progress.txt` — Implementation diary that argues for the pattern.
  Needs edits per §6.2.

The pattern is introduced in `feature-list.json` and `progress.txt` ONLY — no
prior research doc proposes the family. The closest precursors
(`2026-04-11-185-...` and `2026-03-28-copilot-cli-source-code-analysis.md`)
both document the actual single-path Copilot convention rather than a
generalized engine-prefix family. The leap from "support
`.github/copilot-instructions.md`" to "support `<engine>-instructions.md` for
any engine, anywhere" happened during implementation, not during research.

---

## Related Research

- [`research/docs/2026-05-01-engine-tool-references.md`](./2026-05-01-engine-tool-references.md)
  — Tool-name catalogs (different topic; same date).
- [`research/tickets/2026-05-01-510-aipm-toml-engines.md`](../tickets/2026-05-01-510-aipm-toml-engines.md)
  — Engines field / init wizard / `agent/valid-tool-name` rule research.
- [`research/docs/2026-03-28-copilot-cli-source-code-analysis.md`](./2026-03-28-copilot-cli-source-code-analysis.md)
  — Binary-derived Copilot CLI behavior; cites `.github/copilot-instructions.md`
  as the only path Copilot reads.
- [`research/docs/2026-03-16-claude-code-defaults.md`](./2026-03-16-claude-code-defaults.md)
  — Claude Code reads `CLAUDE.md` only.
- [`research/tickets/2026-04-11-185-prevent-long-instructions-files.md`](../tickets/2026-04-11-185-prevent-long-instructions-files.md)
  — Origin of the `INSTRUCTION_FILENAMES` table and `instructions/oversized`
  rule.
- [`research/docs/2026-05-01-github-copilot-skills-migrate-lint-silent-failure.md`](./2026-05-01-github-copilot-skills-migrate-lint-silent-failure.md)
  — Issue #725's primary research (focuses on SKILL.md, not
  `copilot-instructions.md`).

---

## Open Questions

1. **Should `.github/copilot-instructions.md` (the bare path) be honored after
   the cut?** This is a real Copilot convention, but it is currently matched
   ONLY via the engine-prefix branch. After deletion, it will be silently
   skipped. The cleanest narrow fix is to add `"copilot-instructions.md"` to
   `INSTRUCTION_FILENAMES` and gate downstream rules on
   `infer_engine_root(...) == Engine::Copilot`. Decide explicitly rather than
   default.
2. **Should the spec document be retracted or amended?** Section 5.4 of
   `specs/2026-05-01-unified-discovery-and-copilot-skill-detection.md`
   describes a feature that is being un-implemented. Two options: (a) delete
   the section and the G7 acceptance criterion, leaving the SKILL.md
   discovery sections intact; (b) add a "Withdrawn" notice at the top of the
   spec citing this research doc as the rationale.
3. **What happens to issue #725?** The migrate-side fix (skill detection at
   `.github/copilot/skills/...`) is real and correct. The lint-side fix (this
   feature) was based on a misreading. Reopen the issue with a comment, or
   update its post-fix description, or both.
4. **`AIPM_UNIFIED_DISCOVERY` env-var feature flag** — referenced in
   `research/progress.txt:471` and the in-flight feature-list. If the unified
   discovery refactor is otherwise correct (and it is — the SKILL.md path is
   the customer's real complaint), the env-var gate stands. Only the
   instruction-classifier branch within it needs the cut.
5. **The misleading comment at `docs/rules/source/misplaced-features.md:62`**
   says `copilot-instructions.md` is exempt via `*.instructions.md`. That has
   been false the whole time (the file does not match that suffix). Whether
   to delete the line or replace its example with a real
   `*.instructions.md`-matching name is a docs-fidelity decision.
6. **Whether to backfill a regression test** asserting that
   `.github/copilot/copilot-instructions.md` is NOT classified as
   `Instructions` — to prevent the engine-prefix family from being
   re-introduced by a future agent loop reading the same primary research
   docs and reaching the same misinterpretation.
