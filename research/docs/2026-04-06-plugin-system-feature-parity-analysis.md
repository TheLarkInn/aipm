---
date: 2026-04-06 08:48:21 PDT
researcher: Claude Opus 4.6
git_commit: d3c39d0fcc42d49bc9d6e7aa345929192a4cfab6
branch: main
repository: aipm
topic: "Plugin system feature parity analysis — capabilities other tools have proposed vs aipm"
tags: [research, codebase, plugin, installer, cache, dependencies, marketplace, platform, engine, registry, feature-parity]
status: complete
last_updated: 2026-04-06
last_updated_by: Claude Opus 4.6
---

# Plugin System Feature Parity Analysis

## Research Question

Cross-check and analyze capabilities that other tools have proposed against aipm's plugin installation engine and system. Identify all tested capabilities (unit and e2e) present in other tools that are not yet implemented in aipm. Document these as features not yet implemented, using abstract capability names without revealing source paths.

## Summary

Analysis of other tools shows a mature plugin acquisition and lifecycle management system with **~257 unit tests across 18 source files**. It covers plugin caching, multi-engine validation, multi-source acquisition (local, GitHub, ADO Git, marketplace), transitive dependency resolution, platform compatibility, installed plugin registry management, and security features like path traversal prevention and owner allowlists.

aipm's current system has **~358 unit tests across 33 source files** and implements a full package management pipeline: backtracking constraint solver, content-addressable store with SHA-512, three-tier linking, lockfile reconciliation, workspace monorepo support, feature flags, and dependency overrides.

**The two systems solve different but overlapping problems.** Other tools have proposed a focus on _plugin acquisition from diverse remote sources with caching_, while aipm focuses on _package resolution, storage, and linking with a registry-backed model_. The gap analysis below identifies **13 capability areas** that other tools have proposed which aipm does not yet cover.

---

## Detailed Findings

### 1. Plugin Download Cache Subsystem (NOT IN AIPM)

Other tools have a full filesystem-backed download cache with configurable policies:

**Capabilities:**
- **Cache policies**: Auto (TTL-based freshness), CacheOnly (fail if not cached), SkipCache (bypass entirely), ForceRefresh (always re-download), CacheNoRefresh (use cache regardless of age)
- **Per-entry TTL overrides**: Individual plugins can have custom TTL values stored in the cache index
- **Global TTL**: Default 24-hour TTL for cache freshness
- **Garbage collection**: 30-day GC threshold for unused entries; installed entries are GC-exempt; unreferenced directories cleaned up; young directories preserved to avoid racing with concurrent `put()` operations
- **Cache index**: JSON file tracking spec keys, directory names (UUID-based), fetch timestamps, access timestamps, installed flags, and per-entry TTL overrides
- **File locking**: Read-modify-write of cache index under OS-level exclusive file lock
- **Session copy**: Cached plugins are copied to a per-session temp directory for isolation
- **Old entry cleanup**: When a plugin is re-cached, the previous UUID directory is removed outside the lock
- **Policy switching**: A cache instance can be cloned with a different policy while sharing the same root directory

**Unit tests (18 tests):**
1. `cache_policy_roundtrip` — Serialize/parse all 5 policy variants
2. `cache_miss_returns_none` — Auto policy returns None for unknown spec
3. `cache_put_and_get` — Store and retrieve plugin with content verification
4. `cache_skip_policy_always_misses` — SkipCache always returns None
5. `cache_only_errors_on_miss` — CacheOnly returns error for missing spec
6. `cache_force_refresh_always_misses` — ForceRefresh returns None even after put
7. `cache_stale_entry_returns_none` — TTL=0 makes entries immediately stale
8. `installed_still_respects_ttl` — Installed flag does not bypass TTL checks
9. `copy_to_session` — Copy cached content to session directory with content verification
10. `new_entry_dir_name_is_unique` — UUID-based directory names are unique
11. `gc_removes_old_entries` — GC removes entries with old `last_accessed` timestamps
12. `put_replaces_old_entry_dir` — Re-caching same spec replaces old directory, verifies content, removes old dir
13. `gc_removes_unreferenced_directories` — GC cleans up stray directories not in index
14. `gc_preserves_recent_unreferenced_directories` — GC does not remove young stray directories
15. `gc_preserves_installed_entries` — GC skips entries marked as installed
16. `per_entry_ttl_overrides_global` — Per-entry TTL=0 overrides large global TTL
17. `with_policy_shares_root` — Policy-switched cache shares same root directory
18. `set_entry_ttl_updates_stored_ttl` — TTL can be updated/cleared on existing entries

