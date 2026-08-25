//! Node definition system for loading and managing node metadata.
//!
//! This module provides the infrastructure for loading node definitions from
//! the Pulsar Standard Library (pbgc), managing macro libraries, and converting
//! metadata into UI-ready node definitions. It uses a global singleton pattern
//! for efficient access to node metadata throughout the application.

use super::types::{PinDataType, PinType};
use crate::features::prefabs::PrefabAsset;
use pulsar_reflection::{RuntimeTypeInfo, REGISTRY};
use serde::Deserialize;
use std::collections::HashMap;

// ============================================================================
// Node Definition Types
// ============================================================================

/// Root structure containing all node definitions organized by category.
#[derive(Debug, Deserialize)]
pub struct NodeDefinitions {
    pub categories: Vec<NodeCategory>,
}

/// A category of related nodes with a name and color theme.
#[derive(Debug, Clone, Deserialize)]
pub struct NodeCategory {
    pub name: String,
    pub color: String,
    pub nodes: Vec<NodeDefinition>,
}

/// Complete definition of a node including pins, properties, and metadata.
#[derive(Debug, Clone, Deserialize)]
pub struct NodeDefinition {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub description: String,   // Short one-line description for list display
    pub documentation: String, // Full markdown documentation for docs panel
    pub inputs: Vec<PinDefinition>,
    pub outputs: Vec<PinDefinition>,
    pub properties: HashMap<String, String>,
    pub color: Option<String>,
    /// True when this node's underlying Blueprint type is `NodeTypes::event`
    /// (an entry-point such as `begin_play`, `on_tick`, `on_input_key`, …).
    /// Used to classify the node as `NodeType::Event` regardless of which
    /// palette category it lives in (e.g. "Input" events).
    #[serde(default)]
    pub is_event: bool,
}

/// Definition of a single pin on a node.
#[derive(Debug, Clone, Deserialize)]
pub struct PinDefinition {
    pub id: String,
    pub name: String,
    pub data_type: PinDataType,
    pub pin_type: PinType,
}

// ============================================================================
// Global Node Definitions
// ============================================================================

/// Global node definitions (loaded once at startup)
use std::sync::OnceLock;
static NODE_DEFINITIONS: OnceLock<NodeDefinitions> = OnceLock::new();

pub(crate) fn extract_canonical_node_metadata(
) -> std::collections::HashMap<String, graphy::NodeMetadata> {
    let mut metadata = pbgc::extract_node_metadata().unwrap_or_else(|e| {
        eprintln!("Failed to load node metadata: {}", e);
        std::collections::HashMap::new()
    });

    for node_meta in metadata.values_mut() {
        for param in node_meta.params.iter_mut() {
            param.param_type = ui::graph::DataType::from_type_str(&param.param_type).to_string();
        }

        if let Some(return_type) = node_meta.return_type.as_mut() {
            return_type.type_string =
                ui::graph::DataType::from_type_str(&return_type.type_string).to_string();
        }

        for out in node_meta.output_params.iter_mut() {
            out.param_type = ui::graph::DataType::from_type_str(&out.param_type).to_string();
        }
    }

    metadata
}

