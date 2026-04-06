@p2 @security
Feature: Source allowlist enforcement
  As a DevOps engineer,
  I want to restrict which plugin sources are allowed in CI,
  so that only trusted sources are used in production builds.

  Scenario: Allowed source installs normally
    Given "~/.aipm/config.toml" has allowed_sources = ["github.com/trusted-org/*"]
    When the user installs "git:https://github.com/trusted-org/repo:plugin@main"
    Then the install succeeds

  Scenario: Disallowed source warns when not enforced
    Given enforce_allowlist = false (default)
    When the user installs from a non-allowed source
    Then the install succeeds with a warning about untrusted source

  Scenario: Disallowed source fails when enforced
    Given AIPM_ENFORCE_ALLOWLIST=1 is set
    When the user installs from a non-allowed source
    Then the install fails with a security error listing allowed sources

  Scenario: Local sources always pass allowlist
    Given enforcement is active
    When the user runs "aipm install local:./my-plugin"
    Then the install succeeds regardless of allowlist contents
