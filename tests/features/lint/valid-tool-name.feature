@p2 @lint
Feature: valid-tool-name lint flags engine-incompatible tool references
  As a plugin author,
  I want `aipm lint` to warn me when an agent / skill / hook references a
  tool that none of the plugin's declared engines support,
  so that I catch cross-engine tool typos before they hit the marketplace.

  Background:
    Given the engine API schema is the canonical source for engine-tool compatibility
    And the schema declares "bash" as a shared tool, "Task" as claude-only, and "browser_navigate" as copilot-cli-only

  Rule: Plugins without `[engines]` get warnings on engine-exclusive tools

    Scenario: Undeclared engines + claude-only tool emits a Warning
      Given a plugin with no `[engines]` block in `aipm.toml`
      And an agent that lists tool "Task" in its frontmatter
      When the user runs "aipm lint"
      Then a warning is reported with rule id "valid-tool-name"
      And the message states the tool is exclusive to "claude"
      And the message suggests adding `engines = ["claude"]` to `aipm.toml`

    Scenario: Undeclared engines + copilot-only tool emits a Warning
      Given a plugin with no `[engines]` block in `aipm.toml`
      And a skill that lists tool "browser_navigate" in its frontmatter
      When the user runs "aipm lint"
      Then a warning is reported with rule id "valid-tool-name"
      And the message states the tool is exclusive to "copilot-cli"

  Rule: Plugins with `[engines]` are validated against the declared set

    Scenario: declared engines covering tool support emits no diagnostic
      Given a plugin with `engines = ["claude"]` in `aipm.toml`
      And an agent that lists tool "Task" in its frontmatter
      When the user runs "aipm lint"
      Then no `valid-tool-name` diagnostic is reported

    Scenario: declared engines NOT covering tool support emits an Error
      Given a plugin with `engines = ["claude"]` in `aipm.toml`
      And a hook that lists tool "browser_navigate" in its tools field
      When the user runs "aipm lint"
      Then an error is reported with rule id "valid-tool-name"
      And the message states the tool is not supported by the declared engines

    Scenario: shared tools are always clean regardless of declared engines
      Given a plugin with no `[engines]` block in `aipm.toml`
      And an agent that lists tool "bash" in its frontmatter
      When the user runs "aipm lint"
      Then no `valid-tool-name` diagnostic is reported
