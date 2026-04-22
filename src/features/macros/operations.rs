//! Macro operations - Opening, creating, and navigating macros

use gpui::*;
use crate::editor::panel::BlueprintEditorPanel;
use crate::editor::GraphTab;
use crate::BlueprintGraph;

impl BlueprintEditorPanel {
    /// Open a local macro in a new tab
    pub fn open_local_macro(&mut self, macro_id: String, macro_name: String, window: &mut Window, cx: &mut Context<Self>) {
        // Check if already open
        if let Some(index) = self.open_tabs.iter().position(|tab| tab.id == macro_id) {
            self.switch_to_tab(index, window, cx);
            return;
        }

        // Find and open macro
        if let Some(macro_def) = self.local_macros.iter().find(|m| m.id == macro_id) {
            // Convert to BlueprintGraph (simplified for now)
            let graph = BlueprintGraph {
                nodes: Vec::new(),
                connections: Vec::new(),
                comments: Vec::new(),
                selected_nodes: Vec::new(),
                selected_comments: Vec::new(),
                zoom_level: 1.0,
                pan_offset: Point::new(0.0, 0.0),
                virtualization_stats: crate::VirtualizationStats::default(),
            };

            self.sync_graph_to_active_tab();

            let new_tab = GraphTab {
                id: macro_id,
                name: macro_name.clone(),
                graph,
                is_main: false,
                is_dirty: false,
                is_library_macro: false,
                library_id: None,
            };

            self.open_tabs.push(new_tab);
            self.active_tab_index = self.open_tabs.len() - 1;
            self.load_active_tab_graph();
            self.graph_workspace_tabs_dirty = true;
            self.refresh_graph_workspace_tabs(window, cx);

            tracing::info!("Opened local macro in tab: {}", macro_name);
            cx.notify();
        }
    }

    /// Open a global/engine macro in a new tab
    pub fn open_global_macro(&mut self, macro_id: String, macro_name: String, window: &mut Window, cx: &mut Context<Self>) {
        // Check if already open
        if let Some(index) = self.open_tabs.iter().position(|tab| tab.id == macro_id) {
            self.switch_to_tab(index, window, cx);
            return;
        }

        // Request opening library view (app-level navigation)
        let library_id = self.get_macro_library_id(&macro_id);

        if let Some(lib_id) = library_id.as_ref() {
            self.request_open_engine_library(
                lib_id.clone(),
                "Engine Library".to_string(),
                Some(macro_id.clone()),
                Some(macro_name.clone()),
                cx,
            );
        }
    }

    /// Get library ID for a macro
    pub fn get_macro_library_id(&self, macro_id: &str) -> Option<String> {
        if self.local_macros.iter().any(|m| m.id == macro_id) {
            return None;
        }

        self.library_manager.get_libraries()
            .iter()
            .find(|(_, lib)| lib.subgraphs.iter().any(|sg| sg.id == macro_id))
            .map(|(id, _)| id.clone())
    }

    /// Request opening engine library (emits event for app-level handling)
    pub fn request_open_engine_library(
        &self,
        library_id: String,
        library_name: String,
        macro_id: Option<String>,
        macro_name: Option<String>,
        cx: &mut Context<Self>,
    ) {
        cx.emit(crate::OpenEngineLibraryRequest {
            library_id,
            library_name,
            macro_id,
            macro_name,
        });
    }

    /// Create a new local macro from current selection
    pub fn create_new_local_macro(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let macro_name = format!("Macro {}", self.local_macros.len() + 1);
        let macro_id = uuid::Uuid::new_v4().to_string();

        // Create new empty macro
        let macro_def = ui::graph::SubGraphDefinition {
            id: macro_id.clone(),
            name: macro_name.clone(),
            description: "New macro".to_string(),
            graph: ui::graph::GraphDescription::new(&macro_name),
            interface: ui::graph::SubGraphInterface {
                inputs: Vec::new(),
                outputs: Vec::new(),
            },
            metadata: ui::graph::SubGraphMetadata {
                created_at: chrono::Utc::now().to_rfc3339(),
                modified_at: chrono::Utc::now().to_rfc3339(),
                author: Some(String::new()),
                tags: Vec::new(),
            },
            macro_config: ui::graph::MacroConfiguration::default(),
        };

        self.local_macros.push(macro_def);
        self.open_local_macro(macro_id, macro_name, window, cx);
    }
}
