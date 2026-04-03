# hook/unknown-event

**Severity:** error
**Fixable:** No

Checks that every event name declared in a `hooks.json` file is a recognised aipm hook event. Unknown event names are silently ignored at runtime, meaning the hook will never fire.

## Examples

### Incorrect
```json
{
  "hooks": [
    { "event": "on_install", "command": "./setup.sh" }
  ]
}
```

### Correct
```json
{
  "hooks": [
    { "event": "PostInstall", "command": "./setup.sh" }
  ]
}
```

## How to fix
Replace the unknown event name with a valid hook event name. Consult the aipm documentation for the full list of supported events.
