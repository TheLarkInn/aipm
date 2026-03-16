@p0 @registry @security
Feature: Package security and integrity
  As a plugin ecosystem participant,
  I want packages to be verified for integrity and safety,
  so that the supply chain is protected against tampering and malicious packages.

  Rule: Integrity verification

    Scenario: Published packages include integrity checksums
      When the user runs "aipm-pack publish"
      Then the registry stores a SHA-256 checksum of the archive
      And the lockfile records the integrity hash

    Scenario: Install verifies checksums against the lockfile
      Given a lockfile with integrity hash for "code-review" at "1.0.0"
      And the downloaded archive matches the recorded hash
      When the user runs "aipm install --locked"
      Then the installation succeeds

    Scenario: Install rejects tampered packages
      Given a lockfile with integrity hash for "code-review" at "1.0.0"
      And the downloaded archive does not match the recorded hash
      When the user runs "aipm install --locked"
      Then the command fails with "integrity check failed for code-review@1.0.0"

  Rule: Security auditing

    Scenario: Audit installed dependencies for known vulnerabilities
      Given a project with installed dependencies
      And the advisory database contains a vulnerability for "old-skill" at "1.0.0"
      And "old-skill" at "1.0.0" is installed
      When the user runs "aipm audit"
      Then the vulnerability is reported with severity level
      And a recommended fix version is suggested

    Scenario: Audit on install warns about vulnerabilities
      Given the advisory database contains a high-severity vulnerability for "risky-plugin"
      When the user runs "aipm install risky-plugin@1.0.0"
      Then a warning is displayed about the known vulnerability

  Rule: Authentication and authorization

    Scenario: Login stores credentials securely
      When the user runs "aipm-pack login"
      And provides valid credentials
      Then an API token is stored in the local credential store
      And the token file has restricted permissions

    Scenario: Publish requires authentication
      Given the user is not authenticated
      When the user runs "aipm-pack publish"
      Then the command fails with "authentication required"

    Scenario: Scoped packages respect org permissions
      Given the user is a member of org "myorg"
      But does not have publish permission for "@myorg/restricted"
      When the user runs "aipm-pack publish" for "@myorg/restricted"
      Then the command fails with "insufficient permissions"
