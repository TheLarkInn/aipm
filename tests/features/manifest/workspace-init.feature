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
      | .ai/starter-aipm-plugin/                     |
      | .ai/starter-aipm-plugin/skills/              |
      | .ai/starter-aipm-plugin/scripts/             |
      | .ai/starter-aipm-plugin/agents/              |
      | .ai/starter-aipm-plugin/hooks/               |
      | .ai/starter-aipm-plugin/.claude-plugin/      |
    And a file ".ai/.gitignore" exists in "my-project"

  Scenario: Marketplace generates a valid starter plugin manifest with --manifest
    Given an empty directory "my-project"
    When the user runs "aipm init --workspace --marketplace --manifest" in "my-project"
    Then a file ".ai/starter-aipm-plugin/aipm.toml" exists in "my-project"
    And the starter plugin manifest contains the package name "starter-aipm-plugin"
    And the starter plugin manifest contains a version of "0.1.0"
    And the starter plugin manifest contains the plugin type "composite"

  Scenario: Default marketplace does not generate starter aipm.toml
    Given an empty directory "my-project"
    When the user runs "aipm init --marketplace" in "my-project"
    Then a file ".ai/starter-aipm-plugin/skills/scaffold-plugin/SKILL.md" exists in "my-project"
    And there is no file ".ai/starter-aipm-plugin/aipm.toml" in "my-project"

  Scenario: Marketplace generates a Claude Code plugin structure
    Given an empty directory "my-project"
    When the user runs "aipm init --marketplace" in "my-project"
    Then a file ".ai/starter-aipm-plugin/.claude-plugin/plugin.json" exists in "my-project"
    And a file ".ai/starter-aipm-plugin/skills/scaffold-plugin/SKILL.md" exists in "my-project"
    And a file ".ai/starter-aipm-plugin/scripts/scaffold-plugin.sh" exists in "my-project"
    And a file ".ai/starter-aipm-plugin/agents/marketplace-scanner.md" exists in "my-project"
    And a file ".ai/starter-aipm-plugin/hooks/hooks.json" exists in "my-project"
    And a file ".ai/starter-aipm-plugin/.mcp.json" exists in "my-project"

  Scenario: Marketplace generates a starter skill with description frontmatter
    Given an empty directory "my-project"
    When the user runs "aipm init --marketplace" in "my-project"
    Then a file ".ai/starter-aipm-plugin/skills/scaffold-plugin/SKILL.md" exists in "my-project"
    And the starter skill contains "description:" in the frontmatter

  Scenario: Starter plugin includes a marketplace scanner agent
    Given an empty directory "my-project"
    When the user runs "aipm init --marketplace" in "my-project"
    Then a file ".ai/starter-aipm-plugin/agents/marketplace-scanner.md" exists in "my-project"

  Scenario: Starter plugin includes a logging hook
    Given an empty directory "my-project"
    When the user runs "aipm init --marketplace" in "my-project"
    Then a file ".ai/starter-aipm-plugin/hooks/hooks.json" exists in "my-project"

  Scenario: Starter plugin includes a scaffold script
    Given an empty directory "my-project"
    When the user runs "aipm init --marketplace" in "my-project"
    Then a file ".ai/starter-aipm-plugin/scripts/scaffold-plugin.sh" exists in "my-project"

  Scenario: No-starter flag creates bare marketplace directory
    Given an empty directory "my-project"
    When the user runs "aipm init --no-starter" in "my-project"
    Then a file ".ai/.gitignore" exists in "my-project"
    And there is no directory ".ai/starter-aipm-plugin" in "my-project"

  Scenario: No-starter flag still configures tool settings
    Given an empty directory "my-project"
    When the user runs "aipm init --no-starter --engine claude" in "my-project"
    Then a file ".claude/settings.json" exists in "my-project"
    And there is no directory ".ai/starter-aipm-plugin" in "my-project"

  Scenario: No-starter flag with workspace creates both minus starter
    Given an empty directory "my-project"
    When the user runs "aipm init --workspace --marketplace --no-starter" in "my-project"
    Then a file "aipm.toml" is created in "my-project"
    And a file ".ai/.gitignore" exists in "my-project"
    And there is no directory ".ai/starter-aipm-plugin" in "my-project"

  Scenario: Generated gitignore has aipm managed markers
    Given an empty directory "my-project"
    When the user runs "aipm init --marketplace" in "my-project"
    Then a file ".ai/.gitignore" exists in "my-project"
    And the gitignore contains "aipm managed start"
    And the gitignore contains "aipm managed end"

  Scenario: Generated gitignore includes tool-usage.log when starter plugin is included
    Given an empty directory "my-project"
    When the user runs "aipm init --marketplace" in "my-project"
    Then a file ".ai/.gitignore" exists in "my-project"
    And the gitignore contains ".tool-usage.log"

  Scenario: Generated gitignore omits tool-usage.log when no-starter flag is passed
    Given an empty directory "my-project"
    When the user runs "aipm init --no-starter" in "my-project"
    Then a file ".ai/.gitignore" exists in "my-project"
    And the gitignore does not contain ".tool-usage.log"

  Scenario: Idempotent workspace initialization when aipm.toml exists (#850)
    Given a directory "existing" containing an "aipm.toml"
    When the user runs "aipm init --workspace" in "existing"
    Then the command succeeds
    And stdout contains "Using existing aipm.toml"

  Scenario: Marketplace without workspace generates only marketplace directory
    Given an empty directory "my-project"
    When the user runs "aipm init --marketplace" in "my-project"
    Then the following directories exist in "my-project":
      | directory              |
      | .ai/                   |
      | .ai/starter-aipm-plugin/           |
      | .ai/starter-aipm-plugin/skills/    |
    And there is no file "aipm.toml" in "my-project"

  Scenario: Idempotent marketplace initialization when .ai/ already exists (#850)
    Given an empty directory "my-project"
    And a directory "my-project/.ai" exists
    When the user runs "aipm init --marketplace" in "my-project"
    Then the command succeeds
    And stdout contains "Using existing .ai/ marketplace"

  Scenario: Workspace and marketplace flags compose independently
    Given an empty directory "my-project"
    When the user runs "aipm init --workspace" in "my-project"
    Then a file "aipm.toml" is created in "my-project"
    And there is no directory ".ai/starter-aipm-plugin" in "my-project"

  Scenario: Default init with no flags creates marketplace only
    Given an empty directory "my-project"
    When the user runs "aipm init" in "my-project"
    Then the following directories exist in "my-project":
      | directory      |
      | .ai/           |
      | .ai/starter-aipm-plugin/   |
    And there is no file "aipm.toml" in "my-project"

  Scenario: Starter plugin manifest is valid TOML that round-trips through parser
    Given an empty directory "my-project"
    When the user runs "aipm init --marketplace --manifest" in "my-project"
    Then a file ".ai/starter-aipm-plugin/aipm.toml" exists in "my-project"
    And the starter plugin manifest is valid according to aipm schema

  Rule: Marketplace manifest generation

    Scenario: Marketplace.json is generated with correct structure
      Given an empty directory "my-project"
      When the user runs "aipm init --marketplace --engine claude" in "my-project"
      Then a file ".ai/.claude-plugin/marketplace.json" exists in "my-project"
      And the marketplace.json name is "local-repo-plugins"
      And the marketplace.json contains a plugin named "starter-aipm-plugin"
      And the marketplace.json plugin "starter-aipm-plugin" has source "./starter-aipm-plugin"

    Scenario: Marketplace.json with --no-starter has empty plugins array
      Given an empty directory "my-project"
      When the user runs "aipm init --no-starter --engine claude" in "my-project"
      Then a file ".ai/.claude-plugin/marketplace.json" exists in "my-project"
      And the marketplace.json name is "local-repo-plugins"
      And the marketplace.json plugins array is empty

  Rule: Tool settings integration

    Scenario: Claude Code settings point to .ai/ as local marketplace
      Given an empty directory "my-project"
      When the user runs "aipm init --marketplace --engine claude" in "my-project"
      Then a file ".claude/settings.json" exists in "my-project"
      And the Claude settings contain "extraKnownMarketplaces"
      And the Claude settings marketplace path is "./.ai"

    Scenario: Claude Code settings have enabledPlugins at top level
      Given an empty directory "my-project"
      When the user runs "aipm init --marketplace --engine claude" in "my-project"
      Then a file ".claude/settings.json" exists in "my-project"
      And the Claude settings contain "enabledPlugins" at the top level
      And the Claude settings enable "starter-aipm-plugin@local-repo-plugins"

    Scenario: Existing Claude settings are not overwritten
      Given an empty directory "my-project"
      And a file ".claude/settings.json" with custom content exists in "my-project"
      When the user runs "aipm init --marketplace" in "my-project"
      Then the Claude settings file preserves the custom content

  # Spec G9 part 3 / Feature 20: Engine-aware init scaffolds only chosen engines.
  # See `crates/aipm/tests/init_engine_e2e.rs` for additional Rust-level coverage.
  Rule: Engine-aware init scaffolds only chosen engines

    Scenario: Copilot-only init does not create .claude/ (issue #724 fix)
      Given an empty directory "my-project"
      When the user runs "aipm init --engine copilot" in "my-project"
      Then a file ".github/copilot-instructions.md" exists in "my-project"
      And there is no directory ".claude" in "my-project"

    Scenario: Multi-engine init creates both engine roots
      Given an empty directory "my-project"
      When the user runs "aipm init --engine claude,copilot" in "my-project"
      Then a file ".claude/settings.json" exists in "my-project"
      And a file ".github/copilot-instructions.md" exists in "my-project"

    Scenario: --yes default scaffolds Copilot only (Spec G5)
      Given an empty directory "my-project"
      When the user runs "aipm init --yes" in "my-project"
      Then a file ".github/copilot-instructions.md" exists in "my-project"
      And there is no directory ".claude" in "my-project"

    Scenario: Default (all engines supported) omits the engines field on disk
      Given an empty directory "my-project"
      When the user runs "aipm init --yes --workspace --engine claude" in "my-project"
      Then a file "aipm.toml" is created in "my-project"
      And the file "aipm.toml" in "my-project" does not contain "engines ="

    Scenario: Claude-only scaffold does not create the Copilot instructions file
      Given an empty directory "my-project"
      When the user runs "aipm init --engine claude" in "my-project"
      Then a file ".claude/settings.json" exists in "my-project"
      And there is no file ".github/copilot-instructions.md" in "my-project"

    # Note: scenario "Narrowed support set writes workspace engines field"
    # from spec G9 part 3 requires scripted wizard prompt input, which the
    # CLI doesn't expose. That assertion is covered by the library unit
    # test `init_workspace_with_narrow_support_writes_engines_field` in
    # `crates/libaipm/src/workspace_init/mod.rs::tests` (Feature 9).
