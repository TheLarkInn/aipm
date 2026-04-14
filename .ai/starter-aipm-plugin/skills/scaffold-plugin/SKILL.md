---
name: scaffold-plugin
description: Scaffold a new AI plugin in the .ai/ marketplace directory. Use when the user wants to create a new plugin, skill, agent, or hook package.
---

# Scaffold Plugin

Create a new plugin in the `.ai/` marketplace directory.

## Instructions

1. Ask the user for a plugin name (lowercase, hyphens allowed) if not provided.
2. Run the scaffolding script:
   ```bash
   bash .ai/starter-aipm-plugin/scripts/scaffold-plugin.sh <plugin-name>
   ```
3. Report the created file tree to the user.
4. Suggest next steps: edit the generated `SKILL.md`, add agents or hooks.

## Notes

- The script creates `.ai/<plugin-name>/` with starter plugin files, including a starter skill.
- If the directory already exists, `aipm make plugin` is idempotent (reports existing files without failing).
- After scaffolding, customize the scaffolded plugin files as needed.
