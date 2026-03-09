@p0 @dependencies
Feature: Dependency resolution
  As a plugin consumer,
  I want dependencies to be resolved correctly,
  so that all required components are available without conflicts.

  Scenario: Resolve a simple dependency tree
    Given package "app" depends on "skill-a" at "^1.0"
    And "skill-a" depends on "common-util" at "^2.0"
    And the registry contains "skill-a" at "1.2.0" and "common-util" at "2.1.0"
    When dependencies are resolved for "app"
    Then the resolution contains "skill-a" at "1.2.0"
    And the resolution contains "common-util" at "2.1.0"

  Scenario: Unify shared dependencies to a single version
    Given package "app" depends on "skill-a" at "^1.0" and "skill-b" at "^1.0"
    And "skill-a" depends on "common-util" at "^2.0"
    And "skill-b" depends on "common-util" at "^2.1"
    And the registry contains "common-util" at versions "2.0.0", "2.1.0", "2.2.0"
    When dependencies are resolved for "app"
    Then "common-util" appears exactly once in the resolution
    And "common-util" is resolved to "2.2.0"

  Scenario: Allow multiple incompatible major versions
    Given package "app" depends on "skill-a" at "^1.0" and "skill-b" at "^1.0"
    And "skill-a" depends on "common-util" at "^1.0"
    And "skill-b" depends on "common-util" at "^2.0"
    When dependencies are resolved for "app"
    Then both "common-util" version "1.x" and "2.x" are in the resolution

  Scenario: Detect and report circular dependencies
    Given package "a" depends on "b" at "^1.0"
    And package "b" depends on "a" at "^1.0"
    When dependencies are resolved for "a"
    Then the resolution fails with "circular dependency detected"

  Scenario: Prefer the highest compatible version
    Given a dependency on "skill-a" at "^1.0"
    And the registry contains "skill-a" at versions "1.0.0", "1.1.0", "1.2.0", "2.0.0"
    When dependencies are resolved
    Then "skill-a" is resolved to "1.2.0"

  Scenario: Backtrack on conflict
    Given package "app" depends on "skill-a" at "^1.0" and "skill-b" at "^1.0"
    And "skill-a" at "1.2.0" depends on "common-util" at "=2.0.0"
    And "skill-b" depends on "common-util" at "=2.1.0"
    And "skill-a" at "1.1.0" depends on "common-util" at "^2.0.0"
    When dependencies are resolved for "app"
    Then "skill-a" is resolved to "1.1.0"
    And "common-util" is resolved to "2.1.0"

  Scenario: Report unsolvable conflicts clearly
    Given package "app" depends on "skill-a" at "^1.0" and "skill-b" at "^1.0"
    And "skill-a" depends on "common-util" at "=1.0.0"
    And "skill-b" depends on "common-util" at "=2.0.0"
    When dependencies are resolved for "app"
    Then the resolution fails with a conflict report
    And the conflict report names "common-util" as the conflicting package
    And the conflict report shows both incompatible requirements

  Rule: Dependency overrides (inspired by pnpm overrides)

    Scenario: Override a transitive dependency version globally
      Given a manifest with:
        """toml
        [overrides]
        "vulnerable-lib" = "^2.0.0"
        """
      And "skill-a" depends on "vulnerable-lib" at "^1.0"
      When dependencies are resolved
      Then "vulnerable-lib" is resolved to "2.x" regardless of what "skill-a" declared

    Scenario: Override a dependency only when it is a child of a specific package
      Given a manifest with:
        """toml
        [overrides]
        "skill-a>common-util" = "=2.1.0"
        """
      And "skill-a" depends on "common-util" at "^2.0"
      And "skill-b" depends on "common-util" at "^2.0"
      When dependencies are resolved
      Then "common-util" under "skill-a" is pinned to "2.1.0"
      But "common-util" under "skill-b" resolves normally

    Scenario: Replace a dependency with a fork via override
      Given a manifest with:
        """toml
        [overrides]
        "broken-lib" = "aipm:fixed-lib@^1.0.0"
        """
      When dependencies are resolved
      Then every occurrence of "broken-lib" is replaced with "fixed-lib"

  Rule: Peer dependency handling (inspired by pnpm strictness)

    Scenario: Missing peer dependencies are auto-installed
      Given package "plugin-a" declares a peer dependency on "framework" at "^2.0"
      And "framework" is not in the project's dependencies
      When the user runs "aipm install plugin-a"
      Then "framework" is automatically installed
      And a notice is displayed: "auto-installed peer dependency: framework@2.x"

    Scenario: Conflicting peer dependencies produce a warning
      Given "plugin-a" declares peer dependency "framework" at "^2.0"
      And "plugin-b" declares peer dependency "framework" at "^3.0"
      When both are installed
      Then a warning is displayed about the conflicting peer requirements

    Scenario: Strict peer mode fails on missing peers
      Given the setting "strict_peer_dependencies" is enabled
      And package "plugin-a" declares a peer dependency on "framework" at "^2.0"
      And "framework" is not in the project's dependencies
      When the user runs "aipm install plugin-a"
      Then the command fails with "missing required peer dependency: framework"
