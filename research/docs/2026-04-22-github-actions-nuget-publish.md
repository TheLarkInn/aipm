---
date: 2026-04-22 16:14:50 UTC
researcher: Sean Larkin
git_commit: 5616fd4db5d41b77df55686365308cf12701af2a
branch: main
repository: aipm
topic: "End-to-end GitHub Actions pipeline for publishing a Rust CLI to nuget.org on git tag"
tags: [research, github-actions, nuget, cross-compilation, oidc, trusted-publishing, cargo-zigbuild, release-engineering, rust]
status: complete
last_updated: 2026-04-22
last_updated_by: Sean Larkin
---

# GitHub Actions -> nuget.org publishing for a Rust CLI

## Summary

The pipeline breaks into five stages:

1. **Matrix-based cross-compilation** on native runners (preferred) or `cargo-zigbuild`/`cross` from a single Linux runner.
2. **Upload per-target binaries** as artifacts; fan-in to a single packaging job that lays files out under `runtimes/<RID>/native/`.
3. **Pack** with `dotnet pack` (preferred, .NET 10+ supports `-p:NuspecFile`) or `nuget pack <nuspec>`.
4. **Publish** with `dotnet nuget push` using **NuGet Trusted Publishing (OIDC)**, GA since **September 22, 2025**. No long-lived API keys required.
5. **Derive the package version** from `Cargo.toml`/git tag using `SebRollen/toml-action` or `cargo-get`.

The major 2025-2026 development: **Trusted Publishing (OIDC)** removes the NuGet API-key-in-secret model for GitHub Actions workflows. This should be the default choice for new pipelines.

---

## 1. Cross-Compilation Strategy

### Recommended: Native-runner matrix

Run `cargo build --release --target <triple>` per-OS using GitHub's hosted runners. This is the most robust approach.

| RID | Triple | Runner |
|---|---|---|
| `win-x64` | `x86_64-pc-windows-msvc` | `windows-latest` |
| `win-arm64` | `aarch64-pc-windows-msvc` | `windows-latest` (cross-compile) or `windows-11-arm` |
| `linux-x64` | `x86_64-unknown-linux-musl` | `ubuntu-latest` |
| `linux-arm64` | `aarch64-unknown-linux-musl` | `ubuntu-24.04-arm` (native) or `ubuntu-latest` + cross |
| `osx-x64` | `x86_64-apple-darwin` | `macos-13` (last Intel runner) |
| `osx-arm64` | `aarch64-apple-darwin` | `macos-latest` (now arm64) or `macos-14` |

Key runner facts (2025-2026):
- `macos-latest` points to Apple silicon (macos-14/15), so Intel builds now need an explicitly pinned `macos-13`.
- GitHub ships `ubuntu-24.04-arm` and `windows-11-arm` as free arm64 runners for public repos.

### Alternatives

**`cross` (cross-rs)** — Docker-based; widely used. Best when you have complex native deps. Requires Docker on the runner.

**`cargo-zigbuild`** — uses Zig as a cross-linker, no Docker. Supports only Linux and macOS targets (no Windows). Supports pinning glibc via `aarch64-unknown-linux-gnu.2.17`.

**`cargo-dist` (axodotdev)** — As of v0.31.0 (Feb 2026), supported installers/formats are: shell, PowerShell, Homebrew, npm, and MSI. **cargo-dist does NOT emit a NuGet package natively.**

Related Rust -> NuGet projects:
- `KodrAus/cargo-nuget` — packages Rust *libraries* (cdylibs) as NuGet nupkgs. Not aimed at CLI tools.
- `rylev/cargo-nuget` — installs NuGet deps into a Rust project (opposite direction).

**Conclusion:** you must author the NuGet packaging step yourself; there is no off-the-shelf Rust-CLI-to-NuGet tool.

### Static linking for Linux portability

Use `x86_64-unknown-linux-musl` / `aarch64-unknown-linux-musl` rather than `-gnu`. Add to `.cargo/config.toml`:

```toml
[target.x86_64-unknown-linux-musl]
rustflags = ["-C", "target-feature=+crt-static"]
```

