# Changelog

All notable changes to this project will be documented in this file.
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

## [0.14.9] - 2026-04-01

## [0.14.8] - 2026-04-01

### Bug Fixes
- Correct GitHub latest release download URL format in README installers ([#171](https://github.com/TheLarkInn/aipm/pull/171)) (99b7ec5)

## [0.14.7] - 2026-04-01

### Features
- Add `aipm lint` command with 12 rules ([#168](https://github.com/TheLarkInn/aipm/pull/168)) (1020f35)

## [0.14.6] - 2026-03-31

## [0.14.5] - 2026-03-31

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

## [0.12.1] - 2026-03-28

## [0.12.0] - 2026-03-28

### Features
- Add --destructive flag and cleanup wizard ([#130](https://github.com/TheLarkInn/aipm/pull/130)) (225dbd0)

## [0.11.6] - 2026-03-28

## [0.11.5] - 2026-03-27

## [0.11.4] - 2026-03-27

## [0.11.3] - 2026-03-27

## [0.11.2] - 2026-03-26

## [0.11.1] - 2026-03-26

### Documentation
- Rewrite README with full API docs, roadmap, and apm comparison ([#105](https://github.com/TheLarkInn/aipm/pull/105)) (1aa3cac)

### Features
- Implement install, update, link, lockfile, registry, and resolver pipeline ([#104](https://github.com/TheLarkInn/aipm/pull/104)) (a75d54e)

## [0.11.0] - 2026-03-26

## [0.10.1] - 2026-03-26

## [0.10.0] - 2026-03-25

### Features
- Add --name flag to customize marketplace name in aipm init ([#71](https://github.com/TheLarkInn/aipm/pull/71)) (e9a4657)

## [0.9.1] - 2026-03-25

### Bug Fixes
- Marketplace.json descriptions match plugin.json during migrate ([#69](https://github.com/TheLarkInn/aipm/pull/69)) (9588c60)

## [0.9.0] - 2026-03-25

## [0.8.2] - 2026-03-24

### Bug Fixes
- Use serde_json/toml serialization in migrate emitter to prevent invalid output ([#65](https://github.com/TheLarkInn/aipm/pull/65)) (d72aed1)

## [0.8.1] - 2026-03-24

## [0.8.0] - 2026-03-24

### Features
- Extend aipm migrate to all .claude/ artifact types ([#61](https://github.com/TheLarkInn/aipm/pull/61)) (10f5be4)

## [0.7.0] - 2026-03-24

### Features
- Suppress plugin manifest generation by default ([#59](https://github.com/TheLarkInn/aipm/pull/59)) (10c5aad)

## [0.6.0] - 2026-03-24

### Features
- Change `aipm migrate` default behavior: when `--source` is omitted, migrations are now discovered **recursively** under the current working directory (searching nested `.claude/` directories) ([#57](https://github.com/TheLarkInn/aipm/pull/57)) (5313d5e)
- Add a new `--max-depth` flag to `aipm migrate` to limit how deep the recursive `.claude/` discovery searches, allowing users to constrain the directories scanned.

## [0.5.0] - 2026-03-23

### Features
- Add aipm migrate command ([#55](https://github.com/TheLarkInn/aipm/pull/55)) (237f240)

## [0.4.3] - 2026-03-23

### Refactoring
- Surgical coverage exclusion for wizard TTY code ([#52](https://github.com/TheLarkInn/aipm/pull/52)) (10cab01)

## [0.4.2] - 2026-03-23

### Features
- Add interactive init wizards with inquire ([#50](https://github.com/TheLarkInn/aipm/pull/50)) (e5b64de)

## [0.4.1] - 2026-03-22

### Documentation
- Add CI and Codecov badges to README ([#48](https://github.com/TheLarkInn/aipm/pull/48)) (df75b44)

## [0.4.0] - 2026-03-22

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
