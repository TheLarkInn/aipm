# plugin/broken-paths

**Severity:** error
**Fixable:** No

Checks that every file path referenced in a plugin manifest (e.g. `marketplace.json`) resolves to an existing file on disk. Broken paths prevent the plugin from loading correctly at install or runtime.

## Examples

### Incorrect
```json
{
  "skills": ["skills/SKILL.md", "skills/MISSING.md"]
}
```
*(where `skills/MISSING.md` does not exist)*

### Correct
```json
{
  "skills": ["skills/SKILL.md"]
}
```
*(all listed paths exist on disk)*

## How to fix
Either create the missing file at the referenced path, update the path to point to the correct existing file, or remove the broken reference from the manifest entirely.
