use crate::editor::panel::BlueprintEditorPanel;
use crate::features::prefabs::hierarchy_item::ComponentHierarchyItem;
use gpui::prelude::FluentBuilder;
use gpui::*;
use pulsar_reflection::REGISTRY;
use std::any::Any;
use std::sync::Arc;
use ui::{
    button::Button,
    dropdown::SearchableList, h_flex,
    popover::Popover,
    scroll::ScrollbarAxis, v_flex, ActiveTheme, CollapsibleSection, HierarchicalTreeView,
    HierarchyConfig, HierarchyLayout, IconName, Sizable, StyledExt,
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
        let list = panel.prefab_component_list.clone();

        let add_popover = Popover::<SearchableList<&'static str>>::new("prefab-component-picker")
            .anchor(Corner::TopRight)
            .trigger(
                Button::new("prefab_component_add")
                    .label("Add Component")
                    .icon(IconName::Component)
                    .small(),
            )
            .content(move |_window, _cx| list.clone())
            .into_any_element();

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .child(Self::render_hierarchy(panel, add_popover, cx))
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

            title: None,
            header_buttons: vec![add_button],

            root_drop_zone: None,

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
                        panel.clear_sidebar_selections(false, false, false, true);
                        panel.clear_graph_selections(cx);
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

        cx.new(|cx| HierarchicalTreeView::new(config, cx)).into_any_element()
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

    /// Synthetic key passed to `PropertyStateManager`/`render_property_row_runtime`
    /// so widget state and element ids stay distinct across multiple components
    /// of the same reflected class on one prefab.
    fn state_key(component_index: usize, class_name: &str) -> String {
        format!("{}#{}", component_index, class_name)
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
        let state_key = Self::state_key(index, &class_name);
        let mut missing_in_registry = false;
        let mut row_data: Vec<(AnyElement, Option<String>, Option<String>, bool, Option<usize>)> =
            Vec::new();

        if let Some(instance) = REGISTRY.create_instance(&class_name) {
            for prop in instance.get_properties() {
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

                let current_any: Box<dyn Any> = if current_value.is_null() {
                    Box::new(())
                } else {
                    pulsar_reflection::RUNTIME_TYPE_REGISTRY
                        .deserialize_json_for_type(prop.type_info, current_value.clone())
                        .unwrap_or_else(|_| Box::new(()))
                };

                let panel_for_wb = cx.entity().clone();
                let prop_name_for_wb = prop.name.to_string();
                let write_back = Arc::new(
                    move |new_val: Box<dyn Any + Send>, _window: &mut Window, cx: &mut App| {
                        if let Ok(json) = pulsar_reflection::RUNTIME_TYPE_REGISTRY.serialize_json_for_any(new_val.as_ref()) {
                            panel_for_wb.update(cx, |panel, cx| {
                                panel.update_prefab_component_property(index, &prop_name_for_wb, json);
                                cx.notify();
                            });
                        }
                    },
                );

                let row = ui_common::render_property_row_runtime(
                    &mut panel.prefab_property_state,
                    "prefab",
                    &state_key,
                    &prop.display_name,
                    prop.name,
                    prop.type_info,
                    current_any.as_ref(),
                    write_back,
                    window,
                    cx,
                );

                row_data.push((
                    row,
                    prop.category.map(str::to_string),
                    prop.category_color.map(str::to_string),
                    prop.category_default_collapsed,
                    prop.category_order,
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
                        let (mut uncategorized, categorized) = group_rows_by_category(row_data);
                        let category_elements =
                            Self::render_categorized_rows(panel, index, categorized, cx);
                        uncategorized.extend(category_elements);

                        v_flex()
                            .gap_2()
                            .children(uncategorized)
                            .into_any_element()
                    }),
            )
            .into_any_element()
    }

    /// Render grouped, collapsible category sections for one component's rows —
    /// mirrors the level editor's `category_section` widget. Collapse state is
    /// tracked per `(component_index, category_name)` on the panel so each
    /// component instance keeps its own section open/closed state.
    fn render_categorized_rows(
        panel: &BlueprintEditorPanel,
        component_index: usize,
        mut categorized_rows: Vec<(String, Vec<AnyElement>, Option<String>, bool, usize)>,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> Vec<AnyElement> {
        categorized_rows.sort_by_key(|(_, _, _, _, order)| *order);

        categorized_rows
            .into_iter()
            .map(|(category_name, category_rows, category_color_hex, default_collapsed, _)| {
                let category_key = (component_index, category_name.clone());

                let is_collapsed = if panel.prefab_collapsed_categories.contains(&category_key) {
                    true
                } else if panel.prefab_expanded_categories.contains(&category_key) {
                    false
                } else {
                    default_collapsed
                };

                let toggle_key = category_key.clone();
                let was_collapsed = is_collapsed;
                let accent = category_color_hex
                    .as_deref()
                    .and_then(crate::features::viewport::coordinates::parse_hex_color);

                v_flex()
                    .w_full()
                    .gap_1()
                    .p_2()
                    .rounded(px(6.0))
                    .border_1()
                    .when_some(accent, |el, color| {
                        el.border_color(color.opacity(0.7)).bg(color.opacity(0.08))
                    })
                    .when(accent.is_none(), |el| {
                        el.border_color(cx.theme().border)
                            .bg(cx.theme().border.opacity(0.08))
                    })
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    if was_collapsed {
                                        this.prefab_collapsed_categories.remove(&toggle_key);
                                        this.prefab_expanded_categories
                                            .insert(toggle_key.clone());
                                    } else {
                                        this.prefab_expanded_categories.remove(&toggle_key);
                                        this.prefab_collapsed_categories
                                            .insert(toggle_key.clone());
                                    }
                                    cx.notify();
                                }),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .when_some(accent, |el, color| el.text_color(color))
                                    .when(accent.is_none(), |el| {
                                        el.text_color(cx.theme().muted_foreground)
                                    })
                                    .child(category_name),
                            )
                            .child(
                                ui::Icon::new(if is_collapsed {
                                    IconName::ChevronRight
                                } else {
                                    IconName::ChevronDown
                                })
                                .xsmall()
                                .when_some(accent, |el, color| el.text_color(color))
                                .when(accent.is_none(), |el| {
                                    el.text_color(cx.theme().muted_foreground)
                                }),
                            ),
                    )
                    .when(!is_collapsed, |el| el.children(category_rows))
                    .into_any_element()
            })
            .collect()
    }
}

