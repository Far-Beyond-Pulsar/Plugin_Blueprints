//!
//! This module contains overlay elements that render on top of the main graph:
//! - Selection box during drag-select operations
//! - Debug overlay showing viewport metrics and virtualization stats
//! - Viewport bounds visualization for development
//! - Graph controls (zoom level, etc.)

//! Overlay rendering - debug info, selection box, viewport bounds
use super::graph::NodeGraphRenderer;
use crate::core::spatial_index::GraphRect;
use crate::editor::workspace_panels::GraphCanvasPanel;
use gpui::*;
use ui::{
    button::{Button, ButtonVariants},
    h_flex, v_flex, ActiveTheme, IconName, PixelsExt, Sizable, StyledExt,
};

/// Render the selection box during drag-select
pub fn render_selection_box(
    panel: &crate::editor::workspace_panels::GraphCanvasPanel,
    _view_id: &str,
    cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
) -> impl IntoElement {
    if let (Some(start), Some(end)) = (panel.selection_start, panel.selection_end) {
        // Convert selection bounds to screen coordinates
        let start_screen = NodeGraphRenderer::graph_to_screen_pos(start, &panel.graph);
        let end_screen = NodeGraphRenderer::graph_to_screen_pos(end, &panel.graph);

        let left = start_screen.x.min(end_screen.x);
        let top = start_screen.y.min(end_screen.y);
        let width = (end_screen.x - start_screen.x).abs();
        let height = (end_screen.y - start_screen.y).abs();

        div()
            .absolute()
            .inset_0()
            .child(
                div()
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .w(px(width))
                    .h(px(height))
                    .border_1()
                    .border_color(gpui::Hsla {
                        h: 0.58,
                        s: 0.7,
                        l: 0.6,
                        a: 0.7,
                    })
                    .bg(gpui::Hsla {
                        h: 0.58,
                        s: 0.5,
                        l: 0.5,
                        a: 0.08,
                    })
                    .rounded(px(3.0)),
            )
            .into_any_element()
    } else {
        div().into_any_element()
    }
}

/// Render viewport bounds debug visualization (yellow border showing culling frustum)
pub fn render_viewport_bounds_debug(
    panel: &crate::editor::workspace_panels::GraphCanvasPanel,
    cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
) -> impl IntoElement {
    if !cfg!(debug_assertions) {
        return div().into_any_element();
    }

    let Some(bounds) = panel.element_bounds else {
        return div().into_any_element();
    };
    let viewport_size = Size::new(bounds.size.width.as_f32(), bounds.size.height.as_f32());
    let (visible_left, visible_top, visible_right, visible_bottom) =
        NodeGraphRenderer::viewport_graph_bounds(&panel.graph, viewport_size, 260.0);

    // Convert back to screen coordinates for rendering
    let top_left_screen =
        NodeGraphRenderer::graph_to_screen_pos(Point::new(visible_left, visible_top), &panel.graph);
    let bottom_right_screen = NodeGraphRenderer::graph_to_screen_pos(
        Point::new(visible_right, visible_bottom),
        &panel.graph,
    );

    let width = bottom_right_screen.x - top_left_screen.x;
    let height = bottom_right_screen.y - top_left_screen.y;

    div()
        .absolute()
        .inset_0()
        .child(
            div()
                .absolute()
                .left(px(top_left_screen.x))
                .top(px(top_left_screen.y))
                .w(px(width))
                .h(px(height))
                .border_2()
                .border_color(gpui::yellow()), // Debug overlay - shows viewport bounds for culling
        )
        .into_any_element()
}

