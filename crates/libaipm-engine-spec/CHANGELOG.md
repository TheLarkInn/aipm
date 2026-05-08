# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Bug Fixes
- Schema fix: Claude marketplace manifest path corrected from `.claude-plugin/marketplace.toml` to `.claude-plugin/marketplace.json` to match the format Claude actually consumes. `engine::marketplace_manifest_path(Engine::Claude)` now returns `.json` (was `.toml`); the data file in `data/engine-api-schema.json` already declared `.json` in `manifest_search_paths`, only the `marketplace_manifest_path_for("claude")` builder lookup was out of step ([#850](https://github.com/TheLarkInn/aipm/issues/850)).

## [0.24.2] - 2026-05-07

### Documentation
- Add `libaipm-engine-spec` crate reference to README ([#809](https://github.com/TheLarkInn/aipm/pull/809)) (180fd96)
- Fix `VALID_TOOLS` type and description in `libaipm-engine-spec` reference table ([#815](https://github.com/TheLarkInn/aipm/pull/815)) (e3508d7)

## [0.24.1] - 2026-05-06

## [0.24.0] - 2026-05-05

### Bug Fixes
- Correct GitHub latest release download URL format in README installers ([#171](https://github.com/TheLarkInn/aipm/pull/171)) (99b7ec5)

### Documentation
- Add CI and Codecov badges to README ([#48](https://github.com/TheLarkInn/aipm/pull/48)) (df75b44)
- Rewrite README with full API docs, roadmap, and apm comparison ([#105](https://github.com/TheLarkInn/aipm/pull/105)) (1aa3cac)
- Document install/update/uninstall/link/unlink/list/lint commands and new libaipm modules ([#244](https://github.com/TheLarkInn/aipm/pull/244)) (fa03dcf)
- Add lint configuration guide and workspace.lints README example ([#263](https://github.com/TheLarkInn/aipm/pull/263)) (28b77d6)
- Add `aipm lint` and `aipm migrate` how-to guides ([#266](https://github.com/TheLarkInn/aipm/pull/266)) (d750692)
- Add missing guides, docs index, and fix lint path matching docs ([#268](https://github.com/TheLarkInn/aipm/pull/268)) (e55a9fb)
- Cross-link lint.md with configuring-lint.md and README ([#272](https://github.com/TheLarkInn/aipm/pull/272)) (5ddc7c9)
- Fix marketplace spec format and document mp: alias ([#279](https://github.com/TheLarkInn/aipm/pull/279)) (da9ec77)
- Add verbosity & logging guide and complete global flags reference ([#300](https://github.com/TheLarkInn/aipm/pull/300)) (a949eb3)
- Document `aipm lsp` command in README ([#403](https://github.com/TheLarkInn/aipm/pull/403)) (5714aa1)
- Add VS Code extension guide and aipm lsp command reference ([#411](https://github.com/TheLarkInn/aipm/pull/411)) (8dfd32f)
- Add VS Code extension setup guide and fix project structure ([#412](https://github.com/TheLarkInn/aipm/pull/412)) (9582f5a)
- Add editor schema support section to README ([#419](https://github.com/TheLarkInn/aipm/pull/419)) (d333739)
- Add missing install guide links to `aipm install` See also section ([#422](https://github.com/TheLarkInn/aipm/pull/422)) (faa0849)
- Fix Claude Code LSP artifact listing in README and add v0.19.7 changelog ([#425](https://github.com/TheLarkInn/aipm/pull/425)) (b81ea0d)
- Add instruction file patterns to VS Code LSP document selector ([#466](https://github.com/TheLarkInn/aipm/pull/466)) (8497d8d)
- Add `instructions/oversized` example to workspace lints in README ([#492](https://github.com/TheLarkInn/aipm/pull/492)) (46c94fb)
- Update README to reflect 18-rule lint coverage and `instructions/oversized` example ([#487](https://github.com/TheLarkInn/aipm/pull/487)) (3c88808)
- Add uninstall guide cross-reference to README `aipm uninstall` section ([#500](https://github.com/TheLarkInn/aipm/pull/500)) (0acf52a)
- Add `generate` and `wizard` modules to libaipm reference table ([#503](https://github.com/TheLarkInn/aipm/pull/503)) (2de0d46)
- Add `aipm init` workspace initialization guide ([#505](https://github.com/TheLarkInn/aipm/pull/505)) (3d38565)
- Add `aipm make plugin` command documentation ([#513](https://github.com/TheLarkInn/aipm/pull/513)) (7813296)
- Add `aipm update` guide and lockfile semantics reference ([#508](https://github.com/TheLarkInn/aipm/pull/508)) (2b94a6c)
- Add `make` to README command table and missing `See also` links ([#516](https://github.com/TheLarkInn/aipm/pull/516)) (8506b00)
- Fix init paths, registry caveat, aipm-pack note, and roadmap status markers ([#517](https://github.com/TheLarkInn/aipm/pull/517)) (49a36db)
- Add `--engine both` example to README `aipm make plugin` section ([#546](https://github.com/TheLarkInn/aipm/pull/546)) (c4c394e)
- Fix incorrect marketplace directory path in README `aipm init` section ([#567](https://github.com/TheLarkInn/aipm/pull/567)) (22c161f)
- Add Azure DevOps NuGet installation guide ([#664](https://github.com/TheLarkInn/aipm/pull/664)) (80fe35c)
- Bump example AIPM_VERSION to 0.22.4 ([#704](https://github.com/TheLarkInn/aipm/pull/704)) (12d758c)
- Add NuGet installation guide and update CHANGELOG for v0.22.4 ([#702](https://github.com/TheLarkInn/aipm/pull/702)) (fbb0eca)

### Features
- Merge aipm-pack into aipm ([#417](https://github.com/TheLarkInn/aipm/pull/417)) ([#522](https://github.com/TheLarkInn/aipm/pull/522)) (eab803d)
- Automatic nuget.org publishing pipeline ([#651](https://github.com/TheLarkInn/aipm/pull/651)) (134d94a)
- Engine API schema source-of-truth (libaipm-engine-spec crate) ([#771](https://github.com/TheLarkInn/aipm/pull/771)) (14a7f4f)
