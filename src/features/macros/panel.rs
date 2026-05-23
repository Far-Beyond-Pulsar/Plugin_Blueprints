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
        v_flex()
            .size_full()
            .bg(cx.theme().sidebar)
            .child(
                // HEADER
                v_flex()
                    .w_full()
                    .child(
                        // Main header
                        h_flex()
                            .w_full()
                            .px_3()
                            .py_2()
                            .bg(cx.theme().secondary)
                            .border_b_2()
                            .border_color(cx.theme().border)
                            .items_center()
                            .justify_between()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        // Icon
                                        div()
                                            .flex_shrink_0()
                                            .w(px(28.0))
                                            .h(px(28.0))
                                            .rounded(px(5.0))
                                            .bg(cx.theme().accent.opacity(0.15))
                                            .border_1()
                                            .border_color(cx.theme().accent.opacity(0.3))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(div().text_base().child("📦")),
                                    )
                                    .child(
                                        v_flex()
                                            .gap_0p5()
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_bold()
                                                    .text_color(cx.theme().foreground)
                                                    .child("Local Macros"),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(format!(
                                                        "{} macro{}",
                                                        panel.local_macros.len(),
                                                        if panel.local_macros.len() == 1 {
                                                            ""
                                                        } else {
                                                            "s"
                                                        }
                                                    )),
                                            ),
                                    ),
                            )
                            .child(
                                Button::new("create-macro")
                                    .icon(IconName::Plus)
                                    .primary()
                                    .tooltip("Create New Macro")
                                    .on_click(cx.listener(|panel, _, window, cx| {
                                        panel.create_new_local_macro(window, cx);
                                    })),
                            ),
                    )
                    .child(
                        // Category bar
                        h_flex()
                            .w_full()
                            .px_4()
                            .py_2()
                            .bg(cx.theme().sidebar)
                            .border_b_1()
                            .border_color(cx.theme().border.opacity(0.3))
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(cx.theme().accent)
                                    .child("THIS BLUEPRINT"),
                            )
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .rounded(px(4.0))
                                    .bg(cx.theme().accent.opacity(0.15))
                                    .text_xs()
                                    .font_family("JetBrainsMono-Regular")
                                    .text_color(cx.theme().accent)
                                    .child(format!("{}", panel.local_macros.len())),
                            ),
                    ),
            )
            .child(
                // CONTENT AREA - local macros list (using hierarchical tree view)
                v_flex()
                    .flex_1()
                    .overflow_hidden()
                    .p_1p5()
                    .child(Self::render_macros_hierarchy(panel, cx)),
            )
    }

    fn render_macros_hierarchy(
        panel: &BlueprintEditorPanel,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
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
            layout: HierarchyLayout::Widget,

            // Header config (not used in Widget mode)
            title: None,
            header_buttons: vec![],

            // No root drop zone for macros
            root_drop_zone: None,

            // Widget config
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
                let window_handle = window.window_handle();
                cx.defer(move |cx| {
                    let macro_id = panel.update(cx, |panel, cx| {
                        panel.selected_macro = Some(selected_id);
                        cx.notify();
                        // Return macro ID for opening
                        panel.local_macros.get(selected_id).map(|m| m.id.clone())
                    });

                    // Open the macro tab
                    if let Some(macro_id) = macro_id {
                        let _ = cx.update_window(window_handle, |_root_view, window, cx| {
                            let _ = panel.update(cx, |panel, cx| {
                                panel.open_macro_tab(&macro_id, window, cx);
                            });
                        });
                    }
                });
            }),
            on_drop: Arc::new(move |payload, target_id: &usize, _modifiers: &Modifiers, _window, cx| {
                let from_index = payload.macro_index;
                let to_index = *target_id;
                let panel = panel_entity_for_drop.clone();

                if from_index != to_index {
                    cx.defer(move |cx| {
                        panel.update(cx, |panel, cx| {
                            // Reorder macros
                            if from_index < panel.local_macros.len() && to_index < panel.local_macros.len() {
                                let macro_def = panel.local_macros.remove(from_index);
                                panel.local_macros.insert(to_index, macro_def);
                                cx.notify();
                            }
                        });
                    });
                }
            }),
        };

        HierarchicalTreeView::new(config).render(cx)
    }
}
