//! Core panel struct and initialization
//!
//! This module contains the main `BlueprintEditorPanel` struct definition,
//! constructors, and basic accessors.

use gpui::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use ui::{
    color_picker::ColorPickerState, input::InputState, resizable::ResizableState,
    scroll::ScrollbarState, VirtualListScrollHandle,
};

use super::tabs::GraphTab;
use crate::core::{definitions::NodeDefinitions, events::*, graph::*, types::*};
use crate::editor::workspace_panels::GraphCanvasPanel;
use crate::features::connections::operations::ConnectionDrag;
use crate::features::prefabs::add_component_dialog::AddPrefabComponentDialog;
use crate::features::prefabs::PrefabAsset;
use crate::features::variables::ClassVariable;
use crate::rendering::graph::NodeGraphRenderer;
use crate::ui_components::palette_view::NodePaletteView;
use ui::dock::{DockItem, DockPlacement};
use ui::graph::DataType;
use ui::graph::{DataType as GraphDataType, LibraryManager, SubGraphDefinition};

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
    pub prefab_add_component_dialog: Entity<AddPrefabComponentDialog>,
    pub show_add_component_dialog: bool,
    pub prefab_property_inputs: HashMap<String, Entity<InputState>>,
    pub prefab_color_pickers: HashMap<String, Entity<ColorPickerState>>,
    pub selected_prefab_component: Option<usize>,

    // Comment system
    pub dragging_comment: Option<String>,
    pub resizing_comment: Option<(String, ResizeHandle)>,
    pub editing_comment: Option<String>,
    pub comment_text_input: Entity<InputState>,
    pub comment_color_bindings_dirty: bool,

    // Subscriptions
    pub subscriptions: Vec<Subscription>,

    // Compilation
    pub compilation_status: CompilationStatus,
    pub compilation_history: Vec<CompilationHistoryEntry>,
    pub compile_mode: crate::core::types::CompileMode,
    pub compiler_output_scroll_handle: VirtualListScrollHandle,
    pub compiler_output_scrollbar_state: ScrollbarState,
    pub find_output_scroll_handle: VirtualListScrollHandle,
    pub find_output_scrollbar_state: ScrollbarState,

    // Library/macro system
    pub library_manager: LibraryManager,
    pub local_macros: Vec<SubGraphDefinition>,
    pub selected_macro: Option<usize>,

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
    pub left_top_tab: usize,    // 0=Variables, 1=Functions, 2=Macros
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
        let mut panel = Self::new_internal(Some(file_path.clone()), window, cx);

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
        let prefab_add_component_dialog = cx.new(|cx| AddPrefabComponentDialog::new(window, cx));
        cx.subscribe(
            &prefab_add_component_dialog,
            |this, _, event: &crate::features::prefabs::add_component_dialog::PrefabComponentSelectedEvent, cx| {
                this.add_prefab_component(event.class_name.clone());
                cx.notify();
            },
        )
        .detach();
        cx.subscribe(
            &prefab_add_component_dialog,
            |this, _, _: &DismissEvent, cx| {
                this.show_add_component_dialog = false;
                cx.notify();
            },
        )
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
            prefab_add_component_dialog,
            show_add_component_dialog: false,
            prefab_property_inputs: HashMap::new(),
            prefab_color_pickers: HashMap::new(),
            selected_prefab_component: None,
            dragging_comment: None,
            resizing_comment: None,
            editing_comment: None,
            comment_text_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("Comment text...")),
            comment_color_bindings_dirty: true,
            subscriptions: Vec::new(),
            compilation_status: CompilationStatus::default(),
            compilation_history: Vec::new(),
            compile_mode: crate::core::types::CompileMode::default(),
            compiler_output_scroll_handle: VirtualListScrollHandle::new(),
            compiler_output_scrollbar_state: ScrollbarState::default(),
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
        }
    }

    /// Create a sample graph for demonstration - demonstrates all compiler features
    fn create_sample_graph() -> BlueprintGraph {
        use crate::core::types::*;
        use ui::graph::DataType as GraphDataType;

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
                    data_type: GraphDataType::from_type_str("string"),
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
                    data_type: GraphDataType::from_type_str("string"),
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

    /// Get immutable reference to graph
    pub fn get_graph(&self) -> &BlueprintGraph {
        &self.graph
    }

    /// Get mutable reference to graph
    pub fn get_graph_mut(&mut self) -> &mut BlueprintGraph {
        &mut self.graph
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
            return;
        };

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

        workspace_entity.update(cx, |workspace, cx| {
            workspace.dock_area().update(cx, |dock_area, cx| {
                fn activate_tab_item(
                    item: &mut DockItem,
                    tab_index: usize,
                    window: &mut Window,
                    cx: &mut App,
                ) -> bool {
                    match item {
                        DockItem::Tabs { view, .. } => {
                            view.update(cx, |tab_panel, cx| {
                                tab_panel.set_active_tab(tab_index, window, cx);
                            });
                            true
                        }
                        DockItem::Split { items, .. } => {
                            for child in items.iter_mut() {
                                if activate_tab_item(child, tab_index, window, cx) {
                                    return true;
                                }
                            }
                            false
                        }
                        _ => false,
                    }
                }

                let _ = activate_tab_item(dock_area.items_mut(), tab_index, window, cx);
            });
        });
    }

    /// Switch to a different tab
    pub fn switch_to_tab(&mut self, tab_index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if tab_index < self.open_tabs.len() && tab_index != self.active_tab_index {
            self.sync_graph_to_active_tab();
            self.active_tab_index = tab_index;
            self.load_active_tab_graph();
            self.activate_graph_workspace_tab(tab_index, window, cx);
            cx.notify();
        }
    }

    /// Open a macro tab by macro ID, or switch to it if already open
    pub fn open_macro_tab(&mut self, macro_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        // Check if tab is already open
        if let Some(tab_index) = self.open_tabs.iter().position(|tab| tab.id == macro_id) {
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
                // Sync current tab before switching
                self.sync_graph_to_active_tab();

                // Create new tab
                self.open_tabs.push(GraphTab {
                    id: macro_id.to_string(),
                    name: macro_name,
                    graph: blueprint_graph,
                    is_main: false,
                    is_dirty: false,
                    is_library_macro: false,
                    library_id: None,
                });

                // Switch to the new tab
                let new_tab_index = self.open_tabs.len() - 1;
                self.active_tab_index = new_tab_index;
                self.load_active_tab_graph();
                self.graph_workspace_tabs_dirty = true;
                cx.notify();
            }
        }
    }

    /// Sync current graph to active tab
    pub fn sync_graph_to_active_tab(&mut self) {
        let tab_id = if let Some(tab) = self.open_tabs.get(self.active_tab_index) {
            tab.id.clone()
        } else {
            return;
        };

        let is_main = if let Some(tab) = self.open_tabs.get(self.active_tab_index) {
            tab.is_main
        } else {
            return;
        };

        if let Some(tab) = self.open_tabs.get_mut(self.active_tab_index) {
            tab.graph = self.graph.clone();
            tab.is_dirty = true;
        }

        // Sync local macros
        if !is_main && !tab_id.starts_with("🌐") {
            if let Some(_macro_def) = self.local_macros.iter_mut().find(|m| m.id == tab_id) {
                // Graph conversion would happen here if needed
            }
        }
    }

    /// Load active tab's graph
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

    /// Load blueprint from file
    pub fn load_blueprint(
        &mut self,
        file_path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        println!("📂 ═══════════════════════════════════════════════════════════════");
        println!("📂 LOADING BLUEPRINT FROM FILE");
        println!("📂 ═══════════════════════════════════════════════════════════════");
        println!("📂 File: {}", file_path);

        let content = std::fs::read_to_string(file_path).map_err(|e| {
            let error_msg = format!("Failed to read file: {}", e);
            eprintln!("❌ {}", error_msg);
            error_msg
        })?;

        println!("📂 ✓ File read successfully ({} bytes)", content.len());

        // Strip header comments
        let json = content
            .lines()
            .skip_while(|line| line.trim().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        // Try new unified format first
        match serde_json::from_str::<ui::graph::BlueprintAsset>(&json) {
            Ok(blueprint_asset) => {
                println!("📂 ✓ Detected unified blueprint format");
                self.load_from_blueprint_asset(blueprint_asset, file_path, window, cx)?;
            }
            Err(unified_err) => {
                println!("📂 ⚠️  Unified format parse failed:");
                println!("📂    Error: {}", unified_err);
                println!(
                    "📂    Line: {}, Column: {}",
                    unified_err.line(),
                    unified_err.column()
                );

                // Show context around the error location
                let lines: Vec<&str> = json.lines().collect();
                let error_line = unified_err.line().saturating_sub(1);
                if error_line < lines.len() {
                    println!("📂    Context:");
                    for i in error_line.saturating_sub(2)
                        ..=error_line
                            .saturating_add(2)
                            .min(lines.len().saturating_sub(1))
                    {
                        println!(
                            "📂      {}{}: {}",
                            if i == error_line { ">>> " } else { "    " },
                            i + 1,
                            lines.get(i).unwrap_or(&"")
                        );
                    }
                }

                println!("📂 ✓ Trying legacy format...");

                // Try parsing as legacy format
                self.load_legacy_format(&json, file_path, window, cx)
                    .map_err(|e| {
                        let error_msg = format!("Failed to parse as both unified and legacy format.\nUnified error: {}\nLegacy error: {}", unified_err, e);
                        eprintln!("❌ {}", error_msg);
                        error_msg
                    })?;
            }
        }

        // Reload library manager
        self.library_manager = ui::graph::LibraryManager::default();
        if let Err(e) = self.library_manager.load_all_libraries() {
            eprintln!("Failed to reload sub-graph libraries: {}", e);
        }

        cx.notify();
        Ok(())
    }

    /// Load from unified blueprint asset
    fn load_from_blueprint_asset(
        &mut self,
        asset: ui::graph::BlueprintAsset,
        file_path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let file_path_buf = std::path::Path::new(file_path);
        if let Some(parent) = file_path_buf.parent() {
            self.set_path(parent.to_path_buf());
        }

        // Load main graph
        self.graph = self.convert_graph_description_to_blueprint(&asset.main_graph, window, cx)?;
        self.comment_color_bindings_dirty = true;

        // Load local macros
        self.local_macros = asset.local_macros;

        // Load variables
        self.class_variables = asset
            .variables
            .iter()
            .map(|v| ClassVariable {
                name: v.name.clone(),
                var_type: v.data_type.to_string(),
                default_value: v.default_value.clone(),
            })
            .collect();

        // Restore main tab
        self.open_tabs = vec![GraphTab {
            id: "main".to_string(),
            name: "EventGraph".to_string(),
            graph: self.graph.clone(),
            is_main: true,
            is_dirty: false,
            is_library_macro: false,
            library_id: None,
        }];
        self.active_tab_index = 0;

        // Restore editor state (open tabs, active tab, view states)
        if let Some(editor_state) = asset.editor_state {
            // Restore open tabs
            for tab_id in &editor_state.open_tab_ids {
                if tab_id == "main" {
                    continue; // Already added
                }

                // Check if this is a local macro
                let macro_data = self
                    .local_macros
                    .iter()
                    .find(|m| &m.id == tab_id)
                    .map(|m| (m.name.clone(), m.graph.clone()));

                if let Some((macro_name, macro_graph)) = macro_data {
                    if let Ok(mut blueprint_graph) =
                        self.convert_graph_description_to_blueprint(&macro_graph, window, cx)
                    {
                        // Restore view state for this tab if available
                        if let Some(view_state) = editor_state.graph_view_states.get(tab_id) {
                            blueprint_graph.pan_offset = Point {
                                x: view_state.pan_offset_x,
                                y: view_state.pan_offset_y,
                            };
                            blueprint_graph.zoom_level = view_state.zoom;
                        }

                        self.open_tabs.push(GraphTab {
                            id: tab_id.clone(),
                            name: macro_name,
                            graph: blueprint_graph,
                            is_main: false,
                            is_dirty: false,
                            is_library_macro: false,
                            library_id: None,
                        });
                    }
                }
            }

            // Restore view state for main tab
            if let Some(view_state) = editor_state.graph_view_states.get("main") {
                if let Some(main_tab) = self.open_tabs.iter_mut().find(|t| t.is_main) {
                    main_tab.graph.pan_offset = Point {
                        x: view_state.pan_offset_x,
                        y: view_state.pan_offset_y,
                    };
                    main_tab.graph.zoom_level = view_state.zoom;
                }

                self.graph.pan_offset = Point {
                    x: view_state.pan_offset_x,
                    y: view_state.pan_offset_y,
                };
                self.graph.zoom_level = view_state.zoom;
            }

            // Restore active tab index (with bounds check)
            self.active_tab_index = editor_state
                .active_tab_index
                .min(self.open_tabs.len().saturating_sub(1));

            // Load the active tab's graph into self.graph
            if let Some(active_tab) = self.open_tabs.get(self.active_tab_index) {
                self.graph = active_tab.graph.clone();
                self.comment_color_bindings_dirty = true;
            }
        }

        println!("📂 Loaded unified blueprint format");
        println!("📂   ✓ Main Graph: {} nodes", self.graph.nodes.len());
        println!("📂   ✓ Local Macros: {}", self.local_macros.len());
        println!("📂   ✓ Variables: {}", self.class_variables.len());
        println!("📂   ✓ Open Tabs: {}", self.open_tabs.len());
        println!("📂   ✓ Active Tab Index: {}", self.active_tab_index);
        println!("📂 ═══════════════════════════════════════════════════════════════");

        Ok(())
    }

    /// Load legacy format (old format before unified blueprint)
    fn load_legacy_format(
        &mut self,
        json: &str,
        file_path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        // Define legacy structures inline for backward compatibility
        #[derive(serde::Deserialize)]
        struct LegacyGraphDescription {
            pub nodes: HashMap<String, ui::graph::NodeInstance>,
            pub connections: Vec<ui::graph::Connection>,
            pub metadata: ui::graph::GraphMetadata,
            #[serde(default)]
            pub comments: Vec<LegacyBlueprintComment>,
        }

        #[derive(serde::Deserialize)]
        struct LegacyBlueprintComment {
            pub id: String,
            pub text: String,
            pub position: LegacyPosition,
            pub size: LegacySize,
            pub color: LegacyColor,
            pub contained_node_ids: Vec<String>,
        }

        #[derive(serde::Deserialize)]
        struct LegacyPosition {
            pub x: f32,
            pub y: f32,
        }

        #[derive(serde::Deserialize)]
        struct LegacySize {
            pub width: f32,
            pub height: f32,
        }

        #[derive(serde::Deserialize)]
        struct LegacyColor {
            pub h: f32,
            pub s: f32,
            pub l: f32,
            pub a: f32,
        }

        let legacy_graph: LegacyGraphDescription = serde_json::from_str(json)
            .map_err(|e| format!("Failed to parse legacy format: {}", e))?;

        println!("📂 ✓ Legacy format parsed successfully");

        // Convert legacy format to current format
        let graph_description = ui::graph::GraphDescription {
            nodes: legacy_graph.nodes,
            connections: legacy_graph.connections,
            metadata: legacy_graph.metadata,
            comments: legacy_graph
                .comments
                .into_iter()
                .map(|c| {
                    let (r, g, b) = hsl_to_rgb(c.color.h, c.color.s, c.color.l);
                    ui::graph::BlueprintComment {
                        id: c.id,
                        text: c.text,
                        position: (c.position.x, c.position.y),
                        size: (c.size.width, c.size.height),
                        color: [r, g, b, c.color.a],
                        contained_node_ids: c.contained_node_ids,
                    }
                })
                .collect(),
        };

        self.graph = self.convert_graph_description_to_blueprint(&graph_description, window, cx)?;
        self.comment_color_bindings_dirty = true;

        // Reset to main tab
        self.open_tabs = vec![GraphTab {
            id: "main".to_string(),
            name: "EventGraph".to_string(),
            graph: self.graph.clone(),
            is_main: true,
            is_dirty: false,
            is_library_macro: false,
            library_id: None,
        }];
        self.active_tab_index = 0;

        // Load separate legacy files
        let file_path_buf = std::path::Path::new(file_path);
        if let Some(parent) = file_path_buf.parent() {
            self.set_path(parent.to_path_buf());
            let _ = self.load_local_macros(parent);
            let _ = self.restore_tabs_state(parent, window, cx);
            let _ = self.load_variables_from_class(parent);
        }

        println!("📂 Loaded blueprint in legacy format");
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

// Helper function for HSL to RGB conversion
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s == 0.0 {
        return (l, l, l);
    }

    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;

    let hue_to_rgb = |p: f32, q: f32, mut t: f32| -> f32 {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            return p + (q - p) * 6.0 * t;
        }
        if t < 1.0 / 2.0 {
            return q;
        }
        if t < 2.0 / 3.0 {
            return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
        }
        p
    };

    (
        hue_to_rgb(p, q, h + 1.0 / 3.0),
        hue_to_rgb(p, q, h),
        hue_to_rgb(p, q, h - 1.0 / 3.0),
    )
}
