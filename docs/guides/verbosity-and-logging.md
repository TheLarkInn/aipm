# Verbosity and Logging

`aipm` emits diagnostics through a layered logging system: structured output on
stderr controlled by CLI flags, and an always-on file log for post-hoc debugging.

## Default Behavior

By default, `aipm` prints only warnings and errors to stderr and is silent for
routine operations. This is the recommended setting for interactive use.

```bash
aipm install github:org/repo:plugin@main   # no output unless something goes wrong
```

## Verbosity Flags

Verbosity is controlled by repeating `-v` (increase) or `-q` (decrease):

| Flags | Level | What you see |
|-------|-------|--------------|
| `-qq` | OFF | Silent — no stderr output |
| `-q` | ERROR | Fatal errors only |
| *(default)* | WARN | Warnings + errors |
| `-v` | INFO | Progress messages |
| `-vv` | DEBUG | Detailed internal steps |
| `-vvv` | TRACE | Every code path (very verbose) |

```bash
# Show progress messages during install
aipm install -v github:org/repo:plugin@main

# Debug a slow migration
aipm migrate -vv --dry-run

# Completely silent (useful in scripts where only the exit code matters)
aipm lint -qq
```

## Log Format

Use `--log-format` to switch stderr between human-readable text and machine-readable
JSON. The file log always uses text format.

```bash
# Default: human-readable text
aipm install -v github:org/repo:plugin@main

# JSON: structured output for agentic consumers or log pipelines
aipm install --log-format json -v github:org/repo:plugin@main
```

JSON output follows the `tracing-subscriber` JSON format, with `timestamp`,
`level`, `target`, `fields`, and `span` keys.

## `AIPM_LOG` Environment Variable

For fine-grained control, set `AIPM_LOG` to a
[`tracing` `EnvFilter` directive](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html).
When set, it takes precedence over all CLI verbosity flags.

```bash
# Enable debug level globally
AIPM_LOG=debug aipm install github:org/repo:plugin@main

# Enable trace only for the installer module
AIPM_LOG=libaipm::installer=trace aipm install github:org/repo:plugin@main

# Enable debug for the whole library, warn for everything else
AIPM_LOG=libaipm=debug,warn aipm migrate --dry-run

# Trace only the migration pipeline (discovery → emit → reconcile)
AIPM_LOG=libaipm::migrate=trace aipm migrate --dry-run
```

This is especially useful for narrowing down a specific subsystem without the
noise of full `-vvv` trace output.

## Log File

Every `aipm` run appends `DEBUG`-level diagnostics to a rotating file regardless
of stderr verbosity:

```
<system-temp>/aipm-YYYY-MM-DD.log
```

- **Rotation**: daily — a new file is created each calendar day
- **Retention**: 7 days — older files are automatically removed
- **Level**: always DEBUG (capturing more detail than the default stderr output)
- **Format**: plain text (no ANSI colors)

The log file is useful for diagnosing issues after the fact, especially when `aipm`
was run with the default (quiet) verbosity and something went wrong.

```bash
# Show today's log on Linux/macOS
cat /tmp/aipm-$(date +%Y-%m-%d).log

# Follow the log in real time
tail -f /tmp/aipm-$(date +%Y-%m-%d).log
```

On Windows, `<system-temp>` resolves to `%TEMP%` (typically
`C:\Users\<user>\AppData\Local\Temp`).

## CI Recommendations

### GitHub Actions

```yaml
- name: Lint plugins
  run: aipm lint --reporter ci-github
  # GitHub Actions captures stderr — warnings appear as annotations
```

Use `-v` if you want install/migrate progress in the Actions log:

```yaml
- name: Install plugins
  run: aipm install --locked -v
```

### Suppress all stderr in scripts

```bash
#!/usr/bin/env bash
set -e
aipm lint -qq   # exit code 0/1 only, no stderr
```

### Debug a CI failure locally

```bash
# Reproduce with maximum verbosity
aipm install --locked -vvv 2>&1 | tee aipm-debug.log

# Or target just one subsystem
AIPM_LOG=libaipm::installer=trace aipm install --locked
```
