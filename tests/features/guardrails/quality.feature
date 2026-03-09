@p1 @guardrails
Feature: AI-assisted plugin quality guardrails
  As a plugin ecosystem maintainer,
  I want the CLI to help AI agents create quality plugins,
  so that Claude/Copilot produce well-structured, validated plugins by default.

  Rule: Plugin scaffolding enforces best practices

    Scenario: AI-generated plugin is validated on creation
      Given an AI agent runs "aipm init --type skill"
      When the scaffolding is generated
      Then the generated SKILL.md contains required frontmatter fields
      And the generated SKILL.md has a description under 1024 characters
      And the generated directory follows the standard layout

    Scenario: AI agent receives structured error guidance
      Given an AI agent attempts to create a plugin with an invalid manifest
      When the validation fails
      Then the error output includes a machine-readable error code
      And the error output includes a human-readable fix suggestion
      And the error output includes a link to documentation

  Rule: Lint command catches common quality issues

    Scenario: Lint detects missing required SKILL.md frontmatter
      Given a skill package with a SKILL.md missing the "description" field
      When the user runs "aipm lint"
      Then a warning is reported: "SKILL.md missing required field: description"

    Scenario: Lint detects oversized SKILL.md
      Given a skill package with a SKILL.md exceeding 5000 tokens
      When the user runs "aipm lint"
      Then a warning is reported: "SKILL.md exceeds recommended 5000 token limit"

    Scenario: Lint detects agent without tools declaration
      Given an agent package with an agent markdown missing the "tools" frontmatter
      When the user runs "aipm lint"
      Then a warning is reported: "agent definition missing tools declaration"

    Scenario: Lint validates hook event names
      Given a hook package with a hook referencing event "InvalidEvent"
      When the user runs "aipm lint"
      Then an error is reported: "unknown hook event: InvalidEvent"
      And the error lists valid hook events

    Scenario: Lint passes for a well-formed plugin
      Given a plugin following all quality conventions
      When the user runs "aipm lint"
      Then the command succeeds with "no issues found"

  Rule: Publish gate enforces minimum quality

    @serial
    Scenario: Publish rejects packages that fail lint
      Given a package with lint errors
      When the user runs "aipm publish"
      Then the command fails with "package has quality issues"
      And the lint errors are displayed
      And a hint suggests running "aipm lint --fix"

    Scenario: Lint auto-fix corrects common issues
      Given a SKILL.md with a name field exceeding 64 characters
      When the user runs "aipm lint --fix"
      Then the name is truncated to 64 characters
      And the user is informed of the change

  Rule: Quality score for discoverability

    Scenario: Package receives a quality score on publish
      Given a well-documented package with tests and examples
      When the package is published
      Then a quality score is computed
      And the score is stored in the registry metadata

    Scenario Outline: Quality score criteria
      Given a package with <attribute>
      When the quality score is computed
      Then the score includes points for "<criterion>"

      Examples:
        | attribute                    | criterion         |
        | a description in the manifest| has description   |
        | a LICENSE file               | has license       |
        | a README.md                  | has readme        |
        | example configurations       | has examples      |
        | declared environment deps    | has env deps      |