Pin OpenSSL vendoring if used: `openssl = { version = "*", features = ["vendored"] }`.

### Matrix YAML sketch

```yaml
jobs:
  build:
    strategy:
      fail-fast: false
      matrix:
        include:
          - rid: win-x64
            target: x86_64-pc-windows-msvc
            os: windows-latest
            ext: .exe
          - rid: win-arm64
            target: aarch64-pc-windows-msvc
            os: windows-latest
            ext: .exe
          - rid: linux-x64
            target: x86_64-unknown-linux-musl
            os: ubuntu-latest
          - rid: linux-arm64
            target: aarch64-unknown-linux-musl
            os: ubuntu-24.04-arm
          - rid: osx-x64
            target: x86_64-apple-darwin
            os: macos-13
          - rid: osx-arm64
            target: aarch64-apple-darwin
            os: macos-latest
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - name: Install musl-tools
        if: contains(matrix.target, 'musl')
        run: sudo apt-get install -y musl-tools
      - run: cargo build --release --target ${{ matrix.target }}
      - uses: actions/upload-artifact@v4
        with:
          name: bin-${{ matrix.rid }}
          path: target/${{ matrix.target }}/release/aipm${{ matrix.ext || '' }}
          if-no-files-found: error
          retention-days: 1
```

---

## 2. Artifact Aggregation

`actions/upload-artifact@v4` and `actions/download-artifact@v4` (v4 is mandatory — v3 was deprecated in 2024). Matrix-spawned artifacts must have unique names.

```yaml
  package:
    needs: build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/download-artifact@v4
        with:
          path: bin-staging
          pattern: bin-*
          merge-multiple: false   # keep each RID in its own subfolder
      - name: Stage runtimes/ layout
        run: |
          mkdir -p pkg/runtimes
          for rid in win-x64 win-arm64 linux-x64 linux-arm64 osx-x64 osx-arm64; do
            mkdir -p pkg/runtimes/$rid/native
            cp bin-staging/bin-$rid/* pkg/runtimes/$rid/native/
            chmod +x pkg/runtimes/$rid/native/* || true
          done
```

