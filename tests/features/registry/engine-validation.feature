@p1 @engine
Feature: Engine validation post-install
  As a plugin consumer,
  I want plugins to be validated for engine compatibility,
  so that incompatible plugins are flagged before runtime.

  Scenario: Plugin with matching engine installs successfully
    Given a plugin with aipm.toml declaring engines = ["claude"]
    When the plugin is installed targeting Claude engine
    Then the install succeeds without warnings

  Scenario: Plugin with non-matching engine produces warning
    Given a plugin with aipm.toml declaring engines = ["copilot"]
    When the plugin is installed targeting Claude engine
    Then a warning is emitted about engine incompatibility

  Scenario: Plugin with engines omitted is universal
    Given a plugin with aipm.toml without the engines field
    When the plugin is installed targeting any engine
    Then the install succeeds (universal compatibility)

  Scenario: Plugin without aipm.toml falls back to marker files
    Given a plugin with ".claude-plugin/plugin.json" but no aipm.toml
    When the plugin is installed targeting Claude engine
    Then the install succeeds via marker file fallback

  Scenario: Plugin without aipm.toml and no markers fails
    Given a plugin with neither aipm.toml nor engine marker files
    When the plugin is installed
    Then the install fails with a descriptive validation error
    And the error mentions the expected marker files
