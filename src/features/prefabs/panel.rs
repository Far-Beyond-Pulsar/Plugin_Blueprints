use crate::editor::panel::BlueprintEditorPanel;
use crate::features::prefabs::property_value_to_json;
use gpui::*;
use pulsar_reflection::{PropertyType, PropertyValue, REGISTRY};
use std::sync::Arc;
use ui_common::properties_inspector;
use ui::popover::Popover;
use ui::{
    button::Button,
    h_flex,
    scroll::ScrollbarAxis,
    v_flex, ActiveTheme, CollapsibleSection, IconName, Sizable, StyledExt,
};

pub struct PrefabHierarchyRenderer;
pub struct PrefabPropertiesRenderer;

impl PrefabHierarchyRenderer {
    pub fn render(
        panel: &mut BlueprintEditorPanel,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        let components = panel.prefab_asset.components.clone();
        let dialog = panel.prefab_add_component_dialog.clone();
        let add_popover = Popover::new("prefab-add-component-picker")
            .anchor(Corner::TopRight)
            .trigger(
                Button::new("prefab_component_add")
                    .label("Add Component")
                    .icon(IconName::Component)
                    .small(),
            )
            .content(move |_window, _cx| dialog.clone());

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .child(Self::render_header(cx))
            .child(
                v_flex()
                    .w_full()
                    .p_2()
                    .gap_2()
                    .bg(cx.theme().sidebar)
                    .border_b_1()
                    .border_color(cx.theme().border.opacity(0.5))
                    .child(add_popover),
            )
            .child(
                div().flex_1().overflow_hidden().w_full().child(
                    div().size_full().scrollable(ScrollbarAxis::Vertical).child(
                        if components.is_empty() {
                            Self::render_empty_state(cx).into_any_element()
                        } else {
                            v_flex()
                                .w_full()
                                .p_3()
                                .gap_2()
                                .children(
                                    components.into_iter().enumerate().map(|(idx, component)| {
                                        Self::render_component_item(panel, idx, component, cx)
                                    }),
                                )
                                .into_any_element()
                        },
                    ),
                ),
            )
    }

    fn render_header(cx: &mut Context<BlueprintEditorPanel>) -> impl IntoElement {
        h_flex()
            .w_full()
            .px_4()
            .py_3()
            .items_center()
            .bg(cx.theme().sidebar)
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .text_base()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(cx.theme().foreground)
                    .child("Components"),
            )
    }

    fn render_empty_state(cx: &mut Context<BlueprintEditorPanel>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .p_8()
            .child(
                v_flex()
                    .gap_3()
                    .items_center()
                    .child(
                        ui::Icon::new(IconName::Component)
                            .size(px(48.0))
                            .text_color(cx.theme().muted_foreground.opacity(0.5)),
                    )
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(cx.theme().muted_foreground)
                            .child("No Components"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground.opacity(0.7))
                            .text_center()
                            .child("Add a reflected component to begin authoring this prefab."),
                    ),
            )
    }

    fn render_component_item(
        panel: &BlueprintEditorPanel,
        idx: usize,
        component: engine_backend::scene::metadata::ComponentInstance,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        let selected = panel.selected_prefab_component == Some(idx);
        let row_bg = if selected {
            cx.theme().accent.opacity(0.08)
        } else {
            cx.theme().sidebar
        };
        let row_border = if selected {
            cx.theme().accent.opacity(0.45)
        } else {
            cx.theme().border.opacity(0.4)
        };
        let class_name = component.class_name.clone();
        let enabled = component.enabled;

        h_flex()
            .w_full()
            .px_3()
            .py_2()
            .gap_2()
            .items_center()
            .justify_between()
            .bg(row_bg)
            .border_1()
            .border_color(row_border)
            .rounded(px(6.0))
            .cursor_pointer()
            .hover(|style| style.border_color(cx.theme().accent.opacity(0.35)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |panel, _event: &MouseDownEvent, _window, cx| {
                    panel.select_prefab_component(idx);
                    cx.notify();
                }),
            )
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        ui::Icon::new(IconName::Component)
                            .size(px(13.0))
                            .text_color(if enabled {
                                cx.theme().accent
                            } else {
                                cx.theme().muted_foreground
                            }),
                    )
                    .child(
                        v_flex()
                            .gap_0p5()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(cx.theme().foreground)
                                    .child(class_name),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(if enabled { "Enabled" } else { "Disabled" }),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Button::new(("prefab_component_toggle", idx))
                            .icon(if enabled { IconName::Check } else { IconName::Xmark })
                            .xsmall()
                            .on_click(cx.listener(move |panel, _, _, cx| {
                                panel.set_prefab_component_enabled(idx, !enabled);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new(("prefab_component_remove", idx))
                            .icon(IconName::Trash)
                            .xsmall()
                            .on_click(cx.listener(move |panel, _, _, cx| {
                                panel.remove_prefab_component(idx);
                                cx.notify();
                            })),
                    ),
            )
    }
}

