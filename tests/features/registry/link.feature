@p0 @registry @link
Feature: Local development overrides via link
  As a plugin developer,
  I want to override a registry dependency with a local directory,
  so that I can develop and test changes to a dependency without publishing.

  The `aipm link` command lives in the consumer binary because it's a
  development workflow tool, not a publish operation. It replaces the
  directory link for a registry dependency with a link to a local directory,
  allowing rapid iteration without needing `aipm-pack`. Uses symlinks on
  macOS/Linux, directory junctions on Windows (no elevation required).

  Rule: Link overrides a registry dependency with a local path

    Scenario: Link a local directory as a dependency override
      Given "code-review-skill" at "1.0.0" is installed from the registry
      And a local development copy exists at "/work/code-review-skill"
      When the user runs "aipm link /work/code-review-skill"
      Then the directory link for "code-review-skill" in the plugins directory points to "/work/code-review-skill"
      And the lockfile is not modified
      And a notice is displayed: "linked code-review-skill → /work/code-review-skill"

    Scenario: Link replaces the registry directory link but preserves the lockfile pin
      Given "code-review-skill" at "1.0.0" is installed and locked
      When the user runs "aipm link /work/code-review-skill"
      Then the lockfile still pins "code-review-skill" to "1.0.0"
      And `aipm install --locked` will restore the registry version

    Scenario: Link validates the target has a compatible manifest
      Given "code-review-skill" at "1.0.0" is installed from the registry
      And "/work/code-review-skill/aipm.toml" declares name "code-review-skill"
      When the user runs "aipm link /work/code-review-skill"
      Then the link succeeds

    Scenario: Link fails if target has no manifest
      Given "code-review-skill" at "1.0.0" is installed from the registry
      And "/work/some-dir" does not contain an "aipm.toml"
      When the user runs "aipm link /work/some-dir"
      Then the command fails with "no aipm.toml found in /work/some-dir"

    Scenario: Link fails if target package name doesn't match
      Given "code-review-skill" at "1.0.0" is installed from the registry
      And "/work/other-plugin/aipm.toml" declares name "other-plugin"
      When the user runs "aipm link /work/other-plugin"
      Then the command fails with "package name mismatch: expected code-review-skill, found other-plugin"

    Scenario: Link a package that is not yet installed
      Given no registry dependency "code-review-skill" in the manifest
      And a local directory "/work/code-review-skill" with a valid manifest
      When the user runs "aipm link /work/code-review-skill"
      Then a directory link is created in the plugins directory for "code-review-skill"
      And the manifest is NOT modified (link is a dev-only override)

  Rule: Unlink restores the registry version

    Scenario: Unlink restores the registry directory link
      Given "code-review-skill" is linked to "/work/code-review-skill"
      And the lockfile pins "code-review-skill" to "1.0.0"
      When the user runs "aipm unlink code-review-skill"
      Then the directory link is restored to point to ".aipm/links/code-review-skill"
      And a notice is displayed: "unlinked code-review-skill, restored registry version 1.0.0"

    Scenario: Unlink a package that was link-only (not in manifest)
      Given "code-review-skill" is linked but not in the manifest
      When the user runs "aipm unlink code-review-skill"
      Then the directory link is removed entirely
      And no manifest changes are made

    Scenario: Install with --locked removes all links
      Given "code-review-skill" is linked to a local directory
      When the user runs "aipm install --locked"
      Then all links are replaced with registry versions from the lockfile
      And a warning is displayed: "overriding linked package: code-review-skill"

  Rule: List linked packages

    Scenario: Show currently linked packages
      Given "code-review-skill" is linked to "/work/code-review-skill"
      And "lint-tool" is linked to "/work/lint-tool"
      When the user runs "aipm list --linked"
      Then the output shows:
        | package             | linked_to                  |
        | code-review-skill   | /work/code-review-skill    |
        | lint-tool           | /work/lint-tool            |
