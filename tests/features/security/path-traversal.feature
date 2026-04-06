@p0 @security
Feature: Path traversal prevention
  As a plugin consumer,
  I want path traversal attacks to be blocked,
  so that malicious plugins cannot escape their directory.

  Scenario: Directory traversal in git spec is rejected
    When the user runs "aipm install git:https://github.com/org/repo:../../../etc/passwd@main"
    Then the install fails with a "Path traversal" error

  Scenario: URL-encoded traversal in spec is rejected
    When the user runs "aipm install git:https://github.com/org/repo:foo/%2e%2e/bar@main"
    Then the install fails with a "Path traversal" error

  Scenario: Absolute path in git spec is rejected
    When the user runs "aipm install git:https://github.com/org/repo:/etc/passwd@main"
    Then the install fails with an "Absolute paths not allowed" error

  Scenario: Null bytes in path are rejected
    When a plugin spec contains null bytes in the path component
    Then the spec is rejected before any filesystem operation
