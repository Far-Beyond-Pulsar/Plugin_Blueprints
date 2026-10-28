//! Core panel struct and initialization
//!
//! This module contains the main `BlueprintEditorPanel` struct definition,
//! constructors, and basic accessors.

use gpui::*;

use std::collections::{HashMap, HashSet};

use ui::{
    input::{InputEvent, InputState},
    resizable::ResizableState,
    scroll::ScrollbarState,
    VirtualListScrollHandle,
};

use super::tabs::{GraphTab, TabDoc};
use crate::core::{events::*, graph::*, types::*};
use crate::editor::graph_canvas_panel::GraphCanvasPanel;
use crate::editor::workspace_panels::FindPanel;

use crate::features::prefabs::PrefabAsset;
use crate::features::variables::ClassVariable;
use crate::ui_components::palette_view::NodePaletteView;
use graphy_blueprint::{LibraryManager, SubGraph};
use ui::dock::{DockItem, DockPlacement};
use ui::dropdown::{SearchableList, SearchableListEvent};

/// Which item is being renamed inline in a hierarchy panel.
#[derive(Clone, Debug, PartialEq)]
pub enum RenameTarget {
    Event(String),
    Macro(String),
}

/// Main Blueprint Editor Panel struct
pub struct BlueprintEditorPanel {
    pub(super) focus_handle: FocusHandle,
    /// Asset metadata is kept intact across UI edits and serialization.
    pub blueprint_metadata: graphy_blueprint::BlueprintMetadata,
    /// Project root used for VFS-backed trait discovery and index refresh.
    pub project_root: Option<std::path::PathBuf>,
    pub implemented_trait_catalog: Vec<crate::features::traits::TraitAssetSummary>,
    pub implemented_trait_catalog_error: Option<String>,
    pub implemented_trait_catalog_loaded: bool,
    pub implemented_trait_catalog_loading: bool,
    pub implemented_trait_catalog_refresh_pending: bool,
    pub implemented_trait_status: Option<String>,
    pub implemented_trait_picker:
        Entity<SearchableList<crate::features::traits::TraitAssetSummary>>,

    // Workspace with full docking support
    pub(super) workspace: Option<Entity<ui::workspace::Workspace>>,
    pub(super) find_panel: Option<Entity<FindPanel>>,

    // File I/O
    pub current_class_path: Option<std::path::PathBuf>,
    pub tab_title: Option<String>,
    /// Initial graph data is being read on the background executor.
    pub file_loading: bool,
    /// Set when a requested file could not be loaded. Failed opens render an
    /// explicit error state instead of presenting an empty, editable document.
    pub load_error: Option<String>,







    // Variables system
    pub class_variables: Vec<ClassVariable>,
    pub selected_variable: Option<usize>,
    pub is_creating_variable: bool,
    pub variable_name_input: Entity<InputState>,
    pub variable_type_dropdown:
        Entity<ui::dropdown::DropdownState<Vec<crate::features::variables::TypeItem>>>,
    pub dragging_variable: Option<crate::features::variables::VariableDrag>,
    pub variable_drop_menu_position: Option<Point<f32>>,

    // Prefab sidecar authoring
    pub prefab_asset: PrefabAsset,
    pub prefab_component_list: Entity<SearchableList<&'static str>>,
    pub show_add_component_dialog: bool,
    pub prefab_property_state: ui_common::reflected_properties_panel::PropertyStateManager,
    pub prefab_collapsed_categories: HashSet<(usize, String)>,
    pub prefab_expanded_categories: HashSet<(usize, String)>,
    pub selected_prefab_component: Option<usize>,

    /// The profile whose document this editor edits, if it is not a Blueprint.
    pub profile: Option<std::sync::Arc<dyn crate::profile::EditorProfile>>,

    // Subscriptions
    pub subscriptions: Vec<Subscription>,

    // Compilation
    pub compilation_status: CompilationStatus,
    pub compilation_history: Vec<CompilationHistoryEntry>,
    /// Diagnostics from the last validation stage run (#656 preflight).
    pub validation_problems: Vec<String>,
    pub compile_mode: crate::core::types::CompileMode,
    pub compiler_output_scroll_handle: VirtualListScrollHandle,
    pub compiler_output_scrollbar_state: ScrollbarState,
    pub find_search_input: Entity<InputState>,
    pub find_search_query: String,
    pub find_output_scroll_handle: VirtualListScrollHandle,
    pub find_output_scrollbar_state: ScrollbarState,

    // Library/macro system
    pub library_manager: LibraryManager,
    pub subgraphs: Vec<SubGraph>,
    pub selected_macro: Option<usize>,
    // Event system (mirrors macro storage pattern)
    pub local_event_defs: Vec<crate::core::graph::EventDefinition>,
    /// Host-owned event signatures used by the palette and graph linker.
    pub(crate) component_event_metadata: Vec<plugin_editor_api::ComponentEventMetadata>,
    pub selected_event: Option<usize>,

