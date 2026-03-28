//! Installer pipeline: orchestrate resolve → fetch → store → link → lockfile.
//!
//! This module ties together all subsystems into the end-to-end install flow.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::linker;
use crate::lockfile;
use crate::manifest;
use crate::registry::Registry;
use crate::resolver;
use crate::store;
use crate::version::Version;
use crate::workspace;

use super::error::Error;
use super::manifest_editor;

/// Configuration for an install operation.
#[derive(Debug)]
pub struct InstallConfig {
    /// Path to `aipm.toml`.
    pub manifest_path: PathBuf,
    /// Path to `aipm.lock`.
    pub lockfile_path: PathBuf,
    /// Path to the global content-addressable store.
    pub store_path: PathBuf,
    /// Path to `.aipm/links/` for assembled packages.
    pub links_dir: PathBuf,
    /// Path to `claude-plugins/` (or `.ai/`) for discovery.
    pub plugins_dir: PathBuf,
    /// Path to `.gitignore` in the plugins directory.
    pub gitignore_path: PathBuf,
    /// Path to `.aipm/links.toml` for link state tracking.
    pub link_state_path: PathBuf,
    /// Path to the workspace root directory (if in a workspace).
    pub workspace_root: Option<PathBuf>,
    /// CI mode: fail on lockfile-manifest drift, skip resolution.
    pub locked: bool,
    /// Optional package to add before installing (e.g. `"pkg@^1.0"`).
    pub add_package: Option<String>,
    /// Generator string for lockfile metadata.
    pub generated_by: String,
}

/// The result of an install operation.
#[derive(Debug)]
pub struct InstallResult {
    /// Number of packages installed.
    pub installed: usize,
    /// Number of packages already up-to-date.
    pub up_to_date: usize,
    /// Number of packages removed.
    pub removed: usize,
}

/// Run the full install pipeline.
///
/// # Steps
///
/// 1. Load manifest and lockfile
/// 2. If `--locked`, validate lockfile matches manifest
/// 3. If adding a new package, update manifest
/// 4. Resolve dependencies (lockfile-first, reconciliation, or full)
/// 5. Fetch tarballs not in store
/// 6. Store extracted files
/// 7. Assemble hard-links in `.aipm/links/`
/// 8. Create directory links into plugins dir + update `.gitignore`
/// 9. If `--locked`, clear dev link overrides
/// 10. Write lockfile
///
/// # Errors
///
/// Returns [`Error`] if any step fails.
pub fn install(config: &InstallConfig, registry: &dyn Registry) -> Result<InstallResult, Error> {
    // Step 1: Load manifest
    tracing::info!(manifest = %config.manifest_path.display(), "loading manifest");
    let manifest_content = std::fs::read_to_string(&config.manifest_path)?;
    let manifest = manifest::parse_and_validate(&manifest_content, config.manifest_path.parent())
        .map_err(|e| Error::Manifest { reason: e.to_string() })?;

    // Step 3: If adding a new package, update the manifest file
    if let Some(ref spec) = config.add_package {
        let (name, version_req) = manifest_editor::parse_package_spec(spec);
        tracing::info!(
            package = name.as_str(),
            version = version_req.as_str(),
            "adding dependency to manifest"
        );
        manifest_editor::add_dependency(&config.manifest_path, &name, &version_req)?;
    }

    // Re-read manifest if we just modified it, otherwise use existing
    let manifest = if config.add_package.is_some() {
        let content = std::fs::read_to_string(&config.manifest_path)?;
        manifest::parse_and_validate(&content, config.manifest_path.parent())
            .map_err(|e| Error::Manifest { reason: e.to_string() })?
    } else {
        manifest
    };

    // Extract dependency names from manifest
    let manifest_deps = extract_dep_names(&manifest);

    // Step 1 cont: Load lockfile (if exists)
    let existing_lockfile = if config.lockfile_path.exists() {
        tracing::info!(lockfile = %config.lockfile_path.display(), "loading existing lockfile");
        Some(
            lockfile::read(&config.lockfile_path)
                .map_err(|e| Error::Manifest { reason: format!("lockfile read error: {e}") })?,
        )
    } else {
        None
    };

    // Step 2: If --locked, validate lockfile matches manifest
    if config.locked {
        let lf = existing_lockfile.as_ref().ok_or_else(|| Error::LockfileDrift {
            reason: "no lockfile found but --locked was specified".to_string(),
        })?;
        lockfile::validate_matches_manifest(lf, &manifest_deps)
            .map_err(|e| Error::LockfileDrift { reason: e.to_string() })?;
    }

    // Step 4a: Discover workspace context
    let members = discover_workspace_members(config, &manifest)?;

    // Step 4b: Load link overrides (aipm link takes priority over workspace deps)
    let link_overrides: BTreeSet<String> = if config.link_state_path.exists() {
        linker::link_state::list(&config.link_state_path)
            .map(|entries| entries.iter().map(|e| e.name.clone()).collect())
            .unwrap_or_default()
    } else {
        BTreeSet::new()
    };

    // Step 4c: Split deps into workspace vs registry
    let (workspace_dep_names, registry_deps) = split_dependencies(&manifest);

    // Step 4d: Resolve workspace deps (local version lookup + transitive)
    let workspace_resolved = if workspace_dep_names.is_empty() {
        Vec::new()
    } else {
        resolve_workspace_deps(&workspace_dep_names, &members, &link_overrides)?
    };

    // Step 4d2: Collect transitive registry deps from workspace members
    let all_registry_deps =
        collect_transitive_registry_deps(registry_deps, &workspace_resolved, &members);

    // Step 4e: Resolve registry deps (existing solver, unchanged)
    let registry_resolution = resolve_registry_dependencies(
        &all_registry_deps,
        &manifest,
        &manifest_deps,
        existing_lockfile.as_ref(),
        config.locked,
        registry,
    )?;

    // Step 4f: Merge workspace + registry resolutions
    let mut all_packages = workspace_resolved;
    all_packages.extend(registry_resolution.packages);
    let resolution = resolver::Resolution { packages: all_packages };

    // Steps 5-8: Fetch, store, and link resolved packages
    let (installed, up_to_date) = link_resolved_packages(
        config,
        &resolution,
        &members,
        existing_lockfile.as_ref(),
        registry,
    )?;

    // Handle removed packages
    let removed = handle_removals(
        existing_lockfile.as_ref(),
        &resolution,
        &config.links_dir,
        &config.plugins_dir,
        &config.gitignore_path,
    )?;

    // Step 9: If --locked, clear dev link overrides
    if config.locked {
        clear_dev_links(&config.link_state_path)?;
    }

    // Step 10: Write lockfile
    let new_lockfile = build_lockfile(&resolution, &config.generated_by);
    lockfile::write(&config.lockfile_path, &new_lockfile)
        .map_err(|e| Error::Manifest { reason: format!("lockfile write error: {e}") })?;

    tracing::info!(
        installed = installed,
        up_to_date = up_to_date,
        removed = removed,
        "install complete"
    );

    Ok(InstallResult { installed, up_to_date, removed })
}

/// Fetch, store, and link all resolved packages.
///
/// Returns `(installed, up_to_date)` counts.
fn link_resolved_packages(
    config: &InstallConfig,
    resolution: &resolver::Resolution,
    members: &BTreeMap<String, workspace::Member>,
    existing_lockfile: Option<&lockfile::types::Lockfile>,
    registry: &dyn Registry,
) -> Result<(usize, usize), Error> {
    let content_store = store::Store::new(config.store_path.clone());
    let mut installed = 0_usize;
    let mut up_to_date = 0_usize;

    for resolved in &resolution.packages {
        let pkg_name = &resolved.name;

        match &resolved.source {
            resolver::Source::Workspace => {
                if let Some(member) = members.get(pkg_name) {
                    std::fs::create_dir_all(&config.plugins_dir)?;
                    let link_target = config.plugins_dir.join(pkg_name);
                    linker::directory_link::create(&member.path, &link_target)
                        .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;
                    linker::gitignore::add_entry(&config.gitignore_path, pkg_name)
                        .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;
                    installed += 1;
                }
            },
            resolver::Source::Registry { .. } => {
                let assembled_dir = config.links_dir.join(pkg_name);
                if assembled_dir.exists() && !needs_update(resolved, existing_lockfile) {
                    tracing::debug!(package = pkg_name.as_str(), "package is up-to-date");
                    up_to_date += 1;
                    continue;
                }
                tracing::info!(package = pkg_name.as_str(), version = %resolved.version, "installing package");
                let tarball = registry.download(pkg_name, &resolved.version).map_err(|e| {
                    Error::Resolution(format!("failed to download {pkg_name}: {e}"))
                })?;
                let file_hashes = store_tarball_contents(&content_store, &tarball, pkg_name)?;
                linker::pipeline::link_package(
                    &content_store,
                    &file_hashes,
                    pkg_name,
                    &config.links_dir,
                    &config.plugins_dir,
                )
                .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;
                linker::gitignore::add_entry(&config.gitignore_path, pkg_name)
                    .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;
                installed += 1;
            },
            resolver::Source::Path { .. } => {
                tracing::warn!(
                    package = pkg_name.as_str(),
                    "path dependencies are not yet implemented — skipping"
                );
            },
        }
    }

    Ok((installed, up_to_date))
}

/// Extract direct dependency names from a manifest.
fn extract_dep_names(manifest: &manifest::types::Manifest) -> BTreeSet<String> {
    manifest.dependencies.as_ref().map(|deps| deps.keys().cloned().collect()).unwrap_or_default()
}

/// Convert manifest dependencies into resolver `Dependency` structs.
fn manifest_to_resolver_deps(manifest: &manifest::types::Manifest) -> Vec<resolver::Dependency> {
    let Some(ref deps) = manifest.dependencies else {
        return Vec::new();
    };

    deps.iter()
        .map(|(name, spec)| {
            let (req, features, default_features) = match spec {
                manifest::types::DependencySpec::Simple(v) => (v.clone(), Vec::new(), true),
                manifest::types::DependencySpec::Detailed(d) => {
                    let version = d.version.clone().unwrap_or_else(|| "*".to_string());
                    let feats = d.features.clone().unwrap_or_default();
                    let df = d.default_features.unwrap_or(true);
                    (version, feats, df)
                },
            };

            resolver::Dependency {
                name: name.clone(),
                req,
                source: "root".to_string(),
                features,
                default_features,
            }
        })
        .collect()
}

/// Partition manifest dependencies into workspace deps and registry deps.
///
/// Workspace deps have `workspace = "*"` set. Registry deps are everything else.
fn split_dependencies(
    manifest: &manifest::types::Manifest,
) -> (Vec<String>, Vec<resolver::Dependency>) {
    let Some(ref deps) = manifest.dependencies else {
        return (Vec::new(), Vec::new());
    };

    let mut workspace_dep_names = Vec::new();
    let mut registry_deps = Vec::new();

    for (name, spec) in deps {
        match spec {
            manifest::types::DependencySpec::Detailed(d) if d.workspace.is_some() => {
                workspace_dep_names.push(name.clone());
            },
            _ => {
                let (req, features, default_features) = match spec {
                    manifest::types::DependencySpec::Simple(v) => (v.clone(), Vec::new(), true),
                    manifest::types::DependencySpec::Detailed(d) => {
                        let version = d.version.clone().unwrap_or_else(|| "*".to_string());
                        let feats = d.features.clone().unwrap_or_default();
                        let df = d.default_features.unwrap_or(true);
                        (version, feats, df)
                    },
                };
                registry_deps.push(resolver::Dependency {
                    name: name.clone(),
                    req,
                    source: "root".to_string(),
                    features,
                    default_features,
                });
            },
        }
    }

    (workspace_dep_names, registry_deps)
}

