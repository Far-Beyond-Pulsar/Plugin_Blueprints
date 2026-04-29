//!
//! This module handles rendering of individual nodes including:
//! - Node boxes with headers and bodies
//! - Input/output pins
//! - Node icons and titles
//! - Selection highlighting
//! - Reroute nodes

//! Node rendering - visual representation of blueprint nodes
use gpui::prelude::*;
use gpui::*;
use ui::tooltip::Tooltip;
use ui::ActiveTheme;
use ui::PixelsExt;
use ui::Sizable;
use ui::StyledExt;
use ui::{h_flex, v_flex};

use crate::core::types::*;
use crate::editor::panel::BlueprintEditorPanel;
use crate::rendering::graph::NodeGraphRenderer;
use crate::rendering::{layout, style};
use ui::graph::DataType;

struct GraphCullBounds {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

fn compute_graph_cull_bounds(panel: &BlueprintEditorPanel) -> GraphCullBounds {
    let graph = &panel.graph;
    let viewport_size = panel
        .graph_element_bounds
        .map(|b| {
            (
                b.size.width.as_f32().max(1.0),
                b.size.height.as_f32().max(1.0),
            )
        })
        .unwrap_or((3840.0, 2160.0));

    let graph_origin = NodeGraphRenderer::screen_to_graph_pos(Point::new(px(0.0), px(0.0)), graph);
    let graph_end = NodeGraphRenderer::screen_to_graph_pos(
        Point::new(px(viewport_size.0), px(viewport_size.1)),
        graph,
    );

    // Keep generous padding to avoid edge popping while panning/zooming.
    let padding = (260.0 / graph.zoom_level.max(0.05)).max(120.0);

    GraphCullBounds {
        left: graph_origin.x.min(graph_end.x) - padding,
        top: graph_origin.y.min(graph_end.y) - padding,
        right: graph_origin.x.max(graph_end.x) + padding,
        bottom: graph_origin.y.max(graph_end.y) + padding,
    }
}

fn is_node_visible_in_bounds(node: &BlueprintNode, bounds: &GraphCullBounds) -> bool {
    let node_left = node.position.x;
    let node_top = node.position.y;
    let node_right = node.position.x + node.size.width; 
    let node_bottom = node.position.y + node.size.height;

    !(node_left > bounds.right
        || node_right < bounds.left
        || node_top > bounds.bottom
        || node_bottom < bounds.top)
}

/// Render all nodes in the graph with virtualization
pub fn render_all(
    panel: &mut BlueprintEditorPanel,
    cx: &mut Context<BlueprintEditorPanel>,
) -> impl IntoElement {
    let _render_start = std::time::Instant::now();
    let cull_bounds = compute_graph_cull_bounds(panel);

    // Only render nodes that are visible within the viewport (virtualization)
    let visible_nodes: Vec<BlueprintNode> = panel
        .graph
        .nodes
        .iter()
        .filter(|node| is_node_visible_in_bounds(node, &cull_bounds))
        .map(|node| {
            let mut node = node.clone();
            node.is_selected = panel.graph.selected_nodes.contains(&node.id);
            node
        })
        .collect();

    // Debug info for virtualization
    if cfg!(debug_assertions) && panel.graph.nodes.len() != visible_nodes.len() {
        tracing::info!(
            "[BLUEPRINT-VIRTUALIZATION] Rendering {} of {} nodes (saved {:.1}%)",
            visible_nodes.len(),
            panel.graph.nodes.len(),
            (1.0 - visible_nodes.len() as f32 / panel.graph.nodes.len() as f32) * 100.0
        );
    }

    div().absolute().inset_0().children(
        visible_nodes
            .into_iter()
            .map(|node| render_blueprint_node(&node, panel, cx)),
    )
}

/// Render a single blueprint node
pub fn render_node(
    node: &BlueprintNode,
    panel: &mut BlueprintEditorPanel,
    cx: &mut Context<BlueprintEditorPanel>,
) -> impl IntoElement {
    render_blueprint_node(node, panel, cx)
}

fn render_blueprint_node(
    node: &BlueprintNode,
    panel: &mut BlueprintEditorPanel,
    cx: &mut Context<BlueprintEditorPanel>,
) -> AnyElement {
    if node.node_type == NodeType::Reroute {
        return render_reroute_node(node, panel, cx);
    }

    // Category color
    let ue_node_color = |node_type: &NodeType| match node_type {
        NodeType::Event => gpui::Hsla {
            h: 0.00,
            s: 0.82,
            l: 0.38,
            a: 1.0,
        },
        NodeType::Logic => gpui::Hsla {
            h: 0.61,
            s: 0.78,
            l: 0.40,
            a: 1.0,
        },
        NodeType::Math => gpui::Hsla {
            h: 0.42,
            s: 0.68,
            l: 0.36,
            a: 1.0,
        },
        NodeType::Object => gpui::Hsla {
            h: 0.10,
            s: 0.72,
            l: 0.38,
            a: 1.0,
        },
        NodeType::Reroute => gpui::Hsla {
            h: 0.00,
            s: 0.00,
            l: 0.40,
            a: 1.0,
        },
        NodeType::MacroEntry => gpui::Hsla {
            h: 0.76,
            s: 0.62,
            l: 0.36,
            a: 1.0,
        },
        NodeType::MacroExit => gpui::Hsla {
            h: 0.76,
            s: 0.62,
            l: 0.36,
            a: 1.0,
        },
        NodeType::MacroInstance => gpui::Hsla {
            h: 0.76,
            s: 0.50,
            l: 0.28,
            a: 1.0,
        },
    };
    let node_color = if let Some(ref hex) = node.color {
        parse_hex_color(hex).unwrap_or_else(|| ue_node_color(&node.node_type))
    } else {
        ue_node_color(&node.node_type)
    };

    // Geometry
    let z = panel.graph.zoom_level;
    let screen = NodeGraphRenderer::graph_to_screen_pos(node.position, &panel.graph);
    let node_id = node.id.clone();
    let is_dragging = panel.dragging_node.as_ref() == Some(&node.id);
    // Width and height both snapped to the grid interval so nodes align with
    // grid snapping regardless of zoom level.
    let scaled_width = layout::snap_to_grid(node.size.width) * z;
    let max_rows = node.inputs.len().max(node.outputs.len()).max(1);
    let scaled_height = layout::snap_to_grid(layout::node_height_for_pin_rows(max_rows)) * z;

    // Style
    let body_bg = style::body_bg();
    let title_bg = style::title_bg(node_color);
    let border_color = if node.is_selected {
        style::selected_border(node_color)
    } else {
        style::idle_border()
    };
    let corner_r = style::corner_radius(z);

    // Layout constants — MUST match calculate_pin_position.
    const HEADER_H: f32 = layout::HEADER_H;
    const SEP_H: f32 = layout::SEP_H;

    // Node card
    v_flex()
        .absolute()
        .left(px(screen.x))
        .top(px(screen.y))
        .w(px(scaled_width))
        .h(px(scaled_height))
        .bg(body_bg)
        .rounded(corner_r)
        .overflow_hidden()
        .border_color(border_color)
        .when(node.is_selected, |s| s.border_2().shadow_2xl())
        .when(!node.is_selected, |s| s.border_1().shadow_md())
        .when(is_dragging, |s| s.opacity(0.92))
        .cursor_pointer()
        // Header
        .child(
            div()
                .w_full()
                .h(px(HEADER_H * z))
                .relative()
                .overflow_hidden()
                .corner_radii(gpui::Corners {
                    top_left: corner_r,
                    top_right: corner_r,
                    bottom_right: px(0.0),
                    bottom_left: px(0.0),
                })
                .bg(title_bg)
                .child(
                    h_flex()
                        .w_full()
                        .h_full()
                        .px(px(10.0 * z))
                        .items_center()
                        .gap(px(6.0 * z))
                        .id(ElementId::Name(format!("node-header-{}", node.id).into()))
                        .child(
                            div()
                                .text_size(px(12.0 * z))
                                .text_color(gpui::Hsla {
                                    h: 0.0,
                                    s: 0.0,
                                    l: 1.0,
                                    a: 0.85,
                                })
                                .child(node.icon.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(13.0 * z))
                                .font_semibold()
                                .text_color(gpui::white())
                                .child(node.title.clone()),
                        )
                        .when(node.definition_id.starts_with("subgraph:"), |s| {
                            s.child(
                                div()
                                    .px(px(4.0 * z))
                                    .py(px(1.0 * z))
                                    .rounded(px(3.0 * z))
                                    .bg(gpui::Rgba {
                                        r: 0.55,
                                        g: 0.30,
                                        b: 0.70,
                                        a: 0.45,
                                    })
                                    .border_1()
                                    .border_color(gpui::Rgba {
                                        r: 0.70,
                                        g: 0.50,
                                        b: 0.85,
                                        a: 0.75,
                                    })
                                    .text_size(px(9.0 * z))
                                    .text_color(gpui::Rgba {
                                        r: 0.90,
                                        g: 0.80,
                                        b: 1.0,
                                        a: 1.0,
                                    })
                                    .child("MACRO"),
                            )
                        })
                        .on_mouse_down(gpui::MouseButton::Left, {
                            let node_id = node_id.clone();
                            let node_definition_id = node.definition_id.clone();
                            let node_title = node.title.clone();
                            cx.listener(move |panel, event: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                panel.focus_handle().focus(window);

                                let now = std::time::Instant::now();
                                let is_subgraph = node_definition_id.starts_with("subgraph:");
                                let should_open_subgraph = is_subgraph && {
                                    if let (Some(last_t), Some(last_p)) =
                                        (panel.last_click_time, panel.last_click_pos)
                                    {
                                        if now.duration_since(last_t).as_millis() < 500 {
                                            let ep = NodeGraphRenderer::window_to_graph_element_pos(
                                                event.position,
                                                panel,
                                            );
                                            let cp = Point::new(ep.x.as_f32(), ep.y.as_f32());
                                            ((cp.x - last_p.x).powi(2) + (cp.y - last_p.y).powi(2))
                                                .sqrt()
                                                < 10.0
                                        } else {
                                            false
                                        }
                                    } else {
                                        false
                                    }
                                };

                                if should_open_subgraph {
                                    let subgraph_id = node_definition_id
                                        .strip_prefix("subgraph:")
                                        .unwrap_or(&node_definition_id)
                                        .to_string();
                                    if let Some(library_id) =
                                        panel.get_macro_library_id(&subgraph_id)
                                    {
                                        let library_name = panel
                                            .library_manager
                                            .get_libraries()
                                            .get(&library_id)
                                            .map(|lib| lib.name.clone())
                                            .unwrap_or_else(|| library_id.clone());
                                        panel.request_open_engine_library(
                                            library_id,
                                            library_name,
                                            Some(subgraph_id.clone()),
                                            Some(node_title.clone()),
                                            cx,
                                        );
                                    } else if let Some(m) =
                                        panel.local_macros.iter().find(|m| m.id == subgraph_id)
                                    {
                                        panel.open_local_macro(
                                            subgraph_id.clone(),
                                            m.name.clone(),
                                            window,
                                            cx,
                                        );
                                    } else {
                                        tracing::info!("Macro '{}' not found", node_title);
                                    }
                                    panel.last_click_time = None;
                                    panel.last_click_pos = None;
                                } else {
                                    if !panel.graph.selected_nodes.contains(&node_id) {
                                        panel.select_node(Some(node_id.clone()), cx);
                                    }
                                    let ep = NodeGraphRenderer::window_to_graph_element_pos(
                                        event.position,
                                        panel,
                                    );
                                    let gp =
                                        NodeGraphRenderer::screen_to_graph_pos(ep, &panel.graph);
                                    panel.start_drag(node_id.clone(), gp, cx);
                                    panel.last_click_time = Some(now);
                                    panel.last_click_pos =
                                        Some(Point::new(ep.x.as_f32(), ep.y.as_f32()));
                                }
                            })
                        }),
                ),
        )
        // Accent separator — thin bright line derived from node color
        .child(
            div()
                .w_full()
                .h(px(SEP_H * z))
                .bg(style::accent_separator(node_color)),
        )
        // Pin body
        .child(render_node_pins(node, z, panel, cx))
        // Body mouse handler (select on click)
        .on_mouse_down(gpui::MouseButton::Left, {
            let node_id = node_id.clone();
            cx.listener(move |panel, _event: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                panel.focus_handle().focus(window);
                if !panel.graph.selected_nodes.contains(&node_id) {
                    panel.select_node(Some(node_id.clone()), cx);
                }
            })
        })
        .into_any_element()
}

