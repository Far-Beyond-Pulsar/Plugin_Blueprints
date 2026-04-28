//! Dedicated panel components for the workspace docking system
//!
//! These panels wrap the editor entity and render specific content.

use gpui::*;
use ui::{
    h_flex, v_flex, Icon, IconName,
    ActiveTheme,
    dock::{Panel, PanelEvent},
    input::{InputState, TextInput},
    v_virtual_list, VirtualListScrollHandle,
};

use crate::core::definitions::{NodeDefinitions, NodeDefinition};
use crate::core::types::BlueprintNode;
use crate::editor::panel::BlueprintEditorPanel;
use crate::features::macros::panel::MacrosRenderer;
use crate::features::variables::rendering::VariablesRenderer;
use crate::rendering::graph::NodeGraphRenderer;
use crate::ui_components::node_library::{
    build_item_sizes, build_palette_items, count_nodes,
    filter_palette_items, PaletteItem,
    CATEGORY_HEADER_H, NODE_ENTRY_H,
};
use crate::ui_components::properties::PropertiesRenderer;

/// Variables Panel
pub struct VariablesPanel {
    editor: WeakEntity<BlueprintEditorPanel>,
    focus_handle: FocusHandle,
}

impl VariablesPanel {
    pub fn new(editor: WeakEntity<BlueprintEditorPanel>, cx: &mut Context<Self>) -> Self {
        Self {
            editor,
            focus_handle: cx.focus_handle(),
        }
    }
}

impl EventEmitter<PanelEvent> for VariablesPanel {}

impl Render for VariablesPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(editor) = self.editor.upgrade() {
            div()
                .size_full()
                .bg(cx.theme().sidebar)
                .child(
                    editor.update(cx, |editor, cx| {
                        VariablesRenderer::render(editor, cx)
                    })
                )
        } else {
            div().child("Editor not available")
        }
    }
}

impl Focusable for VariablesPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for VariablesPanel {
    fn panel_name(&self) -> &'static str {
        "variables"
    }

    fn title(&self, _window: &Window, _cx: &App) -> AnyElement {
        "Variables".into_any_element()
    }
}

/// Macros Panel
pub struct MacrosPanel {
    editor: WeakEntity<BlueprintEditorPanel>,
    focus_handle: FocusHandle,
}

impl MacrosPanel {
    pub fn new(editor: WeakEntity<BlueprintEditorPanel>, cx: &mut Context<Self>) -> Self {
        Self {
            editor,
            focus_handle: cx.focus_handle(),
        }
    }
}

impl EventEmitter<PanelEvent> for MacrosPanel {}

impl Render for MacrosPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(editor) = self.editor.upgrade() {
            div()
                .size_full()
                .bg(cx.theme().sidebar)
                .child(
                    editor.update(cx, |editor, cx| {
                        MacrosRenderer::render(editor, cx)
                    })
                )
        } else {
            div().child("Editor not available")
        }
    }
}

impl Focusable for MacrosPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for MacrosPanel {
    fn panel_name(&self) -> &'static str {
        "macros"
    }

    fn title(&self, _window: &Window, _cx: &App) -> AnyElement {
        "Macros".into_any_element()
    }
}

/// Compiler Panel
pub struct CompilerPanel {
    editor: WeakEntity<BlueprintEditorPanel>,
    focus_handle: FocusHandle,
}

impl CompilerPanel {
    pub fn new(editor: WeakEntity<BlueprintEditorPanel>, cx: &mut Context<Self>) -> Self {
        Self {
            editor,
            focus_handle: cx.focus_handle(),
        }
    }
}

impl EventEmitter<PanelEvent> for CompilerPanel {}

impl Render for CompilerPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(editor) = self.editor.upgrade() {
            div()
                .size_full()
                .child(
                    editor.update(cx, |editor, cx| {
                        editor.render_compiler_results(cx)
                    })
                )
        } else {
            div().child("Editor not available")
        }
    }
}

