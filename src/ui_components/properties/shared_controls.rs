//! Shared property inspector cards, controls, and node details.

use super::PropertiesRenderer;
use crate::core::types::{BlueprintNode, NodeType, Pin};
use crate::editor::workspace_panels::GraphCanvasPanel;
use crate::features::connections::compatibility::is_pin_connected;
use gpui::prelude::FluentBuilder;
use gpui::*;
use std::any::Any;
use std::sync::Arc;
use ui::{h_flex, v_flex, ActiveTheme as _, Colorize, IconName, PixelsExt, Sizable, StyledExt};
use ui_common::reflected_properties_panel::rgba_to_hsla;

impl PropertiesRenderer {
    pub(super) fn render_multi_selection_state<T>(
        node_count: usize,
        comment_count: usize,
        cx: &mut Context<T>,
    ) -> AnyElement {
        let summary = match (node_count, comment_count) {
            (n, 0) => format!("{} nodes selected", n),
            (0, c) => format!("{} comments selected", c),
            (n, c) => format!("{} nodes, {} comments selected", n, c),
        };
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_2()
            .child(
                ui::Icon::new(IconName::Copy)
                    .size(px(20.0))
                    .text_color(cx.theme().muted_foreground.opacity(0.5)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(summary),
            )
            .into_any_element()
    }

    /// Card container matching the level-editor properties panel style.
    pub(super) fn render_card<T>(
        children: impl IntoIterator<Item: IntoElement>,
        cx: &mut Context<T>,
    ) -> impl IntoElement {
        v_flex()
            .w_full()
            .bg(cx.theme().sidebar)
            .rounded(px(8.0))
            .border_1()
            .border_color(cx.theme().border)
            .overflow_hidden()
            .children(children)
    }

    pub(super) fn render_section_header<T>(
        title: &str,
        _icon: IconName,
        cx: &mut Context<T>,
    ) -> Div {
        h_flex().items_center().gap_2().child(
            div()
                .text_xs()
                .font_bold()
                .text_color(cx.theme().muted_foreground)
                .child(title.to_uppercase()),
        )
    }

    pub(super) fn render_separator<T>(cx: &mut Context<T>) -> impl IntoElement {
        div().w_full().h_px().bg(cx.theme().border.opacity(0.3))
    }

    pub(super) fn get_node_type_color<T>(node_type: &NodeType, cx: &mut Context<T>) -> gpui::Hsla {
        match node_type {
            NodeType::Event => cx.theme().danger,
            NodeType::Logic => cx.theme().primary,
            NodeType::Math => cx.theme().success,
            NodeType::Object => cx.theme().warning,
            NodeType::Reroute => cx.theme().accent,
            NodeType::Conversion => cx.theme().primary,
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
            NodeType::SubGraphCall => gpui::Hsla {
                h: 0.75,
                s: 0.5,
                l: 0.5,
                a: 1.0,
            },
            NodeType::CustomEvent => gpui::Hsla {
                h: 0.08,
                s: 0.8,
                l: 0.5,
                a: 1.0,
            },
            NodeType::CustomEventDispatch => gpui::Hsla {
                h: 0.55,
                s: 0.8,
                l: 0.5,
                a: 1.0,
            },
        }
    }

    /// Compact, centered placeholder shown when there's nothing to inspect —
    /// matches the empty-details state of professional editors (Unreal/Unity).
    pub(super) fn render_empty_state<T>(cx: &mut Context<T>) -> AnyElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_2()
            .child(
                ui::Icon::new(IconName::Component)
                    .size(px(20.0))
                    .text_color(cx.theme().muted_foreground.opacity(0.5)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Select a node to view its properties"),
            )
            .into_any_element()
    }

    /// Render a pin list section — type display (badge color, resolved name)
    /// is sourced entirely from `PinDataType`/`RuntimeTypeInfo`, the same
    /// canonical reflection-backed lookup the graph view uses for pin colors,
    /// so the panel and graph always agree visually.
    pub(super) fn render_pin_list<T>(pins: &[Pin], cx: &mut Context<T>) -> impl IntoElement {
        v_flex()
            .gap_1p5()
            .children(pins.iter().map(|pin| Self::render_pin_row(pin, cx)))
    }

    pub(super) fn render_pin_editors(
        canvas: &mut GraphCanvasPanel,
        canvas_entity: &Entity<GraphCanvasPanel>,
        node: &BlueprintNode,
        window: &mut Window,
        cx: &mut Context<GraphCanvasPanel>,
    ) -> impl IntoElement {
        v_flex().gap_1p5().children(
            node.inputs.iter().map(|pin| {
                Self::render_input_pin_row(canvas, canvas_entity, node, pin, window, cx)
            }),
        )
    }

    pub(super) fn render_input_pin_row(
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
        let current_json = Self::read_pin_property_value(node, &pin.id);
        let current_any: Box<dyn Any> = if current_json.is_null() {
            Box::new(())
        } else {
            pulsar_reflection::RUNTIME_TYPE_REGISTRY
                .deserialize_json_for_type(type_info, current_json.clone())
                .unwrap_or_else(|_| Box::new(()))
        };

        let canvas_for_wb = canvas_entity.clone();
        let node_id_for_wb = node.id.clone();
        let pin_id_for_wb = pin.id.clone();
        let write_back = Arc::new(
            move |new_val: Box<dyn Any + Send>, _window: &mut Window, cx: &mut App| {
                if let Ok(json) = pulsar_reflection::RUNTIME_TYPE_REGISTRY
                    .serialize_json_for_any(new_val.as_ref())
                {
                    canvas_for_wb.update(cx, |canvas, cx| {
                        canvas.update_node_input_property(
                            &node_id_for_wb,
                            &pin_id_for_wb,
                            json,
                            cx,
                        );
                    });
                }
            },
        );

        let editor = ui_common::render_property_row_runtime(
            &mut canvas.pin_property_state,
            "node-input",
            &state_key,
            &Self::format_property_name(&pin.name),
            &pin.id,
            &pin.name,
            type_info,
            current_any.as_ref(),
            write_back,
            window,
            cx,
        );

        v_flex()
            .gap_1p5()
            .child(row)
            .child(editor)
            .into_any_element()
    }

    pub(super) fn read_pin_property_value(node: &BlueprintNode, pin_id: &str) -> serde_json::Value {
        let Some(raw_value) = node.properties.get(pin_id) else {
            return serde_json::Value::Null;
        };

        serde_json::from_str(raw_value)
            .unwrap_or_else(|_| serde_json::Value::String(raw_value.clone()))
    }

    pub(super) fn render_pin_row<T>(pin: &Pin, cx: &mut Context<T>) -> impl IntoElement {
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
                    .child(div().size(px(8.0)).rounded_full().bg(badge_color))
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

    pub(super) fn render_node_properties<T>(
        node: &BlueprintNode,
        cx: &mut Context<T>,
    ) -> impl IntoElement {
        v_flex().gap_3().children(
            node.properties
                .iter()
                .map(|(key, value)| Self::render_property_field(key, value, cx)),
        )
    }

    pub(super) fn render_property_field<T>(
        key: &str,
        value: &str,
        cx: &mut Context<T>,
    ) -> impl IntoElement {
        h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .child(
                div()
                    .w(px(80.0))
                    .flex_shrink_0()
                    .text_xs()
                    .font_medium()
                    .text_color(cx.theme().muted_foreground)
                    .child(Self::format_property_name(key)),
            )
            .child(
                div()
                    .flex_1()
                    .px_2()
                    .py_1p5()
                    .bg(cx.theme().input)
                    .border_1()
                    .border_color(cx.theme().border.opacity(0.6))
                    .rounded(px(4.0))
                    .text_xs()
                    .text_color(cx.theme().foreground)
                    .child(value.to_string())
                    .cursor_pointer()
                    .hover(|style| style.border_color(cx.theme().accent.opacity(0.5))),
            )
    }

    pub(super) fn render_node_info<T>(
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

    pub(super) fn render_info_row<T>(
        label: &str,
        value: &str,
        cx: &mut Context<T>,
    ) -> impl IntoElement {
        h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .child(
                div()
                    .w(px(80.0))
                    .flex_shrink_0()
                    .text_xs()
                    .font_medium()
                    .text_color(cx.theme().muted_foreground)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .flex_1()
                    .px_2()
                    .py_1p5()
                    .bg(cx.theme().input)
                    .border_1()
                    .border_color(cx.theme().border.opacity(0.6))
                    .rounded(px(4.0))
                    .text_xs()
                    .font_family("JetBrainsMono-Regular")
                    .text_color(cx.theme().foreground)
                    .child(value.to_string()),
            )
    }

    pub(super) fn format_property_name(key: &str) -> String {
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