fn render_reroute_node(
    node: &BlueprintNode,
    panel: &mut BlueprintEditorPanel,
    cx: &mut Context<BlueprintEditorPanel>,
) -> AnyElement {
    let graph_pos = NodeGraphRenderer::graph_to_screen_pos(node.position, &panel.graph);
    let node_id = node.id.clone();
    let is_dragging = panel.dragging_node.as_ref() == Some(&node.id);

    // Get the color from the pin data type (reroute nodes have one input and one output of the same type)
    let pin_color = if let Some(input_pin) = node.inputs.first() {
        get_pin_color(&input_pin.data_type, cx)
    } else if let Some(output_pin) = node.outputs.first() {
        get_pin_color(&output_pin.data_type, cx)
    } else {
        cx.theme().accent
    };

    // Reroute node is rendered as a thick colored dot
    let dot_size = 16.0 * panel.graph.zoom_level;
    let clickable_size = 24.0 * panel.graph.zoom_level; // Larger clickable area

    div()
        .absolute()
        .left(px(graph_pos.x - clickable_size / 2.0)) // Center the clickable area
        .top(px(graph_pos.y - clickable_size / 2.0))
        .w(px(clickable_size))
        .h(px(clickable_size))
        .cursor_pointer()
        .on_mouse_down(gpui::MouseButton::Left, {
            let node_id = node_id.clone();
            cx.listener(move |panel, event: &MouseDownEvent, window, cx| {
                // Stop event propagation
                cx.stop_propagation();

                // Ensure graph has focus for keyboard events
                panel.focus_handle().focus(window);

                // Only change selection if this node is not already selected
                if !panel.graph.selected_nodes.contains(&node_id) {
                    panel.select_node(Some(node_id.clone()), cx);
                }

                // Start dragging
                // Convert to element coordinates first
                let element_pos =
                    NodeGraphRenderer::window_to_graph_element_pos(event.position, panel);
                let graph_pos = NodeGraphRenderer::screen_to_graph_pos(element_pos, &panel.graph);
                panel.start_drag(node_id.clone(), graph_pos, cx);
            })
        })
        .child(
            // The visible dot - refined with dark outline
            div()
                .absolute()
                .left(px((clickable_size - dot_size) / 2.0))
                .top(px((clickable_size - dot_size) / 2.0))
                .w(px(dot_size))
                .h(px(dot_size))
                .bg(pin_color)
                .rounded_full()
                .border_2()
                .border_color(if node.is_selected {
                    gpui::Hsla {
                        h: pin_color.h,
                        s: 0.9,
                        l: 0.7,
                        a: 1.0,
                    }
                } else {
                    gpui::Hsla {
                        h: 0.0,
                        s: 0.0,
                        l: 0.15,
                        a: 0.9,
                    }
                })
                .when(is_dragging, |style| style.opacity(0.9).shadow_2xl())
                .shadow_md(),
        )
        // Invisible pins for connections - positioned at the center
        .children(
            node.inputs
                .iter()
                .map(|input_pin| render_reroute_pin(input_pin, true, &node.id, panel, cx)),
        )
        .children(
            node.outputs
                .iter()
                .map(|output_pin| render_reroute_pin(output_pin, false, &node.id, panel, cx)),
        )
        .into_any_element()
}

