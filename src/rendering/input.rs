//!
//! This module provides input event handlers that can be attached to the graph
//! canvas div. It delegates to appropriate feature operation modules based on
//! what's being interacted with.
//! Input event handling for the graph canvas

use ui::ActiveTheme;
use ui::PixelsExt;
use ui::StyledExt;
use ui::Sizable;
use gpui::*;
use crate::editor::panel::BlueprintEditorPanel;
use super::graph::NodeGraphRenderer;

/// Create mouse down (right button) handler for the graph canvas
pub fn on_mouse_down_right(
    view_id: String,
    cx: &mut Context<BlueprintEditorPanel>,
) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) {
    let entity = cx.entity().clone();
    move |event: &MouseDownEvent, _window: &mut Window, cx: &mut App| {
        entity.update(cx, |panel, cx| {
            // Convert window coordinates to element coordinates
            let element_pos = NodeGraphRenderer::window_to_graph_element_pos_for_view(event.position, panel, &view_id);
            let mouse_pos = Point::new(element_pos.x.as_f32(), element_pos.y.as_f32());
            panel.interaction_view_id = Some(view_id.clone());

            // Store right-click start position for gesture detection
            if panel.dragging_connection.is_none() && panel.dragging_node.is_none() {
                panel.right_click_start = Some(mouse_pos);
                // Don't show menu immediately - wait for mouse up or movement
            }
        });
    }
}

/// Create mouse down (left button) handler for the graph canvas
pub fn on_mouse_down_left(
    view_id: String,
    cx: &mut Context<BlueprintEditorPanel>,
) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) {
    let entity = cx.entity().clone();
    move |event: &MouseDownEvent, _window: &mut Window, cx: &mut App| {
        entity.update(cx, |panel, cx| {
        panel.interaction_view_id = Some(view_id.clone());
        // Debug: Print raw event position and calculated offset
        tracing::info!("[MOUSE] Raw window position: x={}, y={}", event.position.x.as_f32(), event.position.y.as_f32());
        tracing::info!("[MOUSE] Stored element bounds: {:?}", panel.graph_element_bounds_by_view.get(&view_id));

        // Convert window-relative coordinates to element-relative coordinates
        let element_pos = NodeGraphRenderer::window_to_graph_element_pos_for_view(event.position, panel, &view_id);
        tracing::info!("[MOUSE] Calculated element-relative position: x={}, y={}", element_pos.x.as_f32(), element_pos.y.as_f32());

        // Convert element coordinates to graph coordinates
        let graph_pos = NodeGraphRenderer::screen_to_graph_pos(element_pos, &panel.graph);
        let _mouse_pos = Point::new(element_pos.x.as_f32(), element_pos.y.as_f32());

        tracing::info!("[MOUSE] Converted to graph pos: x={}, y={}", graph_pos.x, graph_pos.y);
        tracing::info!("[MOUSE] Pan offset: x={}, y={}", panel.graph.pan_offset.x, panel.graph.pan_offset.y);
        tracing::info!("[MOUSE] Zoom level: {}", panel.graph.zoom_level);

        // Check if clicking on a node (check ALL nodes, not just rendered ones)
        let clicked_node = panel.graph.nodes.iter().find(|node| {
            let node_left = node.position.x;
            let node_top = node.position.y;
            let node_right = node.position.x + node.size.width;
            let node_bottom = node.position.y + node.size.height;

            graph_pos.x >= node_left
                && graph_pos.x <= node_right
                && graph_pos.y >= node_top
                && graph_pos.y <= node_bottom
        });

        if let Some(node) = clicked_node {
            // Clicked on a node
            tracing::info!("Clicked on node: {}", node.id);

            // Delegate to features::nodes::operations::handle_node_click
            // For now, inline implementation:
            if !panel.graph.selected_nodes.contains(&node.id) {
                if !event.modifiers.control {
                    panel.graph.selected_nodes.clear();
                }
                panel.graph.selected_nodes.push(node.id.clone());
            }

            panel.start_drag(node.id.clone(), graph_pos, cx);
        } else {
            // Clicked on empty space
            if !panel.handle_empty_space_click(graph_pos, cx) {
                // No UI element claimed the click, start selection drag
                if !event.modifiers.control {
                    panel.graph.selected_nodes.clear();
                    panel.graph.selected_comments.clear();
                }
                panel.start_selection_drag(graph_pos, event.modifiers.control, cx);
            }
        }
        });
    }
}

