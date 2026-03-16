@p1 @registry
Feature: Package discovery and search
  As a plugin consumer,
  I want to search for packages in the registry,
  so that I can discover reusable AI components.

  Scenario: Search by keyword
    Given the registry contains packages with various keywords
    When the user runs "aipm search code-review"
    Then packages matching "code-review" are listed
    And results include package name, version, and description

  Scenario: Search by component type
    Given the registry contains skills, agents, and MCP packages
    When the user runs "aipm search --type skill"
    Then only skill-type packages are listed

  Scenario: View package details
    Given the registry contains "code-review-skill" at version "1.2.0"
    When the user runs "aipm info code-review-skill"
    Then the package metadata is displayed
    And the component types are listed
    And the dependency tree is shown
    And the environment requirements are shown

  Scenario: List installed packages
    Given a project with installed dependencies
    When the user runs "aipm list"
    Then all direct and transitive dependencies are displayed in a tree format

  Scenario: List outdated packages
    Given a project with "code-review" at "1.0.0" installed
    And the registry has "code-review" at "1.2.0"
    When the user runs "aipm outdated"
    Then "code-review" is listed with current "1.0.0" and latest "1.2.0"
