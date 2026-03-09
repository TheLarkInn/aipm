@p0 @manifest
Feature: Package initialization
  As a plugin author,
  I want to initialize a new AI plugin package,
  so that I have a valid project structure to start building.

  Scenario: Initialize a new plugin in an empty directory
    Given an empty directory "my-plugin"
    When the user runs "aipm init" in "my-plugin"
    Then a file "aipm.toml" is created in "my-plugin"
    And the manifest contains the directory name "my-plugin" as the package name
    And the manifest contains a version of "0.1.0"
    And the manifest contains an edition field

  Scenario: Initialize a new plugin with a custom name
    Given an empty directory "workspace"
    When the user runs "aipm init --name hello-world" in "workspace"
    Then the manifest contains the package name "hello-world"

  Scenario: Reject initialization in a directory with an existing manifest
    Given a directory "existing" containing an "aipm.toml"
    When the user runs "aipm init" in "existing"
    Then the command fails with "already initialized"

  Scenario: Initialize creates a standard directory layout
    Given an empty directory "my-plugin"
    When the user runs "aipm init" in "my-plugin"
    Then the following directories exist in "my-plugin":
      | directory |
      | skills/   |
      | agents/   |
      | hooks/    |
    And a file "skills/.gitkeep" exists in "my-plugin"

  Scenario Outline: Initialize with a specific plugin type
    Given an empty directory "my-plugin"
    When the user runs "aipm init --type <type>" in "my-plugin"
    Then the manifest contains the plugin type "<type>"
    And a starter template for "<type>" is created

    Examples:
      | type      |
      | skill     |
      | agent     |
      | mcp       |
      | hook      |
      | composite |

  Scenario: Package name must follow naming conventions
    Given an empty directory "workspace"
    When the user runs "aipm init --name INVALID_Name!" in "workspace"
    Then the command fails with "invalid package name"
    And the error message explains the naming rules
