//! Palette item types and flat-list helpers for the virtual-list Palette panel.
//!
//! The `PalettePanel` renders its node library as a single flat `Vec<PaletteItem>`
//! fed into `v_virtual_list`.  This module owns:
//!
//! - The [`PaletteItem`] enum (category headers + node rows)
//! - Row-height constants
//! - Helper functions for building, filtering, and sizing the list

use crate::core::definitions::{NodeDefinition, NodeDefinitions, PinDefinition};
use crate::core::types::{PinDataType, PinType};
use gpui::{px, size, Pixels, Size};
use std::collections::HashSet;
use std::rc::Rc;

// ─────────────────────────────────────────────────────────────────────────────
// Row-height constants
// ─────────────────────────────────────────────────────────────────────────────

/// Height of a category-header row in the palette list.
pub const CATEGORY_HEADER_H: f32 = 28.0;

/// Height of a node-entry row in the palette list.
pub const NODE_ENTRY_H: f32 = 52.0;

// ─────────────────────────────────────────────────────────────────────────────
// Item type
// ─────────────────────────────────────────────────────────────────────────────

/// A single row in the palette's virtual list.
#[derive(Clone, Debug)]
pub enum PaletteItem {
    /// A section separator labelled with the category name.
    CategoryHeader {
        name: String,
        color: String,
        /// Nodes in the category.
        node_count: usize,
        /// Whether the category's rows are showing (set by [`visible_items`]).
        expanded: bool,
        /// While a search is narrowing the list: how many nodes in this
        /// category match. `None` when nothing is filtering.
        matched: Option<usize>,
    },
    /// A draggable / clickable node entry.
    NodeEntry {
        def: NodeDefinition,
        category_color: String,
    },
}

impl PaletteItem {
    /// A category header, collapsed. How it is shown is decided later by
    /// [`visible_items`].
    pub fn category(name: String, color: String, node_count: usize) -> Self {
        Self::CategoryHeader {
            name,
            color,
            node_count,
            expanded: false,
            matched: None,
        }
    }