/// Resolve workspace dependencies to local member versions.
///
/// For each workspace dep, looks up the member in the discovery map,
/// reads its version, and produces a `Resolved` with `Source::Workspace`.
///
/// Transitive workspace deps are resolved recursively: if member A depends
/// on member B (via `workspace = "*"`), B is also resolved as a workspace dep.
fn resolve_workspace_deps(
    workspace_dep_names: &[String],
    members: &BTreeMap<String, workspace::Member>,
    link_overrides: &BTreeSet<String>,
) -> Result<Vec<resolver::Resolved>, Error> {
    let mut resolved = Vec::new();
    let mut visited = BTreeSet::new();
    let mut queue: Vec<String> = workspace_dep_names.to_vec();

    while let Some(name) = queue.pop() {
        if visited.contains(&name) {
            continue;
        }
        visited.insert(name.clone());

        // aipm link overrides: keep the dep in the resolution (so handle_removals
        // doesn't unlink it) but use Source::Path so the linking step skips it.
        if link_overrides.contains(&name) {
            tracing::debug!(
                package = name.as_str(),
                "workspace dep has aipm link override — preserving existing link"
            );
            if let Some(member) = members.get(&name) {
                let version = Version::parse(&member.version).map_err(|e| {
                    Error::Resolution(format!(
                        "invalid version '{}' for workspace member '{}': {e}",
                        member.version, name
                    ))
                })?;
                resolved.push(resolver::Resolved {
                    name: name.clone(),
                    version,
                    source: resolver::Source::Path { path: member.path.clone() },
                    checksum: String::new(),
                    dependencies: Vec::new(),
                    features: BTreeSet::new(),
                });
            }
            continue;
        }

        let member = members.get(&name).ok_or_else(|| {
            let available: Vec<&str> = members.keys().map(String::as_str).collect();
            Error::Resolution(format!(
                "workspace dependency '{name}' not found in workspace members — available members: {available:?}"
            ))
        })?;

        // Collect transitive workspace deps from this member
        if let Some(ref deps) = member.manifest.dependencies {
            for (dep_name, dep_spec) in deps {
                if let manifest::types::DependencySpec::Detailed(d) = dep_spec {
                    if d.workspace.is_some() && !visited.contains(dep_name) {
                        queue.push(dep_name.clone());
                    }
                }
            }
        }

        // Collect dependency strings for lockfile
        let transitive_deps: Vec<String> = member
            .manifest
            .dependencies
            .as_ref()
            .map(|deps| {
                deps.iter()
                    .map(|(dep_name, spec)| {
                        let req = match spec {
                            manifest::types::DependencySpec::Simple(v) => v.clone(),
                            manifest::types::DependencySpec::Detailed(d) => {
                                if d.workspace.is_some() {
                                    return format!("{dep_name} *");
                                }
                                d.version.clone().unwrap_or_else(|| "*".to_string())
                            },
                        };
                        format!("{dep_name} {req}")
                    })
                    .collect()
            })
            .unwrap_or_default();

        let version = Version::parse(&member.version).map_err(|e| {
            Error::Resolution(format!(
                "invalid version '{}' for workspace member '{}': {e}",
                member.version, name
            ))
        })?;

        tracing::info!(
            package = name.as_str(),
            version = %version,
            path = %member.path.display(),
            "resolved workspace dependency"
        );

        resolved.push(resolver::Resolved {
            name: name.clone(),
            version,
            source: resolver::Source::Workspace,
            checksum: String::new(),
            dependencies: transitive_deps,
            features: BTreeSet::new(),
        });
    }

    Ok(resolved)
}

/// Collect transitive registry deps from resolved workspace members.
///
/// For each workspace member, any non-workspace dependency in its manifest
/// is added to the registry dep list so the solver can resolve it.
fn collect_transitive_registry_deps(
    mut registry_deps: Vec<resolver::Dependency>,
    workspace_resolved: &[resolver::Resolved],
    members: &BTreeMap<String, workspace::Member>,
) -> Vec<resolver::Dependency> {
    for resolved_ws in workspace_resolved {
        let Some(member) = members.get(&resolved_ws.name) else { continue };
        let Some(ref deps) = member.manifest.dependencies else { continue };

        for (dep_name, spec) in deps {
            if let manifest::types::DependencySpec::Detailed(d) = spec {
                if d.workspace.is_some() {
                    continue;
                }
            }
            let (req, features, default_features) = match spec {
                manifest::types::DependencySpec::Simple(v) => (v.clone(), Vec::new(), true),
                manifest::types::DependencySpec::Detailed(d) => {
                    let version = d.version.clone().unwrap_or_else(|| "*".to_string());
                    let feats = d.features.clone().unwrap_or_default();
                    let df = d.default_features.unwrap_or(true);
                    (version, feats, df)
                },
            };
            registry_deps.push(resolver::Dependency {
                name: dep_name.clone(),
                req,
                source: resolved_ws.name.clone(),
                features,
                default_features,
            });
        }
    }
    registry_deps
}

/// Discover workspace members if the manifest is in a workspace context.
fn discover_workspace_members(
    config: &InstallConfig,
    manifest: &manifest::types::Manifest,
) -> Result<BTreeMap<String, workspace::Member>, Error> {
    // If the manifest has a workspace section, use it directly
    if let Some(ref ws) = manifest.workspace {
        let ws_root = config
            .manifest_path
            .parent()
            .ok_or_else(|| Error::Manifest { reason: "manifest has no parent dir".to_string() })?;
        return workspace::discover_members(ws_root, &ws.members)
            .map_err(|e| Error::Manifest { reason: format!("workspace discovery error: {e}") });
    }

    // If workspace_root is provided, load workspace from there
    if let Some(ref ws_root) = config.workspace_root {
        let ws_manifest_path = ws_root.join("aipm.toml");
        if ws_manifest_path.exists() {
            let content = std::fs::read_to_string(&ws_manifest_path)?;
            let ws_manifest =
                manifest::parse_and_validate(&content, Some(ws_root)).map_err(|e| {
                    Error::Manifest { reason: format!("workspace manifest error: {e}") }
                })?;
            if let Some(ref ws) = ws_manifest.workspace {
                return workspace::discover_members(ws_root, &ws.members).map_err(|e| {
                    Error::Manifest { reason: format!("workspace discovery error: {e}") }
                });
            }
        }
    }

    Ok(BTreeMap::new())
}

/// Resolve only registry dependencies using the appropriate strategy.
///
/// This is the same as `resolve_dependencies` but only processes registry deps
/// (workspace deps are resolved separately).
fn resolve_registry_dependencies(
    registry_deps: &[resolver::Dependency],
    manifest: &manifest::types::Manifest,
    manifest_deps: &BTreeSet<String>,
    existing_lockfile: Option<&lockfile::types::Lockfile>,
    locked: bool,
    registry: &dyn Registry,
) -> Result<resolver::Resolution, Error> {
    // In locked mode, build resolution from lockfile (only registry packages)
    if locked {
        if let Some(lf) = existing_lockfile {
            let resolution = build_resolution_from_lockfile(lf)?;
            // Filter to only registry packages (workspace packages are handled separately)
            let registry_packages = resolution
                .packages
                .into_iter()
                .filter(|p| !matches!(&p.source, resolver::Source::Workspace))
                .collect();
            return Ok(resolver::Resolution { packages: registry_packages });
        }
    }

    // If no registry deps, return empty resolution
    if registry_deps.is_empty() {
        return Ok(resolver::Resolution { packages: Vec::new() });
    }

    // Parse overrides from manifest
    let override_rules = manifest
        .overrides
        .as_ref()
        .map(resolver::overrides::parse)
        .transpose()
        .map_err(Error::Resolution)?
        .unwrap_or_default();

    // Determine lockfile pins
    let lockfile_pins = match existing_lockfile {
        Some(lf) => {
            let recon = lockfile::reconcile::reconcile(lf, manifest_deps);
            if recon.needs_resolution.is_empty() && recon.removed.is_empty() {
                // Nothing changed — build resolution from lockfile (registry only)
                let resolution = build_resolution_from_lockfile(lf)?;
                let registry_packages = resolution
                    .packages
                    .into_iter()
                    .filter(|p| !matches!(&p.source, resolver::Source::Workspace))
                    .collect();
                return Ok(resolver::Resolution { packages: registry_packages });
            }
            build_pins(&recon.carried_forward)
        },
        None => BTreeMap::new(),
    };

    resolver::resolve_with_overrides(registry_deps, &lockfile_pins, registry, &override_rules)
        .map_err(|e| Error::Resolution(e.to_string()))
}

/// Build lockfile pins (name → version) from carried-forward packages.
fn build_pins(packages: &[lockfile::types::Package]) -> BTreeMap<String, Version> {
    let mut pins = BTreeMap::new();
    for pkg in packages {
        if let Ok(v) = Version::parse(&pkg.version) {
            pins.insert(pkg.name.clone(), v);
        }
    }
    pins
}

