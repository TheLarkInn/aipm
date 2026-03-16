@p0 @dependencies @lockfile
Feature: Lockfile management
  As a plugin developer,
  I want a lockfile that captures exact dependency versions,
  so that builds are deterministic and reproducible.

  AIPM follows the Cargo model for lockfile behavior: `aipm install` never
  upgrades existing pins. Only `aipm update` explicitly pulls newer versions.
  `aipm install --locked` is CI mode that fails on any drift.

  Rule: Lockfile creation and basic usage

    Scenario: Lockfile is created on first install
      Given a manifest with dependencies but no lockfile
      When the user runs "aipm install"
      Then a file "aipm.lock" is created
      And the lockfile contains exact versions for all resolved dependencies
      And the lockfile contains integrity checksums for each package

    Scenario: Lockfile is respected on subsequent installs
      Given a lockfile pinning "code-review" to "1.1.0"
      And the registry now has "code-review" at "1.2.0"
      When the user runs "aipm install"
      Then "code-review" version "1.1.0" is installed
      And "code-review" version "1.2.0" is never considered

    Scenario: Lockfile records the dependency tree structure
      Given package "app" depends on "skill-a" which depends on "common-util"
      When dependencies are resolved and locked
      Then the lockfile records that "common-util" is a transitive dependency of "skill-a"

    Scenario: Lockfile is deterministic across platforms
      Given the same manifest and registry state
      When dependencies are resolved on different platforms
      Then the generated lockfiles are byte-for-byte identical

  Rule: Minimal reconciliation on manifest change (Cargo model)

    Scenario: Adding a new dependency only resolves the new entry
      Given a lockfile pinning "code-review" to "1.1.0"
      And the registry has "code-review" at "1.2.0"
      And the user adds "lint-skill" at "^1.0" to the manifest
      When the user runs "aipm install"
      Then "lint-skill" is resolved and added to the lockfile
      And "code-review" remains pinned at "1.1.0" in the lockfile
      And "code-review" is NOT upgraded to "1.2.0"

    Scenario: Removing a dependency removes it from the lockfile
      Given a lockfile pinning "code-review" to "1.1.0" and "lint-skill" to "0.2.0"
      And the user removes "lint-skill" from the manifest
      When the user runs "aipm install"
      Then "lint-skill" is removed from the lockfile
      And "code-review" remains pinned at "1.1.0"

    Scenario: Changing a version range only re-resolves the changed entry
      Given a lockfile pinning "code-review" to "1.1.0" and "lint-skill" to "0.2.0"
      And the user changes "code-review" to "^2.0" in the manifest
      When the user runs "aipm install"
      Then "code-review" is re-resolved to the latest 2.x version
      And "lint-skill" remains pinned at "0.2.0"

  Rule: Locked install for CI (zero-drift mode)

    Scenario: Locked install aborts on lockfile-manifest mismatch
      Given a lockfile that does not include dependency "new-dep"
      And the manifest now declares "new-dep" as a dependency
      When the user runs "aipm install --locked"
      Then the command fails with "lockfile is out of date"

    Scenario: Locked install succeeds when lockfile matches manifest
      Given a lockfile that matches the manifest exactly
      When the user runs "aipm install --locked"
      Then all packages are installed from the lockfile without any resolution
      And no network requests are made to resolve versions

  Rule: Explicit update pulls latest versions

    Scenario: Update a specific dependency in the lockfile
      Given a lockfile pinning "code-review" to "1.1.0" and "lint-skill" to "0.2.0"
      And the registry has "code-review" at "1.2.0"
      When the user runs "aipm update code-review"
      Then the lockfile updates "code-review" to "1.2.0"
      And the lockfile keeps "lint-skill" at "0.2.0"

    Scenario: Update all dependencies in the lockfile
      Given a lockfile with outdated versions
      When the user runs "aipm update"
      Then all dependencies are re-resolved to the latest compatible versions
      And the lockfile is regenerated

    Scenario: Update respects manifest version ranges
      Given a lockfile pinning "code-review" to "1.1.0"
      And the manifest declares "code-review" at "^1.0"
      And the registry has "code-review" at "1.5.0" and "2.0.0"
      When the user runs "aipm update code-review"
      Then the lockfile updates "code-review" to "1.5.0"
      And "2.0.0" is not considered because it's outside "^1.0"
