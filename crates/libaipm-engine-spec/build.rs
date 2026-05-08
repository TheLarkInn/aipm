//! Build script for `libaipm-engine-spec`.
//!
//! Pipeline:
//!   1. emit `cargo:rerun-if-changed` for the inputs.
//!   2. validate `data/engine-api-schema.json` against
//!      `schemas/engine-api.schema.json` (auto-detects 2020-12 dialect).
//!   3. deserialize the validated data into [`types::EngineApiSchemaFile`].
//!   4. assert the data file's `meta_schema_version` matches
//!      [`types::META_SCHEMA_VERSION`].
//!   5. emit `OUT_DIR/engine_data.rs` containing typed const tables.
//!
//! Codegen emits, into `OUT_DIR/engine_data.rs` (single `TokenStream`
//! formatted via `prettyplease::unparse`):
//!
//!   * `pub enum Engine { ... }` + `impl Engine` (`ALL`, `name`,
//!     `from_name`, `as_set`)
//!   * `bitflags::bitflags! { pub struct EngineSet: u32 { ... } }`
//!   * `pub const ENGINES: &[(Engine, EngineSpec)]`
//!   * `pub const TOOL_COMPATIBILITY: &[(&str, EngineSet)]`
//!   * `pub const HOOK_EVENTS_BY_ENGINE: &[(Engine, &[HookEventStatic])]`
//!   * `pub const FEATURES_BY_ENGINE: &[(Engine, EngineFeatureSet)]`
//!   * `pub mod paths { ... }` and `pub mod constraints { ... }`
//!
//! …plus a sibling `OUT_DIR/valid_tools.rs` containing the
//! `pub static VALID_TOOLS: phf::Set<&'static str>` produced by
//! `phf_codegen`.
//!
//! `println!` is denied workspace-wide so cargo directives are
//! emitted via `writeln!(io::stdout(), …)` instead.

use std::io::Write;

use proc_macro2::{Ident, Literal, Span, TokenStream};
use quote::quote;

#[path = "src/types.rs"]
mod types;

// Spec types referenced only by the generated `OUT_DIR/engine_data.rs`
// but never constructed inside the build script itself would be flagged
// `dead_code`. Touch each at module scope to silence the warning without
// resorting to an `#[allow]` attribute (which the workspace's
// `allow_attributes = "warn"` lint would catch).
const _: (
    Option<types::EngineSpec>,
    Option<types::HookEventStatic>,
    Option<types::EngineFeatureSet>,
) = (None, None, None);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    emit_rerun_directives()?;

    let parsed = load_and_validate()?;

    let tokens = generate_engine_module(&parsed);
    let formatted = prettyplease::unparse(&syn::parse2(tokens)?);

    let out_dir = std::env::var("OUT_DIR")?;
    let out_dir_path = std::path::Path::new(&out_dir);

    std::fs::write(out_dir_path.join("engine_data.rs"), formatted)?;
    write_valid_tools_phf(&parsed, out_dir_path)?;

    Ok(())
}

fn emit_rerun_directives() -> Result<(), std::io::Error> {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    for line in [
        "cargo:rerun-if-changed=data/engine-api-schema.json",
        "cargo:rerun-if-changed=../../schemas/engine-api.schema.json",
        "cargo:rerun-if-changed=src/types.rs",
        "cargo:rerun-if-changed=build.rs",
    ] {
        writeln!(handle, "{line}")?;
    }
    Ok(())
}

fn load_and_validate() -> Result<types::EngineApiSchemaFile, Box<dyn std::error::Error>> {
    let meta_schema = load_meta_schema()?;
    let data_text = std::fs::read_to_string("data/engine-api-schema.json")?;
    let data: serde_json::Value = serde_json::from_str(&data_text)?;

    let validator = jsonschema::validator_for(&meta_schema)?;
    if let Err(e) = validator.validate(&data) {
        return Err(format!(
            "data/engine-api-schema.json fails meta-schema validation: {e}\n\
             If the meta-schema needs updating, edit src/types.rs and re-run \
             `cargo run -p libaipm-engine-spec --bin export-schema`."
        )
        .into());
    }

    let parsed: types::EngineApiSchemaFile = serde_json::from_value(data)?;

    if parsed.meta_schema_version != types::META_SCHEMA_VERSION {
        return Err(format!(
            "meta_schema_version mismatch: data file says {data_v} but src/types.rs says {types_v}",
            data_v = parsed.meta_schema_version,
            types_v = types::META_SCHEMA_VERSION,
        )
        .into());
    }

    Ok(parsed)
}

