---
date: 2026-04-22 16:14:50 UTC
researcher: Sean Larkin
git_commit: 5616fd4db5d41b77df55686365308cf12701af2a
branch: main
repository: aipm
topic: "Adding nuget.org publishing to the aipm CI/CD pipeline for consumption from Azure DevOps"
tags: [research, nuget, github-actions, azure-devops, ado, distribution, release-engineering, cargo-dist, release-plz, cross-compilation, multi-rid, oidc, trusted-publishing]
status: complete
last_updated: 2026-04-22
last_updated_by: Sean Larkin
last_updated_note: "Added follow-up verification of the cargo-nuget crates (KodrAus/cargo-nuget and rylev/cargo-nuget) to confirm neither fits aipm's use case."
---

# Research — Automatic NuGet publishing for aipm

## Research Question

> I need to add to my automatic ci/cd pipelines the ability to publish aipm to nuget (so that I can install and use it in an ADO pipeline). What will I need to do to setup nuget automatic publishing?

Refined scope: (1) what CI/CD exists now, (2) how to package the Rust-built `aipm` CLI as a multi-RID NuGet tool package, (3) how to publish it automatically from GitHub Actions to nuget.org, and (4) what ADO pipeline consumers need.

## Summary

**The short answer: add one new GitHub Actions workflow (`release-nuget.yml`) that runs after the existing cargo-dist `release.yml`, plus three new files in the repo (`packaging/aipm.nuspec`, `packaging/build/aipm.targets`, and optionally a `packaging/README-nuget.md`).** The existing cargo-dist + release-plz infrastructure already does the hard work of cross-compiling four targets and producing a GitHub Release on tag — the NuGet step downloads those release archives, re-lays them into `runtimes/<RID>/native/`, packs a `.nupkg`, and pushes to nuget.org using **NuGet Trusted Publishing (OIDC)** rather than long-lived API-key secrets.

The important constraints:

- **Current CI/CD is greenfield for NuGet.** No `.nuspec`, no NuGet workflow, no prior decisions — only a single mention of NuGet-as-category-of-GitHub-Packages in `research/docs/2026-03-19-cargo-dist-installer-github-releases.md:217-234`, which dismissed GitHub Packages for CLI binaries.
- **Current build matrix is four targets** (`dist-workspace.toml:15-20`): `x86_64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`. Shipping a seven-RID NuGet would require adding `aarch64-unknown-linux-{gnu,musl}`, `aarch64-pc-windows-msvc`, and `linux-musl-x64` — **or** shipping only the four existing RIDs first and expanding later. The codebase already notes `aarch64-unknown-linux` was "dropped to simplify adoption; can be added later" (`specs/2026-03-16-ci-cd-release-automation.md:87`).
- **`cargo-dist` v0.31.0 does not emit NuGet output.** The NuGet step must be hand-rolled as a separate job.
- **Trusted Publishing (OIDC)** went GA on nuget.org **2025-09-22**. This is the modern auth story — one-time policy setup on nuget.org, then zero long-lived secrets in GitHub. Fallback: `NUGET_API_KEY` repo/environment secret.
- **Package ID `aipm` is 4 chars** — the borderline case nuget.org warns against for prefix reservation. Claim it by publishing first. Prefix reservation is a later concern.
- **Package type:** omit `<packageTypes>` (defaults to `Dependency`). Do **not** use `DotnetTool` — that would force consumers to have the .NET SDK installed.
- **Consumer pattern:** ADO pipelines use `dotnet restore` against a tiny `NoTargets` csproj with `<PackageDownload Include="aipm" Version="[x.y.z]" />`, then resolve the per-RID binary from `$(NUGET_PACKAGES)/aipm/<version>/runtimes/<RID>/native/` using `Agent.OS`/`Agent.OSArchitecture`, then `##vso[task.prependpath]` to put it on PATH.

**Integration point into the existing release pipeline:** the new `release-nuget.yml` workflow triggers on `release: types: [published]` with a guard on `startsWith(github.event.release.tag_name, 'aipm-v') && !github.event.release.prerelease` — the same gating pattern already used by `update-latest-release.yml` (`.github/workflows/update-latest-release.yml:20`). It downloads the per-target archives from the GitHub Release created by `release.yml`, unpacks them into a `runtimes/` staging directory, packs, and pushes. Optionally, it can also re-build the binaries itself on a matrix (self-contained approach) — the `download-from-release` approach reuses existing artifacts and is cheaper.

