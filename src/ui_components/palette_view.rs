//! Shared node-palette UI component.
//!
//! [`NodePaletteView`] renders the full palette UI — search bar, category
//! headers, and node rows — and can be embedded in **both** the right-side
//! [`PalettePanel`] dock tab and the quick-access right-click overlay on the
//! graph canvas.
//!
//! When a node row is clicked the node is placed in the graph and
//! `BlueprintEditorPanel::quick_palette_open` is set to `false`, which causes
//! the overlay to dismiss itself.  In the dock-panel context that flag is
//! already `false`, so the write is a harmless no-op.

use gpui::prelude::*;
use gpui::*;
use ui::{
    h_flex,
    input::{InputState, TextInput},
    scroll::{Scrollbar, ScrollbarState},
    v_flex, v_virtual_list, ActiveTheme, Icon, IconName, VirtualListScrollHandle,
};

use crate::core::definitions::{NodeDefinition, NodeDefinitions};
use crate::core::types::BlueprintNode;
use crate::editor::panel::BlueprintEditorPanel;
use crate::rendering::graph::NodeGraphRenderer;
use crate::ui_components::node_library::{
    build_compatible_palette_items, build_item_sizes, build_palette_items, count_nodes,
    filter_palette_items, PaletteItem, CATEGORY_HEADER_H, NODE_ENTRY_H,
};

// ─────────────────────────────────────────────────────────────────────────────
// Component
// ─────────────────────────────────────────────────────────────────────────────

/// Shared palette UI — search bar + category headers + node rows.
///
/// Embed this entity in any parent that needs to show the node library.
pub struct NodePaletteView {
    pub editor: WeakEntity<BlueprintEditorPanel>,
    focus_handle: FocusHandle,
    /// Full unfiltered flat list — built once, never mutated.
    all_items: Vec<PaletteItem>,
    /// Drives the search-filter text input.
    search_input: Entity<InputState>,
    /// Allows programmatic scroll-to-top on search change.
    scroll_handle: VirtualListScrollHandle,
    /// Tracks scroll position for the scrollbar thumb.
    scrollbar_state: ScrollbarState,
    /// Tracks whether component nodes have been loaded
    component_nodes_loaded: bool,
}

impl NodePaletteView {
    pub fn new(
        editor: WeakEntity<BlueprintEditorPanel>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // Build base node items only - component nodes will be loaded lazily
        let all_items = build_palette_items(NodeDefinitions::load());

        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search nodes…"));
        Self {
            editor,
            focus_handle: cx.focus_handle(),
            all_items,
            search_input,
            scroll_handle: VirtualListScrollHandle::new(),
            scrollbar_state: ScrollbarState::default(),
            component_nodes_loaded: false,
        }
    }

    /// Rebuild the palette items (called when prefab or macros change).
    pub fn rebuild_items(&mut self, cx: &mut Context<Self>) {
        let mut all_items = build_palette_items(NodeDefinitions::load());

        if let Some(editor_entity) = self.editor.upgrade() {
            let editor_ref = editor_entity.read(cx);

            // Component method nodes
            let component_categories =
                NodeDefinitions::generate_component_nodes(&editor_ref.prefab_asset);
            for category in component_categories {
                all_items.extend(build_palette_items_for_category(&category));
            }

            // Local macros — filtered to exclude the macro currently being edited
            // (prevents a macro from containing itself).
            let editing_macro_id = editor_ref.current_editing_macro_id().map(str::to_owned);
            let local_macro_items =
                build_local_macro_palette_items(&editor_ref.local_macros, editing_macro_id.as_deref());
            all_items.extend(local_macro_items);
        }

        self.all_items = all_items;
        self.component_nodes_loaded = true;
    }
}

