//!
//! This module is responsible for:
//! - Main render() method that composes all feature renderers
//! - Grid background rendering
//! - Coordinate conversion utilities
//! - Viewport culling/virtualization helpers
//! Main graph canvas renderer - orchestrates all rendering features
use gpui::prelude::FluentBuilder;
use gpui::prelude::*;
use gpui::*;
use ui::{
    button::{Button, ButtonVariants},
    h_flex,
    v_flex, ActiveTheme, Colorize, IconName, PixelsExt, Sizable, StyledExt,
};
use crate::editor::panel::BlueprintEditorPanel;
use crate::rendering::{layout, style};
use crate::{BlueprintGraph, BlueprintNode, Connection, NodeType, Pin};
use ui::graph::DataType;

pub struct NodeGraphRenderer;

fn render_pin_hover_tooltip(
    panel: &BlueprintEditorPanel,
    view_id: &str,
    cx: &mut Context<BlueprintEditorPanel>,
) -> impl IntoElement {
    if let Some(text) = panel.hovered_pin_tooltip.as_ref() {
        if let Some(position) = panel.hovered_pin_tooltip_pos {
            let element_pos =
                NodeGraphRenderer::window_to_graph_element_pos_for_view(position, panel, view_id);
            return div()
                .absolute()
                .left(element_pos.x + px(10.0))
                .top(element_pos.y + px(10.0))
                .bg(cx.theme().popover)
                .text_color(cx.theme().popover_foreground)
                .border_1()
                .border_color(cx.theme().border)
                .shadow_md()
                .rounded(px(6.0))
                .py(px(4.0))
                .px(px(8.0))
                .text_sm()
                .child(text.clone())
                .into_any_element();
        }
    }
    div().into_any_element()
}