---

## Detailed Findings

### 1. Current CI/CD inventory (evidence the path is greenfield for NuGet)

Six hand-written workflows and five compiled agentic workflows exist:

| Workflow | File | Trigger | Purpose |
|---|---|---|---|
| CI | [`.github/workflows/ci.yml`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/.github/workflows/ci.yml) | push/PR | `cargo build/test/clippy/fmt` + 89% branch coverage gate |
| CodeQL | [`.github/workflows/codeql.yml`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/.github/workflows/codeql.yml) | push/PR/weekly | Security scanning |
| Release (cargo-dist) | [`.github/workflows/release.yml`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/.github/workflows/release.yml) | tag push | Builds 4-target archives + installers, creates GitHub Release |
| Release (release-plz) | [`.github/workflows/release-plz.yml`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/.github/workflows/release-plz.yml) | push to main | Opens release PRs, publishes crates.io, creates tags |
| Update Latest | [`.github/workflows/update-latest-release.yml`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/.github/workflows/update-latest-release.yml) | release:published | Re-uploads installer scripts to a `latest` GitHub Release |
| Research Codebase | [`.github/workflows/research-codebase.yml`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/.github/workflows/research-codebase.yml) | issue labeled | Research agent |

Plus agentic `.md`/`.lock.yml` pairs: `improve-coverage`, `daily-qa`, `docs-updater`, `update-docs`, `build-timings`.

**Key facts for NuGet integration:**

