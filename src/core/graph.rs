//! Blueprint graph container and state management.
//!
//! This module defines the main `BlueprintGraph` type that holds all nodes,
//! connections, comments, and view state for a single blueprint document.

use super::types::{BlueprintComment, BlueprintNode, Connection, VirtualizationStats};
use gpui::*;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_COLLECTION_ID: AtomicU64 = AtomicU64::new(1);

fn next_collection_id() -> u64 {
    NEXT_COLLECTION_ID.fetch_add(1, Ordering::Relaxed)
}

/// A Vec-compatible collection that records access to mutable contents.
///
/// Blueprint editor geometry is cached in spatial indexes. The editor has a
/// large number of mutation paths (undo, AI edits, macro expansion, drag, and
/// property editing), so invalidation must be tied to mutable access instead
/// of relying on every caller to remember a manual dirty flag.
#[derive(Clone, Debug)]
pub struct RevisionedVec<T> {
    values: Vec<T>,
    revision: u64,
    collection_id: u64,
}

impl<T> Default for RevisionedVec<T> {
    fn default() -> Self {
        Self {
            values: Vec::new(),
            revision: 0,
            collection_id: next_collection_id(),
        }
    }
}

impl<T> RevisionedVec<T> {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn collection_id(&self) -> u64 {
        self.collection_id
    }

    /// Mutate several elements as one revision so hot-path batch edits can
    /// update their spatial-index entries incrementally.
    pub fn with_mut<R>(&mut self, edit: impl FnOnce(&mut Vec<T>) -> R) -> R {
        self.revision = self.revision.wrapping_add(1);
        edit(&mut self.values)
    }
}

impl<T> From<Vec<T>> for RevisionedVec<T> {
    fn from(values: Vec<T>) -> Self {
        Self {
            values,
            revision: 0,
            collection_id: next_collection_id(),
        }
    }
}

impl<T> Deref for RevisionedVec<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

impl<T> DerefMut for RevisionedVec<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.revision = self.revision.wrapping_add(1);
        &mut self.values
    }
}

impl<'a, T> IntoIterator for &'a RevisionedVec<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.values.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut RevisionedVec<T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.revision = self.revision.wrapping_add(1);
        self.values.iter_mut()
    }
}

impl<T> IntoIterator for RevisionedVec<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.values.into_iter()
    }
}

impl<T> std::iter::FromIterator<T> for RevisionedVec<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        iter.into_iter().collect::<Vec<_>>().into()
    }
}

impl<T> AsRef<[T]> for RevisionedVec<T> {
    fn as_ref(&self) -> &[T] {
        &self.values
    }
}

/// A single field in an event definition.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CustomEventField {
    pub name: String,
    pub type_name: String,
}

/// Shared definition for a custom event, stored at the panel level (mirrors
/// how macros are stored).  The On node and all Dispatch nodes derive their
/// pins from this definition.
#[derive(Clone, Debug, Default)]
pub struct EventDefinition {
    pub uid: String,
    pub name: String,
    pub fields: Vec<CustomEventField>,
    pub return_type: String,
}

/// The main container for a blueprint graph, including all nodes, connections,
/// comments, selection state, and viewport information.
#[derive(Clone, Debug, Default)]
pub struct BlueprintGraph {
    pub nodes: RevisionedVec<BlueprintNode>,
    pub connections: RevisionedVec<Connection>,
    pub comments: RevisionedVec<BlueprintComment>,
    pub selected_nodes: Vec<String>,
    pub selected_comments: Vec<String>,
    pub zoom_level: f32,
    pub pan_offset: Point<f32>,
    pub virtualization_stats: VirtualizationStats,
}