/// Create mouse move handler for the graph canvas
pub fn on_mouse_move(
    view_id: String,
    cx: &mut Context<BlueprintEditorPanel>,
) -> impl Fn(&MouseMoveEvent, &mut Window, &mut App) {
    let entity = cx.entity().clone();
    move |event: &MouseMoveEvent, _window: &mut Window, cx: &mut App| {
        entity.update(cx, |panel, cx| {
            if panel.interaction_view_id.as_deref() != Some(view_id.as_str()) {
                return;
            }

            // Convert window coordinates to element coordinates
            let element_pos = NodeGraphRenderer::window_to_graph_element_pos_for_view(event.position, panel, &view_id);
            let mouse_pos = Point::new(element_pos.x.as_f32(), element_pos.y.as_f32());

            // Check if right-click drag should start panning
            if let Some(right_start) = panel.right_click_start {
                let distance = ((mouse_pos.x - right_start.x).powi(2) + (mouse_pos.y - right_start.y).powi(2)).sqrt();
                if distance > panel.right_click_threshold {
                    // Start panning if we've moved beyond threshold
                    panel.start_panning(right_start, cx);
                    panel.right_click_start = None; // Clear the right-click state
                }
            }

            if panel.dragging_comment.is_some() {
                let graph_pos = NodeGraphRenderer::screen_to_graph_pos(element_pos, &panel.graph);
                panel.update_comment_drag(graph_pos, cx);
            } else if panel.resizing_comment.is_some() {
                let graph_pos = NodeGraphRenderer::screen_to_graph_pos(element_pos, &panel.graph);
                panel.update_comment_resize(graph_pos, cx);
            } else if panel.dragging_node.is_some() {
                let graph_pos = NodeGraphRenderer::screen_to_graph_pos(element_pos, &panel.graph);
                panel.update_drag(graph_pos, cx);
            } else if panel.dragging_connection.is_some() {
                // Update mouse position for drag line rendering
                panel.update_connection_drag(mouse_pos, cx);
            } else if panel.is_selecting() {
                // Update selection drag
                let graph_pos = NodeGraphRenderer::screen_to_graph_pos(element_pos, &panel.graph);
                panel.update_selection_drag(graph_pos, cx);
            } else if panel.is_panning() && panel.dragging_node.is_none() {
                // Only update panning if we're not dragging a node
                panel.update_pan(mouse_pos, cx);
            }
        });
    }
}

/// Create mouse up (left button) handler for the graph canvas
pub fn on_mouse_up_left(
    view_id: String,
    cx: &mut Context<BlueprintEditorPanel>,
) -> impl Fn(&MouseUpEvent, &mut Window, &mut App) {
    let entity = cx.entity().clone();
    move |event: &MouseUpEvent, window: &mut Window, cx: &mut App| {
        entity.update(cx, |panel, cx| {
            if panel.interaction_view_id.as_deref() != Some(view_id.as_str()) {
                return;
            }

            if panel.dragging_comment.is_some() {
                panel.end_comment_drag(cx);
            } else if panel.resizing_comment.is_some() {
                panel.end_comment_resize(cx);
            } else if panel.dragging_node.is_some() {
                panel.end_drag(cx);
            } else if panel.dragging_variable.is_some() {
                // Variable dropped on canvas - show Get/Set context menu
                let element_pos = NodeGraphRenderer::window_to_graph_element_pos_for_view(event.position, panel, &view_id);
                let graph_pos = NodeGraphRenderer::screen_to_graph_pos(element_pos, &panel.graph);
                panel.finish_dragging_variable(graph_pos, cx);
            } else if panel.dragging_connection.is_some() {
                // Show node creation menu when dropping connection on empty space
                let element_pos = NodeGraphRenderer::window_to_graph_element_pos_for_view(event.position, panel, &view_id);
                let graph_pos = NodeGraphRenderer::screen_to_graph_pos(element_pos, &panel.graph);
                panel.show_node_picker(graph_pos, window, cx);
                panel.cancel_connection_drag(cx);
            } else if panel.is_selecting() {
                // End selection drag
                panel.end_selection_drag(cx);
            } else if panel.is_panning() {
                panel.end_panning(cx);
            }

            panel.interaction_view_id = None;
        });
    }
}

