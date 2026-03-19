---
date: 2026-03-19 11:15:36 PDT
researcher: Claude Opus 4.6
git_commit: 5296793e62a55965319974cc0549bce8734b64be
branch: main
repository: aipm
topic: "cargo-dist integration for automatic installers, GitHub Releases publishing, and GitHub Packages"
tags: [research, cargo-dist, installers, github-releases, github-packages, release-plz, ci-cd, cross-platform]
status: complete
last_updated: 2026-03-19
last_updated_by: Claude Opus 4.6
---

# Research: cargo-dist Integration for Automatic Installers & GitHub Releases

## Research Question

The `aipm` and `aipm-pack` binaries need automatic installers for the full support matrix (Windows/Linux/macOS), with publishes automatically appearing under GitHub Releases (and GitHub Packages if relevant). The user is open to cargo-dist replacing the current hand-rolled `release.yml`.

## Summary

The workspace already has a functional release pipeline: `release-plz.yml` creates release PRs and git tags, and a hand-rolled `release.yml` builds 5-target binaries and uploads them to **draft** GitHub Releases (which are never finalized). cargo-dist can replace `release.yml` to add shell/PowerShell installer scripts, Homebrew tap support, and automatic GitHub Release publishing. GitHub Packages is not relevant for CLI binary distribution — GitHub Releases is the standard channel, and cargo-dist handles it natively.

---

## Detailed Findings

### 1. Current Release Pipeline State

#### release-plz.yml ([`.github/workflows/release-plz.yml`](https://github.com/TheLarkInn/aipm/blob/5296793e62a55965319974cc0549bce8734b64be/.github/workflows/release-plz.yml))

Two-job workflow triggered on push to main:

| Job | What it does |
|-----|-------------|
| `release-pr` (L8-39) | Runs `release-plz release-pr`, creates/updates a version bump PR with CHANGELOG, enables auto-merge via `gh pr merge --auto --squash` |
| `release` (L45-67) | Runs `release-plz release`, publishes to crates.io and creates git tags (e.g., `v0.1.0`) |

Uses `RELEASE_PLZ_TOKEN` (a PAT) — this is critical because tags created with `GITHUB_TOKEN` do not trigger downstream `on: push: tags` workflows.

#### release-plz.toml ([`release-plz.toml`](https://github.com/TheLarkInn/aipm/blob/5296793e62a55965319974cc0549bce8734b64be/release-plz.toml))

Key settings:
- `git_tag_enable = true` — creates version tags that trigger the release workflow
- `git_release_enable = false` — delegates GitHub Release creation to the downstream workflow
- `semver_check = true` — uses cargo-semver-checks for automatic version bump calculation
- `pr_draft = false` — release PRs are non-draft (eligible for auto-merge)

#### release.yml ([`.github/workflows/release.yml`](https://github.com/TheLarkInn/aipm/blob/5296793e62a55965319974cc0549bce8734b64be/.github/workflows/release.yml))

Tag-triggered workflow (`v[0-9]+.[0-9]+.[0-9]+`) with two jobs:

| Job | What it does |
|-----|-------------|
| `create-release` (L22-37) | Creates a **draft** GitHub Release via `gh release create --draft` |
| `build-release` (L40-148) | Builds for 5 targets, packages archives with SHA256 checksums, uploads to the draft release, attests build provenance |

**Build matrix** (L46-70):

| Target | OS | Cross | Archive |
|--------|----|-------|---------|
| `x86_64-unknown-linux-musl` | ubuntu-latest | `cross` v0.2.5 | tar.gz |
| `aarch64-unknown-linux-musl` | ubuntu-latest | `cross` v0.2.5 | tar.gz |
| `x86_64-apple-darwin` | macos-latest | native | tar.gz |
| `aarch64-apple-darwin` | macos-latest | native | tar.gz |
| `x86_64-pc-windows-msvc` | windows-latest | native | zip |

**Gap identified**: The draft release is never published. There is no `finalize-release` job that runs `gh release edit --draft=false` after all builds complete.

#### ci.yml ([`.github/workflows/ci.yml`](https://github.com/TheLarkInn/aipm/blob/5296793e62a55965319974cc0549bce8734b64be/.github/workflows/ci.yml))

Standard CI: build, test, clippy, fmt. Not directly relevant to release distribution.

### 2. Workspace Structure

| Crate | Type | Binary Name | Notes |
|-------|------|-------------|-------|
| `aipm` | binary | `aipm` | Consumer CLI (install, validate, doctor, link) |
| `aipm-pack` | binary | `aipm-pack` | Author CLI (init, pack, publish, yank, login) |
| `libaipm` | library | n/a | Core library; skipped by cargo-dist automatically |

