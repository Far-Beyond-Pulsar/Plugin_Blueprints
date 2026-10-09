//! Stable identity for nodes that call a subgraph.
//!
//! New call nodes encode the subgraph kind in `definition_id`. Older assets
//! used `macro:<id>` for both macros and collapsed regions, so parsing accepts
//! that form and resolves its kind from the saved subgraph catalog.

use blueprint_graph::{SubGraph, SubGraphKind};

const TYPED_PREFIX: &str = "subgraph:";
const LEGACY_PREFIX: &str = "macro:";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubGraphReference {
    pub id: String,
    pub kind: SubGraphKind,
}

impl SubGraphReference {
    pub fn new(id: impl Into<String>, kind: SubGraphKind) -> Self {
        Self {
            id: id.into(),
            kind,
        }
    }

    /// Encode a node definition id with the role explicit in newly saved data.
    pub fn encode(&self) -> String {
        let kind = match self.kind {
            SubGraphKind::Macro => "macro",
            SubGraphKind::Collapsed => "collapsed",
        };
        format!("{TYPED_PREFIX}{kind}:{}", self.id)
    }

    /// Decode current typed ids and legacy ids, resolving old ambiguous ids
    /// against the subgraphs saved alongside the call node.
    pub fn decode(definition_id: &str, subgraphs: &[SubGraph]) -> Option<Self> {
        let (id, encoded_kind) = if let Some(rest) = definition_id.strip_prefix(TYPED_PREFIX) {
            if let Some(id) = rest.strip_prefix("macro:") {
                (id, Some(SubGraphKind::Macro))
            } else if let Some(id) = rest.strip_prefix("collapsed:") {
                (id, Some(SubGraphKind::Collapsed))
            } else {
                // Early editor builds emitted `subgraph:<id>` for palette
                // definitions. Keep reading those as legacy references too.
                (rest, None)
            }
        } else if let Some(id) = definition_id.strip_prefix(LEGACY_PREFIX) {
            (id, None)
        } else {
            return None;
        };

        if id.is_empty() {
            return None;
        }

        let kind = encoded_kind
            .or_else(|| {
                subgraphs
                    .iter()
                    .find(|subgraph| subgraph.id == id)
                    .map(|subgraph| subgraph.kind)
            })
            .unwrap_or_default();
        Some(Self::new(id, kind))
    }

    /// Read only the subgraph id when a caller does not have the catalog yet.
    pub fn id_from_definition_id(definition_id: &str) -> Option<&str> {
        let rest = definition_id
            .strip_prefix(TYPED_PREFIX)
            .unwrap_or(definition_id);
        let rest = rest
            .strip_prefix("macro:")
            .or_else(|| rest.strip_prefix("collapsed:"))
            .unwrap_or(rest);
        (!rest.is_empty()).then_some(rest)
    }

    /// The compiler's graph expansion API still expects the historical node
    /// type. Keep that backend adapter in one place while saves use typed ids.
    pub fn encode_for_compiler(&self) -> String {
        format!("{LEGACY_PREFIX}{}", self.id)
    }
}

pub fn definition_id_matches(
    definition_id: &str,
    reference: &SubGraphReference,
    subgraphs: &[SubGraph],
) -> bool {
    SubGraphReference::decode(definition_id, subgraphs).as_ref() == Some(reference)
}