    // Rename state — shared across event/macro/variable panels
    pub renaming_target: Option<RenameTarget>,
    pub rename_input: Entity<InputState>,

    // Tab system
    /// Subgraph/library/event data the canvas profile presents nodes from.
    pub profile_data: crate::canvas_profile::SharedProfileData,
    pub open_tabs: Vec<GraphTab>,
    pub active_tab_index: usize,
    pub graph_panels: Vec<(String, Entity<GraphCanvasPanel>)>,
    pub graph_workspace_tabs_dirty: bool,

    // Overlay toggles
    pub show_debug_overlay: bool,
    pub show_minimap: bool,
    pub show_graph_controls: bool,
    pub wire_active_test_mode: bool,
    pub wire_hidden_test_mode: bool,
    pub running_nodes: HashSet<String>,
    pub graph_anim_start: std::time::Instant,

    // Quick palette overlay (right-click on graph canvas)
    pub popup_palette_graph_pos: Option<Point<f32>>,
    /// Whether the right-click quick-palette overlay is currently visible.
    pub quick_palette_open: bool,
    /// Whether the quick-palette search input should be focused on next paint.
    pub quick_palette_focus_pending: bool,
    /// When opening quick palette from a connection drag, this is the source drag metadata.
    pub quick_palette_connection_source:
        Option<crate::editor::graph_canvas_panel::ConnectionDrag>,
    /// Window-space position where the user right-clicked (used to anchor the overlay).
    pub quick_palette_screen_pos: Point<Pixels>,
    /// The shared palette view rendered inside the overlay.
    pub quick_palette_view: Entity<NodePaletteView>,

    // Pin hover tooltip state
    pub hovered_pin_tooltip: Option<String>,
    pub hovered_pin_tooltip_pos: Option<Point<Pixels>>,

    // Tab drag state
    pub dragging_tab: Option<TabDragInfo>,

    pub is_dirty: bool, // Whether there are unsaved changes
    /// True while a toolbar save is running in the editor task queue.
    pub is_saving: bool,

    /// The host's undo histories (Pulsar-Native#931), once given them.
    pub undo_service: Option<plugin_editor_api::UndoService>,



    // ── Debugger ──────────────────────────────────────────────────────────────
    /// Set of node IDs that have an active breakpoint.
    pub breakpoints: HashSet<String>,
    /// Present while a debug session is live (executor paused or navigating frames).
    pub debug_session: Option<crate::features::debug::DebugSession>,

    // ── Macro drag ────────────────────────────────────────────────────────────
    /// Payload from a macro drag that is in flight toward the canvas.
    pub dragging_macro: Option<crate::features::macros::MacroDrag>,

    // ── Macro pin editor state ────────────────────────────────────────────────
    /// When Some: true = adding an input, false = adding an output.
    pub macro_pin_add_mode: Option<bool>,

    // ── Properties panel inline editors ──────────────────────────────────────
    /// Name inputs for macro pins: (macro_id, pin_index, is_input) → Entity
    pub macro_pin_name_inputs: HashMap<(String, usize, bool), Entity<InputState>>,
    /// Type inputs for macro pins: (macro_id, pin_index, is_input) → Entity
    pub macro_pin_type_inputs: HashMap<(String, usize, bool), Entity<InputState>>,
    /// Name inputs for event fields: (event_uid, field_index) → Entity
    pub event_field_name_inputs: HashMap<(String, usize), Entity<InputState>>,
    /// Type inputs for event fields: (event_uid, field_index) → Entity
    pub event_field_type_inputs: HashMap<(String, usize), Entity<InputState>>,
}

/// Information about a tab being dragged
#[derive(Clone, Debug)]
pub struct TabDragInfo {
    pub panel_id: usize,  // Which panel the tab came from
    pub tab_index: usize, // Which tab is being dragged
    pub label: String,
    pub icon: ui::IconName,
}

/// Compilation history entry
#[derive(Clone, Debug)]
pub struct CompilationHistoryEntry {
    pub timestamp: String,
    pub state: CompilationState,
    pub stage: String,
    pub message: String,
    pub detail: Option<String>,
}


impl BlueprintEditorPanel {
    /// Bridge synchronous AI tool calls to the GPUI-owned live canvas. Tool
    /// calls run on worker threads; this task applies each graph snapshot on
    /// the UI thread and acknowledges it before the tool reports success.
    pub(crate) fn attach_ai_graph_updates(
        &mut self,
        file_path: std::path::PathBuf,
        cx: &mut Context<Self>,
    ) {
        let initial = self
            .main_canvas()
            .map(|canvas| canvas.read(cx).graph(cx).clone())
            .unwrap_or_else(|| graphy::GraphDescription::new("EventGraph"));
        let receiver = crate::ai_tools::register_live_editor(file_path.clone(), initial);
        cx.spawn(async move |this, cx| {
            while let Ok(update) = receiver.recv().await {
                let result = this
                    .update(cx, |editor, cx| {
                        editor.apply_ai_graph_update(&file_path, &update.before, &update.after, cx)
                    })
                    .map_err(|error| error.to_string())
                    .and_then(|result| result);
                update.respond(result).await;
            }
        })
        .detach();
    }

