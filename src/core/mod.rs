//! Core module for the Blueprint Editor
//!
//! - `types`: pin data types, compile status, node classification
//! - `graph`: event definitions shared by the side panels
//! - `definitions`: node definition system for loading and managing node metadata
//! - `events`: actions and event types for editor interactions
//! - `subgraph_ref`: macro/collapsed subgraph call references

pub mod definitions;
pub mod events;
pub mod graph;
pub mod subgraph_ref;
pub mod types;
