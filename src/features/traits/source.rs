//! Blueprints as trait implementers: the Blueprint plugin's
//! `pulsar_traits::TraitImplementerSource` (registered by this plugin's root,
//! `builtin_provider`), and the Blueprint-format parts of the trait system
//! that only this plugin knows: which files are Blueprints, what a Blueprint
//! declares (`blueprint_metadata`: `implemented_trait_ids`, and the older
//! `implemented_traits` paths) and what it provides (custom events, macros,
//! placed trait event nodes), checked with the engine's conformance rules
//! (`pulsar_traits::conformance`) at compile and preflight.
//!
//! A Blueprint compiles to a script class (`classes/<Name>/`), so the
//! implementer the index records carries `Binding::ScriptClass`: the game
//! runtime reaches its trait methods through the compiled class's handlers
//! and never reads a Blueprint.

use std::collections::BTreeSet;

use pulsar_traits::conformance::{
    check, is_unit_type, normalize_name, ConformanceIssue, IssueKind, ProvidedMethod,
    ProvidedParam, ProvidedReturn, Severity,
};
use pulsar_traits::{Binding, Declared, Scanned, TraitDb};
use serde_json::Value;

/// The implementer kind of Blueprints.
pub const BLUEPRINT_KIND: &str = "pulsar.blueprints.blueprint";

/// Blueprint `format_version`s the editor writes and reads (graphy-blueprint
/// `codec::is_version_supported`). Version 1 is the original; 2 is current.
const BLUEPRINT_FORMAT_VERSIONS: [u64; 2] = [1, 2];

pub fn is_blueprint_asset(path: &str) -> bool {
    path.ends_with(".blueprint.json")
        || std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            == Some("graph_save.json")
}

/// Parse a Blueprint asset. The editor writes `//` comment lines before the
/// JSON (graphy-blueprint `codec::strip_header_comments`); they are skipped
/// the same way. Anything else that is not JSON is `None`.
pub fn parse_blueprint_json(bytes: &[u8]) -> Option<Value> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut rest = text;
    loop {
        let trimmed = rest.trim_start_matches(['\r', '\n', ' ', '\t']);
        if let Some(comment) = trimmed.strip_prefix("//") {
            rest = comment.split_once('\n').map_or("", |(_, after)| after);
        } else {
            rest = trimmed;
            break;
        }
    }
    serde_json::from_str(rest).ok()
}

/// What a Blueprint says about traits, as authored, or `None` for a
/// malformed or unsupported asset.
pub fn declared_from_value(value: &Value) -> Option<Declared> {
    if !BLUEPRINT_FORMAT_VERSIONS.contains(&value.get("format_version")?.as_u64()?) {
        return None;
    }
    if !value.get("main_graph")?.is_object() || !value.get("variables")?.is_array() {
        return None;
    }
    let metadata = value.get("blueprint_metadata").and_then(Value::as_object);
    let list = |key: &str| -> Option<Vec<String>> {
        let Some(raw) = metadata.and_then(|metadata| metadata.get(key)) else {
            return Some(Vec::new());
        };
        Some(
            raw.as_array()?
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
        )
    };
    Some(Declared {
        ids: list("implemented_trait_ids")?,
        paths: list("implemented_traits")?,
    })
}

/// The source's scan: one Blueprint file's implementer. A class's
/// `graph_save.json` binds to its class folder; a loose `*.blueprint.json` is
/// indexed (pickers, conformance) but runs as no class.
pub fn scan(relative: &str, bytes: &[u8]) -> Option<Scanned> {
    let value = parse_blueprint_json(bytes)?;
    let declared = declared_from_value(&value)?;
    let binding = (std::path::Path::new(relative)
        .file_name()
        .and_then(|name| name.to_str())
        == Some("graph_save.json"))
    .then(|| relative.rsplit_once('/').map(|(dir, _)| dir.to_owned()))
    .flatten()
    .map(|dir| Binding::ScriptClass { dir });
    Some(Scanned {
        id: relative.to_owned(),
        declared,
        binding,
    })
}

