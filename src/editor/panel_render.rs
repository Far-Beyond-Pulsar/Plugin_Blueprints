//! Rendering - GPUI render implementation and trait implementations

use gpui::prelude::*;
use gpui::*;
use ui::{
    dock::{Panel, PanelEvent, PanelState},
    h_flex,
    scroll::Scrollbar,
    v_flex, v_virtual_list, ActiveTheme, StyledExt,
};

use super::panel::BlueprintEditorPanel;
use super::toolbar::ToolbarRenderer;
use crate::core::events::*;
use crate::rendering::graph::NodeGraphRenderer;

impl Panel for BlueprintEditorPanel {
    fn panel_name(&self) -> &'static str {
        "Blueprint Editor"
    }

    fn panel_file_path(&self, _cx: &App) -> Option<std::path::PathBuf> {
        self.current_class_path.clone()
    }

    fn title(&self, _window: &Window, _cx: &App) -> AnyElement {
        h_flex()
            .gap_2()
            .items_center()
            .child(div().text_sm().child(if let Some(title) = &self.tab_title {
                title.clone()
            } else {
                "Blueprint Editor".to_string()
            }))
            .into_any_element()
    }

    fn dump(&self, _cx: &App) -> PanelState {
        PanelState {
            panel_name: self.panel_name().to_string(),
            ..Default::default()
        }
    }
}

impl Focusable for BlueprintEditorPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for BlueprintEditorPanel {}
impl EventEmitter<OpenEngineLibraryRequest> for BlueprintEditorPanel {}
impl EventEmitter<ShowNodePickerRequest> for BlueprintEditorPanel {}

