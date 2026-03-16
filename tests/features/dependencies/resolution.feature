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

  Rule: Version coexistence (Cargo model — no peer dependencies)

    AIPM does not have peer dependencies. Instead, the resolver uses
    aggressive version unification within the same semver-major. Across
    semver-major boundaries, multiple versions coexist in the graph.

    Scenario: Same-major dependencies are unified to one version
      Given "plugin-a" depends on "common-util" at "^2.0"
      And "plugin-b" depends on "common-util" at "^2.5"
      And the registry contains "common-util" at versions "2.0.0", "2.5.0", "2.8.0"
      When dependencies are resolved
      Then "common-util" appears exactly once at version "2.8.0"
      And both "plugin-a" and "plugin-b" use the same version

    Scenario: Cross-major dependencies coexist in the graph
      Given "plugin-a" depends on "framework" at "^1.0"
      And "plugin-b" depends on "framework" at "^2.0"
      When dependencies are resolved
      Then both "framework" 1.x and "framework" 2.x are in the resolution
      And "plugin-a" links to "framework" 1.x
      And "plugin-b" links to "framework" 2.x

    Scenario: Coexisting major versions are stored independently
      Given the resolution contains "framework" at "1.5.0" and "2.1.0"
      When packages are installed
      Then both versions exist in the content-addressable store
      And each consumer's symlink tree contains only their declared major version