/// Build a resolution directly from the lockfile (for --locked mode or unchanged deps).
fn build_resolution_from_lockfile(
    lf: &lockfile::types::Lockfile,
) -> Result<resolver::Resolution, Error> {
    let packages = lf
        .packages
        .iter()
        .map(|pkg| {
            let version = Version::parse(&pkg.version).map_err(|e| Error::Manifest {
                reason: format!("invalid version in lockfile: {e}"),
            })?;

            let source = if pkg.source == "workspace" {
                resolver::Source::Workspace
            } else if let Some(path) = pkg.source.strip_prefix("path+") {
                resolver::Source::Path { path: PathBuf::from(path) }
            } else {
                let url = pkg.source.strip_prefix("git+").unwrap_or(&pkg.source);
                resolver::Source::Registry { index_url: url.to_string() }
            };

            Ok(resolver::Resolved {
                name: pkg.name.clone(),
                version,
                source,
                checksum: pkg.checksum.clone(),
                dependencies: pkg.dependencies.clone(),
                features: BTreeSet::new(),
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;

    Ok(resolver::Resolution { packages })
}

/// Check if a resolved package differs from the lockfile version.
fn needs_update(
    resolved: &resolver::Resolved,
    lockfile: Option<&lockfile::types::Lockfile>,
) -> bool {
    let Some(lf) = lockfile else { return true };
    let found = lf.packages.iter().find(|p| p.name == resolved.name);
    found.is_none_or(|locked| {
        let version_str = format!("{}", resolved.version);
        locked.version != version_str || locked.checksum != resolved.checksum
    })
}

/// Store tarball contents into the content-addressable store.
///
/// Extracts the tarball to a temp directory, then stores all files.
fn store_tarball_contents(
    content_store: &store::Store,
    tarball: &[u8],
    pkg_name: &str,
) -> Result<BTreeMap<PathBuf, String>, Error> {
    // Create a unique temporary directory for extraction
    let tmp_dir = tempfile::tempdir().map_err(|e| {
        Error::Io(std::io::Error::other(format!("failed to create temp dir for {pkg_name}: {e}")))
    })?;

    // Extract tarball (gzip-compressed tar)
    let decoder = flate2::read::GzDecoder::new(tarball);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(tmp_dir.path()).map_err(|e| {
        Error::Io(std::io::Error::other(format!("failed to extract tarball for {pkg_name}: {e}")))
    })?;

    // Store all extracted files in the content store
    let file_hashes = content_store.store_package(tmp_dir.path()).map_err(|e| {
        Error::Io(std::io::Error::other(format!("failed to store package {pkg_name}: {e}")))
    })?;

    // tmp_dir is cleaned up automatically on drop

    Ok(file_hashes)
}

/// Handle removal of packages that were in the old lockfile but not in the new resolution.
fn handle_removals(
    existing_lockfile: Option<&lockfile::types::Lockfile>,
    resolution: &resolver::Resolution,
    links_dir: &Path,
    plugins_dir: &Path,
    gitignore_path: &Path,
) -> Result<usize, Error> {
    let Some(lf) = existing_lockfile else { return Ok(0) };

    let new_names: BTreeSet<&str> = resolution.packages.iter().map(|p| p.name.as_str()).collect();
    let mut removed = 0_usize;

    for pkg in &lf.packages {
        if !new_names.contains(pkg.name.as_str()) {
            tracing::info!(package = pkg.name.as_str(), "removing package");
            linker::pipeline::unlink_package(&pkg.name, links_dir, plugins_dir)
                .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;

            linker::gitignore::remove_entry(gitignore_path, &pkg.name)
                .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;

            removed += 1;
        }
    }

    Ok(removed)
}

/// Clear all dev link overrides (for --locked mode).
fn clear_dev_links(link_state_path: &Path) -> Result<(), Error> {
    if link_state_path.exists() {
        let entries = linker::link_state::list(link_state_path)
            .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;

        for link in &entries {
            tracing::warn!(
                package = link.name.as_str(),
                path = %link.path.display(),
                "removing dev link override in --locked mode"
            );
        }

        linker::link_state::clear_all(link_state_path)
            .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;
    }
    Ok(())
}

/// Build a new lockfile from the resolution.
fn build_lockfile(
    resolution: &resolver::Resolution,
    generated_by: &str,
) -> lockfile::types::Lockfile {
    let packages = resolution
        .packages
        .iter()
        .map(|resolved| {
            let source = match &resolved.source {
                resolver::Source::Registry { index_url } => format!("git+{index_url}"),
                resolver::Source::Workspace => "workspace".to_string(),
                resolver::Source::Path { path } => format!("path+{}", path.display()),
            };

            lockfile::types::Package {
                name: resolved.name.clone(),
                version: format!("{}", resolved.version),
                source,
                checksum: resolved.checksum.clone(),
                dependencies: resolved.dependencies.clone(),
            }
        })
        .collect();

    lockfile::types::Lockfile {
        metadata: lockfile::types::Metadata {
            lockfile_version: lockfile::types::LOCKFILE_VERSION,
            generated_by: generated_by.to_string(),
        },
        packages,
    }
}

/// Configuration for an update operation.
#[derive(Debug)]
pub struct UpdateConfig {
    /// Path to `aipm.toml`.
    pub manifest_path: PathBuf,
    /// Path to `aipm.lock`.
    pub lockfile_path: PathBuf,
    /// Path to the global content-addressable store.
    pub store_path: PathBuf,
    /// Path to `.aipm/links/` for assembled packages.
    pub links_dir: PathBuf,
    /// Path to `claude-plugins/` (or `.ai/`) for discovery.
    pub plugins_dir: PathBuf,
    /// Path to `.gitignore` in the plugins directory.
    pub gitignore_path: PathBuf,
    /// Path to `.aipm/links.toml` for link state tracking.
    pub link_state_path: PathBuf,
    /// Optional specific package to update (None = update all).
    pub package: Option<String>,
    /// Generator string for lockfile metadata.
    pub generated_by: String,
}

/// Run the update pipeline.
///
/// If `package` is specified, only re-resolve that dependency.
/// If `package` is `None`, re-resolve all dependencies (discard all pins).
///
/// # Errors
///
/// Returns [`Error`] if any step fails.
pub fn update(config: &UpdateConfig, registry: &dyn Registry) -> Result<InstallResult, Error> {
    // Load manifest
    tracing::info!(manifest = %config.manifest_path.display(), "loading manifest for update");
    let manifest_content = std::fs::read_to_string(&config.manifest_path)?;
    let manifest = manifest::parse_and_validate(&manifest_content, config.manifest_path.parent())
        .map_err(|e| Error::Manifest { reason: e.to_string() })?;

    // Load existing lockfile
    let existing_lockfile = if config.lockfile_path.exists() {
        Some(
            lockfile::read(&config.lockfile_path)
                .map_err(|e| Error::Manifest { reason: format!("lockfile read error: {e}") })?,
        )
    } else {
        None
    };

    // Build lockfile pins, excluding packages being updated
    let lockfile_pins = match (&existing_lockfile, &config.package) {
        (Some(lf), Some(pkg_name)) => {
            // Targeted update: remove the specific package pin so it gets re-resolved
            tracing::info!(package = pkg_name.as_str(), "re-resolving targeted package");
            let mut pins = build_pins(&lf.packages);
            pins.remove(pkg_name);
            pins
        },
        (Some(_), None) => {
            // Full update: discard all pins
            tracing::info!("re-resolving all dependencies");
            BTreeMap::new()
        },
        _ => BTreeMap::new(),
    };

    // Resolve dependencies with the adjusted pins
    let root_deps = manifest_to_resolver_deps(&manifest);
    let override_rules = manifest
        .overrides
        .as_ref()
        .map(resolver::overrides::parse)
        .transpose()
        .map_err(Error::Resolution)?
        .unwrap_or_default();

    let resolution =
        resolver::resolve_with_overrides(&root_deps, &lockfile_pins, registry, &override_rules)
            .map_err(|e| Error::Resolution(e.to_string()))?;

    // Use the same install steps for fetch → store → link
    let content_store = store::Store::new(config.store_path.clone());
    let mut installed = 0_usize;
    let mut up_to_date = 0_usize;

    for resolved in &resolution.packages {
        let pkg_name = &resolved.name;
        let assembled_dir = config.links_dir.join(pkg_name);

        if assembled_dir.exists() && !needs_update(resolved, existing_lockfile.as_ref()) {
            tracing::debug!(package = pkg_name.as_str(), "package is up-to-date");
            up_to_date += 1;
            continue;
        }

        tracing::info!(
            package = pkg_name.as_str(),
            version = %resolved.version,
            "updating package"
        );

        let tarball = registry
            .download(pkg_name, &resolved.version)
            .map_err(|e| Error::Resolution(format!("failed to download {pkg_name}: {e}")))?;

        let file_hashes = store_tarball_contents(&content_store, &tarball, pkg_name)?;

        linker::pipeline::link_package(
            &content_store,
            &file_hashes,
            pkg_name,
            &config.links_dir,
            &config.plugins_dir,
        )
        .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;

        linker::gitignore::add_entry(&config.gitignore_path, pkg_name)
            .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;

        installed += 1;
    }

    // Handle removals
    let removed = handle_removals(
        existing_lockfile.as_ref(),
        &resolution,
        &config.links_dir,
        &config.plugins_dir,
        &config.gitignore_path,
    )?;

    // Write updated lockfile
    let new_lockfile = build_lockfile(&resolution, &config.generated_by);
    lockfile::write(&config.lockfile_path, &new_lockfile)
        .map_err(|e| Error::Manifest { reason: format!("lockfile write error: {e}") })?;

    tracing::info!(
        installed = installed,
        up_to_date = up_to_date,
        removed = removed,
        "update complete"
    );

    Ok(InstallResult { installed, up_to_date, removed })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{self, PackageMetadata, VersionEntry};
    use crate::version::Version;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    /// A mock registry for testing the install pipeline.
    struct MockRegistry {
        packages: BTreeMap<String, Vec<VersionEntry>>,
        downloads: Mutex<Vec<(String, String)>>,
    }

    impl MockRegistry {
        fn new() -> Self {
            Self { packages: BTreeMap::new(), downloads: Mutex::new(Vec::new()) }
        }

        fn add_package(&mut self, name: &str, version: &str, deps: Vec<registry::DepEntry>) {
            let entry = VersionEntry {
                name: name.to_string(),
                vers: version.to_string(),
                deps,
                cksum: format!("sha512-{name}-{version}"),
                features: BTreeMap::new(),
                yanked: false,
            };
            self.packages.entry(name.to_string()).or_default().push(entry);
        }
    }

    impl Registry for MockRegistry {
        fn get_metadata(&self, name: &str) -> Result<PackageMetadata, registry::error::Error> {
            self.packages
                .get(name)
                .map(|versions| PackageMetadata {
                    name: name.to_string(),
                    versions: versions.clone(),
                })
                .ok_or_else(|| registry::error::Error::PackageNotFound { name: name.to_string() })
        }

        fn download(
            &self,
            name: &str,
            version: &Version,
        ) -> Result<Vec<u8>, registry::error::Error> {
            if let Ok(mut downloads) = self.downloads.lock() {
                downloads.push((name.to_string(), format!("{version}")));
            }

            // Create a minimal valid gzip-compressed tar archive
            let mut archive_buf = Vec::new();
            {
                let encoder =
                    flate2::write::GzEncoder::new(&mut archive_buf, flate2::Compression::fast());
                let mut builder = tar::Builder::new(encoder);

                // Add a simple aipm.toml file
                let content = format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\n");
                let mut header = tar::Header::new_gnu();
                header.set_size(content.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder
                    .append_data(&mut header, "aipm.toml", content.as_bytes())
                    .map_err(|e| registry::error::Error::Io { reason: e.to_string() })?;

                builder
                    .finish()
                    .map_err(|e| registry::error::Error::Io { reason: e.to_string() })?;
            }

            Ok(archive_buf)
        }
    }

    fn setup_project(tmp: &Path) -> InstallConfig {
        let manifest_path = tmp.join("aipm.toml");
        let manifest_content = r#"[package]
name = "test-project"
version = "0.1.0"

[dependencies]
pkg-a = "^1.0"
"#;
        std::fs::write(&manifest_path, manifest_content).expect("write manifest");

        InstallConfig {
            manifest_path,
            lockfile_path: tmp.join("aipm.lock"),
            store_path: tmp.join(".aipm/store"),
            links_dir: tmp.join(".aipm/links"),
            plugins_dir: tmp.join("claude-plugins"),
            gitignore_path: tmp.join("claude-plugins/.gitignore"),
            link_state_path: tmp.join(".aipm/links.toml"),
            workspace_root: None,
            locked: false,
            add_package: None,
            generated_by: "aipm-test 0.1.0".to_string(),
        }
    }

    fn make_registry() -> MockRegistry {
        let mut reg = MockRegistry::new();
        reg.add_package("pkg-a", "1.0.0", vec![]);
        reg.add_package("pkg-a", "1.1.0", vec![]);
        reg.add_package(
            "pkg-b",
            "2.0.0",
            vec![registry::DepEntry {
                name: "pkg-a".to_string(),
                req: "^1.0".to_string(),
                features: vec![],
                optional: false,
                default_features: true,
            }],
        );
        reg
    }

    #[test]
    fn extract_dep_names_from_manifest() {
        let toml_str = r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
foo = "^1.0"
bar = "^2.0"
"#;
        let m = manifest::parse(toml_str).unwrap();
        let names = extract_dep_names(&m);
        assert_eq!(names.len(), 2);
        assert!(names.contains("foo"));
        assert!(names.contains("bar"));
    }

    #[test]
    fn extract_dep_names_no_dependencies() {
        let toml_str = r#"
[package]
name = "test"
version = "0.1.0"
"#;
        let m = manifest::parse(toml_str).unwrap();
        let names = extract_dep_names(&m);
        assert!(names.is_empty());
    }

    #[test]
    fn manifest_to_resolver_deps_simple() {
        let toml_str = r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
foo = "^1.0"
"#;
        let m = manifest::parse(toml_str).unwrap();
        let deps = manifest_to_resolver_deps(&m);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "foo");
        assert_eq!(deps[0].req, "^1.0");
        assert_eq!(deps[0].source, "root");
        assert!(deps[0].default_features);
    }

    #[test]
    fn manifest_to_resolver_deps_detailed() {
        let toml_str = r#"
[package]
name = "test"
version = "0.1.0"

[dependencies.bar]
version = "^2.0"
default-features = false
features = ["json"]
"#;
        let m = manifest::parse(toml_str).unwrap();
        let deps = manifest_to_resolver_deps(&m);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "bar");
        assert_eq!(deps[0].req, "^2.0");
        assert!(!deps[0].default_features);
        assert_eq!(deps[0].features, vec!["json".to_string()]);
    }

    #[test]
    fn build_lockfile_from_resolution() {
        let resolution = resolver::Resolution {
            packages: vec![resolver::Resolved {
                name: "test-pkg".to_string(),
                version: Version::parse("1.2.3").unwrap(),
                source: resolver::Source::Registry {
                    index_url: "https://github.com/org/reg.git".to_string(),
                },
                checksum: "sha512-abc".to_string(),
                dependencies: vec!["dep-a ^1.0".to_string()],
                features: BTreeSet::new(),
            }],
        };

        let lf = build_lockfile(&resolution, "aipm 0.10.0");
        assert_eq!(lf.metadata.lockfile_version, 1);
        assert_eq!(lf.metadata.generated_by, "aipm 0.10.0");
        assert_eq!(lf.packages.len(), 1);
        assert_eq!(lf.packages[0].name, "test-pkg");
        assert_eq!(lf.packages[0].version, "1.2.3");
        assert_eq!(lf.packages[0].source, "git+https://github.com/org/reg.git");
        assert_eq!(lf.packages[0].checksum, "sha512-abc");
    }

    #[test]
    fn build_lockfile_workspace_source() {
        let resolution = resolver::Resolution {
            packages: vec![resolver::Resolved {
                name: "ws-pkg".to_string(),
                version: Version::parse("0.1.0").unwrap(),
                source: resolver::Source::Workspace,
                checksum: String::new(),
                dependencies: vec![],
                features: BTreeSet::new(),
            }],
        };

        let lf = build_lockfile(&resolution, "test");
        assert_eq!(lf.packages[0].source, "workspace");
    }

    #[test]
    fn build_lockfile_path_source() {
        let resolution = resolver::Resolution {
            packages: vec![resolver::Resolved {
                name: "path-pkg".to_string(),
                version: Version::parse("0.1.0").unwrap(),
                source: resolver::Source::Path { path: PathBuf::from("/dev/plugin") },
                checksum: String::new(),
                dependencies: vec![],
                features: BTreeSet::new(),
            }],
        };

        let lf = build_lockfile(&resolution, "test");
        assert_eq!(lf.packages[0].source, "path+/dev/plugin");
    }

    #[test]
    fn build_resolution_from_lockfile_round_trip() {
        let lf = lockfile::types::Lockfile {
            metadata: lockfile::types::Metadata {
                lockfile_version: 1,
                generated_by: "test".to_string(),
            },
            packages: vec![
                lockfile::types::Package {
                    name: "pkg-a".to_string(),
                    version: "1.0.0".to_string(),
                    source: "git+https://example.com".to_string(),
                    checksum: "sha512-abc".to_string(),
                    dependencies: vec![],
                },
                lockfile::types::Package {
                    name: "ws-pkg".to_string(),
                    version: "0.1.0".to_string(),
                    source: "workspace".to_string(),
                    checksum: String::new(),
                    dependencies: vec![],
                },
            ],
        };

        let resolution = build_resolution_from_lockfile(&lf).unwrap();
        assert_eq!(resolution.packages.len(), 2);
        assert_eq!(resolution.packages[0].name, "pkg-a");
        assert_eq!(resolution.packages[1].name, "ws-pkg");
        assert!(matches!(resolution.packages[1].source, resolver::Source::Workspace));
    }

    #[test]
    fn needs_update_detects_version_change() {
        let resolved = resolver::Resolved {
            name: "pkg-a".to_string(),
            version: Version::parse("2.0.0").unwrap(),
            source: resolver::Source::Registry { index_url: "test".to_string() },
            checksum: "sha512-new".to_string(),
            dependencies: vec![],
            features: BTreeSet::new(),
        };

        let lf = lockfile::types::Lockfile {
            metadata: lockfile::types::Metadata {
                lockfile_version: 1,
                generated_by: "test".to_string(),
            },
            packages: vec![lockfile::types::Package {
                name: "pkg-a".to_string(),
                version: "1.0.0".to_string(),
                source: "git+test".to_string(),
                checksum: "sha512-old".to_string(),
                dependencies: vec![],
            }],
        };

        assert!(needs_update(&resolved, Some(&lf)));
    }

    #[test]
    fn needs_update_false_when_same() {
        let resolved = resolver::Resolved {
            name: "pkg-a".to_string(),
            version: Version::parse("1.0.0").unwrap(),
            source: resolver::Source::Registry { index_url: "test".to_string() },
            checksum: "sha512-abc".to_string(),
            dependencies: vec![],
            features: BTreeSet::new(),
        };

        let lf = lockfile::types::Lockfile {
            metadata: lockfile::types::Metadata {
                lockfile_version: 1,
                generated_by: "test".to_string(),
            },
            packages: vec![lockfile::types::Package {
                name: "pkg-a".to_string(),
                version: "1.0.0".to_string(),
                source: "git+test".to_string(),
                checksum: "sha512-abc".to_string(),
                dependencies: vec![],
            }],
        };

        assert!(!needs_update(&resolved, Some(&lf)));
    }

    #[test]
    fn needs_update_true_when_no_lockfile() {
        let resolved = resolver::Resolved {
            name: "pkg-a".to_string(),
            version: Version::parse("1.0.0").unwrap(),
            source: resolver::Source::Registry { index_url: "test".to_string() },
            checksum: "sha512-abc".to_string(),
            dependencies: vec![],
            features: BTreeSet::new(),
        };

        assert!(needs_update(&resolved, None));
    }

    #[test]
    fn install_creates_lockfile() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let config = setup_project(tmp.path());
        let registry = make_registry();

        let result = install(&config, &registry);
        assert!(result.is_ok(), "install failed: {result:?}");

        // Lockfile should exist
        assert!(config.lockfile_path.exists());

        // Read back and verify
        let lf = lockfile::read(&config.lockfile_path).unwrap();
        assert!(!lf.packages.is_empty());
    }

    #[test]
    fn install_fails_when_package_not_in_registry() {
        // Covers the `?` error-propagation branch of resolve_dependencies (line 129)
        let tmp = tempfile::tempdir().expect("tempdir");
        // Write a manifest that requires a package NOT in the registry
        let manifest_path = tmp.path().join("aipm.toml");
        std::fs::write(
            &manifest_path,
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n\n[dependencies]\nmissing-pkg = \"^1.0\"\n",
        )
        .expect("write");

        let config = InstallConfig {
            manifest_path,
            lockfile_path: tmp.path().join("aipm.lock"),
            store_path: tmp.path().join(".aipm/store"),
            links_dir: tmp.path().join(".aipm/links"),
            plugins_dir: tmp.path().join("claude-plugins"),
            gitignore_path: tmp.path().join("claude-plugins/.gitignore"),
            link_state_path: tmp.path().join(".aipm/links.toml"),
            workspace_root: None,
            locked: false,
            add_package: None,
            generated_by: "aipm-test 0.1.0".to_string(),
        };
        let registry = make_registry(); // doesn't have "missing-pkg"

        let result = install(&config, &registry);
        assert!(result.is_err());
    }

    #[test]
    fn install_locked_fails_without_lockfile() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut config = setup_project(tmp.path());
        config.locked = true;
        let registry = make_registry();

        let result = install(&config, &registry);
        assert!(result.is_err());
        let err = format!("{}", result.err().unwrap());
        assert!(err.contains("lockfile") || err.contains("locked"));
    }

    #[test]
    fn install_locked_fails_on_drift() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut config = setup_project(tmp.path());
        config.locked = true;

        // Write a lockfile that's missing "pkg-a"
        let lf = lockfile::types::Lockfile::new("test".to_string());
        lockfile::write(&config.lockfile_path, &lf).unwrap();

        let registry = make_registry();
        let result = install(&config, &registry);
        assert!(result.is_err());
    }

    #[test]
    fn build_pins_from_packages() {
        let packages = vec![
            lockfile::types::Package {
                name: "a".to_string(),
                version: "1.2.3".to_string(),
                source: "git+test".to_string(),
                checksum: "".to_string(),
                dependencies: vec![],
            },
            lockfile::types::Package {
                name: "b".to_string(),
                version: "invalid".to_string(),
                source: "git+test".to_string(),
                checksum: "".to_string(),
                dependencies: vec![],
            },
        ];

        let pins = build_pins(&packages);
        assert_eq!(pins.len(), 1);
        assert!(pins.contains_key("a"));
    }

    #[test]
    fn install_with_add_package() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut config = setup_project(tmp.path());
        config.add_package = Some("pkg-b@^2.0".to_string());
        let registry = make_registry();

        let result = install(&config, &registry);
        assert!(result.is_ok(), "install failed: {result:?}");

        // Manifest should contain pkg-b
        let content = std::fs::read_to_string(&config.manifest_path).expect("read");
        assert!(content.contains("pkg-b"));
    }

    #[test]
    fn clear_dev_links_when_no_state_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let state_path = tmp.path().join("links.toml");
        // Should be a no-op when file doesn't exist
        assert!(clear_dev_links(&state_path).is_ok());
    }

    #[test]
    fn clear_dev_links_with_existing_entries() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let state_path = tmp.path().join(".aipm/links.toml");

        // Create a state file with entries so the warn loop executes
        let entry = linker::link_state::LinkEntry {
            name: "my-plugin".to_string(),
            path: std::path::PathBuf::from("/work/my-plugin"),
            linked_at: "2026-03-26T12:00:00Z".to_string(),
        };
        linker::link_state::add(&state_path, entry).expect("add entry");

        assert!(clear_dev_links(&state_path).is_ok());

        // Entries should be cleared
        let remaining = linker::link_state::list(&state_path).expect("list");
        assert!(remaining.is_empty());
    }

    #[test]
    fn handle_removals_no_existing_lockfile() {
        let resolution = resolver::Resolution { packages: vec![] };
        let result = handle_removals(
            None,
            &resolution,
            Path::new("/tmp/links"),
            Path::new("/tmp/plugins"),
            Path::new("/tmp/.gitignore"),
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    fn make_update_config(tmp: &Path) -> UpdateConfig {
        let manifest_path = tmp.join("aipm.toml");
        let manifest_content = r#"[package]
name = "test-project"
version = "0.1.0"

[dependencies]
pkg-a = "^1.0"
"#;
        std::fs::write(&manifest_path, manifest_content).expect("write manifest");

        UpdateConfig {
            manifest_path,
            lockfile_path: tmp.join("aipm.lock"),
            store_path: tmp.join(".aipm/store"),
            links_dir: tmp.join(".aipm/links"),
            plugins_dir: tmp.join("claude-plugins"),
            gitignore_path: tmp.join("claude-plugins/.gitignore"),
            link_state_path: tmp.join(".aipm/links.toml"),
            package: None,
            generated_by: "aipm-test 0.1.0".to_string(),
        }
    }

    #[test]
    fn update_all_without_lockfile() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let config = make_update_config(tmp.path());
        let registry = make_registry();

        let result = update(&config, &registry);
        assert!(result.is_ok(), "update failed: {result:?}");

        // Should create lockfile
        assert!(config.lockfile_path.exists());
    }

    #[test]
    fn update_targeted_package() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut config = make_update_config(tmp.path());
        config.package = Some("pkg-a".to_string());
        let registry = make_registry();

        // First install
        let install_config = setup_project(tmp.path());
        let _ = install(&install_config, &registry);

        // Now update pkg-a
        let result = update(&config, &registry);
        assert!(result.is_ok(), "update failed: {result:?}");
    }

    #[test]
    fn update_full_re_resolves_all() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let config = make_update_config(tmp.path());
        let registry = make_registry();

        // First install
        let install_config = setup_project(tmp.path());
        let _ = install(&install_config, &registry);

        // Full update (no specific package)
        let result = update(&config, &registry);
        assert!(result.is_ok(), "update failed: {result:?}");

        // Lockfile should be updated
        let lf = lockfile::read(&config.lockfile_path).unwrap();
        assert!(!lf.packages.is_empty());
    }

    #[test]
    fn needs_update_true_when_package_not_in_lockfile() {
        // Package exists in resolution but NOT in the lockfile — needs_update returns true
        let resolved = resolver::Resolved {
            name: "new-pkg".to_string(),
            version: Version::parse("1.0.0").unwrap(),
            source: resolver::Source::Registry { index_url: "test".to_string() },
            checksum: "sha512-abc".to_string(),
            dependencies: vec![],
            features: BTreeSet::new(),
        };

        // Lockfile contains a different package
        let lf = lockfile::types::Lockfile {
            metadata: lockfile::types::Metadata {
                lockfile_version: 1,
                generated_by: "test".to_string(),
            },
            packages: vec![lockfile::types::Package {
                name: "other-pkg".to_string(),
                version: "1.0.0".to_string(),
                source: "git+test".to_string(),
                checksum: "sha512-abc".to_string(),
                dependencies: vec![],
            }],
        };

        // new-pkg not in lockfile → needs_update should return true
        assert!(needs_update(&resolved, Some(&lf)));
    }

    #[test]
    fn needs_update_true_when_checksum_differs() {
        let resolved = resolver::Resolved {
            name: "pkg-a".to_string(),
            version: Version::parse("1.0.0").unwrap(),
            source: resolver::Source::Registry { index_url: "test".to_string() },
            checksum: "sha512-new".to_string(), // different checksum
            dependencies: vec![],
            features: BTreeSet::new(),
        };

        let lf = lockfile::types::Lockfile {
            metadata: lockfile::types::Metadata {
                lockfile_version: 1,
                generated_by: "test".to_string(),
            },
            packages: vec![lockfile::types::Package {
                name: "pkg-a".to_string(),
                version: "1.0.0".to_string(),
                source: "git+test".to_string(),
                checksum: "sha512-old".to_string(), // old checksum
                dependencies: vec![],
            }],
        };

        assert!(needs_update(&resolved, Some(&lf)));
    }

    #[test]
    fn build_resolution_from_lockfile_path_source() {
        // Cover the path+ branch in build_resolution_from_lockfile
        let lf = lockfile::types::Lockfile {
            metadata: lockfile::types::Metadata {
                lockfile_version: 1,
                generated_by: "test".to_string(),
            },
            packages: vec![lockfile::types::Package {
                name: "path-pkg".to_string(),
                version: "0.1.0".to_string(),
                source: "path+/local/plugin".to_string(),
                checksum: String::new(),
                dependencies: vec![],
            }],
        };

        let resolution = build_resolution_from_lockfile(&lf).unwrap();
        assert_eq!(resolution.packages.len(), 1);
        assert!(matches!(
            &resolution.packages[0].source,
            resolver::Source::Path { path } if path.to_string_lossy() == "/local/plugin"
        ));
    }

    #[test]
    fn build_resolution_from_lockfile_invalid_version_errors() {
        let lf = lockfile::types::Lockfile {
            metadata: lockfile::types::Metadata {
                lockfile_version: 1,
                generated_by: "test".to_string(),
            },
            packages: vec![lockfile::types::Package {
                name: "bad-pkg".to_string(),
                version: "not-a-version".to_string(),
                source: "git+test".to_string(),
                checksum: String::new(),
                dependencies: vec![],
            }],
        };

        let result = build_resolution_from_lockfile(&lf);
        assert!(result.is_err());
    }

    #[test]
    fn manifest_to_resolver_deps_detailed_no_version() {
        // Detailed dep with no version field should default to "*"
        let toml_str = r#"
[package]
name = "test"
version = "0.1.0"

[dependencies.foo]
features = ["extra"]
"#;
        let m = manifest::parse(toml_str).unwrap();
        let deps = manifest_to_resolver_deps(&m);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].req, "*");
        assert_eq!(deps[0].features, vec!["extra".to_string()]);
    }

    #[test]
    fn install_locked_clears_dev_links() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut config = setup_project(tmp.path());
        let registry = make_registry();

        // First do an unlocked install to create the lockfile
        let result = install(&config, &registry);
        assert!(result.is_ok(), "first install failed: {result:?}");

        // Add a link entry to the state file
        let link_entry = crate::linker::link_state::LinkEntry {
            name: "dev-pkg".to_string(),
            path: std::path::PathBuf::from("/dev/path"),
            linked_at: "2026-01-01T00:00:00Z".to_string(),
        };
        crate::linker::link_state::add(&config.link_state_path, link_entry).unwrap();

        // Verify entry was added
        let entries = crate::linker::link_state::list(&config.link_state_path).unwrap();
        assert_eq!(entries.len(), 1);

        // Now install in --locked mode — should clear dev links
        config.locked = true;
        let result = install(&config, &registry);
        assert!(result.is_ok(), "locked install failed: {result:?}");

        // Dev links should be cleared
        let entries = crate::linker::link_state::list(&config.link_state_path).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn install_with_manifest_having_overrides() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let manifest_path = tmp.path().join("aipm.toml");
        let manifest_content = r#"[package]