The `runtimes/<RID>/native/` path is the NuGet convention per ["Native files in .NET packages"](https://learn.microsoft.com/en-us/nuget/create-packages/native-files-in-net-packages). The .NET SDK **flattens** the directory structure under `runtimes/<rid>/native/` when copying to output.

---

## 3. Packing & Signing

### .nuspec (recommended for CLI-with-native-binaries)

Minimal nuspec:

```xml
<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://schemas.microsoft.com/packaging/2010/07/nuspec.xsd">
  <metadata>
    <id>aipm</id>
    <version>$version$</version>
    <authors>Sean Larkin</authors>
    <description>Cross-platform Rust CLI distributed as a NuGet package.</description>
    <projectUrl>https://github.com/TheLarkInn/aipm</projectUrl>
    <license type="expression">MIT</license>
    <readme>docs\README.md</readme>
    <icon>images\icon.png</icon>
    <tags>cli tool rust cross-platform ai</tags>
    <repository type="git" url="https://github.com/TheLarkInn/aipm.git" commit="$commit$" />
  </metadata>
  <files>
    <file src="runtimes\**" target="runtimes" />
    <file src="README.md" target="docs\" />
    <file src="icon.png" target="images\" />
    <file src="LICENSE" target="" />
  </files>
</package>
```

Note: `readme` requires NuGet 5.10+; `icon` requires NuGet 5.3+. License must be either an SPDX expression or a packaged file.

### dotnet pack vs nuget pack

- `nuget pack my.nuspec -Version 1.2.3 -OutputDirectory ./out` — straight path, no project needed. Install via `nuget/setup-nuget@v2`.
- `dotnet pack` — prefers SDK-style csproj. As of **.NET 10** (2025), `dotnet pack -p:NuspecFile=my.nuspec -p:NuspecBasePath=.` works without a csproj stub.

Recommendation: `nuget pack` with `setup-nuget@v2` for simplicity.

### Signing

Two distinct things:

1. **Repository signature** — nuget.org **automatically signs every package** on ingest. No opt-in needed.
2. **Author signature** — requires commercial code-signing certificate. Use `nuget sign my.nupkg -CertificatePath cert.pfx -Timestamper http://timestamp.digicert.com`. Windows-only in practice.

**For open-source Rust CLIs, rely solely on the repository signature.**

---

## 4. Pushing to nuget.org — Trusted Publishing (OIDC)

**Status:** GA since **September 22, 2025** (announced on the .NET blog).

### Why use it

- Zero long-lived secrets in GitHub.
- Tokens are short-lived (~1 hour) and single-use.
- Policy is bound to `repo owner + repo name + workflow filename + optional environment`.

### Required permissions

```yaml
permissions:
  contents: read
  id-token: write   # MANDATORY
```

### Policy configuration (one-time, on nuget.org)

Log into nuget.org -> username dropdown -> Trusted Publishing -> **Add policy**:

- Policy owner: you or your org
- Repository Owner: e.g. `TheLarkInn`
- Repository: e.g. `aipm`
- Workflow File: **filename only** (e.g. `release.yml`)
- Environment: optional; bind to a specific GitHub Environment if using environment protection rules.

For private repos: policy is "temporarily active for 7 days" until first successful publish fuses it to immutable GitHub repo/owner IDs.

### Workflow step

```yaml
- name: NuGet login (OIDC -> temp API key)
  uses: NuGet/login@v1
  id: nuget_login
  with:
    user: ${{ secrets.NUGET_USERNAME }}   # your nuget.org profile name (NOT email)

- name: Push to nuget.org
  run: |
    dotnet nuget push out/*.nupkg \
      --api-key "${{ steps.nuget_login.outputs.NUGET_API_KEY }}" \
      --source https://api.nuget.org/v3/index.json \
      --skip-duplicate
```

### Legacy API-key path (fallback)

- Create API key on nuget.org with glob pattern matching your package ID
- Store as `secrets.NUGET_API_KEY`
- Same `dotnet nuget push` command, just `--api-key ${{ secrets.NUGET_API_KEY }}`

### `--skip-duplicate` and idempotency

Treats HTTP 409 Conflict responses as warnings. Makes workflow re-runs safe.

Known bug: `.snupkg` (symbol) packages can cause failures ([NuGet/Home#10475](https://github.com/NuGet/Home/issues/10475)) — avoid pushing symbols alongside main package.

### Package ID reservation

No prior reservation required — just pick a unique ID (<= 128 chars). Prefix reservations (for `MyCompany.*` patterns) are requested via `account@nuget.org` and require >= 4 characters.

---

## 5. Versioning

### Reading version from Cargo.toml

**(a) `SebRollen/toml-action`** — simplest:
```yaml
- id: version
  uses: SebRollen/toml-action@v1.2.0
  with:
    file: Cargo.toml
    field: package.version
```

**(b) `cargo metadata` + `jq`:**
```yaml
- run: |
    VERSION=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].version')
    echo "VERSION=$VERSION" >> "$GITHUB_ENV"
```

**(c) `cargo-get`:**
```yaml
- uses: nicolaiunrein/cargo-get@master
  id: meta
```

### Matching git tag `v1.2.3` to version

```yaml
on:
  push:
    tags: ['v*.*.*']

jobs:
  release:
    steps:
      - id: tagver
        run: echo "version=${GITHUB_REF_NAME#v}" >> "$GITHUB_OUTPUT"
      - name: Verify Cargo.toml matches tag
        run: |
          CARGO_VER=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].version')
          if [ "$CARGO_VER" != "${{ steps.tagver.outputs.version }}" ]; then
            echo "::error::Tag ${{ steps.tagver.outputs.version }} does not match Cargo.toml $CARGO_VER"
            exit 1
          fi
```

For this repo specifically: release-plz creates tags like `aipm-v0.22.3` (see `release-plz.toml`) — so the tag filter should be `'aipm-v*.*.*'` or similar, and stripping the prefix needs adjustment.

### SemVer & pre-release conventions

- NuGet honors SemVer 2.0.0 since 4.3.0+.
- Pre-release suffix: `1.2.3-alpha`, `1.2.3-beta.1`, `1.2.3-rc.1` map 1:1 from Cargo.
- **Pitfall:** use zero-padded numeric suffixes (`beta01`, `beta02`) for pre-NuGet-4.3.0 consumers. Dotted form (`beta.2`) only works with SemVer 2.0 clients.
- `version` field is capped at **64 characters** on nuget.org.

### `nuget pack -Version` vs nuspec token

```bash
nuget pack aipm.nuspec -Version "${VERSION}" -Properties commit="${GITHUB_SHA}" -OutputDirectory out/
```

The `$version$` and `$commit$` tokens in the nuspec are substituted.

---

## 6. End-to-End Example Workflow

### Real-world references

**No fully public Rust-CLI-to-NuGet workflow found** in standard search surface. Closest adjacent:
- [KodrAus/cargo-nuget](https://github.com/KodrAus/cargo-nuget) — Rust cdylibs as NuGet (libraries, not CLIs).
- [axodotdev/cargo-dist](https://github.com/axodotdev/cargo-dist) — no NuGet output as of v0.31.0.
- Microsoft native-binary packages: `Microsoft.Web.Webview2`, `Microsoft.Data.Sqlite.Core` follow the same `runtimes/<RID>/native/` pattern.

### Complete workflow

```yaml
name: Release
on:
  push:
    tags: ['v*.*.*']

permissions:
  contents: read

jobs:
  build:
    strategy:
      fail-fast: false
      matrix:
        include:
          - { rid: win-x64,     target: x86_64-pc-windows-msvc,     os: windows-latest, ext: .exe }
          - { rid: win-arm64,   target: aarch64-pc-windows-msvc,    os: windows-latest, ext: .exe }
          - { rid: linux-x64,   target: x86_64-unknown-linux-musl,  os: ubuntu-latest }
          - { rid: linux-arm64, target: aarch64-unknown-linux-musl, os: ubuntu-24.04-arm }
          - { rid: osx-x64,     target: x86_64-apple-darwin,        os: macos-13 }
          - { rid: osx-arm64,   target: aarch64-apple-darwin,       os: macos-latest }
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - if: contains(matrix.target, 'linux-musl')
        run: sudo apt-get update && sudo apt-get install -y musl-tools
      - run: cargo build --release --target ${{ matrix.target }} --bin aipm
      - uses: actions/upload-artifact@v4
        with:
          name: bin-${{ matrix.rid }}
          path: target/${{ matrix.target }}/release/aipm${{ matrix.ext || '' }}
          if-no-files-found: error

  package-and-publish:
    needs: build
    runs-on: ubuntu-latest
    environment: release          # optional: gate with an environment
    permissions:
      contents: read
      id-token: write             # REQUIRED for NuGet Trusted Publishing
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-dotnet@v4
        with:
          dotnet-version: '10.0.x'
      - uses: nuget/setup-nuget@v2

      - uses: actions/download-artifact@v4
        with:
          path: bin-staging
          pattern: bin-*

      - id: version
        run: echo "value=${GITHUB_REF_NAME#v}" >> "$GITHUB_OUTPUT"

      - name: Stage runtimes layout
        run: |
          for rid in win-x64 win-arm64 linux-x64 linux-arm64 osx-x64 osx-arm64; do
            mkdir -p pkg/runtimes/$rid/native
            cp bin-staging/bin-$rid/* pkg/runtimes/$rid/native/
            chmod +x pkg/runtimes/$rid/native/* || true
          done
          cp README.md LICENSE icon.png pkg/

      - name: Pack
        working-directory: pkg
        run: nuget pack ../packaging/aipm.nuspec -Version "${{ steps.version.outputs.value }}" -Properties commit=${{ github.sha }} -OutputDirectory ../out

      - name: NuGet OIDC login
        id: nuget_login
        uses: NuGet/login@v1
        with:
          user: ${{ secrets.NUGET_USERNAME }}

      - name: Push
        run: |
          dotnet nuget push out/*.nupkg \
            --api-key "${{ steps.nuget_login.outputs.NUGET_API_KEY }}" \
            --source https://api.nuget.org/v3/index.json \
            --skip-duplicate
```

---

## Gaps / Caveats

1. **No reference Rust -> NuGet workflow exists in the wild.** Test against `*-alpha` versions first.
2. **NuGet consumers expect a managed assembly.** A nupkg with only `runtimes/<RID>/native/` is unusual — consider wrapping in a `dotnet tool` with a C# shim, or shipping `build/<id>.targets`.
3. **`cargo-dist` does not emit NuGet output** as of v0.31.0. File a feature request or keep hand-rolled.
4. **`nuget sign` is Windows-only** in practice.
5. **Symbol packages can break `--skip-duplicate`** re-runs.
6. **.NET 8 RID graph change** — use only portable RIDs.
7. **macOS Intel runner deprecation** — `macos-13` is the last Intel runner; plan for cross-compile via `cargo-zigbuild` or removal of `osx-x64`.

---

## Sources

Cross-compilation:
- [Cross Compiling Rust Projects in GitHub Actions](https://blog.urth.org/2023/03/05/cross-compiling-rust-projects-in-github-actions/)
- [Rust Cross-Compilation With GitHub Actions](https://reemus.dev/tldr/rust-cross-compilation-github-actions)
- [cross-rs/cross](https://github.com/cross-rs/cross)
- [rust-cross/cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild)
- [emk/rust-musl-builder](https://github.com/emk/rust-musl-builder)

cargo-dist:
- [axodotdev/cargo-dist](https://github.com/axodotdev/cargo-dist)
- [cargo-dist CHANGELOG](https://github.com/axodotdev/cargo-dist/blob/main/CHANGELOG.md)

NuGet native packaging:
- [Including native libraries in .NET packages](https://learn.microsoft.com/en-us/nuget/create-packages/native-files-in-net-packages)
- [.nuspec File Reference](https://learn.microsoft.com/en-us/nuget/reference/nuspec)
- [.NET RID catalog](https://learn.microsoft.com/en-us/dotnet/core/rid-catalog)
- [KodrAus/cargo-nuget](https://github.com/KodrAus/cargo-nuget)

Trusted Publishing / OIDC:
- [Trusted Publishing on nuget.org — Microsoft Learn](https://learn.microsoft.com/en-us/nuget/nuget-org/trusted-publishing)
- [New Trusted Publishing enhances security on NuGet.org — .NET Blog (Sep 22, 2025)](https://devblogs.microsoft.com/dotnet/enhanced-security-is-here-with-the-new-trust-publishing-on-nuget-org/)
- [NuGet/login GitHub Action](https://github.com/NuGet/login)
- [Publishing NuGet packages from GitHub actions — Andrew Lock](https://andrewlock.net/easily-publishing-nuget-packages-from-github-actions-with-trusted-publishing/)
- [Switching to NuGet trusted publishing — Damir's Corner](https://www.damirscorner.com/blog/posts/20251003-SwitchingToNuGetTrustedPublishing.html)

Publishing:
- [dotnet nuget push command](https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-nuget-push)
- [NuGet Package ID prefix reservation](https://learn.microsoft.com/en-us/nuget/nuget-org/id-prefix-reservation)
- [Signed Packages reference](https://learn.microsoft.com/en-us/nuget/reference/signed-packages-reference)

Versioning:
- [SebRollen/toml-action](https://github.com/SebRollen/toml-action)
- [cargo pkgid](https://doc.rust-lang.org/cargo/commands/cargo-pkgid.html)
- [NuGet Package Versioning](https://learn.microsoft.com/en-us/nuget/concepts/package-versioning)

Artifacts:
- [actions/upload-artifact](https://github.com/actions/upload-artifact)
- [Get started with v4 of GitHub Actions Artifacts](https://github.blog/news-insights/product-news/get-started-with-v4-of-github-actions-artifacts/)
