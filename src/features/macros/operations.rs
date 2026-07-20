//! Macro operations — creating, opening, editing, and placing macro instances.

use crate::core::graph::BlueprintGraph;
use crate::core::types::{BlueprintNode, NodeType, Pin, PinType};
use crate::editor::panel::BlueprintEditorPanel;
use crate::editor::GraphTab;
use crate::rendering::layout;
use gpui::*;
use std::collections::HashMap;
use crate::core::types::PinDataType as DataType;
use ui::PixelsExt;

impl BlueprintEditorPanel {
    // ─── Queries ──────────────────────────────────────────────────────────────

    /// Returns the macro ID currently being edited in the active tab, if any.
    pub fn current_editing_macro_id(&self) -> Option<&str> {
        let tab = self.open_tabs.get(self.active_tab_index)?;
        if !tab.is_main {
            Some(tab.id.as_str())
        } else {
            None
        }
    }

    /// True when the given macro would be nested inside itself in the active tab.
    pub fn would_nest_macro(&self, macro_id: &str) -> bool {
        self.current_editing_macro_id() == Some(macro_id)
    }

    // ─── Opening macros ───────────────────────────────────────────────────────

    /// Open a local macro for editing in a new tab (or switch to it if already open).
    pub fn open_local_macro(
        &mut self,
        macro_id: String,
        macro_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Switch to existing tab if open.
        if let Some(index) = self.open_tabs.iter().position(|tab| tab.id == macro_id) {
            self.switch_to_tab(index, window, cx);
            return;
        }

        // Flush the current active canvas into its tab before leaving.
        let active_tab_id = self.open_tabs.get(self.active_tab_index).map(|t| t.id.clone());
        if let Some(tab_id) = active_tab_id {
            if let Some((_, canvas)) = self.graph_panels.iter().find(|(id, _)| id == &tab_id) {
                let live = canvas.read(cx).graph.clone();
                self.graph = live.clone();
                if let Some(tab) = self.open_tabs.get_mut(self.active_tab_index) {
                    tab.graph = live;
                }
            }
        }

        // Push an empty tab for the new macro.
        let new_tab = GraphTab {
            id: macro_id.clone(),
            name: macro_name.clone(),
            graph: BlueprintGraph {
                nodes: Vec::new(),
                connections: Vec::new(),
                comments: Vec::new(),
                selected_nodes: Vec::new(),
                selected_comments: Vec::new(),
                zoom_level: 1.0,
                pan_offset: Point::new(0.0, 0.0),
                virtualization_stats: crate::VirtualizationStats::default(),
                custom_event_defs: std::collections::HashMap::new(),
            },
            is_main: false,
            is_dirty: false,
            is_library_macro: false,
            library_id: None,
        };

        self.open_tabs.push(new_tab);
        self.active_tab_index = self.open_tabs.len() - 1;

        // Seed entry/exit nodes directly into the tab graph.
        // No canvas exists yet — it will be created from this tab graph.
        self.sync_entry_exit_in_active_graph(&macro_id, cx);

        self.graph_workspace_tabs_dirty = true;
        self.refresh_graph_workspace_tabs(window, cx);
        cx.notify();
    }

    /// Open a global/engine library macro.
    pub fn open_global_macro(
        &mut self,
        macro_id: String,
        macro_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self.open_tabs.iter().position(|tab| tab.id == macro_id) {
            self.switch_to_tab(index, window, cx);
            return;
        }
        if let Some(lib_id) = self.get_macro_library_id(&macro_id) {
            self.request_open_engine_library(
                lib_id,
                "Engine Library".to_string(),
                Some(macro_id),
                Some(macro_name),
                cx,
            );
        }
    }

