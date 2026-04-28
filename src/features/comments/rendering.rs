//!
//! This module handles all comment rendering including:
//! - Comment boxes with background color and border
//! - Text display and editing interface
//! - Resize handles (8-directional)
//! - Color picker integration for customization
//! - Selection highlighting
//! Comment rendering - visual representation of comment boxes

use super::operations::ResizeHandle;
use crate::core::types::BlueprintComment;
use crate::editor::panel::BlueprintEditorPanel;
use crate::rendering::graph::NodeGraphRenderer;
use gpui::prelude::FluentBuilder;
use gpui::*;
use ui::{Colorize, PixelsExt};

fn comment_body_color(color: Hsla) -> Hsla {
    Hsla {
        a: color.a.max(0.45),
        ..color
    }
}

/// Comment rendering methods
///
/// These methods will be implemented on the NodeGraphRenderer or similar rendering struct.
/// For now, they are defined as standalone functions that can be called from the renderer.

/// Render all comments in the graph
///
/// This creates the comment layer that sits behind nodes but above connections.
/// Comments are rendered with their selection state and z-ordering.
///
/// # Arguments
/// * `panel` - The editor panel containing the graph state
/// * `cx` - GPUI context for rendering
///
/// # Returns
/// An element containing all rendered comments
pub fn render_all(
    panel: &mut BlueprintEditorPanel,
    cx: &mut Context<BlueprintEditorPanel>,
) -> impl IntoElement {
    let visible_comments: Vec<BlueprintComment> = panel
        .graph
        .comments
        .iter()
        .map(|comment| {
            let mut comment = comment.clone();
            comment.is_selected = panel.graph.selected_comments.contains(&comment.id);
            comment
        })
        .collect();

    div().absolute().inset_0().children(
        visible_comments
            .into_iter()
            .map(|comment| render_comment(&comment, panel, cx)),
    )
}

