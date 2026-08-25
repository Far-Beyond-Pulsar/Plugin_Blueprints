//! Core panel struct and initialization
//!
//! This module contains the main `BlueprintEditorPanel` struct definition,
//! constructors, and basic accessors.

use gpui::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use ui::{
    input::{InputEvent, InputState}, resizable::ResizableState,
    scroll::ScrollbarState, VirtualListScrollHandle,
};

use super::tabs::GraphTab;
use crate::core::{events::*, graph::*, types::*};
use crate::editor::workspace_panels::GraphCanvasPanel;
use crate::features::connections::operations::ConnectionDrag;

use crate::features::prefabs::PrefabAsset;
use ui::dropdown::{SearchableList, SearchableListEvent};
use crate::features::variables::ClassVariable;
use crate::ui_components::palette_view::NodePaletteView;
use ui::dock::{DockItem, DockPlacement};
use ui::graph::{LibraryManager, SubGraphDefinition};

/// Which item is being renamed inline in a hierarchy panel.
#[derive(Clone, Debug, PartialEq)]
pub enum RenameTarget {
    Event(String),
    Macro(String),
}

/// Main Blueprint Editor Panel struct
pub struct BlueprintEditorPanel {
    pub(super) focus_handle: FocusHandle,
    pub graph: BlueprintGraph,

    // Workspace with full docking support
    pub(super) workspace: Option<Entity<ui::workspace::Workspace>>,

    // File I/O
    pub current_class_path: Option<std::path::PathBuf>,
    pub tab_title: Option<String>,

    // Node drag state
    pub dragging_node: Option<String>,
    pub drag_offset: Point<f32>,
    pub initial_drag_positions: HashMap<String, Point<f32>>,
    pub initial_comment_drag_positions: HashMap<String, Point<f32>>,
    pub node_clipboard: Option<BlueprintNode>,
    /// Node click that *may* become a drag once the mouse moves past the threshold.
    /// Set on mouse-down; converted to a real drag in mouse-move.
    pub pending_drag_node: Option<String>,
    /// Canvas-space position where the pending drag mouse-down landed.
    pub pending_drag_start: Option<Point<f32>>,
    /// Pixels of canvas movement required to commit a drag (avoids phantom moves on clicks).
    pub drag_commit_threshold: f32,

    // Connection drag state
    pub dragging_connection: Option<ConnectionDrag>,

    // Panning state
    pub is_panning: bool,
    pub pan_start: Point<f32>,
    pub pan_start_offset: Point<f32>,

    // Selection state
    pub selection_start: Option<Point<f32>>,
    pub selection_end: Option<Point<f32>>,
    pub last_mouse_pos: Option<Point<f32>>,

    // Right-click gesture detection
    pub right_click_start: Option<Point<f32>>,
    pub right_click_threshold: f32,

    // Double-click for reroute nodes
    pub last_click_time: Option<std::time::Instant>,
    pub last_click_pos: Option<Point<f32>>,

    // Coordinate conversion
    /// Window-space origin of the single bp canvas element, captured each frame during paint.
    /// Event handlers subtract this to get canvas-relative (= "screen") coordinates.
    pub canvas_origin: Rc<RefCell<Point<f32>>>,
    pub graph_element_bounds: Option<Bounds<Pixels>>,
    pub graph_element_bounds_by_view: HashMap<String, Bounds<Pixels>>,
    pub interaction_view_id: Option<String>,
    pub interaction_state_by_view: HashMap<String, GraphInteractionState>,

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

    // Comment system
    pub dragging_comment: Option<String>,
    pub resizing_comment: Option<(String, ResizeHandle)>,
    pub resizing_comment_start: Option<(Point<f32>, Size<f32>)>,
    pub editing_comment: Option<String>,
    pub comment_text_input: Entity<InputState>,
    pub comment_color_bindings_dirty: bool,

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
    pub local_macros: Vec<SubGraphDefinition>,
    pub selected_macro: Option<usize>,
    // Event system (mirrors macro storage pattern)
    pub local_event_defs: Vec<crate::core::graph::EventDefinition>,
    pub selected_event: Option<usize>,

    // Rename state — shared across event/macro/variable panels
    pub renaming_target: Option<RenameTarget>,
    pub rename_input: Entity<InputState>,

    // Tab system
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
        Option<crate::features::connections::operations::ConnectionDrag>,
    /// Window-space position where the user right-clicked (used to anchor the overlay).
    pub quick_palette_screen_pos: Point<Pixels>,
    /// The shared palette view rendered inside the overlay.
    pub quick_palette_view: Entity<NodePaletteView>,

    // Pin hover tooltip state
    pub hovered_pin_tooltip: Option<String>,
    pub hovered_pin_tooltip_pos: Option<Point<Pixels>>,

    // Sidebar tab states
    pub left_top_tab: usize,    // 0=Variables, 1=Functions, 2=Macros, 3=Events
    pub left_bottom_tab: usize, // 0=Library, 1=Compiler
    pub right_tab: usize,       // 0=Details, 1=Prefabs, 2=Palette

