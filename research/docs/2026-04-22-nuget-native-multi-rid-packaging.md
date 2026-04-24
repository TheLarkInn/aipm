---
date: 2026-04-22 16:14:50 UTC
researcher: Sean Larkin
git_commit: 5616fd4db5d41b77df55686365308cf12701af2a
branch: main
repository: aipm
topic: "Packaging a non-.NET native CLI binary (Rust-compiled) as a multi-RID NuGet package for Azure DevOps restore"
tags: [research, nuget, packaging, distribution, azure-devops, native, rust, multi-rid, runtimes, nuspec]
status: complete
last_updated: 2026-04-22
last_updated_by: Sean Larkin
---

# Packaging a Rust-compiled CLI as a multi-RID NuGet package

## TL;DR

- The **canonical folder convention** for native bits in a NuGet package is `runtimes/<RID>/native/<binary>`. This is a first-class layout understood by the .NET SDK and the NuGet asset-selection engine (`ManagedCodeConventions.cs`). See Microsoft Learn, "Native files in .NET packages" ([link](https://learn.microsoft.com/en-us/nuget/create-packages/native-files-in-net-packages)).
- That layout was designed for **P/Invoke scenarios** (managed wrapper DLL + per-RID native library). For a **pure native CLI** (no managed entry point) consumed by `NuGetCommand@2 restore` in Azure Pipelines, the `runtimes/<RID>/native/` folder is still a valid place to store the binaries, but NuGet will not auto-select one binary for you — you need a `build/<id>.props` (or `.targets`) to pick the right RID and surface it as an MSBuild property / add it to `PATH`, OR you treat the package like a classic `tools/` package. Microsoft's docs explicitly state the "best option" for non-P/Invoke native scenarios is to "package your own MSBuild props and targets files" ([link](https://learn.microsoft.com/en-us/nuget/create-packages/native-files-in-net-packages)).
- The closest real-world precedent to what `aipm` needs is **`Grpc.Tools`**, which ships `protoc.exe` / `protoc` / `grpc_csharp_plugin` per-RID under a `tools/<os>_<arch>/` folder (not `runtimes/`) and hooks them up via `build/_protobuf/Google.Protobuf.Tools.targets` ([link](https://github.com/grpc/grpc/blob/master/src/csharp/BUILD-INTEGRATION.md)). `Esbuild.Native.linux-x64` on nuget.org shows the other common pattern: one RID per published package ID, selected at consumer time by project RID ([link](https://www.nuget.org/packages/Esbuild.Native.linux-x64/)).
- For a pipeline-only consumer (`NuGetCommand@2 restore`, no .NET SDK required), you do **not** want `packageType=DotnetTool` (that is reserved for `dotnet tool install`, requires a managed entry point, and ties you to the .NET tool runner). Leave the package as the **default `Dependency`** type and provide a `build/<id>.targets` that sets `$(AipmToolPath)` / prepends the right `runtimes/<RID>/native/` directory to `PATH` for downstream tasks. `DotnetTool` + RID-specific tools (introduced in .NET 10) still require a .NET entry point and a `DotnetToolSettings.xml`, so it's the wrong shape for a pure Rust binary ([Andrew Lock, Apr 2025](https://andrewlock.net/exploring-dotnet-10-preview-features-7-packaging-self-contained-and-native-aot-dotnet-tools-for-nuget/); [Microsoft Learn, "Create RID-specific tools"](https://learn.microsoft.com/en-us/dotnet/core/tools/rid-specific-tools)).
- nuget.org hard limits: **250 MB per .nupkg** ([NuGet FAQ](https://learn.microsoft.com/en-us/nuget/nuget-org/nuget-org-faq)). A stripped `aipm` binary x 7 RIDs is well under this. ID `aipm` does not need prefix reservation to be published — prefix reservation is an anti-squatting / visual-indicator feature, and nuget.org explicitly warns against reserving prefixes shorter than four characters ([ID prefix reservation docs](https://learn.microsoft.com/en-us/nuget/nuget-org/id-prefix-reservation)). Claim the ID by being first to publish.
- Build the package with **`nuget pack aipm.nuspec`** (classic nuget.exe). `dotnet pack` is SDK-style-project-only; `nuget pack` on a pure `.nuspec` is still the supported path for non-.NET packages. Starting with NuGet 6.5 `nuget pack` will error on PackageReference projects, but it still packs `.nuspec` files fine ([cli-ref-pack](https://learn.microsoft.com/en-us/nuget/reference/cli-reference/cli-ref-pack)).

---

## 1. `.nuspec` schema for native multi-RID packages

### 1.1 Canonical `runtimes/<RID>/native/` layout

From the official doc ([Microsoft Learn, "Native files in .NET packages"](https://learn.microsoft.com/en-us/nuget/create-packages/native-files-in-net-packages)):

> "NuGet will select native assets from the `runtimes/{rid}/native/` directory."
>
> "The .NET SDK flattens any directory structure under `runtimes/{rid}/native/` when copying to the output directory."

For a binary that has no managed wrapper, the nupkg layout is simply:

```text
aipm.0.1.0.nupkg
 - aipm.nuspec
 - runtimes/
    - win-x64/native/aipm.exe
    - win-arm64/native/aipm.exe
    - linux-x64/native/aipm
    - linux-arm64/native/aipm
    - linux-musl-x64/native/aipm
    - osx-x64/native/aipm
    - osx-arm64/native/aipm
 - build/
    - aipm.targets        # MSBuild glue so consumers can reference $(AipmToolPath)
 - README.md
 - LICENSE.txt
```

### 1.2 `<files>` section

```xml
<files>
  <!-- per-RID native executables -->
  <file src="dist/win-x64/aipm.exe"        target="runtimes/win-x64/native/" />
  <file src="dist/win-arm64/aipm.exe"      target="runtimes/win-arm64/native/" />
  <file src="dist/linux-x64/aipm"          target="runtimes/linux-x64/native/" />
  <file src="dist/linux-arm64/aipm"        target="runtimes/linux-arm64/native/" />
  <file src="dist/linux-musl-x64/aipm"     target="runtimes/linux-musl-x64/native/" />
  <file src="dist/osx-x64/aipm"            target="runtimes/osx-x64/native/" />
  <file src="dist/osx-arm64/aipm"          target="runtimes/osx-arm64/native/" />

  <!-- MSBuild integration (convention: build/<packageId>.targets) -->
  <file src="build/aipm.targets"           target="build/" />

  <!-- docs / license -->
  <file src="README.md"                    target="" />
  <file src="LICENSE.txt"                  target="" />
</files>
```

The directly analogous pattern in the wild is `LibSassHost.Native.win-x64.nuspec`, which uses `<file src="../../lib/win-x64/libsass.dll" target="runtimes/win-x64/native/" />` ([source](https://github.com/Taritsyn/LibSassHost/blob/master/src/LibSassHost.Native.win-x64/LibSassHost.Native.win-x64.nuspec)).

### 1.3 Supported RIDs

All seven portable RIDs are defined in [`PortableRuntimeIdentifierGraph.json`](https://github.com/dotnet/runtime/blob/main/src/libraries/Microsoft.NETCore.Platforms/src/PortableRuntimeIdentifierGraph.json) in `dotnet/runtime`. From the [.NET RID catalog](https://learn.microsoft.com/en-us/dotnet/core/rid-catalog):

| RID             | Notes |
|-----------------|-------|
| `win-x64`       | Windows 10+, x64 |
| `win-arm64`     | Windows 11 on ARM |
| `linux-x64`     | Most glibc distros (Ubuntu/Debian/Fedora/...) |
| `linux-arm64`   | ARM64 glibc (Ubuntu on RPi 3+, Azure ARM VMs) |
| `linux-musl-x64`| Alpine/musl distros (used heavily in Docker base images) |
| `osx-x64`       | macOS 10.12+ on Intel (note: the doc says "osx" — NOT "macos") |
| `osx-arm64`     | Apple Silicon macOS |

Per .NET 8+ breaking changes ([rid-graph compatibility note](https://learn.microsoft.com/en-us/dotnet/core/compatibility/sdk/8.0/rid-graph)), use only **portable** (non-versioned, non-distro) RIDs. Do NOT use `ubuntu.22.04-x64`, `alpine.3.18-x64`, etc.

### 1.4 `tools/` vs `runtimes/<RID>/native/`

Three real-world patterns:

**A. `tools/<rid>/...` with MSBuild glue — used by Grpc.Tools.** De-facto industry pattern for shipping a native CLI invoked during a build.

**B. `runtimes/<RID>/native/...` with a managed stub assembly — what the official docs describe.** Designed for P/Invoke and the .NET runtime's default probing paths.

**C. One package per RID (plus an optional meta-package).** Used by `Esbuild.Native.<rid>` on nuget.org.

**Recommendation for `aipm`:** Pattern **A** (`tools/` + MSBuild targets) is cleanest for the user's constraints: no .NET SDK at install time, no P/Invoke, single package ID `aipm`. If you want the documented SDK-native layout, keep the binaries at `runtimes/<RID>/native/` and have `build/aipm.targets` resolve them there — functionally equivalent.

### 1.5 Required `<metadata>` fields for nuget.org

From the [.nuspec reference](https://learn.microsoft.com/en-us/nuget/reference/nuspec) and the [ID-prefix-reservation criteria](https://learn.microsoft.com/en-us/nuget/nuget-org/id-prefix-reservation):

**Required (nuget.org rejects the upload without these):**
- `<id>` — <= 128 chars
- `<version>` — SemVer, <= 64 chars
- `<authors>` — comma-separated
- `<description>` — <= 4000 chars

**Strongly recommended:**
- `<license type="expression">MIT</license>` — OSI/FSF-approved SPDX ID. `licenseUrl` is **deprecated**.
- `<projectUrl>` — homepage
- `<repository type="git" url="..." branch="..." commit="..." />` — links the .nupkg to its source commit; required for the nuget.org "source code" button.
- `<readme>README.md</readme>` — embedded README (Markdown only).
- `<icon>images/icon.png</icon>` — embedded 128x128 PNG/JPEG, <= 1 MB.
- `<tags>` — space-delimited
- `<releaseNotes>` — <= 35 000 chars
- `<copyright>`
- `<packageTypes>` — see section 1.6

### 1.6 `<packageTypes>` — which type for a native CLI

Canonical list ([Microsoft Learn, "Set a NuGet package type"](https://learn.microsoft.com/en-us/nuget/create-packages/set-package-type)):

| Type          | Purpose                                                                 | Fit for `aipm`?                                    |
|---------------|-------------------------------------------------------------------------|----------------------------------------------------|
| `Dependency`  | Default. Added to projects via `<PackageReference>`. **Restored by `NuGetCommand@2 restore`.** | **Yes — use this (or omit `<packageTypes>` entirely).** |
| `DotnetTool`  | For `dotnet tool install -g <id>`. Requires `tools/<tfm>/<rid>/` + `DotnetToolSettings.xml` + managed entry point. | No — requires .NET tool runner + `dotnet` CLI at install time. |
| `MSBuildSdk`  | Custom project SDK                                                       | No.                                                |
| `Template`    | `dotnet new` template                                                    | No.                                                |
| `McpServer`   | MCP server                                                               | No.                                                |

**Conclusion:** omit the `<packageTypes>` element entirely. A missing `<packageTypes>` defaults to `Dependency`, which is exactly what `NuGetCommand@2 restore` expects.

> "Packages not marked with a type, including all packages created with earlier versions of NuGet, default to the `Dependency` type." — [set-package-type docs](https://learn.microsoft.com/en-us/nuget/create-packages/set-package-type)

Do **not** invent a `NativeTool` or `AipmTool` custom type — `nuget.exe` (the engine behind `NuGetCommand@2`) will refuse to install it.

---

## 2. Build / pack step

### 2.1 `nuget pack` vs `dotnet pack`

| Command                            | Needs `.csproj`? | Works with pure `.nuspec`? | Recommended for native-only package |
|------------------------------------|------------------|----------------------------|-------------------------------------|
| `nuget pack aipm.nuspec`           | No               | Yes (canonical)            | **Yes**                              |
| `dotnet pack aipm.csproj`          | Yes              | No (via csproj flow)       | Only if you have a .NET wrapper     |
| `dotnet pack aipm.nuspec`          | Partially        | Yes, but still uses MSBuild | Works but non-canonical              |
| `msbuild -t:pack`                  | Yes              | No                         | No                                   |

The [`nuget pack` CLI reference](https://learn.microsoft.com/en-us/nuget/reference/cli-reference/cli-ref-pack) explicitly shows `nuget pack foo.nuspec` as the supported usage for non-project-based packaging.

### 2.2 Do we need a minimal C# wrapper csproj?

**No.** Precedents:
- `LibSassHost.Native.win-x64` — pure `.nuspec`, built with `nuget pack`.
- `Esbuild.Native.linux-x64` on nuget.org — native-only, no managed assembly.
- The [NNanomsg.NETStandard walkthrough](https://rendered-obsolete.github.io/2018/08/15/nupkg-with-native.html) uses `nuget pack foo.nuspec` with nothing else.

### 2.3 Recommended pack command

```bash
# Working directory contains aipm.nuspec, dist/<rid>/..., build/aipm.targets, README.md, LICENSE.txt
nuget pack aipm.nuspec \
  -OutputDirectory ./artifacts \
  -Version "0.1.0" \
  -NoDefaultExcludes \
  -NonInteractive
```

`-NoDefaultExcludes` is needed because nuget.exe by default excludes dotfiles (`.something`).

### 2.4 Complete example `aipm.nuspec`

```xml
<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://schemas.microsoft.com/packaging/2010/07/nuspec.xsd">
  <metadata>
    <id>aipm</id>
    <version>0.1.0</version>
    <authors>Sean Larkin</authors>
    <description>aipm - AI plugin manager. Manages AI plugins (Claude, Copilot, Cursor, etc.) across .claude/.github/.ai directories. Native CLI.</description>
    <license type="expression">MIT</license>
    <projectUrl>https://github.com/TheLarkInn/aipm</projectUrl>
    <repository type="git"
                url="https://github.com/TheLarkInn/aipm.git"
                branch="main"
                commit="5616fd4" />
    <readme>README.md</readme>
    <tags>ai claude copilot plugin-manager cli rust native</tags>
    <copyright>Copyright (c) 2026 Sean Larkin</copyright>
    <!-- No <packageTypes> element => defaults to Dependency -->
  </metadata>
  <files>
    <file src="dist/win-x64/aipm.exe"        target="runtimes/win-x64/native/" />
    <file src="dist/win-arm64/aipm.exe"      target="runtimes/win-arm64/native/" />
    <file src="dist/linux-x64/aipm"          target="runtimes/linux-x64/native/" />
    <file src="dist/linux-arm64/aipm"        target="runtimes/linux-arm64/native/" />
    <file src="dist/linux-musl-x64/aipm"     target="runtimes/linux-musl-x64/native/" />
    <file src="dist/osx-x64/aipm"            target="runtimes/osx-x64/native/" />
    <file src="dist/osx-arm64/aipm"          target="runtimes/osx-arm64/native/" />
    <file src="build/aipm.targets"           target="build/" />
    <file src="README.md"                    target="" />
    <file src="LICENSE.txt"                  target="" />
  </files>
</package>
```

### 2.5 Example `build/aipm.targets` (the MSBuild glue)

```xml
<Project>
  <PropertyGroup>
    <_AipmPackageDir>$(MSBuildThisFileDirectory)..\</_AipmPackageDir>

    <!-- Host-OS detection (Windows vs Unix) -->
    <_AipmOs Condition=" '$(OS)' == 'Windows_NT' ">win</_AipmOs>
    <_AipmOs Condition=" '$([MSBuild]::IsOSPlatform(`Linux`))' == 'true' ">linux</_AipmOs>
    <_AipmOs Condition=" '$([MSBuild]::IsOSPlatform(`OSX`))' == 'true' ">osx</_AipmOs>

    <!-- Arch detection -->
    <_AipmArch Condition=" '$([System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture)' == 'X64'   ">x64</_AipmArch>
    <_AipmArch Condition=" '$([System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture)' == 'Arm64' ">arm64</_AipmArch>

    <_AipmRid>$(_AipmOs)-$(_AipmArch)</_AipmRid>

    <_AipmExe Condition=" '$(_AipmOs)' == 'win' ">aipm.exe</_AipmExe>
    <_AipmExe Condition=" '$(_AipmOs)' != 'win' ">aipm</_AipmExe>

    <AipmToolPath>$(_AipmPackageDir)runtimes\$(_AipmRid)\native\$(_AipmExe)</AipmToolPath>
  </PropertyGroup>

  <Target Name="_AipmValidate" BeforeTargets="Build">
    <Error Condition="!Exists('$(AipmToolPath)')"
           Text="aipm: no binary for RID '$(_AipmRid)'. Supported: win-x64, win-arm64, linux-x64, linux-arm64, linux-musl-x64, osx-x64, osx-arm64." />
  </Target>
</Project>
```

Musl detection from MSBuild is imprecise — if Alpine is required, either ship a separate `aipm-musl` package or detect at script-step time via `ldd --version 2>&1 | grep -q musl`.

---

## 3. How the package is consumed

### 3.1 Where the binary ends up on disk after `NuGetCommand@2 restore`

Per [docs](https://learn.microsoft.com/en-us/nuget/consume-packages/managing-the-global-packages-and-cache-folders):

- Windows: `%userprofile%\.nuget\packages\<id>\<version>\`
- macOS/Linux: `~/.nuget/packages/<id>/<version>/`
- Azure Pipelines override via `NUGET_PACKAGES` env var or `restoreDirectory` input

### 3.2 Does the consumer pick the RID?

**No**, if the package ships a `build/<id>.targets` doing the detection. For a pipeline consumer, Azure DevOps exposes `$(Agent.OS)` and `$(Agent.OSArchitecture)` — a pipeline YAML can resolve the RID without MSBuild:

```yaml
- task: NuGetCommand@2
  inputs:
    command: 'restore'
    restoreSolution: 'nuget.config-backed/packages.config'
    restoreDirectory: '$(Pipeline.Workspace)/nuget'

- bash: |
    case "$(Agent.OS)-$(Agent.OSArchitecture)" in
      Linux-X64)    RID=linux-x64 ;;
      Linux-ARM64)  RID=linux-arm64 ;;
      Darwin-X64)   RID=osx-x64 ;;
      Darwin-ARM64) RID=osx-arm64 ;;
      Windows_NT-X64)   RID=win-x64 ;;
      Windows_NT-ARM64) RID=win-arm64 ;;
    esac
    AIPM="$(Pipeline.Workspace)/nuget/aipm/0.1.0/runtimes/$RID/native/aipm"
    chmod +x "$AIPM"
    "$AIPM" --version
```

### 3.3 `.props`/`.targets` marker files

Per [Microsoft Learn, "MSBuild props and targets in a package"](https://learn.microsoft.com/en-us/nuget/concepts/msbuild-props-and-targets):

The file **must** be `build/<package id>.targets` (i.e. `build/aipm.targets`) to be auto-imported by NuGet.

To prepend the binary to PATH for subsequent pipeline tasks:

```xml
<Target Name="_AipmPrependPath" BeforeTargets="Build">
  <ItemGroup>
    <_AipmDir Include="$(_AipmPackageDir)runtimes\$(_AipmRid)\native" />
  </ItemGroup>
  <Exec Command="echo ##vso[task.prependpath]@(_AipmDir)" />
</Target>
```

---

## 4. Real-world examples

### 4.1 `Esbuild.Native.<rid>` — one package per RID
- Page: https://www.nuget.org/packages/Esbuild.Native.linux-x64/
- Pattern: one package per RID, ~3.9 MB for linux-x64 v0.21.3.

### 4.2 `Grpc.Tools` — best-in-class native pipeline CLI
- Page: https://www.nuget.org/packages/grpc.tools/
- Source: https://github.com/grpc/grpc/tree/master/src/csharp/Grpc.Tools
- Integration guide: https://github.com/grpc/grpc/blob/master/src/csharp/BUILD-INTEGRATION.md
- Ships per-RID `protoc` and `grpc_csharp_plugin` under `tools/<os>_<arch>/` plus `build/Grpc.Tools.targets`.

### 4.3 `LibSassHost.Native.win-x64` — canonical `runtimes/<RID>/native/`
- Source: https://github.com/Taritsyn/LibSassHost/blob/master/src/LibSassHost.Native.win-x64/LibSassHost.Native.win-x64.nuspec
- One package per RID, placed at `runtimes/<RID>/native/` with per-package `build/<id>.props`.

### 4.4 Microsoft.PowerShell.Native
- Page: https://www.nuget.org/packages/Microsoft.PowerShell.Native/
- Ships native helpers for PowerShell Core across Windows/Linux/macOS in `runtimes/<RID>/native/`.

### 4.5 Rust-specific precedents

**No widely-adopted Rust-compiled CLI on nuget.org found as of April 2026.** The pattern is well-trodden for native binaries in general (protoc, libsass, esbuild), but `aipm` would be a pioneer for Rust -> NuGet.

---

## 5. nuget.org publishing constraints

### 5.1 Package size

From the [NuGet.org FAQ](https://learn.microsoft.com/en-us/nuget/nuget-org/nuget-org-faq):

> "NuGet.org allows packages up to 250MB, but we recommend keeping packages under 1MB if possible."

`aipm` stripped is ~5 MB per RID. Seven RIDs x 5 MB = ~35 MB per .nupkg — well under the cap.

### 5.2 Package ID reservation

From the [ID prefix reservation docs](https://learn.microsoft.com/en-us/nuget/nuget-org/id-prefix-reservation):

1. Email `account@nuget.org` with your nuget.org owner display name and the prefix you want reserved.
2. Criteria: prefix properly identifies the owner; prefix isn't too common; reservation prevents ambiguity/harm.
3. **"Avoid reservations shorter than four characters."**

For `aipm`: the ID is 4 characters — borderline. You do **not need** prefix reservation to publish — just be the first to upload `aipm`.

### 5.3 Required metadata

nuget.org rejects uploads missing: `id`, `version`, `authors`, `description`. Best-practice additions: `readme` (embedded .md), `icon` (embedded .png/.jpg), `repository` with commit SHA, `tags`, `license` as SPDX expression.

### 5.4 Other constraints

- Limits: `id` <= 128 chars; `version` <= 64; `description` <= 4000; `tags` <= 4000; `releaseNotes` <= 35 000.
- Package deletion is not self-service; only **unlisting** (hides from search; existing restores still work).
- Publishing command: `nuget push aipm.0.1.0.nupkg -Source https://api.nuget.org/v3/index.json -ApiKey <key>`.

---

## 6. Concrete recommendation for `aipm`

1. **Package type:** omit `<packageTypes>` -> defaults to `Dependency`. `NuGetCommand@2 restore` supports this out of the box.
2. **Layout:** `runtimes/<RID>/native/aipm[.exe]` for all seven target RIDs + `build/aipm.targets` + `README.md` + `LICENSE.txt` at package root.
3. **Build tool:** `nuget pack aipm.nuspec -NoDefaultExcludes` — no csproj, no managed wrapper.
4. **Consumer UX in Azure Pipelines:** restore with `NuGetCommand@2`, resolve the per-RID binary path in a bash/pwsh step using `$(Agent.OS)` + `$(Agent.OSArchitecture)`, or via `$(AipmToolPath)` if consumer uses MSBuild.
5. **Publishing:** claim `aipm` on nuget.org by publishing first.
6. **Size budget:** keep each .nupkg under ~50 MB.

---

## Gaps / open questions

- **Musl RID detection is hard from MSBuild.** Either ship separate `aipm.musl` packages or detect at bash step time.
- **Azure DevOps `NuGetCommand@2` is on maintenance-only status.** Plan for `NuGetAuthenticate@1` + `dotnet restore` as the forward-looking path.
- **No precedent for a Rust CLI on nuget.org.** First mover risk.
- **.NET 10 `RidSpecificTool`** is structurally similar but requires a managed entry point — not a fit.

---

## Key source links

- [Native files in .NET packages](https://learn.microsoft.com/en-us/nuget/create-packages/native-files-in-net-packages)
- [.NET RID catalog](https://learn.microsoft.com/en-us/dotnet/core/rid-catalog)
- [Set a NuGet package type](https://learn.microsoft.com/en-us/nuget/create-packages/set-package-type)
- [.nuspec File Reference](https://learn.microsoft.com/en-us/nuget/reference/nuspec)
- [`nuget pack` CLI reference](https://learn.microsoft.com/en-us/nuget/reference/cli-reference/cli-ref-pack)
- [MSBuild props and targets in a package](https://learn.microsoft.com/en-us/nuget/concepts/msbuild-props-and-targets)
- [NuGet.org FAQ](https://learn.microsoft.com/en-us/nuget/nuget-org/nuget-org-faq)
- [ID prefix reservation](https://learn.microsoft.com/en-us/nuget/nuget-org/id-prefix-reservation)
- [NuGetCommand@2 task reference](https://learn.microsoft.com/en-us/azure/devops/pipelines/tasks/reference/nuget-command-v2)
- [Create RID-specific, self-contained, and AOT .NET tools (.NET 10)](https://learn.microsoft.com/en-us/dotnet/core/tools/rid-specific-tools)
- [Andrew Lock — Packaging self-contained and native AOT .NET tools (.NET 10)](https://andrewlock.net/exploring-dotnet-10-preview-features-7-packaging-self-contained-and-native-aot-dotnet-tools-for-nuget/)
- [dotnet/runtime — `PortableRuntimeIdentifierGraph.json`](https://github.com/dotnet/runtime/blob/main/src/libraries/Microsoft.NETCore.Platforms/src/PortableRuntimeIdentifierGraph.json)
- [Esbuild.Native.linux-x64](https://www.nuget.org/packages/Esbuild.Native.linux-x64/)
- [Grpc.Tools](https://www.nuget.org/packages/grpc.tools/)
- [grpc/grpc BUILD-INTEGRATION.md](https://github.com/grpc/grpc/blob/master/src/csharp/BUILD-INTEGRATION.md)
- [Taritsyn/LibSassHost nuspec](https://github.com/Taritsyn/LibSassHost/blob/master/src/LibSassHost.Native.win-x64/LibSassHost.Native.win-x64.nuspec)
- [Rendered Obsolete — Nupkg Containing Native Libraries](https://rendered-obsolete.github.io/2018/08/15/nupkg-with-native.html)
- [Kyle Kukshtel — packaging native libs into NuGet](https://kylekukshtel.com/nuget-native-dll-packing)
