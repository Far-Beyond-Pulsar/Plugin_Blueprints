// Input event handlers for the blueprint graph canvas.
//
// All mouse positions arrive in window space.  The first step in every handler
// is converting to canvas space:
//
//   canvas_pos = window_pos - canvas_origin          (subtract GPU surface origin)
//   graph_pos  = canvas_pos / zoom - pan             (apply inverse viewport transform)
//
// Hit testing is then done entirely in graph space using the same layout
// constants as the GPU renderer, so click targets exactly match what's drawn.

use crate::rendering::graph::{
    NodeGraphRenderer, BODY_PAD, HEADER_H, PIN_GAP, PIN_ROW_H, PIN_SIZE, SEP_H,
};
use crate::core::types::NodeType;
use crate::editor::panel::BlueprintEditorPanel;
use gpui::*;
use ui::graph::DataType;
use ui::PixelsExt;

// ─── coordinate conversion ────────────────────────────────────────────────────

/// Window → canvas-relative position using the captured GPU surface origin.
fn to_canvas(window_pos: Point<Pixels>, panel: &BlueprintEditorPanel) -> Point<f32> {
    let o = *panel.canvas_origin.borrow();
    Point::new(
        window_pos.x.as_f32() - o.x,
        window_pos.y.as_f32() - o.y,
    )
}

/// Canvas-relative → graph space.
fn to_graph(canvas: Point<f32>, panel: &BlueprintEditorPanel) -> Point<f32> {
    let z = panel.graph.zoom_level;
    Point::new(
        canvas.x / z - panel.graph.pan_offset.x,
        canvas.y / z - panel.graph.pan_offset.y,
    )
}

// ─── hit testing ─────────────────────────────────────────────────────────────

/// Find whichever node the graph-space point lands inside (AABB, last-first z-order).
fn hit_node<'a>(gp: Point<f32>, panel: &'a BlueprintEditorPanel) -> Option<&'a str> {
    for node in panel.graph.nodes.iter().rev() {
        let nl = node.position.x;
        let nt = node.position.y;
        let nr = nl + node.size.width;
        let nb = nt + node.size.height;
        if gp.x >= nl && gp.x <= nr && gp.y >= nt && gp.y <= nb {
            return Some(&node.id);
        }
    }
    None
}

/// Find the output pin nearest to a canvas-space point (for drag-start).
fn hit_output_pin(canvas: Point<f32>, panel: &BlueprintEditorPanel) -> Option<(String, String)> {
    let r = (PIN_SIZE * panel.graph.zoom_level * 0.9).max(6.0);
    for node in &panel.graph.nodes {
        for (i, pin) in node.outputs.iter().enumerate() {
            let c = NodeGraphRenderer::pin_canvas_pos(node, false, i, &panel.graph);
            let d = ((canvas.x - c.x).powi(2) + (canvas.y - c.y).powi(2)).sqrt();
            if d <= r {
                return Some((node.id.clone(), pin.id.clone()));
            }
        }
    }
    None
}

/// Find the input pin nearest to a canvas-space point (for connection drop).
fn hit_input_pin(
    canvas:      Point<f32>,
    panel:       &BlueprintEditorPanel,
    skip_node:   &str,
    src_type:    &DataType,
) -> Option<(String, String)> {
    let r = (PIN_SIZE * panel.graph.zoom_level * 1.3).max(8.0);
    for node in &panel.graph.nodes {
        if node.id == skip_node { continue; }
        for (i, pin) in node.inputs.iter().enumerate() {
            if !src_type.is_compatible_with(&pin.data_type) { continue; }
            let c = NodeGraphRenderer::pin_canvas_pos(node, true, i, &panel.graph);
            let d = ((canvas.x - c.x).powi(2) + (canvas.y - c.y).powi(2)).sqrt();
            if d <= r {
                return Some((node.id.clone(), pin.id.clone()));
            }
        }
    }
    None
}

/// Find a pin (either side) near canvas point — for right-click disconnect menu.
fn hit_any_pin(canvas: Point<f32>, panel: &BlueprintEditorPanel) -> Option<(String, String)> {
    let r = (PIN_SIZE * panel.graph.zoom_level * 1.2).max(8.0);
    for node in &panel.graph.nodes {
        for (is_input, pins) in [(true, &node.inputs), (false, &node.outputs)] {
            for (i, pin) in pins.iter().enumerate() {
                let c = NodeGraphRenderer::pin_canvas_pos(node, is_input, i, &panel.graph);
                let d = ((canvas.x - c.x).powi(2) + (canvas.y - c.y).powi(2)).sqrt();
                if d <= r {
                    return Some((node.id.clone(), pin.id.clone()));
                }
            }
        }
    }
    None
}

// ─── event handlers ───────────────────────────────────────────────────────────

