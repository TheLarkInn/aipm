@p1 @environment
Feature: Environment dependency declarations
  As a plugin author,
  I want to declare what environment my plugin requires,
  so that consumers know upfront if their system meets the requirements.

  Rule: Declare required environment capabilities

    Scenario: Declare required system tools
      Given a manifest with environment dependencies:
        """toml
        [environment]
        requires = ["git", "docker"]
        """
      When the user runs "aipm validate"
      Then the command checks that "git" is available on PATH
      And the command checks that "docker" is available on PATH

    Scenario: Warn when a required tool is missing
      Given a manifest requiring "docker"
      And "docker" is not installed on the system
      When the user runs "aipm install"
      Then a warning is displayed: "missing required environment dependency: docker"

    Scenario: Fail hard on missing environment dependency
      Given a manifest with:
        """toml
        [environment]
        requires = ["git"]
        strict = true
        """
      And "git" is not available on PATH
      When the user runs "aipm install"
      Then the command fails with "required environment dependency not found: git"

  Rule: Declare required environment variables

    Scenario: Declare required environment variables
      Given a manifest with:
        """toml
        [environment.variables]
        required = ["OPENAI_API_KEY", "DATABASE_URL"]
        """
      When the user runs "aipm validate"
      Then the validator checks for the presence of "OPENAI_API_KEY"
      And the validator checks for the presence of "DATABASE_URL"

    Scenario: Warn when a required env var is missing
      Given a manifest requiring env var "OPENAI_API_KEY"
      And the environment variable "OPENAI_API_KEY" is not set
      When the user runs "aipm install"
      Then a warning is displayed: "missing required environment variable: OPENAI_API_KEY"

    Scenario: Env var requirement with description
      Given a manifest with:
        """toml
        [[environment.variables.spec]]
        name = "OPENAI_API_KEY"
        description = "API key for OpenAI services"
        required = true

        [[environment.variables.spec]]
        name = "LOG_LEVEL"
        description = "Logging verbosity"
        required = false
        default = "info"
        """
      When the user runs "aipm info"
      Then the environment variable requirements are listed with descriptions

  Rule: Declare runtime/platform constraints

    Scenario: Declare minimum aipm version
      Given a manifest with:
        """toml
        [environment]
        aipm = ">=0.5.0"
        """
      And the installed aipm version is "0.3.0"
      When the user runs "aipm install"
      Then the command fails with "requires aipm >= 0.5.0, found 0.3.0"

    Scenario: Declare supported platforms
      Given a manifest with:
        """toml
        [environment]
        platforms = ["linux-x64", "macos-arm64", "windows-x64"]
        """
      And the current platform is "linux-arm64"
      When the user runs "aipm install"
      Then a warning is displayed: "current platform linux-arm64 not in supported platforms"

    Scenario: Declare MCP server runtime requirements
      Given a package containing an MCP server that requires Node.js
      And the manifest declares:
        """toml
        [environment.runtime]
        node = ">=18.0.0"
        """
      And Node.js version "16.0.0" is installed
      When the user runs "aipm install"
      Then a warning is displayed: "MCP server requires node >= 18.0.0, found 16.0.0"

  Rule: Environment check command

    Scenario: Check all environment requirements
      Given a project with multiple installed packages having environment requirements
      When the user runs "aipm doctor"
      Then all environment requirements across all installed packages are checked
      And a summary report shows which requirements are met and which are missing

    Scenario: Doctor command suggests fixes
      Given a package requiring "git" which is not installed
      When the user runs "aipm doctor"
      Then the report includes the missing requirement
      And a suggested installation command is provided