impl Focusable for CompilerPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for CompilerPanel {
    fn panel_name(&self) -> &'static str {
        "compiler"
    }

    fn title(&self, _window: &Window, _cx: &App) -> AnyElement {
        "Compiler".into_any_element()
    }
}

/// Find Panel
pub struct FindPanel {
    editor: WeakEntity<BlueprintEditorPanel>,
    focus_handle: FocusHandle,
}

impl FindPanel {
    pub fn new(editor: WeakEntity<BlueprintEditorPanel>, cx: &mut Context<Self>) -> Self {
        Self {
            editor,
            focus_handle: cx.focus_handle(),
        }
    }
}

impl EventEmitter<PanelEvent> for FindPanel {}

impl Render for FindPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(editor) = self.editor.upgrade() {
            div()
                .size_full()
                .child(
                    editor.update(cx, |editor, cx| {
                        editor.render_find_panel(cx)
                    })
                )
        } else {
            div().child("Editor not available")
        }
    }
}

impl Focusable for FindPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for FindPanel {
    fn panel_name(&self) -> &'static str {
        "find"
    }

    fn title(&self, _window: &Window, _cx: &App) -> AnyElement {
        "Find".into_any_element()
    }
}

/// Properties Panel
pub struct PropertiesPanel {
    editor: WeakEntity<BlueprintEditorPanel>,
    focus_handle: FocusHandle,
}

impl PropertiesPanel {
    pub fn new(editor: WeakEntity<BlueprintEditorPanel>, cx: &mut Context<Self>) -> Self {
        Self {
            editor,
            focus_handle: cx.focus_handle(),
        }
    }
}

impl EventEmitter<PanelEvent> for PropertiesPanel {}

impl Render for PropertiesPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(editor) = self.editor.upgrade() {
            div()
                .size_full()
                .bg(cx.theme().sidebar)
                .child(
                    editor.update(cx, |editor, cx| {
                        PropertiesRenderer::render(editor, cx)
                    })
                )
        } else {
            div().child("Editor not available")
        }
    }
}

