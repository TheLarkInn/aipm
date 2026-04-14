@p1 @monorepo
Feature: Monorepo orchestrator integration
  As a developer in a monorepo,
  I want the AI plugin manager to integrate with build orchestrators,
  so that installs, build hooks, and validation fit naturally into my existing workflow.

  Rule: Workspace support for multi-package repos

    Scenario: Define a workspace with local plugins directory
      Given a workspace root with manifest:
        """toml
        [workspace]
        members = ["claude-plugins/*"]
        plugins_dir = "claude-plugins"
        """
      And the following local plugin directories:
        | path                         |
        | claude-plugins/linter        |
        | claude-plugins/formatter     |
        | claude-plugins/test-runner   |
      When the user runs "aipm install" from the workspace root
      Then dependencies for all local plugins are resolved together
      And a single lockfile is created at the workspace root
      And registry-installed plugins are linked into "claude-plugins/"

    Scenario: Workspace members share a single lockfile
      Given a workspace with members "plugin-a" and "plugin-b"
      And both depend on "common-util" at "^1.0"
      When dependencies are resolved
      Then both members use the same version of "common-util"
      And only one lockfile exists at the workspace root

    Scenario: Workspace dependency inheritance
      Given a workspace root manifest with shared dependencies:
        """toml
        [workspace.dependencies]
        common-skill = { version = "^2.0" }
        """
      And a member manifest with:
        """toml
        [dependencies]
        common-skill = { workspace = true }
        """
      When the member's dependencies are resolved
      Then "common-skill" uses the version from the workspace root

    Scenario: Run a command across all workspace members
      Given a workspace with 3 plugin members
      When the user runs "aipm lint --workspace"
      Then lint is executed for each workspace member
      And results are reported per-member

    Scenario: Build only affected packages
      Given a workspace with members "plugin-a" and "plugin-b"
      And only files in "plugin-a" have changed since the last build
      When the user runs "aipm build --workspace --affected"
      Then only "plugin-a" is built

  Rule: Integration with external orchestrators

    Scenario: Generate Rush-compatible configuration
      Given a workspace using Rush as the monorepo orchestrator
      When the user runs "aipm orchestrator init --type rush"
      Then a Rush-compatible project configuration is generated
      And aipm install hooks are registered in the Rush command config

    Scenario: Generate Turborepo-compatible configuration
      Given a workspace using Turborepo as the orchestrator
      When the user runs "aipm orchestrator init --type turborepo"
      Then a turbo.json pipeline entry for aipm tasks is generated

    Scenario: Expose lifecycle hooks for orchestrator integration
      Given an aipm workspace
      When the orchestrator invokes "aipm run preinstall"
      Then the preinstall hooks from all workspace members execute
      And the exit code reflects success or failure

    Scenario Outline: Orchestrator-specific install integration
      Given a monorepo using "<orchestrator>"
      When the orchestrator runs its install step
      Then aipm dependencies are resolved alongside other package managers
      And no conflicting lockfiles are created

      Examples:
        | orchestrator |
        | Rush         |
        | Turborepo    |
        | BuildXL      |
        | MSBuild      |

  Rule: Workspace validation

    Scenario: Validate workspace member consistency
      Given a workspace where member "plugin-a" declares "common" at "^1.0"
      And member "plugin-b" declares "common" at "^2.0"
      When the user runs "aipm validate --workspace"
      Then a warning is reported about inconsistent dependency ranges for "common"

    Scenario: Prevent workspace member from publishing with workspace-only deps
      Given a workspace member referencing a workspace dependency
      When the user runs "aipm publish" from the member directory
      Then the command fails unless all workspace references are resolved to real versions

  Rule: Workspace protocol for inter-package references

    Scenario: Reference a workspace sibling with workspace protocol
      Given a workspace with members "core" and "cli"
      And "cli" manifest declares:
        """toml
        [dependencies]
        core = { workspace = "*" }
        """
      When the user runs "aipm install" from the workspace root
      Then "cli" links to the local "core" package
      And no registry lookup is performed for "core"

    @wip
    Scenario: Workspace protocol is replaced on publish
      Given workspace member "cli" depends on "core" with workspace protocol "*"
      And "core" is at version "2.3.0"
      When the user runs "aipm publish" from the "cli" directory
      Then the published manifest replaces workspace reference with "2.3.0"

    Scenario: Workspace protocol with caret is rejected
      Given a workspace member manifest with:
        """toml
        [dependencies]
        core = { workspace = "^" }
        """
      When the manifest is validated
      Then validation fails with "invalid workspace protocol"

    Scenario: Workspace protocol with equals is rejected
      Given a workspace member manifest with:
        """toml
        [dependencies]
        core = { workspace = "=" }
        """
      When the manifest is validated
      Then validation fails with "invalid workspace protocol"

  Rule: Catalogs for shared version ranges (inspired by pnpm)

    Scenario: Define a catalog of shared dependency versions
      Given a workspace root manifest with:
        """toml
        [catalog]
        common-skill = "^2.0.0"
        lint-skill = "^1.5.0"
        """
      And a member manifest with:
        """toml
        [dependencies]
        common-skill = "catalog:"
        lint-skill = "catalog:"
        """
      When the member's dependencies are resolved
      Then "common-skill" resolves using the catalog range "^2.0.0"
      And "lint-skill" resolves using the catalog range "^1.5.0"

    Scenario: Named catalogs for different version tracks
      Given a workspace root manifest with:
        """toml
        [catalogs.stable]
        framework = "^1.0.0"

        [catalogs.next]
        framework = "^2.0.0-beta"
        """
      And member "app-stable" depends on framework with "catalog:stable"
      And member "app-next" depends on framework with "catalog:next"
      When dependencies are resolved
      Then "app-stable" gets "framework" at "^1.0.0"
      And "app-next" gets "framework" at "^2.0.0-beta"

    Scenario: Catalog references are replaced on publish
      Given a member depending on "common-skill" via "catalog:"
      And the catalog defines "common-skill" as "^2.0.0"
      When the user runs "aipm publish" from the member directory
      Then the published manifest contains "common-skill" at "^2.0.0"

    Scenario: Catalog enforces version consistency across members
      Given a workspace with 5 members all depending on "common-skill" via "catalog:"
      When the catalog version for "common-skill" is updated to "^3.0.0"
      And the user runs "aipm install"
      Then all 5 members resolve "common-skill" using the new "^3.0.0" range

  Rule: Workspace filtering (inspired by pnpm --filter)

    Scenario: Filter commands by package name pattern
      Given a workspace with members "plugin-auth", "plugin-lint", "plugin-format"
      When the user runs "aipm lint --filter 'plugin-*'"
      Then lint runs for all three members

    Scenario: Filter commands by directory path
      Given a workspace with members in "plugins/" and "tools/"
      When the user runs "aipm build --filter './plugins/**'"
      Then only members under "plugins/" are built

    Scenario: Filter by packages changed since a git ref
      Given a workspace with members "plugin-a" and "plugin-b"
      And only "plugin-a" has changes since "origin/main"
      When the user runs "aipm test --filter '[origin/main]'"
      Then only "plugin-a" tests are run

    Scenario: Filter includes transitive dependencies
      Given "plugin-cli" depends on "plugin-core" in the workspace
      When the user runs "aipm build --filter 'plugin-cli...'"
      Then both "plugin-cli" and "plugin-core" are built
      And "plugin-core" is built before "plugin-cli"

    Scenario: Filter by dependents of a package
      Given "plugin-a" and "plugin-b" both depend on "shared-utils"
      When the user runs "aipm test --filter '...shared-utils'"
      Then tests run for "shared-utils", "plugin-a", and "plugin-b"

    Scenario: Exclude packages from filter
      Given a workspace with members "plugin-a", "plugin-b", "plugin-c"
      When the user runs "aipm lint --filter '!plugin-c'"
      Then lint runs for "plugin-a" and "plugin-b"
      But not for "plugin-c"
