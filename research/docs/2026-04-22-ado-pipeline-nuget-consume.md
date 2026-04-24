---
date: 2026-04-22 16:14:50 UTC
researcher: Sean Larkin
git_commit: 5616fd4db5d41b77df55686365308cf12701af2a
branch: main
repository: aipm
topic: "Azure DevOps pipeline consumption of a multi-RID native NuGet CLI tool"
tags: [research, azure-devops, ado, nuget, pipeline, PackageDownload, NuGetAuthenticate, consume, distribution]
status: complete
last_updated: 2026-04-22
last_updated_by: Sean Larkin
---

# ADO pipeline consumption of a NuGet native CLI tool

## Summary

Consuming a NuGet package containing a pre-built multi-RID native CLI binary from an Azure DevOps YAML pipeline is fully supported and well-documented. The cleanest 2025-era pattern: use `NuGetAuthenticate@1` + `dotnet restore` (or plain `nuget restore`) against a tiny "tool wrapper" csproj that uses `<PackageDownload>` (not `<PackageReference>`), then compute the RID from `Agent.OS` / `Agent.OSArchitecture`, then emit `##vso[task.prependpath]` to add `runtimes/<RID>/native/` to PATH for subsequent steps. No authentication is required for public nuget.org, and the same YAML works across `ubuntu-latest`, `windows-latest`, and `macOS-latest`.

## Detailed Findings

### 1. Restore from nuget.org in ADO

**`NuGetCommand@2` `restore` vs `DotNetCoreCLI@2` `restore`**