fn render_reroute_pin(
    pin: &Pin,
    is_input: bool,
    node_id: &str,
    panel: &BlueprintEditorPanel,
    cx: &mut Context<BlueprintEditorPanel>,
) -> impl IntoElement {
    let node_id_clone = node_id.to_string();
    let pin_id = pin.id.clone();

    // Check if this pin is compatible with the current drag
    let is_compatible = if let Some(ref drag) = panel.dragging_connection {
        is_input
            && node_id != drag.source_node
            && pin.data_type.is_compatible_with(&drag.source_pin_type)
    } else {
        false
    };

    // Invisible pin area at the center of the dot for connections
    div()
        .absolute()
        .left_1_2()
        .top_1_2()
        .w(px(8.0))
        .h(px(8.0))
        .ml(px(-4.0)) // Center it
        .mt(px(-4.0))
        // Make it visible when compatible
        .when(is_compatible, |style| {
            style.bg(gpui::white().opacity(0.3)).rounded_full()
        })
        .cursor_pointer()
        .on_mouse_down(gpui::MouseButton::Left, {
            let node_id = node_id_clone.clone();
            let pin_id = pin_id.clone();
            cx.listener(move |panel, event: &MouseDownEvent, _window, cx| {
                cx.stop_propagation();

                if is_input {
                    // Clicking input pin - do nothing for now
                } else {
                    // Clicking output pin - start connection drag
                    let graph_pos =
                        NodeGraphRenderer::screen_to_graph_pos(event.position, &panel.graph);
                    panel.start_connection_drag_from_pin(
                        node_id.clone(),
                        pin_id.clone(),
                        graph_pos,
                        cx,
                    );
                }
            })
        })
        .on_mouse_up(gpui::MouseButton::Left, {
            let node_id = node_id_clone.clone();
            let pin_id = pin_id.clone();
            cx.listener(move |panel, _event: &MouseUpEvent, _window, cx| {
                if is_input && panel.dragging_connection.is_some() {
                    panel.complete_connection_on_pin(node_id.clone(), pin_id.clone(), cx);
                }
            })
        })
}