- The CLI binary name is `aipm` ([`crates/aipm/Cargo.toml:2`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/crates/aipm/Cargo.toml#L2), verified by `clap::Parser` at `crates/aipm/src/main.rs:18`).
- Workspace version is `0.22.3` (lockstep across all member crates) in [`Cargo.toml:10`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/Cargo.toml#L10).
- Repository URL is `https://github.com/thelarkinn/aipm` ([`Cargo.toml:13`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/Cargo.toml#L13)), license MIT ([`Cargo.toml:12`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/Cargo.toml#L12)).
- Build targets (four) in [`dist-workspace.toml:15-20`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/dist-workspace.toml#L15-L20): `x86_64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`.
- Archive formats: `.tar.xz` on Unix, `.zip` on Windows ([`dist-workspace.toml:40-41`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/dist-workspace.toml#L40-L41)).
- Release tag pattern: `aipm-v<semver>` (release-plz with per-crate tags; confirmed by [`update-latest-release.yml:20`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/.github/workflows/update-latest-release.yml#L20) filter `startsWith(github.event.release.tag_name, 'aipm-v')`).
- **No existing NuGet artifacts.** `rg -i nuget` returned only a single informational mention in `research/docs/2026-03-19-cargo-dist-installer-github-releases.md:223`.
- **No `[package.metadata.dist]`, `[package.metadata.binstall]`, `[package.metadata.release]`** sections anywhere.
- **No `rust-toolchain.toml`** — each workflow selects toolchain explicitly.
- **No `.cargo/config.toml`, no `Cross.toml`** — cross-compilation is entirely cargo-dist's responsibility today.
- **Existing secrets inventory**: `GITHUB_TOKEN`, `CODECOV_TOKEN`, `RELEASE_PLZ_TOKEN`, `CARGO_REGISTRY_TOKEN`, plus agentic tokens. No `NUGET_API_KEY` or `NUGET_USERNAME` yet.

### 2. RID mapping between current targets and NuGet runtimes

| Current cargo-dist target | NuGet RID | Status |
|---|---|---|
| `x86_64-unknown-linux-gnu` | `linux-x64` | Built today |
| `x86_64-apple-darwin` | `osx-x64` | Built today |
| `aarch64-apple-darwin` | `osx-arm64` | Built today |
| `x86_64-pc-windows-msvc` | `win-x64` | Built today |
| `aarch64-unknown-linux-gnu` | `linux-arm64` | **Not built** — was deliberately dropped ([`specs/2026-03-16-ci-cd-release-automation.md:87`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/specs/2026-03-16-ci-cd-release-automation.md#L87)) |
| `x86_64-unknown-linux-musl` | `linux-musl-x64` | **Not built** — needs `cross-rs` workarounds per [cargo-dist issue #1581](https://github.com/axodotdev/cargo-dist/issues/1581); see [`research/docs/2026-03-19-cargo-dist-installer-github-releases.md:208-216`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/research/docs/2026-03-19-cargo-dist-installer-github-releases.md#L208-L216) |
| `aarch64-pc-windows-msvc` | `win-arm64` | **Not built** |

**Decision point:** ship the NuGet with only the four existing RIDs first (win-x64, linux-x64, osx-x64, osx-arm64), document the gap in the package description, and expand when the underlying cargo-dist matrix expands. This aligns with the prior spec decision to "drop to simplify adoption; can be added later".

### 3. Package layout & .nuspec

Omit `<packageTypes>` entirely so the package defaults to `Dependency` type — this is what `NuGetCommand@2 restore` and `dotnet restore` consume, and it does **not** require the .NET SDK at install time (unlike `DotnetTool`). See the dedicated packaging research doc: [`2026-04-22-nuget-native-multi-rid-packaging.md`](./2026-04-22-nuget-native-multi-rid-packaging.md).

**Proposed new file: `packaging/aipm.nuspec`**

```xml
<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://schemas.microsoft.com/packaging/2010/07/nuspec.xsd">
  <metadata>
    <id>aipm</id>
    <version>$version$</version>
    <authors>Sean Larkin</authors>
    <description>AI plugin manager. Manages AI plugins (Claude, Copilot, Cursor, etc.) across .claude/.github/.ai directories.</description>
    <license type="expression">MIT</license>
    <projectUrl>https://github.com/TheLarkInn/aipm</projectUrl>
    <repository type="git"
                url="https://github.com/TheLarkInn/aipm.git"
                branch="main"
                commit="$commit$" />
    <readme>docs\README.md</readme>
    <icon>images\icon.png</icon>
    <tags>ai claude copilot plugin-manager cli rust native cross-platform</tags>
    <copyright>Copyright (c) 2026 Sean Larkin</copyright>
    <!-- No <packageTypes> => defaults to Dependency -->
  </metadata>
  <files>
    <file src="runtimes\**" target="runtimes" />
    <file src="build\aipm.targets" target="build\" />
    <file src="README.md" target="docs\" />
    <file src="icon.png" target="images\" />
    <file src="LICENSE" target="" />
  </files>
</package>
```

**Proposed new file: `packaging/build/aipm.targets`** (MSBuild integration for consumers who want `$(AipmToolPath)`; see the packaging research doc section 2.5 for the full content).

### 4. New workflow: `release-nuget.yml`

Triggers off the existing cargo-dist GitHub Release (rather than re-running the whole matrix) to avoid duplicate cross-compilation. This mirrors how [`update-latest-release.yml`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/.github/workflows/update-latest-release.yml) already hooks the `release:published` event.

```yaml
name: Publish to NuGet
on:
  release:
    types: [published]

jobs:
  publish:
    if: startsWith(github.event.release.tag_name, 'aipm-v') && !github.event.release.prerelease
    runs-on: ubuntu-latest
    environment: release   # optional: gate behind environment approval
    permissions:
      contents: read
      id-token: write      # REQUIRED for Trusted Publishing (OIDC)
    steps:
      - uses: actions/checkout@v4

      - name: Extract version from tag
        id: ver
        run: |
          TAG="${{ github.event.release.tag_name }}"
          VERSION="${TAG#aipm-v}"
          echo "version=$VERSION" >> "$GITHUB_OUTPUT"

      - name: Download release archives
        env:
          GH_TOKEN: ${{ github.token }}
        run: |
          mkdir -p archives
          gh release download "${{ github.event.release.tag_name }}" \
            --pattern 'aipm-*-*.tar.xz' \
            --pattern 'aipm-*-*.zip' \
            --dir archives

      - name: Unpack archives into runtimes layout
        run: |
          mkdir -p pkg/runtimes pkg/build
          declare -A RID_MAP=(
            [x86_64-unknown-linux-gnu]=linux-x64
            [x86_64-apple-darwin]=osx-x64
            [aarch64-apple-darwin]=osx-arm64
            [x86_64-pc-windows-msvc]=win-x64
          )
          for triple in "${!RID_MAP[@]}"; do
            rid="${RID_MAP[$triple]}"
            mkdir -p "pkg/runtimes/$rid/native"
            if [[ "$triple" == *windows* ]]; then
              unzip -j "archives/aipm-${triple}.zip" 'aipm*/aipm.exe' -d "pkg/runtimes/$rid/native/"
            else
              tar -xf "archives/aipm-${triple}.tar.xz" --strip-components=1 -C /tmp
              install -m 755 /tmp/aipm "pkg/runtimes/$rid/native/aipm"
            fi
          done

      - name: Stage metadata files
        run: |
          cp packaging/build/aipm.targets pkg/build/
          cp README.md LICENSE pkg/
          cp packaging/icon.png pkg/ 2>/dev/null || true

      - uses: actions/setup-dotnet@v4
        with:
          dotnet-version: '8.x'
      - uses: nuget/setup-nuget@v2

      - name: Pack
        working-directory: pkg
        run: |
          nuget pack ../packaging/aipm.nuspec \
            -Version "${{ steps.ver.outputs.version }}" \
            -Properties "version=${{ steps.ver.outputs.version }};commit=${{ github.sha }}" \
            -NoDefaultExcludes \
            -OutputDirectory ../out

      - name: NuGet OIDC login
        id: nuget_login
        uses: NuGet/login@v1
        with:
          user: ${{ secrets.NUGET_USERNAME }}   # public profile name, not email

      - name: Push
        run: |
          dotnet nuget push out/*.nupkg \
            --api-key "${{ steps.nuget_login.outputs.NUGET_API_KEY }}" \
            --source https://api.nuget.org/v3/index.json \
            --skip-duplicate
```

Full derivation of this workflow (including the self-contained alternative that re-builds from source, Trusted Publishing policy setup, and fallback API-key path) is in [`2026-04-22-github-actions-nuget-publish.md`](./2026-04-22-github-actions-nuget-publish.md).

### 5. Trusted Publishing setup (one-time, manual)

Before the workflow can push for the first time:

1. Log in to nuget.org -> username dropdown -> **Trusted Publishing** -> **Add policy**.
2. Fields:
   - Repository Owner: `TheLarkInn`
   - Repository: `aipm`
   - Workflow File: `release-nuget.yml` (filename only — not the full path)
   - Environment: `release` (optional; matches the workflow's `environment:` key)
3. Add a repo secret `NUGET_USERNAME` with the nuget.org public profile handle (not the email).

No `NUGET_API_KEY` is needed with Trusted Publishing — the [`NuGet/login@v1` action](https://github.com/NuGet/login) uses GitHub's OIDC token to request a short-lived API key at each workflow run.

If Trusted Publishing is not yet available for the account, fall back to:
- Create an API key on nuget.org scoped to the glob `aipm*`.
- Store as `NUGET_API_KEY` repo/environment secret.
- Replace the `NuGet/login@v1` + `--api-key "${{ steps.nuget_login.outputs.NUGET_API_KEY }}"` lines with `--api-key "${{ secrets.NUGET_API_KEY }}"`.

### 6. ADO pipeline consumer example

The consumer-side research doc ([`2026-04-22-ado-pipeline-nuget-consume.md`](./2026-04-22-ado-pipeline-nuget-consume.md)) has the full YAML. Short form:

```yaml
# azure-pipelines.yml snippet
variables:
  AIPM_VERSION: '0.22.3'
  NUGET_PACKAGES: $(Pipeline.Workspace)/.nuget/packages

steps:
  - task: UseDotNet@2
    inputs: { packageType: sdk, version: 8.x }

  # Create a throwaway NoTargets csproj with <PackageDownload Include="aipm" Version="[x.y.z]" />
  # then: dotnet restore

  - pwsh: |
      $os = "$(Agent.OS)"; $arch = "$(Agent.OSArchitecture)".ToLowerInvariant()
      switch ($os) { 'Windows_NT' { $r='win'; $exe='aipm.exe' } 'Linux' { $r='linux'; $exe='aipm' } 'Darwin' { $r='osx'; $exe='aipm' } }
      $a = if ($arch -eq 'arm64') { 'arm64' } else { 'x64' }
      $rid = "$r-$a"
      $bin = "$env:NUGET_PACKAGES/aipm/$env:AIPM_VERSION/runtimes/$rid/native"
      if ($os -ne 'Windows_NT') { chmod +x "$bin/$exe" }
      Write-Host "##vso[task.prependpath]$bin"
    displayName: 'Prepend aipm to PATH'

  - script: aipm --version
```

`NuGetAuthenticate@1` is **not** needed for public nuget.org — it only applies to authenticated feeds like Azure Artifacts.

### 7. Integration with existing release-plz / cargo-dist flow

The release pipeline today is:

1. Commits to `main` -> `release-plz.yml` opens a "Release PR" bumping versions & updating CHANGELOGs.
2. PR merged -> `release-plz.yml` creates git tags (e.g., `aipm-v0.22.4`) and publishes each crate to crates.io.
3. Tag push (`**[0-9]+.[0-9]+.[0-9]+*`) -> `release.yml` (cargo-dist) builds the 4-target matrix and creates a GitHub Release with archives + installers.
4. Release published -> `update-latest-release.yml` updates the `latest` GitHub Release's installer scripts.

**Proposed insertion:** step 5 -> `release-nuget.yml` listens to the same `release:published` event, downloads the per-target archives, packs the .nupkg, publishes to nuget.org.

This keeps **zero changes** to `release.yml`, `release-plz.yml`, and `dist-workspace.toml` in v1. If later you want self-contained builds (e.g., adding ARM64 Linux or musl without waiting for cargo-dist to support them), a second workflow variant can run its own matrix build independent of cargo-dist.

### 8. Observed limitations and risks

| Risk | Impact | Mitigation |
|---|---|---|
| cargo-dist 0.31.0 ships only 4 RIDs | Missing `linux-arm64`, `win-arm64`, `linux-musl-x64` in the NuGet | Ship 4 RIDs first; add more when dist matrix expands or run a parallel build job |
| `cargo-dist` will never emit NuGet natively (as of 2026-04) | Hand-rolled workflow forever | Low — the workflow is ~80 lines of YAML |
| Trusted Publishing still rolling out | Account may not have access yet | Fall back to `NUGET_API_KEY` secret; switch later |
| Package ID `aipm` is 4 chars | Can't reserve prefix, someone could squat satellite IDs | Claim by publishing first; reserve prefix once you ship `aipm.*` satellites |
| No Rust-CLI precedent on nuget.org | First-mover uncertainty | Test with `*-alpha` version before cutting `1.0.0` |
| ADO `NuGetCommand@2` is in maintenance mode | Long-term consumer task may break | Document `NuGetAuthenticate@1` + `dotnet restore` as the consumer pattern |
| nuget.org 250 MB limit | 7 RIDs x ~5 MB = well under cap | Monitor; split per-RID if aipm binaries bloat |
| `nuget sign` is Windows-only | Can't do author-signing from Linux runner | Rely on nuget.org repository signature (automatic on ingest) |
| Symbol packages can break `--skip-duplicate` | Workflow re-runs fail | Don't push `.snupkg` for native packages |

## Code References

- [`Cargo.toml:10`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/Cargo.toml#L10) — workspace version (lockstep `0.22.3`)
- [`Cargo.toml:13`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/Cargo.toml#L13) — repository URL used in `.nuspec` `<projectUrl>`
- [`Cargo.toml:188-193`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/Cargo.toml#L188-L193) — `[profile.dist]` used by cargo-dist
- [`crates/aipm/Cargo.toml:2`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/crates/aipm/Cargo.toml#L2) — binary name `aipm`
- [`crates/aipm/src/main.rs:18`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/crates/aipm/src/main.rs#L18) — `clap::Parser` with `name = "aipm"`
- [`dist-workspace.toml:15-20`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/dist-workspace.toml#L15-L20) — current 4-target matrix
- [`dist-workspace.toml:40-41`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/dist-workspace.toml#L40-L41) — archive format selection
- [`.github/workflows/release.yml`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/.github/workflows/release.yml) — cargo-dist release orchestration
- [`.github/workflows/release-plz.yml`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/.github/workflows/release-plz.yml) — crates.io + tag creation
- [`.github/workflows/update-latest-release.yml:20`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/.github/workflows/update-latest-release.yml#L20) — the `startsWith(tag_name, 'aipm-v')` guard pattern the new workflow should copy
- [`release-plz.toml`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/release-plz.toml) — release-plz per-crate tag strategy

## Architecture Documentation

### Current release flow (pre-NuGet)

```text
commit to main
    |
    v
release-plz.yml (Release PR) ---> opens PR with version bump + changelog
    |
    v
PR merged
    |
    v
release-plz.yml (Publish & Tag) ---> cargo publish + git tag aipm-v<semver>
    |
    v
release.yml (cargo-dist) ---> builds 4-target matrix + GitHub Release
    |
    v
update-latest-release.yml ---> updates "latest" GitHub Release with installer scripts
```

### Target release flow (post-NuGet)

```text
... (same as above, ending at release.yml creating a GitHub Release)
    |
    v
release-nuget.yml [NEW] ---> downloads archives from Release, packs .nupkg, pushes to nuget.org via OIDC
    |
    v
(in parallel) update-latest-release.yml ---> unchanged
```

Both `release-nuget.yml` and `update-latest-release.yml` fire on the same `release:published` event and are independent of each other.

## Historical Context (from research/)

- [`research/docs/2026-03-16-rust-cross-platform-release-distribution.md`](./2026-03-16-rust-cross-platform-release-distribution.md) — foundational survey of Rust CLI distribution channels (cargo-dist, cargo-binstall, Homebrew, Scoop, GH Actions matrices). Establishes the baseline matrix and channel decisions the NuGet path must slot into.
- [`research/docs/2026-03-19-cargo-dist-installer-github-releases.md`](./2026-03-19-cargo-dist-installer-github-releases.md) — cargo-dist adoption rationale; lines 217-234 explicitly dismiss GitHub Packages for CLI binaries (no native format), which is why nuget.org is the right target for this research rather than ghcr.io.
- [`research/docs/2026-03-19-cargo-dist-installer-github-releases.md:208-216`](./2026-03-19-cargo-dist-installer-github-releases.md) — documents why `aarch64-unknown-linux-musl` was skipped (requires manual cargo-dist workarounds), informing the initial 4-RID NuGet scope.
- [`research/docs/2026-03-20-changelog-generation-investigation.md`](./2026-03-20-changelog-generation-investigation.md) — release-plz / git-cliff setup that drives the `aipm-v<semver>` tag pattern the NuGet workflow triggers on.
- [`research/docs/2026-04-20-azure-devops-lint-reporter-parity.md`](./2026-04-20-azure-devops-lint-reporter-parity.md) — prior ADO integration work (`aipm lint --reporter ci-azure`). Establishes that aipm already targets ADO pipelines as consumers, making NuGet distribution a natural next step.
- [`specs/2026-03-16-ci-cd-release-automation.md:87`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/specs/2026-03-16-ci-cd-release-automation.md#L87) — "aarch64-unknown-linux (ARM64 Linux) — dropped to simplify adoption; can be added later".
- [`specs/2026-03-19-cargo-dist-installers.md`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/specs/2026-03-19-cargo-dist-installers.md) — canonical distribution spec any NuGet extension must honor.

**No prior NuGet-specific ticket, spec, or note exists.** This is greenfield for NuGet and ADO Artifacts.

## Related Research

- [`2026-04-22-nuget-native-multi-rid-packaging.md`](./2026-04-22-nuget-native-multi-rid-packaging.md) — full `.nuspec` schema, `runtimes/<RID>/native/` conventions, package-type choice, build/pack mechanics.
- [`2026-04-22-github-actions-nuget-publish.md`](./2026-04-22-github-actions-nuget-publish.md) — cross-compilation matrix alternatives, artifact aggregation, Trusted Publishing (OIDC), versioning approaches.
- [`2026-04-22-ado-pipeline-nuget-consume.md`](./2026-04-22-ado-pipeline-nuget-consume.md) — ADO YAML consumer pattern with `<PackageDownload>`, RID resolution from `Agent.OS`/`Agent.OSArchitecture`, `##vso[task.prependpath]` plumbing.

## Open Questions

1. **Initial RID scope.** Ship with the existing 4 RIDs (win-x64, linux-x64, osx-x64, osx-arm64) or block on expanding cargo-dist's matrix to 7? Recommendation: start with 4 and document the gap in the package description.
2. **Trusted Publishing vs. classic API key.** Is the nuget.org account already enrolled in Trusted Publishing? If not, does enrolling require waiting on support, or can a fallback API key ship v1?
3. **Package ID ownership.** Who owns the nuget.org account that will publish `aipm`? If it's a personal account, migrating ownership later is possible but friction-ful.
4. **`environment: release` gate.** Should the NuGet publish require manual environment approval (GitHub Environments), matching how crates.io publish works today? Or fully automated like `update-latest-release.yml`?
5. **`.nupkg` size.** Once built, confirm the 4-RID package stays well under 50 MB so we have headroom for adding 3 more RIDs.
6. **MSBuild `build/aipm.targets` depth.** Do we want the full `$(AipmToolPath)` + PATH-prepend targets (adds ~30 lines of MSBuild) or just the bare `runtimes/<RID>/native/` layout (ADO pipelines resolve the RID themselves)? The targets file is more consumer-friendly but only meaningful for MSBuild-based consumers.
7. **Versioning on pre-releases.** Do we want to push `aipm 0.22.4-alpha.1` to nuget.org from `aipm-v0.22.4-alpha.1` tags, or gate NuGet pushes on stable-only with the `!github.event.release.prerelease` condition?
8. **Symbol package / debug info.** We strip symbols in `[profile.release]` ([`Cargo.toml:185`](https://github.com/TheLarkInn/aipm/blob/5616fd4db5d41b77df55686365308cf12701af2a/Cargo.toml#L185)) so `.snupkg` is moot — confirm we don't want to change this for consumers.
9. **Fallback consumer path.** Should the package ID also exist as a `dotnet tool`-style nupkg (a separate `aipm.Tool` ID perhaps), for consumers who want `dotnet tool install -g aipm`? Or is the `PackageDownload`-from-pipeline pattern sufficient?

---

## Follow-up Research 2026-04-22 — `cargo-nuget` crate verification

Verified whether either of the two projects named `cargo-nuget` could replace or simplify the hand-rolled workflow.

### `KodrAus/cargo-nuget`

- Purpose: packages Rust **native libraries** (`cdylib`: `.dll`/`.so`/`.dylib`) as NuGet packages for **P/Invoke consumption from .NET code** via `DllImport`.
- Has a `cargo-nuget cross` subcommand supporting targets `win-x64`, `linux-x64`, `osx-x64`.
- Flags: `pack`, `cross`, `--test`, `--cargo-dir`, `--nupkg-dir`, `--release`, `--targets`.
- Last release: v0.1.0 on **2017-11-25**. References Rust 1.18.0 and .NET SDK 2.0.0. **Dormant ~8 years.**
- Produces packages for "local feed" use — **no nuget.org publish support**.
- **Does not use the `runtimes/<RID>/native/` layout** that modern NuGet asset selection and ADO `dotnet restore` expect.

**Verdict: wrong target (libraries, not binaries) and unmaintained.** Not usable for aipm.

### `rylev/cargo-nuget`

- Purpose: the **inverse direction** — lets a Rust project *consume* NuGet packages (designed for WinRT-rs interop).
- Usage: `cargo nuget install` fetches NuGet deps declared in `Cargo.toml` metadata.
- Only 9 commits, no releases. Status unclear.

**Verdict: solves the opposite problem.** Not usable for aipm.

### Conclusion

The main-synthesis recommendation stands unchanged: there is no off-the-shelf Rust-CLI-to-NuGet tooling in 2026, and the hand-rolled `release-nuget.yml` workflow in section 4 remains the right path. cargo-dist still does not emit NuGet output (v0.31.0), and the only adjacent Rust ecosystem tooling (`cargo-nuget`) addresses cdylib P/Invoke rather than CLI binary distribution.

### Sources

- [KodrAus/cargo-nuget on GitHub](https://github.com/KodrAus/cargo-nuget)
- [cargo-nuget on crates.io](https://crates.io/crates/cargo-nuget)
- [cargo-nuget on docs.rs (v0.1.0 readme)](https://docs.rs/crate/cargo-nuget/latest/source/README.md)
- [rylev/cargo-nuget on GitHub](https://github.com/rylev/cargo-nuget)
