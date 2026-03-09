@p0 @dependencies @lockfile
Feature: Lockfile management
  As a plugin developer,
  I want a lockfile that captures exact dependency versions,
  so that builds are deterministic and reproducible.

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

  Scenario: Locked install aborts on lockfile-manifest mismatch
    Given a lockfile that does not include dependency "new-dep"
    And the manifest now declares "new-dep" as a dependency
    When the user runs "aipm install --locked"
    Then the command fails with "lockfile is out of date"

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

  Scenario: Lockfile records the dependency tree structure
    Given package "app" depends on "skill-a" which depends on "common-util"
    When dependencies are resolved and locked
    Then the lockfile records that "common-util" is a transitive dependency of "skill-a"

  Scenario: Lockfile is deterministic across platforms
    Given the same manifest and registry state
    When dependencies are resolved on different platforms
    Then the generated lockfiles are byte-for-byte identical