    // Tab drag state
    pub dragging_tab: Option<TabDragInfo>,

    pub is_dirty: bool, // Whether there are unsaved changes

    // Undo/redo system
    pub undo_manager: crate::features::undo::UndoManager,

    // ── GPU renderer ──────────────────────────────────────────────────────────
    pub bp_renderer: crate::rendering::gpu::BpRenderer,
    pub bp_surface: Option<gpui::WgpuSurfaceHandle>,

    // ── Context menus (shown as GPUI overlays above the GPU surface) ──────────
    /// Right-clicked node: (node_id, window-space position for anchoring)
    pub node_context_menu: Option<(String, Point<Pixels>)>,
    /// Right-clicked pin: (node_id, pin_id, window-space position)
    pub pin_context_menu: Option<(String, String, Point<Pixels>)>,

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

#[derive(Clone, Debug)]
pub struct GraphInteractionState {
    pub dragging_node: Option<String>,
    pub pending_drag_node: Option<String>,
    pub pending_drag_start: Option<Point<f32>>,
    pub drag_offset: Point<f32>,
    pub initial_drag_positions: HashMap<String, Point<f32>>,
    pub initial_comment_drag_positions: HashMap<String, Point<f32>>,
    pub dragging_connection: Option<ConnectionDrag>,
    pub is_panning: bool,
    pub pan_start: Point<f32>,
    pub pan_start_offset: Point<f32>,
    pub selection_start: Option<Point<f32>>,
    pub selection_end: Option<Point<f32>>,
    pub last_mouse_pos: Option<Point<f32>>,
    pub right_click_start: Option<Point<f32>>,
    pub last_click_time: Option<std::time::Instant>,
    pub last_click_pos: Option<Point<f32>>,
    pub dragging_variable: Option<crate::features::variables::VariableDrag>,
    pub variable_drop_menu_position: Option<Point<f32>>,
    pub dragging_comment: Option<String>,
    pub resizing_comment: Option<(String, ResizeHandle)>,
    pub resizing_comment_start: Option<(Point<f32>, Size<f32>)>,
    pub editing_comment: Option<String>,
}

impl Default for GraphInteractionState {
    fn default() -> Self {
        Self {
            dragging_node: None,
            pending_drag_node: None,
            pending_drag_start: None,
            drag_offset: Point::new(0.0, 0.0),
            initial_drag_positions: HashMap::new(),
            initial_comment_drag_positions: HashMap::new(),
            dragging_connection: None,
            is_panning: false,
            pan_start: Point::new(0.0, 0.0),
            pan_start_offset: Point::new(0.0, 0.0),
            selection_start: None,
            selection_end: None,
            last_mouse_pos: None,
            right_click_start: None,
            last_click_time: None,
            last_click_pos: None,
            dragging_variable: None,
            variable_drop_menu_position: None,
            dragging_comment: None,
            resizing_comment: None,
            resizing_comment_start: None,
            editing_comment: None,
        }
    }
}

/// Resize handle for comment boxes
#[derive(Clone, Debug, PartialEq)]
pub enum ResizeHandle {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Top,
    Bottom,
    Left,
    Right,
}

impl BlueprintEditorPanel {
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
        tracing::info!(
            ">>> new_with_path: after new_internal: open_tabs={}, self.graph.nodes={}, graph_panels={}, current_class_path={:?}",
            panel.open_tabs.len(),
            panel.graph.nodes.len(),
            panel.graph_panels.len(),
            panel.current_class_path,
        );

        // Blueprint classes are folders containing graph_save.json
        let graph_file = file_path.join("graph_save.json");

        // Load the blueprint file
        if let Err(e) = panel.load_blueprint(graph_file.to_str().unwrap(), window, cx) {
            log::error!("Failed to load blueprint: {}", e);
            return Err(e.into());
        }

        if let Err(e) = panel.load_prefab_sidecar() {
            log::warn!("Failed to load prefab sidecar: {}", e);
        }

        tracing::info!(
            ">>> new_with_path: loaded. open_tabs={}, self.graph.nodes={}, graph_panels={}, current_class_path={:?}",
            panel.open_tabs.len(),
            panel.graph.nodes.len(),
            panel.graph_panels.len(),
            panel.current_class_path,
        );

        log::info!("Loaded blueprint from {:?}", file_path);
        Ok(panel)
    }

