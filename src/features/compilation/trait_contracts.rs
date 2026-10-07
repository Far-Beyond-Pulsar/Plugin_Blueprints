//! Resolves Blueprint trait assignments and emits their Rust declarations.
//!
//! Trait assignment is metadata today. The generated module makes each
//! referenced Rust trait declaration available to the class crate, but it
//! does not claim that the Blueprint graph provides method implementations.

use std::collections::BTreeSet;
use std::path::{Component, Path};

use ui_types_common::{TraitAsset, TypeIndex, TypeKind};

const TYPE_INDEX_PATH: &str = "type-index/index.json";

/// Build one generated Rust module containing referenced trait declarations.
pub(super) fn generate_module(project_root: &Path, raw_paths: &[String]) -> Result<String, String> {
    let paths = normalize_trait_paths(raw_paths)?;
    if paths.is_empty() {
        return Ok(String::new());
    }

    let type_index = load_type_index(project_root)?;
    let mut module = String::from(
        "//! Trait declarations selected by this Blueprint.\n\
         //! Generated from project `.trait.json` assets.\n\
         //! Trait assignment does not yet bind Blueprint functions to these methods.\n\n\
         pub mod trait_contracts {\n",
    );

    for path in paths {
        let asset = load_trait(project_root, &path)?;
        validate_trait_asset(&asset, &type_index, &path)?;
        reject_unresolvable_aliases(&asset, &path)?;

        let generated = ui_types_common::codegen::generate_trait(&asset)
            .map_err(|error| format!("Could not generate Rust for trait '{path}': {error}"))?;
        syn::parse_file(&generated)
            .map_err(|error| format!("Trait '{path}' generated invalid Rust source: {error}"))?;

        let module_name = trait_module_name(&asset, &path);
        module.push_str(&format!(
            "    /// Trait asset `{path}`.\n    pub mod {module_name} {{\n"
        ));
        for line in generated.lines() {
            module.push_str("        ");
            module.push_str(line);
            module.push('\n');
        }
        module.push_str("    }\n\n");
    }
    module.push_str("}\n");

    syn::parse_file(&module)
        .map_err(|error| format!("Generated trait contract module is invalid Rust: {error}"))?;
    Ok(module)
}

fn normalize_trait_paths(paths: &[String]) -> Result<Vec<String>, String> {
    let mut normalized = BTreeSet::new();
    for raw_path in paths {
        let path = raw_path.replace('\\', "/");
        let parsed = Path::new(&path);
        if parsed.is_absolute()
            || parsed.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
            || !path.starts_with("types/traits/")
            || !path.ends_with(".trait.json")
        {
            return Err(format!(
                "Blueprint trait reference '{raw_path}' must be a normalized project-relative path under types/traits ending in .trait.json"
            ));
        }
        let normalized_path = parsed
            .components()
            .filter_map(|component| match component {
                Component::Normal(part) => part.to_str(),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");
        if normalized_path != path {
            return Err(format!(
                "Blueprint trait reference '{raw_path}' is not canonical"
            ));
        }
        if !normalized.insert(normalized_path.clone()) {
            return Err(format!(
                "Blueprint declares trait '{normalized_path}' more than once"
            ));
        }
    }
    Ok(normalized.into_iter().collect())
}

fn load_trait(project_root: &Path, relative_path: &str) -> Result<TraitAsset, String> {
    let path = project_root.join(relative_path);
    let bytes = engine_fs::virtual_fs::read_file(&path)
        .map_err(|error| format!("Could not read assigned trait '{relative_path}': {error}"))?;
    let mut value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Trait '{relative_path}' is malformed JSON: {error}"))?;
    normalize_legacy_trait(&mut value, relative_path)?;

    let asset: TraitAsset = serde_json::from_value(value)
        .map_err(|error| format!("Trait '{relative_path}' does not match TraitAsset: {error}"))?;
    if asset.schema_version != 1 || asset.type_kind != TypeKind::Trait {
        return Err(format!(
            "Trait '{relative_path}' has unsupported schemaVersion/typeKind ({}/{})",
            asset.schema_version,
            asset.type_kind.as_str()
        ));
    }
    Ok(asset)
}

fn normalize_legacy_trait(value: &mut serde_json::Value, path: &str) -> Result<(), String> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| format!("Trait '{path}' must contain a JSON object"))?;
    let has_version = object.contains_key("schemaVersion");
    let has_kind = object.contains_key("typeKind");
    if has_version || has_kind {
        if !has_version || !has_kind {
            return Err(format!(
                "Trait '{path}' has an incomplete canonical schema discriminator"
            ));
        }
        return Ok(());
    }

    // Accept the original engine_fs trait template, which predates the
    // canonical ui_types_common envelope and is also upgraded by Trait Editor.
    if !object.contains_key("display_name") {
        return Err(format!(
            "Trait '{path}' is neither a canonical TraitAsset nor a supported legacy trait"
        ));
    }
    object.insert("schemaVersion".into(), serde_json::json!(1));
    object.insert("typeKind".into(), serde_json::json!("trait"));
    if !object.contains_key("displayName") {
        let display_name = object
            .remove("display_name")
            .or_else(|| object.get("name").cloned())
            .ok_or_else(|| format!("Trait '{path}' is missing display_name"))?;
        object.insert("displayName".into(), display_name);
    }
    object.remove("visibility");
    object
        .entry("meta")
        .or_insert_with(|| serde_json::json!({}));
    Ok(())
}

