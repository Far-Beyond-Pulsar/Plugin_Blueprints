//! The `pulsar_std` node catalogue as Graphy node metadata.
//!
//! `#[blueprint]` functions in `pulsar_std` register themselves in a registry
//! at compile time. The palette and the data-flow checks read those nodes in
//! Graphy's metadata shape, converted here.

use graphy::core::{NodeMetadata, NodeMetadataProvider, OutputParam, ParamInfo, TypeInfo};
use std::collections::HashMap;
use std::sync::OnceLock;

static NODE_METADATA: OnceLock<HashMap<String, NodeMetadata>> = OnceLock::new();

fn convert_node_metadata(node: &pulsar_std::registry::NodeMetadata) -> NodeMetadata {
    let node_type = match node.node_type {
        pulsar_std::NodeTypes::pure => graphy::NodeTypes::pure,
        pulsar_std::NodeTypes::fn_ => graphy::NodeTypes::fn_,
        pulsar_std::NodeTypes::control_flow => graphy::NodeTypes::control_flow,
        pulsar_std::NodeTypes::event => graphy::NodeTypes::event,
    };

    let params: Vec<ParamInfo> = node
        .params
        .iter()
        .map(|p| ParamInfo::new(p.name, p.ty))
        .collect();

    let imports: Vec<String> = node
        .imports
        .iter()
        .map(|import| {
            if import.items.is_empty() {
                format!("use {};", import.crate_name)
            } else {
                format!(
                    "use {}::{{{}}};",
                    import.crate_name,
                    import.items.join(", ")
                )
            }
        })
        .collect();

    // A multi-value return is a tuple; each output reads its field (.0, .1, ...).
    let outputs: Vec<OutputParam> = node
        .output_params
        .iter()
        .enumerate()
        .map(|(i, p)| OutputParam::new(p.name, p.ty, format!(".{i}")))
        .collect();

    let return_type = node.return_type.unwrap_or("()");

    let mut meta = NodeMetadata::new(node.name, node_type, node.category)
        .with_params(params)
        .with_return_type(TypeInfo::new(return_type.to_string()))
        .with_outputs(outputs)
        .with_exec_outputs(node.exec_outputs.iter().map(|s| s.to_string()).collect())
        .with_imports(imports)
        .with_source(node.function_source);

    if let Some(conv) = &node.conversion {
        meta = meta.with_conversion(
            TypeInfo::new(conv.from_type.to_string()),
            TypeInfo::new(conv.to_type.to_string()),
            conv.lossless,
        );
    }

    meta
}

/// Every `pulsar_std` node, keyed by node name, freshly converted.
pub fn extract_node_metadata() -> HashMap<String, NodeMetadata> {
    pulsar_std::get_all_nodes()
        .iter()
        .map(|node| {
            let meta = convert_node_metadata(node);
            (meta.name.clone(), meta)
        })
        .collect()
}

/// The converted catalogue, built on first use.
pub fn node_metadata() -> &'static HashMap<String, NodeMetadata> {
    NODE_METADATA.get_or_init(extract_node_metadata)
}

/// Serves the `pulsar_std` catalogue to Graphy's data-flow analysis.
pub struct StdNodeProvider {
    metadata: &'static HashMap<String, NodeMetadata>,
}

impl StdNodeProvider {
    pub fn new() -> Self {
        Self {
            metadata: node_metadata(),
        }
    }
}

impl Default for StdNodeProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeMetadataProvider for StdNodeProvider {
    fn get_node_metadata(&self, node_type: &str) -> Option<&NodeMetadata> {
        self.metadata.get(node_type)
    }

    fn get_all_nodes(&self) -> Vec<&NodeMetadata> {
        self.metadata.values().collect()
    }

    fn get_nodes_by_category(&self, category: &str) -> Vec<&NodeMetadata> {
        self.metadata
            .values()
            .filter(|m| m.category == category)
            .collect()
    }
}