    /// Create a new blueprint editor panel with a file to load
    pub fn new_with_file(
        file_path: std::path::PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut panel = Self::new_internal(Some(file_path.clone()), window, cx);

        // Try to load the blueprint file
        if let Err(e) = panel.load_blueprint(file_path.to_str().unwrap(), window, cx) {
            eprintln!("Failed to load blueprint: {}", e);
        } else {
            if let Err(e) = panel.load_prefab_sidecar() {
                log::warn!("Failed to load prefab sidecar: {}", e);
            }
            println!("Loaded blueprint from {:?}", file_path);
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

        // Create demo graph with sample nodes (only if no file is being loaded)
        let main_graph = if project_path.is_some() {
            // Empty graph - will be loaded from file
            BlueprintGraph {
                nodes: Vec::new(),
                connections: Vec::new(),
                comments: Vec::new(),
                selected_nodes: Vec::new(),
                selected_comments: Vec::new(),
                zoom_level: 1.0,
                pan_offset: Point::new(0.0, 0.0),
                virtualization_stats: VirtualizationStats::default(),
            }
        } else {
            // No file to load - create sample graph
            Self::create_sample_graph()
        };

        let editor_weak = cx.entity().downgrade();
        let quick_palette_view = cx.new(|cx| NodePaletteView::new(editor_weak, window, cx));
        let mut engine_classes = pulsar_reflection::REGISTRY.get_class_names();
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

        let rename_input: Entity<InputState> =
            cx.new(|cx| InputState::new(window, cx).placeholder("Rename..."));
        // Commit rename on blur or Enter
        let sub_input = rename_input.clone();
        cx.subscribe_in(&rename_input, window, move |this, input, event: &InputEvent, window, cx| {
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
        })
        .detach();

        // ── Find panel search input ────────────────────────────────────────────
        let find_search_input: Entity<InputState> =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search nodes…"));
        cx.subscribe_in(&find_search_input, window, move |this, input, _event: &InputEvent, _window, cx| {
            this.find_search_query = input.read(cx).text().to_string();
            cx.notify();
        })
        .detach();

        Self {
            focus_handle: cx.focus_handle(),
            graph: main_graph.clone(),
            workspace: None, // Will be initialized in render
            current_class_path: None,
            tab_title: None,
            dragging_node: None,
            drag_offset: Point::new(0.0, 0.0),
            initial_drag_positions: HashMap::new(),
            initial_comment_drag_positions: HashMap::new(),
            node_clipboard: None,
            pending_drag_node: None,
            pending_drag_start: None,
            drag_commit_threshold: 5.0,
            dragging_connection: None,
            is_panning: false,
            pan_start: Point::new(0.0, 0.0),
            pan_start_offset: Point::new(0.0, 0.0),
            selection_start: None,
            selection_end: None,
            last_mouse_pos: None,
            right_click_start: None,
            right_click_threshold: 5.0,
            last_click_time: None,
            last_click_pos: None,
            canvas_origin: Rc::new(RefCell::new(Point::new(0.0, 0.0))),
            graph_element_bounds: None,
            graph_element_bounds_by_view: HashMap::new(),
            interaction_view_id: None,
            interaction_state_by_view: HashMap::new(),
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
            prefab_property_state: ui_common::reflected_properties_panel::PropertyStateManager::new(),
            prefab_collapsed_categories: HashSet::new(),
            prefab_expanded_categories: HashSet::new(),
            selected_prefab_component: None,
            dragging_comment: None,
            resizing_comment: None,
            resizing_comment_start: None,
            editing_comment: None,
            comment_text_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("Comment text...")),
            comment_color_bindings_dirty: true,
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
                if let Err(e) = lib_manager.load_all_libraries() {
                    eprintln!("Failed to load sub-graph libraries: {}", e);
                }
                lib_manager
            },
            local_macros: Vec::new(),
            selected_macro: None,
            local_event_defs: Vec::new(),
            selected_event: None,
            renaming_target: None,
            rename_input,
            open_tabs: vec![GraphTab {
                id: "main".to_string(),
                name: "EventGraph".to_string(),
                graph: main_graph,
                is_main: true,
                is_dirty: false,
                is_library_macro: false,
                library_id: None,
            }],
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
            left_top_tab: 0,
            left_bottom_tab: 0,
            right_tab: 0,
            dragging_tab: None,
            is_dirty: false,
            undo_manager: crate::features::undo::UndoManager::new(),
            bp_renderer: crate::rendering::gpu::BpRenderer::new(),
            bp_surface: None,
            node_context_menu: None,
            pin_context_menu: None,
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

    /// Create a sample graph for demonstration - demonstrates all compiler features
    fn create_sample_graph() -> BlueprintGraph {
        use crate::core::types::*;
        use crate::core::types::PinDataType as GraphDataType;

        let mut nodes = Vec::new();

        // Main event node (defines pub fn main())
        nodes.push(BlueprintNode {
            id: "main_event".to_string(),
            definition_id: "main".to_string(),
            title: "Main".to_string(),
            icon: "Play".to_string(),
            node_type: NodeType::Event,
            position: Point::new(100.0, 200.0),
            size: Size::new(240.0, 60.0),
            inputs: vec![],
            outputs: vec![Pin {
                id: "Body".to_string(),
                name: "Body".to_string(),
                pin_type: PinType::Output,
                data_type: GraphDataType::from_type_str("execution"),
            }],
            properties: HashMap::new(),
            is_selected: false,
            description: "Entry point for the main function".to_string(),
            color: None,
        });

        // Pure node: add(2, 3)
        let mut add_props = HashMap::new();
        add_props.insert("a".to_string(), "2".to_string());
        add_props.insert("b".to_string(), "3".to_string());

        nodes.push(BlueprintNode {
            id: "add_node".to_string(),
            definition_id: "add".to_string(),
            title: "Add".to_string(),
            icon: "Plus".to_string(),
            node_type: NodeType::Math,
            position: Point::new(400.0, 80.0),
            size: Size::new(240.0, 80.0),
            inputs: vec![
                Pin {
                    id: "a".to_string(),
                    name: "A".to_string(),
                    pin_type: PinType::Input,
                    data_type: GraphDataType::from_type_str("i64"),
                },
                Pin {
                    id: "b".to_string(),
                    name: "B".to_string(),
                    pin_type: PinType::Input,
                    data_type: GraphDataType::from_type_str("i64"),
                },
            ],
            outputs: vec![Pin {
                id: "result".to_string(),
                name: "Result".to_string(),
                pin_type: PinType::Output,
                data_type: GraphDataType::from_type_str("i64"),
            }],
            properties: add_props,
            is_selected: false,
            description: "Adds two numbers: (2 + 3) = 5".to_string(),
            color: None,
        });

        // Control flow: branch
        nodes.push(BlueprintNode {
            id: "branch_node".to_string(),
            definition_id: "branch".to_string(),
            title: "Branch".to_string(),
            icon: "GitBranch".to_string(),
            node_type: NodeType::Logic,
            position: Point::new(400.0, 280.0),
            size: Size::new(240.0, 80.0),
            inputs: vec![
                Pin {
                    id: "exec".to_string(),
                    name: "".to_string(),
                    pin_type: PinType::Input,
                    data_type: GraphDataType::from_type_str("execution"),
                },
                Pin {
                    id: "condition".to_string(),
                    name: "Condition".to_string(),
                    pin_type: PinType::Input,
                    data_type: GraphDataType::from_type_str("bool"),
                },
            ],
            outputs: vec![
                Pin {
                    id: "True".to_string(),
                    name: "True".to_string(),
                    pin_type: PinType::Output,
                    data_type: GraphDataType::from_type_str("execution"),
                },
                Pin {
                    id: "False".to_string(),
                    name: "False".to_string(),
                    pin_type: PinType::Output,
                    data_type: GraphDataType::from_type_str("execution"),
                },
            ],
            properties: HashMap::new(),
            is_selected: false,
            description: "Branches execution based on a condition.".to_string(),
            color: None,
        });

        // Function node: print (true path)
        let mut print_true_props = HashMap::new();
        print_true_props.insert(
            "message".to_string(),
            "Result is greater than 3!".to_string(),
        );

        nodes.push(BlueprintNode {
            id: "print_true".to_string(),
            definition_id: "print_string".to_string(),
            title: "Print String".to_string(),
            icon: "Terminal".to_string(),
            node_type: NodeType::Logic,
            position: Point::new(680.0, 220.0),
            size: Size::new(260.0, 80.0),
            inputs: vec![
                Pin {
                    id: "exec".to_string(),
                    name: "".to_string(),
                    pin_type: PinType::Input,
                    data_type: GraphDataType::from_type_str("execution"),
                },
                Pin {
                    id: "message".to_string(),
                    name: "Message".to_string(),
                    pin_type: PinType::Input,
                    data_type: GraphDataType::from_type_str("String"),
                },
            ],
            outputs: vec![Pin {
                id: "exec_out".to_string(),
                name: "".to_string(),
                pin_type: PinType::Output,
                data_type: GraphDataType::from_type_str("execution"),
            }],
            properties: print_true_props,
            is_selected: false,
            description: "Prints success message.".to_string(),
            color: None,
        });

        // Function node: print (false path)
        let mut print_false_props = HashMap::new();
        print_false_props.insert("message".to_string(), "Result is 3 or less.".to_string());

        nodes.push(BlueprintNode {
            id: "print_false".to_string(),
            definition_id: "print_string".to_string(),
            title: "Print String".to_string(),
            icon: "Terminal".to_string(),
            node_type: NodeType::Logic,
            position: Point::new(680.0, 360.0),
            size: Size::new(260.0, 80.0),
            inputs: vec![
                Pin {
                    id: "exec".to_string(),
                    name: "".to_string(),
                    pin_type: PinType::Input,
                    data_type: GraphDataType::from_type_str("execution"),
                },
                Pin {
                    id: "message".to_string(),
                    name: "Message".to_string(),
                    pin_type: PinType::Input,
                    data_type: GraphDataType::from_type_str("String"),
                },
            ],
            outputs: vec![Pin {
                id: "exec_out".to_string(),
                name: "".to_string(),
                pin_type: PinType::Output,
                data_type: GraphDataType::from_type_str("execution"),
            }],
            properties: print_false_props,
            is_selected: false,
            description: "Prints alternative message.".to_string(),
            color: None,
        });

        // Pure node: greater than
        let mut gt_props = HashMap::new();
        gt_props.insert("b".to_string(), "3".to_string());

        nodes.push(BlueprintNode {
            id: "greater_node".to_string(),
            definition_id: "greater_than".to_string(),
            title: "Greater Than".to_string(),
            icon: "ChevronRight".to_string(),
            node_type: NodeType::Logic,
            position: Point::new(620.0, 80.0),
            size: Size::new(240.0, 80.0),
            inputs: vec![
                Pin {
                    id: "a".to_string(),
                    name: "A".to_string(),
                    pin_type: PinType::Input,
                    data_type: GraphDataType::from_type_str("i64"),
                },
                Pin {
                    id: "b".to_string(),
                    name: "B".to_string(),
                    pin_type: PinType::Input,
                    data_type: GraphDataType::from_type_str("i64"),
                },
            ],
            outputs: vec![Pin {
                id: "result".to_string(),
                name: "Result".to_string(),
                pin_type: PinType::Output,
                data_type: GraphDataType::from_type_str("bool"),
            }],
            properties: gt_props,
            is_selected: false,
            description: "Checks if A > B: result > 3?".to_string(),
            color: None,
        });

        let connections = vec![
            // Execution: main -> branch
            Connection {
                id: "conn_main_branch".to_string(),
                source_node: "main_event".to_string(),
                source_pin: "Body".to_string(),
                target_node: "branch_node".to_string(),
                target_pin: "exec".to_string(),
                connection_type: ui::graph::ConnectionType::Execution,
            },
            // Data: add -> greater_than
            Connection {
                id: "conn_add_gt".to_string(),
                source_node: "add_node".to_string(),
                source_pin: "result".to_string(),
                target_node: "greater_node".to_string(),
                target_pin: "a".to_string(),
                connection_type: ui::graph::ConnectionType::Data,
            },
            // Data: greater_than -> branch
            Connection {
                id: "conn_gt_branch".to_string(),
                source_node: "greater_node".to_string(),
                source_pin: "result".to_string(),
                target_node: "branch_node".to_string(),
                target_pin: "condition".to_string(),
                connection_type: ui::graph::ConnectionType::Data,
            },
            // Execution: branch(True) -> print_true
            Connection {
                id: "conn_branch_true".to_string(),
                source_node: "branch_node".to_string(),
                source_pin: "True".to_string(),
                target_node: "print_true".to_string(),
                target_pin: "exec".to_string(),
                connection_type: ui::graph::ConnectionType::Execution,
            },
            // Execution: branch(False) -> print_false
            Connection {
                id: "conn_branch_false".to_string(),
                source_node: "branch_node".to_string(),
                source_pin: "False".to_string(),
                target_node: "print_false".to_string(),
                target_pin: "exec".to_string(),
                connection_type: ui::graph::ConnectionType::Execution,
            },
        ];

        BlueprintGraph {
            nodes,
            connections,
            comments: vec![],
            selected_nodes: vec![],
            selected_comments: vec![],
            zoom_level: 1.0,
            pan_offset: Point::new(0.0, 0.0),
            virtualization_stats: VirtualizationStats::default(),
        }
    }

    /// Replace the current runtime execution set used by GPU debug rendering.
    pub fn set_running_nodes<I, S>(&mut self, node_ids: I, cx: &mut Context<Self>)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.running_nodes.clear();
        for id in node_ids {
            self.running_nodes.insert(id.as_ref().to_string());
        }
        cx.notify();
    }

