---
date: 2026-03-16 13:58:00 PDT
researcher: Claude Opus 4.6
git_commit: d6f1b73b58293b677b67240f92a993b19c880392
branch: main
repository: aipm
topic: "Cross-platform binary distribution and CI/CD release automation for Rust CLI tools"
tags: [research, release, ci-cd, cargo-dist, cross-compilation, installers, github-actions, homebrew, scoop, cargo-binstall]
status: complete
last_updated: 2026-03-16
last_updated_by: Claude Opus 4.6
---

# Cross-Platform Binary Distribution & CI/CD for Rust CLI Tools

## Research Question

How do most Rust tool codebases provide OS-agnostic installers and deploy them in their CI/CD? What patterns should the `aipm` workspace (binaries: `aipm`, `aipm-pack`; library: `libaipm`) adopt for cross-platform release publishing?

## Summary

The Rust ecosystem has converged on **cargo-dist** (by Axo) as the leading all-in-one release automation tool. It generates cross-platform binaries, installers (shell, PowerShell, Homebrew, npm, MSI), and GitHub Actions CI from a single config file. For version management and changelog generation, **release-plz** (CI-automated) or **cargo-release** (CLI-manual) complement cargo-dist. Popular projects like ripgrep, bat, fd, starship, and zoxide demonstrate consistent patterns: tag-triggered GitHub Actions workflows, build matrices targeting 5-8 platforms, musl-linked static Linux binaries, and distribution via GitHub Releases + package managers.

For `aipm` specifically: the workspace has two binary crates and one library. cargo-dist handles this natively — each binary gets independent release artifacts while `libaipm` is ignored. The Windows-only `junction` dependency is automatically handled by `cfg(windows)` during cross-compilation.

---

## Detailed Findings

### 1. cargo-dist — The Recommended All-in-One Tool

**Repository**: https://github.com/axodotdev/cargo-dist (2,000+ stars)
**Docs**: https://axodotdev.github.io/cargo-dist/
**Latest version**: v0.31.0 (February 2026)

#### What It Does

cargo-dist implements a five-stage release pipeline:

1. **Plan** — triggered by git tags; selects apps; generates machine-readable manifests with changelogs
2. **Build** — spins up multi-platform CI runners; compiles binaries; creates archives and installers
3. **Publish** — uploads to package managers (crates.io, npm, Homebrew)
4. **Host** — creates/updates GitHub Releases; uploads artifacts
5. **Announce** — adds release notes parsed from RELEASES/CHANGELOG files

#### Setup for the aipm Workspace

```bash
# Install
cargo install cargo-dist --locked
# OR
cargo binstall cargo-dist

# Initialize (generates CI + config)
dist init --yes
```

This creates either a `dist-workspace.toml` (preferred in v0.20+) or adds `[workspace.metadata.dist]` to `Cargo.toml`, plus generates `.github/workflows/release.yml`.

#### Configuration

**Option A — `dist-workspace.toml` (standalone, recommended):**
```toml
[workspace]
members = ["crates/*"]

[dist]
cargo-dist-version = "0.31.0"
ci = ["github"]
installers = ["shell", "powershell", "homebrew"]
targets = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "aarch64-unknown-linux-gnu",
  "x86_64-unknown-linux-gnu",
  "x86_64-pc-windows-msvc",
]
pr-run-mode = "upload"
install-updater = true
```

**Option B — In `Cargo.toml`:**
```toml
[profile.dist]
inherits = "release"
lto = "thin"

[workspace.metadata.dist]
cargo-dist-version = "0.31.0"
ci = ["github"]
installers = ["shell", "powershell", "homebrew"]
targets = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "aarch64-unknown-linux-gnu",
  "x86_64-unknown-linux-gnu",
  "x86_64-pc-windows-msvc",
]
pr-run-mode = "upload"
```

#### Workspace Behavior

- **Each package with binaries is an "App"** — `aipm` and `aipm-pack` each get independent archives/installers
- **Packages without binaries are ignored** — `libaipm` is skipped entirely
- **Unified tag** (`v0.1.0`) releases all packages at that version (ideal for lockstep versioning)
- **Singular tag** (`aipm-v0.1.0`) releases only the named package (for independent versioning)

#### Installer Formats Generated

