//! Rendering of a tab's canvas: the Graphy canvas element plus the
//! Blueprint-only layers over it (comment title editor, breakpoint badges, debug HUD,
//! macro pin editor). The context menu and node picker belong to the canvas.

use gpui::prelude::*;
use gpui::*;

use crate::editor::graph_canvas_panel::GraphCanvasPanel;

impl Render for GraphCanvasPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme::graph_theme(cx);
        if self.canvas.read(cx).theme() != &theme {
            self.canvas.update(cx, |canvas, cx| canvas.set_theme(theme, cx));
        }
        let theme = crate::theme::graph_theme(cx);
        if self.canvas.read(cx).theme() != &theme {
            self.canvas.update(cx, |canvas, cx| canvas.set_theme(theme, cx));
        }
        let mut root = div()
            .relative()
            .size_full()
            .on_action(cx.listener(|canvas, _: &plugin_editor_api::Undo, _, cx| canvas.undo(cx)))
            .on_action(cx.listener(|canvas, _: &plugin_editor_api::Redo, _, cx| canvas.redo(cx)));

        // The shared canvas, wrapped in drop areas so sidebars can drag items onto it.
        let canvas = div().size_full().child(self.canvas.clone());
        let canvas = ui::drop_area::DropArea::<crate::features::prefabs::ComponentDrag>::new(
            "blueprint-canvas-component-drop",
        )
        .can_accept(|_| true)
        .on_drop(cx.listener(
            |canvas, payload: &crate::features::prefabs::ComponentDrag, window, cx| {
                let mouse_pos = window.mouse_position();
                canvas.finish_dragging_component(payload.clone(), mouse_pos, cx);
            },
        ))
        .child(
            ui::drop_area::DropArea::<crate::features::macros::MacroDrag>::new(
                "blueprint-canvas-macro-drop",
            )
            .can_accept(|_| true)
            .on_drop(cx.listener(
                |canvas, payload: &crate::features::macros::MacroDrag, window, cx| {
                    canvas.dragging_macro = Some(payload.clone());
                    let mouse_pos = window.mouse_position();
                    canvas.finish_dragging_macro(mouse_pos, cx);
                },
            ))
            .child(canvas),
        );
        root = root.child(canvas);

        root = root
            .child(self.render_breakpoint_badges(cx))
            .child(Self::render_debug_hud(self, cx))
            .child(Self::render_macro_pin_editor(self, cx))
            .child(self.render_comment_title_editor(cx));
        root
    }
}

impl GraphCanvasPanel {
    /// Title editor over the comment being renamed.
    fn render_comment_title_editor(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(index) = self.editing_comment else {
            return div().into_any_element();
        };
        let canvas = self.canvas.read(cx);
        let Some(comment) = canvas.graph().comments.get(index) else {
            return div().into_any_element();
        };
        let (_, zoom) = canvas.view();
        let title_h = (30.0 * zoom).clamp(18.0, 44.0);
        let title_w = (comment.size.0 as f32 * zoom - 24.0 * zoom).max(80.0);
        let origin = canvas.graph_to_window(graphy_render::Vec2::new(
            comment.position.x as f32,
            comment.position.y as f32,
        ));
        let position = Point::new(origin.x + px(12.0 * zoom), origin.y + px(6.0 * zoom));
        deferred(
            anchored()
                .position(position)
                .snap_to_window_with_margin(px(4.0))
                .anchor(gpui::Corner::TopLeft)
                .child(
                    div().occlude().w(px(title_w)).h(px(title_h * 0.68)).child(
                        ui::input::TextInput::new(&self.comment_text_input)
                            .appearance(false)
                            .bordered(false)
                            .focus_bordered(false),
                    ),
                ),
        )
        .with_priority(2)
        .into_any_element()
    }

    /// Red stop badges over breakpointed nodes, and a ring on the paused one.
    fn render_breakpoint_badges(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.breakpoints.is_empty() {
            return div().into_any_element();
        }
        let paused_node = self
            .debug_session
            .as_ref()
            .and_then(|s| s.current())
            .map(|f| f.node_id.clone());
        let canvas = self.canvas.read(cx);
        let (_, zoom) = canvas.view();
        let mut container = div();
        for id in &self.breakpoints {
            let Some(node) = canvas.scene().node(id) else {
                continue;
            };
            let top_left = canvas.graph_to_window(node.pos);
            let is_current = paused_node.as_deref() == Some(id.as_str());
            container = container.child(
                deferred(
                    anchored()
                        .position(Point::new(top_left.x - px(4.0), top_left.y - px(4.0)))
                        .anchor(gpui::Corner::TopLeft)
                        .child(
                            div()
                                .w(px(18.0))
                                .h(px(18.0))
                                .rounded(px(3.0))
                                .bg(if is_current {
                                    gpui::rgba(0xFF4400FF)
                                } else {
                                    gpui::rgba(0xCC1111FF)
                                })
                                .border_1()
                                .border_color(gpui::rgba(0xFF8888FF))
                                .flex()
                                .items_center()
                                .justify_center()
                                .shadow_md()
                                .child(
                                    div()
                                        .text_size(px(10.0))
                                        .text_color(gpui::white())
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .child("⏹"),
                                ),
                        ),
                )
                .with_priority(3),
            );
            if is_current {
                let size = node.layout.size;
                container = container.child(
                    deferred(
                        anchored()
                            .position(Point::new(top_left.x - px(2.0), top_left.y - px(2.0)))
                            .anchor(gpui::Corner::TopLeft)
                            .child(
                                div()
                                    .w(px(size.x * zoom + 4.0))
                                    .h(px(size.y * zoom + 4.0))
                                    .rounded(px(8.0))
                                    .border_2()
                                    .border_color(gpui::rgba(0xFF440088))
                                    .shadow_lg(),
                            ),
                    )
                    .with_priority(2),
                );
            }
        }
        container.into_any_element()
    }
}