/// Collects a flat list of `(row, category?, color?, default_collapsed, order)`
/// rows into uncategorised + categorised buckets — direct port of the level
/// editor's `category_section::group_rows_by_category` so both panels group
/// reflected properties identically.
pub fn group_rows_by_category(
    rows: Vec<(
        AnyElement,
        Option<String>,
        Option<String>,
        bool,
        Option<usize>,
    )>,
) -> (
    Vec<AnyElement>,
    Vec<(String, Vec<AnyElement>, Option<String>, bool, usize)>,
) {
    let mut uncategorized: Vec<AnyElement> = Vec::new();
    let mut categorized: Vec<(String, Vec<AnyElement>, Option<String>, bool, usize)> = Vec::new();
    let mut category_index: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for (row, category, category_color, default_collapsed, category_order) in rows {
        if let Some(cat) = category.filter(|c| !c.trim().is_empty()) {
            if let Some(&ix) = category_index.get(&cat) {
                let entry = &mut categorized[ix];
                entry.1.push(row);
                if entry.2.is_none() {
                    entry.2 = category_color;
                }
                entry.3 = entry.3 || default_collapsed;
                if category_order.unwrap_or(usize::MAX) < entry.4 {
                    entry.4 = category_order.unwrap_or(usize::MAX);
                }
            } else {
                let ix = categorized.len();
                category_index.insert(cat.clone(), ix);
                categorized.push((
                    cat,
                    vec![row],
                    category_color,
                    default_collapsed,
                    category_order.unwrap_or(usize::MAX),
                ));
            }
        } else {
            uncategorized.push(row);
        }
    }

    (uncategorized, categorized)
}