    /// Open the graph backing a placed macro instance.
    ///
    /// Local macros open/switch to a local macro tab.
    /// Library macros route through `OpenEngineLibraryRequest`.
    pub fn open_macro_from_instance(
        &mut self,
        macro_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(local) = self.local_macros.iter().find(|m| m.id == macro_id) {
            self.open_local_macro(local.id.clone(), local.name.clone(), window, cx);
            return;
        }

        if let Some(lib_id) = self.get_macro_library_id(macro_id) {
            let macro_name = self
                .library_manager
                .get_libraries()
                .get(&lib_id)
                .and_then(|lib| {
                    lib.subgraphs
                        .iter()
                        .find(|sg| sg.id == macro_id)
                        .map(|sg| sg.name.clone())
                })
                .unwrap_or_else(|| "Macro".to_string());

            self.request_open_engine_library(
                lib_id,
                "Engine Library".to_string(),
                Some(macro_id.to_string()),
                Some(macro_name),
                cx,
            );
        }
    }

    /// Return the library ID that owns `macro_id`, or `None` if it is a local macro.
    pub fn get_macro_library_id(&self, macro_id: &str) -> Option<String> {
        if self.local_macros.iter().any(|m| m.id == macro_id) {
            return None;
        }
        self.library_manager
            .get_libraries()
            .iter()
            .find(|(_, lib)| lib.subgraphs.iter().any(|sg| sg.id == macro_id))
            .map(|(id, _)| id.clone())
    }

    /// Emit an `OpenEngineLibraryRequest` event.
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

    // ─── Creating macros ──────────────────────────────────────────────────────

    /// Create a new empty local macro and open it for editing.
    pub fn create_new_local_macro(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let macro_name = format!("Macro {}", self.local_macros.len() + 1);
        let macro_id = uuid::Uuid::new_v4().to_string();

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
        self.invalidate_palette(cx);
    }

    /// Rename a local macro in-place.
    pub fn rename_local_macro(&mut self, macro_id: &str, new_name: String, cx: &mut Context<Self>) {
        if let Some(m) = self.local_macros.iter_mut().find(|m| m.id == macro_id) {
            m.name = new_name.clone();
        }
        // Update the tab name too.
        if let Some(tab) = self.open_tabs.iter_mut().find(|t| t.id == macro_id) {
            tab.name = new_name;
        }
        self.invalidate_palette(cx);
        cx.notify();
    }

    /// Delete a local macro and all its open tabs.
    pub fn delete_local_macro(&mut self, macro_id: &str, cx: &mut Context<Self>) {
        self.local_macros.retain(|m| m.id != macro_id);
        let before = self.open_tabs.len();
        self.open_tabs.retain(|t| t.id != macro_id);
        if self.open_tabs.len() < before {
            self.active_tab_index = self
                .active_tab_index
                .min(self.open_tabs.len().saturating_sub(1));
        }
        self.invalidate_palette(cx);
        cx.notify();
    }

    // ─── Interface pin management ─────────────────────────────────────────────

    /// Add a pin to a local macro's interface.
    ///
    /// `is_input` — true for an input pin on the macro instance (exposed via
    /// the Macro Entry node inside the graph), false for an output.
    pub fn add_macro_pin(
        &mut self,
        macro_id: &str,
        pin_name: String,
        type_str: String,
        is_input: bool,
        cx: &mut Context<Self>,
    ) {
        let pin = ui::graph::SubGraphPin {
            id: uuid::Uuid::new_v4().to_string(),
            name: pin_name,
            data_type: ui::graph::DataType::from_type_str(&type_str),
            description: None,
            default_value: None,
            is_instance_editable: false,
            category: None,
        };

        if let Some(m) = self.local_macros.iter_mut().find(|m| m.id == macro_id) {
            if is_input {
                m.interface.inputs.push(pin);
            } else {
                m.interface.outputs.push(pin);
            }
        }

        let macro_id = macro_id.to_string();
        self.sync_entry_exit_in_active_graph(&macro_id, cx);
        self.sync_all_macro_instances(&macro_id, cx);
        self.invalidate_palette(cx);
    }

