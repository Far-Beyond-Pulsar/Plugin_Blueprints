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
        node_count: usize,
    },
    /// A draggable / clickable node entry.
    NodeEntry {
        def: NodeDefinition,
        category_color: String,
    },
}

impl PaletteItem {
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
        items.push(PaletteItem::CategoryHeader {
            name: category.name.clone(),
            color: category.color.clone(),
            node_count: category.nodes.len(),
        });
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

/// Return a filtered copy of `all_items`.
///
/// - **Empty query** → returns a clone of the full list (category headers
///   included).
/// - **Non-empty query** → strips all category headers and returns only node
///   entries whose `name` or `description` match the query (case-insensitive).
pub fn filter_palette_items(all_items: &[PaletteItem], query: &str) -> Vec<PaletteItem> {
    if query.is_empty() {
        return all_items.to_vec();
    }
    let q = query.to_lowercase();
    all_items
        .iter()
        .filter(|item| match item {
            PaletteItem::CategoryHeader { .. } => false,
            PaletteItem::NodeEntry { def, .. } => {
                def.name.to_lowercase().contains(&q) || def.description.to_lowercase().contains(&q)
            }
        })
        .cloned()
        .collect()
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

        items.push(PaletteItem::CategoryHeader {
            name: category.name.clone(),
            color: category.color.clone(),
            node_count: compatible_nodes.len(),
        });

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

    let mut flush_category =
        |header: &Option<(String, String)>, nodes: &mut Vec<PaletteItem>, out: &mut Vec<PaletteItem>| {
            if nodes.is_empty() {
                return;
            }

            if let Some((name, color)) = header {
                out.push(PaletteItem::CategoryHeader {
                    name: name.clone(),
                    color: color.clone(),
                    node_count: nodes.len(),
                });
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
            PaletteItem::NodeEntry { def, category_color } => {
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
        if !crate::features::connections::compatibility::are_types_compatible(source_type, &pin.data_type) {
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
        PaletteItem::CategoryHeader { name: name.into(), color: String::new(), node_count: 0 }
    }

    fn ids(items: &[PaletteItem]) -> Vec<String> {
        items
            .iter()
            .map(|item| match item {
                PaletteItem::CategoryHeader { name, node_count, .. } => format!("[{name} {node_count}]"),
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
        assert_eq!(ids(&strings), ["[Math 2]", "concat", "generic", "[Debug 2]", "print", "print_any"]);

        let ints = filter_compatible_palette_items(&all, &PinDataType::from_type_str("i64"));
        assert_eq!(ids(&ints), ["[Math 2]", "add_int", "generic", "[Debug 1]", "print_any"]);

        let exec = filter_compatible_palette_items(&all, &PinDataType::execution());
        assert_eq!(ids(&exec), ["[Flow 1]", "branch", "[Debug 2]", "print", "print_any"]);
    }
}
