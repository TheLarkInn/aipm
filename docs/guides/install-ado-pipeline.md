# Using `aipm` in Azure DevOps Pipelines (NuGet)

Use `aipm` in Azure DevOps pipelines by restoring it from [nuget.org](https://www.nuget.org/packages/aipm) — no `curl | sh`, no separate installer step, no service connection for the public feed.

## When to use this

- Your organisation runs Azure DevOps rather than GitHub Actions.
- You prefer reproducible, version-pinned tool installs via a package manager.
- You need `aipm` on Windows, Linux, or macOS agents from a single YAML snippet.

## Prerequisites

- A `.NET 8` (or later) SDK available on the agent, installed via `UseDotNet@2`.
- No Azure Artifacts service connection — public nuget.org is anonymous for restore.

## Quick start

Copy this into your `azure-pipelines.yml`:

```yaml
variables:
  AIPM_VERSION: '0.22.3'              # pin to any published version
  NUGET_PACKAGES: $(Pipeline.Workspace)/.nuget/packages

steps:
  - task: UseDotNet@2
    displayName: 'Use .NET 8 SDK'
    inputs:
      packageType: sdk
      version: 8.x

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
    displayName: 'Restore aipm from nuget.org'

  - pwsh: |
      switch ("$(Agent.OS)") {
        'Windows_NT' { $o = 'win';   $x = 'aipm.exe' }
        'Linux'      { $o = 'linux'; $x = 'aipm'     }
        'Darwin'     { $o = 'osx';   $x = 'aipm'     }
      }
      $a = if ("$(Agent.OSArchitecture)".ToLower() -eq 'arm64') { 'arm64' } else { 'x64' }
      $bin = "$env:NUGET_PACKAGES/aipm/$env:AIPM_VERSION/runtimes/$o-$a/native"
      if ("$(Agent.OS)" -ne 'Windows_NT') { chmod +x "$bin/$x" }
      Write-Host "##vso[task.prependpath]$bin"
    displayName: 'Resolve RID and prepend aipm to PATH'

  - script: aipm --version
    displayName: 'Smoke test'
```

## How it works

### 1. `<PackageDownload>` instead of `<PackageReference>`

`aipm` is a native Rust binary, not a .NET library. `<PackageDownload>` extracts the package into the NuGet global-packages folder without adding any compile-time assets to the project — which is exactly what a CLI-only package needs.

> Version brackets (`[0.22.3]`) are **mandatory** for `<PackageDownload>`; floating ranges are not supported.

The `Microsoft.Build.NoTargets` SDK lets dotnet restore a `.csproj` that produces no build output, keeping the wrapper project self-contained in `$(Agent.TempDirectory)`.

### 2. The binary path after restore

After `dotnet restore`, the aipm binary sits at:

```
$(NUGET_PACKAGES)/aipm/<version>/runtimes/<RID>/native/aipm[.exe]
```

Setting `NUGET_PACKAGES` to `$(Pipeline.Workspace)/.nuget/packages` (a per-run writable location) keeps the path predictable across agent configurations.

### 3. RID resolution

The PowerShell step maps `Agent.OS` and `Agent.OSArchitecture` to a [.NET RID](https://learn.microsoft.com/en-us/dotnet/core/rid-catalog):

| `Agent.OS`   | `Agent.OSArchitecture` | RID           | Binary       |
|--------------|------------------------|---------------|--------------|
| `Windows_NT` | `X64`                  | `win-x64`     | `aipm.exe`   |
| `Windows_NT` | `ARM64`                | `win-arm64`   | `aipm.exe`   |
| `Linux`      | `X64`                  | `linux-x64`   | `aipm`       |
| `Linux`      | `ARM64`                | `linux-arm64` | `aipm`       |
| `Darwin`     | `X64`                  | `osx-x64`     | `aipm`       |
| `Darwin`     | `ARM64`                | `osx-arm64`   | `aipm`       |

> v1 packages ship `win-x64`, `linux-x64`, `osx-x64`, and `osx-arm64`. ARM64 Linux will be added in a later release.

### 4. `##vso[task.prependpath]`

The `task.prependpath` [logging command](https://learn.microsoft.com/en-us/azure/devops/pipelines/scripts/logging-commands?view=azure-devops) prepends the binary directory to `PATH` for every step that follows. Subsequent steps can call `aipm` as a plain command.

## Caching the NuGet package

`dotnet restore` cold-downloads the package on every run (~5–15 MB, 20–40 s). Cache it with `Cache@2` to avoid repeated downloads:

```yaml
- task: Cache@2
  displayName: 'Cache aipm NuGet package'
  inputs:
    key: 'aipm-nuget | $(AIPM_VERSION)'
    path: $(NUGET_PACKAGES)/aipm/$(AIPM_VERSION)
```

Place the `Cache@2` step **before** the `dotnet restore` step. When the cache key matches, the restore step becomes a no-op.

## Full multi-platform matrix example

```yaml
trigger: [main]

strategy:
  matrix:
    linux:
      imageName: 'ubuntu-latest'
    windows:
      imageName: 'windows-latest'
    macos:
      imageName: 'macOS-latest'

pool:
  vmImage: $(imageName)

variables:
  AIPM_VERSION: '0.22.3'
  NUGET_PACKAGES: $(Pipeline.Workspace)/.nuget/packages

steps:
  - task: UseDotNet@2
    displayName: 'Use .NET 8 SDK'
    inputs: { packageType: sdk, version: 8.x }

  - task: Cache@2
    displayName: 'Cache aipm'
    inputs:
      key: 'aipm-nuget | $(AIPM_VERSION) | $(Agent.OS)'
      path: $(NUGET_PACKAGES)/aipm/$(AIPM_VERSION)

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
    displayName: 'Restore aipm from nuget.org'

  - pwsh: |
      switch ("$(Agent.OS)") {
        'Windows_NT' { $o = 'win';   $x = 'aipm.exe' }
        'Linux'      { $o = 'linux'; $x = 'aipm'     }
        'Darwin'     { $o = 'osx';   $x = 'aipm'     }
      }
      $a = if ("$(Agent.OSArchitecture)".ToLower() -eq 'arm64') { 'arm64' } else { 'x64' }
      $bin = "$env:NUGET_PACKAGES/aipm/$env:AIPM_VERSION/runtimes/$o-$a/native"
      if ("$(Agent.OS)" -ne 'Windows_NT') { chmod +x "$bin/$x" }
      Write-Host "##vso[task.prependpath]$bin"
    displayName: 'Resolve RID and prepend aipm to PATH'

  - script: aipm --version
    displayName: 'Smoke test'
```

## Private Azure Artifacts feeds

Public nuget.org requires no authentication. If your organisation mirrors packages through a private Azure Artifacts feed, add `NuGetAuthenticate@1` before the restore step:

```yaml
- task: NuGetAuthenticate@1
  displayName: 'Authenticate to Azure Artifacts'

- script: dotnet restore "$(Agent.TempDirectory)/aipm-fetch/fetch.csproj"
  displayName: 'Restore aipm from private feed'
```

For cross-organisation feeds, supply the service connection name:

```yaml
- task: NuGetAuthenticate@1
  inputs:
    nuGetServiceConnections: OtherOrgFeedConnection
```

## Troubleshooting

### `aipm` not found after prepend

Check that `NUGET_PACKAGES` is set to the same value in both the restore step and the RID-resolution step. If the variable is unset, NuGet falls back to OS defaults (`~/.nuget/packages`), which may differ between steps on Windows.

### Wrong binary picked on ARM64

`Agent.OSArchitecture` reports `ARM64` (capital letters) on some agent images. The snippet lowercases it with `.ToLower()` before the comparison, so case differences are handled.

### Package not found / version not listed

Run `dotnet nuget list source` to verify `https://api.nuget.org/v3/index.json` is present. If you have a `nuget.config` in your repo that clears sources, add nuget.org back explicitly:

```xml
<packageSources>
  <clear />
  <add key="nuget.org" value="https://api.nuget.org/v3/index.json" protocolVersion="3" />
</packageSources>
```

## See also

- [Install — Shell / PowerShell](../../README.md#install) — for GitHub Actions and local machines
- [Cache Management](cache-management.md) — how `aipm` manages its own download cache
- [Lint in CI](lint.md#ci-integration) — using `aipm lint` with the `ci` reporter in ADO
- [`packaging/aipm.nuspec`](../../packaging/aipm.nuspec) — the NuGet package manifest
