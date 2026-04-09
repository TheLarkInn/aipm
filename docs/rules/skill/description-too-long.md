# skill/description-too-long

**Severity:** warning
**Fixable:** No

Checks that the `description` field in SKILL.md frontmatter is no longer than 1 024 characters. This limit is derived from the Copilot CLI Zod schema (`z.string().max(1024)`). Very long descriptions are truncated in `aipm list` output and plugin marketplace listings.

## Examples

### Incorrect
```markdown
---
name: my-skill
description: [a description exceeding 1 024 characters]
---
```

### Correct
```markdown
---
name: my-skill
description: Does something useful in a single, concise sentence.
---
```

## How to fix
Shorten the description to 1 024 characters or fewer. Keep it to one or two sentences that capture the core purpose of the skill.