fn load_meta_schema() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    match std::fs::read_to_string("../../schemas/engine-api.schema.json") {
        Ok(meta_schema_text) => Ok(serde_json::from_str(&meta_schema_text)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let stdout = std::io::stdout();
            let mut handle = stdout.lock();
            writeln!(
                handle,
                "cargo:warning=../../schemas/engine-api.schema.json not found; \
                 using schema derived from src/types.rs for validation"
            )?;
            Ok(serde_json::to_value(schemars::schema_for!(types::EngineApiSchemaFile))?)
        },
        Err(error) => Err(error.into()),
    }
}

fn generate_engine_module(parsed: &types::EngineApiSchemaFile) -> TokenStream {
    let engine_enum = generate_engine_enum(parsed);
    let engine_set = generate_engine_set(parsed);
    let engines_const = generate_engines_const(parsed);
    let tool_compat = generate_tool_compatibility(parsed);
    let hook_events = generate_hook_events_by_engine(parsed);
    let features_by_engine = generate_features_by_engine(parsed);
    let paths_module = generate_paths_module();
    let constraints_module = generate_constraints_module(parsed);

    quote! {
        // @generated by build.rs from data/engine-api-schema.json — do not edit.

        use crate::types::{EngineFeatureSet, EngineSpec, HookEventStatic};

        #engine_enum
        #engine_set
        #engines_const
        #tool_compat
        #hook_events
        #features_by_engine
        #paths_module
        #constraints_module
    }
}

fn generate_engine_enum(parsed: &types::EngineApiSchemaFile) -> TokenStream {
    let variants: Vec<Ident> = parsed
        .engines
        .iter()
        .map(|e| Ident::new(&to_pascal_case(&e.name), Span::call_site()))
        .collect();
    let names: Vec<&str> = parsed.engines.iter().map(|e| e.name.as_str()).collect();
    let flag_names: Vec<Ident> = parsed
        .engines
        .iter()
        .map(|e| Ident::new(&to_screaming_snake(&e.name), Span::call_site()))
        .collect();

    quote! {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Engine {
            #( #variants, )*
        }

        impl Engine {
            pub const ALL: &'static [Self] = &[ #( Self::#variants ),* ];

            #[must_use]
            pub const fn name(self) -> &'static str {
                match self {
                    #( Self::#variants => #names, )*
                }
            }

            #[must_use]
            pub fn from_name(name: &str) -> Option<Self> {
                match name {
                    #( #names => Some(Self::#variants), )*
                    _ => None,
                }
            }

            /// Return the `EngineSet` bit corresponding to this variant.
            ///
            /// Useful when converting an enum value to a flag for set-based
            /// membership checks (`set.contains(engine.as_set())`).
            #[must_use]
            pub const fn as_set(self) -> EngineSet {
                match self {
                    #( Self::#variants => EngineSet::#flag_names, )*
                }
            }
        }
    }
}

fn generate_engine_set(parsed: &types::EngineApiSchemaFile) -> TokenStream {
    let flag_names: Vec<Ident> = parsed
        .engines
        .iter()
        .map(|e| Ident::new(&to_screaming_snake(&e.name), Span::call_site()))
        .collect();
    let bit_values: Vec<u32> = parsed
        .engines
        .iter()
        .enumerate()
        .map(|(i, _)| 1u32 << u32::try_from(i).unwrap_or(0))
        .collect();

    quote! {
        bitflags::bitflags! {
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
            pub struct EngineSet: u32 {
                #( const #flag_names = #bit_values; )*
                const ALL = #( Self::#flag_names.bits() )|*;
            }
        }
    }
}

