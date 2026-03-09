@p0 @registry
Feature: Package yanking and deprecation
  As a package maintainer,
  I want to yank or deprecate a version,
  so that I can discourage use of broken versions without breaking existing users.

  Background:
    Given the user is authenticated with the registry
    And the user owns the package "my-plugin"

  Scenario: Yank a published version
    Given "my-plugin" version "1.0.0" is published
    When the user runs "aipm yank my-plugin@1.0.0"
    Then version "1.0.0" is marked as yanked
    And the archive remains available for existing lockfiles

  Scenario: Yanked version is still usable by existing lockfiles
    Given "my-plugin" version "1.0.0" is yanked
    And a project has a lockfile pinning "my-plugin" to "1.0.0"
    When the project runs "aipm install --locked"
    Then "my-plugin" version "1.0.0" is installed successfully

  Scenario: Yanked version is excluded from new resolutions
    Given "my-plugin" version "1.0.0" is yanked
    And "my-plugin" version "1.0.1" is available
    When a new project runs "aipm install my-plugin@^1.0"
    Then "my-plugin" version "1.0.1" is resolved

  Scenario: Un-yank a version
    Given "my-plugin" version "1.0.0" is yanked
    When the user runs "aipm yank --undo my-plugin@1.0.0"
    Then version "1.0.0" is available for new resolutions again

  Scenario: Deprecate a package with a message
    When the user runs "aipm deprecate my-plugin --message 'Use new-plugin instead'"
    Then the package is marked as deprecated
    And installing "my-plugin" shows a deprecation warning with "Use new-plugin instead"
