use blueprint_graph::codec::deserialize_blueprint;
use serde_json::{json, Value};

const FIXTURE: &str = include_str!("fixtures/graph_save.json");

/// `1` and `1.0` are distinct JSON values but the same authored number; the
/// float fields are written back as floats, so compare numerically.
fn numbers_as_f64(value: &Value) -> Value {
    match value {
        Value::Number(n) => json!(n.as_f64()),
        Value::Array(items) => Value::Array(items.iter().map(numbers_as_f64).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), numbers_as_f64(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[test]
fn authored_v1_round_trip_preserves_identity_defaults_macros_events_and_layout() {
    let original: Value = serde_json::from_str(FIXTURE).unwrap();
    let asset = deserialize_blueprint(FIXTURE).unwrap();
    let saved = serde_json::to_value(&asset).unwrap();
    // Optional empty fields may be omitted, so compare the normalized schema
    // and independently assert the authored data most at risk during extraction.
    let back = deserialize_blueprint(&saved.to_string()).unwrap();
    assert_eq!(serde_json::to_value(back).unwrap(), saved);
    for key in ["main_graph", "variables", "local_events", "editor_state"] {
        assert_eq!(
            numbers_as_f64(&saved[key]),
            numbers_as_f64(&original[key]),
            "{key}"
        );
    }
    assert_eq!(asset.local_macros[0].id, "once-macro");
    assert_eq!(asset.local_macros[0].graph.connections.len(), 2);
    assert_eq!(
        asset.local_macros[0].graph.nodes["once"].properties["reset"],
        false
    );
}

#[test]
fn rename_and_reorder_do_not_reassign_variable_ids() {
    let mut asset = deserialize_blueprint(FIXTURE).unwrap();
    let mut added = asset.variables[0].clone();
    added.id = "new-variable".into();
    added.name = "other".into();
    asset.variables.push(added);
    asset.variables[0].name = "renamed".into();
    asset.variables.reverse();
    let back = deserialize_blueprint(&serde_json::to_string(&asset).unwrap()).unwrap();
    assert_eq!(back.variables[1].id, "var_0");
    assert_eq!(back.variables[1].name, "renamed");
    assert_eq!(
        back.variables[1].description,
        "Keep this legacy ID when renamed"
    );
}

#[test]
fn legacy_pin_maps_and_comment_objects_remain_readable() {
    let mut value: Value = serde_json::from_str(FIXTURE).unwrap();
    let output = &mut value["main_graph"]["nodes"]["begin"]["outputs"];
    let mut pin = output[0].clone();
    pin.as_object_mut().unwrap().remove("id");
    *output = json!({"Body": pin});
    value["main_graph"]["comments"][0]["position"] = json!({"x": 1, "y": 2});
    value["main_graph"]["comments"][0]["size"] = json!({"width": 400, "height": 180});
    let asset = deserialize_blueprint(&format!("// Legacy header\n{}", value)).unwrap();
    assert_eq!(asset.main_graph.nodes["begin"].outputs[0].id, "Body");
    assert_eq!(asset.main_graph.comments[0].position, (1., 2.));
    assert_eq!(asset.main_graph.comments[0].size, (400., 180.));
}

#[test]
fn malformed_pins_versions_and_variable_ids_are_reported() {
    let original: Value = serde_json::from_str(FIXTURE).unwrap();
    let mut value = original.clone();
    value["main_graph"]["nodes"]["begin"]["outputs"][0]["data_type"] = json!("not a type");
    assert!(deserialize_blueprint(&value.to_string()).is_err());
    value = original.clone();
    value["format_version"] = json!(999);
    assert!(deserialize_blueprint(&value.to_string())
        .unwrap_err()
        .contains("version 999"));
    value = original;
    let duplicate = value["variables"][0].clone();
    value["variables"].as_array_mut().unwrap().push(duplicate);
    assert!(deserialize_blueprint(&value.to_string())
        .unwrap_err()
        .contains("duplicate id"));
}
