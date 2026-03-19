@p0 @manifest @workspace
Feature: Workspace initialization
  As a plugin consumer,
  I want to initialize my repository for AI plugin management,
  so that I can install registry packages and develop local plugins.

  Scenario: Initialize a workspace in an empty directory
    Given an empty directory "my-project"
    When the user runs "aipm init --workspace" in "my-project"
    Then a file "aipm.toml" is created in "my-project"
    And the manifest contains a "[workspace]" section
    And the manifest contains members ".ai/*"
    And the manifest contains plugins_dir ".ai"

  Scenario: Initialize a workspace with default marketplace
    Given an empty directory "my-project"
    When the user runs "aipm init --workspace --marketplace" in "my-project"
    Then a file "aipm.toml" is created in "my-project"
    And the following directories exist in "my-project":
      | directory                        |
      | .ai/                             |
      | .ai/starter/                     |
      | .ai/starter/skills/              |
      | .ai/starter/agents/              |
      | .ai/starter/hooks/               |
      | .ai/starter/.claude-plugin/      |
    And a file ".ai/.gitignore" exists in "my-project"

  Scenario: Marketplace generates a valid starter plugin manifest
    Given an empty directory "my-project"
    When the user runs "aipm init --workspace --marketplace" in "my-project"
    Then a file ".ai/starter/aipm.toml" exists in "my-project"
    And the starter plugin manifest contains the package name "starter"
    And the starter plugin manifest contains a version of "0.1.0"
    And the starter plugin manifest contains the plugin type "composite"

  Scenario: Marketplace generates a Claude Code plugin structure
    Given an empty directory "my-project"
    When the user runs "aipm init --marketplace" in "my-project"
    Then a file ".ai/starter/.claude-plugin/plugin.json" exists in "my-project"
    And a file ".ai/starter/skills/hello/SKILL.md" exists in "my-project"
    And a file ".ai/starter/.mcp.json" exists in "my-project"

  Scenario: Marketplace generates a starter skill with description frontmatter
    Given an empty directory "my-project"
    When the user runs "aipm init --marketplace" in "my-project"
    Then a file ".ai/starter/skills/hello/SKILL.md" exists in "my-project"
    And the starter skill contains "description:" in the frontmatter

  Scenario: Generated gitignore has aipm managed markers
    Given an empty directory "my-project"
    When the user runs "aipm init --marketplace" in "my-project"
    Then a file ".ai/.gitignore" exists in "my-project"
    And the gitignore contains "aipm managed start"
    And the gitignore contains "aipm managed end"

  Scenario: Reject workspace initialization if aipm.toml already exists
    Given a directory "existing" containing an "aipm.toml"
    When the user runs "aipm init --workspace" in "existing"
    Then the command fails with "already initialized"

  Scenario: Marketplace without workspace generates only marketplace directory
    Given an empty directory "my-project"
    When the user runs "aipm init --marketplace" in "my-project"
    Then the following directories exist in "my-project":
      | directory              |
      | .ai/                   |
      | .ai/starter/           |
      | .ai/starter/skills/    |
    And there is no file "aipm.toml" in "my-project"

  Scenario: Marketplace skips if .ai directory already exists
    Given an empty directory "my-project"
    And a directory "my-project/.ai" exists
    When the user runs "aipm init --marketplace" in "my-project"
    Then the command fails with "already exists"

  Scenario: Workspace and marketplace flags compose independently
    Given an empty directory "my-project"
    When the user runs "aipm init --workspace" in "my-project"
    Then a file "aipm.toml" is created in "my-project"
    And there is no directory ".ai/starter" in "my-project"

  Scenario: Default init with no flags creates marketplace only
    Given an empty directory "my-project"
    When the user runs "aipm init" in "my-project"
    Then the following directories exist in "my-project":
      | directory      |
      | .ai/           |
      | .ai/starter/   |
    And there is no file "aipm.toml" in "my-project"

  Scenario: Starter plugin manifest is valid TOML that round-trips through parser
    Given an empty directory "my-project"
    When the user runs "aipm init --marketplace" in "my-project"
    Then a file ".ai/starter/aipm.toml" exists in "my-project"
    And the starter plugin manifest is valid according to aipm schema

  Rule: Tool settings integration

    Scenario: Claude Code settings point to .ai/ as local marketplace
      Given an empty directory "my-project"
      When the user runs "aipm init --marketplace" in "my-project"
      Then a file ".claude/settings.json" exists in "my-project"
      And the Claude settings contain "extraKnownMarketplaces"
      And the Claude settings reference ".ai" as the marketplace path

    Scenario: Existing Claude settings are not overwritten
      Given an empty directory "my-project"
      And a file ".claude/settings.json" with custom content exists in "my-project"
      When the user runs "aipm init --marketplace" in "my-project"
      Then the Claude settings file preserves the custom content
