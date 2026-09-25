use blueprint_editor_plugin::io::formats::{
    BlueprintAsset, BlueprintEditorState, strip_header_comments,
};
use blueprint_editor_plugin::io::legacy::{
    LegacyBlueprintComment, LegacyColor, LegacyConnection, LegacyPosition, LegacySize,
};
use blueprint_editor_plugin::parse_hex_color;
use ui::graph::{BlueprintComment, Connection, ConnectionType};

#[test]
fn blueprint_asset_creation_uses_current_defaults() {
    let asset = BlueprintAsset::new();

    assert_eq!(asset.format_version, 1);
    assert_eq!(asset.main_graph.metadata.name, "EventGraph");
    assert!(asset.main_graph.custom_event_defs.is_empty());
    assert!(asset.local_macros.is_empty());
    assert!(asset.variables.is_empty());
}

#[test]
fn header_comments_are_removed_before_deserialization() {
    let content = "// Comment 1\n// Comment 2\n{\"data\": \"value\"}";
    assert_eq!(strip_header_comments(content), "{\"data\": \"value\"}");
}

#[test]
fn editor_state_tracks_unique_tabs() {
    let mut state = BlueprintEditorState::new();
    state.add_tab("macro1".to_string());
    state.add_tab("macro1".to_string());

    assert_eq!(state.open_tab_ids, ["main", "macro1"]);
}

#[test]
fn legacy_connection_preserves_endpoints() {
    let connection: Connection = LegacyConnection {
        id: "conn1".to_string(),
        source_node: "node1".to_string(),
        source_pin: "out".to_string(),
        target_node: "node2".to_string(),
        target_pin: "in".to_string(),
        connection_type: ConnectionType::Data,
    }
    .into();

    assert_eq!(connection.id, "conn1");
    assert_eq!(connection.source_node, "node1");
    assert_eq!(connection.target_node, "node2");
}

#[test]
fn legacy_grayscale_hsl_converts_to_equal_rgb_channels() {
    let comment = converted_legacy_comment(LegacyColor {
        h: 0.0,
        s: 0.0,
        l: 0.5,
        a: 1.0,
    });

    assert!((comment.color[0] - 0.5).abs() < 0.001);
    assert!((comment.color[1] - 0.5).abs() < 0.001);
    assert!((comment.color[2] - 0.5).abs() < 0.001);
}

#[test]
fn legacy_red_hsl_converts_to_red_rgb() {
    let comment = converted_legacy_comment(LegacyColor {
        h: 0.0,
        s: 1.0,
        l: 0.5,
        a: 0.3,
    });

    assert!((comment.color[0] - 1.0).abs() < 0.001);
    assert!(comment.color[1].abs() < 0.001);
    assert!(comment.color[2].abs() < 0.001);
    assert_eq!(comment.color[3], 0.3);
}

#[test]
fn hex_colors_support_rgb_and_rgba_forms() {
    assert!(parse_hex_color("#4A90E2").is_some());
    assert!(parse_hex_color("#FF000080").is_some());
    assert!(parse_hex_color("#not-a-color").is_none());
}

fn converted_legacy_comment(color: LegacyColor) -> BlueprintComment {
    LegacyBlueprintComment {
        id: "comment1".to_string(),
        text: "Test".to_string(),
        position: LegacyPosition { x: 0.0, y: 0.0 },
        size: LegacySize {
            width: 100.0,
            height: 100.0,
        },
        color,
        contained_node_ids: vec![],
    }
    .into()
}

/// #921: prefab components get stable slot ids. Old files get the same
/// deterministic `<Class>_<n>` ids the engine assigns when it reads them;
/// saving writes them and leaves the component data untouched.
#[test]
fn prefab_components_get_stable_slot_ids() {
    let dir = std::env::temp_dir().join(format!("bp_prefab_slots_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("prefab.json");
    std::fs::write(
        &path,
        r#"{
            "prefab_version": 1, "name": "Lamp",
            "components": [
                { "class_name": "LightComponent", "enabled": true, "data": { "__parent_index": 1 } },
                { "class_name": "StaticMeshComponent", "enabled": true, "data": {} },
                { "slot_id": "LightComponent_0", "class_name": "LightComponent", "enabled": false, "data": {} }
            ]
        }"#,
    )
    .unwrap();

    let mut prefab = blueprint_editor_plugin::io::prefab::load_prefab(&path).unwrap();
    assert!(prefab.fill_missing_slot_ids());
    let ids: Vec<&str> = prefab.components.iter().map(|c| c.slot_id.as_str()).collect();
    // The stored id is kept; the others get the next free deterministic ids.
    assert_eq!(ids, ["LightComponent_1", "StaticMeshComponent_0", "LightComponent_0"]);
    assert_eq!(prefab.new_slot_id("LightComponent"), "LightComponent_2");

    blueprint_editor_plugin::io::prefab::save_prefab(&path, &prefab).unwrap();
    let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved["components"][0]["slot_id"], "LightComponent_1");
    assert_eq!(saved["components"][0]["class_name"], "LightComponent");
    assert_eq!(saved["components"][0]["data"]["__parent_index"], 1);
    assert_eq!(saved["components"][2]["enabled"], false);

    let _ = std::fs::remove_dir_all(&dir);
}
