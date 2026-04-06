@p2 @platform
Feature: Platform compatibility checking
  As a plugin consumer,
  I want to be warned when a plugin is incompatible with my platform,
  so that I don't install plugins that won't work on my OS.

  Scenario: Plugin with no platform restrictions installs on any OS
    Given a plugin without "environment.platforms" in aipm.toml
    When the plugin is installed
    Then the install succeeds with no platform warning

  Scenario: Plugin compatible with current platform
    Given a plugin declaring the current OS in environment.platforms
    When the plugin is installed
    Then the install succeeds

  Scenario: Plugin incompatible with current platform produces warning
    Given a plugin declaring only platforms that don't match the current OS
    When the plugin is installed
    Then a warning is emitted listing declared vs current platforms
    And the install still succeeds (non-blocking)