impl NodeDefinitions {
    /// Get the global node definitions, loading them if necessary.
    pub fn load() -> &'static NodeDefinitions {
        NODE_DEFINITIONS.get_or_init(|| {
            // Load dynamic node definitions from PBGC metadata, then canonicalize type names.
            let metadata = extract_canonical_node_metadata();

            // Load sub-graph libraries
            let mut lib_manager = ui::graph::LibraryManager::default();
            if let Err(e) = lib_manager.load_all_libraries() {
                eprintln!("Failed to load sub-graph libraries: {}", e);
            }

            // Convert metadata to UI format (includes both regular nodes and sub-graphs)
            Self::from_node_metadata_and_libraries(metadata, lib_manager)
        })
    }

    fn from_node_metadata_and_libraries(
        metadata: std::collections::HashMap<String, graphy::NodeMetadata>,
        lib_manager: ui::graph::LibraryManager,
    ) -> NodeDefinitions {
        let mut categories_map: std::collections::HashMap<String, Vec<NodeDefinition>> =
            std::collections::HashMap::new();

        // First, add all sub-graphs from libraries
        for library in lib_manager.get_libraries().values() {
            let category_name = format!("Macros/{}", library.name);

            for subgraph in &library.subgraphs {
                // Convert sub-graph inputs to pin definitions
                let inputs: Vec<PinDefinition> = subgraph
                    .interface
                    .inputs
                    .iter()
                    .map(|pin| {
                        let canonical =
                            ui::graph::DataType::from_type_str(&pin.data_type.to_string());
                        PinDefinition {
                            id: pin.id.clone(),
                            name: pin.name.clone(),
                            data_type: PinDataType::from_type_str(canonical.to_string()),
                            pin_type: PinType::Input,
                        }
                    })
                    .collect();

                // Convert sub-graph outputs to pin definitions
                let outputs: Vec<PinDefinition> = subgraph
                    .interface
                    .outputs
                    .iter()
                    .map(|pin| {
                        let canonical =
                            ui::graph::DataType::from_type_str(&pin.data_type.to_string());
                        PinDefinition {
                            id: pin.id.clone(),
                            name: pin.name.clone(),
                            data_type: PinDataType::from_type_str(canonical.to_string()),
                            pin_type: PinType::Output,
                        }
                    })
                    .collect();

                let node_def = NodeDefinition {
                    id: format!("subgraph:{}", subgraph.id),
                    name: subgraph.name.clone(),
                    icon: "📦".to_string(), // Macro icon
                    description: subgraph.description.clone(),
                    documentation: subgraph.description.clone(), // Use same text for docs
                    inputs,
                    outputs,
                    properties: std::collections::HashMap::new(),
                    color: Some("#9B59B6".to_string()), // Purple for macros
                    is_event: false,
                };

                categories_map
                    .entry(category_name.clone())
                    .or_insert_with(Vec::new)
                    .push(node_def);
            }
        }

        // Then add regular nodes from metadata
        Self::populate_categories_from_metadata(metadata, &mut categories_map);

        // Convert to NodeDefinitions
        Self::categories_to_definitions(categories_map)
    }

    fn from_node_metadata(
        metadata: std::collections::HashMap<String, graphy::NodeMetadata>,
    ) -> NodeDefinitions {
        let mut categories_map: std::collections::HashMap<String, Vec<NodeDefinition>> =
            std::collections::HashMap::new();
        Self::populate_categories_from_metadata(metadata, &mut categories_map);
        Self::categories_to_definitions(categories_map)
    }

    fn populate_categories_from_metadata(
        metadata: std::collections::HashMap<String, graphy::NodeMetadata>,
        categories_map: &mut std::collections::HashMap<String, Vec<NodeDefinition>>,
    ) {
        // Add special reroute node to Utility category
        categories_map
            .entry("Utility".to_string())
            .or_insert_with(Vec::new)
            .push(NodeDefinition {
                id: "reroute".to_string(),
                name: "Reroute".to_string(),
                icon: "•".to_string(),
                description:
                    "Organize connections with a pass-through node (typeless until connected)"
                        .to_string(),
                documentation:
                    "Organize connections with a pass-through node (typeless until connected)"
                        .to_string(),
                inputs: vec![],
                outputs: vec![],
                properties: std::collections::HashMap::new(),
                color: None,
                is_event: false,
            });

        // Group nodes by category
        for (id, node_meta) in metadata {
            let mut inputs = Vec::new();
            let mut outputs = Vec::new();

            // Add execution input for nodes that need sequencing (fn_ and control_flow)
            if matches!(
                node_meta.node_type,
                graphy::NodeTypes::fn_ | graphy::NodeTypes::control_flow
            ) {
                inputs.push(PinDefinition {
                    id: "exec".to_string(),
                    name: "exec".to_string(),
                    data_type: PinDataType::from_type_str("execution"),
                    pin_type: PinType::Input,
                });
            }

            // Add regular inputs
            for param in node_meta.params.iter() {
                let canonical = ui::graph::DataType::from_type_str(&param.param_type).to_string();
                inputs.push(PinDefinition {
                    id: param.name.to_string(),
                    name: param.name.to_string(),
                    data_type: PinDataType::from_type_str(canonical),
                    pin_type: PinType::Input,
                });
            }

            // Add execution outputs
            for exec_pin in node_meta.exec_outputs.iter() {
                outputs.push(PinDefinition {
                    id: exec_pin.to_string(),
                    name: exec_pin.to_string(),
                    data_type: PinDataType::from_type_str("execution"),
                    pin_type: PinType::Output,
                });
            }

            // Add multi-output params (Break nodes, etc.)
            for out in &node_meta.output_params {
                let canonical = ui::graph::DataType::from_type_str(&out.param_type).to_string();
                outputs.push(PinDefinition {
                    id: out.name.clone(),
                    name: out.name.clone(),
                    data_type: PinDataType::from_type_str(canonical),
                    pin_type: PinType::Output,
                });
            }

            // Add single regular output (return type, skip void) — only when
            // there are no named output_params (backward compat).
            if node_meta.output_params.is_empty() {
                if let Some(return_type) = &node_meta.return_type {
                    let canonical =
                        ui::graph::DataType::from_type_str(&return_type.type_string).to_string();
                    if canonical != "()" && canonical != "execution" {
                        outputs.push(PinDefinition {
                            id: "result".to_string(),
                            name: "result".to_string(),
                            data_type: PinDataType::from_type_str(canonical),
                            pin_type: PinType::Output,
                        });
                    }
                }
            }

            let category = node_meta.category.clone();
            let description = format!("{} ({})", node_meta.name, node_meta.category);
            let is_event = matches!(node_meta.node_type, graphy::NodeTypes::event);

            let static_def = NodeDefinition {
                id: id.clone(),
                name: node_meta.name.clone(),
                icon: "⚙️".to_string(), // Default icon
                description: description.clone(),
                documentation: description,
                inputs,
                outputs,
                properties: std::collections::HashMap::new(),
                color: None,
                is_event,
            };

            categories_map
                .entry(category)
                .or_insert_with(Vec::new)
                .push(static_def);
        }
    }

    fn categories_to_definitions(
        categories_map: std::collections::HashMap<String, Vec<NodeDefinition>>,
    ) -> NodeDefinitions {
        // Convert to categories
        let categories = categories_map
            .into_iter()
            .map(|(name, nodes)| NodeCategory {
                name: name.clone(),
                color: Self::get_category_color(&name),
                nodes,
            })
            .collect();

        NodeDefinitions { categories }
    }

    fn get_category_color(category: &str) -> String {
        match category {
            "Math" | "Math/Vector" => "#4A90E2".to_string(),
            "Logic" => "#E2A04A".to_string(),
            "String" => "#7ED321".to_string(),
            "Array" => "#BD10E0".to_string(),
            "File I/O" => "#50E3C2".to_string(),
            "Graphics" => "#F5A623".to_string(),
            "Time" => "#9013FE".to_string(),
            "Utility" => "#B8E986".to_string(),
            _ => "#9B9B9B".to_string(),
        }
    }

    pub fn get_node_definition(&self, node_id: &str) -> Option<&NodeDefinition> {
        self.categories
            .iter()
            .flat_map(|category| &category.nodes)
            .find(|node| node.id == node_id)
    }

    pub fn get_node_definition_by_name(&self, node_name: &str) -> Option<&NodeDefinition> {
        self.categories
            .iter()
            .flat_map(|category| &category.nodes)
            .find(|node| node.name == node_name)
    }

    pub fn get_category_for_node(&self, node_id: &str) -> Option<&NodeCategory> {
        self.categories
            .iter()
            .find(|category| category.nodes.iter().any(|node| node.id == node_id))
    }

    /// Generate component nodes for all components in a prefab.
    ///
    /// For each component class the prefab references, this creates:
    /// - A **Get Property** method node per property (`comp_get_prop::ClassName::PropName`)
    /// - A **Set Property** method node per property (`comp_set_prop::ClassName::PropName`)
    /// - A **Call Method** node per blueprint-callable method (`comp_call::ClassName::MethodName`)
    ///
    /// These node IDs follow the convention expected by the PBGC Rust codegen so that
    /// compiled blueprints produce correct `ComponentStore` access code.
    pub fn generate_component_nodes(prefab: &PrefabAsset) -> Vec<NodeCategory> {
        let mut categories = Vec::new();

        // Collect unique class names (each class only needs one set of nodes regardless
        // of how many instances the prefab contains).
        let mut seen_classes: std::collections::HashSet<String> = std::collections::HashSet::new();

        for component in &prefab.components {
            if !component.enabled {
                continue;
            }
            let class_name = &component.class_name;
            if seen_classes.contains(class_name) {
                continue;
            }
            seen_classes.insert(class_name.clone());

            let Some(instance) = REGISTRY.create_instance(class_name) else {
                continue;
            };

            let mut nodes: Vec<NodeDefinition> = Vec::new();

            // ── Property nodes ────────────────────────────────────────────────
            for property in instance.get_properties() {
                nodes.push(Self::make_comp_get_prop_node(
                    class_name,
                    property.name,
                    &property.display_name,
                    property.type_info,
                ));
                nodes.push(Self::make_comp_set_prop_node(
                    class_name,
                    property.name,
                    &property.display_name,
                    property.type_info,
                ));
            }

            // ── Method nodes ──────────────────────────────────────────────────
            if let Some(methods) = REGISTRY.get_methods(class_name) {
                for method in methods {
                    nodes.push(Self::make_comp_call_node(class_name, &method));
                }
            }

            if !nodes.is_empty() {
                categories.push(NodeCategory {
                    name: format!("Components/{}", class_name),
                    color: "#9B59B6".to_string(),
                    nodes,
                });
            }
        }

        categories
    }

    /// Create a `comp_get_prop::ClassName::PropName` method node definition.
    fn make_comp_get_prop_node(
        class_name: &str,
        prop_name: &str,
        display_name: &str,
        type_info: &'static RuntimeTypeInfo,
    ) -> NodeDefinition {
        NodeDefinition {
            id: format!("comp_get_prop::{}::{}", class_name, prop_name),
            name: format!("Get {}", display_name),
            icon: "⬇".to_string(),
            description: format!(
                "Get {}.{} from a component reference",
                class_name, prop_name
            ),
            documentation: format!(
                "Reads the `{}` property from a `{}` component reference.\n\n\
                 **Returns** the current value of the property.",
                prop_name, class_name
            ),
            inputs: vec![PinDefinition {
                id: "component_ref".to_string(),
                name: "component".to_string(),
                data_type: PinDataType::from_type_str(class_name),
                pin_type: PinType::Input,
            }],
            outputs: vec![PinDefinition {
                id: "value".to_string(),
                name: "value".to_string(),
                data_type: pin_data_type_for(type_info),
                pin_type: PinType::Output,
            }],
            properties: HashMap::new(),
            color: Some("#2ECC71".to_string()),
            is_event: false,
        }
    }

    /// Create a `comp_set_prop::ClassName::PropName` method node definition (exec).
    fn make_comp_set_prop_node(
        class_name: &str,
        prop_name: &str,
        display_name: &str,
        type_info: &'static RuntimeTypeInfo,
    ) -> NodeDefinition {
        NodeDefinition {
            id: format!("comp_set_prop::{}::{}", class_name, prop_name),
            name: format!("Set {}", display_name),
            icon: "⬆".to_string(),
            description: format!("Set {}.{} on a component reference", class_name, prop_name),
            documentation: format!(
                "Writes a new value to the `{}` property on a `{}` component reference.",
                prop_name, class_name
            ),
            inputs: vec![
                PinDefinition {
                    id: "exec".to_string(),
                    name: "exec".to_string(),
                    data_type: PinDataType::from_type_str("execution"),
                    pin_type: PinType::Input,
                },
                PinDefinition {
                    id: "component_ref".to_string(),
                    name: "component".to_string(),
                    data_type: PinDataType::from_type_str(class_name),
                    pin_type: PinType::Input,
                },
                PinDefinition {
                    id: "value".to_string(),
                    name: "value".to_string(),
                    data_type: pin_data_type_for(type_info),
                    pin_type: PinType::Input,
                },
            ],
            outputs: vec![PinDefinition {
                id: "exec_out".to_string(),
                name: "exec".to_string(),
                data_type: PinDataType::from_type_str("execution"),
                pin_type: PinType::Output,
            }],
            properties: HashMap::new(),
            color: Some("#E67E22".to_string()),
            is_event: false,
        }
    }

    /// Create a `comp_call::ClassName::MethodName` node definition (exec method call).
    fn make_comp_call_node(
        class_name: &str,
        method: &pulsar_reflection::MethodMetadata,
    ) -> NodeDefinition {
        let mut inputs = vec![
            PinDefinition {
                id: "exec".to_string(),
                name: "exec".to_string(),
                data_type: PinDataType::from_type_str("execution"),
                pin_type: PinType::Input,
            },
            PinDefinition {
                id: "component_ref".to_string(),
                name: "component".to_string(),
                data_type: PinDataType::from_type_str(class_name),
                pin_type: PinType::Input,
            },
        ];

        for param in &method.params {
            inputs.push(PinDefinition {
                id: param.name.to_string(),
                name: param.name.to_string(),
                data_type: pin_data_type_for(param.type_info),
                pin_type: PinType::Input,
            });
        }

        let mut outputs = vec![PinDefinition {
            id: "exec_out".to_string(),
            name: "exec".to_string(),
            data_type: PinDataType::from_type_str("execution"),
            pin_type: PinType::Output,
        }];

        if let Some(ret) = &method.return_type {
            outputs.push(PinDefinition {
                id: "return_value".to_string(),
                name: "return value".to_string(),
                data_type: pin_data_type_for(ret.type_info),
                pin_type: PinType::Output,
            });
        }

        NodeDefinition {
            id: format!("comp_call::{}::{}", class_name, method.name),
            name: method.display_name.to_string(),
            icon: "▶".to_string(),
            description: format!(
                "Call {}.{}() on a component reference",
                class_name, method.name
            ),
            documentation: format!(
                "Calls the `{}` method on a `{}` component reference.",
                method.name, class_name
            ),
            inputs,
            outputs,
            properties: HashMap::new(),
            color: Some("#3498DB".to_string()),
            is_event: false,
        }
    }
}

/// Build a pin's canonical data type directly from its registered reflection
/// type info — `type_name` is already the registry-stable identity, so no
/// string-parsing/conversion layer is needed (the registry resolves it back
/// to this exact `RuntimeTypeInfo` on demand).
fn pin_data_type_for(type_info: &RuntimeTypeInfo) -> PinDataType {
    PinDataType::from_type_str(type_info.type_name)
}
