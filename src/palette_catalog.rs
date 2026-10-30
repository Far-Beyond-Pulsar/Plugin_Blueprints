//! The picker entries that depend on the open asset: local macros, custom
//! event dispatch, and the engine and component events. The base node
//! catalogue is the profile's; these are published into its `ProfileData` by
//! [`BlueprintEditorPanel::refresh_catalog`] whenever they may have changed.

use std::collections::{BTreeMap, HashMap};

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
    definitions: &mut HashMap<String, NodeDefinition>,
    events: impl IntoIterator<Item = (String, NodeDefinition)>,
) {
    let mut by_category: BTreeMap<String, Vec<NodeDefinition>> = BTreeMap::new();
    for (category, def) in events {
        definitions.insert(def.id.clone(), def.clone());
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
    pub fn catalog_extras(&self, cx: &App) -> (Vec<CatalogEntry>, HashMap<String, NodeDefinition>) {
        let mut definitions = HashMap::new();
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
            &mut definitions,
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
                &mut definitions,
                engine_events::component_event_node_definitions(&catalog.events),
            );
        }
        push_event_categories(&mut out, &mut definitions, self.trait_event_definitions());
        (out, definitions)
    }

    /// The Add-menu nodes of the declared traits' methods (memory only).
    fn trait_event_definitions(&self) -> Vec<(String, NodeDefinition)> {
        crate::features::traits::events::node_definitions(
            &self.trait_event_class_name(),
            &self.trait_method_specs(),
        )
    }

    /// Republishes only the trait entries of the Add menu. Adding or removing
    /// a trait takes this path, not [`Self::refresh_catalog`], which also reads
    /// other classes' compiled modules from disk.
    pub fn refresh_trait_catalog(&self) {
        let mut data = self.profile_data.write().unwrap_or_else(|e| e.into_inner());
        publish_trait_entries(&mut data, self.trait_event_definitions());
    }

    /// Publishes [`Self::catalog_extras`] to the canvas profile. Call it when
    /// macros, events, the class or the prefab change.
    pub fn refresh_catalog(&self, cx: &App) {
        let (extras, definitions) = self.catalog_extras(cx);
        let mut data = self.profile_data.write().unwrap_or_else(|e| e.into_inner());
        data.extra_catalog = extras;
        data.extra_definitions = definitions;
        data.catalog_version += 1;
    }
}

/// Replace the trait entries of the Add menu, and the node definitions the
/// canvas creates them from, leaving every other entry alone.
///
/// A menu entry only names a node (`CatalogEntry::definition`); the canvas
/// creates the node by looking that name up again (`BlueprintProfile::create_node`).
/// The static node library has no class-specific events, so the definitions
/// are published with the entries, and both must change together.
pub(crate) fn publish_trait_entries(
    data: &mut crate::canvas_profile::ProfileData,
    definitions: Vec<(String, NodeDefinition)>,
) {
    let is_trait = |category: &str| {
        category.starts_with(crate::features::traits::events::TRAIT_EVENT_CATEGORY)
    };
    let stale: Vec<String> = data
        .extra_catalog
        .iter()
        .filter(|entry| is_trait(&entry.category))
        .map(|entry| entry.definition.clone())
        .collect();
    for id in stale {
        data.extra_definitions.remove(&id);
    }
    data.extra_catalog.retain(|entry| !is_trait(&entry.category));
    let mut entries = Vec::new();
    push_event_categories(&mut entries, &mut data.extra_definitions, definitions);
    data.extra_catalog.extend(entries);
    data.catalog_version += 1;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas_profile::{BlueprintProfile, ProfileData};
    use crate::features::traits::events::{node_definitions, TraitMethodSpec};
    use graphy::{GraphDescription, Position};
    use graphy_gpui::GraphProfile;
    use std::sync::{Arc, RwLock};

    fn spec(name: &str, params: &[(&str, &str)]) -> TraitMethodSpec {
        TraitMethodSpec {
            trait_id: "t1".into(),
            trait_name: "Mode".into(),
            name: name.into(),
            params: params.iter().map(|(n, t)| ((*n).into(), (*t).into())).collect(),
            return_type: "()".into(),
            doc: String::new(),
        }
    }

    /// The path the Add menu takes: the entry's `definition` string goes to
    /// `BlueprintProfile::create_node`, and the node is added to the graph.
    #[test]
    fn a_trait_event_picked_from_the_menu_creates_a_node_in_the_graph() {
        let data = Arc::new(RwLock::new(ProfileData::default()));
        let specs = [spec("on_game_start", &[]), spec("score", &[("who", "u32")])];
        publish_trait_entries(
            &mut data.write().unwrap(),
            node_definitions("Player", &specs),
        );
        let profile = BlueprintProfile::blueprint(data.clone());

        let entry = profile
            .catalog()
            .into_iter()
            .find(|entry| entry.definition == "event::on::Player.score")
            .expect("the menu offers the trait method");
        assert_eq!(entry.category, "Events/Trait/Mode");

        let node = profile
            .create_node(&entry.definition, "n1", Position::new(10.0, 20.0))
            .expect("the canvas can create what the menu offers");
        assert_eq!(node.node_type, "event::on::Player.score");
        let outputs: Vec<_> = node.outputs.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(outputs, ["Body", "who"]);
        assert!(node.inputs.is_empty(), "an entry point has no inputs");
        assert!(node.outputs[0].data_type().is_execution());
        assert_eq!(node.outputs[1].data_type().type_string(), Some("i64"));

        let mut graph = GraphDescription::new("g");
        graph.add_node(node);
        assert!(graph.nodes.contains_key("n1"));
    }

    #[test]
    fn removing_the_trait_removes_the_entries_and_their_definitions() {
        let data = Arc::new(RwLock::new(ProfileData::default()));
        publish_trait_entries(
            &mut data.write().unwrap(),
            node_definitions("Player", &[spec("on_game_start", &[])]),
        );
        let profile = BlueprintProfile::blueprint(data.clone());
        assert!(profile.create_node("event::on::Player.on_game_start", "a", Position::zero()).is_some());

        publish_trait_entries(&mut data.write().unwrap(), Vec::new());
        assert!(profile.catalog().iter().all(|e| e.definition != "event::on::Player.on_game_start"));
        assert!(profile.create_node("event::on::Player.on_game_start", "b", Position::zero()).is_none());
    }
}
