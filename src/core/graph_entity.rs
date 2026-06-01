//! Unified trait for all graph entities (nodes, comments, connections, etc.)

use gpui::*;

/// Common interface for all entities that can be placed on the graph
pub trait GraphEntity {
    /// Get unique identifier
    fn id(&self) -> &str;

    /// Get position in graph space
    fn position(&self) -> Point<f32>;

    /// Set position in graph space
    fn set_position(&mut self, pos: Point<f32>);

    /// Get size/bounds
    fn size(&self) -> Size<f32>;

    /// Check if a point is inside this entity's bounds
    fn contains_point(&self, point: Point<f32>) -> bool {
        let pos = self.position();
        let size = self.size();
        point.x >= pos.x
            && point.x <= pos.x + size.width
            && point.y >= pos.y
            && point.y <= pos.y + size.height
    }

    /// Check if this entity intersects with a rectangle
    fn intersects_rect(&self, min: Point<f32>, max: Point<f32>) -> bool {
        let pos = self.position();
        let size = self.size();
        let entity_right = pos.x + size.width;
        let entity_bottom = pos.y + size.height;

        !(entity_right < min.x || pos.x > max.x || entity_bottom < min.y || pos.y > max.y)
    }
}

/// Selection wrapper for graph entities
#[derive(Clone, Debug, PartialEq)]
pub enum EntitySelection {
    Node(String),
    Comment(String),
}

impl EntitySelection {
    pub fn id(&self) -> &str {
        match self {
            EntitySelection::Node(id) => id,
            EntitySelection::Comment(id) => id,
        }
    }

    pub fn is_node(&self) -> bool {
        matches!(self, EntitySelection::Node(_))
    }

    pub fn is_comment(&self) -> bool {
        matches!(self, EntitySelection::Comment(_))
    }
}

/// Helper for storing initial drag positions for any entity type
#[derive(Default, Clone)]
pub struct DragState {
    positions: Vec<(EntitySelection, Point<f32>)>,
}

impl DragState {
    pub fn new() -> Self {
        Self {
            positions: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.positions.clear();
    }

    pub fn add(&mut self, entity: EntitySelection, position: Point<f32>) {
        self.positions.push((entity, position));
    }

    pub fn iter(&self) -> impl Iterator<Item = &(EntitySelection, Point<f32>)> {
        self.positions.iter()
    }

    pub fn get(&self, entity: &EntitySelection) -> Option<Point<f32>> {
        self.positions
            .iter()
            .find(|(e, _)| e == entity)
            .map(|(_, pos)| *pos)
    }

    pub fn len(&self) -> usize {
        self.positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }
}
