//! Prefab feature - sidecar prefab authoring integrated into the blueprint editor.

pub mod add_component_dialog;
pub mod hierarchy_item;
pub mod panel;

// Re-export commonly used types
pub use hierarchy_item::{ComponentDrag, ComponentHierarchyItem};

use crate::core::types::{BlueprintNode, NodeType, Pin, PinType};
use crate::editor::panel::BlueprintEditorPanel;
use crate::editor::workspace_panels::GraphCanvasPanel;
use engine_backend::scene::ComponentInstance;
use gpui::{AppContext, Context, Entity, Window};
use pulsar_reflection::{REGISTRY, RUNTIME_TYPE_REGISTRY};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use ui::PixelsExt;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct PrefabAsset {
    pub prefab_version: u32,
    pub name: String,
    #[serde(default)]
    pub components: Vec<PrefabComponent>,
    #[serde(default)]
    pub blueprint_class: Option<BlueprintClassRef>,
    #[serde(default)]
    pub script_graph: Option<ui::graph::GraphDescription>,
}

/// One prefab component: the component record plus its **slot id**.
///
/// The slot id is a UUID, unique within this class. The class's compiled
/// script names the component by it (a `get_component_ref` node carries it),
/// and levels key per-instance overrides by it. It exists only on disk:
/// placing the class resolves each slot id once into a handle to the placed
/// instance's real component. It must never change once assigned; files
/// without one (or with the readable `<Class>_<n>` ids of early builds) get a
/// fresh UUID on load, saved right away (`pulsar_class` does the same).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrefabComponent {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub slot_id: String,
    #[serde(flatten)]
    pub component: ComponentInstance,
}

impl std::ops::Deref for PrefabComponent {
    type Target = ComponentInstance;
    fn deref(&self) -> &ComponentInstance {
        &self.component
    }
}

impl std::ops::DerefMut for PrefabComponent {
    fn deref_mut(&mut self) -> &mut ComponentInstance {
        &mut self.component
    }
}

/// Whether `slot_id` is a valid slot id (a UUID).
pub fn is_slot_uuid(slot_id: &str) -> bool {
    uuid::Uuid::parse_str(slot_id.trim()).is_ok()
}

/// Component classes the prefab editor never offers: `ClassInstance` marks a
/// placed class in a level and only comes from placing one.
pub fn is_internal_component(class_name: &str) -> bool {
    class_name == "ClassInstance"
}

/// Tell the editor and a running game that the class in `class_dir` was
/// rewritten (#921): the level editor rebuilds its placed instances, a
/// running game reloads its script module.
pub fn publish_class_updated(class_dir: &std::path::Path) {
    let id = std::fs::read_to_string(class_dir.join(CLASS_META_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|meta| meta.get("class_id").and_then(|v| v.as_str()).map(str::to_string));
    let mut event = plugin_editor_api::AssetUpdated::new(plugin_editor_api::AssetKind::Blueprint)
        .with_path(class_dir.to_path_buf());
    event.id = id;
    plugin_editor_api::publish_asset_updated(event);
}

/// Class metadata file holding the class GUID (`pulsar_class::ClassMeta`).
pub const CLASS_META_FILE: &str = "class.json";

/// Make sure `class_dir/class.json` carries a class GUID, generating one on
/// the class's first save. Other keys in the file are kept.
pub fn ensure_class_id(class_dir: &std::path::Path) -> Result<String, String> {
    let path = class_dir.join(CLASS_META_FILE);
    let mut meta: serde_json::Map<String, serde_json::Value> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    if let Some(id) = meta
        .get("class_id")
        .and_then(|v| v.as_str())
        .filter(|id| !id.trim().is_empty())
    {
        return Ok(id.to_string());
    }
    let id = uuid::Uuid::new_v4().to_string();
    meta.insert("class_id".into(), serde_json::Value::String(id.clone()));
    let text = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("Failed to write class.json: {e}"))?;
    Ok(id)
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct BlueprintClassRef {
    pub class_path: String,
    #[serde(default)]
    pub variable_defaults: HashMap<String, serde_json::Value>,
}

impl PrefabAsset {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            prefab_version: 1,
            name: name.into(),
            components: Vec::new(),
            blueprint_class: None,
            script_graph: None,
        }
    }

    /// Give every component whose slot id is missing, duplicated or not a
    /// UUID a fresh UUID. Returns whether anything changed.
    pub fn fill_missing_slot_ids(&mut self) -> bool {
        let mut used = std::collections::HashSet::new();
        let mut changed = false;
        for component in &mut self.components {
            let id = component.slot_id.trim().to_string();
            if !is_slot_uuid(&id) || !used.insert(id) {
                let fresh = uuid::Uuid::new_v4().to_string();
                used.insert(fresh.clone());
                component.slot_id = fresh;
                changed = true;
            }
        }
        changed
    }

    /// A fresh slot id for a new component.
    pub fn new_slot_id(&self) -> String {
        uuid::Uuid::new_v4().to_string()
    }
}