/// Render a single comment box
///
/// Renders a comment with:
/// - Background color (semi-transparent)
/// - Border (highlighted if selected)
/// - Text content (editable on double-click)
/// - 8 resize handles (corners and edges)
/// - Color picker button (when selected)
///
/// # Arguments
/// * `comment` - The comment to render
/// * `panel` - The editor panel for accessing state
/// * `cx` - GPUI context for rendering
///
/// # Returns
/// An element representing the comment box
pub fn render_comment(
    comment: &BlueprintComment,
    panel: &mut BlueprintEditorPanel,
    cx: &mut Context<BlueprintEditorPanel>,
) -> AnyElement {
    let graph_pos = NodeGraphRenderer::graph_to_screen_pos(comment.position, &panel.graph);
    let comment_id = comment.id.clone();
    let is_dragging = panel.dragging_comment.as_ref() == Some(&comment.id);
    let is_resizing = panel.resizing_comment.as_ref().map(|(id, _)| id) == Some(&comment.id);

    let scaled_width = comment.size.width * panel.graph.zoom_level;
    let scaled_height = comment.size.height * panel.graph.zoom_level;
    let resize_handle_size = 12.0 * panel.graph.zoom_level;

    div()
        .absolute()
        .left(px(graph_pos.x))
        .top(px(graph_pos.y))
        .w(px(scaled_width))
        .h(px(scaled_height))
        .child(
            div()
                .size_full()
                .bg(comment_body_color(comment.color))
                .border_2()
                .border_color(if comment.is_selected {
                    gpui::yellow()
                } else {
                    comment.color.lighten(0.2)
                })
                .rounded(px(8.0 * panel.graph.zoom_level))
                .when(is_dragging || is_resizing, |style| style.opacity(0.8))
                .shadow_md()
                .overflow_hidden()
                .child({
                    let is_editing = panel.editing_comment.as_ref() == Some(&comment.id);

                    if is_editing {
                        div()
                            .p(px(12.0 * panel.graph.zoom_level))
                            .size_full()
                            .font_family("JetBrainsMono-Regular")
                            .font_weight(gpui::FontWeight::default())
                            .child(ui::input::TextInput::new(&panel.comment_text_input))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|_panel, _event: &MouseDownEvent, _window, cx| {
                                    cx.stop_propagation();
                                }),
                            )
                            .on_mouse_move(cx.listener(
                                |_panel, _event: &MouseMoveEvent, _window, cx| {
                                    cx.stop_propagation();
                                },
                            ))
                            .into_any_element()
                    } else {
                        div()
                            .p(px(12.0 * panel.graph.zoom_level))
                            .size_full()
                            .text_size(px(14.0 * panel.graph.zoom_level))
                            .text_color(gpui::white())
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(comment.text.clone())
                            .on_mouse_down(gpui::MouseButton::Left, {
                                let comment_id = comment_id.clone();
                                cx.listener(move |panel, event: &MouseDownEvent, window, cx| {
                                    cx.stop_propagation();

                                    if !panel.graph.selected_comments.contains(&comment_id) {
                                        panel.graph.selected_comments.clear();
                                        panel.graph.selected_comments.push(comment_id.clone());
                                    }

                                    let now = std::time::Instant::now();
                                    let should_edit = if let Some(last_click) =
                                        panel.last_click_time
                                    {
                                        if now.duration_since(last_click).as_millis() < 500 {
                                            if let Some(last_pos) = panel.last_click_pos {
                                                let element_pos =
                                                    NodeGraphRenderer::window_to_graph_element_pos(
                                                        event.position,
                                                        panel,
                                                    );
                                                let current_pos = Point::new(
                                                    element_pos.x.as_f32(),
                                                    element_pos.y.as_f32(),
                                                );
                                                let distance = ((current_pos.x - last_pos.x)
                                                    .powi(2)
                                                    + (current_pos.y - last_pos.y).powi(2))
                                                .sqrt();
                                                distance < 10.0
                                            } else {
                                                false
                                            }
                                        } else {
                                            false
                                        }
                                    } else {
                                        false
                                    };

                                    if should_edit {
                                        panel.editing_comment = Some(comment_id.clone());
                                        if let Some(comment) =
                                            panel.graph.comments.iter().find(|c| c.id == comment_id)
                                        {
                                            panel.comment_text_input.update(cx, |state, cx| {
                                                state.set_value(comment.text.clone(), window, cx);
                                            });
                                        }
                                        panel.last_click_time = None;
                                    } else {
                                        let element_pos =
                                            NodeGraphRenderer::window_to_graph_element_pos(
                                                event.position,
                                                panel,
                                            );
                                        let graph_pos = NodeGraphRenderer::screen_to_graph_pos(
                                            element_pos,
                                            &panel.graph,
                                        );

                                        if let Some(comment) =
                                            panel.graph.comments.iter().find(|c| c.id == comment_id)
                                        {
                                            panel.dragging_comment = Some(comment_id.clone());
                                            panel.drag_offset = Point::new(
                                                graph_pos.x - comment.position.x,
                                                graph_pos.y - comment.position.y,
                                            );
                                        }

                                        let current_pos = Point::new(
                                            element_pos.x.as_f32(),
                                            element_pos.y.as_f32(),
                                        );
                                        panel.last_click_time = Some(now);
                                        panel.last_click_pos = Some(current_pos);
                                    }

                                    cx.notify();
                                })
                            })
                            .into_any_element()
                    }
                })
                .children([
                    render_resize_handle(
                        ResizeHandle::TopLeft,
                        &comment_id,
                        resize_handle_size,
                        panel,
                        cx,
                    ),
                    render_resize_handle(
                        ResizeHandle::TopRight,
                        &comment_id,
                        resize_handle_size,
                        panel,
                        cx,
                    ),
                    render_resize_handle(
                        ResizeHandle::BottomLeft,
                        &comment_id,
                        resize_handle_size,
                        panel,
                        cx,
                    ),
                    render_resize_handle(
                        ResizeHandle::BottomRight,
                        &comment_id,
                        resize_handle_size,
                        panel,
                        cx,
                    ),
                    render_resize_handle(
                        ResizeHandle::Top,
                        &comment_id,
                        resize_handle_size,
                        panel,
                        cx,
                    ),
                    render_resize_handle(
                        ResizeHandle::Bottom,
                        &comment_id,
                        resize_handle_size,
                        panel,
                        cx,
                    ),
                    render_resize_handle(
                        ResizeHandle::Left,
                        &comment_id,
                        resize_handle_size,
                        panel,
                        cx,
                    ),
                    render_resize_handle(
                        ResizeHandle::Right,
                        &comment_id,
                        resize_handle_size,
                        panel,
                        cx,
                    ),
                ])
                .when(comment.is_selected, |this| {
                    this.child(
                        div()
                            .absolute()
                            .top(px(8.0 * panel.graph.zoom_level))
                            .right(px(8.0 * panel.graph.zoom_level))
                            .child(
                                ui::color_picker::ColorPicker::new(
                                    comment
                                        .color_picker_state
                                        .as_ref()
                                        .expect("color picker state"),
                                )
                                .size(ui::Size::Small),
                            )
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|_panel, _event: &MouseDownEvent, _window, cx| {
                                    cx.stop_propagation();
                                }),
                            ),
                    )
                }),
        )
        .into_any_element()
}