| Installer     | Platform        | How Users Install                           |
|---------------|-----------------|---------------------------------------------|
| `shell`       | Unix            | `curl -sSf https://... \| sh`               |
| `powershell`  | Windows         | `irm https://... \| iex`                    |
| `homebrew`    | macOS/Linux     | `brew install org/tap/aipm`                  |
| `npm`         | Cross-platform  | `npx @org/aipm` (thin wrapper around binary) |
| `msi`         | Windows         | Standard Windows Installer (WiX)             |
| `pkg`         | macOS           | macOS `.pkg` installer                       |
| `updater`     | Cross-platform  | Built-in self-update (axoupdater)            |

#### Release Workflow

```bash
# 1. Update version in Cargo.toml (or use release-plz/cargo-release)
# 2. Commit and push
git commit -am "release: version 0.1.0"
git push
# 3. Tag and push — triggers CI
git tag v0.1.0
git push --tags
```

The CI spins up runners for each target, builds, creates installers, and publishes a GitHub Release.

#### GitHub Actions Integration

- `dist init` with `ci = ["github"]` auto-generates `.github/workflows/release.yml`
- Triggers on git tag push matching version patterns (e.g., `v*`)
- Setting `pr-run-mode = "upload"` also builds artifacts on PRs for pre-release validation
- Generated workflow handles all platform matrix logic automatically

---

### 2. GitHub Actions Cross-Compilation Patterns

#### Standard Five-Target Matrix

The consensus across popular Rust CLIs (ripgrep, bat, fd, starship, zoxide, delta):

```yaml
strategy:
  matrix:
    include:
      - target: x86_64-unknown-linux-musl
        os: ubuntu-latest
        use-cross: true
        archive: tar.gz
      - target: aarch64-unknown-linux-musl
        os: ubuntu-latest
        use-cross: true
        archive: tar.gz
      - target: x86_64-apple-darwin
        os: macos-latest
        use-cross: false
        archive: tar.gz
      - target: aarch64-apple-darwin
        os: macos-latest
        use-cross: false
        archive: tar.gz
      - target: x86_64-pc-windows-msvc
        os: windows-latest
        use-cross: false
        archive: zip
```

**Key conventions:**
- **Linux uses `musl`** for fully static binaries (no glibc dependency at runtime)
- **macOS builds run natively** on `macos-latest` (ARM64 runners since 2024)
- **Windows uses MSVC** toolchain (not GNU) for best compatibility
- **`cross` (cross-rs)** is used only for Linux targets; macOS/Windows compile natively

#### Cross-Compilation Tools

| Tool | Usage | Notes |
|------|-------|-------|
| `cross` (cross-rs) | Linux cross-compilation | Docker-based; pin version (e.g., v0.2.5) |
| `houseabsolute/actions-rust-cross` | GH Action wrapper | Auto-selects cross vs cargo per target |
| `taiki-e/install-action` | Tool installer | Used by starship to install cross |
| `cargo-zigbuild` | Alternative cross-compiler | Uses Zig as linker; lighter than Docker |

#### Release Triggers

```yaml
# Tag-based (most common)
on:
  push:
    tags: ['v[0-9]+.[0-9]+.[0-9]+']
  workflow_dispatch:  # manual re-runs
```

Ripgrep uses `[0-9]+.[0-9]+.[0-9]+` (no `v` prefix). Bat/fd use `v[0-9].*`. Starship uses release-please for automatic tag management.

#### Platform-Specific Dependencies (junction crate)

The `junction` crate in `libaipm` is already correctly gated:
```toml
[target.'cfg(windows)'.dependencies]
junction = { workspace = true }
```

When cross-compiling for Linux/macOS, Cargo evaluates `cfg(windows)` as false and skips `junction` entirely. No special CI configuration needed.

#### Artifact Naming Conventions

| Project  | Pattern | Example |
|----------|---------|---------|
| ripgrep  | `{name}-{version}-{target}.{ext}` | `ripgrep-14.1.1-x86_64-unknown-linux-musl.tar.gz` |
| bat      | `{name}-v{version}-{target}.{ext}` | `bat-v0.24.0-x86_64-unknown-linux-musl.tar.gz` |
| starship | `{name}-{target}.{ext}` | `starship-x86_64-unknown-linux-musl.tar.gz` |

