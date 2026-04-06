@p2 @source
Feature: Source redirect for marketplace stubs
  As a plugin consumer,
  I want marketplace stubs to transparently redirect to external repos,
  so that I don't need to know the real source location.

  Scenario: Plugin with source redirect is re-acquired from target
    Given a marketplace plugin that is a stub with [package.source] in aipm.toml
    And the source redirect points to "https://github.com/org/repo.git" at "plugins/real-plugin"
    When the plugin is installed
    Then the stub is deleted
    And the real plugin is fetched from the redirect URL
    And the final plugin directory contains the real code

  Scenario: Double redirect is rejected
    Given a plugin whose redirect target also contains [package.source]
    When the plugin is installed
    Then the install fails with a "redirect loop" error
