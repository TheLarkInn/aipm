# Installing via NuGet (Azure DevOps Pipelines)

Starting with v0.22.4, `aipm` is published to [nuget.org](https://www.nuget.org/packages/aipm) as a
multi-RID native package. This lets Azure DevOps pipelines that prefer `dotnet restore` semantics
install `aipm` without `curl | sh`.

The package ships pre-built binaries for four runtime identifiers:

| RID | Platform |
|-----|----------|
| `win-x64` | Windows (64-bit) |
| `linux-x64` | Linux (64-bit) |
| `osx-x64` | macOS Intel |
| `osx-arm64` | macOS Apple Silicon |

No runtime dependency beyond `dotnet` (used only for the restore step). Public nuget.org requires
**no service connection** and **no authentication**.

## Quick Start (Azure DevOps)

Pin a version with the `AIPM_VERSION` variable, then restore and add the binary to `PATH` in a
single job:

```yaml
variables:
  AIPM_VERSION: '0.23.1'
  NUGET_PACKAGES: $(Pipeline.Workspace)/.nuget/packages

steps:
  - task: UseDotNet@2
    inputs: { packageType: sdk, version: 8.x }

  - pwsh: |
      New-Item -ItemType Directory -Force -Path "$(Agent.TempDirectory)/aipm-fetch" | Out-Null
      @'
      <Project Sdk="Microsoft.Build.NoTargets/3.7.0">
        <PropertyGroup>
          <TargetFramework>net8.0</TargetFramework>
          <DisableImplicitNuGetFallbackFolder>true</DisableImplicitNuGetFallbackFolder>
        </PropertyGroup>
        <ItemGroup>
          <PackageDownload Include="aipm" Version="[$(env:AIPM_VERSION)]" />
        </ItemGroup>
      </Project>
      '@ | Set-Content "$(Agent.TempDirectory)/aipm-fetch/fetch.csproj"
    displayName: 'Generate aipm download-only project'

  - script: dotnet restore "$(Agent.TempDirectory)/aipm-fetch/fetch.csproj"
    displayName: 'Restore aipm NuGet package'

  - pwsh: |
      switch ("$(Agent.OS)") {
        'Windows_NT' { $o = 'win';   $x = 'aipm.exe' }
        'Linux'      { $o = 'linux'; $x = 'aipm' }
        'Darwin'     { $o = 'osx';   $x = 'aipm' }
      }
      $a   = if ("$(Agent.OSArchitecture)".ToLower() -eq 'arm64') { 'arm64' } else { 'x64' }
      $bin = "$env:NUGET_PACKAGES/aipm/$env:AIPM_VERSION/runtimes/$o-$a/native"
      if ("$(Agent.OS)" -ne 'Windows_NT') { chmod +x "$bin/$x" }
      Write-Host "##vso[task.prependpath]$bin"
    displayName: 'Add aipm to PATH'

  - script: aipm --version
    displayName: 'Verify aipm'
```

## How It Works

`aipm` is packaged using the `<PackageDownload>` pattern (not `<PackageReference>`), so:

- **No assets are added to any project** — the csproj is a throw-away wrapper used only to drive
  `dotnet restore`.
- **Dependencies are not resolved** — `aipm` is a self-contained binary with no managed
  dependencies.
- The restored package lands in the NuGet global-packages folder
  (`$(Pipeline.Workspace)/.nuget/packages` when you set `NUGET_PACKAGES`), not in any project
  output directory.

After restore, the binary lives at:

```
$NUGET_PACKAGES/aipm/<version>/runtimes/<rid>/native/aipm[.exe]
```

The `prependpath` logging command makes `aipm` available to all subsequent pipeline steps.

## Caching the Download

Add a `Cache@2` step before the restore step to avoid re-downloading `aipm` on every run:

```yaml
- task: Cache@2
  inputs:
    key: nuget | aipm-$(AIPM_VERSION) | $(Agent.OS)
    path: $(NUGET_PACKAGES)/aipm
  displayName: 'Cache aipm NuGet package'
```

## Pinning vs Floating Versions

`<PackageDownload>` requires an **exact version** in brackets — `Version="[0.22.5]"`. Floating
ranges (e.g., `[0.22,)`) are not supported. Update `AIPM_VERSION` in your pipeline variables to
upgrade.

## Linting in Azure DevOps

After installing, use the `ci-azure` reporter to surface lint violations as collapsible groups
with inline work-item–linkable rule codes:

```yaml
- script: aipm lint --reporter ci-azure
  displayName: 'Lint AI plugins'
```

Warnings exit `0` and mark the step `SucceededWithIssues` (yellow). Errors exit non-zero and
fail the step. See [Using `aipm lint`](lint.md) for the full reporter reference.

## See also

- [README — Install section](../../README.md#install) — shell and PowerShell installers for
  non-ADO environments
- [Using `aipm lint`](lint.md) — CI reporters, exit codes, and rule configuration
- [Source Security](source-security.md) — allowlists and path-traversal protection
