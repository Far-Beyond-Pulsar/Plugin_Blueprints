//! Trait methods as event nodes.
//!
//! A trait on a Blueprint contributes one entry-point ("On <Class>.<method>")
//! node per required method, including its supertraits' methods, to the Add
//! menu under `Events/Trait/<Trait>`. The node is an ordinary engine event
//! node (`event::on::<Class>.<method>`, see `blueprint_compiler::palette`): the
//! method's parameters are its outputs, and the compiler declares the event on
//! the class, so the engine can call it by name (`pulsar_game`'s role hooks).
//!
//! The events are derived, never stored: they come from the trait ids in the
//! Blueprint's metadata and the project's traits, so removing the trait
//! removes the entries, and renaming or moving the trait asset changes
//! nothing. Placing the node is what provides the method for
//! `engine_fs::trait_conformance`.
//!
//! A trait method's return type has no counterpart in the event system, so
//! it is shown in the node's description but a handler cannot return it.
//!
//! # Removing a trait
//!
//! Nodes already placed are not deleted. Their event is no longer declared,
//! so they are *orphaned*: [`orphaned_events`] names them, the editor reports
//! how many there are, and the Blueprint does not compile until they are
//! deleted or the trait is added back (adding it back revives them, since the
//! node type is unchanged).

use std::collections::BTreeSet;

use blueprint_compiler::palette::{event_nodes, PaletteEvent};
use pulsar_script_vm::{EventField, EventSignature};

use crate::core::definitions::NodeDefinition;

/// Palette category prefix of every trait event.
pub const TRAIT_EVENT_CATEGORY: &str = "Events/Trait/";

/// One method a declared trait requires, in the terms the event system
/// needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraitMethodSpec {
    pub trait_id: String,
    pub trait_name: String,
    pub name: String,
    /// `(name, Rust type)` as the trait spells them.
    pub params: Vec<(String, String)>,
    pub return_type: String,
    pub doc: String,
}

/// The methods required by the traits `ids` (and their supertraits), each
/// name once, nearest trait first.
pub fn specs_for(db: &engine_fs::TraitDb, ids: &[String]) -> Vec<TraitMethodSpec> {
    let mut seen = BTreeSet::new();
    let mut specs = Vec::new();
    for id in ids {
        for (owner, method) in db.required_methods(id) {
            if !seen.insert(method.name.clone()) {
                continue;
            }
            specs.push(TraitMethodSpec {
                trait_id: owner.id.clone(),
                trait_name: owner.name.clone(),
                name: method.name.clone(),
                params: method
                    .params
                    .iter()
                    .map(|param| (param.name.clone(), param.type_name.clone()))
                    .collect(),
                return_type: method.return_type.clone(),
                doc: method.doc.clone().unwrap_or_default(),
            });
        }
    }
    specs
}

/// Script field types for the spec's parameters, or `None` when one has no
/// script representation (the method then has no event).
fn fields(spec: &TraitMethodSpec) -> Option<Vec<EventField>> {
    spec.params
        .iter()
        .map(|(name, ty)| {
            let script = blueprint_compiler::script_type(ty);
            if script.is_none() {
                tracing::warn!(
                    method = %spec.name, parameter = %name, ty = %ty,
                    "trait method has no event: parameter type is not a script type"
                );
            }
            Some(EventField::new(name.clone(), script?))
        })
        .collect()
}

/// The compiler's declaration of the trait events of class `class`.
pub fn event_sources(specs: &[TraitMethodSpec]) -> Vec<blueprint_compiler::EventSource> {
    specs
        .iter()
        .filter_map(|spec| {
            fields(spec)?;
            Some(blueprint_compiler::EventSource {
                uid: format!("trait:{}:{}", spec.trait_id, spec.name),
                name: spec.name.clone(),
                fields: spec.params.clone(),
            })
        })
        .collect()
}

/// The compiler's declarations of the trait events a Blueprint's metadata
/// asks for, resolved against the project's traits (ids first, then legacy
/// paths). `local` are the names of its own custom events, which win over a
/// trait event of the same name. Reads the trait index; the compile and
/// preflight paths call this, the editing paths never do.
pub fn declared_event_sources(
    metadata: &graphy_blueprint::BlueprintMetadata,
    project_root: &std::path::Path,
    local: &[&str],
) -> Vec<blueprint_compiler::EventSource> {
    if metadata.implemented_trait_ids.is_empty() && metadata.implemented_traits.is_empty() {
        return Vec::new();
    }
    let index = match engine_fs::BlueprintTraitIndex::load_or_rebuild(project_root) {
        Ok(index) => index,
        Err(error) => {
            tracing::warn!("Trait events not declared: could not read the project's traits: {error:#}");
            return Vec::new();
        }
    };
    let mut ids: Vec<String> = metadata
        .implemented_trait_ids
        .iter()
        .chain(&metadata.implemented_traits)
        .filter_map(|reference| index.resolve_trait(reference))
        .map(|record| record.id.clone())
        .collect();
    ids.dedup();
    event_sources(&specs_for(index.db(), &ids))
        .into_iter()
        .filter(|source| !local.contains(&source.name.as_str()))
        .collect()
}

