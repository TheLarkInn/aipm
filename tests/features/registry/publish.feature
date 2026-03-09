@p0 @registry
Feature: Package publishing
  As a plugin author,
  I want to publish my package to a registry,
  so that others can discover and install it.

  Background:
    Given the user is authenticated with the registry
    And a plugin directory "my-plugin" with a valid manifest

  Scenario: Publish a new package version
    Given the package "my-plugin" version "0.1.0" does not exist in the registry
    When the user runs "aipm publish"
    Then the package is uploaded to the registry
    And the registry contains "my-plugin" at version "0.1.0"

  Scenario: Dry-run publish validates without uploading
    When the user runs "aipm publish --dry-run"
    Then the package is validated
    And the package is packed into an archive
    But the package is not uploaded to the registry

  Scenario: Reject publishing without authentication
    Given the user is not authenticated
    When the user runs "aipm publish"
    Then the command fails with "not authenticated"

  Scenario: Reject republishing an existing version
    Given the package "my-plugin" version "0.1.0" already exists in the registry
    When the user runs "aipm publish"
    Then the command fails with "version 0.1.0 already exists"

  Scenario: Published versions are immutable
    Given the package "my-plugin" version "1.0.0" exists in the registry
    When the user attempts to overwrite "my-plugin" version "1.0.0"
    Then the command fails with "published versions are immutable"

  Scenario: Publish validates all declared components exist
    Given the manifest declares a skill at "skills/review/SKILL.md"
    But the file "skills/review/SKILL.md" does not exist
    When the user runs "aipm publish"
    Then the command fails with "component not found"

  Scenario: Publish respects the files allowlist
    Given the manifest declares files to include:
      | path                 |
      | skills/              |
      | agents/              |
      | aipm.toml            |
    And a file "secrets.env" exists in the project
    When the user runs "aipm publish"
    Then the archive does not contain "secrets.env"
    And the archive contains "aipm.toml"

  Scenario: Scoped package publishing
    Given the manifest package name is "@myorg/my-plugin"
    When the user runs "aipm publish"
    Then the package is published under the "@myorg" scope
