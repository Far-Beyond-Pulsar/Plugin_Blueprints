//! Prefab feature - sidecar prefab authoring integrated into the blueprint editor.

pub mod add_component_dialog;
pub mod hierarchy_item;
pub mod panel;

// Re-export commonly used types
pub use hierarchy_item::{ComponentDrag, ComponentHierarchyItem};

use crate::editor::panel::BlueprintEditorPanel;
use engine_backend::scene::metadata::ComponentInstance;
use gpui::{AppContext, Context, Entity, Hsla, Window};
use pulsar_reflection::{PropertyType, PropertyValue, REGISTRY};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use ui::color_picker::{ColorPickerEvent, ColorPickerState};
use ui::input::{InputEvent, InputState};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct PrefabAsset {
    pub prefab_version: u32,
    pub name: String,
    #[serde(default)]
    pub components: Vec<ComponentInstance>,
    #[serde(default)]
    pub blueprint_class: Option<BlueprintClassRef>,
    #[serde(default)]
    pub script_graph: Option<ui::graph::GraphDescription>,
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
}

impl BlueprintEditorPanel {
    pub fn prefab_file_path(&self) -> Option<PathBuf> {
        self.current_class_path.as_ref().map(|p| p.join("prefab.json"))
    }

    pub fn load_prefab_sidecar(&mut self) -> Result<(), String> {
        let Some(path) = self.prefab_file_path() else {
            return Ok(());
        };

        if !path.exists() {
            return Ok(());
        }

        let prefab = crate::io::prefab::load_prefab(&path)?;

        self.prefab_asset = prefab;
        self.prefab_property_inputs.clear();
        self.prefab_color_pickers.clear();
        self.selected_prefab_component = None;
        Ok(())
    }

    pub fn save_prefab_sidecar(&mut self) -> Result<(), String> {
        self.sync_prefab_to_script()?;

        let Some(path) = self.prefab_file_path() else {
            return Err("No class path available for prefab save".to_string());
        };

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

        let graph_desc = self.convert_to_graph_description()?;
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
            values.insert(prop.name.to_string(), property_value_to_json(&value));
        }

        self.prefab_asset.components.push(ComponentInstance {
            class_name: class_name.to_string(),
            enabled: true,
            data: serde_json::Value::Object(values),
        });
        self.selected_prefab_component = Some(self.prefab_asset.components.len().saturating_sub(1));
        self.prefab_property_inputs.clear();
        self.prefab_color_pickers.clear();
        self.is_dirty = true;
    }

    pub fn remove_prefab_component(&mut self, index: usize) {
        if index < self.prefab_asset.components.len() {
            self.prefab_asset.components.remove(index);
            self.prefab_property_inputs.clear();
            self.prefab_color_pickers.clear();
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

    pub fn ensure_prefab_property_input(
        &mut self,
        component_index: usize,
        class_name: &str,
        prop_name: &str,
        property_type: &PropertyType,
        current_value: &serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<InputState> {
        let key = format!("{}::{}::{}", component_index, class_name, prop_name);
        if let Some(existing) = self.prefab_property_inputs.get(&key) {
            return existing.clone();
        }

        let initial = format_property_value(property_type, current_value);
        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |state: &mut InputState, cx| {
            state.set_value(initial, window, cx);
        });

        let cls = class_name.to_string();
        let prop = prop_name.to_string();
        cx.subscribe_in(
            &input,
            window,
            move |this, state: &Entity<InputState>, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change | InputEvent::Blur) {
                    let text = state.read(cx).text().to_string();
                    if let Some(parsed) = parse_property_text(&cls, &prop, &text) {
                        this.update_prefab_component_property(component_index, &prop, parsed);
                        cx.notify();
                    }
                }
            },
        )
        .detach();

        self.prefab_property_inputs.insert(key, input.clone());
        input
    }

    pub fn ensure_prefab_color_picker(
        &mut self,
        component_index: usize,
        class_name: &str,
        prop_name: &str,
        current_value: &serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<ColorPickerState> {
        let key = format!("{}::{}::{}::color", component_index, class_name, prop_name);
        if let Some(existing) = self.prefab_color_pickers.get(&key) {
            return existing.clone();
        }

        let rgba = json_to_rgba(current_value).unwrap_or([1.0, 1.0, 1.0, 1.0]);
        let picker = cx.new(|cx| {
            let mut state = ColorPickerState::new(window, cx);
            state.set_value(rgba_to_hsla(rgba), window, cx);
            state
        });

        let prop = prop_name.to_string();
        cx.subscribe_in(
            &picker,
            window,
            move |this, _state, event: &ColorPickerEvent, _window, cx| {
                if let ColorPickerEvent::Change(Some(hsla)) = event {
                    this.update_prefab_component_property(
                        component_index,
                        &prop,
                        serde_json::json!(hsla_to_rgba(*hsla)),
                    );
                    cx.notify();
                }
            },
        )
        .detach();

        self.prefab_color_pickers.insert(key, picker.clone());
        picker
    }
}

