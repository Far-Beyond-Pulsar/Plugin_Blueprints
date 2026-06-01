//! Macros panel - Rendering for the macros sidebar panel

use super::hierarchy_item::MacroHierarchyItem;
use gpui::*;
use std::sync::Arc;
use ui::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme as _, HierarchicalTreeView, HierarchyConfig, HierarchyLayout,
    IconName, StyledExt,
};

use crate::editor::panel::BlueprintEditorPanel;

pub struct MacrosRenderer;

impl MacrosRenderer {
    pub fn render(
        panel: &BlueprintEditorPanel,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        let add_button = Button::new("create-macro")
            .icon(IconName::Plus)
            .ghost()
            .compact()
            .tooltip("Create New Macro")
            .on_click(cx.listener(|panel, _, window, cx| {
                panel.create_new_local_macro(window, cx);
            }))
            .into_any_element();

        Self::render_macros_hierarchy(panel, add_button, cx)
    }

    fn render_macros_hierarchy(
        panel: &BlueprintEditorPanel,
        add_button: AnyElement,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        // Convert macros to hierarchy items
        let items: Vec<MacroHierarchyItem> = panel
            .local_macros
            .iter()
            .enumerate()
            .map(|(index, subgraph)| MacroHierarchyItem {
                subgraph: subgraph.clone(),
                index,
                is_selected: panel.selected_macro == Some(index),
            })
            .collect();

        let root_ids: Vec<usize> = (0..items.len()).collect();

        // Get entity for proper GPUI pattern
        let panel_entity = cx.entity().clone();
        let panel_entity_for_drop = panel_entity.clone();

        let config = HierarchyConfig {
            items,
            root_ids,
            layout: HierarchyLayout::Panel,

            // Panel header
            title: Some("Local Macros".to_string()),
            header_buttons: vec![add_button],

            // No root drop zone for macros
            root_drop_zone: None,

            // Widget config (not used in Panel mode)
            widget_title: None,
            widget_icon: None,
            widget_add_button: None,
            empty_message: "No local macros yet\nClick + to create one".to_string(),

            // Drag-and-drop options
            disable_nesting: true, // Macros are a flat list - no nesting

            // Callbacks - Use on_click_custom() in HierarchyItem for macro opening
            is_expanded: Arc::new(|_: &usize| false),
            on_toggle_expand: Arc::new(|_: &usize, _window, _cx| {}),
            on_select: Arc::new(move |id: &usize, window, cx| {
                let selected_id = *id;
                let panel = panel_entity.clone();
                // Capture the window handle before deferring so we can pass &mut Window
                // to open_local_macro inside the deferred callback.
                let window_handle = window.window_handle();
                // Defer so the entity borrow from cx.listener is released first.
                cx.defer(move |cx| {
                    let _ = cx.update_window(window_handle, |_, window, cx| {
                        panel.update(cx, |panel, cx| {
                            panel.selected_macro = Some(selected_id);
                            cx.notify();

                            if let Some(macro_def) = panel.local_macros.get(selected_id) {
                                let macro_id = macro_def.id.clone();
                                let macro_name = macro_def.name.clone();
                                panel.open_local_macro(macro_id, macro_name, window, cx);
                            }
                        });
                    });
                });
            }),
            on_drop: Arc::new(
                move |payload, target_id: &usize, _modifiers: &Modifiers, _window, cx| {
                    let from_index = payload.macro_index;
                    let to_index = *target_id;
                    let panel = panel_entity_for_drop.clone();

                    if from_index != to_index {
                        cx.defer(move |cx| {
                            panel.update(cx, |panel, cx| {
                                // Reorder macros
                                if from_index < panel.local_macros.len()
                                    && to_index < panel.local_macros.len()
                                {
                                    let macro_def = panel.local_macros.remove(from_index);
                                    panel.local_macros.insert(to_index, macro_def);
                                    cx.notify();
                                }
                            });
                        });
                    }
                },
            ),
        };

        HierarchicalTreeView::new(config).render(cx)
    }
}
