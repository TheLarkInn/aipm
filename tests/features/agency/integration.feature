@p1 @agency
Feature: Agency integration
  As a developer using Microsoft internal tooling,
  I want AIPM to integrate cleanly with Agency,
  so that packages can declare and configure Agency-wrapped MCP servers
  without manual authentication setup.

  Agency is a Microsoft 1ES/StartRight tool that wraps agent CLIs
  (Claude Code, Copilot) and provides automatic Azure authentication
  for internal MCP servers (ADO, Bluebird, WorkIQ, ES-Chat, Kusto, etc.).

  Rule: Packages can declare Agency MCP server dependencies

    Scenario: Declare an Agency-wrapped MCP server in the manifest
      Given a manifest with:
        """toml
        [agency.mcp_servers.ado]
        organization = "onedrive"
        """
      When the user runs "aipm validate"
      Then the command succeeds
      And the manifest recognizes "ado" as a known Agency MCP server

    Scenario: Declare multiple Agency MCP servers
      Given a manifest with:
        """toml
        [agency.mcp_servers.ado]
        organization = "onedrive"

        [agency.mcp_servers.bluebird]
        organization = "onedrive"
        project = "ODSP-Web"
        repository = "odsp-web"

        [agency.mcp_servers.kusto]
        service_uri = "https://kusto.aria.microsoft.com"
        """
      When the user runs "aipm validate"
      Then all three Agency MCP server declarations are valid

    Scenario: Declare a remote Agency MCP server
      Given a manifest with:
        """toml
        [agency.mcp_servers.code-companion]
        type = "remote"
        url = "https://codecompanionmcp.azurewebsites.net/mcp"
        """
      When the user runs "aipm validate"
      Then the remote MCP server declaration is valid

  Rule: AIPM generates valid .mcp.json for Agency

    Scenario: Generate .mcp.json from Agency declarations
      Given a manifest declaring Agency MCP server "bluebird" with organization "onedrive"
      When the user runs "aipm generate-mcp-config"
      Then a file ".mcp.json" is created with:
        """json
        {
          "mcpServers": {
            "bluebird": {
              "type": "stdio",
              "command": "dev",
              "args": ["agency", "mcp", "bluebird", "--organization", "onedrive"]
            }
          }
        }
        """

    Scenario: Generate .mcp.json with multiple Agency servers
      Given a manifest declaring Agency MCP servers "ado", "bluebird", and "es-chat"
      When the user runs "aipm generate-mcp-config"
      Then the generated ".mcp.json" contains entries for all three servers
      And each entry uses the "dev agency mcp" command pattern

    Scenario: Merge Agency MCP config with non-Agency MCP config
      Given a manifest declaring:
        | type           | server      |
        | agency         | ado         |
        | standard       | sqlite-mcp  |
      When the user runs "aipm generate-mcp-config"
      Then the generated ".mcp.json" contains both Agency and standard MCP entries
      And the Agency entry uses "dev agency mcp ado"
      And the standard entry uses the package's own MCP command

  Rule: Agency authentication is delegated, not duplicated

    Scenario: AIPM does not store Azure credentials
      When the user runs "aipm install" for a package with Agency dependencies
      Then no Azure credentials or tokens are written to the AIPM config
      And no authentication prompts are shown by AIPM

    Scenario: Warn when Agency is not available
      Given the "dev" CLI is not available on PATH
      And a package declares Agency MCP server dependencies
      When the user runs "aipm install"
      Then a warning is displayed: "Agency CLI not found; Agency MCP servers will not be available"
      And the rest of the installation proceeds

    Scenario: Warn when Azure auth is not configured
      Given the "dev" CLI is available
      But "az login" has not been run
      When the user runs "aipm doctor"
      Then the report includes: "Azure authentication not configured (run 'az login')"

  Rule: Installed packages with Agency deps are discoverable

    Scenario: List Agency MCP servers from installed packages
      Given installed packages with the following Agency MCP server declarations:
        | package       | agency_mcp |
        | ci-tools      | ado        |
        | search-plugin | bluebird   |
        | data-explorer | kusto      |
      When the user runs "aipm list --agency"
      Then the output shows all Agency MCP servers grouped by package

    Scenario: Resolve full configuration for an agent using Agency servers
      Given an agent "code-reviewer" that depends on package "ci-tools"
      And "ci-tools" declares Agency MCP server "ado" with organization "onedrive"
      When the user runs "aipm resolve --agent code-reviewer"
      Then the output includes the Agency MCP server "ado" with its configuration

  Rule: Cross-package Agency MCP server deduplication

    Scenario: Multiple packages requiring the same Agency MCP server
      Given package "plugin-a" declares Agency MCP "ado" with organization "onedrive"
      And package "plugin-b" declares Agency MCP "ado" with organization "onedrive"
      When the user runs "aipm generate-mcp-config"
      Then only one "ado" entry appears in ".mcp.json"

    Scenario: Conflict when packages require same server with different configs
      Given package "plugin-a" declares Agency MCP "ado" with organization "onedrive"
      And package "plugin-b" declares Agency MCP "ado" with organization "office"
      When the user runs "aipm generate-mcp-config"
      Then the command warns about conflicting configurations for "ado"
      And both configurations are generated with disambiguated names

  Rule: Agency integration in plugin export

    Scenario: Export as Claude Code plugin preserves Agency MCP config
      Given a project with installed Agency MCP server dependencies
      When the user runs "aipm export --format claude-plugin"
      Then the generated plugin.json references the Agency MCP servers
      And the generated .mcp.json uses "dev agency mcp" commands
