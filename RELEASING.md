# Releasing `aipm`

Operational runbook for cutting and rolling back releases.

## Release flow (normal path)

1. Commits land on `main` with conventional-commit messages.
2. `release-plz.yml` opens a "Release PR" bumping versions in every member crate's `Cargo.toml` and updating the per-crate `CHANGELOG.md` via `git-cliff`.
3. Merging the Release PR triggers `release-plz.yml` again, which:
   - Publishes each workspace crate to [crates.io](https://crates.io/crates/aipm)
   - Creates per-crate git tags (e.g., `aipm-v0.22.4`, `libaipm-v0.22.4`)
4. The `aipm-v<semver>` tag push triggers `release.yml` (cargo-dist), which:
   - Cross-compiles 4 targets (win-x64, linux-x64, osx-x64, osx-arm64)
   - Produces `.tar.xz` / `.zip` archives, `sha256` checksums, `aipm-installer.{sh,ps1}`
   - Creates a GitHub Release with all artifacts attached
5. `update-latest-release.yml` fires on `release:published` and republishes the installer scripts to the rolling `latest` GitHub Release.
6. *(Pending Phase 2 activation)* `release-nuget.yml` fires on `release:published` and publishes `aipm.<version>.nupkg` to [nuget.org](https://www.nuget.org/packages/aipm).

## NuGet publish — current status

`release-nuget.yml` ships with **`workflow_dispatch` only**. The `release:published` trigger will be enabled in a follow-up PR after Phase 1 dry-run validates. To run a dry-run or one-off publish:

1. Ensure [`NUGET_USERNAME`](https://github.com/TheLarkInn/aipm/settings/secrets/actions) (public nuget.org handle) and `NUGET_API_KEY` (fallback) secrets exist.
2. Go to **Actions → Publish to NuGet → Run workflow**.
3. Enter `tag` as an existing `aipm-v<semver>` tag whose GitHub Release contains platform archives.
4. The workflow downloads the 4 archives, repacks into `runtimes/<RID>/native/` inside a `.nupkg`, and pushes to nuget.org via OIDC Trusted Publishing (falling back to `NUGET_API_KEY` if OIDC fails).

## Nuspec authoring rules (cross-platform)

The CI runner builds the `.nupkg` using **mono's port of nuget.exe on Linux**, not the Windows-native binary. Mono is stricter than the Windows build — some constructs that Windows tolerates silently cause an `ArgumentException: String cannot be empty. Parameter name: entryName` failure on Linux because mono converts nuspec entries directly into `ZipArchive.CreateEntry` calls.

Rules for editing [`packaging/aipm.nuspec`](packaging/aipm.nuspec):

1. **Use forward slashes everywhere.** Backslash paths (`docs\README.md`, `runtimes\`) work on Windows but fail on mono. Use `docs/README.md` and `runtimes/` instead. This applies to both `<file>` attributes and `<metadata>` fields such as `<readme>`.

2. **Never use an empty `target` attribute.** `<file src="LICENSE" target="" />` produces an entry with an empty name → `ArgumentException`. Give every `<file>` an explicit target: either a full relative path (`target="LICENSE"`) or a trailing slash for directory globs (`target="runtimes/"`).

3. **No trailing separator without a glob.** `target="docs\"` (backslash with no filename) is ambiguous; mono may interpret it literally, misplacing the file in the archive.

Quick reference for the three patterns used in the current nuspec:

```xml
<!-- directory glob: trailing slash tells nuget "place files into this folder" -->
<file src="runtimes/**" target="runtimes/" />

<!-- single file: always specify the target filename explicitly -->
<file src="README.md"   target="docs/README.md" />
<file src="LICENSE"     target="LICENSE" />
```

> **Background.** Run [24904613922](https://github.com/TheLarkInn/aipm/actions/runs/24904613922) first exposed this when the nuspec used `target=""` and backslash paths, causing the Pack step to abort with `String cannot be empty. Parameter name: entryName` before `dotnet nuget push` ran (aipm 0.22.3 was therefore never published). Fixed in [#659](https://github.com/TheLarkInn/aipm/pull/659).

## Rollback — broken nuget.org version

**nuget.org does not permit package deletion.** The only operation is **unlist**, which hides the version from search but leaves it resolvable to anyone who pinned to that exact version. This is a property of the NuGet protocol, not a policy choice.

Procedure if a broken version ships:

1. **Unlist immediately.**
   - Navigate to `https://www.nuget.org/packages/aipm/<broken-version>`.
   - Sign in as the package owner.
   - Click **Manage Package** → **Listing**.
   - Uncheck "List in search results" and save.
   - The version disappears from the gallery within minutes; no new consumer can find it via `dotnet add package aipm` without an explicit version.
2. **Cut a patch release via the normal flow.**
   - Land the fix on `main` (conventional-commit message).
   - Merge the next release-plz Release PR.
   - The `aipm-v<semver+1>` tag triggers `release.yml` and `release-nuget.yml`.
3. **Verify the patch publishes and is listed.**
   - Confirm the new version appears at `https://www.nuget.org/packages/aipm`.
   - Confirm `dotnet add package aipm` picks up the patched version by default.
4. **Communicate.**
   - Add a CHANGELOG entry explicitly calling out the broken version and its unlisting.
   - If the issue is security-relevant, see [`SECURITY.md`](SECURITY.md) and file a GitHub Security Advisory.

## Rollback — broken GitHub Release

Unlike nuget.org, GitHub Releases can be deleted or edited. To remove a broken release:

```bash
gh release delete aipm-v<broken-version> --yes
git push origin :refs/tags/aipm-v<broken-version>
```

Note: crates.io also does not permit deletion — `cargo yank` is the equivalent of nuget.org's unlist. Always run `cargo yank --version <broken>` for the affected crate in addition to the steps above.

## Rollback — broken crates.io publish

```bash
cargo yank --version <broken> aipm
cargo yank --version <broken> libaipm   # also yank the library crate if published in lockstep
```

Yanked versions remain downloadable for anyone who has them in a lockfile but cannot be picked up by new resolution.

## Version scheme

- Workspace is versioned in **lockstep**: all member crates share the same version, set in [`Cargo.toml:10`](Cargo.toml) `[workspace.package]`.
- release-plz creates **per-crate tags** (`aipm-v*`, `libaipm-v*`), not a single `v*` tag.
- The `aipm-v*` tag is the authoritative trigger for binary/NuGet publishing. The `libaipm-v*` tag has no publishing side effect beyond `cargo publish`.
- Pre-release suffixes (`-alpha.N`, `-beta.N`, `-rc.N`) are honored by SemVer 2.0 but are **not** published to nuget.org (the workflow's `if:` guard skips `prerelease` releases).

## References

- [`specs/2026-04-22-nuget-publishing-pipeline.md`](specs/2026-04-22-nuget-publishing-pipeline.md) — NuGet publishing spec
- [`specs/2026-03-16-ci-cd-release-automation.md`](specs/2026-03-16-ci-cd-release-automation.md) — original CI/CD spec
- [`specs/2026-03-19-cargo-dist-installers.md`](specs/2026-03-19-cargo-dist-installers.md) — cargo-dist integration spec
- [`release-plz.toml`](release-plz.toml) — release-plz config
- [`dist-workspace.toml`](dist-workspace.toml) — cargo-dist target matrix
- [`cliff.toml`](cliff.toml) — changelog template