impl PrefabPropertiesRenderer {
    pub fn render(
        panel: &mut BlueprintEditorPanel,
        window: &mut Window,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .child(Self::render_header(panel.selected_prefab_component.is_some(), cx))
            .child(
                div().flex_1().overflow_hidden().w_full().child(
                    div().size_full().scrollable(ScrollbarAxis::Vertical).child(
                        match panel.selected_prefab_component {
                            Some(index) => {
                                Self::render_component_properties(panel, index, window, cx)
                            }
                            None => Self::render_empty_state(cx).into_any_element(),
                        },
                    ),
                ),
            )
    }

    fn render_header(
        has_selection: bool,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        properties_inspector::render_header(
            "Component Properties",
            has_selection,
            "Selected",
            "prefab-component-props-more",
            cx,
        )
    }

    fn render_empty_state(cx: &mut Context<BlueprintEditorPanel>) -> impl IntoElement {
        properties_inspector::render_empty_state(
            IconName::CursorPointer,
            "No Component Selected",
            "Select a component in the Components panel to edit reflected properties.",
            cx,
        )
    }

    fn render_component_properties(
        panel: &mut BlueprintEditorPanel,
        index: usize,
        window: &mut Window,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
        let Some(component) = panel.prefab_asset.components.get(index).cloned() else {
            return div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("Component not found")
                .into_any_element();
        };

        let class_name = component.class_name.clone();
        let mut reflected_properties: Vec<(
            String,
            String,
            PropertyType,
            PropertyValue,
            Option<Entity<ui::input::InputState>>,
            Option<Entity<ui::color_picker::ColorPickerState>>,
        )> = Vec::new();
        let mut missing_in_registry = false;

        if let Some(instance) = REGISTRY.create_instance(&class_name) {
            for prop in instance.get_properties() {
                let default_value = (prop.getter)(instance.as_ref());
                let current_value = component
                    .data
                    .as_object()
                    .and_then(|obj| obj.get(prop.name))
                    .cloned()
                    .unwrap_or_else(|| property_value_to_json(&default_value));
                let input = match prop.property_type {
                    PropertyType::F32 { .. } | PropertyType::I32 { .. } => {
                        Some(panel.ensure_prefab_property_input(
                            index,
                            &class_name,
                            prop.name,
                            &prop.property_type,
                            &current_value,
                            window,
                            cx,
                        ))
                    }
                    PropertyType::Bool
                    | PropertyType::Enum { .. }
                    | PropertyType::Color
                    | PropertyType::String { .. }
                    | PropertyType::Vec3
                    | PropertyType::Vec { .. }
                    | PropertyType::Component { .. } => None,
                };
                let should_create_picker = matches!(prop.property_type, PropertyType::Color)
                    || (matches!(&default_value, PropertyValue::String(s) if s == "unsupported")
                        && Self::is_color_field_name(prop.name));

                let color_picker = if should_create_picker {
                    Some(panel.ensure_prefab_color_picker(
                        index,
                        &class_name,
                        prop.name,
                        &current_value,
                        window,
                        cx,
                    ))
                } else {
                    None
                };

                let current_typed =
                    json_to_property_value(&prop.property_type, &current_value).unwrap_or(default_value);
                reflected_properties.push((
                    prop.display_name.to_string(),
                    prop.name.to_string(),
                    prop.property_type.clone(),
                    current_typed,
                    input,
                    color_picker,
                ));
            }
        } else {
            missing_in_registry = true;
        }

        v_flex()
            .w_full()
            .p_3()
            .gap_4()
            .min_w_full()
            .child(
                CollapsibleSection::new(class_name.clone())
                    .icon(IconName::Settings)
                    .open(true)
                    .child(if missing_in_registry {
                        div()
                            .text_sm()
                            .text_color(cx.theme().warning)
                            .child(
                                "This component class is not available in the reflection registry.",
                            )
                            .into_any_element()
                    } else {
                        v_flex()
                            .gap_2()
                            .children(reflected_properties.into_iter().map(
                                |(display_name, prop_name, property_type, value, input, color_picker)| {
                                    Self::render_property_row(
                                        index,
                                        class_name.clone(),
                                        display_name,
                                        prop_name,
                                        property_type,
                                        value,
                                        input,
                                        color_picker,
                                        cx,
                                    )
                                },
                            ))
                            .into_any_element()
                    }),
            )
            .into_any_element()
    }