pub fn property_value_to_json(value: &PropertyValue) -> serde_json::Value {
    match value {
        PropertyValue::F32(v) => serde_json::Value::from(*v),
        PropertyValue::I32(v) => serde_json::Value::from(*v),
        PropertyValue::Bool(v) => serde_json::Value::from(*v),
        PropertyValue::String(v) => serde_json::Value::from(v.clone()),
        PropertyValue::Vec3(v) => serde_json::json!([v[0], v[1], v[2]]),
        PropertyValue::Color(v) => serde_json::json!([v[0], v[1], v[2], v[3]]),
        PropertyValue::EnumVariant(v) => serde_json::Value::from(*v as u64),
        PropertyValue::Vec(values) => serde_json::Value::Array(
            values.iter().map(property_value_to_json).collect(),
        ),
        PropertyValue::Component { class_name, .. } => {
            serde_json::json!({"class_name": class_name})
        }
    }
}

fn format_property_value(property_type: &PropertyType, value: &serde_json::Value) -> String {
    match property_type {
        PropertyType::String { .. } => value.as_str().unwrap_or_default().to_string(),
        _ => value.to_string(),
    }
}

fn parse_property_text(
    class_name: &str,
    prop_name: &str,
    text: &str,
) -> Option<serde_json::Value> {
    let mut instance = REGISTRY.create_instance(class_name)?;
    let properties = instance.get_properties();
    let prop = properties
        .iter()
        .find(|property| property.name == prop_name)?;

    match &prop.property_type {
        PropertyType::F32 { .. } => text
            .trim()
            .parse::<f32>()
            .ok()
            .map(serde_json::Value::from),
        PropertyType::I32 { .. } => text
            .trim()
            .parse::<i32>()
            .ok()
            .map(serde_json::Value::from),
        PropertyType::Bool => text
            .trim()
            .parse::<bool>()
            .ok()
            .map(serde_json::Value::from),
        PropertyType::String { .. } => Some(serde_json::Value::String(text.to_string())),
        PropertyType::Vec3 | PropertyType::Color | PropertyType::Vec { .. } => {
            serde_json::from_str::<serde_json::Value>(text).ok()
        }
        PropertyType::Enum { .. } => text
            .trim()
            .parse::<u64>()
            .ok()
            .map(serde_json::Value::from),
        PropertyType::Component { class_name } => {
            Some(serde_json::json!({"class_name": class_name}))
        }
    }
}

fn json_to_rgba(value: &serde_json::Value) -> Option<[f32; 4]> {
    let arr = value.as_array()?;
    if arr.len() != 4 {
        return None;
    }
    Some([
        arr.first()?.as_f64()? as f32,
        arr.get(1)?.as_f64()? as f32,
        arr.get(2)?.as_f64()? as f32,
        arr.get(3)?.as_f64()? as f32,
    ])
}

fn rgba_to_hsla([r, g, b, a]: [f32; 4]) -> Hsla {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let s = if max == min {
        0.0
    } else if l < 0.5 {
        (max - min) / (max + min)
    } else {
        (max - min) / (2.0 - max - min)
    };
    let h = if max == min {
        0.0
    } else if max == r {
        ((g - b) / (max - min)).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / (max - min) + 2.0) / 6.0
    } else {
        ((r - g) / (max - min) + 4.0) / 6.0
    };
    Hsla { h, s, l, a }
}

fn hsla_to_rgba(Hsla { h, s, l, a }: Hsla) -> [f32; 4] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h * 6.0).rem_euclid(2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r1, g1, b1) = match (h * 6.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r1 + m, g1 + m, b1 + m, a]
}
