//! Variable list rendering
use super::hierarchy_item::VariableHierarchyItem;
use super::types::ClassVariable;
use crate::editor::panel::BlueprintEditorPanel;
use gpui::*;
use std::sync::Arc;
use ui::{
    button::{Button, ButtonVariants as _},
    dropdown::Dropdown,
    h_flex, v_flex, ActiveTheme, Colorize, HierarchicalTreeView, HierarchyConfig, HierarchyLayout,
    IconName, PixelsExt, Sizable, StyledExt,
};

pub struct VariablesRenderer;

impl VariablesRenderer {
    pub fn render(
        panel: &BlueprintEditorPanel,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        let add_button = Button::new("add-variable")
            .icon(IconName::Plus)
            .ghost()
            .compact()
            .tooltip("Add Variable (Ctrl+Shift+V)")
            .on_click(cx.listener(|panel, _, window, cx| {
                panel.start_creating_variable(window, cx);
            }))
            .into_any_element();

        v_flex()
            .size_full()
            .gap_1p5()
            .children(if panel.is_creating_variable {
                vec![Self::render_variable_creation_form(panel, cx).into_any_element()]
            } else {
                Vec::new()
            })
            .child(Self::render_variables_hierarchy(panel, add_button, cx))
    }

    fn render_variables_hierarchy(
        panel: &BlueprintEditorPanel,
        add_button: AnyElement,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        // Convert variables to hierarchy items
        let panel_weak_for_items = cx.entity().downgrade();
        let items: Vec<VariableHierarchyItem> = panel
            .class_variables
            .iter()
            .enumerate()
            .map(|(index, variable)| VariableHierarchyItem {
                variable: variable.clone(),
                index,
                is_selected: panel.selected_variable == Some(index),
                panel: panel_weak_for_items.clone(),
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
            title: Some("Variables".to_string()),
            header_buttons: vec![add_button],

            // No root drop zone for variables
            root_drop_zone: None,

            // Widget config (not used in Panel mode)
            widget_title: None,
            widget_icon: None,
            widget_add_button: None,
            empty_message: "No variables yet\nClick + to create one".to_string(),

            // Drag-and-drop options
            disable_nesting: true, // Variables are a flat list - no nesting

            // Callbacks
            is_expanded: Arc::new(|_: &usize| false), // No expansion needed
            on_toggle_expand: Arc::new(|_: &usize, _window, _cx| {}), // No-op
            on_select: Arc::new(move |id: &usize, _window, cx| {
                let selected_id = *id;
                let panel = panel_entity.clone();
                // Defer so the entity borrow from cx.listener is released first.
                cx.defer(move |cx| {
                    panel.update(cx, |panel, cx| {
                        panel.selected_variable = Some(selected_id);
                        cx.notify();
                    });
                });
            }),
            on_drop: Arc::new(
                move |payload, target_id: &usize, _modifiers: &Modifiers, _window, cx| {
                    let from_index = payload.var_index;
                    let to_index = *target_id;
                    let panel = panel_entity_for_drop.clone();

                    if from_index != to_index {
                        cx.defer(move |cx| {
                            panel.update(cx, |panel, cx| {
                                // Reorder variables
                                if from_index < panel.class_variables.len()
                                    && to_index < panel.class_variables.len()
                                {
                                    let variable = panel.class_variables.remove(from_index);
                                    panel.class_variables.insert(to_index, variable);
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

    fn render_variable_creation_form(
        panel: &BlueprintEditorPanel,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        use ui::input::TextInput;

        v_flex()
            .w_full()
            .p_3()
            .gap_3()
            .bg(cx.theme().sidebar)
            .border_1()
            .border_color(cx.theme().border)
            .rounded(cx.theme().radius)
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(cx.theme().foreground)
                    .child("New Variable"),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Name"),
                            )
                            .child(TextInput::new(&panel.variable_name_input)),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Type"),
                            )
                            .child(Dropdown::new(&panel.variable_type_dropdown)),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .justify_end()
                    .child(
                        Button::new("cancel-var")
                            .ghost()
                            .label("Cancel")
                            .on_click(cx.listener(|panel, _, _, cx| {
                                panel.cancel_creating_variable(cx);
                            })),
                    )
                    .child(
                        Button::new("create-var")
                            .primary()
                            .label("Create")
                            .on_click(cx.listener(|panel, _, _, cx| {
                                panel.complete_creating_variable(cx);
                            })),
                    ),
            )
    }
}