    /// Remove a pin from a local macro's interface.
    pub fn remove_macro_pin(
        &mut self,
        macro_id: &str,
        pin_id: &str,
        is_input: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(m) = self.local_macros.iter_mut().find(|m| m.id == macro_id) {
            if is_input {
                m.interface.inputs.retain(|p| p.id != pin_id);
            } else {
                m.interface.outputs.retain(|p| p.id != pin_id);
            }
        }

        let macro_id = macro_id.to_string();
        self.sync_entry_exit_in_active_graph(&macro_id, cx);
        self.sync_all_macro_instances(&macro_id, cx);
        self.invalidate_palette(cx);
    }

    // ─── Entry / Exit synchronisation ────────────────────────────────────────

    /// Ensure the Macro Entry and Macro Exit nodes in the macro's graph reflect
    /// the current interface of `macro_id`.
    ///
    /// If the macro canvas is already open, the live canvas graph is updated
    /// directly (no full replacement — user nodes are preserved).
    /// The matching tab snapshot is always updated so serialisation is correct.
    pub fn sync_entry_exit_in_active_graph(&mut self, macro_id: &str, cx: &mut Context<Self>) {
        // Only run when the active tab belongs to this macro.
        let is_active = self
            .open_tabs
            .get(self.active_tab_index)
            .map(|t| t.id == macro_id && !t.is_main)
            .unwrap_or(false);
        if !is_active {
            return;
        }

        let Some(macro_def) = self.local_macros.iter().find(|m| m.id == macro_id).cloned() else {
            return;
        };

        let entry_id = format!("macro_entry_{}", macro_id);
        let exit_id = format!("macro_exit_{}", macro_id);

        // Build the desired pin lists.
        let entry_outputs: Vec<Pin> = macro_def
            .interface
            .inputs
            .iter()
            .map(|p| Pin {
                id: p.id.clone(),
                name: p.name.clone(),
                pin_type: PinType::Output,
                data_type: DataType::from_type_str(p.data_type.to_string()),
            })
            .collect();

        let exit_inputs: Vec<Pin> = macro_def
            .interface
            .outputs
            .iter()
            .map(|p| Pin {
                id: p.id.clone(),
                name: p.name.clone(),
                pin_type: PinType::Input,
                data_type: DataType::from_type_str(p.data_type.to_string()),
            })
            .collect();

        // Helper: apply entry/exit changes to a graph in-place.
        // Does NOT replace the whole graph — only touches the sentinel nodes.
        let apply = |graph: &mut BlueprintGraph| {
            // Entry node
            if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == entry_id) {
                node.title = macro_def.name.clone();
                node.outputs = entry_outputs.clone();
                let rows = node.outputs.len().max(1);
                node.size.height = layout::node_height_for_pin_rows(rows);
            } else {
                let rows = entry_outputs.len().max(1);
                graph.nodes.insert(
                    0,
                    BlueprintNode {
                        id: entry_id.clone(),
                        definition_id: "macro_entry".to_string(),
                        title: macro_def.name.clone(),
                        icon: "▶".to_string(),
                        node_type: NodeType::MacroEntry,
                        position: Point::new(60.0, 180.0),
                        size: gpui::Size::new(180.0, layout::node_height_for_pin_rows(rows)),
                        inputs: vec![],
                        outputs: entry_outputs.clone(),
                        properties: HashMap::new(),
                        is_selected: false,
                        description: format!("Entry — provides inputs into '{}'", macro_def.name),
                        color: Some("#7C3AED".to_string()),
                    },
                );
            }

            // Exit node
            if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == exit_id) {
                node.title = format!("{} (Return)", macro_def.name);
                node.inputs = exit_inputs.clone();
                let rows = node.inputs.len().max(1);
                node.size.height = layout::node_height_for_pin_rows(rows);
            } else {
                let rows = exit_inputs.len().max(1);
                graph.nodes.push(BlueprintNode {
                    id: exit_id.clone(),
                    definition_id: "macro_exit".to_string(),
                    title: format!("{} (Return)", macro_def.name),
                    icon: "◀".to_string(),
                    node_type: NodeType::MacroExit,
                    position: Point::new(820.0, 180.0),
                    size: gpui::Size::new(180.0, layout::node_height_for_pin_rows(rows)),
                    inputs: exit_inputs.clone(),
                    outputs: vec![],
                    properties: HashMap::new(),
                    is_selected: false,
                    description: format!("Exit — collects outputs from '{}'", macro_def.name),
                    color: Some("#7C3AED".to_string()),
                });
            }
        };

        // Apply to the active tab snapshot synchronously — safe because we are
        // only modifying data, not crossing entity update boundaries.
        if let Some(tab) = self.open_tabs.get_mut(self.active_tab_index) {
            apply(&mut tab.graph);
            self.graph = tab.graph.clone();
        }

        // If the canvas entity is already open we must also update it, BUT we
        // cannot call canvas.update(cx, …) here if we were called from inside a
        // canvas update (e.g. from the "Add pin" button in graph.rs).  Defer to
        // the next event-loop tick so the current update chain has unwound first.
        if let Some(canvas) = self.active_canvas().cloned() {
            // Clone the data the deferred closure needs — all 'static types.
            let entry_id_d = entry_id.clone();
            let exit_id_d = exit_id.clone();
            let macro_name_d = macro_def.name.clone();
            let entry_outputs_d = entry_outputs.clone();
            let exit_inputs_d = exit_inputs.clone();
            cx.defer(move |cx| {
                canvas.update(cx, |canvas_panel, cx| {
                    // Entry sentinel
                    if let Some(node) = canvas_panel.graph.nodes.iter_mut().find(|n| n.id == entry_id_d) {
                        node.title = macro_name_d.clone();
                        node.outputs = entry_outputs_d.clone();
                        let rows = node.outputs.len().max(1);
                        node.size.height = layout::node_height_for_pin_rows(rows);
                    } else {
                        let rows = entry_outputs_d.len().max(1);
                        canvas_panel.graph.nodes.insert(0, BlueprintNode {
                            id: entry_id_d.clone(),
                            definition_id: "macro_entry".to_string(),
                            title: macro_name_d.clone(),
                            icon: "▶".to_string(),
                            node_type: NodeType::MacroEntry,
                            position: Point::new(60.0, 180.0),
                            size: gpui::Size::new(180.0, layout::node_height_for_pin_rows(rows)),
                            inputs: vec![],
                            outputs: entry_outputs_d.clone(),
                            properties: HashMap::new(),
                            is_selected: false,
                            description: format!("Entry — provides inputs into '{}'", macro_name_d),
                            color: Some("#7C3AED".to_string()),
                        });
                    }
                    // Exit sentinel
                    if let Some(node) = canvas_panel.graph.nodes.iter_mut().find(|n| n.id == exit_id_d) {
                        node.title = format!("{} (Return)", macro_name_d);
                        node.inputs = exit_inputs_d.clone();
                        let rows = node.inputs.len().max(1);
                        node.size.height = layout::node_height_for_pin_rows(rows);
                    } else {
                        let rows = exit_inputs_d.len().max(1);
                        canvas_panel.graph.nodes.push(BlueprintNode {
                            id: exit_id_d.clone(),
                            definition_id: "macro_exit".to_string(),
                            title: format!("{} (Return)", macro_name_d),
                            icon: "◀".to_string(),
                            node_type: NodeType::MacroExit,
                            position: Point::new(820.0, 180.0),
                            size: gpui::Size::new(180.0, layout::node_height_for_pin_rows(rows)),
                            inputs: exit_inputs_d.clone(),
                            outputs: vec![],
                            properties: HashMap::new(),
                            is_selected: false,
                            description: format!("Exit — collects outputs from '{}'", macro_name_d),
                            color: Some("#7C3AED".to_string()),
                        });
                    }
                    cx.notify();
                });
            });
        }

        cx.notify();
    }

    // ─── MacroInstance placement ───────────────────────────────────────────────

    /// Create a `MacroInstance` node at `position` for the local macro identified
    /// by `macro_id`.  Rejects the operation silently if the active tab IS that
    /// macro (prevents a macro from containing itself).
    /// Delegate macro instance creation to the active canvas.
    pub fn create_macro_instance_node(
        &mut self,
        macro_id: String,
        position: Point<f32>,
        cx: &mut Context<Self>,
    ) {
        if let Some(canvas) = self.active_canvas().cloned() {
            cx.defer(move |cx| {
                canvas.update(cx, |canvas, cx| {
                    canvas.create_macro_instance_node(macro_id, position, cx);
                });
            });
        }
    }

    /// Update the pins of every `MacroInstance` node that references `macro_id`
    /// across all tab snapshots AND all live canvas graphs.
    pub fn sync_all_macro_instances(&mut self, macro_id: &str, cx: &mut Context<Self>) {
        let def_prefix = format!("macro:{}", macro_id);
        let Some(macro_def) = self.local_macros.iter().find(|m| m.id == macro_id).cloned() else {
            return;
        };

        let rebuild_node = |node: &mut BlueprintNode| {
            if node.definition_id == def_prefix && node.node_type == NodeType::MacroInstance {
                node.inputs = macro_def
                    .interface
                    .inputs
                    .iter()
                    .map(|p| Pin {
                        id: p.id.clone(),
                        name: p.name.clone(),
                        pin_type: PinType::Input,
                        data_type: DataType::from_type_str(p.data_type.to_string()),
                    })
                    .collect();
                node.outputs = macro_def
                    .interface
                    .outputs
                    .iter()
                    .map(|p| Pin {
                        id: p.id.clone(),
                        name: p.name.clone(),
                        pin_type: PinType::Output,
                        data_type: DataType::from_type_str(p.data_type.to_string()),
                    })
                    .collect();
                let rows = node.inputs.len().max(node.outputs.len()).max(1);
                node.size.height = layout::node_height_for_pin_rows(rows);
            }
        };

        // Update all tab snapshots (covers closed tabs and the authoritative store).
        for tab in self.open_tabs.iter_mut() {
            for node in tab.graph.nodes.iter_mut() {
                rebuild_node(node);
            }
        }

        // Also update live canvas graphs so the display is immediately correct.
        // Defer each canvas.update to avoid reentrancy — this function can be
        // called from within a canvas update (e.g. add/remove macro pin button).
        let canvases: Vec<Entity<crate::editor::workspace_panels::GraphCanvasPanel>> =
            self.graph_panels.iter().map(|(_, c)| c.clone()).collect();
        for canvas in canvases {
            let def_prefix_d = def_prefix.clone();
            let macro_def_d = macro_def.clone();
            cx.defer(move |cx| {
                canvas.update(cx, |canvas_panel, cx| {
                    for node in canvas_panel.graph.nodes.iter_mut() {
                        if node.definition_id == def_prefix_d && node.node_type == NodeType::MacroInstance {
                            node.inputs = macro_def_d.interface.inputs.iter().map(|p| Pin {
                                id: p.id.clone(),
                                name: p.name.clone(),
                                pin_type: PinType::Input,
                                data_type: DataType::from_type_str(p.data_type.to_string()),
                            }).collect();
                            node.outputs = macro_def_d.interface.outputs.iter().map(|p| Pin {
                                id: p.id.clone(),
                                name: p.name.clone(),
                                pin_type: PinType::Output,
                                data_type: DataType::from_type_str(p.data_type.to_string()),
                            }).collect();
                            let rows = node.inputs.len().max(node.outputs.len()).max(1);
                            node.size.height = layout::node_height_for_pin_rows(rows);
                        }
                    }
                    cx.notify();
                });
            });
        }

        // Keep the self.graph shadow in sync with the active tab.
        if let Some(tab) = self.open_tabs.get(self.active_tab_index) {
            self.graph = tab.graph.clone();
        }
    }

    // ─── Palette invalidation ─────────────────────────────────────────────────

    /// Notify the active canvas's quick-palette to rebuild so local macros appear.
    pub fn invalidate_palette(&self, cx: &mut Context<Self>) {
        if let Some(canvas) = self.active_canvas().cloned() {
            cx.defer(move |cx| {
                canvas.update(cx, |canvas, cx| {
                    let v = canvas.quick_palette_view.clone();
                    cx.defer(move |cx| {
                        v.update(cx, |view, cx| view.rebuild_items(cx));
                    });
                });
            });
        }
    }
}