/// Renders all pin rows for a node body.
/// Layout constants must stay in sync with `calculate_pin_position`.
fn render_node_pins(
    node: &BlueprintNode,
    z: f32,
    panel: &BlueprintEditorPanel,
    cx: &mut Context<BlueprintEditorPanel>,
) -> impl IntoElement {
    use crate::rendering::layout as L;

    let label_color = style::label_color();
    let corner_r = style::corner_radius(z);
    let max_rows = node.inputs.len().max(node.outputs.len());

    div()
        .w_full()
        .bg(style::body_bg())
        .corner_radii(gpui::Corners {
            top_left: px(0.0),
            top_right: px(0.0),
            bottom_right: corner_r,
            bottom_left: corner_r,
        })
        .px(px(L::BODY_PAD * z))
        .pt(px(L::BODY_PAD * z))
        .pb(px(L::BODY_PAD * z))
        .flex()
        .flex_col()
        .gap(px(L::PIN_GAP * z))
        .children((0..max_rows).map(|i| {
            div()
                .w_full()
                .h(px(L::PIN_ROW_H * z))
                .flex()
                .items_center()
                // Left: input pin + label
                .child({
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0 * z))
                        .child(if let Some(pin) = node.inputs.get(i) {
                            render_pin(pin, true, &node.id, panel, cx).into_any_element()
                        } else {
                            div()
                                .w(px(L::PIN_SIZE * z))
                                .h(px(L::PIN_SIZE * z))
                                .into_any_element()
                        })
                        .child(if let Some(pin) = node.inputs.get(i) {
                            if !pin.name.is_empty() {
                                div()
                                    .text_size(px(11.0 * z))
                                    .text_color(label_color)
                                    .child(pin.name.clone())
                                    .into_any_element()
                            } else {
                                div().into_any_element()
                            }
                        } else {
                            div().into_any_element()
                        })
                })
                // Centre spacer
                .child(div().flex_1())
                // Right: label + output pin
                .child({
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0 * z))
                        .child(if let Some(pin) = node.outputs.get(i) {
                            if !pin.name.is_empty() {
                                div()
                                    .text_size(px(11.0 * z))
                                    .text_color(label_color)
                                    .child(pin.name.clone())
                                    .into_any_element()
                            } else {
                                div().into_any_element()
                            }
                        } else {
                            div().into_any_element()
                        })
                        .child(if let Some(pin) = node.outputs.get(i) {
                            render_pin(pin, false, &node.id, panel, cx).into_any_element()
                        } else {
                            div()
                                .w(px(L::PIN_SIZE * z))
                                .h(px(L::PIN_SIZE * z))
                                .into_any_element()
                        })
                })
        }))
}