**aipm status:** aipm has a content-addressable store (`crates/libaipm/src/store/`) with SHA-512 hashing but **no download cache with TTL policies**. The store is for deduplicating file content, not for caching downloaded plugin archives. There is no cache policy system, no GC mechanism for downloads, and no per-entry TTL configuration.

---

### 2. Multi-Engine Validation (NOT IN AIPM)

Other tools support multiple AI tool engines with engine-specific plugin structure requirements:

**Capabilities:**
- **Engine types**: Claude (requires `.claude-plugin/plugin.json`) and Copilot (requires any of: `plugin.json`, `.github/plugin/plugin.json`, or `.claude-plugin/plugin.json`)
- **Local validation**: Checks filesystem for marker files
- **Remote validation**: Async validation via callback function that checks remote path existence
- **Engine-specific marketplace manifests**: Different manifest paths per engine (`.claude-plugin/marketplace.json` vs `.github/plugin/marketplace.json`)
- **Human-readable error messages**: Single-marker: "missing X"; multi-marker: "expected at least one of: X, Y, Z"

**Unit tests (8 tests):**
1. `claude_validate_local_valid` — Valid Claude plugin directory passes
2. `claude_validate_local_missing_marker` — Missing marker produces descriptive error
3. `copilot_validate_local_with_github_json` — Copilot accepts `.github/plugin/plugin.json`
4. `copilot_validate_local_missing_all_markers` — Copilot fails when all 3 markers missing
5. `claude_validate_remote_valid` — Async remote validation succeeds
6. `claude_validate_remote_missing` — Async remote validation fails correctly
7. `copilot_validate_remote_with_root_json` — Copilot remote accepts `plugin.json`
8. `copilot_validate_remote_missing_all` — Copilot remote fails when all missing

**aipm status:** aipm does not have multi-engine validation. The manifest system (`crates/libaipm/src/manifest/`) validates plugin type (Skill, Agent, Mcp, Hook, Lsp, Composite) but does not validate engine-specific directory structures. No async remote validation exists.

---

### 3. Plugin Path Security Validation (PARTIALLY IN AIPM)