    /// Pixel height for this row type.
    #[inline]
    pub fn height(&self) -> f32 {
        match self {
            Self::CategoryHeader { .. } => CATEGORY_HEADER_H,
            Self::NodeEntry { .. } => NODE_ENTRY_H,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Build the complete flat list from the global node definitions,
/// optionally augmented with custom event dispatch entries and the
/// "Add Custom Event" sentinel.
///
/// The list is ordered: category header, then all nodes in that category,
/// then the next category header, and so on.
pub fn build_palette_items(defs: &NodeDefinitions) -> Vec<PaletteItem> {
    let mut items = Vec::new();
    for category in &defs.categories {
        items.push(PaletteItem::category(
            category.name.clone(),
            category.color.clone(),
            category.nodes.len(),
        ));
        for def in &category.nodes {
            items.push(PaletteItem::NodeEntry {
                def: def.clone(),
                category_color: category.color.clone(),
            });
        }

        // Inject "Add Custom Event" sentinel into the Events category.
        if category.name == "Events" {
            items.push(PaletteItem::NodeEntry {
                def: NodeDefinition {
                    id: "__add_custom_event__".to_string(),
                    name: "Add Custom Event".to_string(),
                    icon: "📡".to_string(),
                    description: "Create a new custom event definition".to_string(),
                    documentation: String::new(),
                    inputs: Vec::new(),
                    outputs: Vec::new(),
                    properties: std::collections::HashMap::new(),
                    color: Some("#E67E22".to_string()),
                    is_event: false,
                },
                category_color: category.color.clone(),
            });
            // Test node that returns a fn pointer (for testing return pin connections)
            items.push(PaletteItem::NodeEntry {
                def: NodeDefinition {
                    id: "__fn_return_test__".to_string(),
                    name: "FnReturn (test)".to_string(),
                    icon: "🔙".to_string(),
                    description: "Test node that outputs a fn pointer (f32 -> f32)".to_string(),
                    documentation: String::new(),
                    inputs: vec![PinDefinition {
                        id: "__fn_ptr__".to_string(),
                        name: "fn(f32)->f32".to_string(),
                        data_type: crate::core::types::PinDataType::from_type_str("fn(f32)->f32"),
                        pin_type: PinType::Input,
                    }],
                    outputs: Vec::new(),
                    properties: std::collections::HashMap::new(),
                    color: Some("#FF3333".to_string()),
                    is_event: false,
                },
                category_color: category.color.clone(),
            });
        }
    }
    items
}

/// Build the `Rc<Vec<Size<Pixels>>>` required by `v_virtual_list`.
///
/// Width is `px(0.0)` (stretch to available width); height is taken from
/// [`PaletteItem::height`].
pub fn build_item_sizes(items: &[PaletteItem]) -> Rc<Vec<Size<Pixels>>> {
    Rc::new(
        items
            .iter()
            .map(|item| size(px(0.0), px(item.height())))
            .collect(),
    )
}

/// Whether a node matches a search query (already lowercase, non-empty): its
/// name or description contains it.
fn node_matches(def: &NodeDefinition, query: &str) -> bool {
    def.name.to_lowercase().contains(query) || def.description.to_lowercase().contains(query)
}

/// The rows to show for `all_items`.
///
/// Categories are collapsed by default; what a category shows depends on
/// three things:
///
/// - **Searching** (`query` not empty): a category with matches opens by
///   itself and shows *only* its matching nodes; a category without matches is
///   hidden altogether.
/// - **Manually expanded** (`expanded` holds its name): it shows *all* its
///   nodes, search or not, as long as it has a match when searching.
/// - **`open_all`**: every category opens, as when the list is already
///   narrowed to the nodes that can take a dragged wire.
///
/// Each header carries whether it is open and, while searching, how many of its
/// nodes matched.
pub fn visible_items(
    all_items: &[PaletteItem],
    query: &str,
    expanded: &HashSet<String>,
    open_all: bool,
) -> Vec<PaletteItem> {
    let query = query.trim().to_lowercase();
    let searching = !query.is_empty();

    // Group each header with the nodes after it.
    let mut groups: Vec<(Option<&PaletteItem>, Vec<&PaletteItem>)> = Vec::new();
    for item in all_items {
        match item {
            PaletteItem::CategoryHeader { .. } => groups.push((Some(item), Vec::new())),
            PaletteItem::NodeEntry { .. } => match groups.last_mut() {
                Some((_, nodes)) => nodes.push(item),
                None => groups.push((None, vec![item])),
            },
        }
    }

    let mut out = Vec::new();
    for (header, nodes) in groups {
        let matching: Vec<&PaletteItem> = if searching {
            nodes
                .iter()
                .copied()
                .filter(|n| matches!(n, PaletteItem::NodeEntry { def, .. } if node_matches(def, &query)))
                .collect()
        } else {
            nodes.clone()
        };
        if searching && matching.is_empty() {
            continue;
        }

        let Some(PaletteItem::CategoryHeader {
            name,
            color,
            node_count,
            ..
        }) = header
        else {
            // Nodes with no category are always shown.
            out.extend(matching.into_iter().cloned());
            continue;
        };
        let manual = expanded.contains(name);
        let auto = searching || open_all;
        let open = manual || auto;
        out.push(PaletteItem::CategoryHeader {
            name: name.clone(),
            color: color.clone(),
            node_count: *node_count,
            expanded: open,
            matched: searching.then_some(matching.len()),
        });
        if open {
            let shown = if manual { &nodes } else { &matching };
            out.extend(shown.iter().map(|n| (*n).clone()));
        }
    }
    out
}

/// How many nodes match `query` (every node when it is empty), however the
/// categories are folded.
pub fn matching_node_count(all_items: &[PaletteItem], query: &str) -> usize {
    let query = query.trim().to_lowercase();
    all_items
        .iter()
        .filter(|item| match item {
            PaletteItem::NodeEntry { def, .. } => query.is_empty() || node_matches(def, &query),
            PaletteItem::CategoryHeader { .. } => false,
        })
        .count()
}

/// Build a palette list containing only nodes that have at least one compatible input pin
/// for the given source pin type.
pub fn build_compatible_palette_items(
    defs: &NodeDefinitions,
    source_type: &crate::core::types::PinDataType,
) -> Vec<PaletteItem> {
    let mut items = Vec::new();
    for category in &defs.categories {
        let compatible_nodes: Vec<_> = category
            .nodes
            .iter()
            .filter(|def| {
                def.inputs.iter().any(|pin| {
                    crate::features::connections::compatibility::are_types_compatible(
                        source_type,
                        &pin.data_type,
                    )
                })
            })
            .cloned()
            .collect();

        if compatible_nodes.is_empty() {
            continue;
        }

        items.push(PaletteItem::category(
            category.name.clone(),
            category.color.clone(),
            compatible_nodes.len(),
        ));

        for def in compatible_nodes {
            items.push(PaletteItem::NodeEntry {
                def,
                category_color: category.color.clone(),
            });
        }
    }
    items
}

/// Filter an existing flat palette list to only categories/nodes that can accept
/// the given source pin type.
///
/// This preserves dynamically injected categories (e.g. prefab component methods,
/// local macros) that are already present in `all_items`.
pub fn filter_compatible_palette_items(
    all_items: &[PaletteItem],
    source_type: &crate::core::types::PinDataType,
) -> Vec<PaletteItem> {
    let mut result = Vec::new();
    let mut current_header: Option<(String, String)> = None;
    let mut current_nodes: Vec<PaletteItem> = Vec::new();

    let mut flush_category = |header: &Option<(String, String)>,
                              nodes: &mut Vec<PaletteItem>,
                              out: &mut Vec<PaletteItem>| {
        if nodes.is_empty() {
            return;
        }

        if let Some((name, color)) = header {
            out.push(PaletteItem::category(
                name.clone(),
                color.clone(),
                nodes.len(),
            ));
        }

        out.append(nodes);
    };

    // Within each category, nodes with an input of exactly the dragged type
    // come before nodes that only take it through a wildcard pin.
    let mut current_wildcard: Vec<PaletteItem> = Vec::new();

    for item in all_items {
        match item {
            PaletteItem::CategoryHeader { name, color, .. } => {
                current_nodes.append(&mut current_wildcard);
                flush_category(&current_header, &mut current_nodes, &mut result);
                current_header = Some((name.clone(), color.clone()));
            }
            PaletteItem::NodeEntry {
                def,
                category_color,
            } => {
                let entry = || PaletteItem::NodeEntry {
                    def: def.clone(),
                    category_color: category_color.clone(),
                };
                match input_match(def, source_type) {
                    Some(InputMatch::Exact) => current_nodes.push(entry()),
                    Some(InputMatch::Wildcard) => current_wildcard.push(entry()),
                    None => {}
                }
            }
        }
    }

    current_nodes.append(&mut current_wildcard);
    flush_category(&current_header, &mut current_nodes, &mut result);
    result
}

/// How a node can take a value of the dragged type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputMatch {
    /// An input of exactly that type.
    Exact,
    /// Only a wildcard (generic / reroute) input accepts it.
    Wildcard,
}

fn input_match(def: &NodeDefinition, source_type: &PinDataType) -> Option<InputMatch> {
    let mut best = None;
    for pin in &def.inputs {
        if !crate::features::connections::compatibility::are_types_compatible(
            source_type,
            &pin.data_type,
        ) {
            continue;
        }
        if source_type.is_wildcard() || !pin.data_type.is_wildcard() {
            return Some(InputMatch::Exact);
        }
        best = Some(InputMatch::Wildcard);
    }
    best
}

/// Count the number of `NodeEntry` items in a slice.
pub fn count_nodes(items: &[PaletteItem]) -> usize {
    items
        .iter()
        .filter(|i| matches!(i, PaletteItem::NodeEntry { .. }))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, inputs: &[&str]) -> PaletteItem {
        PaletteItem::NodeEntry {
            def: NodeDefinition {
                id: id.into(),
                name: id.into(),
                icon: String::new(),
                description: String::new(),
                documentation: String::new(),
                inputs: inputs
                    .iter()
                    .enumerate()
                    .map(|(i, ty)| PinDefinition {
                        id: format!("in{i}"),
                        name: format!("in{i}"),
                        data_type: PinDataType::from_type_str(*ty),
                        pin_type: PinType::Input,
                    })
                    .collect(),
                outputs: Vec::new(),
                properties: Default::default(),
                color: None,
                is_event: false,
            },
            category_color: String::new(),
        }
    }

    fn header(name: &str) -> PaletteItem {
        PaletteItem::category(name.into(), String::new(), 0)
    }

    fn ids(items: &[PaletteItem]) -> Vec<String> {
        items
            .iter()
            .map(|item| match item {
                PaletteItem::CategoryHeader {
                    name, node_count, ..
                } => format!("[{name} {node_count}]"),
                PaletteItem::NodeEntry { def, .. } => def.id.clone(),
            })
            .collect()
    }

    #[test]
    fn drag_filter_keeps_only_nodes_taking_the_type_exact_matches_first() {
        let all = vec![
            header("Math"),
            node("generic", &["?"]),
            node("add_int", &["i64", "i64"]),
            node("concat", &["String"]),
            header("Flow"),
            node("branch", &["execution", "bool"]),
            header("Debug"),
            node("print", &["execution", "String"]),
            node("print_any", &["execution", "?"]),
        ];

        let strings = filter_compatible_palette_items(&all, &PinDataType::from_type_str("String"));
        assert_eq!(
            ids(&strings),
            [
                "[Math 2]",
                "concat",
                "generic",
                "[Debug 2]",
                "print",
                "print_any"
            ]
        );

        let ints = filter_compatible_palette_items(&all, &PinDataType::from_type_str("i64"));
        assert_eq!(
            ids(&ints),
            ["[Math 2]", "add_int", "generic", "[Debug 1]", "print_any"]
        );

        let exec = filter_compatible_palette_items(&all, &PinDataType::execution());
        assert_eq!(
            ids(&exec),
            ["[Flow 1]", "branch", "[Debug 2]", "print", "print_any"]
        );
    }
}

#[cfg(test)]
mod fold_tests {
    use super::*;