fn generate_engines_const(parsed: &types::EngineApiSchemaFile) -> TokenStream {
    let entries: Vec<TokenStream> =
        parsed.engines.iter().map(|bootstrap| generate_engine_entry(parsed, bootstrap)).collect();

    quote! {
        pub const ENGINES: &[(Engine, EngineSpec)] = &[ #( #entries, )* ];
    }
}

fn generate_engine_entry(
    parsed: &types::EngineApiSchemaFile,
    bootstrap: &types::EngineBootstrap,
) -> TokenStream {
    let variant = Ident::new(&to_pascal_case(&bootstrap.name), Span::call_site());
    let name = bootstrap.name.as_str();
    let package = bootstrap.package.as_str();
    let version = parsed.versions.get(&bootstrap.name).map_or("", String::as_str);

    let api = parsed.apis.get(&bootstrap.name);
    let manifest_search_paths = collect_strs(api.map(|a| a.manifest_search_paths.as_slice()));
    let settings_paths = collect_strs(api.map(|a| a.settings_paths.as_slice()));
    let folder_conventions = collect_strs(api.map(|a| a.folder_conventions.as_slice()));
    let convention_file_entries: Vec<TokenStream> = api
        .map(|a| {
            a.convention_files
                .iter()
                .map(|cf| {
                    let filename = cf.filename.as_str();
                    let paths: Vec<&str> = cf.convention_paths.iter().map(String::as_str).collect();
                    quote! { (#filename, &[#( #paths ),*]) }
                })
                .collect()
        })
        .unwrap_or_default();

    let marker_paths = marker_paths_for(&bootstrap.name);
    let marketplace_manifest_path = marketplace_manifest_path_for(&bootstrap.name);

    quote! {
        (
            Engine::#variant,
            EngineSpec {
                name: #name,
                package: #package,
                version: #version,
                marker_paths: &[#( #marker_paths ),*],
                marketplace_manifest_path: #marketplace_manifest_path,
                manifest_search_paths: &[#( #manifest_search_paths ),*],
                settings_paths: &[#( #settings_paths ),*],
                folder_conventions: &[#( #folder_conventions ),*],
                convention_files: &[#( #convention_file_entries ),*],
            }
        )
    }
}

fn collect_strs(slice: Option<&[String]>) -> Vec<&str> {
    slice.map(|s| s.iter().map(String::as_str).collect()).unwrap_or_default()
}

fn generate_tool_compatibility(parsed: &types::EngineApiSchemaFile) -> TokenStream {
    let tc = &parsed.tool_compatibility;

    let shared: Vec<TokenStream> = tc
        .shared_tools
        .iter()
        .map(|name| {
            let n = name.as_str();
            quote! { (#n, EngineSet::ALL) }
        })
        .collect();

    let exclusive: Vec<TokenStream> = tc
        .engine_exclusive_tools
        .iter()
        .map(|(name, support)| {
            let n = name.as_str();
            let set_expr = engine_set_expression(&support.supported_by);
            quote! { (#n, #set_expr) }
        })
        .collect();

    quote! {
        pub const TOOL_COMPATIBILITY: &[(&str, EngineSet)] = &[
            #( #shared, )*
            #( #exclusive, )*
        ];
    }
}

fn generate_hook_events_by_engine(parsed: &types::EngineApiSchemaFile) -> TokenStream {
    let entries: Vec<TokenStream> = parsed
        .engines
        .iter()
        .map(|bootstrap| {
            let variant = Ident::new(&to_pascal_case(&bootstrap.name), Span::call_site());
            let api = parsed.apis.get(&bootstrap.name);
            let events: Vec<TokenStream> = api
                .map(|a| a.hook_events.iter().map(hook_event_token).collect())
                .unwrap_or_default();
            quote! {
                (Engine::#variant, &[ #( #events ),* ])
            }
        })
        .collect();

    quote! {
        pub const HOOK_EVENTS_BY_ENGINE: &[(Engine, &[HookEventStatic])] = &[
            #( #entries, )*
        ];
    }
}

fn hook_event_token(event: &types::HookEvent) -> TokenStream {
    let name = event.name.as_str();
    let aliases: Vec<&str> = event.aliases.iter().map(String::as_str).collect();
    let deprecated = event.deprecated;
    let notes_expr =
        event.notes.as_deref().map_or_else(|| quote! { None }, |n| quote! { Some(#n) });
    quote! {
        HookEventStatic {
            name: #name,
            aliases: &[ #( #aliases ),* ],
            deprecated: #deprecated,
            notes: #notes_expr,
        }
    }
}

/// Conventional path constants used across the workspace.
///
/// These don't all appear directly in the schema's path lists (e.g.
/// `.ai`, `aipm.toml`, `marketplace.toml` are aipm- or claude-specific
/// conventions, not part of any engine's discovered API surface), but
/// having them in one place avoids the scattered string literals the
/// migration map (spec §5.8) is trying to clean up.
fn generate_paths_module() -> TokenStream {
    quote! {
        pub mod paths {
            //! Centralised path-string constants. Generated.

            pub const CLAUDE_PLUGIN_DIR:    &str = ".claude-plugin";
            pub const GITHUB_PLUGIN_DIR:    &str = ".github/plugin";
            pub const MARKETPLACE_JSON:     &str = "marketplace.json";
            pub const MARKETPLACE_TOML:     &str = "marketplace.toml";
            pub const PLUGIN_JSON:          &str = "plugin.json";
            pub const PLUGIN_TOML:          &str = "plugin.toml";
            pub const AIPM_TOML:            &str = "aipm.toml";
            pub const SETTINGS_JSON:        &str = "settings.json";
            pub const SETTINGS_LOCAL_JSON:  &str = "settings.local.json";
            pub const CLAUDE_DOT:           &str = ".claude";
            pub const GITHUB_DOT:           &str = ".github";
            pub const AI_DOT:               &str = ".ai";
        }
    }
}

/// Emit `pub mod constraints { … }` populated from each engine's
/// `manifest_fields[].constraints`. Field names are mapped to constant
/// prefixes via [`constraint_const_name`]; max/min length and regex
/// constraints become typed const items.
///
/// First-occurrence-wins via a `BTreeMap` keyed on field name, so the
/// emitted output is stable across builds.
fn generate_constraints_module(parsed: &types::EngineApiSchemaFile) -> TokenStream {
    use std::collections::BTreeMap;

    let mut by_field: BTreeMap<&str, &types::Constraints> = BTreeMap::new();
    for api in parsed.apis.values() {
        for field in &api.manifest_fields {
            if constraint_const_name(&field.name).is_none() {
                continue;
            }
            let has_constraint = field.constraints.max_length.is_some()
                || field.constraints.min_length.is_some()
                || field.constraints.regex.is_some();
            if has_constraint {
                by_field.entry(field.name.as_str()).or_insert(&field.constraints);
            }
        }
    }

    let mut consts: Vec<TokenStream> = Vec::new();
    for (field_name, constraints) in &by_field {
        let Some(prefix) = constraint_const_name(field_name) else {
            continue;
        };
        if let Some(max_len) = constraints.max_length {
            consts.push(emit_usize_const(prefix, "MAX_LEN", max_len));
        }
        if let Some(min_len) = constraints.min_length {
            consts.push(emit_usize_const(prefix, "MIN_LEN", min_len));
        }
        if let Some(regex) = &constraints.regex {
            let ident = Ident::new(&format!("{prefix}_REGEX"), Span::call_site());
            consts.push(quote! { pub const #ident: &str = #regex; });
        }
    }

    quote! {
        pub mod constraints {
            //! Centralised numeric/regex constraints from `manifest_fields`. Generated.
            #( #consts )*
        }
    }
}

fn emit_usize_const(prefix: &str, suffix: &str, value: u32) -> TokenStream {
    let ident = Ident::new(&format!("{prefix}_{suffix}"), Span::call_site());
    let val = usize::try_from(value).unwrap_or(0);
    let val_lit = Literal::usize_unsuffixed(val);
    quote! { pub const #ident: usize = #val_lit; }
}

/// Map a manifest-field name to the prefix used for its generated const.
/// Hand-rolled because schema field names are camelCase while consumers
/// expect `SCREAMING_SNAKE` constants with `PLUGIN_`-style namespacing.
fn constraint_const_name(field_name: &str) -> Option<&'static str> {
    match field_name {
        "name" => Some("PLUGIN_NAME"),
        "description" => Some("DESCRIPTION"),
        "postInstallMessage" => Some("POST_INSTALL_MSG"),
        _ => None,
    }
}

fn generate_features_by_engine(parsed: &types::EngineApiSchemaFile) -> TokenStream {
    let entries: Vec<TokenStream> = parsed
        .engines
        .iter()
        .map(|bootstrap| {
            let variant = Ident::new(&to_pascal_case(&bootstrap.name), Span::call_site());
            let kinds: Vec<types::FeatureKind> = parsed
                .apis
                .get(&bootstrap.name)
                .map(|a| a.features.iter().map(|f| f.kind).collect())
                .unwrap_or_default();
            let set_expr = feature_set_expression(&kinds);
            quote! { (Engine::#variant, #set_expr) }
        })
        .collect();

    quote! {
        pub const FEATURES_BY_ENGINE: &[(Engine, EngineFeatureSet)] = &[
            #( #entries, )*
        ];
    }
}

fn feature_set_expression(kinds: &[types::FeatureKind]) -> TokenStream {
    let mut terms = kinds.iter().map(|k| {
        let bit = feature_kind_bit_name(*k);
        let ident = Ident::new(bit, Span::call_site());
        quote! { EngineFeatureSet::#ident }
    });

    let Some(first) = terms.next() else {
        return quote! { EngineFeatureSet::empty() };
    };
    terms.fold(first, |acc, term| quote! { #acc.union(#term) })
}

const fn feature_kind_bit_name(kind: types::FeatureKind) -> &'static str {
    match kind {
        types::FeatureKind::Skill => "SKILL",
        types::FeatureKind::Agent => "AGENT",
        types::FeatureKind::Mcp => "MCP",
        types::FeatureKind::Hook => "HOOK",
        types::FeatureKind::OutputStyle => "OUTPUT_STYLE",
        types::FeatureKind::Lsp => "LSP",
        types::FeatureKind::Extension => "EXTENSION",
        types::FeatureKind::Command => "COMMAND",
    }
}

/// Build a const expression of type `EngineSet` from a list of engine
/// names. Single engine -> `EngineSet::FOO`; multiple engines ->
/// `EngineSet::FOO.union(EngineSet::BAR)…` (bitflags 2's `union` is
/// `const fn` so this works in static initializers).
fn engine_set_expression(engine_names: &[String]) -> TokenStream {
    let mut terms = engine_names.iter().map(|name| {
        let ident = Ident::new(&to_screaming_snake(name), Span::call_site());
        quote! { EngineSet::#ident }
    });

    let Some(first) = terms.next() else {
        return quote! { EngineSet::empty() };
    };
    terms.fold(first, |acc, term| quote! { #acc.union(#term) })
}

/// Engine-name-keyed marker paths.
///
/// The schema's `manifest_search_paths` enumerates marketplace manifest
/// locations, not plugin-marker (`plugin.json`) locations, so these stay
/// hand-rolled until the schema grows a `marker_paths` field.
fn marker_paths_for(engine_name: &str) -> &'static [&'static str] {
    match engine_name {
        "claude" => &[".claude-plugin/plugin.json", ".claude-plugin/plugin.toml"],
        "copilot" => &["plugin.json", ".github/plugin/plugin.json", ".claude-plugin/plugin.json"],
        _ => &[],
    }
}

/// Engine-name-keyed canonical marketplace manifest path.
///
/// Per Q2b "schema wins": copilot's path is `.json` (not `.toml` as the
/// previous hand-written libaipm `engine.rs` had) because that's what the
/// schema's `manifest_search_paths` reports. Claude's path is also `.json`
/// — the runtime format Claude consumes — fixed in #850.
fn marketplace_manifest_path_for(engine_name: &str) -> &'static str {
    match engine_name {
        "claude" => ".claude-plugin/marketplace.json",
        "copilot" => ".github/plugin/marketplace.json",
        _ => "",
    }
}

/// Emit `OUT_DIR/valid_tools.rs` containing a perfect-hash set of every
/// tool name + alias declared anywhere in the schema's `tool_calls`.
///
/// The set is keyed on `&'static str` so consumers can call
/// `VALID_TOOLS.contains(tool_name)` without allocation. Inputs are
/// sorted-deduped before being handed to `phf_codegen` so the generated
/// output is reproducible across builds.
fn write_valid_tools_phf(
    parsed: &types::EngineApiSchemaFile,
    out_dir: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut tool_names: Vec<&str> = parsed
        .apis
        .values()
        .flat_map(|api| api.tool_calls.iter())
        .flat_map(|tc| {
            std::iter::once(tc.name.as_str()).chain(tc.aliases.iter().map(String::as_str))
        })
        .collect();
    tool_names.sort_unstable();
    tool_names.dedup();

    let mut set = phf_codegen::Set::new();
    for name in &tool_names {
        set.entry(*name);
    }

    let path = out_dir.join("valid_tools.rs");
    let mut writer = std::fs::File::create(&path)?;
    writeln!(writer, "pub static VALID_TOOLS: phf::Set<&'static str> = {};", set.build())?;
    Ok(())
}

fn to_pascal_case(s: &str) -> String {
    s.split('-')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut chars = p.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect::<String>()
            })
        })
        .collect()
}

fn to_screaming_snake(s: &str) -> String {
    s.replace('-', "_").to_uppercase()
}