    /// Mark/unmark a single node as executing.
    pub fn set_node_running(&mut self, node_id: impl AsRef<str>, running: bool, cx: &mut Context<Self>) {
        if running {
            self.running_nodes.insert(node_id.as_ref().to_string());
        } else {
            self.running_nodes.remove(node_id.as_ref());
        }
        cx.notify();
    }

    /// Clear all runtime execution highlights.
    pub fn clear_running_nodes(&mut self, cx: &mut Context<Self>) {
        self.running_nodes.clear();
        cx.notify();
    }

    /// Get focus handle
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    /// Return the active graph canvas entity, if one exists.
    pub fn active_canvas(&self) -> Option<&Entity<crate::editor::workspace_panels::GraphCanvasPanel>> {
        let tab_id = self.open_tabs.get(self.active_tab_index)?.id.as_str();
        self.graph_panels
            .iter()
            .find(|(id, _)| id == tab_id)
            .map(|(_, entity)| entity)
    }

    /// Clear all sidebar selections so the Properties panel can switch modes.
    /// Keeps `selected_*` fields that match `keep` (bitmask).
    pub fn clear_sidebar_selections(&mut self, keep_variable: bool, keep_macro: bool, keep_event: bool, keep_prefab: bool) {
        if !keep_variable { self.selected_variable = None; }
        if !keep_macro { self.selected_macro = None; }
        if !keep_event { self.selected_event = None; }
        if !keep_prefab { self.selected_prefab_component = None; }
    }