    fn render_property_row(
        component_index: usize,
        class_name: String,
        display_name: String,
        prop_name: String,
        property_type: PropertyType,
        value: PropertyValue,
        input: Option<Entity<ui::input::InputState>>,
        color_picker: Option<Entity<ui::color_picker::ColorPickerState>>,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        let view_bool = cx.entity().downgrade();
        let prop_bool = prop_name.clone();
        let on_bool_toggle = Arc::new(move |checked: bool, _window: &mut Window, cx: &mut App| {
            if let Some(entity) = view_bool.upgrade() {
                entity.update(cx, |panel, cx| {
                    panel.update_prefab_component_property(
                        component_index,
                        &prop_bool,
                        serde_json::Value::Bool(checked),
                    );
                    cx.notify();
                });
            }
        });

        let view_enum = cx.entity().downgrade();
        let prop_enum = prop_name.clone();
        let on_enum_select = Arc::new(move |ix: usize, _window: &mut Window, cx: &mut App| {
            if let Some(entity) = view_enum.upgrade() {
                entity.update(cx, |panel, cx| {
                    panel.update_prefab_component_property(
                        component_index,
                        &prop_enum,
                        serde_json::Value::from(ix as u64),
                    );
                    cx.notify();
                });
            }
        });

        properties_inspector::render_reflected_property_row(
            "prefab",
            &class_name,
            &display_name,
            &prop_name,
            &property_type,
            &value,
            input,
            color_picker,
            on_bool_toggle,
            on_enum_select,
            cx,
        )
    }
}

impl PrefabPropertiesRenderer {
    fn is_color_field_name(prop_name: &str) -> bool {
        prop_name == "color" || prop_name == "base_color"
    }
}

fn json_to_property_value(property_type: &PropertyType, json: &serde_json::Value) -> Option<PropertyValue> {
    match property_type {
        PropertyType::F32 { .. } => json.as_f64().map(|v| PropertyValue::F32(v as f32)),
        PropertyType::I32 { .. } => json.as_i64().map(|v| PropertyValue::I32(v as i32)),
        PropertyType::Bool => json.as_bool().map(PropertyValue::Bool),
        PropertyType::String { .. } => json.as_str().map(|s| PropertyValue::String(s.to_string())),
        PropertyType::Vec3 => {
            let arr = json.as_array()?;
            if arr.len() != 3 {
                return None;
            }
            Some(PropertyValue::Vec3([
                arr.first()?.as_f64()? as f32,
                arr.get(1)?.as_f64()? as f32,
                arr.get(2)?.as_f64()? as f32,
            ]))
        }
        PropertyType::Color => {
            let arr = json.as_array()?;
            if arr.len() != 4 {
                return None;
            }
            Some(PropertyValue::Color([
                arr.first()?.as_f64()? as f32,
                arr.get(1)?.as_f64()? as f32,
                arr.get(2)?.as_f64()? as f32,
                arr.get(3)?.as_f64()? as f32,
            ]))
        }
        PropertyType::Enum { .. } => json.as_u64().map(|v| PropertyValue::EnumVariant(v as usize)),
        PropertyType::Vec { .. } => None,
        PropertyType::Component { class_name } => Some(PropertyValue::Component {
            class_name: class_name.to_string(),
        }),
    }
}