impl GraphCanvasPanel {
    /// Create a getter node that outputs a runtime reference to a prefab component instance.
    ///
    /// The node carries the component's prefab `slot_id`: the compiler
    /// resolves it to the entity holding that slot on the placed instance
    /// (the root or a generated child), so a second component of the same
    /// class is reachable too.
    pub fn create_component_getter_node(
        &mut self,
        component_index: usize,
        slot_id: String,
        class_name: String,
        position: gpui::Point<f32>,
        cx: &mut Context<Self>,
    ) {
        let node = BlueprintNode {
            id: format!("get_component_node_{}", uuid::Uuid::new_v4()),
            definition_id: format!("get_component_ref::{}::{}", class_name, component_index),
            title: format!("Get {}", class_name),
            icon: "📦".to_string(),
            node_type: NodeType::Object,
            position,
            size: gpui::Size::new(220.0, 80.0),
            inputs: vec![],
            outputs: vec![Pin {
                id: "component".to_string(),
                name: class_name.clone(),
                pin_type: PinType::Output,
                data_type: crate::core::types::PinDataType::from_type_str(&class_name),
            }],
            properties: HashMap::from([
                ("component_index".to_string(), component_index.to_string()),
                ("component_class".to_string(), class_name.clone()),
                ("slot_id".to_string(), slot_id),
            ]),
            is_selected: false,
            description: format!("Gets a runtime reference to component {}", class_name),
            color: Some("#9B59B6".to_string()),
        };

        self.add_node(node, cx);
    }

    /// Handle dropping a component from the prefab hierarchy onto the graph canvas.
    pub fn finish_dragging_component(
        &mut self,
        drag: ComponentDrag,
        window_pos: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        let origin = *self.canvas_origin.borrow();
        let canvas = gpui::Point::new(
            window_pos.x.as_f32() - origin.x,
            window_pos.y.as_f32() - origin.y,
        );
        let z = self.graph.zoom_level;
        let graph_pos = gpui::Point::new(
            canvas.x / z - self.graph.pan_offset.x,
            canvas.y / z - self.graph.pan_offset.y,
        );

        self.create_component_getter_node(
            drag.component_index,
            drag.slot_id,
            drag.class_name,
            graph_pos,
            cx,
        );
    }
}

impl BlueprintEditorPanel {
    pub fn prefab_file_path(&self) -> Option<PathBuf> {
        self.current_class_path
            .as_ref()
            .map(|p| p.join("prefab.json"))
    }

    pub fn load_prefab_sidecar(&mut self) -> Result<(), String> {
        let Some(path) = self.prefab_file_path() else {
            return Ok(());
        };

        if !path.exists() {
            return Ok(());
        }

        let mut prefab = crate::io::prefab::load_prefab(&path)?;
        if prefab.fill_missing_slot_ids() {
            // Assigned once, saved at once: levels and the compiled script
            // refer to these ids.
            if let Err(error) = crate::io::prefab::save_prefab(&path, &prefab) {
                tracing::warn!("Could not save assigned component slot ids: {error}");
            }
        }

        self.prefab_asset = prefab;
        self.prefab_property_state.clear();
        self.selected_prefab_component = None;
        Ok(())
    }

