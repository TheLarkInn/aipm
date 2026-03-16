@p1 @dependencies @features
Feature: Optional features and conditional components
  As a plugin author,
  I want to declare optional features for my package,
  so that consumers can opt into additional capabilities without bloat.

  Scenario: Declare default features
    Given a manifest with features:
      """toml
      [features]
      default = ["json-output"]
      json-output = []
      xml-output = []
      """
    When the package is installed by a consumer
    Then the "json-output" feature is enabled
    And the "xml-output" feature is not enabled

  Scenario: Opt out of default features
    Given a dependency declaration:
      """toml
      [dependencies]
      my-plugin = { version = "^1.0", default-features = false }
      """
    When the dependency is resolved
    Then no default features are enabled for "my-plugin"

  Scenario: Enable a specific optional feature
    Given a dependency declaration:
      """toml
      [dependencies]
      my-plugin = { version = "^1.0", features = ["xml-output"] }
      """
    When the dependency is resolved
    Then the "xml-output" feature is enabled for "my-plugin"

  Scenario: Features are additive across the dependency graph
    Given package "a" depends on "common" with features ["json"]
    And package "b" depends on "common" with features ["xml"]
    And package "app" depends on both "a" and "b"
    When dependencies are resolved for "app"
    Then "common" is built with both "json" and "xml" features enabled

  Scenario: Optional dependency activated by feature
    Given a manifest with:
      """toml
      [dependencies]
      heavy-analyzer = { version = "^1.0", optional = true }

      [features]
      deep-analysis = ["dep:heavy-analyzer"]
      """
    When the package is installed without the "deep-analysis" feature
    Then "heavy-analyzer" is not downloaded

  Scenario: Feature enables an optional component
    Given a manifest with:
      """toml
      [features]
      advanced-review = []

      [components]
      skills = ["skills/basic-review/SKILL.md"]

      [components.feature.advanced-review]
      skills = ["skills/advanced-review/SKILL.md"]
      """
    When the package is installed with feature "advanced-review"
    Then the skill "skills/advanced-review/SKILL.md" is included