// ─── Canvas-side macro operations ────────────────────────────────────────────

impl crate::editor::workspace_panels::GraphCanvasPanel {
    /// Place a MacroInstance node built from an already-resolved macro definition.
    pub fn create_macro_instance_node(
        &mut self,
        macro_id: String,
        position: Point<f32>,
        cx: &mut Context<Self>,
    ) {
        // Get the macro definition from the shared panel
        let macro_def = self.panel.upgrade().and_then(|p| {
            p.read(cx)
                .local_macros
                .iter()
                .find(|m| m.id == macro_id)
                .cloned()
        });
        let Some(macro_def) = macro_def else { return };

        let inputs: Vec<crate::core::types::Pin> = macro_def
            .interface
            .inputs
            .iter()
            .map(|p| crate::core::types::Pin {
                id: p.id.clone(),
                name: p.name.clone(),
                pin_type: crate::core::types::PinType::Input,
                data_type: DataType::from_type_str(p.data_type.to_string()),
            })
            .collect();
        let outputs: Vec<crate::core::types::Pin> = macro_def
            .interface
            .outputs
            .iter()
            .map(|p| crate::core::types::Pin {
                id: p.id.clone(),
                name: p.name.clone(),
                pin_type: crate::core::types::PinType::Output,
                data_type: DataType::from_type_str(p.data_type.to_string()),
            })
            .collect();
        let max_rows = inputs.len().max(outputs.len()).max(1);
        let node = crate::core::types::BlueprintNode {
            id: uuid::Uuid::new_v4().to_string(),
            definition_id: format!("macro:{}", macro_id),
            title: macro_def.name.clone(),
            icon: "📦".to_string(),
            node_type: crate::core::types::NodeType::MacroInstance,
            position,
            size: gpui::Size::new(200.0, layout::node_height_for_pin_rows(max_rows)),
            inputs,
            outputs,
            properties: std::collections::HashMap::new(),
            is_selected: false,
            description: format!("Instance of macro '{}'", macro_def.name),
            color: Some("#9B59B6".to_string()),
        };
        self.add_node(node, cx);
    }

    /// Called when the user drops a `MacroDrag` payload onto this canvas.
    pub fn finish_dragging_macro(&mut self, window_pos: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(drag) = self.dragging_macro.take() else {
            return;
        };
        let origin = *self.canvas_origin.borrow();
        let cp = Point::new(
            window_pos.x.as_f32() - origin.x,
            window_pos.y.as_f32() - origin.y,
        );
        let z = self.graph.zoom_level;
        let graph_pos = Point::new(
            cp.x / z - self.graph.pan_offset.x,
            cp.y / z - self.graph.pan_offset.y,
        );
        self.create_macro_instance_node(drag.macro_id, graph_pos, cx);
    }

    /// Cancel a pending macro drag without creating a node.
    pub fn cancel_dragging_macro(&mut self, cx: &mut Context<Self>) {
        self.dragging_macro = None;
        cx.notify();
    }
}
