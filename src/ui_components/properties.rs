//! Unified multi-mode Properties panel.
//!
//! Dispatches to the appropriate detail renderer based on the current
//! selection type (prefab component, macro, event, variable, graph node,
//! or comment).  Mutual exclusivity of selection types is enforced by the
//! sidebar / graph click handlers — the renderer simply reads whichever
//! selection field is populated.

use gpui::prelude::FluentBuilder;
use gpui::*;
use pulsar_reflection::REGISTRY;
use ui::{
    button::Button,
    input::{InputEvent, InputState},
    scroll::ScrollbarAxis,
    IconName,
};
use ui::{
    button::ButtonVariants as _, h_flex, v_flex, ActiveTheme as _, Colorize, PixelsExt, Sizable,
    StyledExt,
};

use crate::core::types::{BlueprintComment, BlueprintNode};
use crate::editor::panel::BlueprintEditorPanel;
use crate::editor::workspace_panels::GraphCanvasPanel;
use crate::features::prefabs::panel::group_rows_by_category;
use std::any::Any;
use std::sync::Arc;
use ui_common::properties_inspector;

/// Unified multi-mode Properties panel renderer.
///
/// Dispatches to the appropriate sub-renderer based on selection priority:
/// prefab component → macro → event → variable → graph node → comment → empty.
mod event_details;
mod macro_details;
mod shared_controls;

pub struct PropertiesRenderer;

#[derive(Clone, Copy, Debug, PartialEq)]
enum SelectionKind {
    PrefabComponent,
    Macro,
    Event,
    Variable,
    GraphNode(usize),
    Comment(usize),
    Multi,
    None,
}

impl PropertiesRenderer {
    pub fn render(
        panel: &mut BlueprintEditorPanel,
        window: &mut Window,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        let active_canvas = panel.active_canvas().cloned();
        if let Some(canvas) = active_canvas.as_ref() {
            canvas.update(cx, |canvas, cx| {
                canvas.sync_comment_inspector_state(window, cx)
            });
        }

        let selection_kind = Self::active_selection_kind(panel, &active_canvas, cx);

        v_flex()
            .size_full()
            .bg(cx.theme().sidebar)
            .child(Self::render_header(selection_kind, cx))
            .child(
                v_flex().flex_1().overflow_hidden().child(
                    div()
                        .size_full()
                        .p_3()
                        .scrollable(ScrollbarAxis::Vertical)
                        .child(Self::render_properties_content(panel, window, cx)),
                ),
            )
    }

    fn active_selection_kind(
        panel: &BlueprintEditorPanel,
        active_canvas: &Option<Entity<GraphCanvasPanel>>,
        cx: &Context<BlueprintEditorPanel>,
    ) -> SelectionKind {
        if panel.selected_prefab_component.is_some() {
            return SelectionKind::PrefabComponent;
        }
        if panel.selected_macro.is_some() {
            return SelectionKind::Macro;
        }
        if panel.selected_event.is_some() {
            return SelectionKind::Event;
        }
        if panel.selected_variable.is_some() {
            return SelectionKind::Variable;
        }
        if let Some(canvas) = active_canvas {
            let graph = &canvas.read(cx).graph;
            let n = graph.selected_nodes.len();
            let c = graph.selected_comments.len();
            if n > 1 || (n > 0 && c > 0) || c > 1 {
                return SelectionKind::Multi;
            }
            if n == 1 {
                return SelectionKind::GraphNode(1);
            }
            if c == 1 {
                return SelectionKind::Comment(1);
            }
        }
        SelectionKind::None
    }

    fn render_header(
        selection_kind: SelectionKind,
        cx: &Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        let (title, icon, badge) = match &selection_kind {
            SelectionKind::PrefabComponent => ("Properties", IconName::Component, "Component"),
            SelectionKind::Macro => ("Properties", IconName::GitBranch, "Macro"),
            SelectionKind::Event => ("Properties", IconName::Flash, "Event"),
            SelectionKind::Variable => ("Properties", IconName::Component, "Variable"),
            SelectionKind::GraphNode(_) => ("Properties", IconName::Component, "Node"),
            SelectionKind::Comment(_) => ("Properties", IconName::Info, "Comment"),
            SelectionKind::Multi => ("Properties", IconName::Copy, "Multiple"),
            SelectionKind::None => ("Properties", IconName::Settings, "None"),
        };

        let has_selection = !matches!(selection_kind, SelectionKind::None);

        properties_inspector::render_header(title, has_selection, badge, "properties-more", cx)
    }