    fn node(id: &str, name: &str, description: &str) -> PaletteItem {
        PaletteItem::NodeEntry {
            def: NodeDefinition {
                id: id.into(),
                name: name.into(),
                icon: String::new(),
                description: description.into(),
                documentation: String::new(),
                inputs: Vec::new(),
                outputs: Vec::new(),
                properties: Default::default(),
                color: None,
                is_event: false,
            },
            category_color: String::new(),
        }
    }

    fn library() -> Vec<PaletteItem> {
        vec![
            PaletteItem::category("Math".into(), String::new(), 3),
            node("add", "Add", "sum two numbers"),
            node("sub", "Subtract", "difference"),
            node("sin", "Sine", "trigonometry"),
            PaletteItem::category("Flow".into(), String::new(), 2),
            node("branch", "Branch", "if / else"),
            node("loop", "For Loop", "repeat"),
        ]
    }

    /// `[Name open matched]` for headers, ids for nodes.
    fn shape(items: &[PaletteItem]) -> Vec<String> {
        items
            .iter()
            .map(|item| match item {
                PaletteItem::CategoryHeader {
                    name,
                    expanded,
                    matched,
                    ..
                } => {
                    format!(
                        "[{name} {} {matched:?}]",
                        if *expanded { "open" } else { "shut" }
                    )
                }
                PaletteItem::NodeEntry { def, .. } => def.id.clone(),
            })
            .collect()
    }

