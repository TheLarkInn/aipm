@p0 @manifest
Feature: Plugin scaffolding
  As a plugin author,
  I want to scaffold new plugins in a marketplace,
  so that I have a valid project structure to start building.

  Scenario: Create a new plugin in an initialized marketplace
    Given an initialized marketplace in "my-project"
    When the user runs "aipm make plugin --name my-plugin --engine claude --feature skill -y" in "my-project"
    Then the command succeeds
    And a plugin directory "my-plugin" exists in the marketplace in "my-project"
    And the plugin.json in "my-plugin" in "my-project" contains "my-plugin"

  Scenario: Create a plugin with --name flag
    Given an initialized marketplace in "my-project"
    When the user runs "aipm make plugin --name hello-world --engine claude --feature skill -y" in "my-project"
    Then the command succeeds
    And the plugin.json in "hello-world" in "my-project" contains "hello-world"

  Scenario: Creating a plugin in existing directory succeeds idempotently
    Given an initialized marketplace in "my-project"
    When the user runs "aipm make plugin --name my-plugin --engine claude --feature skill -y" in "my-project"
    Then the command succeeds
    When the user runs "aipm make plugin --name my-plugin --engine claude --feature skill -y" in "my-project"
    Then the command succeeds

  Scenario: Plugin scaffold includes feature directories
    Given an initialized marketplace in "my-project"
    When the user runs "aipm make plugin --name my-plugin --engine claude --feature skill -y" in "my-project"
    Then the command succeeds
    And the plugin "my-plugin" has a "skills" directory in "my-project"

  Scenario Outline: Plugin scaffold with specific features
    Given an initialized marketplace in "my-project"
    When the user runs "aipm make plugin --name my-plugin --engine claude --feature <feature> -y" in "my-project"
    Then the command succeeds
    And the plugin "my-plugin" has a "<expected_dir>" directory in "my-project"

    Examples:
      | feature | expected_dir |
      | skill   | skills       |
      | agent   | agents       |
      | hook    | hooks        |

  Scenario: Plugin name must follow naming conventions
    Given an initialized marketplace in "my-project"
    When the user runs "aipm make plugin --name INVALID_Name! --engine claude --feature skill -y" in "my-project"
    Then the command fails with "invalid"
