@p1 @global
Feature: Global plugin installation with engine scoping
  As a plugin consumer,
  I want to install plugins globally so they are available across all projects,
  and optionally restrict them to specific engines.

  Scenario: Install plugin globally for all engines
    When the user runs "aipm install --global local:./my-plugin"
    Then "~/.aipm/installed.json" contains an entry with empty engines (all)

  Scenario: Install plugin globally for a specific engine
    When the user runs "aipm install --global --engine claude local:./my-plugin"
    Then the installed entry has engines: ["claude"]

  Scenario: Add second engine to existing global plugin
    Given "my-plugin" is installed globally for Claude
    When the user runs "aipm install --global --engine copilot local:./my-plugin"
    Then the engines become ["claude", "copilot"]

  Scenario: Install with empty engines resets to all
    Given "my-plugin" is installed globally for Claude only
    When the user runs "aipm install --global local:./my-plugin"
    Then the engines are reset to [] (all engines)

  Scenario: Uninstall global plugin completely
    Given "my-plugin" is installed globally
    When the user runs "aipm uninstall --global local:./my-plugin"
    Then the entry is removed from "installed.json"

  Scenario: Uninstall specific engine from global plugin
    Given "my-plugin" is installed for Claude and Copilot
    When the user runs "aipm uninstall --global --engine claude local:./my-plugin"
    Then only Copilot remains in the engines list

  Scenario: Uninstall last engine removes plugin entirely
    Given "my-plugin" is installed for Claude only
    When the user runs "aipm uninstall --global --engine claude local:./my-plugin"
    Then the plugin is fully removed from "installed.json"

  Scenario: List globally installed plugins
    Given multiple plugins are installed globally
    When the user runs "aipm list --global"
    Then all globally installed plugins are listed with their engines

  Scenario: Name conflict between global plugins on overlapping engines
    Given "my-plugin" is installed globally from GitHub for all engines
    When the user runs "aipm install --global local:./my-plugin" (same name, different source)
    Then the install fails with a name conflict error

  Scenario: Name conflict allowed on non-overlapping engines
    Given "my-plugin" from GitHub is installed for Claude only
    When the user runs "aipm install --global --engine copilot local:./my-plugin"
    Then the install succeeds (no engine overlap)

  Scenario: Resolve ambiguous plugin by engine filter
    Given "my-plugin" from GitHub installed for Claude
    And "my-plugin" from local installed for Copilot
    When the user runs "aipm uninstall --global --engine claude my-plugin"
    Then the GitHub source is resolved and uninstalled for Claude

  Scenario: Global plugin with custom cache policy
    When the user runs "aipm install --global --plugin-cache no-refresh local:./my-plugin"
    Then the installed entry has cache_policy "no-refresh"