/// The functions a Blueprint (as its JSON asset) provides: custom events and
/// macros.
pub fn provided_methods(blueprint: &Value) -> Vec<ProvidedMethod> {
    let mut methods = Vec::new();
    let mut seen = BTreeSet::new();
    let mut push = |method: ProvidedMethod| {
        if seen.insert(normalize_name(&method.name)) {
            methods.push(method);
        }
    };

    let events = blueprint
        .get("local_events")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .chain(
            blueprint
                .pointer("/main_graph/custom_event_defs")
                .and_then(Value::as_object)
                .into_iter()
                .flat_map(|defs| defs.values()),
        );
    for event in events {
        let Some(name) = event.get("name").and_then(Value::as_str) else {
            continue;
        };
        let params = event
            .get("fields")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|field| ProvidedParam {
                name: field
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                type_name: field
                    .get("type_name")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            })
            .collect();
        let return_type = event
            .get("return_type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let returns = if is_unit_type(return_type) {
            ProvidedReturn::Unit
        } else {
            ProvidedReturn::Value(Some(return_type.to_owned()))
        };
        push(ProvidedMethod {
            name: name.to_owned(),
            params,
            returns,
        });
    }

    for graph in blueprint
        .get("subgraphs")
        .or_else(|| blueprint.get("local_macros"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if graph.get("kind").and_then(Value::as_str) == Some("collapsed") {
            continue;
        }
        let Some(name) = graph.get("name").and_then(Value::as_str) else {
            continue;
        };
        let pins = |side: &str| -> Vec<&Value> {
            graph
                .pointer(&format!("/interface/{side}"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|pin| pin.get("data_type").and_then(Value::as_str) != Some("Execution"))
                .collect()
        };
        let params = pins("inputs")
            .into_iter()
            .map(|pin| ProvidedParam {
                name: pin
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                type_name: None,
            })
            .collect();
        let returns = match pins("outputs").len() {
            0 => ProvidedReturn::Unit,
            _ => ProvidedReturn::Value(None),
        };
        push(ProvidedMethod {
            name: name.to_owned(),
            params,
            returns,
        });
    }

    // Placed entry-point nodes for a trait method (`event::on::<Class>.<name>`,
    // what the Blueprint editor's Add menu creates for a declared trait): the
    // node is the implementation, in the main graph or a macro.
    let graphs = std::iter::once(blueprint.get("main_graph")).chain(
        blueprint
            .get("subgraphs")
            .or_else(|| blueprint.get("local_macros"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|graph| graph.get("graph")),
    );
    for graph in graphs.flatten() {
        let nodes = graph.get("nodes").and_then(Value::as_object);
        for node in nodes.into_iter().flat_map(|nodes| nodes.values()) {
            let Some(event) = node
                .get("node_type")
                .and_then(Value::as_str)
                .and_then(|node_type| node_type.strip_prefix("event::on::"))
            else {
                continue;
            };
            let Some((_, method)) = event.rsplit_once('.') else {
                continue;
            };
            // Outputs other than the exec `Body` pin are the parameters.
            let outputs: Vec<&Value> = match node.get("outputs") {
                Some(Value::Array(pins)) => pins.iter().collect(),
                Some(Value::Object(pins)) => pins.values().collect(),
                _ => Vec::new(),
            };
            let params = outputs
                .into_iter()
                .filter_map(|pin| pin.get("id").and_then(Value::as_str))
                .filter(|id| *id != "Body")
                .map(|id| ProvidedParam {
                    name: id.to_owned(),
                    type_name: None,
                })
                .collect();
            push(ProvidedMethod {
                name: method.to_owned(),
                params,
                returns: ProvidedReturn::Unknown,
            });
        }
    }
    methods
}

/// Check the Blueprint JSON against the traits it declares. A Blueprint of
/// an unsupported format, or one declaring nothing, has no issues.
pub fn check_blueprint(db: &TraitDb, blueprint: &Value) -> Vec<ConformanceIssue> {
    let Some(declared) = declared_from_value(blueprint) else {
        return Vec::new();
    };
    if declared.ids.is_empty() && declared.paths.is_empty() {
        return Vec::new();
    }
    let entry = pulsar_traits::resolve_declared(db, &declared);
    let severity = if entry.path_only {
        Severity::Warning
    } else {
        Severity::Error
    };
    let mut issues: Vec<ConformanceIssue> = entry
        .unresolved
        .iter()
        .map(|reference| ConformanceIssue {
            severity,
            kind: IssueKind::UnresolvedTrait,
            trait_name: reference.clone(),
            method: None,
            message: format!(
                "Declared trait '{reference}' does not exist in this project. Remove it or restore the trait asset."
            ),
        })
        .collect();
    issues.extend(check(
        db,
        &entry.implemented_trait_ids,
        &provided_methods(blueprint),
        severity,
    ));
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsar_traits::{TraitMethodRecord, TraitParamRecord, TraitRecord};
    use serde_json::json;

    fn method(name: &str, params: &[(&str, &str)], ret: &str, default: bool) -> TraitMethodRecord {
        TraitMethodRecord {
            name: name.into(),
            params: params
                .iter()
                .map(|(n, t)| TraitParamRecord {
                    name: (*n).into(),
                    type_name: (*t).into(),
                })
                .collect(),
            return_type: ret.into(),
            doc: None,
            has_default: default,
        }
    }

    fn trait_record(
        id: &str,
        name: &str,
        supers: &[&str],
        methods: Vec<TraitMethodRecord>,
    ) -> TraitRecord {
        TraitRecord {
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
        }
    }

    fn db() -> TraitDb {
        TraitDb::build(
            vec![
                trait_record(
                    "base",
                    "Base",
                    &[],
                    vec![method("tick", &[("dt", "f32")], "()", false)],
                ),
                trait_record(
                    "mode",
                    "Mode",
                    &["base"],
                    vec![
                        method("on_game_start", &[], "()", false),
                        method("score", &[("who", "u32")], "i32", false),
                        method("optional", &[], "()", true),
                    ],
                ),
            ],
            Vec::new(),
        )
    }

    fn blueprint(ids: &[&str], paths: &[&str], events: Value, subgraphs: Value) -> Value {
        json!({
            "format_version": 1, "main_graph": {}, "variables": [],
            "local_events": events, "subgraphs": subgraphs,
            "blueprint_metadata": {"implemented_trait_ids": ids, "implemented_traits": paths}
        })
    }

    fn event(name: &str, fields: &[(&str, &str)], ret: &str) -> Value {
        json!({
            "uid": name, "name": name, "return_type": ret,
            "fields": fields.iter().map(|(n, t)| json!({"name": n, "type_name": t})).collect::<Vec<_>>()
        })
    }

    fn full() -> Value {
        blueprint(
            &["mode"],
            &[],
            json!([
                event("On Game Start", &[], ""),
                event("score", &[("player", "number")], "number"),
            ]),
            json!([{
                "id": "m", "kind": "macro", "name": "Tick",
                "interface": {
                    "inputs": [{"id": "e", "name": "In", "data_type": "Execution"},
                               {"id": "d", "name": "dt", "data_type": {"Data": {}}}],
                    "outputs": [{"id": "o", "name": "Out", "data_type": "Execution"}]
                }
            }]),
        )
    }

    #[test]
    fn a_blueprint_providing_every_method_conforms() {
        assert_eq!(check_blueprint(&db(), &full()), Vec::new());
    }

    #[test]
    fn missing_methods_are_errors_including_inherited_ones_but_not_defaults() {
        let bp = blueprint(
            &["mode"],
            &[],
            json!([event("on_game_start", &[], "")]),
            json!([]),
        );
        let issues = check_blueprint(&db(), &bp);
        let missing: Vec<_> = issues
            .iter()
            .filter(|i| i.kind == IssueKind::MissingMethod)
            .map(|i| i.method.clone().unwrap())
            .collect();
        assert_eq!(missing, ["score", "tick"]);
        assert!(issues.iter().all(|i| i.severity == Severity::Error));
        assert!(issues[0].message.contains("score(who: u32) -> i32"));
        assert!(issues
            .iter()
            .any(|i| i.trait_name == "Base (required by Mode)"));
    }

    #[test]
    fn signature_mismatches_are_reported() {
        let bp = blueprint(
            &["mode"],
            &[],
            json!([
                event("on_game_start", &[("extra", "i32")], ""),
                event("score", &[("who", "string")], ""),
                event("tick", &[("dt", "f32")], "bool"),
            ]),
            json!([]),
        );
        let kinds: Vec<_> = check_blueprint(&db(), &bp)
            .iter()
            .map(|i| (i.method.clone().unwrap(), i.kind))
            .collect();
        assert!(kinds.contains(&("on_game_start".into(), IssueKind::ParamCount)));
        assert!(kinds.contains(&("score".into(), IssueKind::ParamType)));
        assert!(kinds.contains(&("score".into(), IssueKind::ReturnType)));
        assert!(kinds.contains(&("tick".into(), IssueKind::ReturnType)));
    }

    #[test]
    fn legacy_path_only_declarations_are_warnings() {
        let bp = blueprint(&[], &["types/traits/Mode.trait.json"], json!([]), json!([]));
        let issues = check_blueprint(&db(), &bp);
        assert!(!issues.is_empty());
        assert!(issues.iter().all(|i| i.severity == Severity::Warning));
    }

    #[test]
    fn unresolved_declarations_are_reported() {
        let bp = blueprint(&["ghost"], &[], json!([]), json!([]));
        let issues = check_blueprint(&db(), &bp);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, IssueKind::UnresolvedTrait);
        assert_eq!(issues[0].severity, Severity::Error);
    }

    #[test]
    fn blueprints_declaring_nothing_or_in_other_formats_are_not_checked() {
        assert!(check_blueprint(&db(), &blueprint(&[], &[], json!([]), json!([]))).is_empty());
        assert!(check_blueprint(&db(), &json!({"format_version": 9})).is_empty());
    }

    #[test]
    fn collapsed_subgraphs_do_not_count_as_functions() {
        let bp = blueprint(
            &["mode"],
            &[],
            json!([]),
            json!([{"id": "c", "kind": "collapsed", "name": "on_game_start", "interface": {"inputs": [], "outputs": []}}]),
        );
        assert!(check_blueprint(&db(), &bp)
            .iter()
            .any(|i| i.method.as_deref() == Some("on_game_start")));
    }

    #[test]
    fn a_placed_trait_event_node_provides_its_method() {
        let mut bp = blueprint(&["mode"], &[], json!([]), json!([]));
        bp["main_graph"] = json!({"nodes": {
            "n1": {"id": "n1", "node_type": "event::on::Player.on_game_start", "outputs": [{"id": "Body"}]},
            "n2": {"id": "n2", "node_type": "event::on::Player.score", "outputs": [{"id": "Body"}, {"id": "who"}]},
            "n3": {"id": "n3", "node_type": "event::on::Player.tick", "outputs": {"Body": {"id": "Body"}, "dt": {"id": "dt"}}},
        }});
        // The event system has no return value, so `score -> i32` is not
        // judged on it; parameters still count.
        assert_eq!(check_blueprint(&db(), &bp), Vec::new());

        bp["main_graph"]["nodes"]["n2"]["outputs"] = json!([{"id": "Body"}]);
        let issues = check_blueprint(&db(), &bp);
        assert!(issues
            .iter()
            .any(|i| i.kind == IssueKind::ParamCount && i.method.as_deref() == Some("score")));
    }
    #[test]
    fn blueprint_files_with_editor_header_comments_parse() {
        let saved = "// Auto Generated by the Pulsar Blueprint Editor\n// DO NOT EDIT\n//\n\n{\"format_version\": 2}";
        assert_eq!(
            parse_blueprint_json(saved.as_bytes()).unwrap()["format_version"],
            2
        );
        assert!(parse_blueprint_json(b"{\"a\":1}").is_some());
        assert!(parse_blueprint_json(b"// only a comment").is_none());
        assert!(parse_blueprint_json(b"not json").is_none());
    }

    #[test]
    fn a_class_blueprint_binds_to_its_class_folder_and_a_loose_one_to_none() {
        let body = json!({
            "format_version": 2, "main_graph": {}, "variables": [],
            "blueprint_metadata": { "implemented_trait_ids": ["pulsar.GameMode"] }
        })
        .to_string();
        let scanned = scan("classes/Mode/graph_save.json", body.as_bytes()).unwrap();
        assert_eq!(scanned.declared.ids, ["pulsar.GameMode"]);
        assert_eq!(
            scanned.binding,
            Some(Binding::ScriptClass {
                dir: "classes/Mode".into()
            })
        );
        assert_eq!(
            scan("x/loose.blueprint.json", body.as_bytes())
                .unwrap()
                .binding,
            None
        );
        assert!(scan("classes/Bad/graph_save.json", b"not json").is_none());
        assert!(is_blueprint_asset("a/graph_save.json") && is_blueprint_asset("b.blueprint.json"));
        assert!(!is_blueprint_asset("a/graph.json"));
    }
}
