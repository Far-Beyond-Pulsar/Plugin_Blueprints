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
        v_flex()
            .size_full()
            .bg(cx.theme().sidebar)
            .child(
                // STUDIO-QUALITY HEADER
                v_flex()
                    .w_full()
                    .child(
                        // Main header with gradient background
                        h_flex()
                            .w_full()
                            .px_2()
                            .py_1p5()
                            .bg(cx.theme().secondary)
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .items_center()
                            .justify_between()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        ui::Icon::new(IconName::Code)
                                            .size(px(16.0))
                                            .text_color(cx.theme().accent),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(cx.theme().foreground)
                                            .child("My Blueprint"),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .gap_1()
                                    .items_center()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(format!("{}", panel.class_variables.len())),
                                    )
                                    .child(
                                        Button::new("add-variable")
                                            .icon(IconName::Plus)
                                            .ghost()
                                            .compact()
                                            .tooltip("Add Variable (Ctrl+Shift+V)")
                                            .on_click(cx.listener(|panel, _, window, cx| {
                                                panel.start_creating_variable(window, cx);
                                            })),
                                    ),
                            ),
                    )
                    .child(
                        // Compact category bar with search
                        h_flex()
                            .w_full()
                            .px_2()
                            .py_1()
                            .bg(cx.theme().sidebar.darken(0.02))
                            .border_b_1()
                            .border_color(cx.theme().border.opacity(0.2))
                            .items_center()
                            .gap_1p5()
                            .child(
                                ui::Icon::new(IconName::Code)
                                    .size(px(12.0))
                                    .text_color(cx.theme().accent.opacity(0.8)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Variables"),
                            )
                            .child(
                                Button::new("functions-section")
                                    .icon(IconName::Code)
                                    .ghost()
                                    .compact()
                                    .tooltip("Functions")
                                    .on_click(cx.listener(|panel, _, _window, cx| {
                                        panel.left_top_tab = 1;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("macros-section")
                                    .icon(IconName::Component)
                                    .ghost()
                                    .compact()
                                    .tooltip("Macros")
                                    .on_click(cx.listener(|panel, _, _window, cx| {
                                        panel.left_top_tab = 2;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(
                // CONTENT AREA - clean scrollable list
                v_flex()
                    .flex_1()
                    .overflow_hidden()
                    .p_1p5()
                    .scrollable(Axis::Vertical)
                    .child(Self::render_variables_list(panel, cx)),
            )
    }

    fn render_variables_list(
        panel: &BlueprintEditorPanel,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        v_flex()
            .gap_1()
            .children(if panel.is_creating_variable {
                vec![Self::render_variable_creation_form(panel, cx).into_any_element()]
            } else {
                Vec::new()
            })
            .child(Self::render_variables_hierarchy(panel, cx))
    }

    fn render_variables_hierarchy(
        panel: &BlueprintEditorPanel,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
        // Convert variables to hierarchy items
        let items: Vec<VariableHierarchyItem> = panel
            .class_variables
            .iter()
            .enumerate()
            .map(|(index, variable)| VariableHierarchyItem {
                variable: variable.clone(),
                index,
                is_selected: false, // TODO: Track selection state in panel
            })
            .collect();

        let root_ids: Vec<usize> = (0..items.len()).collect();

        // Get entity for proper GPUI pattern
        let panel_entity = cx.entity().clone();

        let config = HierarchyConfig {
            items,
            root_ids,
            layout: HierarchyLayout::Widget,

            // Header config (not used in Widget mode)
            title: None,
            header_buttons: vec![],

            // No root drop zone for variables
            root_drop_zone: None,

            // Widget config
            widget_title: None,
            widget_icon: None,
            widget_add_button: None,
            empty_message: "No variables defined".to_string(),

            // Drag-and-drop options
            disable_nesting: true, // Variables are a flat list - no nesting

            // Callbacks
            is_expanded: Arc::new(|_: &usize| false), // No expansion needed
            on_toggle_expand: Arc::new(|_: &usize| {}), // No-op
            on_select: Arc::new(|_id: &usize| {
                // Single-click selection - no-op for now
                // TODO: Track selection state in panel
            }),
            on_drop: Arc::new(move |_payload, _target_id: &usize, _modifiers: &Modifiers| {
                // TODO: Implement variable reordering with entity
                let _ = panel_entity;
            }),
        };

        HierarchicalTreeView::new(config).render(cx)
    }

    fn render_variable_row(
        var: &ClassVariable,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
        let var_name = var.name.clone();
        let var_type = var.var_type.clone();
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        var.name.hash(&mut hasher);
        let var_hash = hasher.finish() as usize;

        // Get the color for this type
        let type_info = ui::graph::TypeInfo::parse(&var.var_type);
        let pin_color = type_info.generate_color();

        // Clone for the mouse down handler
        let drag_var_name = var_name.clone();
        let drag_var_type = var_type.clone();

        // Compact variable row
        h_flex()
            .w_full()
            .px_2()
            .py_1p5()
            .gap_2()
            .bg(cx.theme().background)
            .border_1()
            .border_color(cx.theme().border.opacity(0.3))
            .rounded(px(4.0))
            .cursor_grab()
            .hover(|style| {
                style
                    .bg(cx.theme().accent.opacity(0.05))
                    .border_color(cx.theme().accent.opacity(0.5))
            })
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |panel, _event, _window, cx| {
                    panel.start_dragging_variable(drag_var_name.clone(), drag_var_type.clone(), cx);
                }),
            )
            .child(
                ui::Icon::new(IconName::Menu)
                    .size(px(12.0))
                    .text_color(cx.theme().muted_foreground.opacity(0.5)),
            )
            .child(
                // Type indicator
                div()
                    .flex_shrink_0()
                    .w(px(10.))
                    .h(px(10.))
                    .rounded_full()
                    .bg(gpui::Rgba {
                        r: pin_color.r,
                        g: pin_color.g,
                        b: pin_color.b,
                        a: pin_color.a,
                    })
                    .border_1()
                    .border_color(cx.theme().border.opacity(0.5)),
            )
            .child(
                // Variable name
                div()
                    .flex_1()
                    .text_sm()
                    .font_medium()
                    .text_color(cx.theme().foreground)
                    .child(var.name.clone()),
            )
            .child(
                // Type badge
                div()
                    .px_1p5()
                    .py_0p5()
                    .rounded(px(3.0))
                    .bg(gpui::Rgba {
                        r: pin_color.r,
                        g: pin_color.g,
                        b: pin_color.b,
                        a: 0.15,
                    })
                    .child(
                        div()
                            .text_xs()
                            .text_color(gpui::Rgba {
                                r: pin_color.r,
                                g: pin_color.g,
                                b: pin_color.b,
                                a: 1.0,
                            })
                            .child(var.var_type.clone()),
                    ),
            )
            .child(
                Button::new(("delete-var", var_hash))
                    .icon(IconName::Trash)
                    .ghost()
                    .compact()
                    .tooltip("Delete")
                    .on_click(cx.listener(move |panel, _, _, cx| {
                        panel.remove_variable(&var_name, cx);
                    })),
            )
            .into_any_element()
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