/// Render a resize handle for a comment
///
/// Creates an invisible drag handle for resizing comments. The handle has:
/// - Transparent background for minimal visual clutter
/// - Appropriate cursor style for the resize direction
/// - Mouse down handler to start resize operation
///
/// # Arguments
/// * `handle` - The handle position/type
/// * `comment_id` - ID of the comment being resized
/// * `size` - Size of the handle in pixels (scaled with zoom)
/// * `panel` - The editor panel for state access
/// * `cx` - GPUI context for rendering
///
/// # Returns
/// An element representing the resize handle
pub fn render_resize_handle(
    handle: ResizeHandle,
    comment_id: &str,
    size: f32,
    _panel: &BlueprintEditorPanel,
    cx: &mut Context<BlueprintEditorPanel>,
) -> impl IntoElement {
    let (left, top, cursor) = match handle {
        ResizeHandle::TopLeft => (
            Some(px(0.0)),
            Some(px(0.0)),
            CursorStyle::ResizeUpLeftDownRight,
        ),
        ResizeHandle::TopRight => (None, Some(px(0.0)), CursorStyle::ResizeUpRightDownLeft),
        ResizeHandle::BottomLeft => (Some(px(0.0)), None, CursorStyle::ResizeUpRightDownLeft),
        ResizeHandle::BottomRight => (None, None, CursorStyle::ResizeUpLeftDownRight),
        ResizeHandle::Top => (None, Some(px(0.0)), CursorStyle::ResizeUpDown),
        ResizeHandle::Bottom => (None, None, CursorStyle::ResizeUpDown),
        ResizeHandle::Left => (Some(px(0.0)), None, CursorStyle::ResizeLeftRight),
        ResizeHandle::Right => (None, None, CursorStyle::ResizeLeftRight),
    };

    let comment_id = comment_id.to_string();

    div()
        .absolute()
        .when_some(left, |this, l| this.left(l))
        .when(left.is_none(), |this| this.right(px(0.0)))
        .when_some(top, |this, t| this.top(t))
        .when(top.is_none(), |this| this.bottom(px(0.0)))
        .when(
            matches!(handle, ResizeHandle::Top | ResizeHandle::Bottom),
            |this| this.left_0().right_0().h(px(size)),
        )
        .when(
            matches!(handle, ResizeHandle::Left | ResizeHandle::Right),
            |this| this.top_0().bottom_0().w(px(size)),
        )
        .when(
            !matches!(
                handle,
                ResizeHandle::Top | ResizeHandle::Bottom | ResizeHandle::Left | ResizeHandle::Right
            ),
            |this| this.size(px(size)),
        )
        .bg(gpui::transparent_black())
        .cursor(cursor)
        .on_mouse_down(gpui::MouseButton::Left, {
            cx.listener(move |panel, event: &MouseDownEvent, _window, cx| {
                cx.stop_propagation();

                let element_pos =
                    NodeGraphRenderer::window_to_graph_element_pos(event.position, panel);
                let graph_pos = NodeGraphRenderer::screen_to_graph_pos(element_pos, &panel.graph);

                panel.resizing_comment = Some((comment_id.clone(), handle.clone()));
                panel.drag_offset = graph_pos;
                cx.notify();
            })
        })
}

// ============================================================================
// Reference Implementation
// ============================================================================
//
// The complete implementation from src_old/node_graph.rs (lines 405-643)
// is provided below for reference during migration:

/*
fn render_comments(
    panel: &mut BlueprintEditorPanel,
    cx: &mut Context<BlueprintEditorPanel>,
) -> impl IntoElement {
    let visible_comments: Vec<BlueprintComment> = panel
        .graph
        .comments
        .iter()
        .map(|comment| {
            let mut comment = comment.clone();
            comment.is_selected = panel.graph.selected_comments.contains(&comment.id);
            comment
        })
        .collect();

    div().absolute().inset_0().children(
        visible_comments
            .into_iter()
            .map(|comment| Self::render_comment(&comment, panel, cx)),
    )
}

fn render_comment(
    comment: &BlueprintComment,
    panel: &mut BlueprintEditorPanel,
    cx: &mut Context<BlueprintEditorPanel>,
) -> AnyElement {
    let graph_pos = Self::graph_to_screen_pos(comment.position, &panel.graph);
    let comment_id = comment.id.clone();
    let is_dragging = panel.dragging_comment.as_ref() == Some(&comment.id);
    let is_resizing = panel.resizing_comment.as_ref().map(|(id, _)| id) == Some(&comment.id);

    // Scale comment size with zoom level
    let scaled_width = comment.size.width * panel.graph.zoom_level;
    let scaled_height = comment.size.height * panel.graph.zoom_level;

    let resize_handle_size = 12.0 * panel.graph.zoom_level;

    div()
        .absolute()
        .left(px(graph_pos.x))
        .top(px(graph_pos.y))
        .w(px(scaled_width))
        .h(px(scaled_height))
        .child(
            div()
                .size_full()
                .bg(comment_body_color(comment.color))
                .border_2()
                .border_color(if comment.is_selected {
                    gpui::yellow()
                } else {
                    comment.color.lighten(0.2)
                })
                .rounded(px(8.0 * panel.graph.zoom_level))
                .when(is_dragging || is_resizing, |style| style.opacity(0.8))
                .shadow_md()
                .overflow_hidden()
                .child({
                    let is_editing = panel.editing_comment.as_ref() == Some(&comment.id);

                    if is_editing {
                        // Show text input for editing
                        div()
                            .p(px(12.0 * panel.graph.zoom_level))
                            .size_full()
                            .font_family("JetBrainsMono-Regular")
                            .font_weight(gpui::FontWeight::default())
                            .child(
                                ui::input::TextInput::new(&panel.comment_text_input)
                            )
                            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|_panel, _event: &MouseDownEvent, _window, cx| {
                                cx.stop_propagation();
                            }))
                            .on_mouse_move(cx.listener(|_panel, _event: &MouseMoveEvent, _window, cx| {
                                cx.stop_propagation();
                            }))
                            .into_any_element()
                    } else {
                        // Show static text
                        div()
                            .p(px(12.0 * panel.graph.zoom_level))
                            .size_full()
                            .text_size(px(14.0 * panel.graph.zoom_level))
                            .text_color(gpui::white())
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(comment.text.clone())
                            .on_mouse_down(gpui::MouseButton::Left, {
                                let comment_id = comment_id.clone();
                                cx.listener(move |panel, event: &MouseDownEvent, _window, cx| {
                                    cx.stop_propagation();

                                    // Select comment
                                    if !panel.graph.selected_comments.contains(&comment_id) {
                                        panel.graph.selected_comments.clear();
                                        panel.graph.selected_comments.push(comment_id.clone());
                                    }

                                    // Check for double-click to start editing
                                    let now = std::time::Instant::now();
                                    let should_edit = if let Some(last_click) = panel.last_click_time {
                                        if now.duration_since(last_click).as_millis() < 500 {
                                            if let Some(last_pos) = panel.last_click_pos {
                                                let element_pos = Self::window_to_graph_element_pos(event.position, panel);
                                                let current_pos = Point::new(element_pos.x.as_f32(), element_pos.y.as_f32());
                                                let distance = ((current_pos.x - last_pos.x).powi(2) + (current_pos.y - last_pos.y).powi(2)).sqrt();
                                                distance < 10.0
                                            } else {
                                                false
                                            }
                                        } else {
                                            false
                                        }
                                    } else {
                                        false
                                    };

                                    if should_edit {
                                        // Start editing
                                        panel.editing_comment = Some(comment_id.clone());

                                        // Load current comment text into input
                                        if let Some(comment) = panel.graph.comments.iter().find(|c| c.id == comment_id) {
                                            panel.comment_text_input.update(cx, |state, cx| {
                                                state.set_value(comment.text.clone(), _window, cx);
                                            });
                                        }

                                        panel.last_click_time = None;
                                    } else {
                                        // Start dragging
                                        let element_pos = Self::window_to_graph_element_pos(event.position, panel);
                                        let graph_pos = Self::screen_to_graph_pos(element_pos, &panel.graph);

                                        // Calculate drag offset (same as node dragging)
                                        if let Some(comment) = panel.graph.comments.iter().find(|c| c.id == comment_id) {
                                            panel.dragging_comment = Some(comment_id.clone());
                                            panel.drag_offset = Point::new(
                                                graph_pos.x - comment.position.x,
                                                graph_pos.y - comment.position.y,
                                            );
                                        }

                                        // Update click tracking
                                        let current_pos = Point::new(element_pos.x.as_f32(), element_pos.y.as_f32());
                                        panel.last_click_time = Some(now);
                                        panel.last_click_pos = Some(current_pos);
                                    }

                                    cx.notify();
                                })
                            })
                            .into_any_element()
                    }
                })
                // Resize handles
                .children([
                    Self::render_resize_handle(ResizeHandle::TopLeft, &comment_id, resize_handle_size, panel, cx),
                    Self::render_resize_handle(ResizeHandle::TopRight, &comment_id, resize_handle_size, panel, cx),
                    Self::render_resize_handle(ResizeHandle::BottomLeft, &comment_id, resize_handle_size, panel, cx),
                    Self::render_resize_handle(ResizeHandle::BottomRight, &comment_id, resize_handle_size, panel, cx),
                    Self::render_resize_handle(ResizeHandle::Top, &comment_id, resize_handle_size, panel, cx),
                    Self::render_resize_handle(ResizeHandle::Bottom, &comment_id, resize_handle_size, panel, cx),
                    Self::render_resize_handle(ResizeHandle::Left, &comment_id, resize_handle_size, panel, cx),
                    Self::render_resize_handle(ResizeHandle::Right, &comment_id, resize_handle_size, panel, cx),
                ])
                // Color picker button (only when selected)
                .when(comment.is_selected, |this| {
                    this.child(
                        div()
                            .absolute()
                            .top(px(8.0 * panel.graph.zoom_level))
                            .right(px(8.0 * panel.graph.zoom_level))
                            .child(
                                ui::color_picker::ColorPicker::new(
                                    comment.color_picker_state.as_ref().expect("Color picker state")
                                )
                                .size(ui::Size::Small)
                            )
                            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|_panel, _event: &MouseDownEvent, _window, cx| {
                                cx.stop_propagation();
                            }))
                    )
                }),
        )
        .into_any_element()
}

fn render_resize_handle(
    handle: ResizeHandle,
    comment_id: &str,
    size: f32,
    panel: &BlueprintEditorPanel,
    cx: &mut Context<BlueprintEditorPanel>,
) -> impl IntoElement {
    let (left, top, cursor) = match handle {
        ResizeHandle::TopLeft => (Some(px(0.0)), Some(px(0.0)), CursorStyle::ResizeUpLeftDownRight),
        ResizeHandle::TopRight => (None, Some(px(0.0)), CursorStyle::ResizeUpRightDownLeft),
        ResizeHandle::BottomLeft => (Some(px(0.0)), None, CursorStyle::ResizeUpRightDownLeft),
        ResizeHandle::BottomRight => (None, None, CursorStyle::ResizeUpLeftDownRight),
        ResizeHandle::Top => (None, Some(px(0.0)), CursorStyle::ResizeUpDown),
        ResizeHandle::Bottom => (None, None, CursorStyle::ResizeUpDown),
        ResizeHandle::Left => (Some(px(0.0)), None, CursorStyle::ResizeLeftRight),
        ResizeHandle::Right => (None, None, CursorStyle::ResizeLeftRight),
    };

    let comment_id = comment_id.to_string();

    div()
        .absolute()
        .when_some(left, |this, l| this.left(l))
        .when(left.is_none(), |this| this.right(px(0.0)))
        .when_some(top, |this, t| this.top(t))
        .when(top.is_none(), |this| this.bottom(px(0.0)))
        .when(matches!(handle, ResizeHandle::Top | ResizeHandle::Bottom), |this| {
            this.left_0().right_0().h(px(size))
        })
        .when(matches!(handle, ResizeHandle::Left | ResizeHandle::Right), |this| {
            this.top_0().bottom_0().w(px(size))
        })
        .when(!matches!(handle, ResizeHandle::Top | ResizeHandle::Bottom | ResizeHandle::Left | ResizeHandle::Right), |this| {
            this.size(px(size))
        })
        .bg(gpui::transparent_black())
        .cursor(cursor)
        .on_mouse_down(gpui::MouseButton::Left, {
            cx.listener(move |panel, event: &MouseDownEvent, _window, cx| {
                cx.stop_propagation();

                let element_pos = Self::window_to_graph_element_pos(event.position, panel);
                let graph_pos = Self::screen_to_graph_pos(element_pos, &panel.graph);

                panel.resizing_comment = Some((comment_id.clone(), handle.clone()));
                panel.drag_offset = graph_pos;

                cx.notify();
            })
        })
}
*/
