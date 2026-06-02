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

use crate::core::types::NodeType;
use crate::editor::workspace_panels::GraphCanvasPanel;
use crate::rendering::graph::{
    NodeGraphRenderer, BODY_PAD, HEADER_H, PIN_GAP, PIN_ROW_H, PIN_SIZE, SEP_H,
};
use gpui::*;
use ui::graph::DataType;
use ui::PixelsExt;

// ─── coordinate conversion ────────────────────────────────────────────────────

fn to_canvas(window_pos: Point<Pixels>, canvas: &GraphCanvasPanel) -> Point<f32> {
    let o = *canvas.canvas_origin.borrow();
    Point::new(window_pos.x.as_f32() - o.x, window_pos.y.as_f32() - o.y)
}

fn to_graph(cp: Point<f32>, canvas: &GraphCanvasPanel) -> Point<f32> {
    let z = canvas.graph.zoom_level;
    Point::new(
        cp.x / z - canvas.graph.pan_offset.x,
        cp.y / z - canvas.graph.pan_offset.y,
    )
}

// ─── hit testing ─────────────────────────────────────────────────────────────

fn hit_node<'a>(gp: Point<f32>, canvas: &'a GraphCanvasPanel) -> Option<&'a str> {
    for node in canvas.graph.nodes.iter().rev() {
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

fn hit_output_pin(cp: Point<f32>, canvas: &GraphCanvasPanel) -> Option<(String, String)> {
    let r = (PIN_SIZE * canvas.graph.zoom_level * 0.9).max(6.0);
    for node in &canvas.graph.nodes {
        for (i, pin) in node.outputs.iter().enumerate() {
            let c = NodeGraphRenderer::pin_canvas_pos(node, false, i, &canvas.graph);
            let d = ((cp.x - c.x).powi(2) + (cp.y - c.y).powi(2)).sqrt();
            if d <= r {
                return Some((node.id.clone(), pin.id.clone()));
            }
        }
    }
    None
}

fn hit_input_pin(
    cp: Point<f32>,
    canvas: &GraphCanvasPanel,
    skip_node: &str,
    src_type: &DataType,
) -> Option<(String, String)> {
    let r = (PIN_SIZE * canvas.graph.zoom_level * 1.3).max(8.0);
    for node in &canvas.graph.nodes {
        if node.id == skip_node {
            continue;
        }
        for (i, pin) in node.inputs.iter().enumerate() {
            if !src_type.is_compatible_with(&pin.data_type) {
                continue;
            }
            let c = NodeGraphRenderer::pin_canvas_pos(node, true, i, &canvas.graph);
            let d = ((cp.x - c.x).powi(2) + (cp.y - c.y).powi(2)).sqrt();
            if d <= r {
                return Some((node.id.clone(), pin.id.clone()));
            }
        }
    }
    None
}

fn hit_any_pin(cp: Point<f32>, canvas: &GraphCanvasPanel) -> Option<(String, String)> {
    let r = (PIN_SIZE * canvas.graph.zoom_level * 1.2).max(8.0);
    for node in &canvas.graph.nodes {
        for (is_input, pins) in [(true, &node.inputs), (false, &node.outputs)] {
            for (i, pin) in pins.iter().enumerate() {
                let c = NodeGraphRenderer::pin_canvas_pos(node, is_input, i, &canvas.graph);
                let d = ((cp.x - c.x).powi(2) + (cp.y - c.y).powi(2)).sqrt();
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
    cx: &mut Context<GraphCanvasPanel>,
) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) {
    let entity = cx.entity().clone();
    move |event: &MouseDownEvent, _window, cx| {
        entity.update(cx, |canvas, cx| {
            canvas.node_context_menu = None;
            canvas.pin_context_menu = None;

            let cp = to_canvas(event.position, canvas);
            let gp = to_graph(cp, canvas);
            canvas.popup_palette_graph_pos = Some(gp);

            if canvas.dragging_connection.is_none() && canvas.dragging_node.is_none() {
                canvas.right_click_start = Some(Point::new(cp.x, cp.y));
            }
            cx.notify();
        });
    }
}

pub fn on_mouse_down_left(
    cx: &mut Context<GraphCanvasPanel>,
) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) {
    let entity = cx.entity().clone();
    move |event: &MouseDownEvent, window, cx| {
        entity.update(cx, |canvas, cx| {
            // Close palette / context menus on any left click
            if canvas.quick_palette_open {
                canvas.quick_palette_open = false;
                canvas.quick_palette_focus_pending = false;
                canvas.quick_palette_connection_source = None;
                canvas.popup_palette_graph_pos = None;
                cx.notify();
                return;
            }
            canvas.node_context_menu = None;
            canvas.pin_context_menu = None;

            if canvas.editing_comment.is_some() {
                canvas.finish_comment_editing(cx);
            }
            if canvas.variable_drop_menu_position.is_some() {
                canvas.variable_drop_menu_position = None;
                cx.notify();
            }

            let cp = to_canvas(event.position, canvas);
            let gp = to_graph(cp, canvas);

            // Priority: output pin → node → empty space
            if let Some((node_id, pin_id)) = hit_output_pin(cp, canvas) {
                canvas.start_connection_drag_from_pin(node_id, pin_id, gp, cx);
                return;
            }

            if let Some(node_id) = hit_node(gp, canvas).map(str::to_owned) {
                // ── Double-click detection ────────────────────────────────────
                let now = std::time::Instant::now();
                let is_double_click = if let (Some(t), Some(p)) =
                    (canvas.last_click_time, canvas.last_click_pos)
                {
                    let ms = now.duration_since(t).as_millis();
                    let d = ((gp.x - p.x).powi(2) + (gp.y - p.y).powi(2)).sqrt();
                    ms < 500 && d < 50.0
                } else {
                    false
                };

                if is_double_click {
                    if let Some(node) = canvas.graph.nodes.iter().find(|n| n.id == node_id) {
                        if node.node_type == NodeType::MacroInstance {
                            if let Some(macro_id) = node.definition_id.strip_prefix("macro:") {
                                let macro_id: String = macro_id.to_string();
                                // Read macro name from shared panel
                                let macro_name = canvas
                                    .panel
                                    .upgrade()
                                    .and_then(|p| {
                                        p.read(cx)
                                            .local_macros
                                            .iter()
                                            .find(|m| m.id == macro_id)
                                            .map(|m| m.name.clone())
                                    })
                                    .unwrap_or_else(|| "Macro".to_string());
                                canvas.last_click_time = None;
                                canvas.last_click_pos = None;
                                let win_handle = window.window_handle();
                                let panel_weak = canvas.panel.clone();
                                cx.defer(move |cx| {
                                    let _ = cx.update_window(win_handle, |_, window, cx| {
                                        if let Some(p) = panel_weak.upgrade() {
                                            p.update(cx, |panel, cx| {
                                                panel.open_local_macro(
                                                    macro_id.clone(),
                                                    macro_name.clone(),
                                                    window,
                                                    cx,
                                                );
                                            });
                                        }
                                    });
                                });
                                return;
                            }
                        }
                    }
                    canvas.last_click_time = None;
                    canvas.last_click_pos = None;
                } else {
                    canvas.last_click_time = Some(now);
                    canvas.last_click_pos = Some(gp);
                }

                if !canvas.graph.selected_nodes.contains(&node_id) {
                    if !event.modifiers.control {
                        canvas.graph.selected_nodes.clear();
                        canvas.graph.selected_comments.clear();
                    }
                    canvas.graph.selected_nodes.push(node_id.clone());
                }

                canvas.pending_drag_node = Some(node_id);
                canvas.pending_drag_start = Some(cp);
                cx.notify();
                return;
            }

            // Empty space — start selection drag
            if !event.modifiers.control {
                canvas.graph.selected_nodes.clear();
                canvas.graph.selected_comments.clear();
            }
            canvas.start_selection_drag(gp, event.modifiers.control, cx);
        });
    }
}

pub fn on_mouse_move(
    cx: &mut Context<GraphCanvasPanel>,
) -> impl Fn(&MouseMoveEvent, &mut Window, &mut App) {
    let entity = cx.entity().clone();
    move |event: &MouseMoveEvent, _window, cx| {
        entity.update(cx, |canvas, cx| {
            let cp = to_canvas(event.position, canvas);
            let mp = Point::new(cp.x, cp.y);

            // Threshold-detect right-drag → pan
            if let Some(right_start) = canvas.right_click_start {
                let dist =
                    ((mp.x - right_start.x).powi(2) + (mp.y - right_start.y).powi(2)).sqrt();
                if dist > canvas.right_click_threshold {
                    canvas.start_panning(right_start, cx);
                    canvas.right_click_start = None;
                }
            }

            // Commit pending node drag once past threshold
            if let Some(ref start) = canvas.pending_drag_start.clone() {
                let dist = ((mp.x - start.x).powi(2) + (mp.y - start.y).powi(2)).sqrt();
                if dist > canvas.drag_commit_threshold {
                    if let Some(node_id) = canvas.pending_drag_node.take() {
                        canvas.pending_drag_start = None;
                        let gp_start = to_graph(*start, canvas);
                        canvas.start_drag(node_id, gp_start, cx);
                    }
                }
            }

            let gp = to_graph(cp, canvas);

            if canvas.dragging_comment.is_some() {
                canvas.update_comment_drag(gp, cx);
            } else if canvas.resizing_comment.is_some() {
                canvas.update_comment_resize(gp, cx);
            } else if canvas.dragging_node.is_some() {
                canvas.update_drag(gp, cx);
            } else if canvas.dragging_connection.is_some() {
                canvas.update_connection_drag(mp, cx);
            } else if canvas.is_selecting() {
                canvas.update_selection_drag(gp, cx);
            } else if canvas.is_panning() {
                canvas.update_pan(mp, cx);
            }
        });
    }
}

pub fn on_mouse_up_left(
    cx: &mut Context<GraphCanvasPanel>,
) -> impl Fn(&MouseUpEvent, &mut Window, &mut App) {
    let entity = cx.entity().clone();
    move |event: &MouseUpEvent, _window, cx| {
        entity.update(cx, |canvas, cx| {
            let cp = to_canvas(event.position, canvas);
            let gp = to_graph(cp, canvas);

            if canvas.pending_drag_node.is_some() {
                canvas.pending_drag_node = None;
                canvas.pending_drag_start = None;
            }

            if canvas.dragging_comment.is_some() {
                canvas.end_comment_drag(cx);
            } else if canvas.resizing_comment.is_some() {
                canvas.end_comment_resize(cx);
            } else if canvas.dragging_node.is_some() {
                canvas.end_drag(cx);
            } else if canvas.dragging_variable.is_some() {
                canvas.finish_dragging_variable(gp, cx);
            } else if let Some(drag) = canvas.dragging_connection.clone() {
                if let Some((nid, pid)) =
                    hit_input_pin(cp, canvas, &drag.source_node, &drag.source_pin_type)
                {
                    canvas.complete_connection_on_pin(nid, pid, cx);
                } else {
                    canvas.popup_palette_graph_pos = Some(gp);
                    canvas.quick_palette_connection_source = Some(drag);
                    canvas.quick_palette_open = true;
                    canvas.quick_palette_focus_pending = true;
                    canvas.quick_palette_screen_pos = event.position;
                    canvas.dragging_connection = None;
                    cx.notify();
                }
            } else if canvas.is_selecting() {
                canvas.end_selection_drag(cx);
            } else if canvas.is_panning() {
                canvas.end_panning(cx);
            }
        });
    }
}

pub fn on_mouse_up_right(
    cx: &mut Context<GraphCanvasPanel>,
) -> impl Fn(&MouseUpEvent, &mut Window, &mut App) {
    let entity = cx.entity().clone();
    move |event: &MouseUpEvent, _window, cx| {
        entity.update(cx, |canvas, cx| {
            let was_click = canvas.right_click_start.is_some() && !canvas.is_panning();

            if canvas.is_panning() {
                canvas.end_panning(cx);
            }

            if was_click {
                let cp = to_canvas(event.position, canvas);
                let gp = to_graph(cp, canvas);

                if let Some((nid, pid)) = hit_any_pin(cp, canvas) {
                    canvas.pin_context_menu = Some((nid, pid, event.position));
                    canvas.quick_palette_open = false;
                } else if let Some(node_id) = hit_node(gp, canvas).map(str::to_owned) {
                    if !canvas.graph.selected_nodes.contains(&node_id) {
                        canvas.select_node(Some(node_id.clone()), cx);
                    }
                    canvas.node_context_menu = Some((node_id, event.position));
                    canvas.quick_palette_open = false;
                } else {
                    canvas.quick_palette_open = true;
                    canvas.quick_palette_focus_pending = true;
                    canvas.quick_palette_screen_pos = event.position;
                    canvas.node_context_menu = None;
                    canvas.pin_context_menu = None;
                }
                cx.notify();
            }

            canvas.right_click_start = None;
        });
    }
}

pub fn on_scroll_wheel(
    cx: &mut Context<GraphCanvasPanel>,
) -> impl Fn(&ScrollWheelEvent, &mut Window, &mut App) {
    let entity = cx.entity().clone();
    move |event: &ScrollWheelEvent, _window, cx| {
        entity.update(cx, |canvas, cx| {
            let delta_y = match event.delta {
                ScrollDelta::Pixels(p) => p.y.as_f32(),
                ScrollDelta::Lines(l) => l.y * 20.0,
            };
            let cp = to_canvas(event.position, canvas);
            let element_pos = Point::new(px(cp.x), px(cp.y));
            canvas.handle_zoom(delta_y, element_pos, cx);
        });
    }
}

pub fn on_key_down(
    cx: &mut Context<GraphCanvasPanel>,
) -> impl Fn(&KeyDownEvent, &mut Window, &mut App) {
    let entity = cx.entity().clone();
    move |event: &KeyDownEvent, window, cx| {
        entity.update(cx, |canvas, cx| {
            let key = event.keystroke.key.to_lowercase();
            let has_copy_paste_modifier =
                event.keystroke.modifiers.control || event.keystroke.modifiers.platform;

            if canvas.editing_comment.is_some() {
                if key == "escape" {
                    canvas.editing_comment = None;
                    cx.notify();
                } else if key == "enter" && event.keystroke.modifiers.control {
                    canvas.finish_comment_editing(cx);
                }
                return;
            }

            match key.as_str() {
                "escape" => {
                    canvas.node_context_menu = None;
                    canvas.pin_context_menu = None;
                    if canvas.variable_drop_menu_position.is_some() {
                        canvas.variable_drop_menu_position = None;
                    } else if canvas.dragging_connection.is_some() {
                        canvas.cancel_connection_drag(cx);
                    }
                    cx.notify();
                }
                "delete" | "backspace" => canvas.delete_selected_nodes(cx),
                "c" if !has_copy_paste_modifier => {
                    canvas.create_comment_at_center(window, cx);
                }
                "c" if has_copy_paste_modifier => {
                    canvas.copy_selected_entities(cx);
                }
                "v" if has_copy_paste_modifier => {
                    canvas.paste_entities(window, cx);
                }
                "z" if event.keystroke.modifiers.control && event.keystroke.modifiers.shift => {
                    canvas.redo(cx);
                }
                "z" if event.keystroke.modifiers.control => {
                    canvas.undo(cx);
                }
                "y" if event.keystroke.modifiers.control => {
                    canvas.redo(cx);
                }
                "f9" => {
                    if let Some(node_id) = canvas.graph.selected_nodes.first().cloned() {
                        canvas.toggle_breakpoint(node_id, cx);
                    }
                }
                "f5" if event.keystroke.modifiers.shift => {
                    canvas.debug_stop(cx);
                }
                "f5" => {
                    canvas.debug_continue(cx);
                }
                "f10" if event.keystroke.modifiers.shift => {
                    canvas.debug_step_backward(cx);
                }
                "f10" => {
                    canvas.debug_step_forward(cx);
                }
                _ => {}
            }
        });
    }
}