pub fn on_mouse_down_right(
    _view_id: String,
    cx: &mut Context<BlueprintEditorPanel>,
) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) {
    let entity = cx.entity().clone();
    move |event: &MouseDownEvent, _window, cx| {
        entity.update(cx, |panel, cx| {
            // Close any open context menus
            panel.node_context_menu = None;
            panel.pin_context_menu  = None;

            let canvas = to_canvas(event.position, panel);
            let gp     = to_graph(canvas, panel);

            panel.popup_palette_graph_pos = Some(gp);

            if panel.dragging_connection.is_none() && panel.dragging_node.is_none() {
                let mp = Point::new(canvas.x, canvas.y);
                panel.right_click_start = Some(mp);
            }
        });
    }
}

pub fn on_mouse_down_left(
    _view_id: String,
    cx: &mut Context<BlueprintEditorPanel>,
) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) {
    let entity = cx.entity().clone();
    move |event: &MouseDownEvent, _window, cx| {
        entity.update(cx, |panel, cx| {
            // Close palette / context menus on any left click
            if panel.quick_palette_open {
                panel.quick_palette_open = false;
                panel.quick_palette_focus_pending = false;
                panel.quick_palette_connection_source = None;
                panel.popup_palette_graph_pos = None;
                cx.notify();
                return;
            }
            panel.node_context_menu = None;
            panel.pin_context_menu  = None;

            if panel.editing_comment.is_some() {
                panel.finish_comment_editing(cx);
            }
            if panel.variable_drop_menu_position.is_some() {
                panel.variable_drop_menu_position = None;
                cx.notify();
            }

            let canvas = to_canvas(event.position, panel);
            let gp     = to_graph(canvas, panel);

            // Priority: output pin → node → empty space
            if let Some((node_id, pin_id)) = hit_output_pin(canvas, panel) {
                panel.start_connection_drag_from_pin(node_id, pin_id, gp, cx);
                return;
            }

            if let Some(node_id) = hit_node(gp, panel).map(str::to_owned) {
                if !panel.graph.selected_nodes.contains(&node_id) {
                    if !event.modifiers.control {
                        panel.graph.selected_nodes.clear();
                        panel.graph.selected_comments.clear();
                    }
                    panel.graph.selected_nodes.push(node_id.clone());
                }
                panel.start_drag(node_id, gp, cx);
                return;
            }

            // Empty space — start selection drag
            if !event.modifiers.control {
                panel.graph.selected_nodes.clear();
                panel.graph.selected_comments.clear();
            }
            panel.start_selection_drag(gp, event.modifiers.control, cx);
        });
    }
}

pub fn on_mouse_move(
    view_id: String,
    cx: &mut Context<BlueprintEditorPanel>,
) -> impl Fn(&MouseMoveEvent, &mut Window, &mut App) {
    let entity = cx.entity().clone();
    move |event: &MouseMoveEvent, _window, cx| {
        entity.update(cx, |panel, cx| {
            let canvas = to_canvas(event.position, panel);
            let mp     = Point::new(canvas.x, canvas.y);

            // Threshold-detect right-drag → pan
            if let Some(right_start) = panel.right_click_start {
                let dist = ((mp.x - right_start.x).powi(2) + (mp.y - right_start.y).powi(2)).sqrt();
                if dist > panel.right_click_threshold {
                    panel.start_panning(right_start, cx);
                    panel.right_click_start = None;
                }
            }

            let gp = to_graph(canvas, panel);

            if panel.dragging_comment.is_some() {
                panel.update_comment_drag(gp, cx);
            } else if panel.resizing_comment.is_some() {
                panel.update_comment_resize(gp, cx);
            } else if panel.dragging_node.is_some() {
                panel.update_drag(gp, cx);
            } else if panel.dragging_connection.is_some() {
                panel.update_connection_drag(mp, cx);
            } else if panel.is_selecting() {
                panel.update_selection_drag(gp, cx);
            } else if panel.is_panning() {
                panel.update_pan(mp, cx);
            }
        });
    }
}

pub fn on_mouse_up_left(
    _view_id: String,
    cx: &mut Context<BlueprintEditorPanel>,
) -> impl Fn(&MouseUpEvent, &mut Window, &mut App) {
    let entity = cx.entity().clone();
    move |event: &MouseUpEvent, _window, cx| {
        entity.update(cx, |panel, cx| {
            let canvas = to_canvas(event.position, panel);
            let gp     = to_graph(canvas, panel);
            let mp     = Point::new(canvas.x, canvas.y);

            if panel.dragging_comment.is_some() {
                panel.end_comment_drag(cx);
            } else if panel.resizing_comment.is_some() {
                panel.end_comment_resize(cx);
            } else if panel.dragging_node.is_some() {
                panel.end_drag(cx);
            } else if panel.dragging_variable.is_some() {
                panel.finish_dragging_variable(gp, cx);
            } else if let Some(drag) = panel.dragging_connection.clone() {
                // Check if we landed on a compatible input pin
                if let Some((nid, pid)) = hit_input_pin(canvas, panel, &drag.source_node, &drag.source_pin_type) {
                    panel.complete_connection_on_pin(nid, pid, cx);
                } else {
                    // Dropped on empty space → open quick palette filtered by type
                    panel.popup_palette_graph_pos        = Some(gp);
                    panel.quick_palette_connection_source = Some(drag);
                    panel.quick_palette_open             = true;
                    panel.quick_palette_focus_pending    = true;
                    panel.quick_palette_screen_pos       = event.position;
                    panel.dragging_connection            = None;
                    cx.notify();
                }
            } else if panel.is_selecting() {
                panel.end_selection_drag(cx);
            } else if panel.is_panning() {
                panel.end_panning(cx);
            }
        });
    }
}