    /// The canvas of the main event graph.
    pub fn main_canvas(&self) -> Option<&Entity<GraphCanvasPanel>> {
        let tab = self.open_tabs.iter().find(|tab| tab.is_main)?;
        self.graph_panels
            .iter()
            .find(|(id, _)| id == &tab.id)
            .map(|(_, canvas)| canvas)
    }

    fn apply_ai_graph_update(
        &mut self,
        file_path: &std::path::Path,
        before: &graphy::GraphDescription,
        after: &graphy::GraphDescription,
        cx: &mut Context<Self>,
    ) -> Result<crate::ai_tools::AiGraphUpdateAck, String> {
        let Some(open_path) = self.current_class_path.as_deref() else {
            return Err("Blueprint editor has no loaded class path".to_string());
        };
        if !crate::ai_tools::same_file_key(open_path, file_path) {
            return Err(format!(
                "Requested Blueprint {} does not match this editor {}",
                file_path.display(),
                open_path.display()
            ));
        }

        let Some(canvas) = self.main_canvas().cloned() else {
            return Err("The main Blueprint graph canvas is not ready".to_string());
        };
        let current_graph = canvas.read(cx).graph(cx).clone();
        if !crate::ai_tools::graphs_match(&current_graph, before) {
            return Ok(crate::ai_tools::AiGraphUpdateAck {
                graph: current_graph,
                applied: false,
            });
        }

        canvas
            .update(cx, |canvas, cx| canvas.replace_graph("AI edit", after, cx))
            .map_err(|error| error.to_string())?;
        let graph = canvas.read(cx).graph(cx).clone();
        Ok(crate::ai_tools::AiGraphUpdateAck {
            graph,
            applied: true,
        })
    }

