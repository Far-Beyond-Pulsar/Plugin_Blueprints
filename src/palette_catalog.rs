//! The picker entries that depend on the open asset: local macros, custom
//! event dispatch, and the engine and component events. The base node
//! catalogue is the profile's; these are published into its `ProfileData` by
//! [`BlueprintEditorPanel::refresh_catalog`] whenever they may have changed.

use std::collections::BTreeMap;

use gpui::App;
use graphy::DataType;
use graphy_gpui::CatalogEntry;
use graphy_render::geom::parse_hex;

use crate::canvas_profile::datatype_from_str;
use crate::core::definitions::NodeDefinition;
use crate::core::subgraph_ref::SubGraphReference;
use crate::editor::panel::BlueprintEditorPanel;
use crate::features::events::engine_events;

const MACRO_COLOR: &str = "#9B59B6";
const CUSTOM_EVENT_COLOR: &str = "#E67E22";
const ENGINE_EVENT_COLOR: &str = "#C0392B";

fn entry_from_definition(def: &NodeDefinition, category: &str, color: &str) -> CatalogEntry {
    CatalogEntry {
        definition: def.id.clone(),
        title: def.name.clone(),
        category: category.to_string(),
        description: def.description.clone(),
        icon: def.icon.clone(),
        color: parse_hex(color),
        inputs: Some(
            def.inputs
                .iter()
                .map(|p| datatype_from_str(&p.data_type.to_string()))
                .collect(),
        ),
        ..Default::default()
    }
}

/// Events grouped by category, as `(category, definitions)`.
fn push_event_categories(
    out: &mut Vec<CatalogEntry>,
    events: impl IntoIterator<Item = (String, NodeDefinition)>,
) {
    let mut by_category: BTreeMap<String, Vec<NodeDefinition>> = BTreeMap::new();
    for (category, def) in events {
        by_category.entry(category).or_default().push(def);
    }
    for (category, defs) in by_category {
        out.extend(
            defs.iter()
                .map(|def| entry_from_definition(def, &category, ENGINE_EVENT_COLOR)),
        );
    }
}

impl BlueprintEditorPanel {
    /// The picker entries that depend on this asset.
    pub fn catalog_extras(&self, cx: &App) -> Vec<CatalogEntry> {
        let mut out = Vec::new();

        for m in self
            .subgraphs
            .iter()
            .filter(|m| m.kind == graphy_blueprint::SubGraphKind::Macro)
        {
            out.push(CatalogEntry {
                definition: SubGraphReference::new(
                    m.id.clone(),
                    graphy_blueprint::SubGraphKind::Macro,
                )
                .encode(),
                title: m.name.clone(),
                category: "Local Macros".into(),
                description: m.description.clone(),
                icon: "📦".into(),
                color: parse_hex(MACRO_COLOR),
                inputs: Some(
                    m.interface
                        .inputs
                        .iter()
                        .map(|p| datatype_from_str(&p.data_type.to_string()))
                        .collect(),
                ),
                ..Default::default()
            });
        }

        for def in &self.local_event_defs {
            let mut inputs = vec![DataType::Exec];
            inputs.extend(def.fields.iter().map(|f| datatype_from_str(&f.type_name)));
            out.push(CatalogEntry {
                definition: format!("custom_event_dispatch:{}", def.uid),
                title: format!("Dispatch {}", def.name),
                category: "Custom Events".into(),
                description: format!("Dispatches the '{}' custom event", def.name),
                icon: "📡".into(),
                color: parse_hex(CUSTOM_EVENT_COLOR),
                inputs: Some(inputs),
                ..Default::default()
            });
        }

        // Engine events (#924): On / Send / Broadcast nodes for the built-in
        // events, other classes' events and this class's own, by category.
        let class_path = self.current_class_path.clone();
        let class_name = class_path
            .as_deref()
            .and_then(crate::features::class_dirs::class_name_of)
            .unwrap_or_else(|| "unnamed_blueprint".to_owned());
        let animation_assets =
            engine_events::animation_asset_ids_from_prefab(&self.prefab_asset);
        push_event_categories(
            &mut out,
            engine_events::event_node_definitions(
                class_path.as_deref(),
                &class_name,
                &self.local_event_defs,
                &animation_assets,
            ),
        );

        // Use the host's snapshot rather than this editor DLL's local inventory,
        // which can differ from the registrations in the host. The typed
        // EventDecl preserves structured payload pins hidden by the opaque byte
        // descriptor.
        if let Some(catalog) = cx.try_global::<plugin_editor_api::ComponentEventCatalog>() {
            push_event_categories(
                &mut out,
                engine_events::component_event_node_definitions(&catalog.events),
            );
        }
        out
    }

    /// Publishes [`Self::catalog_extras`] to the canvas profile. Call it when
    /// macros, events, the class or the prefab change.
    pub fn refresh_catalog(&self, cx: &App) {
        let extras = self.catalog_extras(cx);
        let mut data = self.profile_data.write().unwrap_or_else(|e| e.into_inner());
        data.extra_catalog = extras;
        data.catalog_version += 1;
    }
}
