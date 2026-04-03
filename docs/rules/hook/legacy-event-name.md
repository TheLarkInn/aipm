# hook/legacy-event-name

**Severity:** warning
**Fixable:** No

Checks that hook event names in `hooks.json` use the current PascalCase naming convention rather than a legacy snake_case or camelCase alias. Legacy names are deprecated and may be removed in a future release.

## Examples

### Incorrect
```json
{
  "hooks": [
    { "event": "pre_install", "command": "./prepare.sh" }
  ]
}
```

### Correct
```json
{
  "hooks": [
    { "event": "PreInstall", "command": "./prepare.sh" }
  ]
}
```

## How to fix
Rename the event value to its PascalCase equivalent (e.g. `pre_install` becomes `PreInstall`). Run `aipm migrate` to have the migration tool update hook event names automatically.
