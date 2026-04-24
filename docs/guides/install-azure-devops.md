# Installing `aipm` in Azure DevOps Pipelines (NuGet)

Use this guide if your Azure DevOps pipeline needs `aipm` and you cannot or prefer not to use `curl | sh`. The `aipm` NuGet package on [nuget.org](https://www.nuget.org/packages/aipm) contains pre-built binaries for all four supported RIDs and requires no authentication for public feeds.

## When to use this approach

| Scenario | Recommended method |
|----------|--------------------|
| ADO self-hosted agents with nuget.org allowlisted | **This guide** (NuGet + `PackageDownload`) |
| Enterprise / regulated environments with NuGet proxy | **This guide** with an Azure Artifacts upstream |
| GitHub Actions | `curl` installer (see `README.md`) or `cargo install` |
| Local development | Shell/PowerShell installer from the GitHub Release |

## Supported platforms

| NuGet RID | OS / arch |
|-----------|-----------|
| `win-x64` | Windows x64 |
| `linux-x64` | Linux x64 |
| `osx-x64` | macOS Intel |
| `osx-arm64` | macOS Apple Silicon |

## Step 1 — Create a download-only wrapper project

Create a small csproj file that uses `<PackageDownload>` (not `<PackageReference>`) so NuGet extracts the package without adding any assets to your build:

```xml
<!-- .pipeline/aipm-fetch/fetch.csproj -->
<Project Sdk="Microsoft.Build.NoTargets/3.7.0">
  <PropertyGroup>
    <TargetFramework>net8.0</TargetFramework>
    <DisableImplicitNuGetFallbackFolder>true</DisableImplicitNuGetFallbackFolder>
  </PropertyGroup>
  <ItemGroup>
    <!-- Exact version pin is required for PackageDownload (square brackets = exact match) -->
    <PackageDownload Include="aipm" Version="[$(AipmVersion)]" />
  </ItemGroup>
</Project>
```

You can commit this file to your repository or generate it in-pipeline (see the full YAML below).

## Step 2 — YAML pipeline

```yaml
variables:
  AIPM_VERSION: '0.22.3'          # pin to a specific release
  NUGET_PACKAGES: $(Pipeline.Workspace)/.nuget/packages   # reproducible cache location

steps:
  # 1. Ensure the .NET SDK is available (skip if your agent already has it)
  - task: UseDotNet@2
    inputs:
      packageType: sdk
      version: 8.x

  # 2. Generate the download-only wrapper project inline
  - pwsh: |
      New-Item -ItemType Directory -Force -Path "$(Agent.TempDirectory)/aipm-fetch" | Out-Null
      @"
      <Project Sdk="Microsoft.Build.NoTargets/3.7.0">
        <PropertyGroup>
          <TargetFramework>net8.0</TargetFramework>
          <DisableImplicitNuGetFallbackFolder>true</DisableImplicitNuGetFallbackFolder>
        </PropertyGroup>
        <ItemGroup>
          <PackageDownload Include="aipm" Version="[$(AIPM_VERSION)]" />
        </ItemGroup>
      </Project>
      "@ | Set-Content "$(Agent.TempDirectory)/aipm-fetch/fetch.csproj"
    displayName: 'Generate aipm download-only project'

  # 3. Restore (no NuGetAuthenticate needed — nuget.org is anonymous for reads)
  - script: dotnet restore "$(Agent.TempDirectory)/aipm-fetch/fetch.csproj"
    displayName: 'Restore aipm from nuget.org'

  # 4. Compute RID from ADO agent variables and add the binary directory to PATH
  - pwsh: |
      switch ("$(Agent.OS)") {
        'Windows_NT' { $o = 'win';   $x = 'aipm.exe' }
        'Linux'      { $o = 'linux'; $x = 'aipm'     }
        'Darwin'     { $o = 'osx';   $x = 'aipm'     }
      }
      $a   = if ("$(Agent.OSArchitecture)".ToLower() -eq 'arm64') { 'arm64' } else { 'x64' }
      $bin = "$(NUGET_PACKAGES)/aipm/$(AIPM_VERSION)/runtimes/$o-$a/native"
      if ("$(Agent.OS)" -ne 'Windows_NT') { chmod +x "$bin/$x" }
      Write-Host "##vso[task.prependpath]$bin"
    displayName: 'Add aipm to PATH'

  # 5. Verify
  - script: aipm --version
    displayName: 'Verify aipm'
```

## How it works

After `dotnet restore`, NuGet extracts the package into the global-packages folder:

```
$(NUGET_PACKAGES)/
  aipm/
    <version>/
      runtimes/
        win-x64/native/aipm.exe
        linux-x64/native/aipm
        osx-x64/native/aipm
        osx-arm64/native/aipm
```

The `##vso[task.prependpath]` log command adds the correct `runtimes/<RID>/native/` directory to `PATH` for all subsequent steps.

## Pinning vs floating versions

`<PackageDownload>` requires an **exact** version with square brackets (`[0.22.3]`). To upgrade, change the `AIPM_VERSION` pipeline variable and re-run. There is no wildcard or range resolution.

## Caching (optional)

Cache the global-packages folder to avoid re-downloading on every run:

```yaml
- task: Cache@2
  inputs:
    key: 'nuget | "$(Agent.OS)" | aipm-$(AIPM_VERSION)'
    path: $(NUGET_PACKAGES)
  displayName: 'Cache NuGet packages'
```

Place this step before the `dotnet restore` step.

## Authentication

Public nuget.org does **not** require authentication for package downloads. If you are mirroring through an Azure Artifacts upstream feed, add `NuGetAuthenticate@1` before the restore step and update the `nuget.config` source URL to point at your feed.

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `Unable to find package aipm` | Version not yet published or mistyped | Check [nuget.org/packages/aipm](https://www.nuget.org/packages/aipm) |
| `Permission denied` on Linux/macOS | Binary not executable | The `chmod +x` in step 4 handles this; ensure that step ran |
| `aipm: command not found` after PATH step | `prependpath` takes effect in the next step | Move the verification step after the `prependpath` step |
| Wrong binary for this agent | RID detection | Log `$o-$a` in the pwsh step to verify the computed RID |

## See also

- [nuget.org/packages/aipm](https://www.nuget.org/packages/aipm) — package page and version history
- [`research/docs/2026-04-22-ado-pipeline-nuget-consume.md`](../../research/docs/2026-04-22-ado-pipeline-nuget-consume.md) — deep-dive research on ADO NuGet patterns
- [`RELEASING.md`](../../RELEASING.md) — how `aipm` is published to nuget.org
- [Engine & Platform Compatibility](engine-platform-compatibility.md) — supported AI engines and OS platforms