    /// Create a new blueprint editor panel
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new_internal(None, window, cx)
    }

    /// Create a new blueprint editor panel with a file path (for plugin)
    pub fn new_with_path(
        file_path: std::path::PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        tracing::info!(
            ">>> new_with_path: file_path={:?}, graph_file={:?}",
            file_path,
            file_path.join("graph_save.json"),
        );

        let mut panel = Self::new_internal(Some(file_path.clone()), window, cx);
        panel.file_loading = true;
        panel.set_path(file_path.clone());
        // A profile's documents are single files, not `.class` folders.
        let graph_file = if panel.profile.is_some() {
            file_path.clone()
        } else {
            file_path.join("graph_save.json")
        };
        let window_handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let parsed = cx
                .background_executor()
                .spawn(async move { Self::parse_asset_from_path(&graph_file) })
                .await;
            let _ = cx.update_window(window_handle, |_, window, cx| {
                let _ = this.update(cx, |panel, cx| {
                    let result = parsed.and_then(|(path, asset)| {
                        panel.apply_loaded_asset(path, asset, window, cx)
                    });
                    panel.file_loading = false;
                    match result {
                        Ok(()) => {
                            panel.load_error = None;
                            panel.attach_ai_graph_updates(file_path, cx);
                            log::info!("Loaded blueprint asynchronously");
                        }
                        Err(error) => {
                            tracing::error!("Failed to load blueprint: {error}");
                            panel.load_error = Some(error);
                        }
                    }
                    cx.notify();
                });
            });
        })
        .detach();

        Ok(panel)
    }

    /// Build a non-editable panel that explains why a requested blueprint
    /// could not be opened. Keeping this as the same panel type lets both
    /// provider paths report the failure without fabricating a blank document.
    pub fn new_with_load_error(
        file_path: std::path::PathBuf,
        error: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut panel = Self::new_internal(Some(file_path), window, cx);
        panel.load_error = Some(error);
        panel
    }

    /// Create a new blueprint editor panel with a file to load
    pub fn new_with_file(
        file_path: std::path::PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut panel = Self::new_internal(Some(file_path.clone()), window, cx);

        // Keep a failed open visibly distinct from a successfully loaded empty
        // graph. The latter is a valid document; the former must not be edited
        // or saved over the unreadable source.
        match panel.load_blueprint(file_path.to_str().unwrap(), window, cx) {
            Ok(()) => println!("Loaded blueprint from {:?}", file_path),
            Err(error) => {
                log::error!("Failed to load blueprint: {}", error);
                panel.load_error = Some(error);
            }
        }

        panel
    }

    /// Create a new blueprint editor for an engine library (virtual blueprint)
    pub fn new_for_library(
        library_id: String,
        library_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut panel = Self::new_internal(None, window, cx);
        panel.tab_title = Some(format!("Library: {}", library_name));

        if let Some(main_tab) = panel.open_tabs.get_mut(0) {
            main_tab.name = format!("{} Overview", library_name);
        }

        println!("Created blueprint editor for library: {}", library_name);
        panel
    }

    /// Internal constructor with sample graph
    fn new_internal(
        project_path: Option<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let _resizable_state = ResizableState::new(cx);
        let _left_sidebar_resizable_state = ResizableState::new(cx);

        // A sample graph unless a file is about to be loaded.
        let main_doc = if project_path.is_some() {
            TabDoc::empty("EventGraph")
        } else {
            Self::create_sample_graph()
        };

        let profile = project_path.as_deref().and_then(crate::profile::profile_for_path);
        let editor_weak = cx.entity().downgrade();
        let quick_palette_view = cx.new(|cx| NodePaletteView::new(editor_weak, profile.clone(), window, cx));
        let mut engine_classes = pulsar_reflection::REGISTRY.get_class_names();
        // A placed class's own marker component is not a prefab component.
        engine_classes.retain(|name| !crate::features::prefabs::is_internal_component(name));
        engine_classes.sort();
        let prefab_component_list = cx.new(|cx| {
            SearchableList::new(window, cx, engine_classes, |name| name.to_string())
                .with_empty_text("No components found")
                .with_max_width(px(260.0))
                .with_max_height(px(320.0))
                .with_icon_getter(|_| ui::IconName::Component)
        });
        cx.subscribe(
            &prefab_component_list,
            |this, _, event: &SearchableListEvent<&'static str>, cx| {
                if let SearchableListEvent::Select(class_name) = event {
                    this.add_prefab_component(class_name.to_string());
                    this.show_add_component_dialog = false;
                    cx.notify();
                }
            },
        )
        .detach();

        let implemented_trait_picker = cx.new(|cx| {
            SearchableList::new(
                window,
                cx,
                Vec::<crate::features::traits::TraitAssetSummary>::new(),
                |asset| format!("{} · {} · {}", asset.display_name, asset.name, asset.path),
            )
            .with_empty_text("No additional traits are available for this Blueprint")
            .with_max_width(px(380.0))
            .with_max_height(px(360.0))
            .with_icon_getter(|_| ui::IconName::Code)
        });
        cx.subscribe_in(
            &implemented_trait_picker,
            window,
            |editor,
             _picker,
             event: &SearchableListEvent<crate::features::traits::TraitAssetSummary>,
             window,
             cx| {
                if let SearchableListEvent::Select(asset) = event {
                    editor.add_implemented_trait(&asset.path, window, cx);
                }
            },
        )
        .detach();

        let mut trait_events = engine_fs::subscribe();
        cx.spawn(async move |this, cx| {
            while let Ok(event) = trait_events.recv().await {
                let path = event.path;
                let _ = this.update(cx, |editor, cx| {
                    editor.refresh_for_trait_asset_event(&path, cx);
                });
            }
        })
        .detach();

        let rename_input: Entity<InputState> =
            cx.new(|cx| InputState::new(window, cx).placeholder("Rename..."));
        // Commit rename on blur or Enter
        let sub_input = rename_input.clone();
        cx.subscribe_in(
            &rename_input,
            window,
            move |this, input, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    if let Some(target) = this.renaming_target.take() {
                        let new_name = input.read(cx).text().to_string().trim().to_string();
                        if !new_name.is_empty() {
                            match target {
                                RenameTarget::Event(uid) => {
                                    this.rename_event_def(&uid, new_name);
                                    this.sync_all_events(window, cx);
                                }
                                RenameTarget::Macro(id) => {
                                    this.rename_local_macro(&id, new_name, cx);
                                }
                            }
                        }
                        cx.notify();
                    }
                }
            },
        )
        .detach();

        // ── Find panel search input ────────────────────────────────────────────
        let find_search_input: Entity<InputState> =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search nodes…"));
        cx.subscribe_in(
            &find_search_input,
            window,
            move |this, input, _event: &InputEvent, _window, cx| {
                this.find_search_query = input.read(cx).text().to_string();
                cx.notify();
            },
        )
        .detach();

        // Script errors from Play-in-Editor select the failing node.
        Self::watch_script_problems(cx);

        Self {
            focus_handle: cx.focus_handle(),
            blueprint_metadata: graphy_blueprint::BlueprintMetadata::default(),
            project_root: None,
            implemented_trait_catalog: Vec::new(),
            implemented_trait_catalog_error: None,
            implemented_trait_catalog_loaded: false,
            implemented_trait_catalog_loading: false,
            implemented_trait_catalog_refresh_pending: false,
            implemented_trait_status: None,
            implemented_trait_picker,
            workspace: None, // Will be initialized in render
            find_panel: None,
            current_class_path: None,
            tab_title: None,
            file_loading: false,
            load_error: None,
            class_variables: Vec::new(),
            selected_variable: None,
            is_creating_variable: false,
            variable_name_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("Variable name...")),
            variable_type_dropdown: cx
                .new(|cx| ui::dropdown::DropdownState::new(Vec::new(), None, window, cx)),
            dragging_variable: None,
            variable_drop_menu_position: None,
            prefab_asset: PrefabAsset::new("Prefab"),
            prefab_component_list,
            show_add_component_dialog: false,
            prefab_property_state: ui_common::reflected_properties_panel::PropertyStateManager::new(
            ),
            prefab_collapsed_categories: HashSet::new(),
            prefab_expanded_categories: HashSet::new(),
            selected_prefab_component: None,
            profile,
            subscriptions: Vec::new(),
            compilation_status: CompilationStatus::default(),
            compilation_history: Vec::new(),
            validation_problems: Vec::new(),
            compile_mode: crate::core::types::CompileMode::default(),
            compiler_output_scroll_handle: VirtualListScrollHandle::new(),
            compiler_output_scrollbar_state: ScrollbarState::default(),
            find_search_input,
            find_search_query: String::new(),
            find_output_scroll_handle: VirtualListScrollHandle::new(),
            find_output_scrollbar_state: ScrollbarState::default(),
            library_manager: {
                let mut lib_manager = LibraryManager::default();
                if let Err(e) = lib_manager.load_all_libraries(crate::io::libraries::load_directory)
                {
                    eprintln!("Failed to load sub-graph libraries: {}", e);
                }
                lib_manager
            },
            subgraphs: Vec::new(),
            selected_macro: None,
            local_event_defs: Vec::new(),
            component_event_metadata: cx
                .try_global::<plugin_editor_api::ComponentEventCatalog>()
                .map(|catalog| catalog.events.clone())
                .unwrap_or_default(),
            selected_event: None,
            renaming_target: None,
            rename_input,
            profile_data: Default::default(),
            open_tabs: vec![GraphTab::new_main(main_doc)],
            active_tab_index: 0,
            graph_panels: Vec::new(),
            graph_workspace_tabs_dirty: true,
            show_debug_overlay: true,
            show_minimap: true,
            show_graph_controls: true,
            wire_active_test_mode: false,
            wire_hidden_test_mode: false,
            running_nodes: HashSet::new(),
            graph_anim_start: std::time::Instant::now(),
            popup_palette_graph_pos: None,
            quick_palette_open: false,
            quick_palette_focus_pending: false,
            quick_palette_connection_source: None,
            quick_palette_screen_pos: Point::default(),
            quick_palette_view,
            hovered_pin_tooltip: None,
            hovered_pin_tooltip_pos: None,
            dragging_tab: None,
            is_dirty: false,
            is_saving: false,
            undo_service: None,
            breakpoints: HashSet::new(),
            debug_session: None,
            dragging_macro: None,
            macro_pin_add_mode: None,
            macro_pin_name_inputs: HashMap::new(),
            macro_pin_type_inputs: HashMap::new(),
            event_field_name_inputs: HashMap::new(),
            event_field_type_inputs: HashMap::new(),
        }
    }

    /// A small demonstration graph: an event wired to a print node.
    fn create_sample_graph() -> TabDoc {
        use graphy_gpui::GraphProfile as _;
        let profile = crate::canvas_profile::BlueprintProfile::blueprint(Default::default());
        let mut graph = graphy::GraphDescription::new("EventGraph");
        let begin = profile.create_node("begin_play", "begin", graphy::Position::new(100.0, 200.0));
        let print = profile.create_node("print_string", "print", graphy::Position::new(420.0, 200.0));
        if let (Some(begin), Some(print)) = (begin, print) {
            let from = begin.outputs.iter().find(|p| p.data_type().is_execution()).map(|p| p.id.clone());
            let to = print.inputs.iter().find(|p| p.data_type().is_execution()).map(|p| p.id.clone());
            graph.add_node(begin);
            graph.add_node(print);
            if let (Some(from), Some(to)) = (from, to) {
                graph.add_connection(graphy::Connection::execution("begin", from, "print", to));
            }
        }
        TabDoc::new(graph, Default::default())
    }

    /// Replace the current runtime execution set used by node highlighting.
    pub fn set_running_nodes<I, S>(&mut self, node_ids: I, cx: &mut Context<Self>)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let ids: Vec<String> = node_ids.into_iter().map(|id| id.as_ref().to_string()).collect();
        for (_, canvas) in self.graph_panels.clone() {
            canvas.update(cx, |canvas, cx| canvas.set_running_nodes(ids.iter(), cx));
        }
        cx.notify();
    }

    /// Mark/unmark a single node as executing.
    pub fn set_node_running(
        &mut self,
        node_id: impl AsRef<str>,
        running: bool,
        cx: &mut Context<Self>,
    ) {
        let node_id = node_id.as_ref().to_string();
        for (_, canvas) in self.graph_panels.clone() {
            canvas.update(cx, |canvas, cx| {
                let mut ids: std::collections::HashSet<String> = canvas
                    .profile_data
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .running
                    .clone();
                if running {
                    ids.insert(node_id.clone());
                } else {
                    ids.remove(&node_id);
                }
                canvas.set_running_nodes(ids, cx);
            });
        }
        cx.notify();
    }

    /// Clear all runtime execution highlights.
    pub fn clear_running_nodes(&mut self, cx: &mut Context<Self>) {
        self.set_running_nodes(std::iter::empty::<&str>(), cx);
    }

    /// Get focus handle
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    /// Adds a node of the profile's catalogue to the active graph, at the centre of
    /// the view, with `properties` set, and lets the profile bring its pins and title
    /// in line with them. How a profile's own panels put a node on the canvas.
    pub fn add_profile_node(
        &mut self,
        definition_id: &str,
        properties: Vec<(String, String)>,
        cx: &mut Context<Self>,
    ) {
        let Some(canvas) = self.active_canvas().cloned() else {
            return;
        };
        canvas.update(cx, |canvas, cx| {
            canvas.add_profile_node(definition_id, properties, cx)
        });
    }

    /// Arranges the active graph with the profile's layout (the toolbar's Auto
    /// Layout button).
    pub fn auto_layout(&mut self, cx: &mut Context<Self>) {
        let Some(profile) = self.profile.clone() else {
            return;
        };
        let Some(canvas) = self.active_canvas().cloned() else {
            return;
        };
        canvas.update(cx, |canvas, cx| {
            let commands = profile.auto_layout(canvas.graph(cx));
            let _ = canvas.apply("Auto layout", commands, cx);
        });
    }

    pub fn active_canvas(
        &self,
    ) -> Option<&Entity<crate::editor::graph_canvas_panel::GraphCanvasPanel>> {
        let tab_id = self.open_tabs.get(self.active_tab_index)?.id.as_str();
        self.graph_panels
            .iter()
            .find(|(id, _)| id == tab_id)
            .map(|(_, entity)| entity)
    }

    /// Clear all sidebar selections so the Properties panel can switch modes.
    /// Keeps `selected_*` fields that match `keep` (bitmask).
    pub fn clear_sidebar_selections(
        &mut self,
        keep_variable: bool,
        keep_macro: bool,
        keep_event: bool,
        keep_prefab: bool,
    ) {
        if !keep_variable {
            self.selected_variable = None;
        }
        if !keep_macro {
            self.selected_macro = None;
        }
        if !keep_event {
            self.selected_event = None;
        }
        if !keep_prefab {
            self.selected_prefab_component = None;
        }
    }

    /// Clear graph-node / comment selections on the active canvas.
    pub fn clear_graph_selections(&mut self, cx: &mut Context<Self>) {
        if let Some(canvas) = self.active_canvas().cloned() {
            canvas.update(cx, |canvas, cx| {
                canvas.canvas.update(cx, |c, cx| {
                    c.controller_mut().selection.clear();
                    cx.notify();
                });
            });
        }
    }

    // ============================================================================
    // Tab Operations
    // ============================================================================

    /// Takes every tab's canvas panel out of the dock (a new document is loading).
    pub(crate) fn remove_graph_panels(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let previous: Vec<_> = self.graph_panels.drain(..).collect();
        if let Some(workspace) = self.workspace.clone() {
            for (_, panel) in previous {
                workspace.update(cx, |workspace, cx| {
                    workspace.remove_panel(panel, DockPlacement::Center, window, cx);
                });
            }
        }
    }

    pub(crate) fn refresh_graph_workspace_tabs(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.graph_workspace_tabs_dirty {
            return;
        }

        let Some(workspace_entity) = self.workspace.clone() else {
            tracing::info!(
                ">>> refresh_graph_workspace_tabs: workspace is None (initial load before render) — deferring panel creation to render()",
            );
            return;
        };

        tracing::info!(
            ">>> refresh_graph_workspace_tabs: desired tabs={}, current graph_panels={}",
            self.open_tabs.len(),
            self.graph_panels.len(),
        );

        let desired_ids: Vec<String> = self.open_tabs.iter().map(|tab| tab.id.clone()).collect();

        let stale_panels: Vec<Entity<GraphCanvasPanel>> = self
            .graph_panels
            .iter()
            .filter(|(tab_id, _)| !desired_ids.contains(tab_id))
            .map(|(_, panel)| panel.clone())
            .collect();

        for panel in stale_panels {
            workspace_entity.update(cx, |workspace, cx| {
                workspace.remove_panel(panel.clone(), DockPlacement::Center, window, cx);
            });
        }

        self.graph_panels
            .retain(|(tab_id, _)| desired_ids.contains(tab_id));

        let editor_weak = cx.entity().downgrade();
        self.sync_profile_data();
        let pending: Vec<(String, String, bool, bool, Option<String>, TabDoc)> = self
            .open_tabs
            .iter_mut()
            .filter(|tab| !self.graph_panels.iter().any(|(id, _)| id == &tab.id))
            .filter_map(|tab| {
                tab.pending.take().map(|doc| {
                    (
                        tab.id.clone(),
                        tab.name.clone(),
                        tab.is_main,
                        tab.is_library_macro,
                        tab.library_id.clone(),
                        doc,
                    )
                })
            })
            .collect();
        for (tab_id, tab_name, tab_is_main, is_library_macro, library_id, doc) in pending {
            let ew = editor_weak.clone();
            let profile = self.profile.clone();
            let profile_data = self.profile_data.clone();
            let id = tab_id.clone();
            let panel = cx.new(|cx| {
                let mut canvas = GraphCanvasPanel::new(
                    ew,
                    tab_id.clone(),
                    tab_name,
                    tab_is_main,
                    doc,
                    profile,
                    profile_data,
                    window,
                    cx,
                );
                canvas.is_library_macro = is_library_macro;
                canvas.library_id = library_id;
                self.adopt_graph_history(&tab_id, &canvas.history);
                canvas
            });

            workspace_entity.update(cx, |workspace, cx| {
                workspace.add_panel(panel.clone(), DockPlacement::Center, window, cx);
            });

            self.graph_panels.push((id, panel));
        }

        self.activate_graph_workspace_tab(self.active_tab_index, window, cx);
        self.graph_workspace_tabs_dirty = false;
    }

    pub(crate) fn activate_graph_workspace_tab(
        &mut self,
        tab_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace_entity) = self.workspace.clone() else {
            return;
        };

        // `tab_index` is a position in `self.open_tabs` / `self.graph_panels` (matched by
        // tab id). The dock's `TabPanel` keeps its own internal panel order, which can
        // diverge from that (e.g. panels are appended to the dock in creation order, not
        // necessarily `open_tabs` order). Resolve the entity for this tab id first, then
        // ask the `TabPanel` for ITS index of that entity — otherwise `set_active_tab`
        // activates whatever panel happens to sit at the same position, leaving the user
        // looking at (and clicking into) a different canvas than `active_canvas()` returns.
        let Some(tab_id) = self.open_tabs.get(tab_index).map(|t| t.id.clone()) else {
            return;
        };
        let Some(panel_entity_id) = self
            .graph_panels
            .iter()
            .find(|(id, _)| id == &tab_id)
            .map(|(_, panel)| panel.entity_id())
        else {
            return;
        };

        workspace_entity.update(cx, |workspace, cx| {
            workspace.dock_area().update(cx, |dock_area, cx| {
                fn activate_tab_item(
                    item: &mut DockItem,
                    panel_entity_id: EntityId,
                    window: &mut Window,
                    cx: &mut App,
                ) -> bool {
                    match item {
                        DockItem::Tabs { view, .. } => {
                            let found = view.update(cx, |tab_panel, cx| {
                                if let Some(ix) =
                                    tab_panel.index_of_panel_by_entity_id(panel_entity_id)
                                {
                                    tab_panel.set_active_tab(ix, window, cx);
                                    true
                                } else {
                                    false
                                }
                            });
                            found
                        }
                        DockItem::Split { items, .. } => {
                            for child in items.iter_mut() {
                                if activate_tab_item(child, panel_entity_id, window, cx) {
                                    return true;
                                }
                            }
                            false
                        }
                        _ => false,
                    }
                }

                let _ = activate_tab_item(dock_area.items_mut(), panel_entity_id, window, cx);
            });
        });
    }

    /// Activate the Find dock tab and put keyboard focus in its search field.
    pub(crate) fn focus_find_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(workspace), Some(find_panel)) = (self.workspace.clone(), self.find_panel.clone())
        else {
            return;
        };
        let panel_id = find_panel.entity_id();

        workspace.update(cx, |workspace, cx| {
            workspace.dock_area().update(cx, |dock_area, cx| {
                fn activate(
                    item: &mut DockItem,
                    panel_id: EntityId,
                    window: &mut Window,
                    cx: &mut App,
                ) -> bool {
                    match item {
                        DockItem::Tabs { view, .. } => view.update(cx, |tabs, cx| {
                            let Some(index) = tabs.index_of_panel_by_entity_id(panel_id) else {
                                return false;
                            };
                            tabs.set_active_tab(index, window, cx);
                            true
                        }),
                        DockItem::Split { items, .. } => items
                            .iter_mut()
                            .any(|child| activate(child, panel_id, window, cx)),
                        _ => false,
                    }
                }

                let _ = activate(dock_area.items_mut(), panel_id, window, cx);
            });
        });

        self.find_search_input
            .read(cx)
            .focus_handle(cx)
            .focus(window, cx);
    }

    /// Switch to a different tab.
    pub fn switch_to_tab(&mut self, tab_index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if tab_index < self.open_tabs.len() && tab_index != self.active_tab_index {
            self.active_tab_index = tab_index;
            self.activate_graph_workspace_tab(tab_index, window, cx);
            cx.notify();
        }
    }

    /// Open a macro tab by macro ID, or switch to it if already open
    pub fn open_macro_tab(&mut self, macro_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(macro_name) = self
            .subgraphs
            .iter()
            .find(|subgraph| subgraph.id == macro_id)
            .map(|subgraph| subgraph.name.clone())
        else {
            return;
        };
        self.open_local_macro(macro_id.to_string(), macro_name, window, cx);
    }

    /// Publishes subgraph, library and event data to the canvas profile.
    pub fn sync_profile_data(&self) {
        let mut data = self.profile_data.write().unwrap_or_else(|e| e.into_inner());
        data.subgraphs = self.subgraphs.clone();
        data.library = self.library_manager.clone();
        data.event_names = self
            .local_event_defs
            .iter()
            .map(|def| (def.uid.clone(), def.name.clone()))
            .collect();
    }

    /// Re-presents every canvas after subgraph or event definitions changed.
    pub fn refresh_canvases(&mut self, cx: &mut Context<Self>) {
        self.sync_profile_data();
        for (_, canvas) in self.graph_panels.clone() {
            canvas.update(cx, |canvas, cx| canvas.refresh_presentation(cx));
        }
    }

    /// An edit happened on the canvas of tab `tab_id`.
    pub(crate) fn on_graph_edited(&mut self, tab_id: &str, cx: &mut Context<Self>) {
        if let Some(tab) = self.open_tabs.iter_mut().find(|tab| tab.id == tab_id) {
            tab.is_dirty = true;
        }
        self.is_dirty = true;
        cx.notify();
    }

    // ============================================================================
    // Menu Operations
    // ============================================================================

    /// Show node picker at graph position
    pub fn show_node_picker(
        &mut self,
        graph_pos: Point<f32>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Emit event to request node picker from global palette
        cx.emit(ShowNodePickerRequest {
            graph_position: graph_pos,
        });
    }

    // ============================================================================
    // File I/O Operations
    // ============================================================================

    /// Load blueprint from file.
    /// Delegates to `load_from_path` which uses the authoritative format/legacy
    /// pipeline and the clean `load_blueprint_asset` loader.
    pub fn load_blueprint(
        &mut self,
        file_path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let path = std::path::PathBuf::from(file_path);
        tracing::info!(
            ">>> load_blueprint: file_path={:?}, current_class_path={:?}, open_tabs={}, graph_panels={}",
            file_path, self.current_class_path, self.open_tabs.len(), self.graph_panels.len(),
        );

        self.load_from_path(&path, window, cx)?;

        // Reload library manager so any library macros are available.
        self.library_manager = graphy_blueprint::LibraryManager::default();
        if let Err(e) = self
            .library_manager
            .load_all_libraries(crate::io::libraries::load_directory)
        {
            eprintln!("Failed to reload sub-graph libraries: {}", e);
        }

        Ok(())
    }

    /// Load local macros from macros.json
    fn load_subgraphs(&mut self, class_path: &std::path::Path) -> Result<(), String> {
        let macros_file = class_path.join("macros.json");
        if !macros_file.exists() {
            self.subgraphs
                .retain(|subgraph| subgraph.kind == graphy_blueprint::SubGraphKind::Collapsed);
            return Ok(());
        }

        let content = std::fs::read_to_string(&macros_file)
            .map_err(|e| format!("Failed to read macros.json: {}", e))?;
        let macros: Vec<graphy_blueprint::SubGraph> = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse macros.json: {}", e))?;

        self.subgraphs
            .retain(|subgraph| subgraph.kind == graphy_blueprint::SubGraphKind::Collapsed);
        self.subgraphs.extend(
            macros
                .into_iter()
                .filter(|subgraph| subgraph.kind == graphy_blueprint::SubGraphKind::Macro),
        );
        println!(
            "📂 Loaded {} local macros from macros.json",
            self.subgraphs
                .iter()
                .filter(|subgraph| subgraph.kind == graphy_blueprint::SubGraphKind::Macro)
                .count()
        );
        Ok(())
    }

}
