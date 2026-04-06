@p1 @source
Feature: Mixed registry and source dependencies
  As a plugin consumer,
  I want to use both registry packages and git/local source plugins in one project,
  so that I have maximum flexibility in dependency management.

  Scenario: Project with both registry and git source dependencies
    Given an "aipm.toml" with:
      | dep-a = "^1.0"                                                              |
      | dep-b = { git = "https://github.com/org/repo", path = "plugins/b", ref = "main" } |
    When the user runs "aipm install"
    Then dep-a is resolved from the registry
    And dep-b is acquired via git clone
    And both are linked in the plugins directory
    And the lockfile has correct source types for each

  Scenario: Project with local path dependency
    Given an "aipm.toml" with:
      | my-local = { path = "../my-plugin" } |
    When the user runs "aipm install"
    Then the local plugin is copied to the plugins directory
    And the lockfile records it as a "path+" source

  Scenario: Update with mixed sources
    Given a project with both registry and git source dependencies
    When the user runs "aipm update"
    Then registry deps are re-resolved
    And git source deps are re-fetched
    And the lockfile is updated for both

  Scenario: Local source plugin also passes engine validation
    Given a local plugin with aipm.toml declaring engines = ["claude"]
    When installed targeting Claude engine
    Then the install succeeds