**Capabilities:**
- **Validated path type**: Wrapper type that guarantees paths are relative, non-empty, and traversal-free
- **Traversal detection**: Rejects `..` components, URL-encoded `%2e%2e` variants, absolute paths (Unix `/` and Windows `C:\`)
- **Folder name extraction**: Derives plugin folder name from the final path component
- **Display and AsRef traits**: Clean string representation

**Unit tests (11 tests):**
1. `validate_empty_path` — Empty string rejected
2. `validate_path_traversal_start` — `../etc/passwd` rejected
3. `validate_path_traversal_middle` — `foo/../../../etc/passwd` rejected
4. `validate_path_traversal_encoded` — `foo/%2e%2e/bar` rejected
5. `validate_absolute_path_unix` — `/etc/passwd` rejected
6. `validate_valid_paths` — Parameterized: simple, nested, deeply nested, dashes, underscores, dots
7. `plugin_path_new_valid` — Constructs validated path successfully
8. `plugin_path_new_invalid_traversal` — Traversal rejected at construction
9. `plugin_path_display` — Display trait works correctly
10. `plugin_path_folder_name` — Extracts final component from multi-segment path
11. `plugin_path_folder_name_simple` — Extracts name from single-segment path

**aipm status:** aipm validates package names (`crates/libaipm/src/manifest/validate.rs`) using a regex-like pattern (`(@[a-z0-9-]+/)?[a-z0-9][a-z0-9-]*`) but does **not** have a dedicated validated path type for plugin paths, URL-encoded traversal detection, or absolute path rejection for plugin identifiers.

---

### 4. Platform Compatibility Checking (PARTIALLY IN AIPM)

**Capabilities:**
- **Runtime OS detection**: Returns platform identifiers for Windows, Linux, macOS
- **Compatibility result type**: Universal (no restrictions), Compatible (current OS in list), Incompatible (with both declared and current platforms for error messages)
- **Metadata-driven**: Reads platform restrictions from plugin metadata file

**Unit tests (7 tests):**
1. `current_platforms_includes_known_os` — At least one known platform detected
2. `current_platforms_exactly_one` — Exactly one platform returned
3. `compatibility_universal_when_no_platforms` — None platforms = universal
4. `compatibility_universal_when_empty_platforms` — Empty list = universal
5. `compatibility_compatible_with_current_os` — Current OS in list = compatible
6. `compatibility_incompatible_different_os` — Different OS = incompatible with details
7. `compatibility_unknown_platform_no_match` — Unknown-only platform = incompatible

**aipm status:** aipm's manifest supports `environment.platforms` but does not have a runtime platform detection and compatibility checking module. The `Environment` type in `crates/libaipm/src/manifest/types.rs` declares platform requirements but there is no `check_platform_compatibility()` function.

---

### 5. Plugin Metadata System (PARTIALLY IN AIPM)

Other tools use a separate metadata file (`agency.json`) alongside plugin content:

**Capabilities:**
- **Platform restrictions**: Optional list of supported platforms
- **Dependency declarations**: Plugin specs parsed at deserialization time
- **Engine compatibility**: List of engines the plugin supports (with unknown engine forward-compatibility)
- **Remote source redirect**: Allows a stub plugin to redirect to an external repository (GitHub)
- **Forward compatibility**: Unknown JSON keys silently ignored; unknown platform/engine values preserved as `Unknown(String)` variants

**Unit tests (22 tests):**
1. `deserialize_empty_object` — Empty JSON = all defaults
2. `deserialize_null_platforms` — Null = None
3. `deserialize_empty_platforms` — Empty array = Some(vec![])
4. `deserialize_all_platforms` — All three platforms parsed
5. `deserialize_unknown_platform_preserved` — Unknown value preserved as variant
6. `deserialize_ignores_unknown_fields` — Extra JSON keys ignored
7. `deserialize_engines_none_when_missing` — Missing engines = None
8. `deserialize_engines_multiple` — Multiple engines parsed
9. `deserialize_engines_empty_array` — Empty engines = Some(vec![])
10. `deserialize_source_none_when_missing` — No source field = None
11. `deserialize_source_with_path` — GitHub source with subdirectory path
12. `deserialize_source_without_path` — GitHub source without path
13. `deserialize_full_agency_json` — All fields together
14. `deserialize_dependencies` — Plugin spec dependencies parsed
15. `deserialize_no_dependencies_field` — Missing = None
16. `platform_display` — Parameterized Display trait
17. `platform_roundtrip` — Parameterized parse/display roundtrip
18. `unknown_string_roundtrips` — Unknown platform roundtrips
19. `read_plugin_metadata_missing_file` — Missing file returns None
20. `read_plugin_metadata_valid` — Valid file parsed correctly
21. `read_plugin_metadata_empty_object` — Empty object = defaults
22. `read_plugin_metadata_malformed` — Invalid JSON returns None with warning

**aipm status:** aipm has `Manifest` with `Package`, `Environment`, `Components`, `Features`, `Install`, and `DependencySpec` types but does not have: remote source redirects, engine compatibility declarations, forward-compatible unknown variant handling for platforms/engines, or a separate metadata file for plugin-level metadata distinct from the package manifest.

---

### 6. Transitive Dependency Resolution with Cache Integration (PARTIALLY IN AIPM)

Other tools resolve plugin dependencies using BFS with cache policy inheritance:

**Capabilities:**
- **BFS discovery**: Discovers transitive dependencies breadth-first
- **Depth limiting**: Maximum 10 levels of transitive dependencies
- **Version conflict detection**: Detects when two parents require the same dependency at different git refs
- **Folder name collision detection**: Prevents two different plugins from having the same folder name (case-insensitive)
- **Cycle detection**: Topological sort via Kahn's algorithm
- **Cache policy inheritance**: Child dependencies inherit parent's cache policy; per-spec overrides take precedence
- **Human-readable chain**: Error messages show full dependency chain (A -> B -> C)
- **Canonical keys**: Strip git refs for identity comparison; same key + different ref = conflict

**Unit tests (12 tests):**
1. `topological_sort_no_deps` — Two independent nodes sorted
2. `topological_sort_linear_chain` — A->B->C sorted correctly
3. `topological_sort_diamond` — Diamond dependency (A->B,C->D) sorted
4. `topological_sort_cycle_detected` — A->B->A cycle returns error
5. `canonical_key_github_strips_ref` — Ref stripped from canonical key
6. `canonical_key_github_no_ref` — No ref produces clean key
7. `canonical_key_local` — Local spec key format
8. `read_dependency_specs_no_file` — Missing metadata = no deps
9. `read_dependency_specs_with_deps` — Dependency specs read from metadata
10. `read_dependency_specs_invalid_spec_skipped` — Invalid specs cause parse failure, returns empty
11. (BFS integration tests exist in the async `resolve_dependencies` function but are not unit-testable without network mocks)

**aipm status:** aipm has a **more sophisticated** backtracking constraint solver (`crates/libaipm/src/resolver/`) with version unification, cross-major coexistence, feature flags, and override rules (~37 tests). However, aipm's resolver does **not** have: cache policy inheritance for dependencies, folder name collision detection during resolution, depth limiting, or BFS-based discovery from remote metadata files. aipm resolves from a registry index, not from individual plugin metadata files.

---

### 7. Multi-Source Plugin Acquisition (NOT IN AIPM)

Other tools acquire plugins from four distinct source types:

**Capabilities:**
- **Local source**: Copy from filesystem path with validation
- **GitHub source**: Download via GitHub API with owner/repo inference from current git remote
- **ADO Git source**: Download via Azure DevOps API with org/project/repo inference
- **Marketplace source**: Download from marketplace manifest in a remote repository
- **Source redirect**: Follow `agency.json` source field to redirect to external repo (1 level deep)
- **Plugin spec format**: `source:identifier` with source-specific parsing
- **Plugin manager**: Manages temp directories, acquires multiple plugins, resolves dependencies, cleans up on drop

**GitHub source tests (15+ tests):**
1. `parse_repo_coords_valid` — Parse `owner/repo` format
2. `parse_repo_coords_missing_repo` — Missing repo errors
3. `parse_repo_coords_too_many_parts` — Extra segments error
4. `parse_repo_coords_empty_owner` — Empty owner errors
5. `parse_github_url_ssh` — Parse `git@github.com:owner/repo.git`
6. `parse_github_url_https_with_git` — Parse HTTPS with `.git`
7. `parse_github_url_https_without_git` — Parse HTTPS without `.git`
8. `parse_github_url_not_github` — Non-GitHub URL errors
9. `validate_owner_valid` — Parameterized valid owners
10. `validate_owner_empty/starts_with_hyphen/ends_with_hyphen/invalid_chars/too_long` — Owner validation
11. `validate_git_ref_valid` — Parameterized valid refs
12. `validate_git_ref_empty/double_dot` — Ref validation
13. `parse_github_spec_fully_qualified` — Full `owner/repo:path@ref` parsing
14. `parse_github_spec_hash_not_a_ref_delimiter` — `#` is not a ref delimiter (kept in path)
15. `parse_github_spec_no_ref` — Spec without ref
16. `parse_github_spec_path_traversal` — Path traversal rejected
17. `parse_github_spec_invalid_owner` — Invalid owner rejected
18. Additional tests for owner allowlist enforcement, display formatting, folder naming

**ADO Git source tests (16+ tests):**
1. `parse_repo_coords_fully_qualified` — Parse `org/project/repo`
2. `parse_repo_coords_with_spaces` — Spaces in names accepted
3. `parse_repo_coords_too_many_slashes` — 4+ segments rejected
4. `validate_ado_name_valid/empty/dot/dotdot/forbidden_chars/null_byte/too_long/max_length_ok` — Name validation (8 tests)
5. `parse_ado_git_spec_hash_not_a_ref_delimiter` — `#` kept in path
6. `parse_ado_git_spec_at_ref` — `@` is the ref delimiter
7. `parse_ado_git_spec_invalid_ref_characters` — Invalid ref chars rejected
8. Additional tests for display, folder naming

**aipm status:** aipm uses a **git-based registry** (`crates/libaipm/src/registry/git.rs`) that clones/fetches a package index and downloads tarballs via HTTP. It does **not** support: direct GitHub API acquisition, ADO Git API acquisition, repo inference from current git remote, marketplace manifest resolution, source redirects, or plugin specs with `source:identifier` format. aipm uses `name@version` package specs with a centralized registry.

---

### 8. Marketplace Plugin Acquisition (NOT IN AIPM)

Other tools support acquiring plugins from marketplace repositories with rich manifest parsing:

**Marketplace spec parsing tests (40+ tests):**
1. `parse_github_short_format` — `plugin@owner/repo`
2. `parse_github_url_valid` — Parameterized HTTPS URLs
3. `parse_ado_url_valid` — Parameterized ADO URLs (dev.azure.com, visualstudio.com)
4. `plugin_name_extracted` — Name before `@`
5. `folder_name_uses_plugin_name` — Folder = plugin name
6. `parse_hash_ref_github_short` — `plugin@owner/repo#main`
7. `parse_hash_ref_ado_url` — ADO URL with `#ref`
8. `parse_hash_ref_github_url` — GitHub URL with `#ref`
9. `parse_no_ref_produces_none` — No `#` = None
10. `parse_empty_ref_after_hash` — `#` with nothing after = error
11. `parse_hash_ref_display_roundtrip` — Display produces parseable string
12. `parse_missing_at_symbol` — Missing `@` = error
13. `parse_empty_plugin_name` — Empty name = error
14. `parse_empty_location` — Empty location = error
15. `parse_invalid_github_format` — Single segment = error
16. `parse_unknown_url_type` — GitLab URL = error
17. `parse_invalid_ado_url` — Missing `_git` segment = error
18. `parse_whitespace_around_at` — Whitespace trimmed
19. `parse_whitespace_only_plugin_name/location` — Whitespace-only = error
20. `parse_special_chars_in_plugin_name` — Underscores, dots, uppercase, mixed
21. `parse_preset_alias_in_location` — Parameterized preset aliases (playground, curated, company)
22. `playground_alias_expands_to_*` — Preset expansion verified
23. `curated_alias_expands_to_*` — Preset expansion verified
24. `company_alias_expands_to_same_as_curated` — Alias equivalence
25. `parse_multiple_at_symbols` — ADO URLs with username in URL
26. `parse_github_url_missing_repo/only_domain/trailing_slash` — Edge cases
27. `parse_ado_url_missing_git_segment` — Edge case
28. `parse_github_special_chars_in_location` — Hyphens, underscores, dots in location
29. `parse_local_absolute_path/relative_dot_slash/relative_parent/absolute_unix` — Local paths
30. `parse_local_path_hash_not_treated_as_ref` — `#` in local path is literal
31. `parse_local_windows_drive` — Windows `C:\` paths (Windows-only test)
32. `parse_local_display` — Display for local paths

**Marketplace manifest tests (30+ tests):**
1. `parse_manifest` — Basic manifest with 2 plugins
2. `find_plugin_by_name` — Lookup by name
3. `find_plugin_not_found` — Missing name = None
4. `available_names` — List all names
5. `parse_manifest_invalid_json` — Invalid JSON = error
6. `parse_manifest_empty_plugins` — Empty array valid
7. `parse_manifest_no_metadata/with_plugin_root/with_empty_plugin_root/with_null_plugin_root` — Metadata handling
8. `parse_source_string` — Relative path source
9. `parse_source_github_object` — GitHub source object
10. `parse_source_github_with_path/url_instead_of_repo/url_and_path/ref_and_sha` — GitHub variants
11. `parse_source_url_object/with_path/with_ref` — URL source variants
12. `parse_source_git_subdir_aliases_to_git_url/with_ref/missing_url/missing_path` — Legacy alias
13. `parse_source_npm_unsupported/pip_unsupported` — Unsupported source types
14. `parse_mixed_source_types` — Multiple source types in one manifest
15. `parse_source_object_missing_source_field` — Defaults to "unknown"
16. `parse_source_github_missing_repo/empty_repo` — Error cases
17. `parse_source_url_missing_url/empty_url` — Error cases
18. `parse_source_empty_ref_treated_as_none/empty_sha_treated_as_none` — Empty strings = None
19. Additional tests for source path normalization, traversal rejection, plugin root application

**aipm status:** aipm does not have marketplace support. There is no marketplace manifest format, no preset aliases, no marketplace spec parsing (`plugin@location` format), no multi-source manifest types (relative path, GitHub, URL, git-subdir), and no plugin root metadata. aipm's manifest is a TOML `aipm.toml` file with `[dependencies]` that reference registry packages.

---

### 9. Installed Plugin Registry (NOT IN AIPM)

Other tools maintain a persistent registry of "installed" plugins:

**Capabilities:**
- **Persistent storage**: JSON file at a well-known location
- **Engine-specific installation**: Plugins can be installed for specific engines or all engines
- **Additive engine updates**: Re-installing with new engines adds to existing set
- **Reset to all**: Installing with empty engines resets to "all engines"
- **Name conflict detection**: Case-insensitive folder name conflicts blocked between different specs on overlapping engines
- **Engine-scoped uninstall**: Remove specific engine(s) from a plugin; full uninstall if no engines remain
- **Spec resolution**: Resolve by full spec or by folder name shorthand (with engine disambiguation)
- **Cache policy per-plugin**: Individual plugins can have custom cache policies and TTLs
- **Validate before download**: Pre-flight name conflict check to avoid wasted network I/O
- **File-locked updates**: Read-modify-write under file lock for concurrent access safety

**Unit tests (35+ tests):**
1. `install_new_plugin` — New plugin added
2. `install_additive_engines` — Second install adds engine
3. `install_no_engine_resets_to_all` — Empty engines = all
4. `install_additive_deduplicates` — Same engine not duplicated
5. `install_specific_engine_when_all_is_noop` — Adding specific engine when already "all" is noop
6. `uninstall_existing/missing` — Remove by spec
7. `uninstall_engine_from_explicit_list` — Remove one engine
8. `uninstall_engine_from_all_engines` — Expand "all" to explicit list then remove
9. `uninstall_last_engine_removes_plugin` — No engines left = full uninstall
10. `uninstall_engine_missing_plugin` — Missing plugin = false
11. `applies_to_all_engines/specific_engine` — Engine filter
12. `plugins_for_engine` — Get plugins for specific engine
13. `serialization_roundtrip` — JSON serialize/deserialize
14. `name_conflict_same_name_all_engines` — Same folder name, different spec, all engines = error
15. `name_conflict_allowed_non_overlapping_engines` — Non-overlapping engines = ok
16. `name_conflict_adding_engine_causes_overlap` — Adding engine creates overlap = error
17. `name_conflict_all_engines_vs_specific` — All vs specific = error
18. `no_conflict_same_spec_reinstall` — Same spec reinstall always ok
19. `no_conflict_different_names` — Different names always ok
20. `name_conflict_case_insensitive` — Case-insensitive name matching
21. `resolve_spec_exact_match` — Resolve by full spec
22. `resolve_spec_by_name_unique` — Resolve by folder name
23. `resolve_spec_by_name_ambiguous` — Ambiguous name = error with list
24. `resolve_spec_by_name_disambiguated_by_engine` — Engine filter resolves ambiguity
25. `resolve_spec_not_found` — Missing = error
26. `engines_overlap_*` — 5 overlap tests (both empty, one empty, matching, no match, case insensitive)
27. `install_stores_cache_policy` — Cache policy stored
28. `reinstall_updates_cache_policy` — Policy updated on reinstall
29. `reinstall_none_preserves_existing_cache_policy` — None = preserve existing
30. `cache_policy_serialization_roundtrip` — Policy survives JSON roundtrip
31. `ttl_stored_with_non_auto_policy` — TTL stored regardless of policy

**aipm status:** aipm does not have an installed plugin registry. Packages are managed via `aipm.toml` manifest and `aipm.lock` lockfile. There is no concept of engine-specific installation, no install-time name conflict detection, no spec resolution by folder name shorthand, and no per-plugin cache policy configuration. The closest equivalent is the manifest's `[dependencies]` section, but it lacks the engine-scoping and persistent registry semantics.

---

### 10. Plugin Spec Parsing Framework (NOT IN AIPM)

Other tools have a unified plugin spec parsing system:

**Capabilities:**
- **Unified enum**: `Local`, `GitHub`, `AdoGit`, `Marketplace` variants
- **Source-type prefix**: `local:`, `github:`, `ado-git:`, `market:` (with aliases `marketplace`, `mp`)
- **Case-insensitive source**: Source prefix is case-insensitive
- **Canonical key**: Identity without git ref (for conflict detection)
- **Git ref access**: Extract ref from spec
- **Canonicalization**: Resolves local paths to absolute canonical paths
- **Serde support**: Serialize/deserialize via string representation
- **Folder name derivation**: Source-specific folder name extraction
- **Duplicate detection**: Case-insensitive folder name collision detection across specs
- **Telemetry integration**: Source type, engine, duration logged for each acquisition

**Unit tests from the plugin spec (35+ tests):**
1. `parse_plugin_spec_local/local_absolute` — Local path parsing
2. `canonicalize_local_existing/nonexistent` — Path canonicalization
3. `parse_plugin_spec_case_insensitive` — Case-insensitive source
4. `parse_plugin_spec_invalid_format` — No colon = error
5. `parse_plugin_spec_unknown_source` — Unknown source = error
6. `parse_plugin_spec_with_colon_in_path` — Colon in path handled
7. `parse_plugin_spec_empty_identifier` — Empty after colon = error
8. `plugin_spec_display_local` — Display for local
9. `plugin_spec_folder_name_*` — Folder name for each source type
10. `parse_plugin_spec_ado_git_*` — ADO Git spec variants (fully qualified, no ref, case insensitive, path traversal, display, folder name)
11. `parse_plugin_spec_github_*` — GitHub spec variants (fully qualified, no ref, case insensitive, display, folder name, path traversal, invalid owner)
12. `parse_plugin_spec_marketplace_*` — Marketplace spec variants (github, ado, case insensitive, display, folder name, missing at, unknown url)
13. `acquire_detects_duplicate_folder_names` — Async test: same folder from different specs = error
14. `acquire_detects_duplicate_folder_names_case_insensitive` — Case-insensitive collision
15. `plugin_manager_plugins_map` — Map of name -> path
16. `extract_plugin_dirs_from_args` — Parse `--plugin-dir` from CLI args
17. `set_installed_policy_normalizes_spec_case` — Policy key normalized
18. `set_installed_policy_fallback_on_invalid_spec` — Invalid spec falls back to raw string

**aipm status:** aipm uses `name@version` format for package specs with a centralized registry model. It does not have: multi-source spec parsing, source-type prefixes, repo inference, canonical key extraction for conflict detection, or folder-name-based duplicate detection. aipm's closest analogue is `DependencySpec` (Simple string or Detailed object) in the manifest.

---

### 11. OS-Level File Locking (PARTIALLY IN AIPM)

**Capabilities:**
- **Exclusive lock**: OS-level blocking exclusive lock on the data file itself (no separate `.lock` files)
- **Read-modify-write**: Read content, modify in-memory, write back — all under lock
- **Lock on drop**: Locks released when handle dropped (including on process crash)
- **Parent directory creation**: Creates parent directories if needed

**aipm status:** aipm has advisory locking via `fs2::FileExt` in the store (`crates/libaipm/src/store/mod.rs`) using a `.lock` file. The locking pattern is similar but uses a separate lock file rather than locking the data file itself. aipm's lock does not have the read-modify-write-under-lock pattern used by other tools' cache index and installed registry.

---

### 12. Owner Allowlist Security (NOT IN AIPM)

**Capabilities:**
- **Compile-time allowlist**: A text file of allowed GitHub owners compiled into the binary
- **Pipeline enforcement**: Enforced in pipeline environments and when an environment variable is set
- **Case-insensitive matching**: Owner comparison is case-insensitive
- **Descriptive errors**: Error message includes the allowed list and enforcement explanation

**Tests (5+ tests):**
1. `allowed_owners_not_empty` — At least one owner in list
2. `allowed_owners_contains_expected` — Expected owner present
3. `is_owner_allowed_case_insensitive` — Case-insensitive match
4. `unknown_owner_outside_pipeline` — Allowed outside pipeline
5. `unknown_owner_rejected_when_enforced` — Rejected when enforced

**aipm status:** aipm does not have owner-based security restrictions. The registry model trusts the registry index. There is no allowlist for package sources, no pipeline-aware enforcement, and no owner validation beyond package name syntax.

---

### 13. Engine Compatibility Warning (NOT IN AIPM)

**Capabilities:**
- **Engine name enum**: Copilot, Claude, Unknown(String) — with case-insensitive matching
- **Metadata-driven**: Reads `engines` field from plugin metadata
- **Non-blocking**: Warns but does not prevent installation of engine-incompatible plugins
- **Forward-compatible**: Unknown engine names preserved, not rejected

**aipm status:** aipm does not have engine compatibility checking. The manifest's `PluginType` enum (Skill, Agent, Mcp, Hook, Lsp, Composite) describes plugin capability, not engine compatibility.

---

## Feature Parity Matrix

| Capability | Other Tools | aipm | Gap? |
|---|---|---|---|
| Plugin download cache with TTL policies | Yes (18 tests) | No | **GAP** |
| Multi-engine validation (Claude/Copilot markers) | Yes (8 tests) | No | **GAP** |
| Plugin path security (traversal, URL-encoded) | Yes (11 tests) | Partial (name validation only) | **PARTIAL GAP** |
| Platform compatibility checking | Yes (7 tests) | Partial (field exists, no checker) | **PARTIAL GAP** |
| Plugin metadata (platforms, engines, source redirect) | Yes (22 tests) | Partial (manifest, no redirect/engine) | **PARTIAL GAP** |
| Transitive dependency resolution with caching | Yes (12 tests) | Yes (37 tests, more sophisticated) | **PARTIAL GAP** (cache integration missing) |
| Multi-source acquisition (GitHub, ADO, local) | Yes (35+ tests) | No (registry-only) | **GAP** |
| Marketplace plugin acquisition | Yes (70+ tests) | No | **GAP** |
| Installed plugin registry (persistent, engine-scoped) | Yes (35+ tests) | No | **GAP** |
| Plugin spec parsing (`source:identifier`) | Yes (35+ tests) | No (uses `name@version`) | **GAP** |
| OS-level file locking | Yes | Yes (different pattern) | Minimal gap |
| Owner allowlist security | Yes (5 tests) | No | **GAP** |
| Engine compatibility warning | Yes | No | **GAP** |
| Backtracking constraint solver | No | Yes (37 tests) | aipm ahead |
| Content-addressable store (SHA-512) | No | Yes (37 tests) | aipm ahead |
| Three-tier linking pipeline | No | Yes (52 tests) | aipm ahead |
| Lockfile with reconciliation | No | Yes (29 tests) | aipm ahead |
| Feature flags & dependency overrides | No | Yes (15 tests) | aipm ahead |
| Workspace monorepo support | No | Yes (20+ tests) | aipm ahead |
| Registry with git-based index | No | Yes (42 tests) | aipm ahead |
| Lifecycle script security | No | Yes (5 tests) | aipm ahead |

---

## Integration / E2E Tests in Other Tools

Other tools have **4 integration tests** in a single test file covering plugin spawning:

1. `spawn_builtin_msft_learn` — Spawns a built-in plugin and verifies output
2. `spawn_builtin_asa` — Spawns another built-in plugin
3. `spawn_agency_remote` — Spawns a remote plugin
4. `spawn_non_builtin_local_preserves_on_failure` — Verifies local plugin state preserved on failure

There are **no BDD/cucumber feature files** in other tools analyzed.

**aipm status:** aipm has **19 BDD feature files** covering install, link, resolution, lockfile, patching, features, security, publishing, search, yanking, workspace, migration, validation, versioning, portability, and more. aipm's test infrastructure is more comprehensive for specification-driven testing.

---

## Test Count Summary

| System | Unit Tests | Integration/E2E Tests | BDD Feature Files | Total Test Files |
|---|---|---|---|---|
| Other Tools | ~253 | 4 | 0 | 18 |
| aipm | ~358 | 0 (BDD covers this) | 19 | 33+ |

---

## Code References (aipm)

- `crates/libaipm/src/store/mod.rs` — Content-addressable store (closest to cache)
- `crates/libaipm/src/store/hash.rs` — SHA-512 hashing
- `crates/libaipm/src/resolver/mod.rs` — Backtracking constraint solver
- `crates/libaipm/src/resolver/overrides.rs` — Dependency overrides
- `crates/libaipm/src/linker/pipeline.rs` — Link pipeline
- `crates/libaipm/src/linker/directory_link.rs` — Symlink/junction creation
- `crates/libaipm/src/linker/security.rs` — Lifecycle script security
- `crates/libaipm/src/lockfile/mod.rs` — Lockfile read/write/validate
- `crates/libaipm/src/lockfile/reconcile.rs` — Lockfile reconciliation
- `crates/libaipm/src/installer/pipeline.rs` — Install pipeline (73 tests)
- `crates/libaipm/src/installer/manifest_editor.rs` — TOML editing
- `crates/libaipm/src/registry/git.rs` — Git-based registry
- `crates/libaipm/src/registry/config.rs` — Registry routing
- `crates/libaipm/src/manifest/mod.rs` — Manifest parsing
- `crates/libaipm/src/manifest/validate.rs` — Manifest validation
- `crates/libaipm/src/version.rs` — Semver version/requirement

## Historical Context (from research/)

- `research/docs/2026-03-26-install-update-link-lockfile-implementation.md` — Implementation readiness for install/update/link/lockfile
- `research/docs/2026-03-28-aipm-install-zero-packages-fixtures.md` — Debugging zero-packages resolution
- `research/tickets/2026-03-28-129-workspace-dependencies-linking.md` — Workspace dep linking (#129)
- `research/docs/2026-03-09-npm-core-principles.md` — npm architecture reference
- `research/docs/2026-03-09-pnpm-core-principles.md` — pnpm architecture reference
- `research/docs/2026-03-09-cargo-core-principles.md` — Cargo architecture reference

## Related Research

- `specs/2026-03-26-install-update-link-lockfile.md` — Primary install/link/lockfile spec
- `specs/2026-03-28-workspace-dependency-linking.md` — Workspace dep linking spec
- `specs/2026-03-09-aipm-technical-design.md` — Foundational technical design

## Open Questions

1. **Should aipm adopt a download cache?** The other tools's TTL-based cache with 5 policies is a significant feature for CI/CD workflows. aipm's content-addressable store handles file deduplication but not download caching.

2. **Should aipm support multi-source acquisition?** The other tools's `source:identifier` spec format supports GitHub, ADO Git, local, and marketplace sources. This is architecturally different from aipm's registry model — adding it would require a parallel acquisition path.

3. **Should aipm add engine-specific validation?** If aipm is targeting multiple AI tool engines (Claude, Copilot), engine-specific plugin structure validation would prevent runtime failures.

4. **Should aipm implement an installed plugin registry?** The other tools's engine-scoped persistent registry is a distinct concept from aipm's manifest-based dependency management. It could complement the existing system for globally-installed plugins.

5. **What's the priority order for these gaps?** The 13 capability gaps range from quick wins (path security, platform checking) to major features (marketplace acquisition, download cache). A prioritized roadmap would help focus effort.
