# Installing `aipm` via NuGet in Azure DevOps Pipelines

Install `aipm` from [nuget.org](https://www.nuget.org/packages/aipm) into an Azure DevOps
pipeline without `curl | sh`. The package ships pre-built native binaries for all four supported
platforms (win-x64, linux-x64, osx-x64, osx-arm64) as a multi-RID NuGet package.

## Overview

The install pattern uses a minimal download-only `.csproj` with `<PackageDownload>` to pull the
package into the NuGet global-packages folder, then resolves the correct binary path for the
running agent and prepends it to `PATH`. No authentication is needed for public nuget.org.

## Prerequisites

- An Azure DevOps pipeline YAML file
- The .NET SDK available on the agent (`UseDotNet@2` or pre-installed)

## Step-by-step

### 1. Set pipeline variables

```yaml
variables:
  AIPM_VERSION: '0.22.3'
  NUGET_PACKAGES: $(Pipeline.Workspace)/.nuget/packages
```

Pin `AIPM_VERSION` to an exact version. `<PackageDownload>` does not accept floating version
ranges.

### 2. Ensure the .NET SDK is available

```yaml
steps:
  - task: UseDotNet@2
    inputs:
      packageType: sdk
      version: 8.x
    displayName: 'Install .NET SDK'
```

Skip this step if your agent image already ships with the .NET 8+ SDK.

### 3. Generate a download-only project

```yaml
  - pwsh: |
      New-Item -ItemType Directory -Force -Path "$(Agent.TempDirectory)/aipm-fetch" | Out-Null

      $csproj = @'
      <Project Sdk="Microsoft.Build.NoTargets/3.7.0">
        <PropertyGroup>
          <TargetFramework>net8.0</TargetFramework>
          <DisableImplicitNuGetFallbackFolder>true</DisableImplicitNuGetFallbackFolder>
          <AutomaticallyUseReferenceAssemblyPackages>false</AutomaticallyUseReferenceAssemblyPackages>
        </PropertyGroup>
        <ItemGroup>
          <PackageDownload Include="aipm" Version="[$($env:AIPM_VERSION)]" />
        </ItemGroup>
      </Project>
      '@
      Set-Content "$(Agent.TempDirectory)/aipm-fetch/fetch.csproj" -Value $csproj -Encoding UTF8
    displayName: 'Generate aipm download-only project'
```

`Microsoft.Build.NoTargets` is a lightweight MSBuild SDK designed for projects that only fetch
packages without compiling anything. Version brackets (`[1.2.3]`) are **mandatory** for
`<PackageDownload>` — floating ranges are not supported.

### 4. Restore the package

```yaml
  - script: dotnet restore "$(Agent.TempDirectory)/aipm-fetch/fetch.csproj"
    displayName: 'Restore aipm from nuget.org'
```

`dotnet restore` resolves and downloads the package into `$(NUGET_PACKAGES)`. No service
connection or `NuGetAuthenticate@1` is needed for public nuget.org.

### 5. Resolve the RID and prepend to PATH

```yaml
  - pwsh: |
      switch ("$(Agent.OS)") {
        'Windows_NT' { $ridOs = 'win';   $exe = 'aipm.exe' }
        'Linux'      { $ridOs = 'linux'; $exe = 'aipm'     }
        'Darwin'     { $ridOs = 'osx';   $exe = 'aipm'     }
        default      { throw "Unsupported Agent.OS: $(Agent.OS)" }
      }
      $ridArch = if ("$(Agent.OSArchitecture)".ToLower() -eq 'arm64') { 'arm64' } else { 'x64' }
      $rid = "$ridOs-$ridArch"

      $binDir = "$env:NUGET_PACKAGES/aipm/$env:AIPM_VERSION/runtimes/$rid/native"
      $binary = Join-Path $binDir $exe

      if (-not (Test-Path $binary)) {
        throw "aipm binary not found at $binary — check AIPM_VERSION and that the package published successfully."
      }

      if ("$(Agent.OS)" -ne 'Windows_NT') { chmod +x $binary }

      Write-Host "Resolved RID: $rid"
      Write-Host "##vso[task.prependpath]$binDir"
    displayName: 'Resolve RID and prepend aipm to PATH'
```

### 6. Verify the install

```yaml
  - script: aipm --version
    displayName: 'aipm smoke test'
```

## Complete example

```yaml
variables:
  AIPM_VERSION: '0.22.3'
  NUGET_PACKAGES: $(Pipeline.Workspace)/.nuget/packages

steps:
  - task: UseDotNet@2
    inputs: { packageType: sdk, version: 8.x }
    displayName: 'Install .NET SDK'

  - pwsh: |
      New-Item -ItemType Directory -Force -Path "$(Agent.TempDirectory)/aipm-fetch" | Out-Null
      $csproj = @"
      <Project Sdk="Microsoft.Build.NoTargets/3.7.0">
        <PropertyGroup>
          <TargetFramework>net8.0</TargetFramework>
          <DisableImplicitNuGetFallbackFolder>true</DisableImplicitNuGetFallbackFolder>
          <AutomaticallyUseReferenceAssemblyPackages>false</AutomaticallyUseReferenceAssemblyPackages>
        </PropertyGroup>
        <ItemGroup>
          <PackageDownload Include="aipm" Version="[$($env:AIPM_VERSION)]" />
        </ItemGroup>
      </Project>
      "@
      Set-Content "$(Agent.TempDirectory)/aipm-fetch/fetch.csproj" -Value $csproj -Encoding UTF8
    displayName: 'Generate aipm download-only project'

  - script: dotnet restore "$(Agent.TempDirectory)/aipm-fetch/fetch.csproj"
    displayName: 'Restore aipm from nuget.org'

  - pwsh: |
      switch ("$(Agent.OS)") {
        'Windows_NT' { $ridOs = 'win';   $exe = 'aipm.exe' }
        'Linux'      { $ridOs = 'linux'; $exe = 'aipm'     }
        'Darwin'     { $ridOs = 'osx';   $exe = 'aipm'     }
        default      { throw "Unsupported Agent.OS: $(Agent.OS)" }
      }
      $ridArch = if ("$(Agent.OSArchitecture)".ToLower() -eq 'arm64') { 'arm64' } else { 'x64' }
      $rid = "$ridOs-$ridArch"
      $binDir = "$env:NUGET_PACKAGES/aipm/$env:AIPM_VERSION/runtimes/$rid/native"
      if (-not (Test-Path (Join-Path $binDir $exe))) { throw "aipm binary not found at $binDir" }
      if ("$(Agent.OS)" -ne 'Windows_NT') { chmod +x (Join-Path $binDir $exe) }
      Write-Host "##vso[task.prependpath]$binDir"
    displayName: 'Resolve RID and prepend aipm to PATH'

  - script: aipm --version
    displayName: 'aipm smoke test'
```

## Caching the NuGet package

Add a `Cache@2` step before the restore to skip network downloads on warm runs:

```yaml
  - task: Cache@2
    inputs:
      key: 'nuget | "$(Agent.OS)" | aipm-$(AIPM_VERSION)'
      path: $(NUGET_PACKAGES)
    displayName: 'Cache NuGet packages'
```

Place this step immediately before the `dotnet restore` step.

## Supported platforms

| Agent OS | `Agent.OS` value | `Agent.OSArchitecture` | RID |
|----------|-----------------|----------------------|-----|
| Windows x64 | `Windows_NT` | `X64` | `win-x64` |
| Linux x64 | `Linux` | `X64` | `linux-x64` |
| macOS x64 | `Darwin` | `X64` | `osx-x64` |
| macOS Apple Silicon | `Darwin` | `ARM64` | `osx-arm64` |

## Troubleshooting

**`aipm binary not found at …`**  
Verify `AIPM_VERSION` matches a published version at
[nuget.org/packages/aipm](https://www.nuget.org/packages/aipm). The `dotnet restore` step logs
the exact path where the package was extracted.

**`PackageDownload` version brackets required**  
`<PackageDownload Include="aipm" Version="[0.22.3]" />` — the square brackets are mandatory.
Omitting them causes the restore to fail.

**`mono` errors on Linux agents**  
This guide uses `dotnet restore` (cross-platform .NET CLI), not `NuGetCommand@2`. No mono
dependency is introduced. If you see mono errors, ensure you are not using `NuGetCommand@2`.

## See also

- [Installing from Git](install-git-plugin.md)
- [Installing from Local Paths](install-local-plugin.md)
- [Installing from Marketplaces](install-marketplace-plugin.md)
- [NuGet packaging spec](../../specs/2026-04-22-nuget-publishing-pipeline.md)
- [RELEASING.md — NuGet publish runbook](../../RELEASING.md#nuget-publish--current-status)