impl BlueprintEditorPanel {
    /// Render compiler results panel (compilation history and status)
    pub fn render_compiler_results(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::core::types::CompilationState;

        let history_entries: Vec<_> = self.compilation_history.iter().rev().cloned().collect();
        let item_sizes = std::rc::Rc::new(
            history_entries
                .iter()
                .map(|_| size(px(0.0), px(56.0)))
                .collect::<Vec<_>>(),
        );
        let compiler_entity = cx.entity().clone();
        let scroll_handle = self.compiler_output_scroll_handle.clone();
        let scrollbar_state = self.compiler_output_scrollbar_state.clone();

        v_flex()
            .size_full()
            .child(
                h_flex()
                    .w_full()
                    .px_2()
                    .py_1p5()
                    .bg(cx.theme().secondary)
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(match self.compilation_status.state {
                                CompilationState::Success => gpui::green(),
                                CompilationState::Error => gpui::red(),
                                CompilationState::Compiling => gpui::yellow(),
                                _ => cx.theme().foreground,
                            })
                            .child(match self.compilation_status.state {
                                CompilationState::Idle => "Compiler Output",
                                CompilationState::Compiling => "⟳ Compiling...",
                                CompilationState::Success => "✓ Build Succeeded",
                                CompilationState::Error => "✗ Build Failed",
                            }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{} entries", self.compilation_history.len())),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .relative()
                    .when(history_entries.is_empty(), |this| {
                        this.child(
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("No compilation messages yet."),
                        )
                    })
                    .when(!history_entries.is_empty(), |this| {
                        this.child(
                            v_virtual_list(
                                compiler_entity,
                                "compiler-history-list",
                                item_sizes,
                                move |_panel, range, _window, cx| {
                                    range
                                        .map(|ix| -> AnyElement {
                                            let Some(entry) = history_entries.get(ix) else {
                                                return div().h(px(56.0)).into_any_element();
                                            };

                                            let accent = match entry.state {
                                                CompilationState::Success => cx.theme().success,
                                                CompilationState::Error => cx.theme().danger,
                                                CompilationState::Compiling => cx.theme().warning,
                                                CompilationState::Idle => {
                                                    cx.theme().muted_foreground.opacity(0.7)
                                                }
                                            };

                                            let icon = match entry.state {
                                                CompilationState::Success => "✓",
                                                CompilationState::Error => "✗",
                                                CompilationState::Compiling => "•",
                                                CompilationState::Idle => "•",
                                            };

                                            h_flex()
                                                .w_full()
                                                .h(px(56.0))
                                                .px_2()
                                                .py_1()
                                                .gap_2()
                                                .border_b_1()
                                                .border_color(cx.theme().border.opacity(0.1))
                                                .hover(|s| s.bg(cx.theme().muted.opacity(0.06)))
                                                .child(
                                                    div()
                                                        .w(px(2.0))
                                                        .h_full()
                                                        .rounded_full()
                                                        .bg(accent)
                                                        .flex_shrink_0(),
                                                )
                                                .child(
                                                    v_flex()
                                                        .w(px(76.0))
                                                        .gap_0p5()
                                                        .flex_shrink_0()
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .font_family(
                                                                    "JetBrainsMono-Regular",
                                                                )
                                                                .text_color(
                                                                    cx.theme()
                                                                        .muted_foreground
                                                                        .opacity(0.8),
                                                                )
                                                                .child(entry.timestamp.clone()),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(accent)
                                                                .child(entry.stage.to_uppercase()),
                                                        ),
                                                )
                                                .child(
                                                    div()
                                                        .w(px(14.0))
                                                        .text_xs()
                                                        .text_color(accent)
                                                        .child(icon),
                                                )
                                                .child(
                                                    v_flex()
                                                        .flex_1()
                                                        .gap_0p5()
                                                        .overflow_hidden()
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .font_weight(
                                                                    gpui::FontWeight::SEMIBOLD,
                                                                )
                                                                .text_color(cx.theme().foreground)
                                                                .child(entry.message.clone()),
                                                        )
                                                        .when(entry.detail.is_some(), |this| {
                                                            this.child(
                                                                div()
                                                                    .text_xs()
                                                                    .text_color(
                                                                        cx.theme().muted_foreground,
                                                                    )
                                                                    .child(
                                                                        entry
                                                                            .detail
                                                                            .clone()
                                                                            .unwrap_or_default(),
                                                                    ),
                                                            )
                                                        }),
                                                )
                                                .into_any_element()
                                        })
                                        .collect()
                                },
                            )
                            .size_full()
                            .track_scroll(&scroll_handle),
                        )
                        .child(
                            div()
                                .absolute()
                                .inset_0()
                                .child(Scrollbar::vertical(&scrollbar_state, &scroll_handle)),
                        )
                    }),
            )
    }

    pub fn render_find_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // Read nodes from the live canvas if available; fall back to self.graph shadow.
        let (node_count, comment_count, nodes, selected_nodes) =
            if let Some(canvas) = self.active_canvas() {
                let g = canvas.read(cx).graph.clone();
                (
                    g.nodes.len(),
                    g.comments.len(),
                    g.nodes.clone(),
                    g.selected_nodes.clone(),
                )
            } else {
                (
                    self.graph.nodes.len(),
                    self.graph.comments.len(),
                    self.graph.nodes.clone(),
                    self.graph.selected_nodes.clone(),
                )
            };
        let item_sizes = std::rc::Rc::new(
            nodes
                .iter()
                .map(|_| size(px(0.0), px(34.0)))
                .collect::<Vec<_>>(),
        );
        let panel_entity = cx.entity().clone();
        let scroll_handle = self.find_output_scroll_handle.clone();
        let scrollbar_state = self.find_output_scrollbar_state.clone();
        let active_canvas_entity: Option<gpui::Entity<crate::editor::workspace_panels::GraphCanvasPanel>> = self
            .graph_panels
            .iter()
            .find(|(id, _)| {
                self.open_tabs
                    .get(self.active_tab_index)
                    .map(|t| &t.id)
                    == Some(id)
            })
            .map(|(_, c)| c.clone());

        v_flex()
            .size_full()
            .p_2()
            .gap_2()
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(cx.theme().foreground)
                            .child("Graph Index"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{} nodes, {} comments", node_count, comment_count)),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Click a node entry to select it in the graph."),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .relative()
                    .when(nodes.is_empty(), |this| {
                        this.child(
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("No nodes in graph."),
                        )
                    })
                    .when(!nodes.is_empty(), |this| {
                        this.child(
                            v_virtual_list(
                                panel_entity,
                                "find-panel-node-list",
                                item_sizes,
                                move |_panel, range, _window, cx| {
                                    range
                                        .map(|ix| -> AnyElement {
                                            let Some(node) = nodes.get(ix) else {
                                                return div().h(px(34.0)).into_any_element();
                                            };

                                            let node_id = node.id.clone();
                                            let node_title = node.title.clone();
                                            let is_selected = selected_nodes.contains(&node_id);
                                            let canvas_for_click = active_canvas_entity.clone();

                                            h_flex()
                                                .w_full()
                                                .h(px(34.0))
                                                .items_center()
                                                .justify_between()
                                                .px_2()
                                                .cursor_pointer()
                                                .bg(if is_selected {
                                                    cx.theme().accent.opacity(0.12)
                                                } else {
                                                    gpui::transparent_black()
                                                })
                                                .border_b_1()
                                                .border_color(cx.theme().border.opacity(0.08))
                                                .hover(|s| s.bg(cx.theme().muted.opacity(0.2)))
                                                .on_mouse_down(
                                                    gpui::MouseButton::Left,
                                                    cx.listener(move |panel, _, _window, cx| {
                                                        panel.graph.selected_nodes.clear();
                                                        panel
                                                            .graph
                                                            .selected_nodes
                                                            .push(node_id.clone());
                                                        if let Some(ref canvas) = canvas_for_click {
                                                            canvas.update(cx, |canvas, _cx| {
                                                                canvas.animate_pan_to_node(&node_id);
                                                            });
                                                        }
                                                        cx.notify();
                                                    }),
                                                )
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(cx.theme().foreground)
                                                        .child(node_title),
                                                )
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(cx.theme().muted_foreground)
                                                        .child(format!(
                                                            "({:.0}, {:.0})",
                                                            node.position.x, node.position.y
                                                        )),
                                                )
                                                .into_any_element()
                                        })
                                        .collect()
                                },
                            )
                            .size_full()
                            .track_scroll(&scroll_handle),
                        )
                        .child(
                            div()
                                .absolute()
                                .inset_0()
                                .child(Scrollbar::vertical(&scrollbar_state, &scroll_handle)),
                        )
                    }),
            )
    }

    pub fn render_tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        use ui::IconName;

        h_flex()
            .w_full()
            .h(px(32.0))
            .bg(cx.theme().secondary)
            .border_b_1()
            .border_color(cx.theme().border)
            .items_center()
            .overflow_x_hidden()
            .child(
                h_flex()
                    .items_center()
                    .children(self.open_tabs.iter().enumerate().map(|(index, tab)| {
                        let is_active = index == self.active_tab_index;

                        h_flex()
                            .items_center()
                            .gap_1p5()
                            .px_3()
                            .h_full()
                            .bg(if is_active {
                                cx.theme().background
                            } else {
                                gpui::transparent_black()
                            })
                            .when(is_active, |this| {
                                this.border_t_2().border_color(cx.theme().accent)
                            })
                            .when(!is_active, |this| {
                                this.hover(|s| s.bg(cx.theme().muted.opacity(0.1)))
                            })
                            .cursor_pointer()
                            .child(
                                ui::Icon::new(if tab.is_main {
                                    IconName::Play
                                } else {
                                    IconName::Component
                                })
                                .size(px(14.0))
                                .text_color(if is_active {
                                    cx.theme().accent
                                } else {
                                    cx.theme().muted_foreground
                                }),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .when(is_active, |s| s.font_weight(gpui::FontWeight::SEMIBOLD))
                                    .text_color(if is_active {
                                        cx.theme().foreground
                                    } else {
                                        cx.theme().muted_foreground
                                    })
                                    .child(tab.name.clone()),
                            )
                            .when(tab.is_dirty, |this| {
                                this.child(
                                    div()
                                        .w(px(6.0))
                                        .h(px(6.0))
                                        .rounded_full()
                                        .bg(cx.theme().accent),
                                )
                            })
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(move |this, _, window, cx| {
                                    this.switch_to_tab(index, window, cx);
                                }),
                            )
                    })),
            )
            .child(div().flex_1())
            .child(
                h_flex().items_center().gap_1().px_2().child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Tabs"),
                ),
            )
    }
}

