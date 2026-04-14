@p0 @registry
Feature: Package publishing
  As a plugin author,
  I want to pack and publish my package to a registry using the aipm binary,
  so that others can discover and install it.

  Publishing is handled by the "aipm" binary's publish subcommand. This follows
  the principle of least privilege: consumers who only install plugins never need
  the publish subcommand, reducing attack surface.

  Background:
    Given the user is authenticated with the registry
    And a plugin directory "my-plugin" with a valid manifest

  Rule: Packing creates a .aipm archive

    Scenario: Pack a plugin into a .aipm archive
      When the user runs "aipm pack"
      Then a file "my-plugin-0.1.0.aipm" is created in the output directory
      And the archive is a gzip-compressed tar
      And the archive contains a normalized "aipm.toml"

    Scenario: Pack normalizes workspace references
      Given the manifest contains:
        """toml
        [dependencies]
        core-hooks = { workspace = "^" }
        """
      And the workspace member "core-hooks" is at version "2.3.0"
      When the user runs "aipm pack"
      Then the archive's "aipm.toml" contains "core-hooks" at "^2.3.0"
      And no workspace protocol references remain in the packed manifest

    Scenario: Pack normalizes catalog references
      Given the manifest contains:
        """toml
        [dependencies]
        common-skill = "catalog:"
        """
      And the catalog defines "common-skill" as "^2.0.0"
      When the user runs "aipm pack"
      Then the archive's "aipm.toml" contains "common-skill" at "^2.0.0"

    Scenario: Pack is deterministic
      Given a plugin directory with unchanged source files
      When the user runs "aipm pack" twice
      Then both .aipm archives have identical SHA-512 checksums
      And file timestamps in the archive are zeroed
      And files are sorted alphabetically

    Scenario: Pack respects the files allowlist
      Given the manifest declares files to include:
        | path                 |
        | skills/              |
        | agents/              |
        | aipm.toml            |
      And a file "secrets.env" exists in the project
      When the user runs "aipm pack"
      Then the archive does not contain "secrets.env"
      And the archive contains "aipm.toml"

    Scenario: Pack excludes secrets by default
      Given a plugin directory containing ".env", "credentials.json", and ".git/"
      When the user runs "aipm pack"
      Then the archive does not contain ".env"
      And the archive does not contain "credentials.json"
      And the archive does not contain ".git/"

    Scenario: Pack dry-run shows archive contents and size
      When the user runs "aipm pack --dry-run"
      Then the output lists all files that would be included
      And the output shows the estimated archive size
      But no archive file is created

    Scenario: Pack enforces maximum archive size
      Given a plugin with files totaling over the maximum archive size
      When the user runs "aipm pack"
      Then the command fails with "archive exceeds maximum size"

  Rule: Publishing uploads to the registry

    Scenario: Publish a new package version
      Given the package "my-plugin" version "0.1.0" does not exist in the registry
      When the user runs "aipm publish"
      Then the package is packed into a .aipm archive
      And the archive is uploaded to the registry
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

    Scenario: Scoped package publishing
      Given the manifest package name is "@myorg/my-plugin"
      When the user runs "aipm publish"
      Then the package is published under the "@myorg" scope

  Rule: Consumer binary cannot publish

    Scenario: The aipm consumer binary does not have publish commands
      When the user runs "aipm publish"
      Then the command fails with "unknown command: publish"
      And the output suggests "did you mean: aipm publish?"

    Scenario: The aipm consumer binary does not have pack commands
      When the user runs "aipm pack"
      Then the command fails with "unknown command: pack"
      And the output suggests "did you mean: aipm pack?"