impl Focusable for PropertiesPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for PropertiesPanel {
    fn panel_name(&self) -> &'static str {
        "properties"
    }

    fn title(&self, _window: &Window, _cx: &App) -> AnyElement {
        "Details".into_any_element()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Palette Panel
// ─────────────────────────────────────────────────────────────────────────────

/// Palette panel – browsable, searchable node library backed by a virtual list.
///
/// All node definitions are flattened into a single [`Vec<PaletteItem>`] once
/// at construction time.  Each render cycle the list is optionally filtered by
/// the search query; only the visible rows are rendered by `v_virtual_list`.
pub struct PalettePanel {
    editor:        WeakEntity<BlueprintEditorPanel>,
    focus_handle:  FocusHandle,
    /// Complete flat list (category headers + node rows) – never mutated after init.
    all_items:     Vec<PaletteItem>,
    /// State for the search-filter input box.
    search_input:  Entity<InputState>,
    /// Allows programmatic scrolling (e.g. scroll-to-top on search).
    scroll_handle: VirtualListScrollHandle,
}

impl PalettePanel {
    pub fn new(
        editor: WeakEntity<BlueprintEditorPanel>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let all_items = build_palette_items(NodeDefinitions::load());
        let search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search nodes…"));
        Self {
            editor,
            focus_handle: cx.focus_handle(),
            all_items,
            search_input,
            scroll_handle: VirtualListScrollHandle::new(),
        }
    }
}

impl EventEmitter<PanelEvent> for PalettePanel {}

impl Render for PalettePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // ── 1. Build filtered visible list ────────────────────────────────────
        let query      = self.search_input.read(cx).value().to_string();
        let visible    = filter_palette_items(&self.all_items, &query);
        let node_count = count_nodes(&visible);
        let item_sizes = build_item_sizes(&visible);

        // ── 2. Capture what the 'static closure needs ────────────────────────
        // All captured data must be owned / Clone – no references.
        let items_snap    = visible;                  // Vec<PaletteItem> → moved
        let view_entity   = cx.entity().clone();
        let scroll_handle = self.scroll_handle.clone();

        v_flex()
            .size_full()
            .bg(cx.theme().sidebar)
            // ── Header ────────────────────────────────────────────────────────
            .child(
                v_flex()
                    .w_full()
                    // Title row
                    .child(
                        h_flex()
                            .w_full()
                            .px_3()
                            .py_2()
                            .bg(cx.theme().secondary)
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .items_center()
                            .justify_between()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        Icon::new(IconName::Search)
                                            .size(px(14.0))
                                            .text_color(cx.theme().accent),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(cx.theme().foreground)
                                            .child("Palette"),
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("{node_count} nodes")),
                            ),
                    )
                    // Search box
                    .child(
                        h_flex()
                            .w_full()
                            .px_2()
                            .py_1p5()
                            .bg(cx.theme().sidebar)
                            .border_b_1()
                            .border_color(cx.theme().border.opacity(0.4))
                            .child(
                                TextInput::new(&self.search_input)
                                    .w_full()
                                    .appearance(false)
                                    .prefix(
                                        Icon::new(IconName::Search)
                                            .size(px(12.0))
                                            .text_color(cx.theme().muted_foreground),
                                    )
                                    .cleanable(),
                            ),
                    ),
            )
            // ── Virtual list body ──────────────────────────────────────────────
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        v_virtual_list(
                            view_entity,
                            "palette-node-list",
                            item_sizes,
                            // The closure is 'static + Fn.  Only owned data is
                            // captured; `items_snap` is borrowed inside each call.
                            move |_panel, range, _window, cx| {
                                range
                                    .map(|ix| -> AnyElement {
                                        let Some(item) = items_snap.get(ix) else {
                                            return div()
                                                .h(px(NODE_ENTRY_H))
                                                .into_any_element();
                                        };
                                        match item {
                                            PaletteItem::CategoryHeader {
                                                name,
                                                color,
                                                node_count,
                                            } => palette_category_header(
                                                name, color, *node_count, cx,
                                            )
                                            .into_any_element(),

                                            PaletteItem::NodeEntry { def, category_color } => {
                                                palette_node_row(ix, def.clone(), category_color, cx)
                                                    .into_any_element()
                                            }
                                        }
                                    })
                                    .collect()
                            },
                        )
                        .size_full()
                        .track_scroll(&scroll_handle),
                    ),
            )
    }
}

impl Focusable for PalettePanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for PalettePanel {
    fn panel_name(&self) -> &'static str {
        "palette"
    }

    fn title(&self, _window: &Window, _cx: &App) -> AnyElement {
        "Palette".into_any_element()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Palette row renderers (free functions so they can be called from the
// 'static virtual-list closure via the `cx: &mut Context<PalettePanel>`
// parameter that the closure receives on every invocation)
// ─────────────────────────────────────────────────────────────────────────────

/// Parse a CSS-style `"#RRGGBB"` hex string into a `gpui::Rgba`.
/// Falls back to the theme accent on any parse failure.
fn hex_color(hex: &str) -> Rgba {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        ) {
            return Rgba {
                r: r as f32 / 255.0,
                g: g as f32 / 255.0,
                b: b as f32 / 255.0,
                a: 1.0,
            };
        }
    }
    // Fallback: mid-grey
    Rgba { r: 0.6, g: 0.6, b: 0.6, a: 1.0 }
}

/// Compact category-header row (non-interactive).
fn palette_category_header(
    name: &str,
    color: &str,
    node_count: usize,
    cx: &mut Context<PalettePanel>,
) -> impl IntoElement {
    let cat_color: Hsla = hex_color(color).into();

    h_flex()
        .w_full()
        .h(px(CATEGORY_HEADER_H))
        .items_center()
        .justify_between()
        .bg(cx.theme().muted.opacity(0.15))
        .border_b_1()
        .border_color(cx.theme().border.opacity(0.3))
        // Coloured left-edge accent bar
        .child(
            div()
                .w(px(3.0))
                .h(px(CATEGORY_HEADER_H))
                .flex_shrink_0()
                .bg(cat_color.opacity(0.7)),
        )
        .child(
            div()
                .flex_1()
                .px_2()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(cx.theme().muted_foreground)
                .child(name.to_uppercase()),
        )
        .child(
            div()
                .px_3()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(node_count.to_string()),
        )
}

