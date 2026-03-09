@p1 @reuse
Feature: Compositional reuse of plugin internals
  As a plugin author,
  I want to define hooks, skills, MCP definitions, and agents once
  and reuse them across multiple plugins without copy/paste,
  so that I can compose plugins from shared building blocks.

  Rule: Skills can be published and consumed as dependencies

    Scenario: Publish a standalone skill as a package
      Given a package "code-review-skill" containing only a SKILL.md
      And the manifest declares the component type as "skill"
      When the user runs "aipm publish"
      Then the package is published to the registry as a reusable skill

    Scenario: Depend on a published skill in another plugin
      Given a plugin "my-linter" with manifest:
        """toml
        [dependencies]
        code-review-skill = "^1.0"
        """
      When the user runs "aipm install"
      Then the skill from "code-review-skill" is available in "my-linter"

    Scenario: Installed skill is discoverable by the host agent
      Given a plugin with an installed skill dependency "code-review-skill"
      When the host agent loads the plugin
      Then the skill "code-review-skill" appears in the available skills list

  Rule: Agents can be shared across plugins

    Scenario: Publish a standalone agent definition
      Given a package "reviewer-agent" containing an agent markdown file
      And the manifest declares the component type as "agent"
      When the user runs "aipm publish"
      Then the package is published as a reusable agent

    Scenario: Compose a plugin from multiple agent dependencies
      Given a plugin "ci-suite" depending on:
        | dependency       | type  |
        | reviewer-agent   | agent |
        | test-runner-agent| agent |
      When the user runs "aipm install"
      Then both agents are available in "ci-suite"

  Rule: MCP server definitions can be shared

    Scenario: Publish an MCP server configuration as a package
      Given a package "sqlite-mcp" containing an MCP server definition
      And the manifest declares the component type as "mcp"
      When the user runs "aipm publish"
      Then the package is published as a reusable MCP server definition

    Scenario: Depend on a shared MCP server
      Given a plugin "data-analyzer" with dependency "sqlite-mcp" at "^1.0"
      When the user runs "aipm install"
      Then the MCP server configuration from "sqlite-mcp" is merged into the plugin

  Rule: Hook definitions can be shared

    Scenario: Publish a hook configuration as a package
      Given a package "security-hooks" containing hook definitions
      And the manifest declares the component type as "hook"
      When the user runs "aipm publish"
      Then the package is published as a reusable hook set

    Scenario: Multiple plugins can depend on the same hook package
      Given plugin "plugin-a" depends on "security-hooks" at "^1.0"
      And plugin "plugin-b" depends on "security-hooks" at "^1.0"
      When both plugins are installed
      Then "security-hooks" is resolved once and shared

  Rule: Composite packages bundle multiple component types

    Scenario: Create a composite package with skills, agents, and hooks
      Given a package "full-ci-toolkit" with manifest:
        """toml
        [package]
        name = "full-ci-toolkit"
        version = "1.0.0"
        type = "composite"

        [components]
        skills = ["skills/lint/SKILL.md", "skills/format/SKILL.md"]
        agents = ["agents/ci-runner.md"]
        hooks = ["hooks/pre-push.json"]
        """
      When the user runs "aipm publish"
      Then the composite package is published with all components

    Scenario: Consumer selectively uses components from a composite package
      Given a dependency on "full-ci-toolkit" with features ["lint-skill"]
      When the user runs "aipm install"
      Then only the lint skill component is activated
