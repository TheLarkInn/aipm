# Installing Plugins from Git Repositories

Install plugins directly from any git repository without publishing to a registry.

## CLI Usage

### Generic git URL

```bash
# Full URL with subdirectory and ref
aipm install git:https://github.com/org/repo:plugins/my-plugin@main

# Full URL without subdirectory (entire repo is the plugin)
aipm install git:https://github.com/org/repo@v2.0

# Any git host
aipm install git:https://git.company.com/team/plugins:my-plugin@main
```

### GitHub shorthand

```bash
# Equivalent to git:https://github.com/org/repo:plugins/my-plugin@main
aipm install github:org/repo:plugins/my-plugin@main

# Without subdirectory
aipm install github:org/my-plugin@main
```

## Manifest Usage

In your `aipm.toml`:

```toml
[dependencies]
# Git source
my-git-plugin = { git = "https://github.com/org/repo", path = "plugins/foo", ref = "main" }

# GitHub sugar
my-gh-plugin = { github = "org/repo", path = "plugins/bar", ref = "v2.0" }

# Entire repo as plugin
whole-repo = { git = "https://github.com/org/my-plugin", ref = "main" }
```

## Authentication

aipm delegates all git authentication to the system's **git credential helper**. It does not store or manage credentials.

### Setup for private repos

1. **GitHub**: Install [Git Credential Manager](https://github.com/git-ecosystem/git-credential-manager) or use `gh auth setup-git`
2. **Self-hosted**: Configure `git config --global credential.helper` for your host
3. **CI/CD**: Set `GIT_ASKPASS` or `GIT_CREDENTIAL_HELPER` in your pipeline environment

### Verify credentials work

```bash
# Test that git clone works outside aipm
git clone --depth=1 https://github.com/org/private-repo /tmp/test-clone
```

If this works, `aipm install` will too.

## How It Works

1. aipm runs `git clone --depth=1 --branch <ref> <url>` to a temporary directory
2. If a `path` is specified, only that subdirectory is copied to the plugin location
3. The `.git` directory is stripped (not copied to the plugin directory)
4. The plugin is cached in `~/.aipm/cache/` for future installs (Auto policy)
5. Engine validation and platform compatibility are checked post-clone

## Cache Behavior

By default (Auto policy), git-sourced plugins are cached for 24 hours. Override with `--plugin-cache`:

```bash
# Always re-download (useful during development)
aipm install --plugin-cache force-refresh github:org/repo:plugin@main

# Use cache regardless of age (offline/reproducible builds)
aipm install --plugin-cache no-refresh github:org/repo:plugin@main

# Fail if not cached (air-gapped environments)
aipm install --plugin-cache cache-only github:org/repo:plugin@main

# Never use cache
aipm install --plugin-cache skip github:org/repo:plugin@main
```

## Lockfile Entry

Git dependencies are recorded in `aipm.lock` as:

```toml
[[package]]
name = "my-git-plugin"
version = "0.0.0"
source = "git+https://github.com/org/repo?path=plugins/foo&ref=main"
checksum = "sha512-..."
```

## Examples

### Public GitHub plugin

```bash
aipm install github:anthropics/claude-plugins:plugins/code-review@main
```

### Private repo with credential helper

```bash
# Ensure credentials are configured
gh auth setup-git
aipm install github:my-company/internal-plugins:tools/linter@v1.0
```

### Self-hosted git

```bash
aipm install git:https://git.company.com/team/plugins:governance/compliance@main
```

---

See also: [`aipm install`](../../README.md#aipm-install), [`docs/guides/install-marketplace-plugin.md`](./install-marketplace-plugin.md), [`docs/guides/install-local-plugin.md`](./install-local-plugin.md), [`docs/guides/source-security.md`](./source-security.md), [`docs/guides/cache-management.md`](./cache-management.md), [`docs/guides/global-plugins.md`](./global-plugins.md), [`docs/guides/uninstall.md`](./uninstall.md), [`docs/guides/update.md`](./update.md) — lockfile semantics and version-range upgrades.