Per the [NuGetCommand@2 reference](https://learn.microsoft.com/en-us/azure/devops/pipelines/tasks/reference/nuget-command-v2?view=azure-pipelines):

- `NuGetCommand@2` uses `NuGet.exe` (full .NET Framework on Windows, or mono on Linux/macOS). Docs explicitly warn: *"Starting with Ubuntu 24.04, Microsoft-hosted agents will not ship with mono which is required to run the underlying NuGet client that powers `NuGetCommand@2`. Users of this task on Ubuntu should migrate to the long term supported cross-platform task `NuGetAuthenticate@1` with .NET CLI."*
- Both `NuGetCommand@2` and restore/push commands of `DotNetCoreCLI@2` are in maintenance mode.
- `DotNetCoreCLI@2` ([reference](https://learn.microsoft.com/en-us/azure/devops/pipelines/tasks/reference/dotnet-core-cli-v2?view=azure-pipelines)) uses the `dotnet` SDK's built-in NuGet client — correct choice for SDK-style `PackageReference` csproj.

For aipm (multi-RID native), `DotNetCoreCLI@2` with `command: restore` on a wrapper csproj, or a plain `- script: dotnet restore` after `NuGetAuthenticate@1`, is the cross-platform path.

**Packaging style: `packages.config` vs `PackageReference` vs `<PackageDownload>`**

Per [NuGet PackageDownload docs](https://learn.microsoft.com/en-us/nuget/consume-packages/packagedownload-functionality):

> "PackageDownload is a utility feature for all .NET SDK-style projects, and it works alongside PackageReference. [...] The primary application of PackageDownload is downloading packages that do not follow the traditional NuGet package structure and primarily carry build time dependencies."

This is exactly aipm's use case. Key properties:
- Only exact versions allowed: `Version="[1.2.3]"` (brackets mandatory).
- No assets added to project; nothing passed to compiler.
- Dependencies are **not** resolved (plus for a leaf tool).
- Packages extracted into NuGet global-packages folder.

Canonical download-only wrapper csproj:

```xml
<Project Sdk="Microsoft.Build.NoTargets/1.0.80">
  <PropertyGroup>
    <TargetFramework>net8.0</TargetFramework>
    <DisableImplicitNuGetFallbackFolder>true</DisableImplicitNuGetFallbackFolder>
    <AutomaticallyUseReferenceAssemblyPackages>false</AutomaticallyUseReferenceAssemblyPackages>
  </PropertyGroup>
  <ItemGroup>
    <PackageDownload Include="Aipm" Version="[1.0.0]" />
  </ItemGroup>
</Project>
```

`PackageDownload` **cannot** live in `Directory.Packages.props` — it must be in a regular csproj's `<ItemGroup>`. `packages.config` is legacy (non-SDK projects) and does not belong in any new pipeline.

**`GlobalPackageReference`** (from `Directory.Packages.props`) is an alternative, but requires at least one `PackageReference`-style project in the repo. For a pipeline that only needs the CLI, the `NoTargets` + `PackageDownload` wrapper is cleaner.

**Does `NuGetAuthenticate@1` help for public nuget.org?**

No. Per [NuGetAuthenticate@1 remarks](https://learn.microsoft.com/en-us/azure/devops/pipelines/tasks/reference/nuget-authenticate-v1?view=azure-pipelines):

> "This task must run before you use a NuGet tool to restore or push packages to **an authenticated package source** such as Azure Artifacts."
> "Some package sources such as nuget.org use API keys for authentication when pushing packages, rather than username/password credentials."

nuget.org is **anonymous for read** (restore), so no authentication task or service connection is needed.

### 2. Locating the binary after restore

**Where packages end up.** Per [global-packages docs](https://learn.microsoft.com/en-us/nuget/consume-packages/managing-the-global-packages-and-cache-folders):

| OS | Default global-packages location |
| --- | --- |
| Windows | `%userprofile%\.nuget\packages` |
| macOS / Linux | `~/.nuget/packages` |

Override precedence: `NUGET_PACKAGES` env var > `globalPackagesFolder` in `nuget.config` > `RestorePackagesPath` MSBuild property > default.

On Microsoft-hosted agents, defaults resolve to:
- `ubuntu-latest`: `/home/vsts/.nuget/packages`
- `macOS-latest`: `/Users/runner/.nuget/packages`
- `windows-latest`: `C:\Users\VssAdministrator\.nuget\packages`

**Recommendation:** set `NUGET_PACKAGES` to a path under `$(Pipeline.Workspace)`:

```yaml
variables:
  NUGET_PACKAGES: $(Pipeline.Workspace)/.nuget/packages
```

After restore, the aipm binary is at:
```
$(NUGET_PACKAGES)/aipm/<version>/runtimes/<RID>/native/aipm(.exe)
```

(NuGet always lowercases the package id and version in this path.)

**Adding to PATH.** Use the `task.prependpath` logging command ([Logging commands docs](https://learn.microsoft.com/en-us/azure/devops/pipelines/scripts/logging-commands?view=azure-devops)):

> "`##vso[task.prependpath]local file path` — Update the PATH environment variable by prepending to the PATH. The updated environment variable will be reflected in subsequent tasks."

Must be a single line to stdout, UTF-8, absolute path.

### 3. Cross-platform pipeline support

Per [Predefined variables](https://learn.microsoft.com/en-us/azure/devops/pipelines/build/variables?view=azure-devops):

- `Agent.OS`: `Windows_NT`, `Darwin`, `Linux`
- `Agent.OSArchitecture`: `X86`, `X64`, `ARM` (docs don't explicitly list `ARM64` but ARM64 agents do report `ARM64` in practice)

**RID lookup table**:

| Agent.OS | Agent.OSArchitecture | RID | Binary |
| --- | --- | --- | --- |
| `Windows_NT` | `X64` | `win-x64` | `aipm.exe` |
| `Windows_NT` | `ARM64` | `win-arm64` | `aipm.exe` |
| `Linux` | `X64` | `linux-x64` | `aipm` |
| `Linux` | `ARM64` | `linux-arm64` | `aipm` |
| `Darwin` | `X64` | `osx-x64` | `aipm` |
| `Darwin` | `ARM64` | `osx-arm64` | `aipm` |

### 4. Alternative: `dotnet tool install`

Per [dotnet tool install docs](https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-tool-install): .NET tool packages require a **managed** `.dll` entry point with `DotnetToolSettings.xml`. Not suitable for arbitrary native Rust binaries.

.NET 10 preview introduced self-contained / native-AOT .NET tools ([Andrew Lock's article](https://andrewlock.net/exploring-dotnet-10-preview-features-7-packaging-self-contained-and-native-aot-dotnet-tools-for-nuget/), [dotnet/sdk#9503](https://github.com/dotnet/sdk/issues/9503)) — but these are for AOT-compiled .NET CLIs. **Not aipm's path.**

**Why we're not using `dotnet tool install`:**
1. aipm is a Rust binary, not .NET.
2. Tool package format assumes a dotnet-host entry point.
3. Forcing consumers to install .NET SDK is a bigger dependency than necessary.

### 5. Example `azure-pipelines.yml`

```yaml
# azure-pipelines.yml — consume the public `Aipm` NuGet package as a build-step CLI
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
  AIPM_VERSION: '1.0.0'
  NUGET_PACKAGES: $(Pipeline.Workspace)/.nuget/packages

steps:
  - task: UseDotNet@2
    displayName: 'Use .NET 8 SDK'
    inputs:
      packageType: sdk
      version: 8.x

  - pwsh: |
      $proj = @'
      <Project Sdk="Microsoft.Build.NoTargets/3.7.0">
        <PropertyGroup>
          <TargetFramework>net8.0</TargetFramework>
          <DisableImplicitNuGetFallbackFolder>true</DisableImplicitNuGetFallbackFolder>
          <AutomaticallyUseReferenceAssemblyPackages>false</AutomaticallyUseReferenceAssemblyPackages>
        </PropertyGroup>
        <ItemGroup>
          <PackageDownload Include="Aipm" Version="[$(env:AIPM_VERSION)]" />
        </ItemGroup>
      </Project>
      '@
      New-Item -ItemType Directory -Force -Path "$(Agent.TempDirectory)/aipm-fetch" | Out-Null
      Set-Content -Path "$(Agent.TempDirectory)/aipm-fetch/fetch.csproj" -Value $proj -Encoding UTF8

      $cfg = @'
      <?xml version="1.0" encoding="utf-8"?>
      <configuration>
        <packageSources>
          <clear />
          <add key="nuget.org" value="https://api.nuget.org/v3/index.json" protocolVersion="3" />
        </packageSources>
      </configuration>
      '@
      Set-Content -Path "$(Agent.TempDirectory)/aipm-fetch/nuget.config" -Value $cfg -Encoding UTF8
    displayName: 'Generate aipm download-only project'

  - script: dotnet restore "$(Agent.TempDirectory)/aipm-fetch/fetch.csproj"
    displayName: 'Restore Aipm from nuget.org'

  - pwsh: |
      $os = "$(Agent.OS)"
      $arch = "$(Agent.OSArchitecture)".ToLowerInvariant()

      switch ($os) {
        'Windows_NT' { $ridOs = 'win';   $exe = 'aipm.exe' }
        'Linux'      { $ridOs = 'linux'; $exe = 'aipm'     }
        'Darwin'     { $ridOs = 'osx';   $exe = 'aipm'     }
        default      { throw "Unsupported Agent.OS: $os" }
      }
      $ridArch = if ($arch -eq 'arm64') { 'arm64' } else { 'x64' }
      $rid = "$ridOs-$ridArch"

      $binDir = Join-Path $env:NUGET_PACKAGES "aipm/$env:AIPM_VERSION/runtimes/$rid/native"
      $binary = Join-Path $binDir $exe

      if (-not (Test-Path $binary)) {
        throw "aipm binary not found at $binary"
      }

      if ($os -ne 'Windows_NT') {
        chmod +x $binary
      }

      Write-Host "Resolved RID: $rid"
      Write-Host "Binary: $binary"
      Write-Host "##vso[task.prependpath]$binDir"
    displayName: 'Resolve RID and prepend aipm to PATH'

  - script: aipm --version
    displayName: 'aipm smoke test'
```

**Notes:**
- `Microsoft.Build.NoTargets` is a well-known MSBuild SDK on nuget.org, designed for "project that doesn't build anything, just orchestrates NuGet".
- Version brackets `[1.0.0]` are **mandatory** for `<PackageDownload>`; floating versions not supported.
- RID resolution is the only platform-aware step.
- `chmod +x` on non-Windows is defensive.

**Real-world examples:** no public repo found demonstrating this exact pattern end-to-end. Closest references:
- [microsoft/winget-cli `azure-pipelines.yml`](https://github.com/microsoft/winget-cli/blob/master/azure-pipelines.yml) — uses `NuGetCommand@2 restore` on a native solution.
- [LanceMcCarthy/DevOpsExamples `azure-pipelines.yml`](https://github.com/LanceMcCarthy/DevOpsExamples/blob/main/azure-pipelines.yml) — `NuGetCommand@2 restore` with custom `nuget.config`.

### 6. Service connections / auth

**Public nuget.org (current target):** No service connection, no `NuGetAuthenticate@1`, no secrets.

**Private Azure Artifacts (future-state):**
1. **Same organization** — no service connection needed; build identity is used automatically.
2. **Different organization** — create NuGet service connection, reference it in `NuGetAuthenticate@1`:
   ```yaml
   - task: NuGetAuthenticate@1
     inputs:
       nuGetServiceConnections: OtherOrgFeedConnection
   ```
3. **Workload identity (2025+)** — use `workloadIdentityServiceConnection` for PAT-less cross-org auth.

Caveats: doesn't work from external forks (no secrets); cross-project feeds require grants.

## Additional Resources

- [Restore NuGet packages with Azure Pipelines](https://learn.microsoft.com/en-us/azure/devops/pipelines/packages/nuget-restore?view=azure-devops)
- [Cache NuGet packages](https://learn.microsoft.com/en-us/azure/devops/pipelines/artifacts/caching-nuget?view=azure-devops) — use `Cache@2` keyed on `$(NUGET_PACKAGES)` + lockfile hash.
- [Agent directory structure](https://learn.microsoft.com/en-us/azure/devops/pipelines/agents/agents#agent-directory-structure)

## Gaps / Limitations

- **`Agent.OSArchitecture` ARM64 documentation.** Not explicitly listed in docs; fallback: `$env:PROCESSOR_ARCHITECTURE` on Windows or `uname -m` on POSIX.
- **No public ADO pipeline found** demonstrating exact `PackageDownload`-multi-RID-native-CLI pattern end-to-end. Snippet above synthesized from canonical docs.
- **`NoTargets` SDK version** should be kept current.
- **First-run `dotnet restore` warm-up cost** (~20-40s). Cache `$(NUGET_PACKAGES)` with `Cache@2`.

## Sources

- [NuGetCommand@2](https://learn.microsoft.com/en-us/azure/devops/pipelines/tasks/reference/nuget-command-v2?view=azure-pipelines)
- [DotNetCoreCLI@2](https://learn.microsoft.com/en-us/azure/devops/pipelines/tasks/reference/dotnet-core-cli-v2?view=azure-pipelines)
- [NuGetAuthenticate@1](https://learn.microsoft.com/en-us/azure/devops/pipelines/tasks/reference/nuget-authenticate-v1?view=azure-pipelines)
- [Restore NuGet packages with Azure Pipelines](https://learn.microsoft.com/en-us/azure/devops/pipelines/packages/nuget-restore?view=azure-devops)
- [Publish NuGet packages with Azure Pipelines](https://learn.microsoft.com/en-us/azure/devops/pipelines/artifacts/nuget?view=azure-devops)
- [Logging commands](https://learn.microsoft.com/en-us/azure/devops/pipelines/scripts/logging-commands?view=azure-devops)
- [Predefined variables](https://learn.microsoft.com/en-us/azure/devops/pipelines/build/variables?view=azure-devops)
- [PackageDownload](https://learn.microsoft.com/en-us/nuget/consume-packages/packagedownload-functionality)
- [Central Package Management](https://learn.microsoft.com/en-us/nuget/consume-packages/central-package-management)
- [Native files in .NET packages](https://learn.microsoft.com/en-us/nuget/create-packages/native-files-in-net-packages)
- [Global packages folder](https://learn.microsoft.com/en-us/nuget/consume-packages/managing-the-global-packages-and-cache-folders)
- [RID catalog](https://learn.microsoft.com/en-us/dotnet/core/rid-catalog)
- [dotnet tool install](https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-tool-install)
- [Andrew Lock — Self-contained .NET 10 tools](https://andrewlock.net/exploring-dotnet-10-preview-features-7-packaging-self-contained-and-native-aot-dotnet-tools-for-nuget/)
- [microsoft/winget-cli azure-pipelines.yml](https://github.com/microsoft/winget-cli/blob/master/azure-pipelines.yml)
- [LanceMcCarthy/DevOpsExamples](https://github.com/LanceMcCarthy/DevOpsExamples/blob/main/azure-pipelines.yml)