name = "test-project"
version = "0.1.0"

[dependencies]
pkg-a = "^1.0"

[overrides]
pkg-a = "^1.0"
"#;
        std::fs::write(&manifest_path, manifest_content).expect("write manifest");

        let config = InstallConfig {
            manifest_path,
            lockfile_path: tmp.path().join("aipm.lock"),
            store_path: tmp.path().join(".aipm/store"),
            links_dir: tmp.path().join(".aipm/links"),
            plugins_dir: tmp.path().join("claude-plugins"),
            gitignore_path: tmp.path().join("claude-plugins/.gitignore"),
            link_state_path: tmp.path().join(".aipm/links.toml"),
            workspace_root: None,
            locked: false,
            add_package: None,
            generated_by: "aipm-test 0.1.0".to_string(),
        };

        let registry = make_registry();
        let result = install(&config, &registry);
        assert!(result.is_ok(), "install with overrides failed: {result:?}");
    }

    #[test]
    fn resolve_dependencies_locked_with_lockfile_uses_it() {
        // When locked=true and a lockfile exists, use the lockfile directly
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut config = setup_project(tmp.path());
        let registry = make_registry();

        // First install to create lockfile
        let result = install(&config, &registry);
        assert!(result.is_ok());

        // Now install again in locked mode
        config.locked = true;
        let result = install(&config, &registry);
        assert!(result.is_ok(), "locked install failed: {result:?}");
    }

    // =========================================================================
    // install: existing_lockfile.is_some() branch — reconcile then re-resolve
    // =========================================================================

    #[test]
    fn install_with_existing_lockfile_reconciles() {
        // Covers the `existing_lockfile.is_some()` path in resolve_dependencies
        // where deps have changed so the lockfile is stale.
        let tmp = tempfile::tempdir().expect("tempdir");
        let registry = make_registry();

        // First install (creates lockfile with pkg-a)
        let config = setup_project(tmp.path());
        let result = install(&config, &registry);
        assert!(result.is_ok(), "first install failed: {result:?}");

        // Second install without changes: reconcile finds nothing changed →
        // build_resolution_from_lockfile is called.
        let config2 = setup_project(tmp.path());
        let result2 = install(&config2, &registry);
        assert!(result2.is_ok(), "second install failed: {result2:?}");
    }

    // =========================================================================
    // install: assembled dir already exists and is up-to-date (up_to_date branch)
    // =========================================================================

    #[test]
    fn install_second_run_counts_up_to_date() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let registry = make_registry();
        let config = setup_project(tmp.path());

        // First install builds the assembled dir
        let r1 = install(&config, &registry);
        assert!(r1.is_ok(), "first install: {r1:?}");

        // Second install: assembled dir already exists AND lockfile matches →
        // up_to_date counter is incremented.
        let r2 = install(&config, &registry);
        assert!(r2.is_ok(), "second install: {r2:?}");
        let stats = r2.unwrap();
        assert_eq!(stats.up_to_date, 1, "expected 1 up-to-date package");
        assert_eq!(stats.installed, 0, "expected 0 newly installed");
    }

    // =========================================================================
    // handle_removals: package present in old lockfile but absent from resolution
    // =========================================================================

    #[test]
    fn handle_removals_with_stale_package_increments_removed() {
        let tmp = tempfile::tempdir().expect("tempdir");

        // Build a resolution that contains pkg-a only
        let resolution = resolver::Resolution {
            packages: vec![resolver::Resolved {
                name: "pkg-a".to_string(),
                version: Version::parse("1.0.0").unwrap(),
                source: resolver::Source::Registry { index_url: "test".to_string() },
                checksum: "sha512-pkg-a-1.0.0".to_string(),
                dependencies: vec![],
                features: BTreeSet::new(),
            }],
        };

        // Old lockfile also contains pkg-old (which is NOT in the new resolution)
        let old_lf = lockfile::types::Lockfile {
            metadata: lockfile::types::Metadata {
                lockfile_version: 1,
                generated_by: "test".to_string(),
            },
            packages: vec![
                lockfile::types::Package {
                    name: "pkg-a".to_string(),
                    version: "1.0.0".to_string(),
                    source: "git+test".to_string(),
                    checksum: "sha512-pkg-a-1.0.0".to_string(),
                    dependencies: vec![],
                },
                lockfile::types::Package {
                    name: "pkg-old".to_string(),
                    version: "9.9.9".to_string(),
                    source: "git+test".to_string(),
                    checksum: "sha512-old".to_string(),
                    dependencies: vec![],
                },
            ],
        };

        let links_dir = tmp.path().join(".aipm/links");
        let plugins_dir = tmp.path().join("claude-plugins");
        let gitignore_path = plugins_dir.join(".gitignore");
        std::fs::create_dir_all(&links_dir).unwrap();
        std::fs::create_dir_all(&plugins_dir).unwrap();

        // pkg-old is not in the new resolution, so it should be removed (count = 1)
        let removed =
            handle_removals(Some(&old_lf), &resolution, &links_dir, &plugins_dir, &gitignore_path)
                .unwrap();

        assert_eq!(removed, 1);
    }

    // =========================================================================
    // clear_dev_links: state file exists with entries → entries are cleared
    // =========================================================================

    #[test]
    fn clear_dev_links_with_existing_entries_clears_them() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let state_path = tmp.path().join(".aipm/links.toml");

        // Add two link entries
        let entry_a = crate::linker::link_state::LinkEntry {
            name: "tool-a".to_string(),
            path: std::path::PathBuf::from("/dev/tool-a"),
            linked_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let entry_b = crate::linker::link_state::LinkEntry {
            name: "tool-b".to_string(),
            path: std::path::PathBuf::from("/dev/tool-b"),
            linked_at: "2026-01-02T00:00:00Z".to_string(),
        };
        crate::linker::link_state::add(&state_path, entry_a).unwrap();
        crate::linker::link_state::add(&state_path, entry_b).unwrap();

        let before = crate::linker::link_state::list(&state_path).unwrap();
        assert_eq!(before.len(), 2);

        // clear_dev_links should empty the file
        clear_dev_links(&state_path).unwrap();

        let after = crate::linker::link_state::list(&state_path).unwrap();
        assert!(after.is_empty(), "expected empty state after clear_dev_links");
    }

    // =========================================================================
    // install: locked=true but existing_lockfile is None (LockfileDrift error)
    // =========================================================================

    #[test]
    fn install_locked_without_lockfile_returns_drift_error() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut config = setup_project(tmp.path());
        config.locked = true;
        // No lockfile written — config.lockfile_path does not exist
        let registry = make_registry();

        let result = install(&config, &registry);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("lockfile") || msg.contains("locked"), "unexpected error: {msg}");
    }

    // =========================================================================
    // update: existing lockfile present with None package (full re-resolve pins)
    // =========================================================================

    #[test]
    fn update_full_with_existing_lockfile() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let registry = make_registry();

        // Install first
        let install_config = setup_project(tmp.path());
        let r = install(&install_config, &registry);
        assert!(r.is_ok(), "install: {r:?}");

        // Full update with an existing lockfile
        let mut config = make_update_config(tmp.path());
        config.package = None; // full update
        let result = update(&config, &registry);
        assert!(result.is_ok(), "full update: {result:?}");

        let lf = lockfile::read(&config.lockfile_path).unwrap();
        assert!(!lf.packages.is_empty());
    }

    // =========================================================================
    // build_resolution_from_lockfile: unknown (non-path, non-workspace) source
    // =========================================================================

    #[test]
    fn build_resolution_from_lockfile_unknown_source_fallback() {
        // A source that doesn't start with "path+" or equal "workspace"
        // and also doesn't start with "git+" exercises the
        // `strip_prefix("git+").unwrap_or(&pkg.source)` fallback.
        let lf = lockfile::types::Lockfile {
            metadata: lockfile::types::Metadata {
                lockfile_version: 1,
                generated_by: "test".to_string(),
            },
            packages: vec![lockfile::types::Package {
                name: "bare-pkg".to_string(),
                version: "1.0.0".to_string(),
                source: "https://raw.example.com/index".to_string(), // no git+ prefix
                checksum: "sha512-abc".to_string(),
                dependencies: vec![],
            }],
        };

        let resolution = build_resolution_from_lockfile(&lf).unwrap();
        assert_eq!(resolution.packages.len(), 1);
        assert!(matches!(
            &resolution.packages[0].source,
            resolver::Source::Registry { index_url }
                if index_url == "https://raw.example.com/index"
        ));
    }

    // =========================================================================
    // manifest_to_resolver_deps: no dependencies section at all
    // =========================================================================

    #[test]
    fn manifest_to_resolver_deps_no_dependencies() {
        let toml_str = r#"
[package]
name = "test"
version = "0.1.0"
"#;
        let m = manifest::parse(toml_str).unwrap();
        let deps = manifest_to_resolver_deps(&m);
        assert!(deps.is_empty());
    }

    // =========================================================================
    // resolve_dependencies: reconcile finds changes (L270 False branch)
    // =========================================================================

    #[test]
    fn install_with_changed_manifest_forces_re_resolution() {
        // First install with only pkg-a in the manifest.
        // Then write a new manifest that adds pkg-b and re-install.
        // This forces reconcile to find `added != empty` → L270 False branch
        // (needs_resolution is non-empty) → full re-resolution with pins.
        let tmp = tempfile::tempdir().expect("tempdir");
        let registry = make_registry();

        let config = setup_project(tmp.path());
        let r1 = install(&config, &registry);
        assert!(r1.is_ok(), "first install: {r1:?}");

        // Now update the manifest to add pkg-b
        let new_manifest = r#"[package]
name = "test-project"
version = "0.1.0"

[dependencies]
pkg-a = "^1.0"
pkg-b = "^2.0"
"#;
        std::fs::write(&config.manifest_path, new_manifest).expect("write new manifest");

        // Re-install — reconcile will find pkg-b as "needs_resolution"
        let config2 = InstallConfig {
            manifest_path: config.manifest_path.clone(),
            lockfile_path: config.lockfile_path.clone(),
            store_path: config.store_path.clone(),
            links_dir: config.links_dir.clone(),
            plugins_dir: config.plugins_dir.clone(),
            gitignore_path: config.gitignore_path.clone(),
            link_state_path: config.link_state_path.clone(),
            workspace_root: None,
            locked: false,
            add_package: None,
            generated_by: "aipm-test 0.1.0".to_string(),
        };

        let r2 = install(&config2, &registry);
        assert!(r2.is_ok(), "second install with added dep: {r2:?}");

        let lf = lockfile::read(&config2.lockfile_path).unwrap();
        let names: Vec<_> = lf.packages.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"pkg-b"), "pkg-b should be in lockfile after re-install");
    }

    // =========================================================================
    // update: existing lockfile, targeted package (L495 True + L539 branches)
    // =========================================================================

    #[test]
    fn update_second_run_counts_up_to_date() {
        // Install, then update twice — second update should find assembled dir
        // exists and up-to-date (covers L539 True + !needs_update True path).
        let tmp = tempfile::tempdir().expect("tempdir");
        let registry = make_registry();

        // First install to create lockfile and assembled dirs
        let install_config = setup_project(tmp.path());
        let r1 = install(&install_config, &registry);
        assert!(r1.is_ok(), "install: {r1:?}");

        // First update (with existing lockfile — covers L495 True)
        let mut config = make_update_config(tmp.path());
        config.package = None;
        let r2 = update(&config, &registry);
        assert!(r2.is_ok(), "first update: {r2:?}");

        // Second update: assembled dirs exist AND version matches lockfile
        // → assembled_dir.exists() == True && !needs_update == True
        // This covers L539:12 True and L539:38 True (up-to-date branch)
        let r3 = update(&config, &registry);
        assert!(r3.is_ok(), "second update: {r3:?}");
        let stats = r3.unwrap();
        assert_eq!(stats.up_to_date, 1, "expected 1 up-to-date package on second update");
        assert_eq!(stats.installed, 0, "expected 0 newly installed on second update");
    }

    // =========================================================================
    // needs_update: version matches but we also test the ||  checksum path
    // =========================================================================

    #[test]
    fn needs_update_false_when_found_and_both_match() {
        // Exercises the `is_none_or` closure returning false:
        // found = Some(locked) where version and checksum both match.
        let resolved = resolver::Resolved {
            name: "pkg-a".to_string(),
            version: Version::parse("1.0.0").unwrap(),
            source: resolver::Source::Registry { index_url: "test".to_string() },
            checksum: "sha512-match".to_string(),
            dependencies: vec![],
            features: BTreeSet::new(),
        };

        let lf = lockfile::types::Lockfile {
            metadata: lockfile::types::Metadata {
                lockfile_version: 1,
                generated_by: "test".to_string(),
            },
            packages: vec![lockfile::types::Package {
                name: "pkg-a".to_string(),
                version: "1.0.0".to_string(),
                source: "git+test".to_string(),
                checksum: "sha512-match".to_string(),
                dependencies: vec![],
            }],
        };

        // Both version and checksum match → needs_update returns false
        let result = needs_update(&resolved, Some(&lf));
        assert!(!result, "needs_update should be false when version and checksum match");
    }

    // =========================================================================
    // build_pins: Err branch for invalid version string
    // =========================================================================

    #[test]
    fn build_pins_skips_invalid_version_entries() {
        // Exercises the `Err` branch of `Version::parse` in build_pins:
        // packages with invalid versions are silently skipped.
        let packages = vec![
            lockfile::types::Package {
                name: "valid-pkg".to_string(),
                version: "2.3.4".to_string(),
                source: "git+test".to_string(),
                checksum: "sha512-xyz".to_string(),
                dependencies: vec![],
            },
            lockfile::types::Package {
                name: "bad-version-pkg".to_string(),
                version: "not-semver".to_string(),
                source: "git+test".to_string(),
                checksum: String::new(),
                dependencies: vec![],
            },
        ];

        let pins = build_pins(&packages);
        assert_eq!(pins.len(), 1);
        assert!(pins.contains_key("valid-pkg"));
        assert!(!pins.contains_key("bad-version-pkg"));
    }

    // =========================================================================
    // resolve_dependencies: locked=true but lockfile is None → error path
    // =========================================================================

    #[test]
    fn resolve_registry_deps_locked_with_no_lockfile_falls_through() {
        let toml_str = r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
pkg-a = "^1.0"
"#;
        let m = manifest::parse(toml_str).unwrap();
        let mut manifest_deps = BTreeSet::new();
        manifest_deps.insert("pkg-a".to_string());
        let registry = make_registry();

        let root_deps = manifest_to_resolver_deps(&m);
        // locked=true, existing_lockfile=None → falls through to fresh resolution
        let result =
            resolve_registry_dependencies(&root_deps, &m, &manifest_deps, None, true, &registry);
        assert!(result.is_ok(), "resolve with locked=true and no lockfile should resolve fresh");
    }

    // =========================================================================
    // handle_removals: all packages still present (L387 False branch)
    // =========================================================================

    #[test]
    fn handle_removals_no_stale_packages_returns_zero() {
        // Exercises L387 False: `!new_names.contains(pkg.name.as_str())` is False
        // when ALL old packages are still in the new resolution.
        let tmp = tempfile::tempdir().expect("tempdir");

        let resolution = resolver::Resolution {
            packages: vec![
                resolver::Resolved {
                    name: "pkg-a".to_string(),
                    version: Version::parse("1.0.0").unwrap(),
                    source: resolver::Source::Registry { index_url: "test".to_string() },
                    checksum: "sha512-pkg-a-1.0.0".to_string(),
                    dependencies: vec![],
                    features: BTreeSet::new(),
                },
                resolver::Resolved {
                    name: "pkg-b".to_string(),
                    version: Version::parse("2.0.0").unwrap(),
                    source: resolver::Source::Registry { index_url: "test".to_string() },
                    checksum: "sha512-pkg-b-2.0.0".to_string(),
                    dependencies: vec![],
                    features: BTreeSet::new(),
                },
            ],
        };

        // Old lockfile has exactly the same packages as the new resolution
        let old_lf = lockfile::types::Lockfile {
            metadata: lockfile::types::Metadata {
                lockfile_version: 1,
                generated_by: "test".to_string(),
            },
            packages: vec![
                lockfile::types::Package {
                    name: "pkg-a".to_string(),
                    version: "1.0.0".to_string(),
                    source: "git+test".to_string(),
                    checksum: "sha512-pkg-a-1.0.0".to_string(),
                    dependencies: vec![],
                },
                lockfile::types::Package {
                    name: "pkg-b".to_string(),
                    version: "2.0.0".to_string(),
                    source: "git+test".to_string(),
                    checksum: "sha512-pkg-b-2.0.0".to_string(),
                    dependencies: vec![],
                },
            ],
        };

        let links_dir = tmp.path().join(".aipm/links");
        let plugins_dir = tmp.path().join("claude-plugins");
        let gitignore_path = plugins_dir.join(".gitignore");
        std::fs::create_dir_all(&links_dir).unwrap();
        std::fs::create_dir_all(&plugins_dir).unwrap();

        // No packages removed → result should be 0
        let removed =
            handle_removals(Some(&old_lf), &resolution, &links_dir, &plugins_dir, &gitignore_path)
                .unwrap();
        assert_eq!(removed, 0, "no packages should be removed when all are still present");
    }

    // =========================================================================
    // update: targeted update with an existing lockfile (covers L495:32 True)
    // =========================================================================

    #[test]
    fn update_targeted_with_existing_lockfile() {
        // Exercises L495:32 True branch: lockfile_path.exists() in update().
        // Runs install first to create lockfile, then targeted update.
        let tmp = tempfile::tempdir().expect("tempdir");
        let registry = make_registry();

        // Install to create lockfile
        let install_config = setup_project(tmp.path());
        let r = install(&install_config, &registry);
        assert!(r.is_ok(), "install: {r:?}");
        assert!(install_config.lockfile_path.exists(), "lockfile must exist");

        // Targeted update with existing lockfile
        let mut config = make_update_config(tmp.path());
        config.package = Some("pkg-a".to_string());

        let result = update(&config, &registry);
        assert!(result.is_ok(), "targeted update with existing lockfile: {result:?}");

        let lf = lockfile::read(&config.lockfile_path).unwrap();
        assert!(!lf.packages.is_empty());
    }

    // =========================================================================
    // split_dependencies tests
    // =========================================================================

    #[test]
    fn split_deps_workspace_and_registry() {
        let toml_str = r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
registry-dep = "^1.0"
ws-dep = { workspace = "*" }
"#;
        let m = manifest::parse(toml_str).unwrap();
        let (ws_deps, reg_deps) = split_dependencies(&m);
        assert_eq!(ws_deps.len(), 1);
        assert_eq!(ws_deps[0], "ws-dep");
        assert_eq!(reg_deps.len(), 1);
        assert_eq!(reg_deps[0].name, "registry-dep");
    }

    #[test]
    fn split_deps_all_registry() {
        let toml_str = r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
a = "^1.0"
b = "^2.0"
"#;
        let m = manifest::parse(toml_str).unwrap();
        let (ws_deps, reg_deps) = split_dependencies(&m);
        assert!(ws_deps.is_empty());
        assert_eq!(reg_deps.len(), 2);
    }

    #[test]
    fn split_deps_empty() {
        let toml_str = "[package]\nname = \"test\"\nversion = \"0.1.0\"\n";
        let m = manifest::parse(toml_str).unwrap();
        let (ws_deps, reg_deps) = split_dependencies(&m);
        assert!(ws_deps.is_empty());
        assert!(reg_deps.is_empty());
    }

    // =========================================================================
    // resolve_workspace_deps tests
    // =========================================================================

    fn make_member(name: &str, version: &str, deps_toml: &str) -> workspace::Member {
        let manifest_str =
            format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\n{deps_toml}");
        let parsed = manifest::parse(&manifest_str).unwrap();
        workspace::Member {
            name: name.to_string(),
            path: PathBuf::from(format!("/fake/{name}")),
            version: version.to_string(),
            manifest: parsed,
        }
    }

    #[test]
    fn resolve_direct_workspace_dep() {
        let mut members = BTreeMap::new();
        let m = make_member("plugin-b", "2.0.0", "");
        members.insert("plugin-b".to_string(), m);

        let ws_deps = vec!["plugin-b".to_string()];
        let overrides = BTreeSet::new();

        let result = resolve_workspace_deps(&ws_deps, &members, &overrides);
        assert!(result.is_ok(), "resolve should succeed: {:?}", result.err());
        let resolved = result.unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "plugin-b");
        assert_eq!(format!("{}", resolved[0].version), "2.0.0");
        assert!(matches!(resolved[0].source, resolver::Source::Workspace));
        assert!(resolved[0].checksum.is_empty());
    }

    #[test]
    fn resolve_transitive_workspace_deps() {
        let mut members = BTreeMap::new();
        let a =
            make_member("plugin-a", "1.0.0", "[dependencies]\nplugin-b = { workspace = \"*\" }\n");
        let b =
            make_member("plugin-b", "2.0.0", "[dependencies]\nplugin-c = { workspace = \"*\" }\n");
        let c = make_member("plugin-c", "3.0.0", "");
        members.insert("plugin-a".to_string(), a);
        members.insert("plugin-b".to_string(), b);
        members.insert("plugin-c".to_string(), c);

        let ws_deps = vec!["plugin-a".to_string()];
        let overrides = BTreeSet::new();

        let result = resolve_workspace_deps(&ws_deps, &members, &overrides);
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert_eq!(resolved.len(), 3);
        let names: BTreeSet<String> = resolved.iter().map(|r| r.name.clone()).collect();
        assert!(names.contains("plugin-a"));
        assert!(names.contains("plugin-b"));
        assert!(names.contains("plugin-c"));
    }

    #[test]
    fn resolve_link_overrides_produce_path_source() {
        let mut members = BTreeMap::new();
        let m = make_member("plugin-b", "2.0.0", "");
        members.insert("plugin-b".to_string(), m);

        let ws_deps = vec!["plugin-b".to_string()];
        let mut overrides = BTreeSet::new();
        overrides.insert("plugin-b".to_string());

        let result = resolve_workspace_deps(&ws_deps, &members, &overrides);
        assert!(result.is_ok());
        let resolved = result.unwrap();
        // Link-overridden deps produce a Source::Path entry to prevent removal
        assert_eq!(resolved.len(), 1);
        assert!(
            matches!(resolved[0].source, resolver::Source::Path { .. }),
            "link-overridden dep should use Source::Path"
        );
    }

    #[test]
    fn resolve_error_unknown_member() {
        let members = BTreeMap::new();
        let ws_deps = vec!["nonexistent".to_string()];
        let overrides = BTreeSet::new();

        let result = resolve_workspace_deps(&ws_deps, &members, &overrides);
        assert!(result.is_err());
        let err = format!("{}", result.err().unwrap());
        assert!(err.contains("not found in workspace members"));
    }

    #[test]
    fn resolve_circular_deps() {
        let mut members = BTreeMap::new();
        let a =
            make_member("plugin-a", "1.0.0", "[dependencies]\nplugin-b = { workspace = \"*\" }\n");
        let b =
            make_member("plugin-b", "2.0.0", "[dependencies]\nplugin-a = { workspace = \"*\" }\n");
        members.insert("plugin-a".to_string(), a);
        members.insert("plugin-b".to_string(), b);

        let ws_deps = vec!["plugin-a".to_string()];
        let overrides = BTreeSet::new();

        let result = resolve_workspace_deps(&ws_deps, &members, &overrides);
        assert!(result.is_ok(), "circular deps should not infinite loop");
        let resolved = result.unwrap();
        assert_eq!(resolved.len(), 2);
    }

    #[test]
    fn split_deps_all_workspace() {
        let toml_str = r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
ws-a = { workspace = "*" }
ws-b = { workspace = "*" }
"#;
        let m = manifest::parse(toml_str).unwrap();
        let (ws_deps, reg_deps) = split_dependencies(&m);
        assert_eq!(ws_deps.len(), 2);
        assert!(reg_deps.is_empty());
    }

    #[test]
    fn resolve_registry_deps_empty() {
        let toml_str = "[package]\nname = \"test\"\nversion = \"0.1.0\"\n";
        let m = manifest::parse(toml_str).unwrap();
        let manifest_deps = BTreeSet::new();
        let registry = make_registry();

        let result = resolve_registry_dependencies(&[], &m, &manifest_deps, None, false, &registry);
        assert!(result.is_ok());
        assert!(result.unwrap().packages.is_empty());
    }

    #[test]
    fn resolve_collects_transitive_dep_strings() {
        let mut members = BTreeMap::new();
        let a = make_member(
            "plugin-a",
            "1.0.0",
            "[dependencies]\nregistry-dep = \"^2.0\"\nws-sibling = { workspace = \"*\" }\n",
        );
        let sibling = make_member("ws-sibling", "3.0.0", "");
        members.insert("plugin-a".to_string(), a);
        members.insert("ws-sibling".to_string(), sibling);

        let ws_deps = vec!["plugin-a".to_string()];
        let overrides = BTreeSet::new();

        let result = resolve_workspace_deps(&ws_deps, &members, &overrides);
        assert!(result.is_ok(), "should succeed: {:?}", result.err());
        let resolved = result.unwrap();
        let plugin_a = resolved.iter().find(|r| r.name == "plugin-a").unwrap();
        assert!(plugin_a.dependencies.contains(&"registry-dep ^2.0".to_string()));
        assert!(plugin_a.dependencies.contains(&"ws-sibling *".to_string()));
    }

    // =========================================================================
    // split_dependencies: Detailed non-workspace registry dep
    // =========================================================================

    #[test]
    fn split_deps_detailed_non_workspace() {
        let toml_str = r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
detailed-dep = { version = "^2.0", features = ["extra"], default-features = false }
ws-dep = { workspace = "*" }
"#;
        let m = manifest::parse(toml_str).unwrap();
        let (ws_deps, reg_deps) = split_dependencies(&m);
        assert_eq!(ws_deps.len(), 1);
        assert_eq!(reg_deps.len(), 1);
        assert_eq!(reg_deps[0].name, "detailed-dep");
        assert_eq!(reg_deps[0].req, "^2.0");
        assert_eq!(reg_deps[0].features, vec!["extra".to_string()]);
        assert!(!reg_deps[0].default_features);
    }

    #[test]
    fn split_deps_detailed_no_version() {
        let toml_str = r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
no-ver = { features = ["x"] }
"#;
        let m = manifest::parse(toml_str).unwrap();
        let (ws_deps, reg_deps) = split_dependencies(&m);
        assert!(ws_deps.is_empty());
        assert_eq!(reg_deps.len(), 1);
        assert_eq!(reg_deps[0].req, "*");
    }

    // =========================================================================
    // resolve_workspace_deps: invalid version error
    // =========================================================================

    #[test]
    fn resolve_workspace_dep_invalid_version() {
        let mut members = BTreeMap::new();
        let manifest_str = "[package]\nname = \"bad-ver\"\nversion = \"not-semver\"\n";
        let parsed = manifest::parse(manifest_str).unwrap();
        members.insert(
            "bad-ver".to_string(),
            workspace::Member {
                name: "bad-ver".to_string(),
                path: PathBuf::from("/fake/bad-ver"),
                version: "not-semver".to_string(),
                manifest: parsed,
            },
        );

        let ws_deps = vec!["bad-ver".to_string()];
        let overrides = BTreeSet::new();

        let result = resolve_workspace_deps(&ws_deps, &members, &overrides);
        assert!(result.is_err());
        let err = format!("{}", result.err().unwrap());
        assert!(err.contains("invalid version"));
    }

    // =========================================================================
    // resolve_workspace_deps: Detailed dep with version (not workspace)
    // =========================================================================

    #[test]
    fn resolve_workspace_dep_with_detailed_non_ws_transitive() {
        let mut members = BTreeMap::new();
        let m = make_member(
            "plugin-a",
            "1.0.0",
            "[dependencies]\nreg-dep = { version = \"^3.0\", features = [\"extra\"] }\n",
        );
        members.insert("plugin-a".to_string(), m);

        let ws_deps = vec!["plugin-a".to_string()];
        let overrides = BTreeSet::new();

        let result = resolve_workspace_deps(&ws_deps, &members, &overrides);
        assert!(result.is_ok());
        let resolved = result.unwrap();
        let plugin_a = resolved.iter().find(|r| r.name == "plugin-a").unwrap();
        assert!(plugin_a.dependencies.contains(&"reg-dep ^3.0".to_string()));
    }

    // =========================================================================
    // discover_workspace_members: manifest has [workspace] section
    // =========================================================================

    #[test]
    fn discover_workspace_members_from_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Write workspace root manifest
        let ws_manifest = r#"
