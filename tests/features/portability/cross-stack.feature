@p1 @portability
Feature: Cross-repo and cross-tech-stack portability
  As a developer working across multiple technology stacks,
  I want plugins and their artifacts to work in any monorepo,
  so that I am not limited to Node/TS environments.

  Rule: Packages are technology-stack agnostic

    Scenario: Plugin works in a Node.js/TypeScript project
      Given a project using Node.js as the primary runtime
      When the user runs "aipm install code-review-skill"
      Then the skill is installed and usable
      And no Node.js-specific files are required

    Scenario: Plugin works in a .NET/C# monorepo
      Given a project using .NET as the primary tech stack
      When the user runs "aipm install code-review-skill"
      Then the skill is installed and usable
      And no Node.js or npm artifacts are created

    Scenario: Plugin works in a Python monorepo
      Given a project using Python as the primary tech stack
      When the user runs "aipm install code-review-skill"
      Then the skill is installed and usable

    Scenario: Plugin works in a Rust project
      Given a project using Rust and Cargo as the build system
      When the user runs "aipm install code-review-skill"
      Then the skill is installed alongside Cargo dependencies without conflict

  Rule: Package format is runtime-independent

    Scenario: Package archive contains no runtime-specific build artifacts
      Given a published package "portable-skill"
      When the package archive is inspected
      Then it contains only SKILL.md, agent definitions, hook configs, and MCP configs
      And it does not contain node_modules, bin, or compiled binaries

    Scenario: Manifest supports declaring runtime-specific adapters
      Given a manifest with:
        """toml
        [adapters]
        node = { entry = "adapters/node/index.js" }
        dotnet = { entry = "adapters/dotnet/Plugin.cs" }
        python = { entry = "adapters/python/plugin.py" }
        """
      When the manifest is validated
      Then all declared adapter entries are verified to exist

  Rule: CLI binary is self-contained

    Scenario: aipm CLI does not require Node.js
      Given a machine without Node.js installed
      When the user runs "aipm --version"
      Then the version is displayed successfully

    Scenario: aipm CLI does not require Python
      Given a machine without Python installed
      When the user runs "aipm install code-review-skill"
      Then the package is installed successfully

    Scenario Outline: aipm CLI works on multiple platforms
      Given the aipm binary for "<platform>"
      When the user runs "aipm --version"
      Then the version is displayed successfully

      Examples:
        | platform       |
        | linux-x64      |
        | linux-arm64    |
        | macos-x64      |
        | macos-arm64    |
        | windows-x64    |

  Rule: Integration with non-Node build systems

    Scenario: aipm integrates with MSBuild projects
      Given a .NET solution with an MSBuild project file
      When the user runs "aipm orchestrator init --type msbuild"
      Then MSBuild targets for aipm restore and validation are generated

    Scenario: aipm integrates with CMake projects
      Given a C++ project with CMakeLists.txt
      When the user runs "aipm orchestrator init --type cmake"
      Then CMake integration files are generated

    Scenario: Package resolution works offline from a local cache
      Given all required packages are in the local cache
      And the network is unavailable
      When the user runs "aipm install --offline"
      Then all packages are installed from the local cache