    fn render_properties_content(
        panel: &mut BlueprintEditorPanel,
        window: &mut Window,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
        // ── Prefab component selected ──────────────────────────────────────
        if let Some(index) = panel.selected_prefab_component {
            return Self::render_prefab_component_properties(panel, index, window, cx);
        }

        // ── Macro selected ─────────────────────────────────────────────────
        if let Some(index) = panel.selected_macro {
            return Self::render_macro_details(panel, index, window, cx);
        }

        // ── Event selected ─────────────────────────────────────────────────
        if let Some(index) = panel.selected_event {
            return Self::render_event_details(panel, index, window, cx);
        }

        // ── Variable selected ──────────────────────────────────────────────
        if let Some(index) = panel.selected_variable {
            return Self::render_variable_details(panel, index, cx);
        }

        let active_canvas_opt = panel.active_canvas().cloned();
        let canvas_ref = active_canvas_opt.as_ref().map(|c| c.read(cx));
        let Some(canvas) = canvas_ref else {
            return Self::render_empty_state(cx);
        };

        let sel_nodes = canvas.graph.selected_nodes.clone();
        let sel_comments = canvas.graph.selected_comments.clone();
        let sel_count = sel_nodes.len();
        let com_count = sel_comments.len();

        // ── Single comment selected ──────────────────────────────────────
        if com_count == 1 && sel_count == 0 {
            let comment_id = &sel_comments[0];
            let selected_comment = canvas
                .graph
                .comments
                .iter()
                .find(|c| &c.id == comment_id)
                .cloned();
            if let Some(comment) = selected_comment {
                return Self::render_comment_properties(panel, &comment, window, cx);
            }
            return Self::render_empty_state(cx);
        }

        // ── Single node selected ──────────────────────────────────────────
        if sel_count == 1 && com_count == 0 {
            let selected_node_id = &sel_nodes[0];
            let node_found = canvas.graph.nodes.iter().any(|n| &n.id == selected_node_id);
            if !node_found {
                return Self::render_empty_state(cx);
            }
            if let Some(active_canvas) = active_canvas_opt {
                return active_canvas.update(cx, |canvas, cx| {
                    Self::render_selected_node_properties(canvas, window, cx)
                });
            }
        }

        // ── Multi-selection ───────────────────────────────────────────────
        if sel_count > 1 || com_count > 0 {
            return Self::render_multi_selection_state(sel_count, com_count, cx);
        }

        // ── Nothing selected ──────────────────────────────────────────────
        Self::render_empty_state(cx)
    }

    // ── Prefab component properties ──────────────────────────────────────────