**Archive contents** (standard pattern from ripgrep):
```
aipm-0.1.0-x86_64-unknown-linux-musl/
  aipm
  README.md
  LICENSE
  complete/
    aipm.bash
    aipm.fish
    _aipm          # zsh
    _aipm.ps1      # powershell
```

**Checksums**: Each archive gets a companion `.sha256` file:
```bash
shasum -a 256 "$ARCHIVE" > "$ARCHIVE.sha256"
```

---

### 3. Package Manager & Installer Integration

#### 3.1 cargo-binstall

Pre-built binary discovery for `cargo binstall aipm`. Add to each binary crate's `Cargo.toml`:

```toml
[package.metadata.binstall]
pkg-url = "{ repo }/releases/download/v{ version }/{ name }-{ target }-v{ version }.{ archive-format }"
bin-dir = "{ name }-{ target }-v{ version }/{ bin }{ binary-ext }"
pkg-fmt = "tgz"

[package.metadata.binstall.overrides.x86_64-pc-windows-msvc]
pkg-fmt = "zip"
```

Template variables: `{ repo }`, `{ name }`, `{ version }`, `{ target }`, `{ archive-format }`, `{ bin }`, `{ binary-ext }`

**Links**: https://github.com/cargo-bins/cargo-binstall

#### 3.2 Homebrew Tap

Create a repo `thelarkinn/homebrew-aipm` with formula files:

```ruby
# Formula/aipm.rb
class Aipm < Formula
  desc "AI Plugin Manager for MCP and agent skills"
  homepage "https://github.com/thelarkinn/aipm"
  version "0.1.0"
  license "MIT"

  if OS.mac?
    if Hardware::CPU.arm?
      url "https://github.com/thelarkinn/aipm/releases/download/v#{version}/aipm-aarch64-apple-darwin.tar.gz"
      sha256 "HASH_HERE"
    else
      url "https://github.com/thelarkinn/aipm/releases/download/v#{version}/aipm-x86_64-apple-darwin.tar.gz"
      sha256 "HASH_HERE"
    end
  elsif OS.linux?
    url "https://github.com/thelarkinn/aipm/releases/download/v#{version}/aipm-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "HASH_HERE"
  end

  def install
    bin.install "aipm"
  end

  test do
    system "#{bin}/aipm", "--version"
  end
end
```

Usage: `brew install thelarkinn/aipm/aipm`

cargo-dist auto-generates and publishes Homebrew formulae when `installers = ["homebrew"]` is set.

**Links**: https://docs.brew.sh/Taps

#### 3.3 Scoop Bucket (Windows)

Create a repo `thelarkinn/scoop-aipm` with JSON manifests:

```json
{
    "version": "0.1.0",
    "description": "AI Plugin Manager for MCP and agent skills",
    "homepage": "https://github.com/thelarkinn/aipm",
    "license": "MIT",
    "architecture": {
        "64bit": {
            "url": "https://github.com/thelarkinn/aipm/releases/download/v0.1.0/aipm-x86_64-pc-windows-msvc.zip",
            "hash": "SHA256_HASH_HERE",
            "bin": "aipm.exe"
        }
    },
    "checkver": "github",
    "autoupdate": {
        "architecture": {
            "64bit": {
                "url": "https://github.com/thelarkinn/aipm/releases/download/v$version/aipm-x86_64-pc-windows-msvc.zip"
            }
        }
    }
}
```

Usage: `scoop bucket add aipm https://github.com/thelarkinn/scoop-aipm && scoop install aipm`

#### 3.4 Shell Install Script (Unix)

cargo-dist auto-generates this. The pattern:
```bash
curl -sSf https://github.com/thelarkinn/aipm/releases/latest/download/aipm-installer.sh | sh
```

#### 3.5 PowerShell Install Script (Windows)

cargo-dist auto-generates this. The pattern:
```powershell
irm https://github.com/thelarkinn/aipm/releases/latest/download/aipm-installer.ps1 | iex
```

#### 3.6 WinGet

Publishing to Windows Package Manager requires submitting a manifest PR to https://github.com/microsoft/winget-pkgs. Can be automated with `wingetcreate` in CI.

#### 3.7 crates.io

Standard `cargo publish` for each workspace crate. Publish order matters: `libaipm` first, then `aipm` and `aipm-pack`.

#### 3.8 npm Wrapper

A thin npm package that downloads the platform-native binary on `postinstall`. Used by tools like biome and turbo. cargo-dist generates this with `installers = ["npm"]`.

