//! Properties panel renderer for displaying node and graph properties.
//!
//! Shows detailed information about selected nodes, including properties,
//! type information, and connection details. Also provides macro interface
//! editing when inside sub-graphs.

use gpui::prelude::FluentBuilder;
use gpui::*;
use ui::{
    button::ButtonVariants as _, h_flex, v_flex, ActiveTheme as _, Colorize, IconName, StyledExt,
};

use crate::core::types::{BlueprintComment, BlueprintNode, NodeType, Pin};
use crate::editor::panel::BlueprintEditorPanel;
use crate::editor::workspace_panels::GraphCanvasPanel;
use crate::features::connections::compatibility::is_pin_connected;
use ui_common::reflected_properties_panel::rgba_to_hsla;
use std::sync::Arc;

/// Renderer for the properties panel
pub struct PropertiesRenderer;

impl PropertiesRenderer {
    pub fn render(
        panel: &BlueprintEditorPanel,
        window: &mut Window,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        let active_canvas = panel.active_canvas().cloned();
        let panel_graph: crate::core::graph::BlueprintGraph = active_canvas
            .as_ref()
            .map(|c| c.read(cx).graph.clone())
            .unwrap_or_default();
        if let Some(canvas) = active_canvas.as_ref() {
            canvas.update(cx, |canvas, cx| canvas.sync_comment_inspector_state(window, cx));
        }
        v_flex()
            .size_full()
            .bg(cx.theme().sidebar)
            .child(
                // STUDIO-QUALITY HEADER (Unreal Details panel style)
                v_flex()
                    .w_full()
                    .child(
                        // Main header with professional styling
                        h_flex()
                            .w_full()
                            .px_2()
                            .py_1p5()
                            .bg(cx.theme().secondary)
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .items_center()
                            .gap_2()
                            .child(
                                ui::Icon::new(IconName::Settings)
                                    .size(px(16.0))
                                    .text_color(cx.theme().info),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    .child("Details"),
                            )
                            .child(
                                div().flex_1().text_right().child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(if panel_graph.selected_nodes.len() > 1 {
                                            format!("{} items", panel_graph.selected_nodes.len())
                                        } else if panel_graph.selected_nodes.len() == 1 {
                                            "1 item".to_string()
                                        } else {
                                            "None".to_string()
                                        }),
                                ),
                            ),
                    )
                    .child(
                        // Compact selection type indicator
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
                                ui::Icon::new(if panel_graph.selected_nodes.len() > 1 {
                                    IconName::Copy
                                } else {
                                    IconName::Component
                                })
                                .size(px(12.0))
                                .text_color(cx.theme().info.opacity(0.8)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(if panel_graph.selected_nodes.len() > 1 {
                                        "Multiple"
                                    } else if panel_graph.selected_nodes.len() == 1 {
                                        "Properties"
                                    } else {
                                        "NO SELECTION"
                                    }),
                            )
                            .child(if !panel_graph.selected_nodes.is_empty() {
                                div()
                                    .px_2()
                                    .py_1()
                                    .rounded(px(4.0))
                                    .bg(cx.theme().info.opacity(0.15))
                                    .text_xs()
                                    .font_family("JetBrainsMono-Regular")
                                    .text_color(cx.theme().info)
                                    .child(format!("{}", panel_graph.selected_nodes.len()))
                            } else {
                                div() // Empty div when no selection
                            }),
                    ),
            )
            .child(
                // CONTENT AREA - clean scrollable content
                v_flex()
                    .flex_1()
                    .overflow_hidden()
                    .p_3()
                    .scrollable(Axis::Vertical)
                    .child(Self::render_properties_content(panel, window, cx)),
            )
    }

    fn render_properties_content(
        panel: &BlueprintEditorPanel,
        window: &mut Window,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
        let active_canvas = panel.active_canvas().cloned();
        let panel_graph: crate::core::graph::BlueprintGraph = active_canvas
            .as_ref()
            .map(|c| c.read(cx).graph.clone())
            .unwrap_or_default();
        if panel_graph.selected_comments.len() == 1 && panel_graph.selected_nodes.is_empty() {
            let selected_comment_id = &panel_graph.selected_comments[0];
            if let Some(selected_comment) = panel_graph
                .comments
                .iter()
                .find(|c| c.id == *selected_comment_id)
            {
                return Self::render_comment_properties(panel, selected_comment, window, cx);
            }
        }

        if panel_graph.selected_nodes.len() == 1 && panel_graph.selected_comments.is_empty() {
            if let Some(canvas) = active_canvas {
                return canvas.update(cx, |canvas, cx| {
                    Self::render_selected_node_properties(canvas, window, cx)
                });
            }

            let selected_node_id = &panel_graph.selected_nodes[0];
            if let Some(selected_node) = panel_graph.nodes.iter().find(|n| n.id == *selected_node_id)
            {
                return Self::render_selected_node_readonly(selected_node, cx);
            } else {
                return Self::render_empty_state(cx);
            }
        } else if !panel_graph.selected_nodes.is_empty() || !panel_graph.selected_comments.is_empty()
        {
            return Self::render_multi_selection_state(
                panel_graph.selected_nodes.len(),
                panel_graph.selected_comments.len(),
                cx,
            );
        } else {
            Self::render_empty_state(cx)
        }
    }

    fn render_selected_node_readonly<T>(
        selected_node: &BlueprintNode,
        cx: &mut Context<T>,
    ) -> AnyElement {
        v_flex()
            .gap_4()
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_3()
                            .child(div().text_2xl().child(selected_node.icon.clone()))
                            .child(
                                div()
                                    .text_lg()
                                    .font_bold()
                                    .text_color(cx.theme().foreground)
                                    .child(selected_node.title.clone()),
                            ),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded(px(4.0))
                            .bg(Self::get_node_type_color(&selected_node.node_type, cx).opacity(0.15))
                            .border_1()
                            .border_color(
                                Self::get_node_type_color(&selected_node.node_type, cx).opacity(0.3),
                            )
                            .text_xs()
                            .font_semibold()
                            .text_color(Self::get_node_type_color(&selected_node.node_type, cx))
                            .child(format!("{:?} Node", selected_node.node_type)),
                    ),
            )
            .when(!selected_node.inputs.is_empty(), |el| {
                el.child(Self::render_separator(cx)).child(
                    v_flex()
                        .gap_3()
                        .child(Self::render_section_header("Inputs", IconName::ArrowRight, cx))
                        .child(Self::render_pin_list(&selected_node.inputs, cx)),
                )
            })
            .when(!selected_node.outputs.is_empty(), |el| {
                el.child(Self::render_separator(cx)).child(
                    v_flex()
                        .gap_3()
                        .child(Self::render_section_header("Outputs", IconName::ArrowRight, cx))
                        .child(Self::render_pin_list(&selected_node.outputs, cx)),
                )
            })
            .when(!selected_node.properties.is_empty(), |el| {
                el.child(Self::render_separator(cx)).child(
                    v_flex()
                        .gap_3()
                        .child(Self::render_section_header("Properties", IconName::Settings, cx))
                        .child(Self::render_node_properties(selected_node, cx)),
                )
            })
            .child(Self::render_separator(cx))
            .child(
                v_flex()
                    .gap_3()
                    .child(Self::render_section_header("Node Info", IconName::Info, cx))
                    .child(Self::render_node_info(selected_node, cx)),
            )
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
        let Some(selected_node) = canvas.graph.nodes.iter().find(|n| n.id == selected_node_id).cloned()
        else {
            return Self::render_empty_state(cx);
        };

        let canvas_entity = cx.entity().clone();

        v_flex()
            .gap_4()
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_3()
                            .child(div().text_2xl().child(selected_node.icon.clone()))
                            .child(
                                div()
                                    .text_lg()
                                    .font_bold()
                                    .text_color(cx.theme().foreground)
                                    .child(selected_node.title.clone()),
                            ),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded(px(4.0))
                            .bg(Self::get_node_type_color(&selected_node.node_type, cx).opacity(0.15))
                            .border_1()
                            .border_color(
                                Self::get_node_type_color(&selected_node.node_type, cx).opacity(0.3),
                            )
                            .text_xs()
                            .font_semibold()
                            .text_color(Self::get_node_type_color(&selected_node.node_type, cx))
                            .child(format!("{:?} Node", selected_node.node_type)),
                    ),
            )
            .when(!selected_node.inputs.is_empty(), |el| {
                el.child(Self::render_separator(cx)).child(
                    v_flex()
                        .gap_3()
                        .child(Self::render_section_header("Inputs", IconName::ArrowRight, cx))
                        .child(Self::render_pin_editors(
                            canvas,
                            &canvas_entity,
                            &selected_node,
                            window,
                            cx,
                        )),
                )
            })
            .when(!selected_node.outputs.is_empty(), |el| {
                el.child(Self::render_separator(cx)).child(
                    v_flex()
                        .gap_3()
                        .child(Self::render_section_header("Outputs", IconName::ArrowRight, cx))
                        .child(Self::render_pin_list(&selected_node.outputs, cx)),
                )
            })
            .when(!selected_node.properties.is_empty(), |el| {
                el.child(Self::render_separator(cx)).child(
                    v_flex()
                        .gap_3()
                        .child(Self::render_section_header("Properties", IconName::Settings, cx))
                        .child(Self::render_node_properties(&selected_node, cx)),
                )
            })
            .child(Self::render_separator(cx))
            .child(
                v_flex()
                    .gap_3()
                    .child(Self::render_section_header("Node Info", IconName::Info, cx))
                    .child(Self::render_node_info(&selected_node, cx)),
            )
            .into_any_element()
    }

    fn render_comment_properties(
        panel: &BlueprintEditorPanel,
        comment: &BlueprintComment,
        _window: &mut Window,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
        let active_canvas = panel.active_canvas().cloned();
        let mut comment_color = comment.color;
        let mut color_picker = None;
        let mut comment_text_input = None;

        if let Some(canvas) = active_canvas {
            let canvas_state = canvas.read(cx);
            comment_text_input = Some(canvas_state.comment_text_input.clone());
            if let Some(selected) = canvas_state.graph.comments.iter().find(|c| c.id == comment.id) {
                comment_color = selected.color;
                color_picker = selected.color_picker_state.clone();
            }
        }

        v_flex()
            .gap_4()
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_3()
                            .child(div().text_2xl().child("💬"))
                            .child(
                                div()
                                    .text_lg()
                                    .font_bold()
                                    .text_color(cx.theme().foreground)
                                    .child(comment.text.clone()),
                            ),
                    )
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
                    ),
            )
            .child(Self::render_separator(cx))
            .child(
                v_flex()
                    .gap_3()
                    .child(Self::render_section_header(
                        "Comment Properties",
                        IconName::Settings,
                        cx,
                    ))
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
                                    .child(color_picker.map(|picker| {
                                        div().w_full().child(picker).into_any_element()
                                    }).unwrap_or_else(|| {
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child("Color picker unavailable")
                                            .into_any_element()
                                    })),
                            ),
                    ),
            )
            .child(Self::render_separator(cx))
            .child(
                v_flex()
                    .gap_3()
                    .child(Self::render_section_header("Comment Info", IconName::Info, cx))
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
                    )),
            )
            .into_any_element()
    }

    fn render_multi_selection_state<T>(
        node_count: usize,
        comment_count: usize,
        cx: &mut Context<T>,
    ) -> AnyElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_3()
            .child(div().text_xl().child("🗂️"))
            .child(
                div()
                    .text_sm()
                    .font_medium()
                    .text_color(cx.theme().muted_foreground)
                    .child("Multiple selection"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground.opacity(0.7))
                    .child(format!("{} nodes, {} comments selected", node_count, comment_count)),
            )
            .into_any_element()
    }

    fn render_section_header<T>(
        title: &str,
        _icon: IconName,
        cx: &mut Context<T>,
    ) -> impl IntoElement {
        h_flex().items_center().gap_2().child(
            div()
                .text_xs()
                .font_bold()
                .text_color(cx.theme().accent)
                .child(title.to_uppercase()),
        )
    }

    fn render_separator<T>(cx: &mut Context<T>) -> impl IntoElement {
        div().w_full().h_px().bg(cx.theme().border.opacity(0.3))
    }

    fn get_node_type_color<T>(
        node_type: &NodeType,
        cx: &mut Context<T>,
    ) -> gpui::Hsla {
        match node_type {
            NodeType::Event => cx.theme().danger,
            NodeType::Logic => cx.theme().primary,
            NodeType::Math => cx.theme().success,
            NodeType::Object => cx.theme().warning,
            NodeType::Reroute => cx.theme().accent,
            NodeType::MacroEntry => gpui::Hsla {
                h: 0.75,
                s: 0.7,
                l: 0.6,
                a: 1.0,
            },
            NodeType::MacroExit => gpui::Hsla {
                h: 0.75,
                s: 0.7,
                l: 0.6,
                a: 1.0,
            },
            NodeType::MacroInstance => gpui::Hsla {
                h: 0.75,
                s: 0.5,
                l: 0.5,
                a: 1.0,
            },
        }
    }

    fn render_empty_state<T>(cx: &mut Context<T>) -> AnyElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_3()
            .child(div().text_xl().child("📋📋📋"))
            .child(
                div()
                    .text_sm()
                    .font_medium()
                    .text_color(cx.theme().muted_foreground)
                    .child("No node selected"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground.opacity(0.7))
                    .child("Select a node to view its properties"),
            )
            .into_any_element()
    }

    /// Render a pin list section — type display (badge color, resolved name)
    /// is sourced entirely from `PinDataType`/`RuntimeTypeInfo`, the same
    /// canonical reflection-backed lookup the graph view uses for pin colors,
    /// so the panel and graph always agree visually.
    fn render_pin_list<T>(
        pins: &[Pin],
        cx: &mut Context<T>,
    ) -> impl IntoElement {
        v_flex()
            .gap_1p5()
            .children(pins.iter().map(|pin| Self::render_pin_row(pin, cx)))
    }

    fn render_pin_editors(
        canvas: &mut GraphCanvasPanel,
        canvas_entity: &Entity<GraphCanvasPanel>,
        node: &BlueprintNode,
        window: &mut Window,
        cx: &mut Context<GraphCanvasPanel>,
    ) -> impl IntoElement {
        v_flex()
            .gap_1p5()
            .children(node.inputs.iter().map(|pin| {
                Self::render_input_pin_row(canvas, canvas_entity, node, pin, window, cx)
            }))
    }

    fn render_input_pin_row(
        canvas: &mut GraphCanvasPanel,
        canvas_entity: &Entity<GraphCanvasPanel>,
        node: &BlueprintNode,
        pin: &Pin,
        window: &mut Window,
        cx: &mut Context<GraphCanvasPanel>,
    ) -> AnyElement {
        let row = Self::render_pin_row(pin, cx);
        if is_pin_connected(&node.id, &pin.id, true, &canvas.graph) {
            return row.into_any_element();
        }

        let Some(type_info) = pin.data_type.runtime_type() else {
            return row.into_any_element();
        };

        let state_key = format!("{}#{}", node.id, pin.id);
        let widgets = canvas.pin_property_state.widget_map_for(&state_key, &pin.id);
        let current_value = Self::read_pin_property_value(node, &pin.id);
        let node_id = node.id.clone();
        let pin_id = pin.id.clone();
        let canvas_for_bool = canvas_entity.clone();
        let on_bool_toggle = Arc::new(
            move |checked: bool, _window: &mut Window, cx: &mut App| {
                canvas_for_bool.update(cx, |canvas, cx| {
                    canvas.update_node_input_property(&node_id, &pin_id, serde_json::Value::Bool(checked), cx);
                });
            },
        );

        let canvas_for_enum = canvas_entity.clone();
        let node_id_for_enum = node.id.clone();
        let pin_id_for_enum = pin.id.clone();
        let on_enum_select = Arc::new(
            move |ix: usize, _window: &mut Window, cx: &mut App| {
                canvas_for_enum.update(cx, |canvas, cx| {
                    canvas.update_node_input_property(
                        &node_id_for_enum,
                        &pin_id_for_enum,
                        serde_json::Value::from(ix as u64),
                        cx,
                    );
                });
            },
        );

        let editor = ui_common::render_property_row_runtime(
            "node-input",
            &state_key,
            &Self::format_property_name(&pin.name),
            &pin.id,
            type_info,
            &current_value,
            widgets,
            on_bool_toggle,
            on_enum_select,
            cx,
        );

        v_flex()
            .gap_1p5()
            .child(row)
            .child(editor)
            .into_any_element()
    }

    fn read_pin_property_value(node: &BlueprintNode, pin_id: &str) -> serde_json::Value {
        let Some(raw_value) = node.properties.get(pin_id) else {
            return serde_json::Value::Null;
        };

        serde_json::from_str(raw_value)
            .unwrap_or_else(|_| serde_json::Value::String(raw_value.clone()))
    }

    fn render_pin_row<T>(pin: &Pin, cx: &mut Context<T>) -> impl IntoElement {
        let badge_color: gpui::Hsla = rgba_to_hsla(pin.data_type.display_color()).into();

        let type_label = if pin.data_type.is_execution() {
            "Execution".to_string()
        } else if pin.data_type.is_wildcard() {
            "Wildcard".to_string()
        } else {
            pin.data_type.type_name.clone()
        };

        h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .px_3()
            .py_2()
            .rounded(px(4.0))
            .hover(|style| style.bg(cx.theme().muted.opacity(0.1)))
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .size(px(8.0))
                            .rounded_full()
                            .bg(badge_color),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(cx.theme().foreground)
                            .child(if pin.name.is_empty() {
                                "(unnamed)".to_string()
                            } else {
                                pin.name.clone()
                            }),
                    ),
            )
            .child(
                div()
                    .px_2()
                    .py_1()
                    .rounded(px(4.0))
                    .bg(badge_color.opacity(0.15))
                    .border_1()
                    .border_color(badge_color.opacity(0.4))
                    .text_xs()
                    .font_family("JetBrainsMono-Regular")
                    .text_color(badge_color)
                    .child(type_label),
            )
    }

    fn render_node_properties<T>(
        node: &BlueprintNode,
        cx: &mut Context<T>,
    ) -> impl IntoElement {
        v_flex().gap_3().children(
            node.properties
                .iter()
                .map(|(key, value)| Self::render_property_field(key, value, cx)),
        )
    }

    fn render_property_field<T>(
        key: &str,
        value: &str,
        cx: &mut Context<T>,
    ) -> impl IntoElement {
        v_flex()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .child(Self::format_property_name(key)),
            )
            .child(
                div()
                    .w_full()
                    .px_3()
                    .py_2p5()
                    .bg(cx.theme().input)
                    .border_1()
                    .border_color(cx.theme().border.opacity(0.6))
                    .rounded(px(6.0))
                    .text_sm()
                    .text_color(cx.theme().foreground)
                    .child(value.to_string())
                    .cursor_pointer()
                    .hover(|style| {
                        style
                            .border_color(cx.theme().accent.opacity(0.8))
                            .bg(cx.theme().input.lighten(0.02))
                    }),
            )
    }

    fn render_node_info<T>(
        node: &BlueprintNode,
        cx: &mut Context<T>,
    ) -> impl IntoElement {
        v_flex()
            .gap_2p5()
            .child(Self::render_info_row("Node ID", &node.id, cx))
            .child(Self::render_info_row(
                "Position",
                &format!("({:.0}, {:.0})", node.position.x, node.position.y),
                cx,
            ))
            .child(Self::render_info_row(
                "Size",
                &format!("{:.0} × {:.0} px", node.size.width, node.size.height),
                cx,
            ))
    }

    fn render_info_row<T>(
        label: &str,
        value: &str,
        cx: &mut Context<T>,
    ) -> impl IntoElement {
        h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .px_3()
            .py_2()
            .rounded(px(4.0))
            .hover(|style| style.bg(cx.theme().muted.opacity(0.1)))
            .child(
                div()
                    .text_xs()
                    .font_medium()
                    .text_color(cx.theme().muted_foreground)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .px_2()
                    .py_1()
                    .rounded(px(4.0))
                    .bg(cx.theme().muted.opacity(0.2))
                    .text_xs()
                    .font_family("JetBrainsMono-Regular")
                    .text_color(cx.theme().foreground)
                    .child(value.to_string()),
            )
    }

    fn format_property_name(key: &str) -> String {
        // Convert snake_case to Title Case
        key.split('_')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect::<Vec<String>>()
            .join(" ")
    }

}