    fn none() -> HashSet<String> {
        HashSet::new()
    }

    #[test]
    fn everything_starts_folded() {
        let rows = visible_items(&library(), "", &none(), false);
        assert_eq!(shape(&rows), ["[Math shut None]", "[Flow shut None]"]);
    }

    #[test]
    fn a_category_the_user_opened_shows_all_its_nodes() {
        let open: HashSet<String> = ["Math".to_string()].into();
        let rows = visible_items(&library(), "", &open, false);
        assert_eq!(
            shape(&rows),
            ["[Math open None]", "add", "sub", "sin", "[Flow shut None]"]
        );
    }

    #[test]
    fn a_search_opens_matching_categories_to_their_matches_only() {
        let rows = visible_items(&library(), "s", &none(), false);
        // Matches by name or description: Add (sum), Subtract, Sine / Branch (else).
        assert_eq!(
            shape(&rows),
            [
                "[Math open Some(3)]",
                "add",
                "sub",
                "sin",
                "[Flow open Some(1)]",
                "branch"
            ]
        );

        let rows = visible_items(&library(), "loop", &none(), false);
        assert_eq!(
            shape(&rows),
            ["[Flow open Some(1)]", "loop"],
            "Math has no match and is hidden"
        );
    }

    #[test]
    fn a_hand_opened_category_stays_complete_during_a_search() {
        let open: HashSet<String> = ["Flow".to_string()].into();
        let rows = visible_items(&library(), "loop", &open, false);
        assert_eq!(shape(&rows), ["[Flow open Some(1)]", "branch", "loop"]);
    }

    #[test]
    fn a_hand_opened_category_without_a_match_is_still_hidden_while_searching() {
        let open: HashSet<String> = ["Math".to_string()].into();
        let rows = visible_items(&library(), "loop", &open, false);
        assert_eq!(shape(&rows), ["[Flow open Some(1)]", "loop"]);
    }

    #[test]
    fn searching_is_case_insensitive_and_ignores_surrounding_space() {
        let a = visible_items(&library(), "  SINE ", &none(), false);
        assert_eq!(shape(&a), ["[Math open Some(1)]", "sin"]);
    }

    #[test]
    fn a_search_with_no_hits_shows_nothing() {
        assert!(visible_items(&library(), "zzz", &none(), false).is_empty());
    }

    #[test]
    fn open_all_unfolds_every_category_without_a_search() {
        let rows = visible_items(&library(), "", &none(), true);
        assert_eq!(rows.len(), library().len());
        assert!(shape(&rows)[0].contains("open"));
    }

    #[test]
    fn the_node_count_ignores_folding() {
        assert_eq!(matching_node_count(&library(), ""), 5);
        assert_eq!(matching_node_count(&library(), "loop"), 1);
    }
}