    /// Clear graph-node / comment selections on the active canvas.
    pub fn clear_graph_selections(&mut self, cx: &mut Context<Self>) {
        if let Some(canvas) = self.active_canvas().cloned() {
            canvas.update(cx, |canvas, cx| {
                canvas.graph.selected_nodes.clear();
                canvas.graph.selected_comments.clear();
                cx.notify();
            });
        }
    }

    fn capture_interaction_state(&self) -> GraphInteractionState {
        GraphInteractionState {
            dragging_node: self.dragging_node.clone(),
            pending_drag_node: self.pending_drag_node.clone(),
            pending_drag_start: self.pending_drag_start,
            drag_offset: self.drag_offset,
            initial_drag_positions: self.initial_drag_positions.clone(),
            initial_comment_drag_positions: self.initial_comment_drag_positions.clone(),
            dragging_connection: self.dragging_connection.clone(),
            is_panning: self.is_panning,
            pan_start: self.pan_start,
            pan_start_offset: self.pan_start_offset,
            selection_start: self.selection_start,
            selection_end: self.selection_end,
            last_mouse_pos: self.last_mouse_pos,
            right_click_start: self.right_click_start,
            last_click_time: self.last_click_time,
            last_click_pos: self.last_click_pos,
            dragging_variable: self.dragging_variable.clone(),
            variable_drop_menu_position: self.variable_drop_menu_position,
            dragging_comment: self.dragging_comment.clone(),
            resizing_comment: self.resizing_comment.clone(),
            resizing_comment_start: self.resizing_comment_start,
            editing_comment: self.editing_comment.clone(),
        }
    }

