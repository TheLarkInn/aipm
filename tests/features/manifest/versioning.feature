@p0 @manifest @versioning
Feature: Semantic versioning
  As a plugin ecosystem participant,
  I want all packages to follow semantic versioning,
  so that dependency ranges are meaningful and safe.

  Scenario Outline: Valid semver versions are accepted
    Given a manifest with version "<version>"
    When the manifest is validated
    Then the version is accepted

    Examples:
      | version           |
      | 0.1.0             |
      | 1.0.0             |
      | 1.2.3             |
      | 0.0.1             |
      | 1.0.0-alpha.1     |
      | 2.0.0-beta.3+build|

  Scenario Outline: Invalid versions are rejected
    Given a manifest with version "<version>"
    When the manifest is validated
    Then the version is rejected with "invalid semver"

    Examples:
      | version   |
      | 1         |
      | 1.0       |
      | v1.0.0    |
      | latest    |
      | 1.0.0.0   |

  Scenario Outline: Version requirement ranges are parsed correctly
    Given a dependency with version requirement "<requirement>"
    When the requirement is parsed
    Then it matches version "<matches>"
    And it does not match version "<no_match>"

    Examples:
      | requirement | matches | no_match |
      | ^1.2.3      | 1.3.0   | 2.0.0    |
      | ~1.2.3      | 1.2.5   | 1.3.0    |
      | =1.2.3      | 1.2.3   | 1.2.4    |
      | >=1.0,<2.0  | 1.5.0   | 2.0.0    |
      | *            | 3.0.0   |          |

  Scenario: Pre-release versions are excluded from caret ranges by default
    Given a dependency with version requirement "^1.0.0"
    And the registry contains versions "1.0.0", "1.1.0", "2.0.0-alpha.1"
    When dependencies are resolved
    Then version "1.1.0" is selected
    And version "2.0.0-alpha.1" is not considered

  Scenario: Pre-1.0 caret ranges treat minor as breaking
    Given a dependency with version requirement "^0.2.3"
    And the registry contains versions "0.2.3", "0.2.5", "0.3.0"
    When dependencies are resolved
    Then version "0.2.5" is selected
    And version "0.3.0" is not considered
