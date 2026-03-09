@p1 @dependencies @patching
Feature: Dependency patching
  As a plugin developer,
  I want to patch installed dependencies without forking them,
  so that I can fix bugs or apply workarounds while waiting for upstream fixes.

  This feature is inspired by pnpm's built-in patch command, eliminating
  the need for third-party patching tools.

  Rule: Create and apply patches

    Scenario: Start patching a dependency
      Given "code-review-skill" at version "1.0.0" is installed
      When the user runs "aipm patch code-review-skill@1.0.0"
      Then a temporary directory is created with the package contents
      And the path to the temporary directory is displayed for editing

    Scenario: Commit a patch after editing
      Given the user has edited files in the patch directory for "code-review-skill@1.0.0"
      When the user runs "aipm patch-commit <temp-directory>"
      Then a unified diff file is created at "patches/code-review-skill@1.0.0.patch"
      And the manifest is updated with a patched dependencies entry
      And the patch is applied to the installed package

    Scenario: Patches are applied automatically on install
      Given the manifest declares a patched dependency:
        """toml
        [patched_dependencies]
        "code-review-skill@1.0.0" = "patches/code-review-skill@1.0.0.patch"
        """
      And the patch file exists at "patches/code-review-skill@1.0.0.patch"
      When the user runs "aipm install"
      Then "code-review-skill" is installed with the patch applied

    Scenario: Patch file is tracked in version control
      Given a patch was committed for "code-review-skill@1.0.0"
      When another developer clones the project and runs "aipm install"
      Then the same patch is applied to their installation

  Rule: Manage patches

    Scenario: Remove a patch
      Given a patch exists for "code-review-skill@1.0.0"
      When the user runs "aipm patch-remove code-review-skill@1.0.0"
      Then the patch file is deleted
      And the patched dependencies entry is removed from the manifest
      And the package is reinstalled without the patch

    Scenario: List all active patches
      Given patches exist for "code-review-skill@1.0.0" and "lint-tool@2.1.0"
      When the user runs "aipm patch-list"
      Then both patches are listed with their file paths

    Scenario: Warn when a patch becomes obsolete
      Given a patch exists for "code-review-skill@1.0.0"
      And the user updates "code-review-skill" to "1.1.0"
      When the user runs "aipm install"
      Then a warning is displayed: "patch for code-review-skill@1.0.0 may not apply to 1.1.0"

  Rule: Patch safety

    Scenario: Reject patch that modifies the package manifest
      Given a patch that changes the dependency's aipm.toml
      When the user runs "aipm patch-commit <temp-directory>"
      Then the command fails with "patches must not modify package manifests"
      And a hint suggests using overrides instead

    Scenario: Failed patch application does not corrupt the install
      Given a patch for "code-review-skill@1.0.0" that cannot be applied cleanly
      When the user runs "aipm install"
      Then the command fails with "patch failed to apply for code-review-skill@1.0.0"
      And the package is left in its unpatched state

    Scenario: Allow unused patches without failing
      Given the manifest setting "allow_unused_patches" is true
      And a patch exists for "old-plugin@1.0.0" which is no longer a dependency
      When the user runs "aipm install"
      Then the install succeeds with a warning about the unused patch