/// Build palette items for all local macros, excluding `editing_macro_id` to
/// prevent a macro from being placed inside itself.
fn build_local_macro_palette_items(
    macros: &[ui::graph::SubGraphDefinition],
    editing_macro_id: Option<&str>,
) -> Vec<PaletteItem> {
    use crate::core::definitions::{NodeDefinition, PinDefinition};
    use crate::core::types::PinType;

    let visible: Vec<_> = macros
        .iter()
        .filter(|m| Some(m.id.as_str()) != editing_macro_id)
        .collect();

    if visible.is_empty() {
        return Vec::new();
    }

    let mut items = vec![PaletteItem::CategoryHeader {
        name: "Local Macros".to_string(),
        color: "#9B59B6".to_string(),
        node_count: visible.len(),
    }];

    for m in visible {
        let inputs = m
            .interface
            .inputs
            .iter()
            .map(|p| PinDefinition {
                id: p.id.clone(),
                name: p.name.clone(),
                data_type: p.data_type.clone(),
                pin_type: PinType::Input,
            })
            .collect();
        let outputs = m
            .interface
            .outputs
            .iter()
            .map(|p| PinDefinition {
                id: p.id.clone(),
                name: p.name.clone(),
                data_type: p.data_type.clone(),
                pin_type: PinType::Output,
            })
            .collect();

        items.push(PaletteItem::NodeEntry {
            def: NodeDefinition {
                id: format!("macro:{}", m.id),
                name: m.name.clone(),
                icon: "📦".to_string(),
                description: m.description.clone(),
                documentation: m.description.clone(),
                inputs,
                outputs,
                properties: std::collections::HashMap::new(),
                color: Some("#9B59B6".to_string()),
            },
            category_color: "#9B59B6".to_string(),
        });
    }
    items
}

/// Build palette items for a single category
fn build_palette_items_for_category(
    category: &crate::core::definitions::NodeCategory,
) -> Vec<PaletteItem> {
    let mut items = Vec::new();
    items.push(PaletteItem::CategoryHeader {
        name: category.name.clone(),
        color: category.color.clone(),
        node_count: category.nodes.len(),
    });
    for node in &category.nodes {
        items.push(PaletteItem::NodeEntry {
            def: node.clone(),
            category_color: category.color.clone(),
        });
    }
    items
}

impl NodePaletteView {
    /// Return the focus handle of the search input so callers can focus it.
    pub fn search_focus_handle(&self, cx: &App) -> FocusHandle {
        self.search_input.read(cx).focus_handle(cx)
    }
}

