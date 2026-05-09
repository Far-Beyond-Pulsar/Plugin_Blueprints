//! Legacy blueprint format support and conversion
//!
//! This module handles backward compatibility with older blueprint file formats.
//! It defines legacy structures and conversion logic to migrate old files to the
//! current format.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ui::graph::{Connection, ConnectionType, GraphDescription, NodeInstance};

// ============================================================================
// Legacy Graph Format
// ============================================================================

/// Legacy graph description format (pre-unified format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyGraphDescription {
    /// Nodes in the graph
    pub nodes: HashMap<String, NodeInstance>,

    /// Connections between nodes
    pub connections: Vec<LegacyConnection>,

    /// Graph metadata
    pub metadata: ui::graph::GraphMetadata,

    /// Blueprint comments (optional, added later)
    #[serde(default)]
    pub comments: Vec<LegacyBlueprintComment>,
}

// ============================================================================
// Legacy Connection Format
// ============================================================================

/// Legacy connection format - actually matches current format exactly
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyConnection {
    pub id: String,
    pub source_node: String,
    pub source_pin: String,
    pub target_node: String,
    pub target_pin: String,
    pub connection_type: ConnectionType,
}

// ============================================================================
// Legacy Comment Format
// ============================================================================

/// Legacy blueprint comment with HSL color format
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LegacyBlueprintComment {
    pub id: String,
    pub text: String,
    pub position: LegacyPosition,
    pub size: LegacySize,
    pub color: LegacyColor,
    pub contained_node_ids: Vec<String>,
}

/// Legacy position format
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LegacyPosition {
    pub x: f32,
    pub y: f32,
}

/// Legacy size format
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LegacySize {
    pub width: f32,
    pub height: f32,
}

/// Legacy color format using HSL
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LegacyColor {
    pub h: f32,
    pub s: f32,
    pub l: f32,
    pub a: f32,
}

// ============================================================================
// Conversion: Legacy -> Current Format
// ============================================================================

impl From<LegacyConnection> for Connection {
    fn from(legacy: LegacyConnection) -> Self {
        Connection {
            id: legacy.id,
            source_node: legacy.source_node,
            source_pin: legacy.source_pin,
            target_node: legacy.target_node,
            target_pin: legacy.target_pin,
            connection_type: legacy.connection_type,
        }
    }
}

impl From<LegacyGraphDescription> for GraphDescription {
    fn from(legacy: LegacyGraphDescription) -> Self {
        GraphDescription {
            nodes: legacy.nodes,
            connections: legacy.connections.into_iter().map(|c| c.into()).collect(),
            metadata: legacy.metadata,
            comments: legacy.comments.into_iter().map(|c| c.into()).collect(),
        }
    }
}

impl From<LegacyBlueprintComment> for ui::graph::BlueprintComment {
    fn from(legacy: LegacyBlueprintComment) -> Self {
        // Convert HSL to RGB for the color array
        let (r, g, b) = hsl_to_rgb(legacy.color.h, legacy.color.s, legacy.color.l);

        ui::graph::BlueprintComment {
            id: legacy.id,
            text: legacy.text,
            position: (legacy.position.x, legacy.position.y),
            size: (legacy.size.width, legacy.size.height),
            color: [r, g, b, legacy.color.a],
            contained_node_ids: legacy.contained_node_ids,
        }
    }
}

// ============================================================================
// Color Conversion Utilities
// ============================================================================

/// Convert HSL color to RGB
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s == 0.0 {
        return (l, l, l);
    }

    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;

    let hue_to_rgb = |p: f32, q: f32, mut t: f32| -> f32 {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            return p + (q - p) * 6.0 * t;
        }
        if t < 1.0 / 2.0 {
            return q;
        }
        if t < 2.0 / 3.0 {
            return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
        }
        p
    };

    (
        hue_to_rgb(p, q, h + 1.0 / 3.0),
        hue_to_rgb(p, q, h),
        hue_to_rgb(p, q, h - 1.0 / 3.0),
    )
}

/// Convert RGB color to HSL
#[allow(dead_code)]
fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    let l = (max + min) / 2.0;

    if delta == 0.0 {
        return (0.0, 0.0, l);
    }

    let s = if l < 0.5 {
        delta / (max + min)
    } else {
        delta / (2.0 - max - min)
    };

    let h = if max == r {
        ((g - b) / delta + if g < b { 6.0 } else { 0.0 }) / 6.0
    } else if max == g {
        ((b - r) / delta + 2.0) / 6.0
    } else {
        ((r - g) / delta + 4.0) / 6.0
    };

    (h, s, l)
}

// ============================================================================
// Legacy Format Detection
// ============================================================================

/// Attempt to parse content as legacy format
pub fn try_parse_legacy_format(json: &str) -> Result<GraphDescription, String> {
    let legacy_graph: LegacyGraphDescription = serde_json::from_str(json)
        .map_err(|e| format!("Failed to parse as legacy format: {}", e))?;

    Ok(legacy_graph.into())
}

/// Check if content appears to be in legacy format
pub fn is_legacy_format(json: &str) -> bool {
    // Try to parse as legacy format
    serde_json::from_str::<LegacyGraphDescription>(json).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hsl_to_rgb_grayscale() {
        let (r, g, b) = hsl_to_rgb(0.0, 0.0, 0.5);
        assert!((r - 0.5).abs() < 0.001);
        assert!((g - 0.5).abs() < 0.001);
        assert!((b - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_hsl_to_rgb_red() {
        let (r, g, b) = hsl_to_rgb(0.0, 1.0, 0.5);
        assert!((r - 1.0).abs() < 0.001);
        assert!(g.abs() < 0.001);
        assert!(b.abs() < 0.001);
    }

    #[test]
    fn test_legacy_connection_conversion() {
        let legacy_conn = LegacyConnection {
            id: "conn1".to_string(),
            source_node: "node1".to_string(),
            source_pin: "out".to_string(),
            target_node: "node2".to_string(),
            target_pin: "in".to_string(),
            connection_type: ConnectionType::Data,
        };

        let conn: Connection = legacy_conn.into();
        assert_eq!(conn.id, "conn1");
        assert_eq!(conn.source_node, "node1");
    }

    #[test]
    fn test_legacy_color_conversion() {
        let legacy_comment = LegacyBlueprintComment {
            id: "comment1".to_string(),
            text: "Test".to_string(),
            position: LegacyPosition { x: 0.0, y: 0.0 },
            size: LegacySize {
                width: 100.0,
                height: 100.0,
            },
            color: LegacyColor {
                h: 0.5,
                s: 0.3,
                l: 0.2,
                a: 0.3,
            },
            contained_node_ids: vec![],
        };

        let comment: ui::graph::BlueprintComment = legacy_comment.into();
        assert_eq!(comment.id, "comment1");
        assert_eq!(comment.text, "Test");
        assert_eq!(comment.color[3], 0.3); // Alpha should be preserved
    }
}
