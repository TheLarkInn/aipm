# skill/description-too-long

**Severity:** warning
**Fixable:** No

Checks that the `description` field in SKILL.md frontmatter is no longer than 200 characters. Long descriptions are truncated in `aipm list` output and plugin marketplace listings.

## Examples

### Incorrect
```markdown
---
name: my-skill
description: This description is far too verbose and goes well beyond the two-hundred character limit that is enforced by this rule, causing it to be truncated in listings.
---
```

### Correct
```markdown
---
name: my-skill
description: Does something useful in under 200 characters.
---
```

## How to fix
Shorten the description to 200 characters or fewer. Keep it to one concise sentence that captures the core purpose of the skill.
