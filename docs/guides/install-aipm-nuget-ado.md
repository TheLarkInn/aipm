# Installing `aipm` in Azure DevOps via NuGet

`aipm` is distributed on [nuget.org](https://www.nuget.org/packages/aipm) as a native multi-platform CLI package. This lets Azure DevOps pipelines acquire the binary through standard NuGet restore instead of `curl | sh`.

**Supported RIDs:**

| Runtime ID | Platform |
|---|---|
| `win-x64` | Windows (x64) |
| `linux-x64` | Linux (x64) |
| `osx-x64` | macOS (Intel) |
| `osx-arm64` | macOS (Apple Silicon) |

---

## Why NuGet instead of the shell installer?

| | Shell installer | NuGet |
|---|---|---|
| Firewall-friendly | ✗ (requires HTTPS to GitHub Releases) | ✓ (standard NuGet feed) |
| NuGet cache / artifact caching | ✗ | ✓ |
| Pin an exact version in source control | ✗ | ✓ |
| Works on `windows-latest` without WSL | ✗ (PowerShell path differs) | ✓ |
| Works behind an Artifacts upstream proxy | ✗ | ✓ |

---

## Prerequisites

- A **.NET SDK** step earlier in the pipeline (`UseDotNet@2`, version 8.x or later).
- No service connection or authentication required — nuget.org is anonymous for package restore.

---

## Step-by-step

### 1. Choose a version

Pick an `aipm` version from [nuget.org/packages/aipm](https://www.nuget.org/packages/aipm) and set it as a pipeline variable so you can bump it in one place:

```yaml
variables:
  AIPM_VERSION: '0.22.3'
  NUGET_PACKAGES: $(Pipeline.Workspace)/.nuget/packages
```

Setting `NUGET_PACKAGES` to a path inside `Pipeline.Workspace` makes the package cache eligible for [pipeline caching](https://learn.microsoft.com/en-us/azure/devops/pipelines/release/caches) (see [Caching the NuGet package](#caching-the-nuget-package) below).

### 2. Ensure the .NET SDK is available

```yaml
- task: UseDotNet@2
  inputs:
    packageType: sdk
    version: 8.x
```

### 3. Generate a download-only project

`<PackageDownload>` fetches the package without adding it as a build dependency:

```yaml
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
  displayName: 'Generate aipm download project'
```

> **Why `NoTargets`?** `Microsoft.Build.NoTargets` produces a project that can restore and evaluate `<PackageDownload>` items without needing any compilable source files. The version `3.7.0` is the latest stable as of this writing; any `3.x` release works.

> **Why brackets around the version?** `PackageDownload` only accepts exact pinned versions. The bracket notation `[1.2.3]` is the NuGet interval syntax for an exact match and is required here.

### 4. Restore the package

```yaml
- script: dotnet restore "$(Agent.TempDirectory)/aipm-fetch/fetch.csproj"
  displayName: 'Restore aipm NuGet package'
```

This downloads the nupkg to the global-packages cache at `$(NUGET_PACKAGES)/aipm/<version>/`.

### 5. Add the per-RID binary to PATH

After restore, the binary lives at `$(NUGET_PACKAGES)/aipm/<version>/runtimes/<RID>/native/aipm[.exe]`. Compute the RID from ADO's built-in `Agent.OS` and `Agent.OSArchitecture` variables:

```yaml
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
  displayName: 'Add aipm to PATH'
```

### 6. Verify the installation

```yaml
- script: aipm --version
  displayName: 'Verify aipm'
```

---

## Complete example

```yaml
variables:
  AIPM_VERSION: '0.22.3'
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
    displayName: 'Generate aipm download project'

  - script: dotnet restore "$(Agent.TempDirectory)/aipm-fetch/fetch.csproj"
    displayName: 'Restore aipm NuGet package'

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
    displayName: 'Add aipm to PATH'

  - script: aipm --version
    displayName: 'Verify aipm'

  # Example: run aipm lint and surface diagnostics in the ADO log pane
  - script: aipm lint --reporter ci-azure
    displayName: 'Lint AI plugins'
```

---

## Caching the NuGet package

Add a [Cache@2](https://learn.microsoft.com/en-us/azure/devops/pipelines/tasks/reference/cache-v2) step before the restore step to avoid re-downloading the package on every run:

```yaml
- task: Cache@2
  inputs:
    key: '"nuget" | "$(Agent.OS)" | "$(AIPM_VERSION)"'
    restoreKeys: '"nuget" | "$(Agent.OS)"'
    path: $(NUGET_PACKAGES)
  displayName: 'Cache NuGet packages'
```

Place this step **before** the `dotnet restore` step. On a cache hit the restore is a no-op.

---

## Using an Azure Artifacts upstream proxy

If your organization routes NuGet traffic through an [Azure Artifacts upstream source](https://learn.microsoft.com/en-us/azure/devops/artifacts/concepts/upstream-sources), add a `NuGetAuthenticate@1` step and point your feed URL at your Artifacts endpoint. The rest of the pipeline stays the same.

```yaml
- task: NuGetAuthenticate@1

- script: dotnet restore "$(Agent.TempDirectory)/aipm-fetch/fetch.csproj"
           --source https://pkgs.dev.azure.com/<org>/_packaging/<feed>/nuget/v3/index.json
  displayName: 'Restore aipm via Artifacts proxy'
```

> The `NuGetAuthenticate@1` task is required only for authenticated feeds. For direct nuget.org access it is unnecessary.

---

## Linting AI plugins in ADO

Once `aipm` is on the PATH, add a lint step that surfaces diagnostics natively in the ADO log pane using the `ci-azure` reporter:

```yaml
- script: aipm lint --reporter ci-azure
  displayName: 'Lint AI plugins'
```

Violations appear as collapsible per-file groups with `##vso[task.logissue]` annotations. Runs with warnings but no errors exit `0` and mark the step yellow (`SucceededWithIssues`). See [Using `aipm lint`](lint.md#azure-pipelines) for the full reporter reference.

---

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `dotnet restore` fails with `Unable to find package aipm` | Version not yet on nuget.org, or a typo | Check [nuget.org/packages/aipm](https://www.nuget.org/packages/aipm) for available versions |
| `aipm: command not found` after the PATH step | `prependpath` command executed but the next step ran before PATH refreshed | Ensure the PATH step is in a separate `pwsh` step (ADO propagates `prependpath` between steps automatically) |
| `chmod: cannot access '...': No such file or directory` | Wrong RID computed | Print `$bin` and `$o`/`$a` to verify the computed path; compare with the unpacked package layout |
| Slow pipeline on every run | NuGet cache not configured | Add the [Cache@2](#caching-the-nuget-package) step above the restore |
| Package not found via Artifacts proxy | Feed doesn't have nuget.org as an upstream source | Add `nuget.org` as an upstream source in your feed settings |

---

## See also

- [NuGet package on nuget.org](https://www.nuget.org/packages/aipm)
- [Using `aipm lint`](lint.md) — lint flags, reporters, and CI integration
- [RELEASING.md](../../RELEASING.md) — release and rollback runbook including NuGet unlist procedure