fn load_type_index(project_root: &Path) -> Result<TypeIndex, String> {
    let path = project_root.join(TYPE_INDEX_PATH);
    match engine_fs::virtual_fs::exists(&path) {
        Ok(false) => Ok(TypeIndex::default()),
        Ok(true) => {
            let bytes = engine_fs::virtual_fs::read_file(&path).map_err(|error| {
                format!("Could not read type index '{}': {error}", path.display())
            })?;
            serde_json::from_slice(&bytes)
                .map_err(|error| format!("Type index '{}' is malformed: {error}", path.display()))
        }
        Err(error) => Err(format!(
            "Could not inspect type index '{}': {error}",
            path.display()
        )),
    }
}

fn validate_trait_asset(asset: &TraitAsset, index: &TypeIndex, path: &str) -> Result<(), String> {
    ui_types_common::validate_trait(asset, index)
        .map_err(|error| format!("Trait '{path}' is invalid: {error}"))?;

    let mut methods = BTreeSet::new();
    for method in &asset.methods {
        if !methods.insert(method.name.as_str()) {
            return Err(format!(
                "Trait '{path}' declares method '{}' more than once",
                method.name
            ));
        }
        let mut parameters = BTreeSet::new();
        for parameter in &method.signature.params {
            if !parameters.insert(parameter.name.as_str()) {
                return Err(format!(
                    "Trait '{path}' method '{}' declares parameter '{}' more than once",
                    method.name, parameter.name
                ));
            }
            if parameter.name == "self" {
                return Err(format!(
                    "Trait '{path}' method '{}' uses a `self` receiver, which this Blueprint trait contract format does not define",
                    method.name
                ));
            }
        }
    }
    Ok(())
}

fn reject_unresolvable_aliases(asset: &TraitAsset, path: &str) -> Result<(), String> {
    for method in &asset.methods {
        for parameter in &method.signature.params {
            if let ui_types_common::TypeRef::AliasRef { alias } = &parameter.type_ref {
                return Err(format!(
                    "Trait '{path}' uses alias type '{alias}' in method '{}'; generated Blueprint class modules do not yet expose project alias modules. Use a fully qualified Path type for Rust export.",
                    method.name
                ));
            }
        }
        if let ui_types_common::TypeRef::AliasRef { alias } = &method.signature.return_type {
            return Err(format!(
                "Trait '{path}' uses alias return type '{alias}' in method '{}'; generated Blueprint class modules do not yet expose project alias modules. Use a fully qualified Path type for Rust export.",
                method.name
            ));
        }
    }
    Ok(())
}

fn trait_module_name(asset: &TraitAsset, path: &str) -> String {
    let mut slug = String::new();
    for character in asset.name.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('_') {
            slug.push('_');
        }
    }
    let slug = slug.trim_matches('_');
    let slug = if slug.is_empty() { "trait" } else { slug };
    format!("trait_{slug}_{:08x}", stable_hash(path) as u32)
}

fn stable_hash(value: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
