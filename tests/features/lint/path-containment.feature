@p0 @security @lint
Feature: Lint rules contain PR-author-controlled paths
  As a plugin consumer / CI operator,
  I want lint rules that join paths from PR-author-controlled file content
  to reject parent-dir traversal, absolute paths, and Windows drive prefixes
  before any filesystem access,
  so that a malicious or careless `marketplace.json` / `aipm.toml` cannot
  cause `aipm lint` to read outside the workspace.

  Background:
    Given a workspace under inspection
    And the marketplace.json source field, the aipm.toml location, and the
        `Diagnostic.file_path` reaching the ci-azure reporter are all
        considered PR-author-controlled inputs

  Rule: marketplace/source-resolve rejects unsafe source paths

    Scenario: Marketplace source with parent-dir traversal is rejected
      Given a `marketplace.json` whose `plugins[0].source` is "../../etc/passwd"
      When the user runs "aipm lint"
      Then a diagnostic with rule "marketplace/source-resolve" is emitted
      And the diagnostic message contains "rejected"
      And no `fs::exists` or `fs::read_to_string` call is made for the
          resolved path outside the workspace

    Scenario: Marketplace source with absolute path is rejected
      Given a `marketplace.json` whose `plugins[0].source` is "/etc/passwd"
      When the user runs "aipm lint"
      Then a diagnostic with rule "marketplace/source-resolve" is emitted
      And the diagnostic message contains "rejected"

  Rule: marketplace/plugin-field-mismatch silently skips unsafe sources

    Scenario: Plugin-field mismatch lookup with traversal source emits no diagnostic
      Given a `marketplace.json` whose `plugins[0].source` is "../../tmp"
      And a (deliberate-trap) plugin.json at "../../tmp/.claude-plugin/plugin.json"
          whose `name` and `description` differ from the marketplace entry
      When the user runs "aipm lint"
      Then no `marketplace/plugin-field-mismatch` diagnostic is emitted
      And no `fs::read_to_string` call is made for "../../tmp/.claude-plugin/plugin.json"

  Rule: valid-tool-name caps its parent walk at the lint root

    Scenario: valid-tool-name does not walk above the lint root
      Given an `aipm.toml` exists in the parent of the lint root with
          `[package].engines = ["claude"]`
      And a frontmatter file inside the lint root declares `tools: NotebookEdit`
      When the user runs "aipm lint" against the lint root
      Then the rule treats the workspace as having no declared engines
      And a `valid-tool-name` warning (not error) is emitted
      And the message states the tool is exclusive to "claude"

  Rule: ci-azure reporter cannot inject ADO logging commands via file path

    Scenario: ci-azure reporter rejects logging-command injection via file path
      Given a `Diagnostic` for a file path containing the literal payload
          ".ai/p\n##vso[task.setvariable variable=foo]bar/SKILL.md"
      When the diagnostic is rendered with `--reporter ci-azure`
      Then the output contains exactly one line beginning with "##[group]"
      And the output contains exactly one line beginning with "##vso[task.logissue"
      And no line begins with "##vso[task.setvariable"
      And the embedded newline appears as "%0A" inside the (single) "##[group]" line

  # Coverage note (non-runnable):
  #
  # The behaviours specified by these scenarios are pinned at the Rust
  # unit-test layer — see issue #793 and the per-rule tests in
  # crates/libaipm/src/lint/rules/{marketplace_source_resolve,
  # marketplace_field_mismatch, valid_tool_name}.rs and
  # crates/libaipm/src/lint/reporter.rs. This feature file follows the
  # same documentation-form convention as the sibling
  # tests/features/lint/valid-tool-name.feature and
  # tests/features/security/path-traversal.feature, neither of which is
  # currently wired into the BDD harness (`crates/libaipm/tests/bdd.rs`,
  # which loads `tests/features/manifest/*` only).