/// The entry-point nodes for the Add menu, as `(category, definition)`.
pub fn node_definitions(class: &str, specs: &[TraitMethodSpec]) -> Vec<(String, NodeDefinition)> {
    let events: Vec<PaletteEvent> = specs
        .iter()
        .filter_map(|spec| {
            Some(PaletteEvent {
                signature: EventSignature {
                    id: 0,
                    name: blueprint_compiler::qualified_event_name(class, &spec.name),
                    fields: fields(spec)?,
                },
                category: format!("Trait/{}", spec.trait_name),
                declared_here: true,
            })
        })
        .collect();
    event_nodes(&events)
        .into_iter()
        .filter(|node| node.is_event)
        .map(|node| {
            let spec = specs
                .iter()
                .find(|spec| node.node_type.ends_with(&format!(".{}", spec.name)));
            let mut definition = crate::features::events::engine_events::node_definition(&node);
            if let Some(spec) = spec {
                let returns = if spec.return_type == "()" {
                    String::new()
                } else {
                    format!(" Returns `{}` in the trait (not carried by events).", spec.return_type)
                };
                definition.description = format!(
                    "Implements `{}::{}` of trait {}.{returns}",
                    spec.trait_name, spec.name, spec.trait_name
                );
                if !spec.doc.is_empty() {
                    definition.documentation = spec.doc.clone();
                }
            }
            (node.category.clone(), definition)
        })
        .collect()
}

/// Node types that were trait events of `removed` and are not provided any
/// more: the placed nodes among `placed` (`(node id, node type)`) that name
/// an event of `class` the trait contributed and `remaining` does not.
pub fn orphaned_events(
    class: &str,
    removed: &[TraitMethodSpec],
    remaining: &[TraitMethodSpec],
    local_events: &[String],
    placed: &[(String, String)],
) -> Vec<String> {
    let provided: BTreeSet<&str> = remaining
        .iter()
        .map(|spec| spec.name.as_str())
        .chain(local_events.iter().map(String::as_str))
        .collect();
    let mut orphans = Vec::new();
    for spec in removed.iter().filter(|spec| !provided.contains(spec.name.as_str())) {
        let node_type = format!(
            "event::on::{}",
            blueprint_compiler::qualified_event_name(class, &spec.name)
        );
        orphans.extend(
            placed
                .iter()
                .filter(|(_, placed_type)| placed_type == &node_type)
                .map(|(id, _)| id.clone()),
        );
    }
    orphans
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_fs::{TraitDb, TraitMethodRecord, TraitParamRecord, TraitRecord};

    fn db() -> TraitDb {
        let method = |name: &str, params: &[(&str, &str)], ret: &str| TraitMethodRecord {
            name: name.into(),
            params: params
                .iter()
                .map(|(n, t)| TraitParamRecord { name: (*n).into(), type_name: (*t).into() })
                .collect(),
            return_type: ret.into(),
            doc: None,
            has_default: false,
        };
        let record = |id: &str, name: &str, supers: &[&str], methods| TraitRecord {
            id: id.into(),
            path: format!("types/traits/{name}.trait.json"),
            name: name.into(),
            display_name: name.into(),
            description: None,
            builtin: false,
            contract_version: 0,
            supertraits: supers.iter().map(|s| s.to_string()).collect(),
            methods,
            type_index_id: None,
        };
        TraitDb::build(
            vec![
                record("a", "Base", &[], vec![method("tick", &[("dt", "f32")], "()")]),
                record(
                    "b",
                    "Mode",
                    &["a"],
                    vec![method("score", &[("who", "u32")], "i32"), method("odd", &[("x", "Weird<T>")], "()")],
                ),
            ],
            Vec::new(),
        )
    }

    #[test]
    fn a_trait_contributes_its_own_and_inherited_methods_once() {
        let specs = specs_for(&db(), &["b".to_owned(), "a".to_owned()]);
        let names: Vec<_> = specs.iter().map(|s| (s.trait_name.as_str(), s.name.as_str())).collect();
        assert_eq!(names, [("Mode", "score"), ("Mode", "odd"), ("Base", "tick")]);
    }

    #[test]
    fn events_are_declared_with_script_types_and_unrepresentable_ones_are_left_out() {
        let specs = specs_for(&db(), &["b".to_owned()]);
        let sources = event_sources(&specs);
        let names: Vec<_> = sources.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["score", "tick"]);
        assert_eq!(sources[0].uid, "trait:b:score");
        assert_eq!(sources[0].fields, [("who".to_owned(), "u32".to_owned())]);
    }

    #[test]
    fn nodes_are_entry_points_under_the_traits_category() {
        let specs = specs_for(&db(), &["b".to_owned()]);
        let nodes = node_definitions("Player", &specs);
        assert_eq!(nodes.len(), 2);
        let (category, score) = nodes.iter().find(|(_, d)| d.id.ends_with("Player.score")).unwrap();
        assert_eq!(category, "Events/Trait/Mode");
        assert!(score.is_event);
        assert!(score.outputs.iter().any(|pin| pin.id == "who"));
        assert!(score.description.contains("Returns `i32`"));
        assert!(nodes.iter().all(|(c, _)| c.starts_with(TRAIT_EVENT_CATEGORY)));
    }

    #[test]
    fn removing_a_trait_orphans_only_placed_nodes_nothing_else_provides() {
        let removed = specs_for(&db(), &["b".to_owned()]);
        let remaining = specs_for(&db(), &["a".to_owned()]);
        let placed = vec![
            ("n1".to_owned(), "event::on::Player.score".to_owned()),
            ("n2".to_owned(), "event::on::Player.tick".to_owned()),
            ("n3".to_owned(), "event::on::Player.other".to_owned()),
        ];
        assert_eq!(orphaned_events("Player", &removed, &remaining, &[], &placed), ["n1"]);
        // A custom event of the same name still provides it.
        assert!(orphaned_events("Player", &removed, &remaining, &["score".to_owned()], &placed).is_empty());
    }
}