All crates share `version = "0.1.0"` via workspace inheritance. Repository: `https://github.com/thelarkinn/aipm`. License: MIT.

No existing dist configuration (`dist-workspace.toml`, `dist.toml`, or `[workspace.metadata.dist]`) exists in the workspace.

### 3. cargo-dist — What It Provides

**Repository**: https://github.com/axodotdev/cargo-dist (~2,000+ stars)
**Docs**: https://axodotdev.github.io/cargo-dist/
**Latest version**: v0.31.0

cargo-dist is the Rust ecosystem's leading all-in-one release automation tool. It generates:

| Artifact | Description |
|----------|-------------|
| Platform archives | `.tar.xz` (Unix) / `.zip` (Windows) with binaries + README + LICENSE |
| Shell installer | `aipm-installer.sh` — detects platform, downloads correct archive, installs to `~/.cargo/bin` |
| PowerShell installer | `aipm-installer.ps1` — downloads archive, installs, updates PATH via registry |
| Homebrew formula | Auto-generated, pushed to a `homebrew-tap` repo |
| SHA256 checksums | Per-archive, embedded in installer scripts for verification |
| GitHub Release | Created and published automatically (not draft) |
| Build provenance | Optional GitHub Artifact Attestations |

#### Installer User Experience

**Shell (Linux/macOS)**:
```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/thelarkinn/aipm/releases/latest/download/aipm-installer.sh | sh
```

**PowerShell (Windows)**:
```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/thelarkinn/aipm/releases/latest/download/aipm-installer.ps1 | iex"
```

**Homebrew (macOS/Linux)**:
```bash
brew install thelarkinn/tap/aipm
```

Both installer scripts are uploaded as release artifacts alongside the platform archives. They are self-contained and fetch the correct binary for the user's platform/architecture.

### 4. cargo-dist + release-plz Integration

The current `release-plz.toml` is already configured for cargo-dist integration:
- `git_tag_enable = true` — creates tags that trigger cargo-dist
- `git_release_enable = false` — lets cargo-dist create the GitHub Release

The end-to-end pipeline becomes:

```
push to main
    │
    ▼
release-plz.yml (existing, unchanged)
    ├── Job 1: release-pr → Creates/updates version bump PR
    └── Job 2: release    → Publishes to crates.io + creates git tag
    │
    ▼  (tag push triggers)
    │
release.yml (cargo-dist generated, replaces current hand-rolled version)
    ├── plan     → Determines what to build based on tag
    ├── build    → Builds binaries for each target platform
    ├── host     → Uploads artifacts to GitHub Release (published, not draft)
    └── announce → Finalizes release, publishes Homebrew formula
```

**PAT requirement**: Already satisfied — `RELEASE_PLZ_TOKEN` creates tags that trigger the downstream `release.yml`.

### 5. cargo-dist Setup Process

Running `cargo dist init` (or `dist init`) in the workspace root:

1. **Interactive prompts** for targets, installers, CI backend
2. **Creates** either `dist-workspace.toml` (preferred) or `[workspace.metadata.dist]` in `Cargo.toml`
3. **Adds** `[profile.dist]` inheriting from `[profile.release]` to `Cargo.toml`
4. **Generates** `.github/workflows/release.yml` (replaces the current one)

Example `dist-workspace.toml` for aipm:

```toml
[workspace]
members = ["cargo:."]

[dist]
cargo-dist-version = "0.31.0"
ci = "github"
installers = ["shell", "powershell", "homebrew"]
targets = [
  "x86_64-apple-darwin",
  "aarch64-apple-darwin",
  "x86_64-unknown-linux-gnu",
  "x86_64-pc-windows-msvc",
]
tap = "thelarkinn/homebrew-tap"
publish-jobs = ["homebrew"]
pr-run-mode = "plan"
install-path = "CARGO_HOME"
github-attestations = true
dispatch-releases = true
```

`[profile.dist]` would layer on top of the existing `[profile.release]` (which already has `lto = "thin"`, `codegen-units = 1`, `strip = "symbols"`).

### 6. Key Configuration Options