/// Render debug overlay showing viewport metrics and virtualization stats
pub fn render_debug_overlay(
    panel: &crate::editor::workspace_panels::GraphCanvasPanel,
    cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
) -> impl IntoElement {
    // Calculate all the viewport metrics
    let Some(bounds) = panel.element_bounds else {
        return div().into_any_element();
    };
    let viewport_size = Size::new(bounds.size.width.as_f32(), bounds.size.height.as_f32());
    let (visible_left, visible_top, visible_right, visible_bottom) =
        NodeGraphRenderer::viewport_graph_bounds(&panel.graph, viewport_size, 260.0);
    let padding_in_graph_space = -panel.graph.pan_offset.x - visible_left;

    // Calculate viewport dimensions
    let viewport_width = visible_right - visible_left;
    let viewport_height = visible_bottom - visible_top;

    // Count visible vs culled nodes and connections
    let (visible_node_count, visible_connection_count) = {
        let mut spatial = panel.spatial_index.borrow_mut();
        spatial.ensure_current(&panel.graph);
        let visible_rect = GraphRect {
            min_x: visible_left,
            min_y: visible_top,
            max_x: visible_right,
            max_y: visible_bottom,
        };
        (
            spatial.nodes_intersecting(visible_rect, false).len(),
            spatial.wires_intersecting(visible_rect).len(),
        )
    };
    let culled_node_count = panel.graph.nodes.len() - visible_node_count;
    let culled_connection_count = panel.graph.connections.len() - visible_connection_count;

    let container_width = viewport_size.width;
    let container_height = viewport_size.height;

    div()
        .absolute()
        .top_4()
        .left_4()
        .w(px(280.0))
        .child(
            div()
                .w(px(280.0))
                .p_3()
                .bg(cx.theme().background.opacity(0.95))
                .rounded(cx.theme().radius)
                .border_1()
                .border_color(cx.theme().border)
                .shadow_lg()
                .child(
                    v_flex()
                        .gap_1()
                        .child(
                            h_flex()
                                .w_full()
                                .justify_between()
                                .items_center()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_bold()
                                        .text_color(cx.theme().foreground)
                                        .child("Blueprint Viewport Debug"),
                                )
                                .child(
                                    Button::new("close_debug_overlay")
                                        .icon(IconName::Close)
                                        .ghost()
                                        .xsmall()
                                        .on_click(cx.listener(|panel, _, _, cx| {
                                            panel.show_debug_overlay = false;
                                            cx.notify();
                                        })),
                                ),
                        )
                        .child(div().h(px(1.0)).bg(cx.theme().border).my_1())
                        .child(div().text_xs().text_color(cx.theme().info).child(format!(
                            "Container: {:.0}×{:.0}px",
                            container_width, container_height
                        )))
                        .child(div().text_xs().text_color(cx.theme().info).child(format!(
                            "Render Bounds: {:.0}×{:.0}",
                            viewport_width, viewport_height
                        )))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!(
                                    "Origin: ({:.0}, {:.0})",
                                    visible_left, visible_top
                                )),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!(
                                    "End: ({:.0}, {:.0})",
                                    visible_right, visible_bottom
                                )),
                        )
                        .child(div().h(px(1.0)).bg(cx.theme().border).my_1())
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().success)
                                .child(format!("Nodes Rendered: {}", visible_node_count)),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().danger)
                                .child(format!("Nodes Culled: {}", culled_node_count)),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!("Total Nodes: {}", panel.graph.nodes.len())),
                        )
                        .child(div().h(px(1.0)).bg(cx.theme().border).my_1())
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().success)
                                .child(format!(
                                    "Connections Rendered: {}",
                                    visible_connection_count
                                )),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().danger)
                                .child(format!("Connections Culled: {}", culled_connection_count)),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!(
                                    "Total Connections: {}",
                                    panel.graph.connections.len()
                                )),
                        )
                        .child(div().h(px(1.0)).bg(cx.theme().border).my_1())
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().warning)
                                .child(format!("Zoom: {:.2}x", panel.graph.zoom_level)),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().warning)
                                .child(format!(
                                    "Pan: ({:.0}, {:.0})",
                                    panel.graph.pan_offset.x, panel.graph.pan_offset.y
                                )),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().warning)
                                .child(format!("Padding: {:.0}", padding_in_graph_space)),
                        ),
                ),
        )
        .into_any_element()
}

/// Render graph controls (zoom level, fit button, etc.)
pub fn render_graph_controls(
    panel: &crate::editor::workspace_panels::GraphCanvasPanel,
    cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
) -> impl IntoElement {
    div().absolute().bottom_4().right_4().w(px(280.0)).child(
        v_flex().gap_2().items_end().w(px(280.0)).child(
            h_flex()
                .gap_2()
                .p_2()
                .w_full()
                .bg(cx.theme().background.opacity(0.9))
                .rounded(cx.theme().radius)
                .border_1()
                .border_color(cx.theme().border)
                .justify_between()
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("Zoom: {:.0}%", panel.graph.zoom_level * 100.0)),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("zoom_fit")
                                .icon(IconName::BadgeCheck)
                                .tooltip("Fit to View")
                                .on_click(cx.listener(|panel, _, _window, cx| {
                                    let graph = &mut panel.graph;
                                    graph.zoom_level = 1.0;
                                    graph.pan_offset = Point::new(0.0, 0.0);
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("close_graph_controls")
                                .icon(IconName::Close)
                                .ghost()
                                .xsmall()
                                .on_click(cx.listener(|panel, _, _, cx| {
                                    panel.show_graph_controls = false;
                                    cx.notify();
                                })),
                        ),
                ),
        ),
    )
}