    fn render_prefab_component_properties(
        panel: &mut BlueprintEditorPanel,
        index: usize,
        window: &mut Window,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
        let Some(component) = panel.prefab_asset.components.get(index).cloned() else {
            return Self::render_empty_state(cx);
        };

        let class_name = component.class_name.clone();
        let state_key = format!("{}#{}", index, class_name);
        let mut missing_in_registry = false;
        let mut row_data: Vec<(
            AnyElement,
            Option<String>,
            Option<String>,
            bool,
            Option<usize>,
        )> = Vec::new();

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
                        if let Ok(json) = pulsar_reflection::RUNTIME_TYPE_REGISTRY
                            .serialize_json_for_any(new_val.as_ref())
                        {
                            panel_for_wb.update(cx, |panel, cx| {
                                panel.update_prefab_component_property(
                                    index,
                                    &prop_name_for_wb,
                                    json,
                                );
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

        Self::render_card(
            [
                h_flex()
                    .w_full()
                    .p_3()
                    .gap_2()
                    .items_center()
                    .child(ui::Icon::new(IconName::Component).size(px(16.0)))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(cx.theme().foreground)
                            .child(class_name.clone()),
                    )
                    .into_any_element(),
                if missing_in_registry {
                    div()
                        .w_full()
                        .px_3()
                        .pb_3()
                        .text_sm()
                        .text_color(cx.theme().warning)
                        .child("This component class is not available in the reflection registry.")
                        .into_any_element()
                } else {
                    let (mut uncategorized, categorized) = group_rows_by_category(row_data);
                    let category_elements =
                        Self::render_categorized_rows(panel, index, categorized, cx);
                    uncategorized.extend(category_elements);

                    v_flex()
                        .w_full()
                        .p_3()
                        .gap_2()
                        .children(uncategorized)
                        .into_any_element()
                },
            ],
            cx,
        )
        .into_any_element()
    }

    fn render_categorized_rows(
        panel: &BlueprintEditorPanel,
        component_index: usize,
        mut categorized_rows: Vec<(String, Vec<AnyElement>, Option<String>, bool, usize)>,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> Vec<AnyElement> {
        categorized_rows.sort_by_key(|(_, _, _, _, order)| *order);

        categorized_rows
            .into_iter()
            .map(
                |(category_name, category_rows, category_color_hex, default_collapsed, _)| {
                    let category_key = (component_index, category_name.clone());

                    let is_collapsed = if panel.prefab_collapsed_categories.contains(&category_key)
                    {
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

                    div()
                        .w_full()
                        .pb(px(8.0))
                        .child(
                            h_flex()
                                .w_full()
                                .items_stretch()
                                .gap_1p5()
                                .child(
                                    div()
                                        .w(px(3.0))
                                        .rounded_full()
                                        .flex_shrink_0()
                                        .when_some(accent, |el, color| el.bg(color.opacity(0.85)))
                                        .when(accent.is_none(), |el| {
                                            el.bg(cx.theme().muted.opacity(0.35))
                                        }),
                                )
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .gap_1()
                                        .child(
                                            h_flex()
                                                .w_full()
                                                .items_center()
                                                .justify_between()
                                                .cursor_pointer()
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(
                                                        move |this, _event, _window, cx| {
                                                            if was_collapsed {
                                                                this.prefab_collapsed_categories
                                                                    .remove(&toggle_key);
                                                                this.prefab_expanded_categories
                                                                    .insert(toggle_key.clone());
                                                            } else {
                                                                this.prefab_expanded_categories
                                                                    .remove(&toggle_key);
                                                                this.prefab_collapsed_categories
                                                                    .insert(toggle_key.clone());
                                                            }
                                                            cx.notify();
                                                        },
                                                    ),
                                                )
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .when_some(accent, |el, color| {
                                                            el.text_color(color)
                                                        })
                                                        .when(accent.is_none(), |el| {
                                                            el.text_color(
                                                                cx.theme().muted_foreground,
                                                            )
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
                                                    .when_some(accent, |el, color| {
                                                        el.text_color(color)
                                                    })
                                                    .when(accent.is_none(), |el| {
                                                        el.text_color(cx.theme().muted_foreground)
                                                    }),
                                                ),
                                        )
                                        .when(!is_collapsed, |el| el.children(category_rows)),
                                ),
                        )
                        .into_any_element()
                },
            )
            .collect()
    }

    // ── Macro details (editable pins) ────────────────────────────────────────

    fn render_variable_details(
        panel: &BlueprintEditorPanel,
        index: usize,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
        let Some(var) = panel.class_variables.get(index) else {
            return Self::render_empty_state(cx);
        };

        v_flex()
            .gap_3()
            // Title card
            .child(Self::render_card(
                [
                    h_flex()
                        .w_full()
                        .p_3()
                        .gap_3()
                        .items_center()
                        .child(
                            ui::Icon::new(IconName::Component)
                                .size(px(18.0))
                                .text_color(cx.theme().info),
                        )
                        .child(
                            div()
                                .text_lg()
                                .font_bold()
                                .text_color(cx.theme().foreground)
                                .child(var.name.clone()),
                        )
                        .into_any_element(),
                    div()
                        .w_full()
                        .px_3()
                        .pb_3()
                        .child(
                            div()
                                .px_2()
                                .py_1()
                                .rounded(px(4.0))
                                .bg(cx.theme().info.opacity(0.15))
                                .border_1()
                                .border_color(cx.theme().info.opacity(0.3))
                                .text_xs()
                                .font_semibold()
                                .text_color(cx.theme().info)
                                .child("Variable"),
                        )
                        .into_any_element(),
                ],
                cx,
            ))
            // Variable Info card
            .child(Self::render_card(
                [
                    Self::render_section_header("Variable Info", IconName::Info, cx)
                        .px_3()
                        .pt_3()
                        .into_any_element(),
                    v_flex()
                        .w_full()
                        .p_3()
                        .gap_2p5()
                        .child(Self::render_info_row("Type", &var.var_type, cx))
                        .child(Self::render_info_row(
                            "Default Value",
                            &var.default_value.clone().unwrap_or_else(|| "—".to_string()),
                            cx,
                        ))
                        .into_any_element(),
                ],
                cx,
            ))
            .into_any_element()
    }

    fn render_selected_node_readonly<T>(
        selected_node: &BlueprintNode,
        cx: &mut Context<T>,
    ) -> AnyElement {
        v_flex()
            .gap_3()
            // Title card
            .child(Self::render_card(
                [
                    h_flex()
                        .w_full()
                        .p_3()
                        .gap_3()
                        .items_center()
                        .child(div().text_2xl().child(selected_node.icon.clone()))
                        .child(
                            div()
                                .text_lg()
                                .font_bold()
                                .text_color(cx.theme().foreground)
                                .child(selected_node.title.clone()),
                        )
                        .into_any_element(),
                    div()
                        .w_full()
                        .px_3()
                        .pb_3()
                        .child(
                            div()
                                .px_2()
                                .py_1()
                                .rounded(px(4.0))
                                .bg(Self::get_node_type_color(&selected_node.node_type, cx)
                                    .opacity(0.15))
                                .border_1()
                                .border_color(
                                    Self::get_node_type_color(&selected_node.node_type, cx)
                                        .opacity(0.3),
                                )
                                .text_xs()
                                .font_semibold()
                                .text_color(Self::get_node_type_color(&selected_node.node_type, cx))
                                .child(format!("{:?} Node", selected_node.node_type)),
                        )
                        .into_any_element(),
                ],
                cx,
            ))
            // Inputs card
            .when(!selected_node.inputs.is_empty(), |el| {
                el.child(Self::render_card(
                    [
                        Self::render_section_header("Inputs", IconName::ArrowRight, cx)
                            .px_3()
                            .pt_3()
                            .into_any_element(),
                        v_flex()
                            .w_full()
                            .p_3()
                            .gap_1p5()
                            .child(Self::render_pin_list(&selected_node.inputs, cx))
                            .into_any_element(),
                    ],
                    cx,
                ))
            })
            // Outputs card
            .when(!selected_node.outputs.is_empty(), |el| {
                el.child(Self::render_card(
                    [
                        Self::render_section_header("Outputs", IconName::ArrowRight, cx)
                            .px_3()
                            .pt_3()
                            .into_any_element(),
                        v_flex()
                            .w_full()
                            .p_3()
                            .gap_1p5()
                            .child(Self::render_pin_list(&selected_node.outputs, cx))
                            .into_any_element(),
                    ],
                    cx,
                ))
            })
            // Properties card
            .when(!selected_node.properties.is_empty(), |el| {
                el.child(Self::render_card(
                    [
                        Self::render_section_header("Properties", IconName::Settings, cx)
                            .px_3()
                            .pt_3()
                            .into_any_element(),
                        v_flex()
                            .w_full()
                            .p_3()
                            .gap_2()
                            .child(Self::render_node_properties(selected_node, cx))
                            .into_any_element(),
                    ],
                    cx,
                ))
            })
            // Node Info card
            .child(Self::render_card(
                [
                    Self::render_section_header("Node Info", IconName::Info, cx)
                        .px_3()
                        .pt_3()
                        .into_any_element(),
                    v_flex()
                        .w_full()
                        .p_3()
                        .gap_2p5()
                        .child(Self::render_node_info(selected_node, cx))
                        .into_any_element(),
                ],
                cx,
            ))
            .into_any_element()
    }

    fn render_selected_node_properties(
        canvas: &mut GraphCanvasPanel,
        window: &mut Window,
        cx: &mut Context<GraphCanvasPanel>,
    ) -> AnyElement {
        let selected_node_id = canvas.graph.selected_nodes.first().cloned();
        let Some(selected_node_id) = selected_node_id else {
            return Self::render_empty_state(cx);
        };
        let Some(selected_node) = canvas
            .graph
            .nodes
            .iter()
            .find(|n| n.id == selected_node_id)
            .cloned()
        else {
            return Self::render_empty_state(cx);
        };

        let canvas_entity = cx.entity().clone();

        v_flex()
            .gap_3()
            // Title card
            .child(Self::render_card(
                [
                    h_flex()
                        .w_full()
                        .p_3()
                        .gap_3()
                        .items_center()
                        .child(div().text_2xl().child(selected_node.icon.clone()))
                        .child(
                            div()
                                .text_lg()
                                .font_bold()
                                .text_color(cx.theme().foreground)
                                .child(selected_node.title.clone()),
                        )
                        .into_any_element(),
                    div()
                        .w_full()
                        .px_3()
                        .pb_3()
                        .child(
                            div()
                                .px_2()
                                .py_1()
                                .rounded(px(4.0))
                                .bg(Self::get_node_type_color(&selected_node.node_type, cx)
                                    .opacity(0.15))
                                .border_1()
                                .border_color(
                                    Self::get_node_type_color(&selected_node.node_type, cx)
                                        .opacity(0.3),
                                )
                                .text_xs()
                                .font_semibold()
                                .text_color(Self::get_node_type_color(&selected_node.node_type, cx))
                                .child(format!("{:?} Node", selected_node.node_type)),
                        )
                        .into_any_element(),
                ],
                cx,
            ))
            // Inputs card
            .when(!selected_node.inputs.is_empty(), |el| {
                el.child(Self::render_card(
                    [
                        Self::render_section_header("Inputs", IconName::ArrowRight, cx)
                            .px_3()
                            .pt_3()
                            .into_any_element(),
                        v_flex()
                            .w_full()
                            .p_3()
                            .gap_1p5()
                            .child(Self::render_pin_editors(
                                canvas,
                                &canvas_entity,
                                &selected_node,
                                window,
                                cx,
                            ))
                            .into_any_element(),
                    ],
                    cx,
                ))
            })
            // Outputs card
            .when(!selected_node.outputs.is_empty(), |el| {
                el.child(Self::render_card(
                    [
                        Self::render_section_header("Outputs", IconName::ArrowRight, cx)
                            .px_3()
                            .pt_3()
                            .into_any_element(),
                        v_flex()
                            .w_full()
                            .p_3()
                            .gap_1p5()
                            .child(Self::render_pin_list(&selected_node.outputs, cx))
                            .into_any_element(),
                    ],
                    cx,
                ))
            })
            // Properties card
            .when(!selected_node.properties.is_empty(), |el| {
                el.child(Self::render_card(
                    [
                        Self::render_section_header("Properties", IconName::Settings, cx)
                            .px_3()
                            .pt_3()
                            .into_any_element(),
                        v_flex()
                            .w_full()
                            .p_3()
                            .gap_2()
                            .child(Self::render_node_properties(&selected_node, cx))
                            .into_any_element(),
                    ],
                    cx,
                ))
            })
            // Node Info card
            .child(Self::render_card(
                [
                    Self::render_section_header("Node Info", IconName::Info, cx)
                        .px_3()
                        .pt_3()
                        .into_any_element(),
                    v_flex()
                        .w_full()
                        .p_3()
                        .gap_2p5()
                        .child(Self::render_node_info(&selected_node, cx))
                        .into_any_element(),
                ],
                cx,
            ))
            .into_any_element()
    }

    fn render_comment_properties(
        panel: &BlueprintEditorPanel,
        comment: &BlueprintComment,
        window: &mut Window,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
        let active_canvas = panel.active_canvas().cloned();
        let mut comment_color = comment.color;
        let mut color_picker = None;
        let mut comment_text_input = None;

        if let Some(canvas) = active_canvas {
            let canvas_state = canvas.read(cx);
            comment_text_input = Some(canvas_state.comment_text_input.clone());
            if let Some(selected) = canvas_state
                .graph
                .comments
                .iter()
                .find(|c| c.id == comment.id)
            {
                comment_color = selected.color;
                color_picker = selected.color_picker_state.clone();
            }
        }

        // Keep the picker controls initialized from the selected comment.
        if let Some(picker) = color_picker.as_ref() {
            if picker.read(cx).value() != Some(comment_color) {
                picker.update(cx, |picker, cx| picker.set_value(comment_color, window, cx));
            }
        }

        v_flex()
            .gap_3()
            // Title card
            .child(Self::render_card(
                [
                    h_flex()
                        .w_full()
                        .p_3()
                        .gap_3()
                        .items_center()
                        .child(
                            ui::Icon::new(IconName::Info)
                                .size(px(18.0))
                                .text_color(cx.theme().info),
                        )
                        .child(
                            div()
                                .text_lg()
                                .font_bold()
                                .text_color(cx.theme().foreground)
                                .child(comment.text.clone()),
                        )
                        .into_any_element(),
                    div()
                        .w_full()
                        .px_3()
                        .pb_3()
                        .child(
                            div()
                                .px_2()
                                .py_1()
                                .rounded(px(4.0))
                                .bg(comment_color.opacity(0.15))
                                .border_1()
                                .border_color(comment_color.opacity(0.3))
                                .text_xs()
                                .font_semibold()
                                .text_color(comment_color)
                                .child("Comment"),
                        )
                        .into_any_element(),
                ],
                cx,
            ))
            // Comment Properties card
            .child(Self::render_card(
                [
                    Self::render_section_header("Comment Properties", IconName::Settings, cx)
                        .px_3()
                        .pt_3()
                        .into_any_element(),
                    v_flex()
                        .w_full()
                        .p_3()
                        .gap_3()
                        .child(
                            v_flex()
                                .gap_2()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_semibold()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("Name"),
                                )
                                .child(
                                    comment_text_input
                                        .map(|input| div().w_full().child(input).into_any_element())
                                        .unwrap_or_else(|| {
                                            div()
                                                .w_full()
                                                .text_sm()
                                                .text_color(cx.theme().muted_foreground)
                                                .child("No comment editor available")
                                                .into_any_element()
                                        }),
                                ),
                        )
                        .child(
                            v_flex()
                                .gap_2()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_semibold()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("Color"),
                                )
                                .child(
                                    h_flex()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            div()
                                                .w(px(24.0))
                                                .h(px(24.0))
                                                .rounded(px(4.0))
                                                .border_1()
                                                .border_color(cx.theme().border)
                                                .bg(comment_color)
                                                .into_any_element(),
                                        )
                                        .child(
                                            color_picker
                                                .map(|picker| {
                                                    div()
                                                        .w_full()
                                                        .child(ui::color_picker::ColorPicker::new(
                                                            &picker,
                                                        ))
                                                        .into_any_element()
                                                })
                                                .unwrap_or_else(|| {
                                                    div()
                                                        .text_xs()
                                                        .text_color(cx.theme().muted_foreground)
                                                        .child("Color picker unavailable")
                                                        .into_any_element()
                                                }),
                                        ),
                                ),
                        )
                        .into_any_element(),
                ],
                cx,
            ))
            // Comment Info card
            .child(Self::render_card(
                [
                    Self::render_section_header("Comment Info", IconName::Info, cx)
                        .px_3()
                        .pt_3()
                        .into_any_element(),
                    v_flex()
                        .w_full()
                        .p_3()
                        .gap_2p5()
                        .child(Self::render_info_row("Comment ID", &comment.id, cx))
                        .child(Self::render_info_row(
                            "Position",
                            &format!("({:.0}, {:.0})", comment.position.x, comment.position.y),
                            cx,
                        ))
                        .child(Self::render_info_row(
                            "Size",
                            &format!("{:.0} × {:.0} px", comment.size.width, comment.size.height),
                            cx,
                        ))
                        .child(Self::render_info_row(
                            "Contained Nodes",
                            &comment.contained_node_ids.len().to_string(),
                            cx,
                        ))
                        .into_any_element(),
                ],
                cx,
            ))
            .into_any_element()
    }
}
