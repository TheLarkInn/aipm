# Changelog

All notable changes to this project will be documented in this file.
## [0.19.0] - 2026-04-07

### Bug Fixes
- Correct help_text direction for hook/legacy-event-name rule ([#299](https://github.com/TheLarkInn/aipm/pull/299)) (33e35f0)

### Documentation
- Add missing guides, docs index, and fix lint path matching docs ([#268](https://github.com/TheLarkInn/aipm/pull/268)) (e55a9fb)
- Cross-link lint.md with configuring-lint.md and README ([#272](https://github.com/TheLarkInn/aipm/pull/272)) (5ddc7c9)
- Fix marketplace spec format and document mp: alias ([#279](https://github.com/TheLarkInn/aipm/pull/279)) (da9ec77)
- Add verbosity & logging guide and complete global flags reference ([#300](https://github.com/TheLarkInn/aipm/pull/300)) (a949eb3)

### Features
- Add marketplace and plugin.json lint rules (#287, #288, #289, #290) ([#296](https://github.com/TheLarkInn/aipm/pull/296)) (bef9f3d)

### Testing
- Cover empty cache index and reconciler raw_content branches (f988cc4)
- Cover three missed branches (b5ca98d)
- Cover unknown-field branch in output_style_detector and read_dir error in skill_detector (581b787)
- Cover unknown migrate source fallback and adaptor error paths (3d170a6)
- Cover fs.exists collision branch in emit_other_files (dcd2e5a)
- Cover hook/rewrite missed branches for 90.83% branch coverage (f1b48f8)
- Cover quote/tab/dollar terminator branches in extract_script_references (b9aab2f)
- Cover empty-files branch in write_artifact_section (c25a6c3)

## [0.18.3] - 2026-04-07

### Documentation
- Add lint configuration guide and workspace.lints README example ([#263](https://github.com/TheLarkInn/aipm/pull/263)) (28b77d6)
- Add `aipm lint` and `aipm migrate` how-to guides ([#266](https://github.com/TheLarkInn/aipm/pull/266)) (d750692)

## [0.18.2] - 2026-04-07

### Documentation
- Document install/update/uninstall/link/unlink/list/lint commands and new libaipm modules ([#244](https://github.com/TheLarkInn/aipm/pull/244)) (fa03dcf)

### Testing
- Cover None branch when no artifacts but other files present (8c8a2f0)
- Cover glob_match middle-part-not-found else branch (2fed3ec)
- Cover missed branches in installed::Registry::resolve_spec (28867bd)
- Cover uncovered branches in skill_name_invalid check_file (1253abe)
- Cover check_file no-frontmatter branch in skill rules (9f8c017)
- Cover double-quote and single-quote terminators in check_file (e876d99)
- Cover None branch of path.parent() in write_gitignore (875fd3e)
- Cover empty-URL branch in parse_git_spec (5ecdf5e)
- Cover is_env_enforced closure branches via AIPM_ENFORCE_ALLOWLIST (fe8c0e4)
- Cover is_valid_segment empty-string guard branch (36e5d7c)
- Cover duplicate source-path branch in emit_package_plugin (ec5ee04)
- Cover is_local_path non-alphabetic first-char branch (940e462)
- Cover as_git, as_marketplace, and error-path branches in spec.rs (74814cd)
- Cover scan_agents plugin-is-file branch (a3a1ce3)

## [0.18.1] - 2026-04-06

### Testing
- Cover find_map else-None branch with multi-line hook JSON (7301691)
- Cover None branch of path.parent() in open() (f9f9de6)
- Cover per-rule ignore path True branch in apply_rule_diagnostics (7c213d8)

## [0.18.0] - 2026-04-06

### Features
- Plugin acquisition system with multi-source support ([#233](https://github.com/TheLarkInn/aipm/pull/233)) (379a06d)

### Testing
- Cover structural-key skips in check_file for hook/unknown-event (917f28b)
- Cover CopilotMcpDetector root source-dir else branch (fb8c639)
- Cover Human reporter Renderer::styled() branch (0426d35)
- Cover all error branches in register_plugins (72112a4)

## [0.17.7] - 2026-04-06

## [0.17.6] - 2026-04-05

## [0.17.5] - 2026-04-05

## [0.17.4] - 2026-04-05

## [0.17.3] - 2026-04-05

## [0.17.2] - 2026-04-05

### Features
- Unified single-pass feature discovery fixes #208 ([#211](https://github.com/TheLarkInn/aipm/pull/211)) (ec24fe7)

## [0.17.1] - 2026-04-04

## [0.17.0] - 2026-04-03

### Features
- Lint display UX improvements ([#198](https://github.com/TheLarkInn/aipm/pull/198)) ([#203](https://github.com/TheLarkInn/aipm/pull/203)) (42d9a01)

## [0.16.1] - 2026-04-03

### Features
- Implement verbosity levels across aipm ([#189](https://github.com/TheLarkInn/aipm/pull/189)) ([#195](https://github.com/TheLarkInn/aipm/pull/195)) (7a06b09)

## [0.16.0] - 2026-04-02

### Features
- Use recursive discovery for lint misplaced-features rule ([#190](https://github.com/TheLarkInn/aipm/pull/190)) (4b1cab6)

## [0.15.1] - 2026-04-02

## [0.15.0] - 2026-04-01

### Features
- Detect, report, and migrate unclaimed files during aipm migrate ([#177](https://github.com/TheLarkInn/aipm/pull/177)) (40afc2f)

### Breaking changes
- `PluginPlan` now exposes a new public field. Code that constructs `PluginPlan` directly may need to be updated to initialize this field.
- The `Action` enum gained additional variants. Exhaustive `match` statements over `Action` must be updated to handle the new cases.
- `dry_run::generate_report` had its arity changed. Call sites must be updated to pass the new set of arguments in the correct order.
## [0.14.9] - 2026-04-01

## [0.14.8] - 2026-04-01

### Bug Fixes
- Correct GitHub latest release download URL format in README installers ([#171](https://github.com/TheLarkInn/aipm/pull/171)) (99b7ec5)

## [0.14.7] - 2026-04-01

### Features
- Add `aipm lint` command with 12 rules ([#168](https://github.com/TheLarkInn/aipm/pull/168)) (1020f35)

## [0.14.6] - 2026-03-31

## [0.14.5] - 2026-03-31

### Performance
- System libgit2 in CI + replace reqwest with ureq ([#162](https://github.com/TheLarkInn/aipm/pull/162)) (a908dc9)

## [0.14.4] - 2026-03-31

## [0.14.3] - 2026-03-31

## [0.14.2] - 2026-03-30

## [0.14.1] - 2026-03-29

### Bug Fixes
- Read plugins_dir from manifest and skip self-links ([#148](https://github.com/TheLarkInn/aipm/pull/148)) (dfc3e1c)

## [0.14.0] - 2026-03-28

### Features
- Workspace dependencies resolve and link locally ([#144](https://github.com/TheLarkInn/aipm/pull/144)) (13630b8)

## [0.13.0] - 2026-03-28

### Features
- Add Copilot CLI migrate adapter with 6 new detectors ([#140](https://github.com/TheLarkInn/aipm/pull/140)) (e0b6398)

## [0.12.1] - 2026-03-28

### Miscellaneous
- Update Cargo.toml dependencies

## [0.12.0] - 2026-03-28

### Features
- Add --destructive flag and cleanup wizard ([#130](https://github.com/TheLarkInn/aipm/pull/130)) (225dbd0)

## [0.11.6] - 2026-03-28

### Testing
- Cover load() success path and fix &&-branch miss (6f60b0a)
- Use separate assertions in edition_field_rejected (a3c72ba)

## [0.11.5] - 2026-03-27

### Testing
- Cover manifest load IO error and lockfile write no-parent branch (2c4c257)
- Address review comments — use tempdir and matches! for coverage tests (cfa6b00)

## [0.11.4] - 2026-03-27

### Testing
- Cover starts_with false branch when non-scoped packages present (e0645da)

## [0.11.3] - 2026-03-27

### Testing
- Cover lone-quote guard-fail branch in strip_yaml_quotes (83ac701)

## [0.11.2] - 2026-03-26

### Bug Fixes
- Address PR #104 review comments ([#107](https://github.com/TheLarkInn/aipm/pull/107)) (fa0f2f7)
- Address PR #107 review comments ([#108](https://github.com/TheLarkInn/aipm/pull/108)) (ddb34b1)

## [0.11.1] - 2026-03-26

### Documentation
- Rewrite README with full API docs, roadmap, and apm comparison ([#105](https://github.com/TheLarkInn/aipm/pull/105)) (1aa3cac)

### Features
- Implement install, update, link, lockfile, registry, and resolver pipeline ([#104](https://github.com/TheLarkInn/aipm/pull/104)) (a75d54e)

## [0.11.0] - 2026-03-26

### Refactoring
- Remove edition field from aipm.toml manifests ([#102](https://github.com/TheLarkInn/aipm/pull/102)) (a6c2374)

## [0.10.1] - 2026-03-26

### Features
- Add .tool-usage.log to .ai/.gitignore when starter plugin is installed ([#98](https://github.com/TheLarkInn/aipm/pull/98)) (23796b5)

## [0.10.0] - 2026-03-25

### Features
- Add --name flag to customize marketplace name in aipm init ([#71](https://github.com/TheLarkInn/aipm/pull/71)) (e9a4657)

## [0.9.1] - 2026-03-25

### Bug Fixes
- Marketplace.json descriptions match plugin.json during migrate ([#69](https://github.com/TheLarkInn/aipm/pull/69)) (9588c60)

## [0.9.0] - 2026-03-25

### Bug Fixes
- Respect --no-starter flag in Claude Code adaptor settings.json ([#67](https://github.com/TheLarkInn/aipm/pull/67)) (7e4c41b)

## [0.8.2] - 2026-03-24

### Bug Fixes
- Use serde_json/toml serialization in migrate emitter to prevent invalid output ([#65](https://github.com/TheLarkInn/aipm/pull/65)) (d72aed1)

## [0.8.1] - 2026-03-24

### Testing
- Improve branch coverage and cross-platform test assertions ([#63](https://github.com/TheLarkInn/aipm/pull/63)) (988f585)

## [0.8.0] - 2026-03-24

### Breaking changes
- `libaipm::artifacts::ArtifactKind` has gained new enum variants. Code that matches exhaustively on `ArtifactKind` (for example, using `match` without a wildcard arm) may need to be updated to handle the additional variants.
- `libaipm::artifacts::ArtifactMetadata` has gained a new `raw_content` field. Call sites that construct `ArtifactMetadata` directly via struct literals must be updated to initialize this field.
### Features
- Extend aipm migrate to all .claude/ artifact types ([#61](https://github.com/TheLarkInn/aipm/pull/61)) (10f5be4)

## [0.7.0] - 2026-03-24

### Features
- Suppress plugin manifest generation by default ([#59](https://github.com/TheLarkInn/aipm/pull/59)) (10c5aad)

## [0.6.0] - 2026-03-24

### Breaking changes
- `libaipm::fs::Fs` now requires `Send + Sync`. Any implementations, type aliases, or usages of `Fs` must satisfy these additional trait bounds.
- `libaipm::migrate::Options` has gained a new `max_depth` field. Call sites that construct `Options` directly (including via struct literals) must be updated to initialize this field.
- `libaipm::migrate::Error` has gained a new enum variant. Code that matches on `migrate::Error` may need to be updated to handle the additional variant, especially if using non-exhaustive match patterns.
### Features
- Add recursive .claude/ discovery to aipm migrate ([#57](https://github.com/TheLarkInn/aipm/pull/57)) (5313d5e)

## [0.5.0] - 2026-03-23

### Features
- Add aipm migrate command ([#55](https://github.com/TheLarkInn/aipm/pull/55)) (237f240)

## [0.4.3] - 2026-03-23

## [0.4.2] - 2026-03-23

## [0.4.1] - 2026-03-22

### Documentation
- Add CI and Codecov badges to README ([#48](https://github.com/TheLarkInn/aipm/pull/48)) (df75b44)

## [0.4.0] - 2026-03-22

### Breaking changes
- `libaipm::init::init` now accepts additional parameters to configure initialization. Call sites must be updated to pass the new arguments (or use the new configuration type) when upgrading to this release.
- `libaipm::workspace_init::init` has also gained additional parameters for workspace initialization. Existing callers need to be adjusted to supply the new arguments.
- `ToolAdaptor::apply` has a changed method signature (for example, to receive additional context/inputs). Any implementors and callers of this method must update their implementations and call sites to match the new signature.
### CI/CD
- Enforce 90% branch coverage as correctness gate ([#46](https://github.com/TheLarkInn/aipm/pull/46)) (40c9a04)

## [0.3.5] - 2026-03-21

## [0.3.4] - 2026-03-21

### Bug Fixes
- scaffold-plugin.ts registers new plugins in marketplace.json and settings.json ([#42](https://github.com/TheLarkInn/aipm/pull/42)) (5693c6f)

## [0.3.3] - 2026-03-20

### Bug Fixes
- Correct settings.json schema for enabledPlugins and path ([#40](https://github.com/TheLarkInn/aipm/pull/40)) (0026380)

## [0.3.2] - 2026-03-20

### Testing
- Add marketplace.json output tests across all layers ([#38](https://github.com/TheLarkInn/aipm/pull/38)) (e4d2c8a)

## [0.3.1] - 2026-03-20

### Bug Fixes
- Add marketplace.json to scaffold and rename starter plugin ([#36](https://github.com/TheLarkInn/aipm/pull/36)) (ae6f1f6)

## [0.3.0] - 2026-03-20

## [0.2.1] - 2026-03-19

## [0.2.0] - 2026-03-19

## [0.1.2] - 2026-03-19

## [0.1.1] - 2026-03-19