---

### 4. Release Automation Tools Comparison

#### 4.1 release-plz (CI-First Automation)

**Repository**: https://github.com/release-plz/release-plz
**Docs**: https://release-plz.dev/

Automates version bumping, changelog generation, release PR creation, crates.io publishing, and git tag creation. Unique features:
- **cargo-semver-checks integration** — auto-detects API breaking changes
- **Zero config** — reads from Cargo.toml and the cargo registry
- **Git-cliff integration** — customizable changelog formatting

**GitHub Actions workflow:**
```yaml
name: Release-plz
on:
  push:
    branches: [main]

jobs:
  release-plz:
    runs-on: ubuntu-latest
    permissions:
      contents: write
      pull-requests: write
    steps:
      - uses: actions/checkout@v6
        with:
          fetch-depth: 0
          persist-credentials: false
      - uses: dtolnay/rust-toolchain@stable
      - uses: release-plz/action@v0.5
        with:
          command: release-pr
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
```

**Configuration (`release-plz.toml`):**
```toml
[workspace]
changelog_config = "cliff.toml"
changelog_update = true
git_tag_enable = true
git_release_enable = false  # Let cargo-dist handle GitHub Releases
semver_check = true
pr_labels = ["release"]
```

#### 4.2 cargo-release (CLI-First Manual)

**Repository**: https://github.com/crate-ci/cargo-release

CLI tool for local release workflows:
```bash
cargo release patch --execute       # bump, commit, tag, publish, push
cargo release 2.0.0 --execute       # specific version
cargo release --workspace --execute # all crates
```

Best for: small projects, infrequent releases, developers who prefer manual control.

#### 4.3 Comparison Matrix

| Feature | cargo-dist | release-plz | cargo-release |
|---------|-----------|-------------|---------------|
| Binary building | Yes (core feature) | No | No |
| Installer generation | Yes (shell, PS, brew, npm, msi) | No | No |
| GitHub Actions CI | Auto-generated | Provided template | No |
| Version bumping | No (delegates) | Yes (auto from commits) | Yes (manual) |
| Changelog generation | Reads existing | Yes (git-cliff) | No (hooks only) |
| crates.io publishing | Yes | Yes | Yes |
| Semver checking | No | Yes (cargo-semver-checks) | No |
| Release PRs | No | Yes (auto-created) | No |
| Git tagging | On release | Yes | Yes |
| Workspace support | Yes (per-binary) | Yes | Yes |
| Primary mode | CI | CI | CLI |

#### 4.4 Recommended Combination: release-plz + cargo-dist

The canonical pattern in the Rust ecosystem:

1. **release-plz** manages version bumping, changelogs, and creates release PRs
2. Merging the release PR creates a **git tag**
3. The git tag triggers **cargo-dist** which builds binaries, creates installers, and publishes the GitHub Release

```
commit → release-plz creates PR → merge PR → git tag → cargo-dist builds & publishes
```

Set `git_release_enable = false` in release-plz config so cargo-dist handles the GitHub Release.

#### 4.5 git-cliff (Changelog Generation)

**Repository**: https://github.com/orhun/git-cliff
**Docs**: https://git-cliff.org/

Generates changelogs from conventional commits. Integrates with release-plz via `cliff.toml`:

```toml
[changelog]
header = "# Changelog\n"
body = """
{% for group, commits in commits | group_by(attribute="group") %}
## {{ group | upper_first }}
{% for commit in commits %}
- {{ commit.message | upper_first }} ({{ commit.id | truncate(length=7, end="") }})
{% endfor %}
{% endfor %}
"""
trim = true

[git]
conventional_commits = true
commit_parsers = [
  { message = "^feat", group = "Features" },
  { message = "^fix", group = "Bug Fixes" },
  { message = "^doc", group = "Documentation" },
  { message = "^perf", group = "Performance" },
  { message = "^refactor", group = "Refactoring" },
  { message = "^test", group = "Testing" },
  { message = "^chore", group = "Miscellaneous" },
]
```

---

### 5. Signing and Provenance

#### GitHub Artifact Attestations (Recommended)

GitHub's built-in attestation system (GA since 2024) uses Sigstore under the hood:

```yaml
- uses: actions/attest-build-provenance@v2
  with:
    subject-path: 'target/dist/*'
```

