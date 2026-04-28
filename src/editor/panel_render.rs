//! Rendering - GPUI render implementation and trait implementations

use gpui::*;
use gpui::prelude::*;
use ui::{
    button::Button,
    dock::{Panel, PanelEvent, PanelState},
    h_flex,
    input::TextInput,
    popover::Popover,
    v_flex,
    v_virtual_list,
    ActiveTheme,
    StyledExt,
};

use super::panel::BlueprintEditorPanel;
use super::toolbar::ToolbarRenderer;
use crate::core::definitions::NodeDefinitions;
use crate::core::events::*;
use crate::rendering::graph::NodeGraphRenderer;
use crate::ui_components::node_library::{
    build_item_sizes,
    build_palette_items,
    filter_palette_items,
    PaletteItem,
};

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
                                .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |this, _, window, cx| {
                                    this.switch_to_tab(index, window, cx);
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

fn render_quick_palette_popover(
    panel: &BlueprintEditorPanel,
    cx: &mut Context<BlueprintEditorPanel>,
) -> AnyElement {
    let trigger_left = panel
        .popup_trigger_screen_pos
        .map(|p| p.x - px(3.0))
        .unwrap_or(px(-10000.0));
    let trigger_top = panel
        .popup_trigger_screen_pos
        .map(|p| p.y - px(3.0))
        .unwrap_or(px(-10000.0));

    let editor_weak = cx.entity().downgrade();
    let trigger_screen_pos = panel.popup_trigger_screen_pos;

    let trigger_btn = Button::new("palette-popover-trigger")
        .absolute()
        .left(trigger_left)
        .top(trigger_top)
        .w(px(6.0))
        .h(px(6.0))
        .opacity(0.01);

    Popover::new(SharedString::from("blueprint-palette-popover"))
        .anchor(Corner::TopLeft)
        .mouse_button(MouseButton::Right)
        .trigger(trigger_btn)
        .content(move |_window, pop_cx| {
            let editor_weak = editor_weak.clone();
            let trigger_screen_pos = trigger_screen_pos;
            pop_cx.new(|cx| {
                let search_input =
                    cx.new(|cx| ui::input::InputState::new(_window, cx).placeholder("Search nodes…"));
                search_input.update(cx, |input, cx| {
                    input.focus(_window, cx);
                });

                ui::popover::PopoverContent::new(_window, cx, move |_window, cx| {
                    let Some(editor) = editor_weak.upgrade() else {
                        return div().into_any_element();
                    };

                    let place_graph_pos = {
                        let panel = editor.read(cx);
                        panel.popup_palette_graph_pos
                    };

                    let query = search_input.read(cx).value().to_string();
                    let all_items = build_palette_items(NodeDefinitions::load());
                    let visible_items = filter_palette_items(&all_items, &query);
                    let item_sizes = build_item_sizes(&visible_items);
                    let view_entity = cx.entity().clone();
                    let items_snap = visible_items;
                    let editor_weak_for_list = editor_weak.clone();

                    v_flex()
                        .w(px(360.0))
                        .min_h(px(220.0))
                        .max_h(px(520.0))
                        .child(
                            v_flex()
                                .p_2()
                                .gap_2()
                                .child(
                                    TextInput::new(&search_input)
                                        .w_full()
                                        .appearance(false)
                                        .cleanable(),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_h_0()
                                        .overflow_hidden()
                                        .child(v_virtual_list(
                                            view_entity,
                                            "popup-palette-list",
                                            item_sizes,
                                            move |_popover, range, _window, cx| {
                                                range
                                                    .map(|ix| -> AnyElement {
                                                        let Some(item) = items_snap.get(ix) else {
                                                            return div().into_any_element();
                                                        };

                                                        match item {
                                                            PaletteItem::CategoryHeader { name, node_count, .. } => {
                                                                h_flex()
                                                                    .w_full()
                                                                    .h(px(28.0))
                                                                    .px_3()
                                                                    .items_center()
                                                                    .justify_between()
                                                                    .bg(cx.theme().muted.opacity(0.12))
                                                                    .child(
                                                                        div()
                                                                            .text_xs()
                                                                            .text_color(cx.theme().muted_foreground)
                                                                            .child(name.clone()),
                                                                    )
                                                                    .child(
                                                                        div()
                                                                            .text_xs()
                                                                            .text_color(cx.theme().muted_foreground)
                                                                            .child(node_count.to_string()),
                                                                    )
                                                                    .into_any_element()
                                                            }
                                                            PaletteItem::NodeEntry { def, .. } => {
                                                                let def_clone = def.clone();
                                                                let editor_weak = editor_weak_for_list.clone();
                                                                h_flex()
                                                                    .w_full()
                                                                    .h(px(44.0))
                                                                    .px_2()
                                                                    .gap_2()
                                                                    .items_center()
                                                                    .cursor_pointer()
                                                                    .hover(|s| s.bg(cx.theme().accent.opacity(0.06)))
                                                                    .child(
                                                                        div()
                                                                            .w(px(28.0))
                                                                            .h(px(28.0))
                                                                            .rounded_full()
                                                                            .bg(cx.theme().accent.opacity(0.12))
                                                                            .flex()
                                                                            .items_center()
                                                                            .justify_center()
                                                                            .child(def.icon.clone()),
                                                                    )
                                                                    .child(
                                                                        v_flex()
                                                                            .flex_1()
                                                                            .min_w_0()
                                                                            .child(
                                                                                div()
                                                                                    .text_xs()
                                                                                    .text_color(cx.theme().foreground)
                                                                                    .child(def.name.clone()),
                                                                            )
                                                                            .child(
                                                                                div()
                                                                                    .text_xs()
                                                                                    .text_color(cx.theme().muted_foreground)
                                                                                    .child(def.description.clone()),
                                                                            ),
                                                                    )
                                                                    .on_mouse_down(
                                                                        MouseButton::Left,
                                                                        cx.listener(move |_popover, _event, _window, cx| {
                                                                            if let Some(editor) = editor_weak.upgrade() {
                                                                                editor.update(cx, |panel, cx| {
                                                                                    let base = place_graph_pos.or_else(|| {
                                                                                        // Fallback: convert trigger window position to graph coordinates
                                                                                        match (trigger_screen_pos, panel.graph_element_bounds) {
                                                                                            (Some(window_pos), Some(bounds)) => {
                                                                                                let screen = Point::new(
                                                                                                    window_pos.x - bounds.origin.x,
                                                                                                    window_pos.y - bounds.origin.y,
                                                                                                );
                                                                                                let graph_pos = NodeGraphRenderer::screen_to_graph_pos(screen, &panel.graph);
                                                                                                Some(Point::new(graph_pos.x, graph_pos.y))
                                                                                            }
                                                                                            _ => None,
                                                                                        }
                                                                                    }).unwrap_or(Point::new(0.0, 0.0));

                                                                                    let stagger = (panel.graph.nodes.len() % 8) as f32 * 18.0;
                                                                                    let node_pos = Point::new(base.x + stagger, base.y + stagger);
                                                                                    let node = crate::core::types::BlueprintNode::from_definition(&def_clone, node_pos);
                                                                                    panel.add_node(node, cx);
                                                                                    panel.popup_palette_graph_pos = None;
                                                                                    cx.notify();
                                                                                });
                                                                            }
                                                                            cx.emit(DismissEvent);
                                                                        }),
                                                                    )
                                                                    .into_any_element()
                                                            }
                                                        }
                                                    })
                                                    .collect()
                                            },
                                        )),
                                ),
                        )
                        .into_any_element()
                })
            })
        })
        .into_any_element()
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
            .child(render_quick_palette_popover(self, cx))
    }
}