pub fn on_mouse_up_right(
    _view_id: String,
    cx: &mut Context<BlueprintEditorPanel>,
) -> impl Fn(&MouseUpEvent, &mut Window, &mut App) {
    let entity = cx.entity().clone();
    move |event: &MouseUpEvent, _window, cx| {
        entity.update(cx, |panel, cx| {
            let was_click = panel.right_click_start.is_some() && !panel.is_panning();

            if panel.is_panning() {
                panel.end_panning(cx);
            }

            if was_click {
                let canvas = to_canvas(event.position, panel);
                let gp     = to_graph(canvas, panel);

                // Hit test: pin → node → empty space
                if let Some((nid, pid)) = hit_any_pin(canvas, panel) {
                    panel.pin_context_menu  = Some((nid, pid, event.position));
                    panel.quick_palette_open = false;
                } else if let Some(node_id) = hit_node(gp, panel).map(str::to_owned) {
                    if !panel.graph.selected_nodes.contains(&node_id) {
                        panel.select_node(Some(node_id.clone()), cx);
                    }
                    panel.node_context_menu  = Some((node_id, event.position));
                    panel.quick_palette_open = false;
                } else {
                    // Empty space → add node palette
                    panel.quick_palette_open          = true;
                    panel.quick_palette_focus_pending = true;
                    panel.quick_palette_screen_pos    = event.position;
                    panel.node_context_menu           = None;
                    panel.pin_context_menu            = None;
                }
                cx.notify();
            }

            panel.right_click_start = None;
        });
    }
}

pub fn on_scroll_wheel(
    _view_id: String,
    cx: &mut Context<BlueprintEditorPanel>,
) -> impl Fn(&ScrollWheelEvent, &mut Window, &mut App) {
    let entity = cx.entity().clone();
    move |event: &ScrollWheelEvent, _window, cx| {
        entity.update(cx, |panel, cx| {
            let delta_y = match event.delta {
                ScrollDelta::Pixels(p) => p.y.as_f32(),
                ScrollDelta::Lines(l)  => l.y * 20.0,
            };
            let canvas_pos = to_canvas(event.position, panel);
            let element_pos = Point::new(px(canvas_pos.x), px(canvas_pos.y));
            panel.handle_zoom(delta_y, element_pos, cx);
        });
    }
}

pub fn on_key_down(
    _view_id: String,
    cx: &mut Context<BlueprintEditorPanel>,
) -> impl Fn(&KeyDownEvent, &mut Window, &mut App) {
    let entity = cx.entity().clone();
    move |event: &KeyDownEvent, window, cx| {
        entity.update(cx, |panel, cx| {
            let key = event.keystroke.key.to_lowercase();
            let has_copy_paste_modifier =
                event.keystroke.modifiers.control || event.keystroke.modifiers.platform;

            if panel.editing_comment.is_some() {
                if key == "escape" { panel.editing_comment = None; cx.notify(); }
                else if key == "enter" && event.keystroke.modifiers.control {
                    panel.finish_comment_editing(cx);
                }
                return;
            }

            match key.as_str() {
                "escape" => {
                    panel.node_context_menu = None;
                    panel.pin_context_menu  = None;
                    if panel.variable_drop_menu_position.is_some() {
                        panel.variable_drop_menu_position = None;
                    } else if panel.dragging_connection.is_some() {
                        panel.cancel_connection_drag(cx);
                    }
                    cx.notify();
                }
                "delete" | "backspace" => panel.delete_selected_nodes(cx),
                "c" if !has_copy_paste_modifier => {
                    panel.create_comment_at_center(window, cx);
                }
                "c" if has_copy_paste_modifier => {
                    panel.copy_selected_entities(cx);
                }
                "v" if has_copy_paste_modifier => {
                    panel.paste_entities(window, cx);
                }
                "z" if event.keystroke.modifiers.control && event.keystroke.modifiers.shift => {
                    panel.redo(cx);
                }
                "z" if event.keystroke.modifiers.control => {
                    panel.undo(cx);
                }
                "y" if event.keystroke.modifiers.control => {
                    panel.redo(cx);
                }
                _ => {}
            }
        });
    }
}