fn render_pin(
    pin: &Pin,
    is_input: bool,
    node_id: &str,
    panel: &BlueprintEditorPanel,
    cx: &mut Context<BlueprintEditorPanel>,
) -> impl IntoElement {
    let pin_style = pin.data_type.generate_pin_style();
    let pin_color = gpui::Hsla::from(gpui::Rgba {
        r: pin_style.color.r,
        g: pin_style.color.g,
        b: pin_style.color.b,
        a: pin_style.color.a,
    });

    let is_compatible = if let Some(ref drag) = panel.dragging_connection {
        is_input
            && node_id != drag.source_node
            && pin.data_type.is_compatible_with(&drag.source_pin_type)
    } else {
        false
    };

    let is_exec = pin.data_type == DataType::Execution;
    let z = panel.graph.zoom_level;
    let sz = layout::PIN_SIZE * z;

    let tooltip_text = if pin.data_type == DataType::Execution {
        "Execution Pin".to_string()
    } else {
        pin.data_type.rust_type_string()
    };
    let element_id = format!("pin-{}-{}", node_id, pin.id);

    let accent = cx.theme().accent;

    div()
        .id(ElementId::Name(element_id.into()))
        .on_hover(cx.listener(move |panel, hovered: &bool, window, cx| {
            if *hovered {
                panel.hovered_pin_tooltip = Some(tooltip_text.clone());
                panel.hovered_pin_tooltip_pos = Some(window.mouse_position());
            } else {
                panel.hovered_pin_tooltip = None;
                panel.hovered_pin_tooltip_pos = None;
            }
            cx.notify();
        }))
        .w(px(sz))
        .h(px(sz))
        .relative()
        .cursor_pointer()
        .when(is_exec, |s| {
            // Execution pin: canvas-drawn |> arrow shape
            let exec_fill = if is_compatible {
                accent
            } else {
                gpui::Hsla {
                    h: 0.0,
                    s: 0.0,
                    l: 0.88,
                    a: 1.0,
                }
            };
            let exec_border = if is_compatible {
                accent
            } else {
                gpui::Hsla {
                    h: 0.0,
                    s: 0.0,
                    l: 0.50,
                    a: 0.9,
                }
            };
            s.bg(gpui::transparent_black())
                .child(paint_exec_pin(sz, exec_fill, exec_border))
        })
        .when(!is_exec, |s| {
            // Data pin: filled circle
            let fill = if is_compatible { accent } else { pin_color };
            let border = if is_compatible {
                accent
            } else {
                gpui::Hsla {
                    h: 0.0,
                    s: 0.0,
                    l: 0.25,
                    a: 0.9,
                }
            };
            s.bg(fill)
                .rounded_full()
                .border_1()
                .border_color(border)
                .when(is_compatible, |s2| s2.border_2().shadow_lg())
        })
        .when(!is_input, |div| {
            let pin_id = pin.id.clone();
            let node_id = node_id.to_string();
            div.on_mouse_down(gpui::MouseButton::Left, {
                cx.listener(move |panel, event: &MouseDownEvent, _window, cx| {
                    cx.stop_propagation();
                    let graph_pos =
                        NodeGraphRenderer::screen_to_graph_pos(event.position, &panel.graph);
                    panel.start_connection_drag_from_pin(
                        node_id.clone(),
                        pin_id.clone(),
                        graph_pos,
                        cx,
                    );
                })
            })
        })
        .when(is_input && panel.dragging_connection.is_some(), |div| {
            let pin_id = pin.id.clone();
            let node_id = node_id.to_string();
            let _pin_type = pin.data_type.clone();
            div.on_mouse_up(gpui::MouseButton::Left, {
                cx.listener(move |panel, _event: &MouseUpEvent, _window, cx| {
                    cx.stop_propagation();
                    panel.complete_connection_on_pin(node_id.clone(), pin_id.clone(), cx);
                })
            })
        })
        .into_any_element()
}

