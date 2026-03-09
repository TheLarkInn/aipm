@p0 @registry @local
Feature: Local and registry plugin coexistence
  As a developer in a repo with local plugins,
  I want to install registry plugins alongside my local plugins,
  and I want my local plugins to consume registry dependencies,
  so that I can compose from both local and shared sources.

  Repos today have local plugin directories (e.g. claude-plugins/) that
  Claude Code discovers automatically. AIPM must integrate with this
  existing pattern: registry-installed plugins get symlinked into the
  same directory so Claude Code finds them without any special config.

  Rule: Registry-installed plugins are symlinked into the plugins directory

    Scenario: Installing a registry plugin creates a symlink in the plugins directory
      Given a workspace root with manifest:
        """toml
        [workspace]
        members = ["claude-plugins/*"]
        plugins_dir = "claude-plugins"
        """
      When the user runs "aipm install @company/code-review"
      Then the package is downloaded to the global content-addressable store
      And a working copy is assembled in ".aipm/links/@company/code-review"
      And a symlink is created at "claude-plugins/@company/code-review"
      And the symlink target is ".aipm/links/@company/code-review"

    Scenario: Symlinked plugins are gitignored automatically
      Given a workspace with plugins_dir "claude-plugins"
      When the user runs "aipm install code-review-skill"
      Then the entry "code-review-skill" is added to "claude-plugins/.gitignore"
      And the symlink "claude-plugins/code-review-skill" is not tracked by git

    Scenario: Local plugins remain git-tracked alongside symlinked installs
      Given a workspace with plugins_dir "claude-plugins"
      And a local plugin at "claude-plugins/my-local-tool" checked into git
      When the user runs "aipm install @company/code-review"
      Then "claude-plugins/my-local-tool" remains a real directory tracked by git
      And "claude-plugins/@company/code-review" is a symlink not tracked by git

    Scenario: Uninstalling removes the symlink and gitignore entry
      Given "code-review-skill" is installed with a symlink in "claude-plugins/"
      When the user runs "aipm uninstall code-review-skill"
      Then the symlink "claude-plugins/code-review-skill" is removed
      And the entry is removed from "claude-plugins/.gitignore"
      And the dependency is removed from the root manifest

    Scenario: Claude Code discovers both local and symlinked plugins
      Given a local plugin at "claude-plugins/my-local-tool"
      And a symlinked registry plugin at "claude-plugins/code-review-skill"
      When Claude Code scans the "claude-plugins" directory
      Then both "my-local-tool" and "code-review-skill" are discovered
      And both have their skills, agents, and hooks loaded

  Rule: Local plugins can declare registry dependencies

    Scenario: Local plugin with registry dependencies
      Given a local plugin at "claude-plugins/my-ci-tools/aipm.toml" with:
        """toml
        [package]
        name = "my-ci-tools"
        version = "0.1.0"

        [dependencies]
        shared-lint-skill = "^1.0"
        """
      When the user runs "aipm install" from the workspace root
      Then "shared-lint-skill" is downloaded from the registry
      And "shared-lint-skill" is available to "my-ci-tools" at runtime

    Scenario: Local plugin depends on another local plugin
      Given local plugins "core-hooks" and "ci-suite" in the workspace
      And "ci-suite" manifest declares:
        """toml
        [dependencies]
        core-hooks = { workspace = "^" }
        """
      When the user runs "aipm install"
      Then "ci-suite" can reference components from "core-hooks"

    Scenario: Local plugin depends on both local and registry packages
      Given local plugin "my-ci-tools" with dependencies:
        | name              | source    |
        | core-hooks        | workspace |
        | shared-lint-skill | registry  |
      When the user runs "aipm install"
      Then both dependencies are resolved
      And the single workspace lockfile records both

    Scenario: Transitive registry deps of local plugins are resolved
      Given local plugin "my-ci-tools" depends on "shared-lint-skill" at "^1.0"
      And "shared-lint-skill" depends on "string-utils" at "^2.0"
      When the user runs "aipm install"
      Then both "shared-lint-skill" and "string-utils" are in the lockfile
      And both are stored in the global content-addressable store

  Rule: Non-workspace mode (simple repos)

    Scenario: Install registry plugins without a workspace
      Given a project with a root manifest without a [workspace] section:
        """toml
        [package]
        name = "my-project"
        version = "0.1.0"

        [dependencies]
        code-review-skill = "^1.0"
        """
      When the user runs "aipm install"
      Then "code-review-skill" is installed to the default plugins directory
      And a symlink is created for Claude Code discovery

    Scenario: Default plugins directory when no workspace is configured
      Given a project without a [workspace] section
      When the user runs "aipm install code-review-skill"
      Then the symlink is created at "claude-plugins/code-review-skill"
      And "claude-plugins/.gitignore" is created or updated

    Scenario: Custom plugins directory in non-workspace mode
      Given a manifest with:
        """toml
        [package]
        name = "my-project"
        plugins_dir = ".claude/plugins"
        """
      When the user runs "aipm install code-review-skill"
      Then the symlink is created at ".claude/plugins/code-review-skill"

  Rule: Vendored (forked) plugins from registry

    Scenario: Vendor a registry plugin into the repo for modification
      Given "code-review-skill" at version "1.0.0" is available in the registry
      When the user runs "aipm vendor code-review-skill"
      Then the package contents are copied to "claude-plugins/code-review-skill"
      And an "aipm.toml" is created with origin metadata:
        """toml
        [package]
        name = "code-review-skill"
        version = "1.0.0"

        [package.origin]
        registry = "default"
        version = "1.0.0"
        """
      And the directory is a real directory tracked by git (not a symlink)

    Scenario: Vendored plugin becomes a workspace member
      Given a vendored plugin at "claude-plugins/code-review-skill"
      And the workspace members pattern includes "claude-plugins/*"
      When the user runs "aipm install"
      Then "code-review-skill" is treated as a local workspace member
      And its registry dependencies are resolved normally

    Scenario: Detect when a vendored plugin is outdated
      Given a vendored plugin with origin version "1.0.0"
      And the registry has "code-review-skill" at version "1.2.0"
      When the user runs "aipm outdated"
      Then "code-review-skill" is listed as vendored with a newer version available

  Rule: Gitignore management

    Scenario: First install creates the plugins gitignore
      Given no "claude-plugins/.gitignore" exists
      When the user runs "aipm install code-review-skill"
      Then "claude-plugins/.gitignore" is created with:
        """
        # Managed by aipm - do not edit this section
        code-review-skill
        """

    Scenario: Subsequent installs append to the gitignore
      Given "claude-plugins/.gitignore" already contains "code-review-skill"
      When the user runs "aipm install lint-tool"
      Then "claude-plugins/.gitignore" now contains both entries

    Scenario: Scoped packages are properly gitignored
      When the user runs "aipm install @company/review-plugin"
      Then "claude-plugins/.gitignore" contains "@company/review-plugin"
      And the scope directory "claude-plugins/@company/" is also gitignored

    Scenario: Manual gitignore entries are preserved
      Given "claude-plugins/.gitignore" contains manual entries like "*.tmp"
      When the user runs "aipm install code-review-skill"
      Then the manual entries are preserved
      And the aipm-managed section is appended separately