/// Node-entry row with icon, name, description, and a click-to-place handler.
fn palette_node_row(
    ix: usize,
    def: NodeDefinition,
    category_color: &str,
    cx: &mut Context<PalettePanel>,
) -> impl IntoElement {
    let icon_bg: Hsla = hex_color(category_color).into();
    let def_for_click = def.clone();

    h_flex()
        .id(("palette-node", ix as u64))
        .w_full()
        .h(px(NODE_ENTRY_H))
        .px_3()
        .gap_2()
        .items_center()
        .cursor_pointer()
        .border_b_1()
        .border_color(cx.theme().border.opacity(0.15))
        .hover(|s| s.bg(cx.theme().accent.opacity(0.06)))
        // ── Icon pill ────────────────────────────────────────────────────────
        .child(
            div()
                .w(px(28.0))
                .h(px(28.0))
                .flex_shrink_0()
                .rounded_full()
                .bg(icon_bg.opacity(0.18))
                .flex()
                .items_center()
                .justify_center()
                .text_base()
                .child(def.icon.clone()),
        )
        // ── Name + one-line description ───────────────────────────────────────
        .child(
            v_flex()
                .flex_1()
                .min_w_0()  // allows flex child to shrink below content width
                .gap_0p5()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
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
        // ── Click → place node at graph centre ────────────────────────────────
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |palette_panel, _event, _window, cx| {
                // Clone once per click (Fn – can be called many times).
                let def_now = def_for_click.clone();
                if let Some(editor) = palette_panel.editor.upgrade() {
                    editor.update(cx, |ep, cx| {
                        let screen_pos = ep
                            .graph_element_bounds
                            .map(|b| b.center())
                            .unwrap_or_else(|| Point::new(px(640.0), px(360.0)));
                        let graph_pos =
                            NodeGraphRenderer::screen_to_graph_pos(screen_pos, &ep.graph);
                        let stagger = (ep.graph.nodes.len() % 8) as f32 * 18.0;
                        let place_pos =
                            Point::new(graph_pos.x + stagger, graph_pos.y + stagger);
                        let node = BlueprintNode::from_definition(&def_now, place_pos);
                        ep.add_node(node, cx);
                    });
                }
            }),
        )
}

/// Main graph canvas panel with tab bar
pub struct GraphCanvasPanel {
    editor: WeakEntity<BlueprintEditorPanel>,
    tab_id: String,
    focus_handle: FocusHandle,
}

impl GraphCanvasPanel {
    pub fn new(editor: WeakEntity<BlueprintEditorPanel>, tab_id: String, cx: &mut Context<Self>) -> Self {
        Self {
            editor,
            tab_id,
            focus_handle: cx.focus_handle(),
        }
    }
}

impl EventEmitter<PanelEvent> for GraphCanvasPanel {}

impl Render for GraphCanvasPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(editor) = self.editor.upgrade() {
            div()
                .size_full()
                .child(editor.update(cx, |editor, cx| {
                    editor.ensure_active_graph_panel_state(&self.tab_id);
                    NodeGraphRenderer::render(editor, &self.tab_id, cx)
                }))
        } else {
            div().child("Editor not available")
        }
    }
}

impl Focusable for GraphCanvasPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for GraphCanvasPanel {
    fn panel_name(&self) -> &'static str {
        "graph-canvas"
    }

    fn title(&self, _window: &Window, cx: &App) -> AnyElement {
        if let Some(editor) = self.editor.upgrade() {
            let editor = editor.read(cx);
            if let Some(tab) = editor.open_tabs.iter().find(|tab| tab.id == self.tab_id) {
                let title = if tab.is_dirty {
                    format!("{} *", tab.name)
                } else {
                    tab.name.clone()
                };
                return title.into_any_element();
            }
        }

        "Event Graph".into_any_element()
    }
}
