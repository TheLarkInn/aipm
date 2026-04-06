@p1 @source
Feature: Git source plugin acquisition
  As a plugin consumer,
  I want to install plugins directly from git repositories,
  so that I can use plugins that are not published to a registry.

  Background:
    Given a plugin directory "my-project" with a valid manifest

  Scenario: Install from a git URL via CLI spec
    Given a git repository at "https://github.com/org/repo" containing a plugin at "plugins/hello-world"
    When the user runs "aipm install git:https://github.com/org/repo:plugins/hello-world@main"
    Then the repository is cloned with depth 1
    And the "plugins/hello-world" subdirectory is extracted
    And the plugin is linked in the plugins directory
    And the lockfile contains a "git+" source entry

  Scenario: Install from GitHub shorthand
    Given a git repository at "https://github.com/owner/repo" containing a plugin at "plugins/my-tool"
    When the user runs "aipm install github:owner/repo:plugins/my-tool@main"
    Then the repository "https://github.com/owner/repo" is cloned
    And the plugin "my-tool" is linked

  Scenario: Install from git URL without subdirectory
    Given a git repository that is itself a plugin
    When the user runs "aipm install github:owner/my-plugin@v2.0"
    Then the entire repository is used as the plugin
    And the plugin folder name is "my-plugin"

  Scenario: Git clone uses system credential helper
    Given a private git repository requiring authentication
    When the user runs "aipm install git:https://github.com/org/private-repo:plugin@main"
    Then git clone delegates authentication to the system credential helper
    And aipm does not store or manage credentials

  Scenario: Second install uses download cache (Auto policy)
    Given a plugin was previously installed from "git:https://github.com/org/repo:plugin@main"
    When the user runs "aipm install" again
    Then the plugin is served from the download cache at "~/.aipm/cache/"
    And no git clone is performed

  Scenario: Install with --plugin-cache skip bypasses cache
    Given a cached plugin exists
    When the user runs "aipm install --plugin-cache skip git:https://github.com/org/repo:plugin@main"
    Then a fresh git clone is performed regardless of cache state

  Scenario: Git clone failure produces clear error
    When the user runs "aipm install git:https://nonexistent-host.invalid/repo"
    Then the exit code is non-zero
    And the error message mentions "Git clone failed"

  Scenario: Plugin path not found in repository
    Given a git repository without the specified subdirectory
    When the user runs "aipm install github:org/repo:nonexistent/path@main"
    Then the error message mentions "does not exist in repository"

  Scenario: Path traversal in git spec is rejected
    When the user runs "aipm install git:https://github.com/org/repo:../../../etc/passwd@main"
    Then the exit code is non-zero
    And the error message mentions "Path traversal"

  Scenario: Install from manifest git dependency
    Given an "aipm.toml" with dependency: my-plugin = { git = "https://github.com/org/repo", ref = "main" }
    When the user runs "aipm install"
    Then the git source dependency is acquired via shallow clone
    And the lockfile records the git source
