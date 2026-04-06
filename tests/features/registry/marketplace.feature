@p1 @source
Feature: Marketplace plugin acquisition
  As a plugin consumer,
  I want to install plugins from marketplace repositories,
  so that I can discover and use curated plugin collections.

  Background:
    Given a plugin directory "my-project" with a valid manifest

  Scenario: Install from marketplace via CLI
    Given a marketplace repository with a "marketplace.toml" listing available plugins
    And the marketplace is configured in "~/.aipm/config.toml" as "test"
    When the user runs "aipm install market:hello-skills@test"
    Then the marketplace manifest is fetched and parsed
    And the "hello-skills" plugin is acquired from its declared source
    And the plugin is linked in the project

  Scenario: Marketplace plugin not found produces descriptive error
    Given a marketplace with plugins "alpha", "beta"
    When the user runs "aipm install market:nonexistent@test"
    Then the error message lists available plugins: "alpha", "beta"

  Scenario: Marketplace with git source object
    Given a marketplace entry with source type "git" pointing to an external repo
    When the user installs the plugin via marketplace
    Then the plugin is fetched from the external git repository

  Scenario: Marketplace with relative path source
    Given a marketplace entry with source "plugins/my-plugin" (relative path)
    When the user installs the plugin
    Then the plugin is extracted from within the marketplace repository

  Scenario: Marketplace with plugin_root metadata
    Given a marketplace with "[metadata] plugin_root = './plugins'"
    And a plugin entry with source "formatter" (relative)
    When the user installs the plugin
    Then the source path is resolved as "plugins/formatter"

  Scenario: Marketplace pinned to a specific ref
    Given a marketplace spec "market:hello@owner/repo#v2.0"
    When the user installs the plugin
    Then the marketplace repository is cloned at ref "v2.0"

  Scenario: Marketplace from local path (for testing)
    Given a local marketplace directory at "./test-fixtures/marketplace"
    When the user runs "aipm install market:my-plugin@./test-fixtures/marketplace"
    Then the manifest is read from the local directory
    And the plugin is acquired from the local marketplace

  Scenario: Marketplace with unsupported source type
    Given a marketplace entry with source type "npm"
    When the user tries to install the plugin
    Then the error message mentions "unsupported" and "npm"