    fn apply_interaction_state(&mut self, state: GraphInteractionState) {
        self.dragging_node = state.dragging_node;
        self.drag_offset = state.drag_offset;
        self.initial_drag_positions = state.initial_drag_positions;
        self.initial_comment_drag_positions = state.initial_comment_drag_positions;
        self.dragging_connection = state.dragging_connection;
        self.is_panning = state.is_panning;
        self.pan_start = state.pan_start;
        self.pan_start_offset = state.pan_start_offset;
        self.selection_start = state.selection_start;
        self.selection_end = state.selection_end;
        self.last_mouse_pos = state.last_mouse_pos;
        self.right_click_start = state.right_click_start;
        self.last_click_time = state.last_click_time;
        self.last_click_pos = state.last_click_pos;
        self.dragging_variable = state.dragging_variable;
        self.variable_drop_menu_position = state.variable_drop_menu_position;
        self.dragging_comment = state.dragging_comment;
        self.resizing_comment = state.resizing_comment;
        self.resizing_comment_start = state.resizing_comment_start;
        self.editing_comment = state.editing_comment;
    }

    pub(crate) fn activate_interaction_view(&mut self, view_id: &str) {
        self.ensure_active_graph_panel_state(view_id);

        if self.interaction_view_id.as_deref() == Some(view_id) {
            return;
        }

        if let Some(previous_view) = self.interaction_view_id.clone() {
            self.interaction_state_by_view
                .insert(previous_view, self.capture_interaction_state());
        }

        let next_state = self
            .interaction_state_by_view
            .get(view_id)
            .cloned()
            .unwrap_or_default();

        self.apply_interaction_state(next_state);
        self.interaction_view_id = Some(view_id.to_string());
    }

    pub(crate) fn persist_active_interaction_state(&mut self) {
        if let Some(view_id) = self.interaction_view_id.clone() {
            self.interaction_state_by_view
                .insert(view_id, self.capture_interaction_state());
        }
    }

    pub(crate) fn clear_interaction_view_owner(&mut self) {
        self.persist_active_interaction_state();
        self.interaction_view_id = None;
    }

    // ============================================================================
    // Tab Operations
    // ============================================================================

