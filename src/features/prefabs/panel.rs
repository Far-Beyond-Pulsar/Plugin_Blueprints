use crate::editor::panel::BlueprintEditorPanel;
use crate::features::prefabs::hierarchy_item::ComponentHierarchyItem;
use gpui::prelude::FluentBuilder;
use gpui::*;
use pulsar_reflection::{TypeStructure, REGISTRY};
use std::sync::Arc;
use ui::{
    button::Button, h_flex, scroll::ScrollbarAxis, v_flex, ActiveTheme, CollapsibleSection,
    HierarchicalTreeView, HierarchyConfig, HierarchyLayout, IconName, Sizable, StyledExt,
};
use ui_common::properties_inspector;
pub struct PrefabHierarchyRenderer;
pub struct PrefabPropertiesRenderer;

impl PrefabHierarchyRenderer {
    /// Get the parent index of a component from its data
    fn get_parent_index(component: &engine_backend::ComponentInstance) -> Option<usize> {
        component
            .data
            .get("__parent_index")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
    }

    /// Get all children indices of a component
    fn get_children(
        components: &[engine_backend::ComponentInstance],
        parent_index: usize,
    ) -> Vec<usize> {
        components
            .iter()
            .enumerate()
            .filter_map(|(idx, comp)| {
                if Self::get_parent_index(comp) == Some(parent_index) {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn render(
        panel: &mut BlueprintEditorPanel,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        let dialog = panel.prefab_add_component_dialog.clone();
        let show_dialog = panel.show_add_component_dialog;

        let add_button = Button::new("prefab_component_add")
            .label("Add Component")
            .icon(IconName::Component)
            .small()
            .on_click(cx.listener(|panel, _, _window, cx| {
                panel.show_add_component_dialog = true;
                cx.notify();
            }))
            .into_any_element();

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .child(Self::render_hierarchy(panel, add_button, cx))
            .when(show_dialog, |el| {
                el.child(
                    div()
                        .absolute()
                        .left(px(8.0))
                        .bottom(px(8.0))
                        .w(px(260.0))
                        .shadow_lg()
                        .rounded(px(6.0))
                        .overflow_hidden()
                        .border_1()
                        .border_color(cx.theme().border)
                        .on_mouse_down_out(cx.listener(|panel, _, _, cx| {
                            panel.show_add_component_dialog = false;
                            cx.notify();
                        }))
                        .child(dialog),
                )
            })
    }

    fn render_hierarchy(
        panel: &BlueprintEditorPanel,
        add_button: AnyElement,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        let components = &panel.prefab_asset.components;

        // Build hierarchy items with children calculated
        let items: Vec<ComponentHierarchyItem> = components
            .iter()
            .enumerate()
            .map(|(index, component)| {
                let children_indices = Self::get_children(components, index);
                ComponentHierarchyItem {
                    component: component.clone(),
                    index,
                    is_selected: panel.selected_prefab_component == Some(index),
                    children_indices,
                }
            })
            .collect();

        // Calculate root IDs (components without a parent)
        let root_ids: Vec<usize> = components
            .iter()
            .enumerate()
            .filter_map(|(idx, comp)| {
                if Self::get_parent_index(comp).is_none() {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect();

        // Capture panel entity for callbacks
        let panel_entity = cx.entity().clone();
        let panel_entity_for_drop = panel_entity.clone();

        let config = HierarchyConfig {
            items,
            root_ids,
            layout: HierarchyLayout::Panel,

            // Panel header
            title: Some("Components".to_string()),
            header_buttons: vec![add_button],

            // No root drop zone for now
            root_drop_zone: None,

            // Widget config (not used in Panel mode)
            widget_title: None,
            widget_icon: None,
            widget_add_button: None,
            empty_message: "No components\nClick + to add one".to_string(),

            // Drag-and-drop options
            disable_nesting: false, // Components CAN be nested

            // Callbacks
            is_expanded: Arc::new(|_: &usize| true), // All expanded for now
            on_toggle_expand: Arc::new(|_id: &usize, _window, _cx| {}), // TODO: Track expansion state
            on_select: Arc::new(move |id: &usize, _window, cx| {
                let selected_id = *id;
                let panel = panel_entity.clone();
                cx.defer(move |cx| {
                    panel.update(cx, |panel, cx| {
                        panel.selected_prefab_component = Some(selected_id);
                        cx.notify();
                    });
                });
            }),
            on_drop: Arc::new(
                move |payload, target_id: &usize, modifiers: &Modifiers, _window, cx| {
                    use crate::features::prefabs::hierarchy_item::ComponentDrag;
                    let from_index = payload.component_index;
                    let to_index = *target_id;
                    let panel = panel_entity_for_drop.clone();
                    let mods = modifiers.clone();

                    if from_index != to_index {
                        cx.defer(move |cx| {
                            panel.update(cx, |panel, cx| {
                                if from_index >= panel.prefab_asset.components.len()
                                    || to_index >= panel.prefab_asset.components.len()
                                {
                                    return;
                                }

                                if mods.shift {
                                    // Shift: Un-nest - remove parent
                                    if let Some(obj) = panel.prefab_asset.components[from_index]
                                        .data
                                        .as_object_mut()
                                    {
                                        obj.remove("__parent_index");
                                    }
                                } else if mods.alt {
                                    // Alt: Reorder as siblings
                                    let component =
                                        panel.prefab_asset.components.remove(from_index);
                                    panel.prefab_asset.components.insert(to_index, component);
                                } else {
                                    // Normal: Nest under target
                                    if let Some(obj) = panel.prefab_asset.components[from_index]
                                        .data
                                        .as_object_mut()
                                    {
                                        obj.insert(
                                            "__parent_index".to_string(),
                                            serde_json::json!(to_index),
                                        );
                                    }
                                }
                                cx.notify();
                            });
                        });
                    }
                },
            ),
        };

        HierarchicalTreeView::new(config).render(cx)
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
                            .icon(if enabled {
                                IconName::Check
                            } else {
                                IconName::Xmark
                            })
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
            .child(Self::render_header(
                panel.selected_prefab_component.is_some(),
                cx,
            ))
            .child(div().flex_1().overflow_hidden().w_full().child(
                div().size_full().scrollable(ScrollbarAxis::Vertical).child(
                    match panel.selected_prefab_component {
                        Some(index) => Self::render_component_properties(panel, index, window, cx),
                        None => Self::render_empty_state(cx).into_any_element(),
                    },
                ),
            ))
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
        let mut props_data = Vec::new();
        let mut missing_in_registry = false;

        if let Some(instance) = REGISTRY.create_instance(&class_name) {
            for prop in instance.get_properties() {
                // Get current JSON value from component data, or serialize default
                let current_value = component
                    .data
                    .as_object()
                    .and_then(|obj| obj.get(prop.name))
                    .cloned()
                    .unwrap_or_else(|| {
                        let default_value = (prop.getter)(instance.as_ref());
                        pulsar_reflection::RUNTIME_TYPE_REGISTRY
                            .serialize_json_for_any(default_value.as_ref())
                            .unwrap_or(serde_json::json!(null))
                    });

                // Determine if we need numeric input based on RuntimeTypeInfo
                let numeric_input = match &prop.type_info.structure {
                    TypeStructure::Primitive
                        if matches!(prop.type_info.base_name(), "f32" | "i32") =>
                    {
                        Some(panel.ensure_prefab_property_input(
                            index,
                            &class_name,
                            prop.name,
                            prop.type_info,
                            &current_value,
                            window,
                            cx,
                        ))
                    }
                    _ => None,
                };

                // Determine if we need color picker based on RuntimeTypeInfo
                let is_color_type = matches!(
                    &prop.type_info.structure,
                    TypeStructure::Primitive if prop.type_info.base_name() == "[f32; 4]"
                );
                let should_create_picker = is_color_type || Self::is_color_field_name(prop.name);

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

                let mut widgets: std::collections::HashMap<
                    std::any::TypeId,
                    std::sync::Arc<dyn std::any::Any + Send + Sync>,
                > = std::collections::HashMap::new();
                if let Some(input) = numeric_input {
                    widgets.insert(
                        std::any::TypeId::of::<gpui::Entity<ui::input::InputState>>(),
                        std::sync::Arc::new(input),
                    );
                }
                if let Some(picker) = color_picker {
                    widgets.insert(
                        std::any::TypeId::of::<gpui::Entity<ui::color_picker::ColorPickerState>>(),
                        std::sync::Arc::new(picker),
                    );
                }
                props_data.push((
                    prop.display_name.to_string(),
                    prop.name.to_string(),
                    prop.type_info,
                    current_value,
                    widgets,
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
                        // Use shared component section renderer - capture panel entity first
                        let panel_entity = cx.entity().clone();

                        let on_bool_toggle = Arc::new(
                            move |prop_name: &str,
                                  checked: bool,
                                  _window: &mut Window,
                                  cx: &mut App| {
                                panel_entity.update(cx, |panel, cx| {
                                    panel.update_prefab_component_property(
                                        index,
                                        prop_name,
                                        serde_json::Value::Bool(checked),
                                    );
                                    cx.notify();
                                });
                            },
                        );

                        let panel_entity = cx.entity().clone();
                        let on_enum_select = Arc::new(
                            move |prop_name: &str,
                                  ix: usize,
                                  _window: &mut Window,
                                  cx: &mut App| {
                                panel_entity.update(cx, |panel, cx| {
                                    panel.update_prefab_component_property(
                                        index,
                                        prop_name,
                                        serde_json::Value::from(ix as u64),
                                    );
                                    cx.notify();
                                });
                            },
                        );

                        v_flex()
                            .gap_2()
                            .children(props_data.into_iter().map(
                                |(display_name, prop_name, type_info, json_value, widgets)| {
                                    let prop_bool = prop_name.clone();
                                    let on_bool_toggle_local = on_bool_toggle.clone();
                                    let bool_callback = Arc::new(
                                        move |checked: bool, window: &mut Window, cx: &mut App| {
                                            (on_bool_toggle_local)(&prop_bool, checked, window, cx);
                                        },
                                    );

                                    let prop_enum = prop_name.clone();
                                    let on_enum_select_local = on_enum_select.clone();
                                    let enum_callback = Arc::new(
                                        move |ix: usize, window: &mut Window, cx: &mut App| {
                                            (on_enum_select_local)(&prop_enum, ix, window, cx);
                                        },
                                    );

                                    ui_common::render_property_row_runtime(
                                        "prefab",
                                        &class_name,
                                        &display_name,
                                        &prop_name,
                                        type_info,
                                        &json_value,
                                        widgets,
                                        bool_callback,
                                        enum_callback,
                                        cx,
                                    )
                                },
                            ))
                            .into_any_element()
                    }),
            )
            .into_any_element()
    }
}

impl PrefabPropertiesRenderer {
    fn is_color_field_name(prop_name: &str) -> bool {
        prop_name == "color" || prop_name == "base_color"
    }
}
