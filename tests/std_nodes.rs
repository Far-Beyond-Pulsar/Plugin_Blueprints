use blueprint_editor_plugin::std_nodes::{extract_node_metadata, node_metadata, StdNodeProvider};
use graphy::NodeMetadataProvider;

#[test]
fn the_standard_library_nodes_are_in_the_catalogue() {
    let metadata = extract_node_metadata();
    assert!(
        metadata.len() > 100,
        "the standard library registers its nodes ({} found)",
        metadata.len()
    );
    for name in ["add", "branch", "do_once", "delay"] {
        assert!(metadata.contains_key(name), "`{name}` is in the catalogue");
    }
}

#[test]
fn flow_intrinsics_keep_their_exec_pins() {
    let metadata = node_metadata();
    assert_eq!(metadata["do_once"].exec_outputs, ["Then"]);
    assert_eq!(metadata["delay"].exec_outputs, ["Completed"]);
}

#[test]
fn the_provider_serves_the_same_catalogue() {
    let provider = StdNodeProvider::new();
    assert!(provider.get_node_metadata("add").is_some());
    assert!(provider.get_node_metadata("no_such_node").is_none());
    assert_eq!(provider.get_all_nodes().len(), node_metadata().len());
}
