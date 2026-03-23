@p0 @manifest @migrate
Feature: Migrate AI tool configurations into marketplace plugins

  Rule: Skills are migrated as plugins

    Scenario: Migrate a single skill from .claude/skills/
      Given an empty directory "my-project"
      And a workspace initialized in "my-project"
      And a skill "deploy" exists in "my-project"
      When the user runs "aipm migrate" in "my-project"
      Then the command succeeds
      And a plugin directory exists at ".ai/deploy/" in "my-project"
      And the file ".ai/deploy/aipm.toml" in "my-project" contains 'name = "deploy"'
      And the file ".ai/deploy/aipm.toml" in "my-project" contains 'type = "skill"'
      And a file ".ai/deploy/skills/deploy/SKILL.md" exists in "my-project"
      And the marketplace.json in "my-project" contains plugin "deploy"

    Scenario: Original skill files are preserved after migration
      Given an empty directory "my-project"
      And a workspace initialized in "my-project"
      And a skill "deploy" exists in "my-project"
      When the user runs "aipm migrate" in "my-project"
      Then a file ".claude/skills/deploy/SKILL.md" exists in "my-project"

    Scenario: Migrated plugins are not auto-enabled
      Given an empty directory "my-project"
      And a workspace initialized in "my-project"
      And a skill "deploy" exists in "my-project"
      When the user runs "aipm migrate" in "my-project"
      Then the file ".claude/settings.json" in "my-project" does not contain "deploy@local-repo-plugins"

  Rule: Legacy commands are converted to skills

    Scenario: Migrate a legacy command with disable-model-invocation
      Given an empty directory "my-project"
      And a workspace initialized in "my-project"
      And a command "review" exists in "my-project"
      When the user runs "aipm migrate" in "my-project"
      Then the file ".ai/review/skills/review/SKILL.md" in "my-project" contains "disable-model-invocation: true"

  Rule: Name conflicts are resolved by renaming

    Scenario: Plugin name conflict triggers auto-rename
      Given an empty directory "my-project"
      And a workspace initialized in "my-project"
      And a pre-existing plugin directory "deploy" in "my-project"
      And a skill "deploy" exists in "my-project"
      When the user runs "aipm migrate" in "my-project"
      Then a plugin directory exists at ".ai/deploy-renamed-1/" in "my-project"
      And the output contains "renamed"

  Rule: Dry run produces report without side effects

    Scenario: Dry run generates report file
      Given an empty directory "my-project"
      And a workspace initialized in "my-project"
      And a skill "deploy" exists in "my-project"
      When the user runs "aipm migrate --dry-run" in "my-project"
      Then a file "aipm-migrate-dryrun-report.md" exists in "my-project"
      And no plugin directory exists at ".ai/deploy/" in "my-project"

  Rule: Prerequisites are validated

    Scenario: Error when marketplace directory is missing
      Given an empty directory "my-project"
      And a skill "deploy" exists in "my-project"
      When the user runs "aipm migrate" in "my-project"
      Then the command fails
      And the error contains "aipm init"
