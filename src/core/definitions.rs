//! Node definition system for loading and managing node metadata.
//!
//! This module provides the infrastructure for loading node definitions from
//! the Pulsar Standard Library (pbgc), managing macro libraries, and converting
//! metadata into UI-ready node definitions. It uses a global singleton pattern
//! for efficient access to node metadata throughout the application.

use super::types::PinType;
use crate::features::prefabs::PrefabAsset;
use pulsar_reflection::{RuntimeTypeInfo, TypeStructure, WrapperType, REGISTRY};
use serde::Deserialize;
use std::collections::HashMap;
use ui::graph::DataType;

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
}

/// Definition of a single pin on a node.
#[derive(Debug, Clone, Deserialize)]
pub struct PinDefinition {
    pub id: String,
    pub name: String,
    pub data_type: DataType,
    pub pin_type: PinType,
}

// ============================================================================
// Global Node Definitions
// ============================================================================

/// Global node definitions (loaded once at startup)
use std::sync::OnceLock;
static NODE_DEFINITIONS: OnceLock<NodeDefinitions> = OnceLock::new();

impl NodeDefinitions {
    /// Get the global node definitions, loading them if necessary.
    pub fn load() -> &'static NodeDefinitions {
        NODE_DEFINITIONS.get_or_init(|| {
            // Load dynamic node definitions from pulsar_std
            let metadata = pbgc::extract_node_metadata().unwrap_or_else(|e| {
                eprintln!("Failed to load node metadata: {}", e);
                std::collections::HashMap::new()
            });

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
                    .map(|pin| PinDefinition {
                        id: pin.id.clone(),
                        name: pin.name.clone(),
                        data_type: pin.data_type.clone(),
                        pin_type: PinType::Input,
                    })
                    .collect();

                // Convert sub-graph outputs to pin definitions
                let outputs: Vec<PinDefinition> = subgraph
                    .interface
                    .outputs
                    .iter()
                    .map(|pin| PinDefinition {
                        id: pin.id.clone(),
                        name: pin.name.clone(),
                        data_type: pin.data_type.clone(),
                        pin_type: PinType::Output,
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
                    data_type: DataType::from_type_str("execution"),
                    pin_type: PinType::Input,
                });
            }

            // Add regular inputs
            for param in node_meta.params.iter() {
                inputs.push(PinDefinition {
                    id: param.name.to_string(),
                    name: param.name.to_string(),
                    data_type: DataType::from_type_str(&param.param_type),
                    pin_type: PinType::Input,
                });
            }

            // Add execution outputs
            for exec_pin in node_meta.exec_outputs.iter() {
                outputs.push(PinDefinition {
                    id: exec_pin.to_string(),
                    name: exec_pin.to_string(),
                    data_type: DataType::from_type_str("execution"),
                    pin_type: PinType::Output,
                });
            }

            // Add regular outputs (return type, skip void)
            if let Some(return_type) = &node_meta.return_type {
                if return_type.type_string != "()" {
                    outputs.push(PinDefinition {
                        id: "result".to_string(),
                        name: "result".to_string(),
                        data_type: DataType::from_type_str(&return_type.type_string),
                        pin_type: PinType::Output,
                    });
                }
            }

            let category = node_meta.category.clone();
            let description = format!("{} ({})", node_meta.name, node_meta.category);

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

    fn convert_data_type(data_type: &str) -> DataType {
        // Use the new DataType system that supports TypeInfo
        DataType::from_type_str(data_type)
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

    /// Generate component method nodes bound to specific instances from a prefab
    ///
    /// This creates blueprint nodes for all methods (both auto-generated property accessors
    /// and manually marked methods) on components in the prefab. Each node is bound to a
    /// specific component instance.
    pub fn generate_component_nodes(prefab: &PrefabAsset) -> Vec<NodeCategory> {
        let mut categories = Vec::new();

        // Group components by class name
        let mut component_classes: HashMap<String, Vec<usize>> = HashMap::new();
        for (index, component) in prefab.components.iter().enumerate() {
            component_classes
                .entry(component.class_name.clone())
                .or_default()
                .push(index);
        }

        // For each component class, create bound method nodes
        for (class_name, indices) in component_classes {
            let mut nodes = Vec::new();

            if let Some(instance) = REGISTRY.create_instance(&class_name) {
                for property in instance.get_properties() {
                    for &index in &indices {
                        nodes.push(Self::create_bound_property_getter_node(
                            &class_name,
                            property.name,
                            &property.display_name,
                            property.type_info,
                            index,
                        ));
                        nodes.push(Self::create_bound_property_setter_node(
                            &class_name,
                            property.name,
                            &property.display_name,
                            property.type_info,
                            index,
                        ));
                    }
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

    fn create_bound_property_getter_node(
        class_name: &str,
        prop_name: &str,
        display_name: &str,
        type_info: &'static RuntimeTypeInfo,
        instance_index: usize,
    ) -> NodeDefinition {
        let inputs = vec![];
        let outputs = vec![PinDefinition {
            id: "value".to_string(),
            name: "value".to_string(),
            data_type: runtime_type_to_data_type(type_info),
            pin_type: PinType::Output,
        }];

        NodeDefinition {
            id: format!("get_{}_{}_{}", class_name, prop_name, instance_index),
            name: format!("Get {} [{} {}]", display_name, class_name, instance_index),
            icon: "⬇".to_string(),
            description: format!("Get {}.{}", class_name, prop_name),
            documentation: format!(
                "Gets the {} property from {} component instance {}",
                prop_name, class_name, instance_index
            ),
            inputs,
            outputs,
            properties: HashMap::new(),
            color: Some("#2ECC71".to_string()),
        }
    }

    fn create_bound_property_setter_node(
        class_name: &str,
        prop_name: &str,
        display_name: &str,
        type_info: &'static RuntimeTypeInfo,
        instance_index: usize,
    ) -> NodeDefinition {
        let inputs = vec![
            PinDefinition {
                id: "exec".to_string(),
                name: "exec".to_string(),
                data_type: DataType::from_type_str("execution"),
                pin_type: PinType::Input,
            },
            PinDefinition {
                id: "value".to_string(),
                name: "value".to_string(),
                data_type: runtime_type_to_data_type(type_info),
                pin_type: PinType::Input,
            },
        ];
        let outputs = vec![PinDefinition {
            id: "exec_out".to_string(),
            name: "exec".to_string(),
            data_type: DataType::from_type_str("execution"),
            pin_type: PinType::Output,
        }];

        NodeDefinition {
            id: format!("set_{}_{}_{}", class_name, prop_name, instance_index),
            name: format!("Set {} [{} {}]", display_name, class_name, instance_index),
            icon: "⬆".to_string(),
            description: format!("Set {}.{}", class_name, prop_name),
            documentation: format!(
                "Sets the {} property on {} component instance {}",
                prop_name, class_name, instance_index
            ),
            inputs,
            outputs,
            properties: HashMap::new(),
            color: Some("#E67E22".to_string()),
        }
    }
}

fn runtime_type_to_data_type(type_info: &RuntimeTypeInfo) -> DataType {
    match &type_info.structure {
        TypeStructure::Primitive => match type_info.base_name() {
            "f32" => DataType::from_type_str("f32"),
            "i32" => DataType::from_type_str("i32"),
            "bool" => DataType::from_type_str("bool"),
            "[f32; 3]" => DataType::from_type_str("[f32; 3]"),
            "[f32; 4]" => DataType::from_type_str("[f32; 4]"),
            other => DataType::from_type_str(other),
        },
        TypeStructure::String => DataType::from_type_str("String"),
        TypeStructure::Enum { .. } => DataType::from_type_str("i32"),
        TypeStructure::Wrapper {
            wrapper_kind: WrapperType::Vec,
            inner,
        } => DataType::from_type_str(&format!("Vec<{}>", inner.base_name())),
        TypeStructure::Wrapper { .. } => DataType::from_type_str(type_info.base_name()),
        TypeStructure::Struct { .. } => DataType::from_type_str(type_info.base_name()),
    }
}
