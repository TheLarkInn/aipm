@p0 @registry
Feature: Package installation
  As a plugin consumer,
  I want to install packages from the registry,
  so that I can use published AI components in my project.

  Background:
    Given a plugin directory "my-project" with a valid manifest

  Scenario: Install a package by name
    Given the registry contains "code-review" at version "1.0.0"
    When the user runs "aipm install code-review"
    Then the package "code-review" is added to the manifest dependencies
    And the package files are stored in the global content-addressable store
    And a working copy is hard-linked into ".aipm/links/code-review"
    And a symlink is created in the plugins directory for Claude Code discovery
    And the lockfile is updated with the exact resolved version

  Scenario: Install a specific version
    Given the registry contains "code-review" at versions "1.0.0", "1.1.0", "2.0.0"
    When the user runs "aipm install code-review@1.1.0"
    Then the manifest dependency for "code-review" is set to "^1.1.0"
    And the lockfile pins "code-review" to exactly "1.1.0"

  Scenario: Install resolves the latest compatible version
    Given the registry contains "code-review" at versions "1.0.0", "1.1.0", "1.2.0", "2.0.0"
    When the user runs "aipm install code-review@^1.0"
    Then the lockfile pins "code-review" to "1.2.0"

  Scenario: Install a scoped package
    Given the registry contains "@myorg/custom-linter" at version "0.3.0"
    When the user runs "aipm install @myorg/custom-linter"
    Then the package "@myorg/custom-linter" is installed

  Scenario: Install from an alternative registry
    Given an alternative registry "internal" is configured
    And the registry "internal" contains "proprietary-skill" at version "1.0.0"
    When the user runs "aipm install proprietary-skill --registry internal"
    Then the package "proprietary-skill" is installed from registry "internal"

  Scenario: Install fails for nonexistent package
    Given the registry does not contain "nonexistent-plugin"
    When the user runs "aipm install nonexistent-plugin"
    Then the command fails with "package not found: nonexistent-plugin"

  Scenario: Install all dependencies from manifest
    Given the manifest declares the following dependencies:
      | name         | version |
      | code-review  | ^1.0.0  |
      | lint-skill   | ~0.2.0  |
    And the registry contains "code-review" at version "1.2.0"
    And the registry contains "lint-skill" at version "0.2.5"
    When the user runs "aipm install"
    Then both packages are downloaded
    And the lockfile contains exact versions for both

  Scenario: Deterministic install from lockfile
    Given a lockfile pinning "code-review" to "1.1.0"
    And the registry also contains "code-review" at version "1.2.0"
    When the user runs "aipm install --locked"
    Then "code-review" version "1.1.0" is installed
    And "code-review" version "1.2.0" is not considered

  Scenario: Install verifies package integrity
    Given a downloaded package archive with a corrupted checksum
    When the user runs "aipm install code-review"
    Then the command fails with "integrity check failed"
    And no files are written to the local store

  Scenario: Yanked versions are excluded from resolution
    Given the registry contains "code-review" at versions "1.0.0", "1.1.0"
    And version "1.1.0" of "code-review" is yanked
    When the user runs "aipm install code-review@^1.0"
    Then the lockfile pins "code-review" to "1.0.0"

  Rule: Content-addressable store (inspired by pnpm)

    Scenario: Packages are stored in a global content-addressable store
      Given a clean global store
      When the user runs "aipm install code-review@1.0.0"
      Then the package files are stored in the global content-addressable store
      And the project links to the store instead of copying files

    Scenario: Shared files across versions are stored once
      Given "common-util" version "1.0.0" has 100 files
      And "common-util" version "1.0.1" changes only 1 file
      When both versions are installed across different projects
      Then only 101 unique files are stored in the global store
      And the unchanged 99 files are shared between both versions

    Scenario: Multiple projects share the same store
      Given project "alpha" depends on "code-review" at "1.0.0"
      And project "beta" depends on "code-review" at "1.0.0"
      When both projects install their dependencies
      Then "code-review" files exist once in the global store
      And both projects link to the same stored files

    Scenario: Configure a custom store location
      Given the user sets the store path to "/custom/store"
      When the user runs "aipm install"
      Then packages are stored in "/custom/store"

  Rule: Strict dependency isolation (inspired by pnpm)

    Scenario: Only declared dependencies are accessible
      Given package "app" declares dependency "skill-a" at "^1.0"
      And "skill-a" depends on "internal-util" at "^1.0"
      When "app" is installed
      Then "app" can resolve "skill-a"
      But "app" cannot resolve "internal-util" directly

    Scenario: Phantom dependency access is prevented
      Given package "app" declares dependency "skill-a" at "^1.0"
      And "skill-a" depends on "lodash-clone" at "^4.0"
      And "app" does not declare "lodash-clone" as a dependency
      When "app" attempts to reference "lodash-clone"
      Then the reference fails because "lodash-clone" is not a declared dependency

  Rule: Side-effects cache (inspired by pnpm)

    Scenario: Lifecycle script results are cached
      Given package "native-tool" has a postinstall script that compiles a binary
      When the user installs "native-tool" for the first time
      Then the postinstall script runs
      And the compiled output is cached in the global store

    Scenario: Cached side-effects skip recompilation
      Given "native-tool" postinstall results are in the side-effects cache
      When the user installs "native-tool" in a new project
      Then the postinstall script does not run
      And the cached compiled output is linked directly

    Scenario: Lifecycle scripts from dependencies are blocked by default
      Given a package "sketchy-plugin" with a postinstall script
      When the user runs "aipm install sketchy-plugin"
      Then the postinstall script does not execute
      And a notice is displayed: "lifecycle script blocked for sketchy-plugin"

    Scenario: Explicitly allow lifecycle scripts for trusted packages
      Given a manifest with:
        """toml
        [install]
        allowed_build_scripts = ["native-tool", "node-sass"]
        """
      When the user installs "native-tool"
      Then the postinstall script runs
      And the results are cached