/// Create mouse up (right button) handler for the graph canvas
pub fn on_mouse_up_right(
    view_id: String,
    cx: &mut Context<BlueprintEditorPanel>,
) -> impl Fn(&MouseUpEvent, &mut Window, &mut App) {
    let entity = cx.entity().clone();
    move |_event: &MouseUpEvent, _window: &mut Window, cx: &mut App| {
        entity.update(cx, |panel, cx| {
            if panel.interaction_view_id.as_deref() != Some(view_id.as_str()) {
                return;
            }

            // Match old behavior: always end panning on RMB release.
            if panel.is_panning() {
                panel.end_panning(cx);
            }

            // Always clear right-click gesture state.
            panel.right_click_start = None;
            panel.interaction_view_id = None;
        });
    }
}

/// Create scroll wheel handler for the graph canvas
pub fn on_scroll_wheel(
    view_id: String,
    cx: &mut Context<BlueprintEditorPanel>,
) -> impl Fn(&ScrollWheelEvent, &mut Window, &mut App) {
    let entity = cx.entity().clone();
    move |event: &ScrollWheelEvent, _window: &mut Window, cx: &mut App| {
        entity.update(cx, |panel, cx| {
            panel.interaction_view_id = Some(view_id.clone());
            // Zoom with scroll wheel
            let delta_y = match event.delta {
                ScrollDelta::Pixels(p) => p.y.as_f32(),
                ScrollDelta::Lines(l) => l.y * 20.0, // Convert lines to pixels
            };

            // Perform zoom centered on the mouse
            // Convert to element coordinates first
            let element_pos = NodeGraphRenderer::window_to_graph_element_pos_for_view(event.position, panel, &view_id);
            panel.handle_zoom(delta_y, element_pos, cx);
        });
    }
}

/// Create key down handler for the graph canvas
pub fn on_key_down(
    _view_id: String,
    cx: &mut Context<BlueprintEditorPanel>,
) -> impl Fn(&KeyDownEvent, &mut Window, &mut App) {
    let entity = cx.entity().clone();
    move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
        entity.update(cx, |panel, cx| {
            tracing::info!("Key pressed: {:?}", event.keystroke.key);

            let key_lower = event.keystroke.key.to_lowercase();

            if panel.editing_comment.is_some() {
                // Handle comment editing keys
                if key_lower == "escape" {
                    // Cancel editing without saving
                    panel.editing_comment = None;
                    cx.notify();
                } else if key_lower == "enter" && event.keystroke.modifiers.control {
                    // Ctrl+Enter saves the comment
                    panel.finish_comment_editing(cx);
                }
            } else if key_lower == "escape" {
                // Escape key dismisses menus and cancels operations
                if panel.variable_drop_menu_position.is_some() {
                    panel.variable_drop_menu_position = None;
                    cx.notify();
                } else if panel.dragging_connection.is_some() {
                    panel.cancel_connection_drag(cx);
                }
            } else if key_lower == "delete" || key_lower == "backspace" {
                tracing::info!(
                    "Delete key pressed! Selected nodes: {:?}",
                    panel.graph.selected_nodes
                );
                panel.delete_selected_nodes(cx);
            } else if key_lower == "c" && event.keystroke.modifiers.control {
                // Ctrl+C creates a new comment
                panel.create_comment_at_center(window, cx);
            }
        });
    }
}