[workspace]
members = [".ai/*"]
"#;
        std::fs::write(root.join("aipm.toml"), ws_manifest).unwrap();

        // Create a member
        let member_dir = root.join(".ai/plugin-a");
        std::fs::create_dir_all(&member_dir).unwrap();
        std::fs::write(
            member_dir.join("aipm.toml"),
            "[package]\nname = \"plugin-a\"\nversion = \"1.0.0\"\n",
        )
        .unwrap();

        let manifest_content = std::fs::read_to_string(root.join("aipm.toml")).unwrap();
        let parsed = manifest::parse_and_validate(&manifest_content, Some(root)).unwrap();

        let config = InstallConfig {
            manifest_path: root.join("aipm.toml"),
            lockfile_path: root.join("aipm.lock"),
            store_path: root.join(".aipm/store"),
            links_dir: root.join(".aipm/links"),
            plugins_dir: root.join(".ai"),
            gitignore_path: root.join(".ai/.gitignore"),
            link_state_path: root.join(".aipm/links.toml"),
            workspace_root: None,
            locked: false,
            add_package: None,
            generated_by: "test".to_string(),
        };

        let members = discover_workspace_members(&config, &parsed);
        assert!(members.is_ok(), "should discover members: {:?}", members.err());
        let members = members.unwrap();
        assert_eq!(members.len(), 1);
        assert!(members.contains_key("plugin-a"));
    }

    // =========================================================================
    // discover_workspace_members: workspace_root provided
    // =========================================================================

    #[test]
    fn discover_workspace_members_via_workspace_root() {
        let tmp = tempfile::tempdir().unwrap();
        let ws_root = tmp.path().join("ws-root");
        std::fs::create_dir_all(&ws_root).unwrap();

        // Workspace root manifest
        std::fs::write(ws_root.join("aipm.toml"), "[workspace]\nmembers = [\".ai/*\"]\n").unwrap();

        // Member
        let member_dir = ws_root.join(".ai/plugin-x");
        std::fs::create_dir_all(&member_dir).unwrap();
        std::fs::write(
            member_dir.join("aipm.toml"),
            "[package]\nname = \"plugin-x\"\nversion = \"2.0.0\"\n",
        )
        .unwrap();

        // Member manifest (no workspace section)
        let member_project = tmp.path().join("member-project");
        std::fs::create_dir_all(&member_project).unwrap();
        std::fs::write(
            member_project.join("aipm.toml"),
            "[package]\nname = \"member-project\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        let manifest_content = std::fs::read_to_string(member_project.join("aipm.toml")).unwrap();
        let parsed =
            manifest::parse_and_validate(&manifest_content, Some(member_project.as_path()))
                .unwrap();

        let config = InstallConfig {
            manifest_path: member_project.join("aipm.toml"),
            lockfile_path: member_project.join("aipm.lock"),
            store_path: member_project.join(".aipm/store"),
            links_dir: member_project.join(".aipm/links"),
            plugins_dir: member_project.join(".ai"),
            gitignore_path: member_project.join(".ai/.gitignore"),
            link_state_path: member_project.join(".aipm/links.toml"),
            workspace_root: Some(ws_root),
            locked: false,
            add_package: None,
            generated_by: "test".to_string(),
        };

        let members = discover_workspace_members(&config, &parsed);
        assert!(members.is_ok(), "should find members via workspace_root: {:?}", members.err());
        let members = members.unwrap();
        assert_eq!(members.len(), 1);
        assert!(members.contains_key("plugin-x"));
    }

    // =========================================================================
    // Full workspace install integration test
    // =========================================================================

    #[test]
    fn install_workspace_deps_end_to_end() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Create workspace root manifest with a workspace dep
        let ws_manifest = r#"
[workspace]
members = [".ai/*"]
plugins_dir = ".ai"

[dependencies]
plugin-b = { workspace = "*" }
"#;
        std::fs::write(root.join("aipm.toml"), ws_manifest).unwrap();

        // Create workspace member plugin-b
        let member_dir = root.join(".ai/plugin-b");
        std::fs::create_dir_all(&member_dir).unwrap();
        std::fs::write(
            member_dir.join("aipm.toml"),
            "[package]\nname = \"plugin-b\"\nversion = \"0.1.0\"\ntype = \"composite\"\n",
        )
        .unwrap();
        // Add a marker file so we can verify the link target
        std::fs::write(member_dir.join("marker.txt"), "hello").unwrap();

        let config = InstallConfig {
            manifest_path: root.join("aipm.toml"),
            lockfile_path: root.join("aipm.lock"),
            store_path: root.join(".aipm/store"),
            links_dir: root.join(".aipm/links"),
            plugins_dir: root.join("plugins"),
            gitignore_path: root.join("plugins/.gitignore"),
            link_state_path: root.join(".aipm/links.toml"),
            workspace_root: None,
            locked: false,
            add_package: None,
            generated_by: "aipm-test 0.1.0".to_string(),
        };

        let registry = StubRegistry;
        let result = install(&config, &registry);
        assert!(result.is_ok(), "workspace install should succeed: {:?}", result.err());

        let res = result.unwrap();
        assert_eq!(res.installed, 1, "should install 1 workspace dep");

        // Verify the directory link was created
        let link_path = root.join("plugins/plugin-b");
        assert!(link_path.exists(), "plugin-b should be linked in plugins dir");

        // Verify we can read through the link
        let marker = std::fs::read_to_string(link_path.join("marker.txt"));
        assert!(marker.is_ok(), "should read marker through link");
        assert_eq!(marker.unwrap(), "hello");

        // Verify lockfile was written with workspace source
        let lf = lockfile::read(&config.lockfile_path).unwrap();
        assert_eq!(lf.packages.len(), 1);
        assert_eq!(lf.packages[0].name, "plugin-b");
        assert_eq!(lf.packages[0].source, "workspace");
        assert!(lf.packages[0].checksum.is_empty());
    }

    struct StubRegistry;

    impl crate::registry::Registry for StubRegistry {
        fn get_metadata(
            &self,
            name: &str,
        ) -> Result<crate::registry::PackageMetadata, crate::registry::error::Error> {
            Err(crate::registry::error::Error::Io { reason: format!("stub: no package {name}") })
        }

        fn download(
            &self,
            name: &str,
            _version: &crate::version::Version,
        ) -> Result<Vec<u8>, crate::registry::error::Error> {
            Err(crate::registry::error::Error::Io {
                reason: format!("stub: cannot download {name}"),
            })
        }
    }

    // =========================================================================
    // discover_workspace_members: workspace_root with no [workspace] section
    // =========================================================================

    #[test]
    fn discover_workspace_members_workspace_root_no_workspace_section() {
        let tmp = tempfile::tempdir().unwrap();
        let ws_root = tmp.path().join("ws-root");
        std::fs::create_dir_all(&ws_root).unwrap();

        // Workspace root has manifest but NO [workspace] section
        std::fs::write(
            ws_root.join("aipm.toml"),
            "[package]\nname = \"not-a-workspace\"\nversion = \"1.0.0\"\n",
        )
        .unwrap();

        let member_project = tmp.path().join("member");
        std::fs::create_dir_all(&member_project).unwrap();
        std::fs::write(
            member_project.join("aipm.toml"),
            "[package]\nname = \"member\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        let manifest_content = std::fs::read_to_string(member_project.join("aipm.toml")).unwrap();
        let parsed =
            manifest::parse_and_validate(&manifest_content, Some(member_project.as_path()))
                .unwrap();

        let config = InstallConfig {
            manifest_path: member_project.join("aipm.toml"),
            lockfile_path: member_project.join("aipm.lock"),
            store_path: member_project.join(".aipm/store"),
            links_dir: member_project.join(".aipm/links"),
            plugins_dir: member_project.join(".ai"),
            gitignore_path: member_project.join(".ai/.gitignore"),
            link_state_path: member_project.join(".aipm/links.toml"),
            workspace_root: Some(ws_root),
            locked: false,
            add_package: None,
            generated_by: "test".to_string(),
        };

        let members = discover_workspace_members(&config, &parsed).unwrap();
        assert!(members.is_empty(), "no workspace section → no members");
    }

    // =========================================================================
    // discover_workspace_members: no workspace context at all
    // =========================================================================

    #[test]
    fn discover_workspace_members_no_workspace_context() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();

        std::fs::write(
            project.join("aipm.toml"),
            "[package]\nname = \"standalone\"\nversion = \"1.0.0\"\n",
        )
        .unwrap();

        let manifest_content = std::fs::read_to_string(project.join("aipm.toml")).unwrap();
        let parsed = manifest::parse_and_validate(&manifest_content, Some(project)).unwrap();

        let config = InstallConfig {
            manifest_path: project.join("aipm.toml"),
            lockfile_path: project.join("aipm.lock"),
            store_path: project.join(".aipm/store"),
            links_dir: project.join(".aipm/links"),
            plugins_dir: project.join(".ai"),
            gitignore_path: project.join(".ai/.gitignore"),
            link_state_path: project.join(".aipm/links.toml"),
            workspace_root: None,
            locked: false,
            add_package: None,
            generated_by: "test".to_string(),
        };

        let members = discover_workspace_members(&config, &parsed).unwrap();
        assert!(members.is_empty());
    }

    // =========================================================================
    // collect_transitive_registry_deps
    // =========================================================================

    #[test]
    fn collect_transitive_registry_deps_from_workspace_members() {
        let mut members = BTreeMap::new();
        let m = make_member(
            "plugin-a",
            "1.0.0",
            "[dependencies]\nreg-dep = \"^2.0\"\nws-dep = { workspace = \"*\" }\n",
        );
        members.insert("plugin-a".to_string(), m);

        let ws_resolved = vec![resolver::Resolved {
            name: "plugin-a".to_string(),
            version: Version::parse("1.0.0").unwrap(),
            source: resolver::Source::Workspace,
            checksum: String::new(),
            dependencies: vec!["reg-dep ^2.0".to_string()],
            features: BTreeSet::new(),
        }];

        let base_deps = vec![];
        let result = collect_transitive_registry_deps(base_deps, &ws_resolved, &members);
        // Should have collected reg-dep but not ws-dep
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "reg-dep");
        assert_eq!(result[0].req, "^2.0");
        assert_eq!(result[0].source, "plugin-a");
    }

    #[test]
    fn collect_transitive_registry_deps_with_detailed() {
        let mut members = BTreeMap::new();
        let m = make_member(
            "plugin-a",
            "1.0.0",
            "[dependencies]\ndetailed = { version = \"^3.0\", features = [\"x\"], default-features = false }\n",
        );
        members.insert("plugin-a".to_string(), m);

        let ws_resolved = vec![resolver::Resolved {
            name: "plugin-a".to_string(),
            version: Version::parse("1.0.0").unwrap(),
            source: resolver::Source::Workspace,
            checksum: String::new(),
            dependencies: vec![],
            features: BTreeSet::new(),
        }];

        let result = collect_transitive_registry_deps(vec![], &ws_resolved, &members);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "detailed");
        assert_eq!(result[0].req, "^3.0");
        assert_eq!(result[0].features, vec!["x".to_string()]);
        assert!(!result[0].default_features);
    }

    // =========================================================================
    // link_resolved_packages: Source::Path is a no-op (warning only)
    // =========================================================================

    #[test]
    fn link_resolved_packages_skips_path_source() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("plugins")).unwrap();

        let config = InstallConfig {
            manifest_path: root.join("aipm.toml"),
            lockfile_path: root.join("aipm.lock"),
            store_path: root.join(".aipm/store"),
            links_dir: root.join(".aipm/links"),
            plugins_dir: root.join("plugins"),
            gitignore_path: root.join("plugins/.gitignore"),
            link_state_path: root.join(".aipm/links.toml"),
            workspace_root: None,
            locked: false,
            add_package: None,
            generated_by: "test".to_string(),
        };

        let resolution = resolver::Resolution {
            packages: vec![resolver::Resolved {
                name: "path-pkg".to_string(),
                version: Version::parse("1.0.0").unwrap(),
                source: resolver::Source::Path { path: PathBuf::from("/some/path") },
                checksum: String::new(),
                dependencies: vec![],
                features: BTreeSet::new(),
            }],
        };

        let members = BTreeMap::new();
        let stub = StubRegistry;
        let result = link_resolved_packages(&config, &resolution, &members, None, &stub);
        assert!(result.is_ok());
        let (installed, up_to_date) = result.unwrap();
        assert_eq!(installed, 0, "path deps should not be installed");
        assert_eq!(up_to_date, 0);
    }

    // =========================================================================
    // link_resolved_packages: Source::Workspace with member in map
    // =========================================================================

    #[test]
    fn link_resolved_packages_workspace_source() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Create a member directory with content
        let member_dir = root.join("members/plugin-a");
        std::fs::create_dir_all(&member_dir).unwrap();
        std::fs::write(
            member_dir.join("aipm.toml"),
            "[package]\nname = \"plugin-a\"\nversion = \"1.0.0\"\n",
        )
        .unwrap();

        let config = InstallConfig {
            manifest_path: root.join("aipm.toml"),
            lockfile_path: root.join("aipm.lock"),
            store_path: root.join(".aipm/store"),
            links_dir: root.join(".aipm/links"),
            plugins_dir: root.join("plugins"),
            gitignore_path: root.join("plugins/.gitignore"),
            link_state_path: root.join(".aipm/links.toml"),
            workspace_root: None,
            locked: false,
            add_package: None,
            generated_by: "test".to_string(),
        };

        let resolution = resolver::Resolution {
            packages: vec![resolver::Resolved {
                name: "plugin-a".to_string(),
                version: Version::parse("1.0.0").unwrap(),
                source: resolver::Source::Workspace,
                checksum: String::new(),
                dependencies: vec![],
                features: BTreeSet::new(),
            }],
        };

        let manifest_str = "[package]\nname = \"plugin-a\"\nversion = \"1.0.0\"\n";
        let parsed = manifest::parse(manifest_str).unwrap();
        let mut members = BTreeMap::new();
        members.insert(
            "plugin-a".to_string(),
            workspace::Member {
                name: "plugin-a".to_string(),
                path: member_dir.clone(),
                version: "1.0.0".to_string(),
                manifest: parsed,
            },
        );

        let stub = StubRegistry;
        let result = link_resolved_packages(&config, &resolution, &members, None, &stub);
        assert!(result.is_ok(), "workspace linking should succeed: {:?}", result.err());
        let (installed, _) = result.unwrap();
        assert_eq!(installed, 1);

        // Verify the link was created
        assert!(root.join("plugins/plugin-a").exists());
    }

    #[test]
    fn collect_transitive_skips_missing_member() {
        // Workspace-resolved package NOT in members map → should be skipped
        let ws_resolved = vec![resolver::Resolved {
            name: "ghost".to_string(),
            version: Version::parse("1.0.0").unwrap(),
            source: resolver::Source::Workspace,
            checksum: String::new(),
            dependencies: vec![],
            features: BTreeSet::new(),
        }];
        let members = BTreeMap::new();
        let result = collect_transitive_registry_deps(vec![], &ws_resolved, &members);
        assert!(result.is_empty());
    }

    #[test]
    fn discover_workspace_members_workspace_root_no_manifest_file() {
        let tmp = tempfile::tempdir().unwrap();
        let ws_root = tmp.path().join("ws-root");
        std::fs::create_dir_all(&ws_root).unwrap();
        // No aipm.toml at all in workspace root

        let project = tmp.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join("aipm.toml"),
            "[package]\nname = \"proj\"\nversion = \"1.0.0\"\n",
        )
        .unwrap();

        let content = std::fs::read_to_string(project.join("aipm.toml")).unwrap();
        let parsed = manifest::parse_and_validate(&content, Some(project.as_path())).unwrap();

        let config = InstallConfig {
            manifest_path: project.join("aipm.toml"),
            lockfile_path: project.join("aipm.lock"),
            store_path: project.join(".aipm/store"),
            links_dir: project.join(".aipm/links"),
            plugins_dir: project.join(".ai"),
            gitignore_path: project.join(".ai/.gitignore"),
            link_state_path: project.join(".aipm/links.toml"),
            workspace_root: Some(ws_root),
            locked: false,
            add_package: None,
            generated_by: "test".to_string(),
        };

        let members = discover_workspace_members(&config, &parsed).unwrap();
        assert!(members.is_empty(), "no aipm.toml at workspace_root → no members");
    }

    #[test]
    fn collect_transitive_skips_member_with_no_deps() {
        // Member exists but has no [dependencies] section
        let mut members = BTreeMap::new();
        let m = make_member("plugin-a", "1.0.0", "");
        members.insert("plugin-a".to_string(), m);

        let ws_resolved = vec![resolver::Resolved {
            name: "plugin-a".to_string(),
            version: Version::parse("1.0.0").unwrap(),
            source: resolver::Source::Workspace,
            checksum: String::new(),
            dependencies: vec![],
            features: BTreeSet::new(),
        }];

        let result = collect_transitive_registry_deps(vec![], &ws_resolved, &members);
        assert!(result.is_empty(), "member with no deps → no transitive deps");
    }
}
