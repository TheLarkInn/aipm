@p0 @manifest
Feature: Manifest validation
  As a plugin author,
  I want the package manager to validate my manifest,
  so that I catch configuration errors before publishing.

  Background:
    Given a plugin directory "test-plugin" with a valid manifest

  Scenario: Valid manifest passes validation
    When the user runs "aipm validate"
    Then the command succeeds
    And no warnings are emitted

  Scenario: Missing required name field fails validation
    Given the manifest is missing the "name" field
    When the user runs "aipm validate"
    Then the command fails with "missing required field: name"

  Scenario: Missing required version field fails validation
    Given the manifest is missing the "version" field
    When the user runs "aipm validate"
    Then the command fails with "missing required field: version"

  Scenario: Invalid semver version fails validation
    Given the manifest version is "not-a-version"
    When the user runs "aipm validate"
    Then the command fails with "invalid semver version"

  Scenario: Valid manifest with dependencies passes validation
    Given the manifest declares the following dependencies:
      | name         | version |
      | code-review  | ^1.0.0  |
      | lint-skill   | ~0.2.3  |
    When the user runs "aipm validate"
    Then the command succeeds

  Scenario: Dependency with invalid version range fails validation
    Given the manifest declares a dependency "broken" with version "???invalid"
    When the user runs "aipm validate"
    Then the command fails with "invalid version requirement for dependency: broken"

  Scenario: Manifest declares plugin components
    Given the manifest declares the following components:
      """toml
      [components]
      skills = ["skills/code-review/SKILL.md", "skills/lint/SKILL.md"]
      agents = ["agents/reviewer.md"]
      hooks = ["hooks/pre-commit.json"]
      mcp_servers = ["mcp/sqlite.json"]
      """
    When the user runs "aipm validate"
    Then the command succeeds
    And all declared component paths are verified to exist

  Scenario: Declared component path that does not exist fails validation
    Given the manifest declares a skill at "skills/nonexistent/SKILL.md"
    And the file "skills/nonexistent/SKILL.md" does not exist
    When the user runs "aipm validate"
    Then the command fails with "component not found: skills/nonexistent/SKILL.md"