impl NodeGraphRenderer {
    /// Main render method that orchestrates all graph rendering
    pub fn render(
        panel: &mut BlueprintEditorPanel,
        view_id: &str,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        let focus_handle = panel.focus_handle().clone();
        let graph_id = "blueprint-graph";
        let panel_entity = cx.entity().clone();

        let view_id = view_id.to_string();

        div()
            .size_full()
            .flex()
            .flex_col()
            .relative()
            .bg(cx.theme().muted.opacity(0.1))
            .border_1()
            .border_color(cx.theme().border)
            .rounded(cx.theme().radius)
            .overflow_hidden()
            .track_focus(&focus_handle)
            .key_context("BlueprintGraph")
            .on_children_prepainted({
                let panel_entity = panel_entity.clone();
                let view_id = view_id.clone();
                move |children_bounds, _window, cx| {
                    // children_bounds are in WINDOW coordinates!
                    // Calculate the bounding box of all children to get our element's window-relative bounds
                    if !children_bounds.is_empty() {
                        let mut min_x = f32::MAX;
                        let mut min_y = f32::MAX;
                        let mut max_x = f32::MIN;
                        let mut max_y = f32::MIN;

                        for child_bounds in &children_bounds {
                            min_x = min_x.min(child_bounds.origin.x.as_f32());
                            min_y = min_y.min(child_bounds.origin.y.as_f32());
                            max_x = max_x
                                .max((child_bounds.origin.x + child_bounds.size.width).as_f32());
                            max_y = max_y
                                .max((child_bounds.origin.y + child_bounds.size.height).as_f32());
                        }

                        let origin = gpui::Point {
                            x: px(min_x),
                            y: px(min_y),
                        };
                        let size = gpui::Size {
                            width: px(max_x - min_x),
                            height: px(max_y - min_y),
                        };

                        // Store the graph element's bounds derived from children (which are in window coords)
                        panel_entity.update(cx, |panel, _cx| {
                            let bounds = gpui::Bounds { origin, size };
                            panel.graph_element_bounds = Some(bounds);
                            panel
                                .graph_element_bounds_by_view
                                .insert(view_id.clone(), bounds);
                        });
                    }
                }
            })
            .id(graph_id)
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |panel, event, window, cx| {
                    // Focus on click to enable keyboard events
                    panel.focus_handle().focus(window);

                    // If editing a comment, clicking outside should save and exit edit mode
                    if panel.editing_comment.is_some() {
                        panel.finish_comment_editing(cx);
                    }

                    // Close variable drop menu if it's open
                    if panel.variable_drop_menu_position.is_some() {
                        panel.variable_drop_menu_position = None;
                        cx.notify();
                    }
                }),
            )
            // Render layers in correct z-order
            .child(Self::render_grid_background(panel, cx))
            .child(Self::render_comments(panel, cx))
            .child(Self::render_connections(panel, cx))
            .child(Self::render_nodes(panel, cx))
            .child(crate::rendering::overlay::render_selection_box(
                panel, &view_id, cx,
            ))
            .child(crate::rendering::overlay::render_viewport_bounds_debug(
                panel, cx,
            ))
            .when(panel.show_debug_overlay, |this| {
                this.child(crate::rendering::overlay::render_debug_overlay(panel, cx))
            })
            .when(panel.show_graph_controls, |this| {
                this.child(crate::rendering::overlay::render_graph_controls(panel, cx))
            })
            // Minimap disabled for now - will be implemented in ui_components
            // .when(panel.show_minimap, |this| {
            //     this.child(crate::ui_components::minimap::MinimapRenderer::render(panel, cx))
            // })
            // Quick-palette overlay — shown on right-click, same primitive as the color-picker popout
            .child(Self::render_quick_palette_overlay(panel, cx))
            .child(render_pin_hover_tooltip(panel, &view_id, cx))
            .on_mouse_down(
                gpui::MouseButton::Right,
                crate::rendering::input::on_mouse_down_right(view_id.clone(), cx),
            )
            .on_mouse_down(
                gpui::MouseButton::Left,
                crate::rendering::input::on_mouse_down_left(view_id.clone(), cx),
            )
            .on_mouse_move(crate::rendering::input::on_mouse_move(view_id.clone(), cx))
            .on_mouse_up(
                gpui::MouseButton::Left,
                crate::rendering::input::on_mouse_up_left(view_id.clone(), cx),
            )
            .on_mouse_up_out(
                gpui::MouseButton::Left,
                crate::rendering::input::on_mouse_up_left(view_id.clone(), cx),
            )
            .on_mouse_up(
                gpui::MouseButton::Right,
                crate::rendering::input::on_mouse_up_right(view_id.clone(), cx),
            )
            .on_mouse_up_out(
                gpui::MouseButton::Right,
                crate::rendering::input::on_mouse_up_right(view_id.clone(), cx),
            )
            .on_scroll_wheel(crate::rendering::input::on_scroll_wheel(
                view_id.clone(),
                cx,
            ))
            .on_key_down(crate::rendering::input::on_key_down(view_id, cx))
    }

    /// Render the quick-palette overlay using the same `deferred(anchored(…))` primitive
    /// as the color-picker popout — no `Popover` wrapper needed.
    fn render_quick_palette_overlay(
        panel: &BlueprintEditorPanel,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
        if !panel.quick_palette_open {
            return div().into_any_element();
        }

        let panel_entity = cx.entity().clone();

        deferred(
            anchored()
                .position(panel.quick_palette_screen_pos)
                .snap_to_window_with_margin(px(8.))
                .anchor(gpui::Corner::TopLeft)
                .child(
                    div()
                        .occlude()
                        .w(px(320.0))
                        .h(px(480.0))
                        .shadow_lg()
                        .rounded(px(6.0))
                        .overflow_hidden()
                        .border_1()
                        .border_color(cx.theme().border)
                        .child(panel.quick_palette_view.clone())
                        .on_children_prepainted({
                            let panel_entity = panel_entity.clone();
                            move |_children_bounds, window, cx| {
                                panel_entity.update(cx, |panel, cx| {
                                    if !panel.quick_palette_focus_pending {
                                        return;
                                    }

                                    let search_handle = panel
                                        .quick_palette_view
                                        .read(cx)
                                        .search_focus_handle(cx);
                                    panel.quick_palette_focus_pending = false;
                                    window.focus(&search_handle);
                                });
                            }
                        })
                        .on_mouse_down_out(move |_, _window, cx| {
                            panel_entity.update(cx, |panel, cx| {
                                panel.quick_palette_open = false;
                                panel.quick_palette_focus_pending = false;
                                panel.quick_palette_connection_source = None;
                                panel.popup_palette_graph_pos = None;
                                cx.notify();
                            });
                        }),
                ),
        )
        .with_priority(1)
        .into_any_element()
    }

    pub fn render_grid_background(
        panel: &BlueprintEditorPanel,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        let zoom = panel.graph.zoom_level;
        let pan = panel.graph.pan_offset;
        let background = cx.theme().muted.opacity(0.05);
        let minor_color = cx.theme().border.opacity(0.08);
        let major_color = cx.theme().border.opacity(0.18);

        gpui::canvas(
            move |_bounds, _window, _cx| {},
            move |bounds, _prepaint, window, _cx| {
                let width = bounds.size.width.as_f32();
                let height = bounds.size.height.as_f32();
                let origin_x = bounds.origin.x.as_f32();
                let origin_y = bounds.origin.y.as_f32();

                Self::paint_grid_rect(window, origin_x, origin_y, width, height, background);

                let minor_step = 10.0 * zoom;
                let major_step = 50.0 * zoom;

                if minor_step >= 6.0 {
                    Self::paint_grid_lines(
                        window,
                        origin_x,
                        origin_y,
                        width,
                        height,
                        pan,
                        zoom,
                        10.0,
                        minor_color,
                    );
                }

                if major_step >= 4.0 {
                    Self::paint_grid_lines(
                        window,
                        origin_x,
                        origin_y,
                        width,
                        height,
                        pan,
                        zoom,
                        50.0,
                        major_color,
                    );
                }
            },
        )
        .absolute()
        .inset_0()
        .size_full()
    }

    fn paint_grid_lines(
        window: &mut Window,
        origin_x: f32,
        origin_y: f32,
        width: f32,
        height: f32,
        pan: Point<f32>,
        zoom: f32,
        grid_size: f32,
        color: gpui::Hsla,
    ) {
        let step = grid_size * zoom;
        if step <= 0.0 {
            return;
        }

        let start_x = (pan.x * zoom).rem_euclid(step);
        let start_y = (pan.y * zoom).rem_euclid(step);

        let mut x = start_x;
        while x <= width {
            Self::paint_grid_rect(window, origin_x + x, origin_y, 1.0, height, color);
            x += step;
        }

        let mut y = start_y;
        while y <= height {
            Self::paint_grid_rect(window, origin_x, origin_y + y, width, 1.0, color);
            y += step;
        }
    }

    fn paint_grid_rect(
        window: &mut Window,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: gpui::Hsla,
    ) {
        let mut builder = gpui::PathBuilder::fill();
        builder.move_to(point(px(x), px(y)));
        builder.line_to(point(px(x + width), px(y)));
        builder.line_to(point(px(x + width), px(y + height)));
        builder.line_to(point(px(x), px(y + height)));
        builder.close();

        if let Ok(path) = builder.build() {
            window.paint_path(path, color);
        }
    }

    // ── Feature rendering delegation ──────────────────────────────────────
    // These methods delegate to feature modules for rendering specific aspects

    fn render_comments(
        panel: &mut BlueprintEditorPanel,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        crate::features::comments::rendering::render_all(panel, cx)
    }

    fn render_connections(
        panel: &mut BlueprintEditorPanel,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        crate::editor::panel::BlueprintEditorPanel::render_connections(panel, cx)
    }

    fn render_nodes(
        panel: &mut BlueprintEditorPanel,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        crate::features::nodes::rendering::render_all(panel, cx)
    }

    // ── Coordinate conversion utilities ───────────────────────────────────

    /// Convert graph coordinates to screen coordinates (accounting for pan and zoom)
    pub fn graph_to_screen_pos(graph_pos: Point<f32>, graph: &BlueprintGraph) -> Point<f32> {
        Point::new(
            (graph_pos.x + graph.pan_offset.x) * graph.zoom_level,
            (graph_pos.y + graph.pan_offset.y) * graph.zoom_level,
        )
    }

    /// Convert window-relative coordinates to graph element coordinates
    /// For graph operations: clicking nodes, selection box, dragging, etc.
    ///
    /// Mouse events from GPUI are relative to window origin.
    /// We already have the graph element's bounds captured during events.
    /// Simple math: element_pos = window_pos - element_origin
    pub fn window_to_graph_element_pos_for_view(
        window_pos: Point<Pixels>,
        panel: &BlueprintEditorPanel,
        view_id: &str,
    ) -> Point<Pixels> {
        if let Some(bounds) = panel.graph_element_bounds_by_view.get(view_id) {
            Point::new(
                window_pos.x - bounds.origin.x,
                window_pos.y - bounds.origin.y,
            )
        } else {
            window_pos
        }
    }

    pub fn window_to_graph_element_pos(
        window_pos: Point<Pixels>,
        panel: &BlueprintEditorPanel,
    ) -> Point<Pixels> {
        if let Some(view_id) = panel.interaction_view_id.as_ref() {
            if let Some(bounds) = panel.graph_element_bounds_by_view.get(view_id) {
                return Point::new(
                    window_pos.x - bounds.origin.x,
                    window_pos.y - bounds.origin.y,
                );
            }
        }

        // If no interaction owner is set yet (common when child handlers stop propagation),
        // resolve against whichever graph view currently contains the pointer.
        let wx = window_pos.x.as_f32();
        let wy = window_pos.y.as_f32();
        for bounds in panel.graph_element_bounds_by_view.values() {
            let left = bounds.origin.x.as_f32();
            let top = bounds.origin.y.as_f32();
            let right = left + bounds.size.width.as_f32();
            let bottom = top + bounds.size.height.as_f32();
            if wx >= left && wx <= right && wy >= top && wy <= bottom {
                return Point::new(
                    window_pos.x - bounds.origin.x,
                    window_pos.y - bounds.origin.y,
                );
            }
        }

        if let Some(bounds) = &panel.graph_element_bounds {
            // Direct subtraction: mouse relative to element = mouse relative to window - element origin relative to window
            Point::new(
                window_pos.x - bounds.origin.x,
                window_pos.y - bounds.origin.y,
            )
        } else {
            // On first event before bounds captured, just return window pos as-is
            // This will be corrected on the next event after bounds are set
            window_pos
        }
    }

    /// Convert window-relative coordinates to panel coordinates
    /// For UI elements positioned at panel level: menus, tooltips, etc.
    pub fn window_to_panel_pos(
        window_pos: Point<Pixels>,
        panel: &BlueprintEditorPanel,
    ) -> Point<Pixels> {
        // Same calculation as graph element since they share the same coordinate space
        Self::window_to_graph_element_pos(window_pos, panel)
    }

    /// Convert screen coordinates to graph coordinates (inverse of graph_to_screen_pos)
    pub fn screen_to_graph_pos(screen_pos: Point<Pixels>, graph: &BlueprintGraph) -> Point<f32> {
        Point::new(
            (screen_pos.x.as_f32() / graph.zoom_level) - graph.pan_offset.x,
            (screen_pos.y.as_f32() / graph.zoom_level) - graph.pan_offset.y,
        )
    }

    /// Snaps a position to the fixed 10px graph grid.
    pub fn snap_to_grid(pos: Point<f32>, _zoom_level: f32) -> Point<f32> {
        let grid_size = 10.0;

        Point::new(
            (pos.x / grid_size).round() * grid_size,
            (pos.y / grid_size).round() * grid_size,
        )
    }

    // ── Viewport culling / Virtualization helpers ─────────────────────────

    /// Check if a node is visible within the current viewport (for virtualization)
    pub fn is_node_visible_simple(node: &BlueprintNode, graph: &BlueprintGraph) -> bool {
        // Calculate node position in screen coordinates
        let node_screen_pos = Self::graph_to_screen_pos(node.position, graph);
        let _node_screen_size = Size::new(
            node.size.width * graph.zoom_level,
            node.size.height * graph.zoom_level,
        );

        // Calculate the visible area based on the inverse of current pan/zoom
        // This creates a dynamic culling frustum that properly accounts for viewport transformations

        // Convert screen bounds back to graph space for accurate culling
        let screen_to_graph_origin = Self::screen_to_graph_pos(Point::new(px(0.0), px(0.0)), graph);
        let screen_to_graph_end =
            Self::screen_to_graph_pos(Point::new(px(3840.0), px(2160.0)), graph); // 4K bounds

        // Add generous padding in graph space to prevent premature culling
        let padding_in_graph_space = 200.0 / graph.zoom_level; // Padding scales with zoom

        let visible_left = screen_to_graph_origin.x - padding_in_graph_space;
        let visible_top = screen_to_graph_origin.y - padding_in_graph_space;
        let visible_right = screen_to_graph_end.x + padding_in_graph_space;
        let visible_bottom = screen_to_graph_end.y + padding_in_graph_space;

        // Check if node intersects with visible bounds in graph space
        let node_left = node.position.x;
        let node_top = node.position.y;
        let node_right = node.position.x + node.size.width;
        let node_bottom = node.position.y + node.size.height;

        !(node_left > visible_right
            || node_right < visible_left
            || node_top > visible_bottom
            || node_bottom < visible_top)
    }

    /// Check if a connection is visible (connection is visible if either endpoint node is visible)
    pub fn is_connection_visible_simple(connection: &Connection, graph: &BlueprintGraph) -> bool {
        // A connection is visible if either of its nodes is visible
        let from_node = graph.nodes.iter().find(|n| n.id == connection.source_node);
        let to_node = graph.nodes.iter().find(|n| n.id == connection.target_node);

        match (from_node, to_node) {
            (Some(from), Some(to)) => {
                Self::is_node_visible_simple(from, graph) || Self::is_node_visible_simple(to, graph)
            }
            _ => false, // If either node doesn't exist, don't render the connection
        }
    }

    // ── Utility helpers ───────────────────────────────────────────────────

    /// Parse hex color string (#RRGGBB or #RRGGBBAA) to HSLA
    pub fn parse_hex_color(hex: &str) -> Option<gpui::Hsla> {
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

    /// Calculate the screen position of a pin on a node
    pub fn calculate_pin_position(
        node: &BlueprintNode,
        pin_id: &str,
        is_input: bool,
        graph: &BlueprintGraph,
    ) -> Option<Point<f32>> {
        // Reroute nodes are a single dot at their graph position.
        if node.node_type == NodeType::Reroute {
            return Some(Self::graph_to_screen_pos(node.position, graph));
        }

        // These MUST match the values used in render_blueprint_node / render_node_pins.
        const HEADER_H: f32 = 27.0;
        const SEP_H: f32 = 1.0;
        const BODY_PAD: f32 = 8.0;
        const PIN_ROW_H: f32 = 16.0;
        const PIN_GAP: f32 = 4.0;

        let z = graph.zoom_level;
        let nsp = Self::graph_to_screen_pos(node.position, graph);

        let row = if is_input {
            node.inputs.iter().position(|p| p.id == pin_id)?
        } else {
            node.outputs.iter().position(|p| p.id == pin_id)?
        };

        // Y: top of node → header → separator → body padding → row center
        let pin_y = nsp.y
            + (HEADER_H + SEP_H + BODY_PAD) * z
            + row as f32 * (PIN_ROW_H + PIN_GAP) * z
            + (PIN_ROW_H * z) / 2.0;

        // X: left or right edge based on input/output
        let pin_x = if is_input {
            nsp.x // Input pins are on the left edge
        } else {
            nsp.x + node.size.width * z // Output pins are on the right edge
        };

        Some(Point::new(pin_x, pin_y))
    }
}
