use crate::editor::panel::BlueprintEditorPanel;
use crate::features::prefabs::hierarchy_item::ComponentHierarchyItem;
use gpui::*;
use std::sync::Arc;
use ui::{
    button::Button, dropdown::SearchableList, popover::Popover, v_flex, ActiveTheme,
    HierarchicalTreeView, HierarchyConfig, HierarchyLayout, IconName, Sizable,
};
pub struct PrefabHierarchyRenderer;

impl PrefabHierarchyRenderer {
    /// Get the parent index of a component from its data
    fn get_parent_index(component: &pulsar_class::prefab::PrefabComponent) -> Option<usize> {
        component
            .data
            .get("__parent_index")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
    }

    /// Get all children indices of a component
    fn get_children(
        components: &[pulsar_class::prefab::PrefabComponent],
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

        let content_key = config.content_key();
        HierarchicalTreeView::new(config)
            .with_list_state(panel.prefab_components_list.clone(), content_key)
            .render(cx)
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