impl Render for BlueprintEditorPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.workspace.is_none() {
            self.initialize_workspace(window, cx);
        }

        // Comment color bindings are per-canvas; refresh via the active canvas
        if let Some(c) = self.active_canvas().cloned() {
            c.update(cx, |canvas, cx| canvas.refresh_comment_color_bindings(window, cx));
        }

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .key_context("BlueprintEditor")
            .on_action(cx.listener(|panel, action: &DuplicateNode, _window, cx| {
                let nid = action.node_id.clone();
                if let Some(c) = panel.active_canvas().cloned() { c.update(cx, |canvas, cx| canvas.duplicate_node(nid, cx)); }
            }))
            .on_action(cx.listener(|panel, action: &DeleteNode, _window, cx| {
                let nid = action.node_id.clone();
                if let Some(c) = panel.active_canvas().cloned() { c.update(cx, |canvas, cx| canvas.delete_node(nid, cx)); }
            }))
            .on_action(cx.listener(|panel, action: &CopyNode, _window, cx| {
                let nid = action.node_id.clone();
                if let Some(c) = panel.active_canvas().cloned() { c.update(cx, |canvas, cx| canvas.copy_node(nid, cx)); }
            }))
            .on_action(cx.listener(|panel, _action: &PasteNode, _window, cx| {
                if let Some(c) = panel.active_canvas().cloned() { c.update(cx, |canvas, cx| canvas.paste_node(cx)); }
            }))
            .on_action(cx.listener(|panel, action: &DisconnectPin, _window, cx| {
                let nid = action.node_id.clone();
                let pid = action.pin_id.clone();
                if let Some(c) = panel.active_canvas().cloned() { c.update(cx, |canvas, cx| canvas.disconnect_pin(nid, pid, cx)); }
            }))
            .on_action(cx.listener(|panel, _action: &OpenAddNodeMenu, window, cx| {
                if let Some(c) = panel.active_canvas().cloned() {
                    c.update(cx, |canvas, cx| {
                        if let Some(bounds) = canvas.element_bounds {
                            let sc = Point::new(bounds.center().x, bounds.center().y);
                            let gp = NodeGraphRenderer::screen_to_graph_pos(sc, &canvas.graph);
                            // drop canvas borrow before calling show_node_picker on panel
                            let _ = gp;
                        }
                    });
                }
                // TODO: route show_node_picker through active canvas
            }))
            .child(ToolbarRenderer::render(self, cx))
            .child(div().flex_1().min_h_0().map(|el| {
                if let Some(workspace) = &self.workspace {
                    el.child(workspace.clone())
                } else {
                    el.child(div().child("Initializing workspace..."))
                }
            }))
    }
}