impl Focusable for NodePaletteView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for NodePaletteView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Load component nodes on first render (safe because editor construction is complete)
        if !self.component_nodes_loaded {
            self.rebuild_items(cx);
        }

        // ── Filtered list ─────────────────────────────────────────────────────
        let query = self.search_input.read(cx).value().to_string();
        let connection_filter_type = self
            .editor
            .upgrade()
            .and_then(|editor| {
                let editor = editor.read(cx);
                editor.quick_palette_connection_source.as_ref().cloned()
            })
            .map(|drag| drag.source_pin_type);

        let items = if let Some(source_type) = connection_filter_type {
            build_compatible_palette_items(NodeDefinitions::load(), &source_type)
        } else {
            self.all_items.clone()
        };
        let visible = filter_palette_items(&items, &query);
        let node_count = count_nodes(&visible);
        let item_sizes = build_item_sizes(&visible);

        // Owned snapshot for the 'static virtual-list closure.
        let items_snap = visible;
        let view_entity = cx.entity().clone();
        let scroll_handle = self.scroll_handle.clone();
        let scrollbar_state = self.scrollbar_state.clone();

        v_flex()
            .size_full()
            .bg(cx.theme().sidebar)
            .track_focus(&self.focus_handle)
            // ── Header: title row + search box ────────────────────────────────
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
            // ── Scrollable node list ───────────────────────────────────────────
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .relative()
                    .child(
                        v_virtual_list(
                            view_entity,
                            "node-palette-view-list",
                            item_sizes,
                            move |_view, range, _window, cx| {
                                range
                                    .map(|ix| -> AnyElement {
                                        let Some(item) = items_snap.get(ix) else {
                                            return div().h(px(NODE_ENTRY_H)).into_any_element();
                                        };
                                        match item {
                                            PaletteItem::CategoryHeader {
                                                name,
                                                color,
                                                node_count,
                                            } => palette_category_header(
                                                name,
                                                color,
                                                *node_count,
                                                cx,
                                            )
                                            .into_any_element(),
                                            PaletteItem::NodeEntry {
                                                def,
                                                category_color,
                                            } => palette_node_row(
                                                ix,
                                                def.clone(),
                                                category_color,
                                                cx,
                                            )
                                            .into_any_element(),
                                        }
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
                    ),
            )
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Row renderers
// ─────────────────────────────────────────────────────────────────────────────

/// Parse a CSS-style `"#RRGGBB"` hex string into a [`gpui::Rgba`].
/// Falls back to mid-grey on any parse failure.
pub fn hex_color(hex: &str) -> Rgba {
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
    Rgba {
        r: 0.6,
        g: 0.6,
        b: 0.6,
        a: 1.0,
    }
}

/// Compact non-interactive category-header row.
fn palette_category_header(
    name: &str,
    color: &str,
    node_count: usize,
    cx: &mut Context<NodePaletteView>,
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

/// Clickable node-entry row: icon pill + name/description + placement handler.
fn palette_node_row(
    ix: usize,
    def: NodeDefinition,
    category_color: &str,
    cx: &mut Context<NodePaletteView>,
) -> impl IntoElement {
    let icon_bg: Hsla = hex_color(category_color).into();
    let def_for_click = def.clone();

    h_flex()
        .id(("node-palette-view-node", ix as u64))
        .w_full()
        .h(px(NODE_ENTRY_H))
        .px_3()
        .gap_2()
        .items_center()
        .cursor_pointer()
        .border_b_1()
        .border_color(cx.theme().border.opacity(0.15))
        .hover(|s| s.bg(cx.theme().accent.opacity(0.06)))
        // Icon pill
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
        // Name + description
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
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
        // Click → place node, then close the quick-palette overlay (if open)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _event, _window, cx| {
                let def_now = def_for_click.clone();
                if let Some(editor) = view.editor.upgrade() {
                    editor.update(cx, |ep, cx| {
                        // Prefer the right-click position; fall back to graph centre.
                        let base = ep
                            .popup_palette_graph_pos
                            .or_else(|| {
                                ep.graph_element_bounds.map(|b| {
                                    let center = b.center();
                                    let gp =
                                        NodeGraphRenderer::screen_to_graph_pos(center, &ep.graph);
                                    Point::new(gp.x, gp.y)
                                })
                            })
                            .unwrap_or(Point::new(0.0, 0.0));

                        let stagger = (ep.graph.nodes.len() % 8) as f32 * 18.0;
                        let place_pos = Point::new(base.x + stagger, base.y + stagger);

                        // Local macros use a special "macro:<id>" definition_id.
                        let node_clone = if let Some(macro_id) = def_now.id.strip_prefix("macro:") {
                            ep.create_macro_instance_node(macro_id.to_string(), place_pos, cx);
                            // For connection completion we need the just-added node.
                            ep.graph.nodes.last().cloned()
                        } else {
                            let node = BlueprintNode::from_definition(&def_now, place_pos);
                            let clone = node.clone();
                            ep.add_node(node, cx);
                            Some(clone)
                        };

                        if let (Some(source), Some(ref new_node)) =
                            (ep.quick_palette_connection_source.take(), node_clone.as_ref())
                        {
                            ep.complete_connection_to_new_node(source, new_node, cx);
                        }

                        ep.popup_palette_graph_pos = None;
                        ep.quick_palette_connection_source = None;
                        // Dismiss the quick-palette overlay when it is active.
                        // In the dock-panel context this is always false already.
                        ep.quick_palette_open = false;
                        ep.quick_palette_focus_pending = false;
                        cx.notify();
                    });
                }
            }),
        )
}