    pub fn save_prefab_sidecar(&mut self) -> Result<(), String> {
        self.sync_prefab_to_script()?;

        let Some(path) = self.prefab_file_path() else {
            return Err("No class path available for prefab save".to_string());
        };

        // Class identity and slot ids (#921): levels refer to the class by
        // its GUID and to its components by slot id.
        self.prefab_asset.fill_missing_slot_ids();
        if let Some(class_dir) = path.parent() {
            std::fs::create_dir_all(class_dir)
                .map_err(|e| format!("Failed to create class directory: {e}"))?;
            ensure_class_id(class_dir)?;
        }

        crate::io::prefab::save_prefab(&path, &self.prefab_asset)
    }

    pub fn sync_prefab_to_script(&mut self) -> Result<(), String> {
        if self.prefab_asset.prefab_version == 0 {
            self.prefab_asset.prefab_version = 1;
        }

        if self.prefab_asset.name.trim().is_empty() {
            let fallback = self
                .current_class_path
                .as_ref()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("Prefab")
                .to_string();
            self.prefab_asset.name = fallback;
        }

        if let Some(path) = self.current_class_path.as_ref() {
            let class_path = path.display().to_string();
            let mut defaults = HashMap::new();
            for var in &self.class_variables {
                if let Some(v) = &var.default_value {
                    defaults.insert(var.name.clone(), serde_json::Value::String(v.clone()));
                }
            }

            self.prefab_asset.blueprint_class = Some(BlueprintClassRef {
                class_path,
                variable_defaults: defaults,
            });
        }

        let graph = self.graph.clone();
        let graph_desc = self.convert_to_graph_description(&graph)?;
        self.prefab_asset.script_graph = Some(graph_desc);
        Ok(())
    }

    pub fn add_prefab_component(&mut self, component_type: String) {
        let class_name = component_type.trim();
        if class_name.is_empty() {
            return;
        }

        if !REGISTRY.has_class(class_name) {
            self.compilation_status.message = format!(
                "Unknown reflected class '{}' - use a class from pulsar_reflection::REGISTRY",
                class_name
            );
            return;
        }

        let Some(instance) = REGISTRY.create_instance(class_name) else {
            self.compilation_status.message =
                format!("Failed to create reflected component '{}'.", class_name);
            return;
        };

        let mut values = serde_json::Map::new();
        for prop in instance.get_properties() {
            let value = (prop.getter)(instance.as_ref());
            // Use runtime type registry for serialization
            let json_value = RUNTIME_TYPE_REGISTRY
                .serialize_json_for_any(value.as_ref())
                .unwrap_or_else(|_| serde_json::json!(null));
            values.insert(prop.name.to_string(), json_value);
        }

        let slot_id = self.prefab_asset.new_slot_id();
        self.prefab_asset.components.push(PrefabComponent {
            slot_id,
            component: ComponentInstance {
                class_name: class_name.to_string(),
                enabled: true,
                data: serde_json::Value::Object(values),
            },
        });
        self.selected_prefab_component = Some(self.prefab_asset.components.len().saturating_sub(1));
        self.prefab_property_state.clear();
        self.is_dirty = true;
    }

    pub fn remove_prefab_component(&mut self, index: usize) {
        if index < self.prefab_asset.components.len() {
            self.prefab_asset.components.remove(index);
            self.prefab_property_state.clear();
            self.selected_prefab_component = match self.selected_prefab_component {
                Some(selected) if selected == index => None,
                Some(selected) if selected > index => Some(selected - 1),
                other => other,
            };
            self.is_dirty = true;
        }
    }

    pub fn select_prefab_root(&mut self) {
        self.selected_prefab_component = None;
    }

    pub fn select_prefab_component(&mut self, index: usize) {
        if index < self.prefab_asset.components.len() {
            self.selected_prefab_component = Some(index);
        }
    }

    pub fn set_prefab_component_enabled(&mut self, index: usize, enabled: bool) {
        if let Some(component) = self.prefab_asset.components.get_mut(index) {
            component.enabled = enabled;
            self.is_dirty = true;
        }
    }

    pub fn update_prefab_component_property(
        &mut self,
        component_index: usize,
        prop_name: &str,
        new_value: serde_json::Value,
    ) {
        let Some(component) = self.prefab_asset.components.get_mut(component_index) else {
            return;
        };

        let mut map = component
            .data
            .as_object()
            .cloned()
            .unwrap_or_else(serde_json::Map::new);
        map.insert(prop_name.to_string(), new_value);
        component.data = serde_json::Value::Object(map);
        self.is_dirty = true;
    }
}





