//! Rendering - GPUI render implementation and trait implementations

use gpui::prelude::*;
use gpui::*;
use ui::{
    dock::{Panel, PanelEvent, PanelState},
    h_flex,
    input::TextInput,
    scroll::Scrollbar,
    v_flex, v_virtual_list, ActiveTheme, StyledExt,
};

use super::panel::BlueprintEditorPanel;
use super::toolbar::ToolbarRenderer;
use crate::core::events::{OpenEngineLibraryRequest, ShowNodePickerRequest};


/// A node entry in the find-panel listing, tagged with its owning subgraph tab
/// so clicking it can switch tabs and pan to the correct canvas.
struct FindNodeEntry {
    id: String,
    title: String,
    node_type: String,
    position: (f64, f64),
    tab_index: usize,
    tab_name: String,
    is_active_tab: bool,
    canvas: Option<Entity<crate::editor::graph_canvas_panel::GraphCanvasPanel>>,
}

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
        let active_tab_index = self.active_tab_index;

        // Collect nodes from every open tab, reading from live canvases first.
        let mut entries: Vec<FindNodeEntry> = Vec::new();
        let mut total_nodes = 0usize;
        for (ti, tab) in self.open_tabs.iter().enumerate() {
            let canvas = self
                .graph_panels
                .iter()
                .find(|(id, _)| id == &tab.id)
                .map(|(_, c)| c.clone());
            // Live canvas data, or the document of a tab not yet mounted.
            let nodes: Vec<(String, String, String, (f64, f64))> = match &canvas {
                Some(c) => {
                    let panel = c.read(cx);
                    let scene = panel.canvas.read(cx).scene();
                    panel
                        .graph(cx)
                        .nodes
                        .values()
                        .map(|n| {
                            let title = scene
                                .node(&n.id)
                                .map(|s| s.presentation.title.clone())
                                .unwrap_or_else(|| n.node_type.clone());
                            (n.id.clone(), title, n.node_type.clone(), (n.position.x, n.position.y))
                        })
                        .collect()
                }
                None => tab
                    .pending
                    .as_ref()
                    .map(|doc| {
                        doc.graph
                            .nodes
                            .values()
                            .map(|n| (n.id.clone(), n.node_type.clone(), n.node_type.clone(), (n.position.x, n.position.y)))
                            .collect()
                    })
                    .unwrap_or_default(),
            };
            total_nodes += nodes.len();
            for (id, title, node_type, position) in nodes {
                entries.push(FindNodeEntry {
                    id,
                    title,
                    node_type,
                    position,
                    tab_index: ti,
                    tab_name: tab.name.clone(),
                    is_active_tab: ti == active_tab_index,
                    canvas: canvas.clone(),
                });
            }
        }

        let tab_count = self.open_tabs.len();
        let query = self.find_search_query.to_lowercase();

        // Filter entries based on search query (matches title, type, definition_id)
        let filtered: Vec<FindNodeEntry> = if query.is_empty() {
            entries
        } else {
            entries
                .into_iter()
                .filter(|e| {
                    e.title.to_lowercase().contains(&query)
                        || e.node_type.to_lowercase().contains(&query)
                })
                .collect()
        };

        let item_sizes = std::rc::Rc::new(
            std::iter::repeat(size(px(0.0), px(36.0)))
                .take(filtered.len())
                .collect::<Vec<_>>(),
        );

        let panel_entity = cx.entity().clone();
        let scroll_handle = self.find_output_scroll_handle.clone();
        let scrollbar_state = self.find_output_scrollbar_state.clone();
        let find_search_input = self.find_search_input.clone();

        v_flex()
            .size_full()
            .p_2()
            .gap_2()
            .child(
                v_flex()
                    .gap_1p5()
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
                                    .child(if query.is_empty() {
                                        format!("{} nodes across {} tabs", total_nodes, tab_count)
                                    } else {
                                        format!("{} of {} nodes", filtered.len(), total_nodes)
                                    }),
                            ),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .h(px(28.0))
                            .items_center()
                            .px_2()
                            .rounded(px(4.0))
                            .bg(cx.theme().input)
                            .border_1()
                            .border_color(cx.theme().border.opacity(0.6))
                            .child(TextInput::new(&find_search_input).text_sm()),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .relative()
                    .when(filtered.is_empty(), |this| {
                        this.child(
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("No nodes in any subgraph."),
                        )
                    })
                    .when(!filtered.is_empty(), |this| {
                        this.child(
                            v_virtual_list(
                                panel_entity,
                                "find-panel-node-list",
                                item_sizes,
                                move |_panel, range, _window, cx| {
                                    range
                                        .map(|ix| -> AnyElement {
                                            let Some(entry) = filtered.get(ix) else {
                                                return div().h(px(36.0)).into_any_element();
                                            };

                                            let node_id = entry.id.clone();
                                            let node_title = entry.title.clone();
                                            let node_tab_name = entry.tab_name.clone();
                                            let node_tab_index = entry.tab_index;
                                            let is_active_tab = entry.is_active_tab;
                                            let canvas_for_click = entry.canvas.clone();

                                            h_flex()
                                                .w_full()
                                                .h(px(36.0))
                                                .items_center()
                                                .justify_between()
                                                .px_2()
                                                .cursor_pointer()
                                                .hover(|s| s.bg(cx.theme().muted.opacity(0.2)))
                                                .on_mouse_down(
                                                    gpui::MouseButton::Left,
                                                    cx.listener(move |panel, _, window, cx| {
                                                        panel.clear_sidebar_selections(
                                                            false, false, false, false,
                                                        );
                                                        // Switch to the owning tab if needed
                                                        if node_tab_index != panel.active_tab_index
                                                        {
                                                            panel.switch_to_tab(
                                                                node_tab_index,
                                                                window,
                                                                cx,
                                                            );
                                                        }

                                                        // Pan & select on the correct canvas
                                                        if let Some(ref canvas) = canvas_for_click {
                                                            canvas.update(cx, |canvas, cx| {
                                                                canvas.canvas.update(cx, |c, cx| {
                                                                    c.select_nodes([node_id.clone()], cx)
                                                                });
                                                                canvas.animate_pan_to_node(&node_id, cx);
                                                            });
                                                        }
                                                        cx.notify();
                                                    }),
                                                )
                                                .child(
                                                    h_flex()
                                                        .gap_2()
                                                        .items_center()
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(cx.theme().foreground)
                                                                .child(node_title),
                                                        )
                                                        .child(
                                                            div()
                                                                .px_1p5()
                                                                .py_0p5()
                                                                .rounded(px(3.0))
                                                                .bg(if is_active_tab {
                                                                    cx.theme().accent.opacity(0.15)
                                                                } else {
                                                                    cx.theme().muted.opacity(0.3)
                                                                })
                                                                .text_xs()
                                                                .font_family(
                                                                    "JetBrainsMono-Regular",
                                                                )
                                                                .text_color(if is_active_tab {
                                                                    cx.theme().accent
                                                                } else {
                                                                    cx.theme().muted_foreground
                                                                })
                                                                .child(node_tab_name),
                                                        ),
                                                )
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(cx.theme().muted_foreground)
                                                        .child(format!(
                                                            "({:.0}, {:.0})",
                                                            entry.position.0,
                                                            entry.position.1
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
        if let Some(error) = self.load_error.as_deref() {
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .bg(cx.theme().background)
                .child(
                    v_flex()
                        .w_full()
                        .max_w(px(720.0))
                        .gap_3()
                        .p_6()
                        .bg(cx.theme().sidebar)
                        .border_1()
                        .border_color(cx.theme().border)
                        .rounded_md()
                        .child(
                            div()
                                .text_lg()
                                .text_color(cx.theme().danger)
                                .child("Blueprint could not be opened"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(error.to_string()),
                        ),
                )
                .into_any_element();
        }

        if self.workspace.is_none() {
            self.initialize_workspace(window, cx);
        }

        v_flex()
            .size_full()
            .relative()
            .bg(cx.theme().background)
            .key_context("BlueprintEditor")
            .child(ToolbarRenderer::render(self, window, cx))
            .child(div().flex_1().min_h_0().map(|el| {
                if let Some(workspace) = &self.workspace {
                    el.child(workspace.clone())
                } else {
                    el.child(div().child("Initializing workspace..."))
                }
            }))
            .when(self.file_loading, |this| {
                this.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(cx.theme().background.opacity(0.88))
                        .child(
                            v_flex()
                                .items_center()
                                .gap_3()
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().foreground)
                                        .child("Loading Blueprint…"),
                                )
                                .child(
                                    div()
                                        .w(px(220.))
                                        .h(px(3.))
                                        .rounded_full()
                                        .bg(cx.theme().border)
                                        .child(
                                            div()
                                                .w(px(70.))
                                                .h_full()
                                                .rounded_full()
                                                .bg(cx.theme().primary),
                                        ),
                                ),
                        ),
                )
            })
            .into_any_element()
    }
}
