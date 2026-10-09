use super::*;

impl NodeGraphRenderer {
    // ── Node context menu ─────────────────────────────────────────────────────

    pub(super) fn render_node_context_menu(
        canvas: &GraphCanvasPanel,
        cx: &mut Context<GraphCanvasPanel>,
    ) -> AnyElement {
        let Some((ref node_id, pos)) = canvas.node_context_menu else {
            return div().into_any_element();
        };
        let node_id = node_id.clone();
        let has_bp = canvas.has_breakpoint(&node_id);
        let is_collapsed_graph = canvas
            .graph
            .nodes
            .iter()
            .find(|node| node.id == node_id)
            .and_then(|node| {
                canvas.panel.upgrade().map(|panel| {
                    crate::core::subgraph_ref::SubGraphReference::decode(
                        &node.definition_id,
                        &panel.read(cx).subgraphs,
                    )
                    .is_some_and(|reference| {
                        reference.kind == blueprint_graph::SubGraphKind::Collapsed
                    })
                })
            })
            .unwrap_or(false);
        let bp_label = if has_bp {
            "Remove Breakpoint"
        } else {
            "Add Breakpoint  ⏹"
        };

        let pe = cx.entity().clone();
        let pe2 = pe.clone();
        let pe3 = pe.clone();
        let pe4 = pe.clone();
        let pe5 = pe.clone();
        let nid_dup = node_id.clone();
        let nid_copy = node_id.clone();
        let nid_del = node_id.clone();
        let nid_bp = node_id.clone();

        deferred(
            anchored()
                .position(pos)
                .snap_to_window_with_margin(px(8.0))
                .anchor(gpui::Corner::TopLeft)
                .child(
                    div()
                        .occlude()
                        .w(px(200.0))
                        .bg(cx.theme().popover)
                        .border_1()
                        .border_color(cx.theme().border)
                        .shadow_lg()
                        .rounded(px(6.0))
                        .py(px(4.0))
                        // ── Breakpoint section ─────────────────────────────────
                        .child(Self::menu_item_colored(
                            bp_label,
                            if has_bp {
                                gpui::rgba(0xFF9999FF)
                            } else {
                                gpui::rgba(0xFF6666FF)
                            },
                            cx,
                            {
                                let pe = pe4.clone();
                                move |_, _, cx| {
                                    pe.update(cx, |canvas, cx| {
                                        canvas.toggle_breakpoint(nid_bp.clone(), cx);
                                        canvas.node_context_menu = None;
                                        cx.notify();
                                    });
                                }
                            },
                        ))
                        .child(Self::menu_divider(cx))
                        // ── Standard edit actions ──────────────────────────────
                        .child(Self::menu_item("Duplicate Node", cx, {
                            let pe = pe.clone();
                            move |_, _, cx| {
                                pe.update(cx, |canvas, cx| {
                                    canvas.duplicate_node(nid_dup.clone(), cx);
                                    canvas.node_context_menu = None;
                                    cx.notify();
                                });
                            }
                        }))
                        .child(Self::menu_item("Copy Node", cx, {
                            let pe = pe2.clone();
                            move |_, _, cx| {
                                pe.update(cx, |canvas, cx| {
                                    canvas.copy_node(nid_copy.clone(), cx);
                                    canvas.node_context_menu = None;
                                    cx.notify();
                                });
                            }
                        }))
                        .child(Self::menu_item(
                            if is_collapsed_graph {
                                "Uncollapse Node"
                            } else {
                                "Collapse to Node"
                            },
                            cx,
                            {
                                let pe = pe5.clone();
                                let nid = node_id.clone();
                                move |_, window, cx| {
                                    pe.update(cx, |canvas, cx| {
                                        if is_collapsed_graph {
                                            canvas.expand_collapsed_node(nid.clone(), window, cx);
                                        } else {
                                            canvas.collapse_selected_nodes(cx);
                                        }
                                        canvas.node_context_menu = None;
                                        cx.notify();
                                    });
                                }
                            },
                        ))
                        .child(Self::menu_divider(cx))
                        .child(Self::menu_item("Delete Node", cx, {
                            let pe = pe3.clone();
                            move |_, _, cx| {
                                pe.update(cx, |canvas, cx| {
                                    canvas.delete_node(nid_del.clone(), cx);
                                    canvas.node_context_menu = None;
                                    cx.notify();
                                });
                            }
                        }))
                        .on_mouse_down_out(move |_, _, cx| {
                            pe.update(cx, |canvas, cx| {
                                canvas.node_context_menu = None;
                                cx.notify();
                            });
                        }),
                ),
        )
        .with_priority(2)
        .into_any_element()
    }

    // ── Pin context menu ──────────────────────────────────────────────────────

    pub(super) fn render_pin_context_menu(
        canvas: &GraphCanvasPanel,
        cx: &mut Context<GraphCanvasPanel>,
    ) -> AnyElement {
        let Some((ref node_id, ref pin_id, pos)) = canvas.pin_context_menu else {
            return div().into_any_element();
        };
        let node_id = node_id.clone();
        let pin_id = pin_id.clone();
        let pe = cx.entity().clone();
        let pe2 = pe.clone();

        deferred(
            anchored()
                .position(pos)
                .snap_to_window_with_margin(px(8.0))
                .anchor(gpui::Corner::TopLeft)
                .child(
                    div()
                        .occlude()
                        .w(px(180.0))
                        .bg(cx.theme().popover)
                        .border_1()
                        .border_color(cx.theme().border)
                        .shadow_lg()
                        .rounded(px(6.0))
                        .py(px(4.0))
                        .child(Self::menu_item("Disconnect Pin", cx, {
                            let pe = pe.clone();
                            move |_, _, cx| {
                                pe.update(cx, |canvas, cx| {
                                    canvas.disconnect_pin(node_id.clone(), pin_id.clone(), cx);
                                    canvas.pin_context_menu = None;
                                    cx.notify();
                                });
                            }
                        }))
                        .on_mouse_down_out(move |_, _, cx| {
                            pe2.update(cx, |canvas, cx| {
                                canvas.pin_context_menu = None;
                                cx.notify();
                            });
                        }),
                ),
        )
        .with_priority(2)
        .into_any_element()
    }

    // ── Shared menu primitives ────────────────────────────────────────────────

    fn menu_item(
        label: &str,
        cx: &mut Context<GraphCanvasPanel>,
        handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> impl IntoElement {
        div()
            .px(px(12.0))
            .py(px(6.0))
            .text_sm()
            .text_color(cx.theme().popover_foreground)
            .cursor_pointer()
            .hover(|s| s.bg(cx.theme().accent.opacity(0.12)))
            .on_mouse_down(gpui::MouseButton::Left, handler)
            .child(label.to_string())
    }

    fn menu_item_colored(
        label: &str,
        color: gpui::Rgba,
        cx: &mut Context<GraphCanvasPanel>,
        handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> impl IntoElement {
        div()
            .px(px(12.0))
            .py(px(6.0))
            .text_sm()
            .text_color(color)
            .cursor_pointer()
            .hover(|s| s.bg(gpui::rgba(0xFF000020)))
            .on_mouse_down(gpui::MouseButton::Left, handler)
            .child(label.to_string())
    }

    fn menu_divider(cx: &mut Context<GraphCanvasPanel>) -> impl IntoElement {
        div()
            .my(px(4.0))
            .mx(px(8.0))
            .h(px(1.0))
            .bg(cx.theme().border)
    }
}