Users verify with: `gh attestation verify ./aipm --owner thelarkinn`

#### cosign / Sigstore

Direct Sigstore integration for keyless signing:
```yaml
- uses: sigstore/cosign-installer@v3
- run: cosign sign-blob --yes --output-signature sig.pem artifact.tar.gz
```

---

### 6. Supply Chain & Security Considerations

| Concern | Tool/Pattern |
|---------|-------------|
| Security scanning | GitHub Dependabot + CodeQL |
| SBOM generation | `anchore/sbom-action` in CI |
| License compliance | `cargo-deny` for dependency license auditing |
| Artifact signing | GitHub Artifact Attestations (preferred) |
| Supply chain | Pin actions with SHA hashes, not tags |

---

## Code References

- `Cargo.toml:1-124` — Workspace root with lint config, dependencies, and metadata
- `crates/aipm/Cargo.toml` — Consumer CLI binary crate
- `crates/aipm-pack/Cargo.toml` — Author CLI binary crate
- `crates/libaipm/Cargo.toml:23` — Windows-only `junction` dependency with `cfg(windows)` gate

## Architecture Documentation

### Current State

- **No CI/CD exists** — no `.github/` directory
- **Two binary targets** — `aipm` and `aipm-pack` would each need independent release artifacts
- **One library** — `libaipm` needs crates.io publishing but no binary distribution
- **Windows-specific code** — `junction` dependency is already properly gated with `cfg(windows)`
- **Lockstep versioning** — all crates share `version = "0.1.0"` via `version.workspace = true`
- **Repository**: `https://github.com/thelarkinn/aipm`

### Recommended Release Architecture for aipm

```
┌──────────────────────────────────────────────────────┐
│ Developer pushes to main                              │
│                                                       │
│  release-plz detects changes → creates Release PR     │
│  (bumps version, updates CHANGELOG.md)                │
│                                                       │
│  Merge Release PR → git tag created (v0.1.0)          │
│                                                       │
│  cargo-dist triggers on tag:                          │
│  ├── Build linux-x86_64-musl   (ubuntu runner + cross)│
│  ├── Build linux-aarch64-musl  (ubuntu runner + cross)│
│  ├── Build macos-x86_64       (macos runner)          │
│  ├── Build macos-aarch64      (macos runner)          │
│  ├── Build windows-x86_64     (windows runner)        │
│  │                                                    │
│  ├── Generate shell installer                         │
│  ├── Generate powershell installer                    │
│  ├── Generate homebrew formula → push to tap repo     │
│  │                                                    │
│  ├── Publish to crates.io                             │
│  ├── Create GitHub Release with all artifacts         │
│  └── Attest artifacts (sigstore/GitHub attestations)  │
└──────────────────────────────────────────────────────┘
```

## Historical Context (from research/)

- `research/docs/2026-03-09-cargo-core-principles.md` — Cargo packaging principles
- `research/docs/2026-03-09-npm-core-principles.md` — npm distribution patterns (relevant for npm wrapper installer)
- `research/docs/2026-03-09-pnpm-core-principles.md` — pnpm patterns

## Related Research

- cargo-dist docs: https://axodotdev.github.io/cargo-dist/
- release-plz docs: https://release-plz.dev/
- cargo-release docs: https://github.com/crate-ci/cargo-release
- git-cliff docs: https://git-cliff.org/
- cargo-binstall: https://github.com/cargo-bins/cargo-binstall
- cross-rs: https://github.com/cross-rs/cross
- actions-rust-cross: https://github.com/houseabsolute/actions-rust-cross
- ripgrep CI: https://deepwiki.com/BurntSushi/ripgrep/3.4-release-process
- GitHub Artifact Attestations: https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations

## Open Questions

1. **Unified vs independent versioning**: Should `aipm` and `aipm-pack` release independently, or always together? Current lockstep setup suggests unified (`v0.1.0` tag releases both).
2. **musl vs gnu for Linux**: musl gives static binaries but may have issues with DNS resolution or locale. Test with aipm's HTTP dependencies (reqwest).
3. **Windows ARM64**: Should `aarch64-pc-windows-msvc` be a target? Growing market but cross-rs support is limited.
4. **WinGet publishing**: Worth the effort? Adds visibility on Windows.
5. **npm wrapper**: Relevant if targeting Node.js/AI developers who already use npm.