| Key | Default | Notes |
|-----|---------|-------|
| `cargo-dist-version` | required | Pin to installed version (e.g., `"0.31.0"`) |
| `ci` | `[]` | `"github"` for GitHub Actions |
| `targets` | none | List of Rust target triples |
| `installers` | `[]` | `["shell", "powershell", "homebrew"]` |
| `tap` | none | `"thelarkinn/homebrew-tap"` — requires creating this repo |
| `publish-jobs` | `[]` | `["homebrew"]` to push formula on release |
| `pr-run-mode` | `"plan"` | `"plan"` = only planning on PRs; `"upload"` = full build on PRs |
| `github-attestations` | `false` | Set `true` to match current attestation behavior |
| `dispatch-releases` | `false` | Set `true` to enable manual `workflow_dispatch` (current release.yml has this) |
| `unix-archive` | `".tar.xz"` | Current setup uses `.tar.gz`; configurable |
| `windows-archive` | `".zip"` | Matches current setup |
| `checksum` | `"sha256"` | Matches current `.sha256` files |
| `install-path` | `"CARGO_HOME"` | Installs to `~/.cargo/bin` |
| `install-updater` | `false` | Optional self-update binary |
| `allow-dirty` | `[]` | Set `["ci"]` if you hand-edit the generated workflow |

### 7. musl Cross-Compilation Gap

The current `release.yml` builds `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl` using `cross-rs`. cargo-dist does **not** natively support musl targets or `cross-rs` integration. This is tracked in:

