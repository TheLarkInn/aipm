# Changelog

All notable changes to this project will be documented in this file.
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
