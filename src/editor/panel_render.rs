//! Rendering - GPUI render implementation and trait implementations

use gpui::*;
use gpui::prelude::*;
use ui::{dock::{Panel, PanelEvent, PanelState}, h_flex, v_flex, ActiveTheme, StyledExt};

use super::panel::BlueprintEditorPanel;
use super::toolbar::ToolbarRenderer;
use crate::core::events::*;
use crate::rendering::graph::NodeGraphRenderer;

impl Panel for BlueprintEditorPanel {
    fn panel_name(&self) -> &'static str {
        "Blueprint Editor"
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
    pub fn render_compiler_results(&self, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::core::types::CompilationState;

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
                            })
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{} messages", self.compilation_history.len()))
                    )
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .child(
                        v_flex()
                            .w_full()
                            .gap_0p5()
                            .children(self.compilation_history.iter().rev().map(|entry| {
                                h_flex()
                                    .w_full()
                                    .px_2()
                                    .py_1()
                                    .gap_2()
                                    .border_b_1()
                                    .border_color(cx.theme().border.opacity(0.1))
                                    .hover(|s| s.bg(cx.theme().muted.opacity(0.05)))
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .text_xs()
                                            .font_family("JetBrainsMono-Regular")
                                            .text_color(cx.theme().muted_foreground.opacity(0.7))
                                            .child(entry.timestamp.clone())
                                    )
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .w(px(12.0))
                                            .text_xs()
                                            .text_color(match entry.state {
                                                CompilationState::Success => gpui::green(),
                                                CompilationState::Error => gpui::red(),
                                                _ => cx.theme().muted_foreground,
                                            })
                                            .child(match entry.state {
                                                CompilationState::Success => "✓",
                                                CompilationState::Error => "✗",
                                                _ => "•",
                                            })
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_xs()
                                            .text_color(cx.theme().foreground)
                                            .child(entry.message.clone())
                                    )
                            }))
                            .when(self.compilation_history.is_empty(), |this| {
                                this.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .py(px(32.0))
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("No compilation messages yet.")
                                )
                            })
                    )
            )
    }

    pub fn render_find_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let node_count = self.graph.nodes.len();
        let comment_count = self.graph.comments.len();

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
                            .child("Graph Index")
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{} nodes, {} comments", node_count, comment_count))
                    )
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Click a node entry to select it in the graph.")
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        v_flex()
                            .gap_1()
                            .scrollable(Axis::Vertical)
                            .children(
                                self.graph.nodes.iter().map(|node| {
                                    let node_id = node.id.clone();
                                    let node_title = node.title.clone();

                                    h_flex()
                                        .w_full()
                                        .items_center()
                                        .justify_between()
                                        .px_2()
                                        .py_1p5()
                                        .rounded(px(4.0))
                                        .cursor_pointer()
                                        .hover(|s| s.bg(cx.theme().muted.opacity(0.2)))
                                        .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |panel, _, _window, cx| {
                                            panel.graph.selected_nodes.clear();
                                            panel.graph.selected_nodes.push(node_id.clone());
                                            cx.notify();
                                        }))
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().foreground)
                                                .child(node_title)
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(format!("({:.0}, {:.0})", node.position.x, node.position.y))
                                        )
                                })
                            )
                    )
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
                    .children(
                        self.open_tabs.iter().enumerate().map(|(index, tab)| {
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
                                    })
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
                                        .child(tab.name.clone())
                                )
                                .when(tab.is_dirty, |this| {
                                    this.child(
                                        div()
                                            .w(px(6.0))
                                            .h(px(6.0))
                                            .rounded_full()
                                            .bg(cx.theme().accent)
                                    )
                                })
                                .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |this, _, _window, cx| {
                                    this.switch_to_tab(index, cx);
                                }))
                        })
                    )
            )
            .child(div().flex_1())
            .child(
                h_flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Tabs")
                    )
            )
    }
}

impl Render for BlueprintEditorPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.workspace.is_none() {
            self.initialize_workspace(window, cx);
        }

        self.refresh_comment_color_bindings(window, cx);

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .key_context("BlueprintEditor")
            .on_action(cx.listener(|panel, action: &DuplicateNode, _window, cx| {
                panel.duplicate_node(action.node_id.clone(), cx);
            }))
            .on_action(cx.listener(|panel, action: &DeleteNode, _window, cx| {
                panel.delete_node(action.node_id.clone(), cx);
            }))
            .on_action(cx.listener(|panel, action: &CopyNode, _window, cx| {
                panel.copy_node(action.node_id.clone(), cx);
            }))
            .on_action(cx.listener(|panel, _action: &PasteNode, _window, cx| {
                panel.paste_node(cx);
            }))
            .on_action(cx.listener(|panel, action: &DisconnectPin, _window, cx| {
                panel.disconnect_pin(action.node_id.clone(), action.pin_id.clone(), cx);
            }))
            .on_action(cx.listener(|panel, _action: &OpenAddNodeMenu, window, cx| {
                if let Some(bounds) = &panel.graph_element_bounds {
                    let screen_center = Point::new(bounds.center().x, bounds.center().y);
                    let graph_pos = NodeGraphRenderer::screen_to_graph_pos(screen_center, &panel.graph);
                    panel.show_node_picker(graph_pos, window, cx);
                }
            }))
            .child(ToolbarRenderer::render(self, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .map(|el| {
                        if let Some(workspace) = &self.workspace {
                            el.child(workspace.clone())
                        } else {
                            el.child(div().child("Initializing workspace..."))
                        }
                    })
            )
    }
}