- [cargo-dist issue #74](https://github.com/axodotdev/cargo-dist/issues/74) — cross-compilation support (partially open)
- [cargo-dist issue #1581](https://github.com/axodotdev/cargo-dist/issues/1581) — `aarch64-unknown-linux-musl` requires manual workarounds

**Options**:
- **Drop musl**: Use `x86_64-unknown-linux-gnu` instead. Simpler but produces dynamically-linked binaries (requires glibc at runtime).
- **Keep musl with `allow-dirty`**: Let cargo-dist generate the base workflow, then hand-edit it to add musl targets using `cross-rs`. Set `allow-dirty = ["ci"]` so `dist init` doesn't overwrite your edits.
- **`github-build-setup`**: cargo-dist supports a `github-build-setup` step for custom pre-build actions. This could install `cross` but requires careful configuration.

### 8. GitHub Packages — Not Relevant for CLI Binaries

GitHub Packages (`ghcr.io`) is designed for:
- Container images (Docker/OCI)
- npm packages
- Maven/Gradle packages
- NuGet packages
- RubyGems

It does **not** have a native format for standalone CLI binaries. cargo-dist does not support GitHub Packages.

**For Rust CLI binary distribution, GitHub Releases is the standard and recommended channel.** This is what ripgrep, bat, fd, starship, zoxide, and every major Rust CLI uses. GitHub Releases:
- Hosts downloadable archives directly
- Supports installer scripts as release assets
- Integrates with `cargo-binstall` for `cargo binstall aipm`
- Provides permanent download URLs (e.g., `/releases/latest/download/...`)

If container-based distribution were needed (e.g., for CI environments), a separate workflow could push to `ghcr.io`, but this is not the standard pattern for CLI tools.

### 9. What cargo-dist Replaces vs. What Stays

| Component | Action |
|-----------|--------|
| `release-plz.yml` | **Keep unchanged** — continues to create release PRs, publish to crates.io, create tags |
| `release-plz.toml` | **Keep unchanged** — `git_release_enable = false` is the correct setting |
| `release.yml` | **Replace entirely** with cargo-dist generated workflow |
| `ci.yml` | **Keep unchanged** — unrelated to release distribution |
| `Cargo.toml` | **Add** `[profile.dist]` section |
| `dist-workspace.toml` | **New file** — cargo-dist configuration |
| Homebrew tap repo | **New repo** — `thelarkinn/homebrew-tap` (required for Homebrew installer) |

### 10. Workspace Behavior with Two Binaries

cargo-dist treats each package with binaries as an independent "App":
- `aipm` and `aipm-pack` each get their own archives and installers
- `libaipm` is automatically skipped (no binaries)
- A unified tag (`v0.1.0`) releases both binaries at the same version (matches the current lockstep versioning)
- Each binary gets its own installer script: `aipm-installer.sh`, `aipm-pack-installer.sh`, etc.

---

## Code References

- [`.github/workflows/release-plz.yml`](https://github.com/TheLarkInn/aipm/blob/5296793e62a55965319974cc0549bce8734b64be/.github/workflows/release-plz.yml) — Release PR creation and crates.io publishing
- [`.github/workflows/release.yml`](https://github.com/TheLarkInn/aipm/blob/5296793e62a55965319974cc0549bce8734b64be/.github/workflows/release.yml) — Current hand-rolled binary build matrix (to be replaced)
- [`.github/workflows/ci.yml`](https://github.com/TheLarkInn/aipm/blob/5296793e62a55965319974cc0549bce8734b64be/.github/workflows/ci.yml) — Standard CI (build, test, clippy, fmt)
- [`release-plz.toml`](https://github.com/TheLarkInn/aipm/blob/5296793e62a55965319974cc0549bce8734b64be/release-plz.toml) — release-plz configuration
- [`Cargo.toml:129-134`](https://github.com/TheLarkInn/aipm/blob/5296793e62a55965319974cc0549bce8734b64be/Cargo.toml#L129-L134) — Current `[profile.release]` settings

## Architecture Documentation

### Current Release Flow

```
push to main
    │
    ▼
release-plz.yml
    ├── release-pr job → Creates version bump PR (auto-merge enabled)
    └── release job    → Publishes to crates.io + creates git tag (v0.1.0)
    │
    ▼  (tag triggers)
    │
release.yml (hand-rolled)
    ├── create-release → Creates DRAFT GitHub Release
    └── build-release  → 5-target matrix, uploads archives + SHA256
         └── (draft never published — gap)
```

### Target Release Flow (with cargo-dist)

```
push to main
    │
    ▼
release-plz.yml (unchanged)
    ├── release-pr job → Creates version bump PR (auto-merge enabled)
    └── release job    → Publishes to crates.io + creates git tag (v0.1.0)
    │
    ▼  (tag triggers)
    │
release.yml (cargo-dist generated)
    ├── plan     → Determines apps to build
    ├── build    → Multi-platform matrix (macOS, Linux, Windows)
    ├── host     → Creates PUBLISHED GitHub Release with all artifacts:
    │               ├── aipm-{version}-{target}.tar.xz / .zip
    │               ├── aipm-pack-{version}-{target}.tar.xz / .zip
    │               ├── aipm-installer.sh
    │               ├── aipm-installer.ps1
    │               ├── aipm-pack-installer.sh
    │               ├── aipm-pack-installer.ps1
    │               └── SHA256 checksums
    └── announce → Publishes Homebrew formula to thelarkinn/homebrew-tap
```

## Historical Context (from research/)

- [`research/docs/2026-03-16-rust-cross-platform-release-distribution.md`](https://github.com/TheLarkInn/aipm/blob/5296793e62a55965319974cc0549bce8734b64be/research/docs/2026-03-16-rust-cross-platform-release-distribution.md) — Prior comprehensive research on cross-platform distribution patterns, cargo-dist capabilities, package manager integration, and signing/provenance. This document builds on that research with specific integration guidance.

## Related Research

- cargo-dist docs: https://axodotdev.github.io/cargo-dist/
- cargo-dist quickstart: https://axodotdev.github.io/cargo-dist/book/quickstart/rust.html
- cargo-dist config reference: https://opensource.axo.dev/cargo-dist/book/reference/config.html
- cargo-dist workspace guide: https://axodotdev.github.io/cargo-dist/book/workspaces/workspace-guide.html
- Shell installer docs: https://axodotdev.github.io/cargo-dist/book/installers/shell.html
- PowerShell installer docs: https://axodotdev.github.io/cargo-dist/book/installers/powershell.html
- Homebrew installer docs: https://axodotdev.github.io/cargo-dist/book/installers/homebrew.html
- Orhun's blog (release-plz + cargo-dist): https://blog.orhun.dev/automated-rust-releases/
- release-plz docs: https://release-plz.dev/
- cross-compilation issue #74: https://github.com/axodotdev/cargo-dist/issues/74
- musl issue #1581: https://github.com/axodotdev/cargo-dist/issues/1581

## Open Questions

1. **musl vs gnu for Linux targets**: The current `release.yml` uses musl for fully static binaries. cargo-dist does not natively support musl. Should musl be kept (requires `allow-dirty = ["ci"]` and hand-editing the generated workflow) or is gnu acceptable?
2. **aarch64-unknown-linux**: cargo-dist does not natively support `aarch64-unknown-linux-musl` or `aarch64-unknown-linux-gnu`. Dropping this target simplifies adoption; keeping it requires `allow-dirty` workarounds.
3. **Homebrew tap setup**: A new repository `thelarkinn/homebrew-tap` needs to be created, and a `HOMEBREW_TAP_TOKEN` secret needs to be added to the aipm repo for cargo-dist to push formula updates.
4. **Archive format**: Current setup uses `.tar.gz`; cargo-dist defaults to `.tar.xz` (better compression). Which to use?
5. **`cargo-binstall` metadata**: Adding `[package.metadata.binstall]` to each binary crate's `Cargo.toml` would enable `cargo binstall aipm`. cargo-dist archives are compatible with cargo-binstall's auto-detection.
6. **Self-updater**: cargo-dist offers an optional `install-updater = true` that generates a self-update binary. Relevant if users should be able to run `aipm self-update`.