    pub(crate) fn ensure_active_graph_panel_state(&mut self, tab_id: &str) {
        if let Some(tab_index) = self.open_tabs.iter().position(|tab| tab.id == tab_id) {
            if tab_index != self.active_tab_index {
                self.sync_graph_to_active_tab();
                self.active_tab_index = tab_index;
                self.load_active_tab_graph();
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
        for tab in &self.open_tabs {
            if self
                .graph_panels
                .iter()
                .any(|(tab_id, _)| tab_id == &tab.id)
            {
                continue;
            }

            let tab_id = tab.id.clone();
            let tab_name = tab.name.clone();
            let tab_is_main = tab.is_main;
            let tab_graph = tab.graph.clone();
            let ew = editor_weak.clone();
            let panel = cx.new(|cx| {
                GraphCanvasPanel::new(ew, tab_id.clone(), tab_name, tab_is_main, tab_graph, window, cx)
            });

            workspace_entity.update(cx, |workspace, cx| {
                workspace.add_panel(panel.clone(), DockPlacement::Center, window, cx);
            });

            self.graph_panels.push((tab.id.clone(), panel));
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

    /// Switch to a different tab, flushing the current canvas first.
    pub fn switch_to_tab(&mut self, tab_index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if tab_index < self.open_tabs.len() && tab_index != self.active_tab_index {
            tracing::info!(
                ">>> switch_to_tab: from {} ({} nodes) to {} ({} nodes), graph_panels={}",
                self.active_tab_index,
                self.graph.nodes.len(),
                tab_index,
                self.open_tabs.get(tab_index).map(|t| t.graph.nodes.len()).unwrap_or(0),
                self.graph_panels.len(),
            );

            // Flush the current active canvas into its tab snapshot before leaving.
            let active_tab_id = self.open_tabs.get(self.active_tab_index).map(|t| t.id.clone());
            if let Some(tab_id) = active_tab_id {
                if let Some((_, canvas)) = self.graph_panels.iter().find(|(id, _)| id == &tab_id) {
                    let live = canvas.read(cx).graph.clone();
                    tracing::info!(
                        ">>> switch_to_tab: flushing canvas {} ({} nodes) to tab",
                        tab_id,
                        live.nodes.len(),
                    );
                    self.graph = live.clone();
                    if let Some(tab) = self.open_tabs.get_mut(self.active_tab_index) {
                        tab.graph = live;
                    }
                }
            }
            self.active_tab_index = tab_index;
            // Update self.graph shadow from the new active tab.
            if let Some(tab) = self.open_tabs.get(tab_index) {
                tracing::info!(
                    ">>> switch_to_tab: loading tab {} ({} nodes) into self.graph",
                    tab.id,
                    tab.graph.nodes.len(),
                );
                self.graph = tab.graph.clone();
                self.comment_color_bindings_dirty = true;
            }
            self.activate_graph_workspace_tab(tab_index, window, cx);
            cx.notify();
        }
    }

    /// Open a macro tab by macro ID, or switch to it if already open
    pub fn open_macro_tab(&mut self, macro_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        tracing::info!(
            ">>> open_macro_tab: macro_id={}, active_tab_index={}, open_tabs={}",
            macro_id,
            self.active_tab_index,
            self.open_tabs.len(),
        );

        // Check if tab is already open
        if let Some(tab_index) = self.open_tabs.iter().position(|tab| tab.id == macro_id) {
            tracing::info!(">>> open_macro_tab: tab already open at index {}, switching", tab_index);
            self.switch_to_tab(tab_index, window, cx);
            return;
        }

        // Find the macro definition
        let macro_data = self
            .local_macros
            .iter()
            .find(|m| m.id == macro_id)
            .map(|m| (m.name.clone(), m.graph.clone()));

        if let Some((macro_name, macro_graph)) = macro_data {
            if let Ok(blueprint_graph) =
                self.convert_graph_description_to_blueprint(&macro_graph, window, cx)
            {
                // Flush the current active canvas into its tab before switching.
                let active_tab_id = self.open_tabs.get(self.active_tab_index).map(|t| t.id.clone());
                if let Some(tab_id) = active_tab_id {
                    if let Some((_, canvas)) = self.graph_panels.iter().find(|(id, _)| id == &tab_id) {
                        let live = canvas.read(cx).graph.clone();
                        tracing::info!(
                            ">>> open_macro_tab: flushing canvas {} ({} nodes) to tab",
                            tab_id,
                            live.nodes.len(),
                        );
                        self.graph = live.clone();
                        if let Some(tab) = self.open_tabs.get_mut(self.active_tab_index) {
                            tab.graph = live;
                        }
                    }
                }

                // Create new tab seeded from the saved macro graph.
                tracing::info!(
                    ">>> open_macro_tab: creating new tab for macro {}, blueprint has {} nodes",
                    macro_id,
                    blueprint_graph.nodes.len(),
                );
                self.open_tabs.push(GraphTab {
                    id: macro_id.to_string(),
                    name: macro_name,
                    graph: blueprint_graph.clone(),
                    is_main: false,
                    is_dirty: false,
                    is_library_macro: false,
                    library_id: None,
                });

                let new_tab_index = self.open_tabs.len() - 1;
                self.active_tab_index = new_tab_index;
                self.graph = blueprint_graph;
                tracing::info!(
                    ">>> open_macro_tab: switched to new tab {} at index {}, self.graph.nodes={}",
                    macro_id,
                    new_tab_index,
                    self.graph.nodes.len(),
                );
                self.graph_workspace_tabs_dirty = true;
                cx.notify();
            }
        }
    }

    /// Flush the active canvas's live graph into its tab snapshot.
    /// Only call when a canvas exists for the active tab.
    /// Kept for legacy call-sites that run before any canvas is created.
    pub fn sync_graph_to_active_tab(&mut self) {
        if let Some(tab) = self.open_tabs.get_mut(self.active_tab_index) {
            tab.graph = self.graph.clone();
            tab.is_dirty = true;
        }
    }

    /// Flush every open canvas's live graph back into its matching tab snapshot.
    ///
    /// This is the **only** correct sync direction: canvas → tab.
    /// All serialisation paths must call this before reading `open_tabs`.
    pub fn sync_all_canvases_to_tabs(&mut self, cx: &App) {
        let canvas_count = self.graph_panels.len();
        tracing::info!(
            ">>> sync_all_canvases_to_tabs: {} canvas panels, {} open tabs",
            canvas_count,
            self.open_tabs.len(),
        );

        let snapshots: Vec<(String, crate::core::graph::BlueprintGraph)> = self
            .graph_panels
            .iter()
            .map(|(tab_id, canvas)| {
                let g = canvas.read(cx);
                tracing::info!(
                    ">>> sync_all_canvases_to_tabs: reading canvas tab={} nodes={} connections={}",
                    tab_id,
                    g.graph.nodes.len(),
                    g.graph.connections.len(),
                );
                (tab_id.clone(), g.graph.clone())
            })
            .collect();

        for (tab_id, live_graph) in &snapshots {
            if let Some(tab) = self.open_tabs.iter_mut().find(|t| t.id == *tab_id) {
                tracing::info!(
                    ">>> sync_all_canvases_to_tabs: writing to tab={} nodes={} connections={} (was nodes={})",
                    tab_id,
                    live_graph.nodes.len(),
                    live_graph.connections.len(),
                    tab.graph.nodes.len(),
                );
                tab.graph = live_graph.clone();
            } else {
                tracing::warn!(
                    ">>> sync_all_canvases_to_tabs: no matching tab for canvas tab_id={}",
                    tab_id,
                );
            }
        }

        if canvas_count == 0 {
            tracing::info!(
                ">>> sync_all_canvases_to_tabs: NO canvas panels exist — tabs retain their current graph data"
            );
        }
    }

    /// Update `self.graph` shadow from the active tab (or its live canvas if one exists).
    pub fn load_active_tab_graph(&mut self) {
        if let Some(tab) = self.open_tabs.get(self.active_tab_index) {
            self.graph = tab.graph.clone();
            self.comment_color_bindings_dirty = true;
        }
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

        tracing::info!(
            ">>> load_blueprint: after load_from_path: open_tabs={}, self.graph.nodes={}, graph_panels={}, current_class_path={:?}",
            self.open_tabs.len(), self.graph.nodes.len(), self.graph_panels.len(), self.current_class_path,
        );

        // Reload library manager so any library macros are available.
        self.library_manager = ui::graph::LibraryManager::default();
        if let Err(e) = self.library_manager.load_all_libraries() {
            eprintln!("Failed to reload sub-graph libraries: {}", e);
        }

        Ok(())
    }

    /// Load local macros from macros.json
    fn load_local_macros(&mut self, class_path: &std::path::Path) -> Result<(), String> {
        let macros_file = class_path.join("macros.json");
        if !macros_file.exists() {
            self.local_macros.clear();
            return Ok(());
        }

        let content = std::fs::read_to_string(&macros_file)
            .map_err(|e| format!("Failed to read macros.json: {}", e))?;
        let macros: Vec<ui::graph::SubGraphDefinition> = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse macros.json: {}", e))?;

        self.local_macros = macros;
        println!(
            "📂 Loaded {} local macros from macros.json",
            self.local_macros.len()
        );
        Ok(())
    }

    /// Restore tabs from tabs.json
    fn restore_tabs_state(
        &mut self,
        class_path: &std::path::Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        #[derive(serde::Deserialize)]
        struct SerializedGraphTab {
            pub id: String,
            pub name: String,
            pub is_main: bool,
            pub is_library_macro: bool,
            pub library_id: Option<String>,
        }

        let tabs_file = class_path.join("tabs.json");
        if !tabs_file.exists() {
            return Ok(());
        }

        let content = std::fs::read_to_string(&tabs_file)
            .map_err(|e| format!("Failed to read tabs.json: {}", e))?;
        let serialized_tabs: Vec<SerializedGraphTab> = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse tabs.json: {}", e))?;

        self.open_tabs.retain(|tab| tab.is_main);
        self.active_tab_index = 0;

        for ser_tab in serialized_tabs {
            if ser_tab.is_main {
                continue;
            }

            if ser_tab.is_library_macro {
                let macro_graph = self
                    .library_manager
                    .get_subgraph(&ser_tab.id)
                    .map(|m| m.graph.clone());

                if let Some(graph) = macro_graph {
                    if let Ok(blueprint_graph) =
                        self.convert_graph_description_to_blueprint(&graph, window, cx)
                    {
                        self.open_tabs.push(GraphTab {
                            id: ser_tab.id.clone(),
                            name: ser_tab.name.clone(),
                            graph: blueprint_graph,
                            is_main: false,
                            is_dirty: false,
                            is_library_macro: true,
                            library_id: ser_tab.library_id.clone(),
                        });
                    }
                }
            } else {
                let macro_graph = self
                    .local_macros
                    .iter()
                    .find(|m| m.id == ser_tab.id)
                    .map(|m| m.graph.clone());

                if let Some(graph) = macro_graph {
                    if let Ok(blueprint_graph) =
                        self.convert_graph_description_to_blueprint(&graph, window, cx)
                    {
                        self.open_tabs.push(GraphTab {
                            id: ser_tab.id.clone(),
                            name: ser_tab.name.clone(),
                            graph: blueprint_graph,
                            is_main: false,
                            is_dirty: false,
                            is_library_macro: false,
                            library_id: None,
                        });
                    }
                }
            }
        }

        println!("📂 Restored {} tabs from tabs.json", self.open_tabs.len());
        Ok(())
    }
}