/// UE-style execution pin:  |>   (flat left wall + triangle pointing right)
///
/// ```text
///   (0,0)────────(body,0)
///     |                  \
///     |                   (w, h/2)
///     |                  /
///   (0,h)────────(body,h)
/// ```
fn paint_exec_pin(sz: f32, fill: gpui::Hsla, border: gpui::Hsla) -> impl IntoElement {
    gpui::canvas(
        move |_bounds, _window, _cx| {},
        move |bounds, _prepaint, window, _cx| {
            let ox = bounds.origin.x.as_f32();
            let oy = bounds.origin.y.as_f32();
            let w = sz;
            let h = sz;
            // The flat "body" portion is ~55% of width, then triangle tip
            let body = w * 0.50;

            // Outline (paint first, slightly expanded)
            let b = 1.2_f32;
            {
                let mut p = gpui::PathBuilder::fill();
                p.move_to(gpui::point(gpui::px(ox - b), gpui::px(oy - b)));
                p.line_to(gpui::point(gpui::px(ox + body), gpui::px(oy - b)));
                p.line_to(gpui::point(gpui::px(ox + w + b), gpui::px(oy + h / 2.0)));
                p.line_to(gpui::point(gpui::px(ox + body), gpui::px(oy + h + b)));
                p.line_to(gpui::point(gpui::px(ox - b), gpui::px(oy + h + b)));
                p.close();
                if let Ok(path) = p.build() {
                    window.paint_path(path, border);
                }
            }
            // Fill
            {
                let mut p = gpui::PathBuilder::fill();
                p.move_to(gpui::point(gpui::px(ox), gpui::px(oy)));
                p.line_to(gpui::point(gpui::px(ox + body), gpui::px(oy)));
                p.line_to(gpui::point(gpui::px(ox + w), gpui::px(oy + h / 2.0)));
                p.line_to(gpui::point(gpui::px(ox + body), gpui::px(oy + h)));
                p.line_to(gpui::point(gpui::px(ox), gpui::px(oy + h)));
                p.close();
                if let Ok(path) = p.build() {
                    window.paint_path(path, fill);
                }
            }
        },
    )
    .absolute()
    .inset_0()
    .size_full()
}

// Helper functions

fn get_pin_color(data_type: &DataType, _cx: &mut Context<BlueprintEditorPanel>) -> gpui::Hsla {
    // Use the new type system to generate pin colors
    let pin_style = data_type.generate_pin_style();
    // Convert RGB to HSLA using the proper GPUI color API
    let rgba = gpui::Rgba {
        r: pin_style.color.r,
        g: pin_style.color.g,
        b: pin_style.color.b,
        a: pin_style.color.a,
    };
    gpui::Hsla::from(rgba)
}

/// Parses a hex color string (e.g., "#4A90E2") into a GPUI Hsla color
fn parse_hex_color(hex: &str) -> Option<gpui::Hsla> {
    let hex = hex.trim_start_matches('#');

    // Parse RGB values
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;

        let rgba = gpui::Rgba { r, g, b, a: 1.0 };
        Some(gpui::Hsla::from(rgba))
    } else if hex.len() == 8 {
        // Support RGBA format as well
        let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
        let a = u8::from_str_radix(&hex[6..8], 16).ok()? as f32 / 255.0;

        let rgba = gpui::Rgba { r, g, b, a };
        Some(gpui::Hsla::from(rgba))
    } else {
        None
    }
}
