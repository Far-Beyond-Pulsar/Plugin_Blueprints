//! Dedicated panel components for the workspace docking system
//!
//! These panels wrap the editor entity and render specific content.

use gpui::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use ui::{
    dock::{Panel, PanelEvent},
    h_flex, input::{InputEvent, InputState}, v_flex, ActiveTheme,
};

use crate::core::graph::BlueprintGraph;
use crate::core::types::BlueprintNode;
use crate::editor::panel::{BlueprintEditorPanel, ResizeHandle};
use crate::features::connections::operations::ConnectionDrag;
use crate::features::macros::panel::MacrosRenderer;
use crate::features::prefabs::panel::{PrefabHierarchyRenderer, PrefabPropertiesRenderer};
use crate::features::undo::UndoManager;
use crate::features::variables::rendering::VariablesRenderer;
use crate::rendering::graph::NodeGraphRenderer;
use crate::ui_components::palette_view::NodePaletteView;
use crate::ui_components::properties::PropertiesRenderer;
use ui_common::reflected_properties_panel::PropertyStateManager;

/// Variables Panel
pub struct VariablesPanel {
    editor: WeakEntity<BlueprintEditorPanel>,
    focus_handle: FocusHandle,
}

impl VariablesPanel {
    pub fn new(editor: WeakEntity<BlueprintEditorPanel>, cx: &mut Context<Self>) -> Self {
        Self {
            editor,
            focus_handle: cx.focus_handle(),
        }
    }
}

impl EventEmitter<PanelEvent> for VariablesPanel {}

impl Render for VariablesPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(editor) = self.editor.upgrade() {
            div()
                .size_full()
                .bg(cx.theme().sidebar)
                .child(editor.update(cx, |editor, cx| VariablesRenderer::render(editor, cx)))
        } else {
            div().child("Editor not available")
        }
    }
}

impl Focusable for VariablesPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for VariablesPanel {
    fn panel_name(&self) -> &'static str {
        "variables"
    }

    fn title(&self, _window: &Window, _cx: &App) -> AnyElement {
        "Variables".into_any_element()
    }
}

/// Macros Panel
pub struct MacrosPanel {
    editor: WeakEntity<BlueprintEditorPanel>,
    focus_handle: FocusHandle,
}

impl MacrosPanel {
    pub fn new(editor: WeakEntity<BlueprintEditorPanel>, cx: &mut Context<Self>) -> Self {
        Self {
            editor,
            focus_handle: cx.focus_handle(),
        }
    }
}

impl EventEmitter<PanelEvent> for MacrosPanel {}

impl Render for MacrosPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(editor) = self.editor.upgrade() {
            div()
                .size_full()
                .bg(cx.theme().sidebar)
                .child(editor.update(cx, |editor, cx| MacrosRenderer::render(editor, cx)))
        } else {
            div().child("Editor not available")
        }
    }
}

impl Focusable for MacrosPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for MacrosPanel {
    fn panel_name(&self) -> &'static str {
        "macros"
    }

    fn title(&self, _window: &Window, _cx: &App) -> AnyElement {
        "Macros".into_any_element()
    }
}

/// Compiler Panel
pub struct CompilerPanel {
    editor: WeakEntity<BlueprintEditorPanel>,
    focus_handle: FocusHandle,
}

impl CompilerPanel {
    pub fn new(editor: WeakEntity<BlueprintEditorPanel>, cx: &mut Context<Self>) -> Self {
        Self {
            editor,
            focus_handle: cx.focus_handle(),
        }
    }
}

impl EventEmitter<PanelEvent> for CompilerPanel {}

impl Render for CompilerPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(editor) = self.editor.upgrade() {
            div()
                .size_full()
                .child(editor.update(cx, |editor, cx| editor.render_compiler_results(cx)))
        } else {
            div().child("Editor not available")
        }
    }
}

impl Focusable for CompilerPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for CompilerPanel {
    fn panel_name(&self) -> &'static str {
        "compiler"
    }

    fn title(&self, _window: &Window, _cx: &App) -> AnyElement {
        "Compiler".into_any_element()
    }
}

/// Find Panel
pub struct FindPanel {
    editor: WeakEntity<BlueprintEditorPanel>,
    focus_handle: FocusHandle,
}

impl FindPanel {
    pub fn new(editor: WeakEntity<BlueprintEditorPanel>, cx: &mut Context<Self>) -> Self {
        Self {
            editor,
            focus_handle: cx.focus_handle(),
        }
    }
}

impl EventEmitter<PanelEvent> for FindPanel {}

impl Render for FindPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(editor) = self.editor.upgrade() {
            div()
                .size_full()
                .child(editor.update(cx, |editor, cx| editor.render_find_panel(cx)))
        } else {
            div().child("Editor not available")
        }
    }
}

impl Focusable for FindPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for FindPanel {
    fn panel_name(&self) -> &'static str {
        "find"
    }

    fn title(&self, _window: &Window, _cx: &App) -> AnyElement {
        "Find".into_any_element()
    }
}

/// Properties Panel
pub struct PropertiesPanel {
    editor: WeakEntity<BlueprintEditorPanel>,
    focus_handle: FocusHandle,
}

impl PropertiesPanel {
    pub fn new(editor: WeakEntity<BlueprintEditorPanel>, cx: &mut Context<Self>) -> Self {
        Self {
            editor,
            focus_handle: cx.focus_handle(),
        }
    }
}

impl EventEmitter<PanelEvent> for PropertiesPanel {}

impl Render for PropertiesPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(editor) = self.editor.upgrade() {
            div()
                .size_full()
                .bg(cx.theme().sidebar)
                .child(editor.update(cx, |editor, cx| {
                    PropertiesRenderer::render(editor, _window, cx)
                }))
        } else {
            div().child("Editor not available")
        }
    }
}

impl Focusable for PropertiesPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for PropertiesPanel {
    fn panel_name(&self) -> &'static str {
        "properties"
    }

    fn title(&self, _window: &Window, _cx: &App) -> AnyElement {
        "Blueprint Details".into_any_element()
    }
}

/// Prefab Hierarchy Panel
pub struct PrefabHierarchyPanel {
    editor: WeakEntity<BlueprintEditorPanel>,
    focus_handle: FocusHandle,
}

impl PrefabHierarchyPanel {
    pub fn new(editor: WeakEntity<BlueprintEditorPanel>, cx: &mut Context<Self>) -> Self {
        Self {
            editor,
            focus_handle: cx.focus_handle(),
        }
    }
}

impl EventEmitter<PanelEvent> for PrefabHierarchyPanel {}

impl Render for PrefabHierarchyPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(editor) = self.editor.upgrade() {
            div()
                .size_full()
                .bg(cx.theme().sidebar)
                .child(editor.update(cx, |editor, cx| PrefabHierarchyRenderer::render(editor, cx)))
        } else {
            div().child("Editor not available")
        }
    }
}

impl Focusable for PrefabHierarchyPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for PrefabHierarchyPanel {
    fn panel_name(&self) -> &'static str {
        "prefab-hierarchy"
    }

    fn title(&self, _window: &Window, _cx: &App) -> AnyElement {
        "Components".into_any_element()
    }
}

/// Prefabs Panel
pub struct PrefabsPanel {
    editor: WeakEntity<BlueprintEditorPanel>,
    focus_handle: FocusHandle,
}

impl PrefabsPanel {
    pub fn new(editor: WeakEntity<BlueprintEditorPanel>, cx: &mut Context<Self>) -> Self {
        Self {
            editor,
            focus_handle: cx.focus_handle(),
        }
    }
}

impl EventEmitter<PanelEvent> for PrefabsPanel {}

impl Render for PrefabsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(editor) = self.editor.upgrade() {
            div()
                .size_full()
                .bg(cx.theme().sidebar)
                .child(editor.update(cx, |editor, cx| {
                    PrefabPropertiesRenderer::render(editor, window, cx)
                }))
        } else {
            div().child("Editor not available")
        }
    }
}

impl Focusable for PrefabsPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for PrefabsPanel {
    fn panel_name(&self) -> &'static str {
        "prefabs"
    }

    fn title(&self, _window: &Window, _cx: &App) -> AnyElement {
        "Component Properties".into_any_element()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Palette Panel
// ─────────────────────────────────────────────────────────────────────────────

/// Palette panel – thin dock wrapper around [`NodePaletteView`].
///
/// All palette logic (search, category headers, virtual list, node placement)
/// lives in `NodePaletteView` so the same component can be reused in both this
/// panel and the quick right-click overlay on the graph canvas.
pub struct PalettePanel {
    focus_handle: FocusHandle,
    palette_view: Entity<NodePaletteView>,
}

impl PalettePanel {
    pub fn new(
        editor: WeakEntity<BlueprintEditorPanel>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette_view = cx.new(|cx| NodePaletteView::new(editor, window, cx));
        Self {
            focus_handle: cx.focus_handle(),
            palette_view,
        }
    }
}

impl EventEmitter<PanelEvent> for PalettePanel {}

impl Render for PalettePanel {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.palette_view.clone())
    }
}

impl Focusable for PalettePanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for PalettePanel {
    fn panel_name(&self) -> &'static str {
        "palette"
    }

    fn title(&self, _window: &Window, _cx: &App) -> AnyElement {
        "Palette".into_any_element()
    }
}

/// Main graph canvas panel — a real GPUI entity that owns ALL state for one tab.
///
/// Each open tab is its own `GraphCanvasPanel` entity. It owns the graph, the
/// per-tab undo history, the GPU surface/renderer, and every piece of
/// interaction state. The `panel` weak-ref is read-only access to shared data
/// (library_manager, class_variables, local_macros) on the shell editor.
pub struct GraphCanvasPanel {
    pub id: String,
    pub name: String,
    pub is_main: bool,
    pub is_dirty: bool,
    pub is_library_macro: bool,
    pub library_id: Option<String>,

    /// Read-only reference to the shell editor for shared data + cross-tab events.
    pub panel: WeakEntity<BlueprintEditorPanel>,

    pub graph: BlueprintGraph,
    pub undo_manager: UndoManager,
    pub focus_handle: FocusHandle,

    // ── GPU renderer (per-canvas) ──────────────────────────────────────────
    pub renderer: crate::rendering::gpu::BpRenderer,
    pub surface: Option<gpui::WgpuSurfaceHandle>,
    pub canvas_origin: Rc<RefCell<Point<f32>>>,
    pub element_bounds: Option<Bounds<Pixels>>,
    pub graph_anim_start: std::time::Instant,

    // ── Runtime / debug ────────────────────────────────────────────────────
    pub running_nodes: HashSet<String>,
    pub breakpoints: HashSet<String>,
    pub debug_session: Option<crate::features::debug::DebugSession>,
    pub show_debug_overlay: bool,
    pub show_minimap: bool,
    pub show_graph_controls: bool,
    pub wire_active_test_mode: bool,
    pub wire_hidden_test_mode: bool,

    // ── Context menus / overlays ───────────────────────────────────────────
    pub node_context_menu: Option<(String, Point<Pixels>)>,
    pub pin_context_menu: Option<(String, String, Point<Pixels>)>,

    // ── Quick palette (right-click on canvas) ──────────────────────────────
    pub quick_palette_open: bool,
    pub quick_palette_focus_pending: bool,
    pub quick_palette_connection_source: Option<ConnectionDrag>,
    pub quick_palette_screen_pos: Point<Pixels>,
    pub popup_palette_graph_pos: Option<Point<f32>>,
    pub quick_palette_view: Entity<NodePaletteView>,

    // ── Pin hover tooltip ──────────────────────────────────────────────────
    pub hovered_pin_tooltip: Option<String>,
    pub hovered_pin_tooltip_pos: Option<Point<Pixels>>,
    pub hovered_connection: Option<String>,

    // ── Clipboard ──────────────────────────────────────────────────────────
    pub node_clipboard: Option<BlueprintNode>,

    // ── Node drag state ────────────────────────────────────────────────────
    pub dragging_node: Option<String>,
    pub drag_offset: Point<f32>,
    pub initial_drag_positions: HashMap<String, Point<f32>>,
    pub initial_comment_drag_positions: HashMap<String, Point<f32>>,
    pub pending_drag_node: Option<String>,
    pub pending_drag_start: Option<Point<f32>>,
    pub drag_commit_threshold: f32,

    // ── Connection drag ────────────────────────────────────────────────────
    pub dragging_connection: Option<ConnectionDrag>,

    // ── Panning ────────────────────────────────────────────────────────────
    pub is_panning: bool,
    pub pan_start: Point<f32>,
    pub pan_start_offset: Point<f32>,

    // ── Selection ──────────────────────────────────────────────────────────
    pub selection_start: Option<Point<f32>>,
    pub selection_end: Option<Point<f32>>,
    pub last_mouse_pos: Option<Point<f32>>,

    // ── Right-click gesture / double-click ─────────────────────────────────
    pub right_click_start: Option<Point<f32>>,
    pub right_click_threshold: f32,
    pub last_click_time: Option<std::time::Instant>,
    pub last_click_pos: Option<Point<f32>>,

    // ── Comments ───────────────────────────────────────────────────────────
    pub dragging_comment: Option<String>,
    pub resizing_comment: Option<(String, ResizeHandle)>,
    pub resizing_comment_start: Option<(Point<f32>, Size<f32>)>,
    pub editing_comment: Option<String>,
    pub last_comment_click_time: Option<std::time::Instant>,
    pub last_comment_click_pos: Option<Point<f32>>,
    pub last_comment_click_id: Option<String>,
    pub comment_text_input: Entity<InputState>,
    pub comment_color_bindings_dirty: bool,
    pub pin_property_state: PropertyStateManager,

    // ── Variable drag ──────────────────────────────────────────────────────
    pub dragging_variable: Option<crate::features::variables::VariableDrag>,
    pub variable_drop_menu_position: Option<Point<f32>>,

    // ── Macro drag / pin editor ────────────────────────────────────────────
    pub dragging_macro: Option<crate::features::macros::MacroDrag>,
    pub macro_pin_add_mode: Option<bool>,

    // ── Custom Event Configurator ──────────────────────────────────────────
    pub event_configurator_open: bool,
    pub editing_event_node: Option<String>,
    pub event_name_input: Entity<InputState>,
    pub event_return_type_input: Entity<InputState>,
    pub event_fields: Vec<crate::editor::panel::EventConfigField>,
    pub field_name_inputs: Vec<Entity<InputState>>,
    pub field_type_dropdowns: Vec<Entity<ui::dropdown::DropdownState<ui::dropdown::SearchableVec<crate::editor::panel::EventFieldType>>>>,

    pub subscriptions: Vec<Subscription>,
}

impl GraphCanvasPanel {
    pub fn new(
        panel: WeakEntity<BlueprintEditorPanel>,
        id: String,
        name: String,
        is_main: bool,
        graph: BlueprintGraph,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let canvas_weak = cx.entity().downgrade();
        let quick_palette_view =
            cx.new(|cx| NodePaletteView::new_for_canvas(canvas_weak, panel.clone(), window, cx));
        let comment_text_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Comment text..."));
        cx.subscribe_in(
            &comment_text_input,
            window,
            |this, state: &Entity<InputState>, event: &InputEvent, _window, cx| {
                if matches!(
                    event,
                    InputEvent::Change | InputEvent::Blur | InputEvent::PressEnter { .. }
                ) {
                    let text = state.read(cx).text().to_string();
                    if let Some(comment_id) = this
                        .editing_comment
                        .clone()
                        .or_else(|| this.graph.selected_comments.first().cloned())
                    {
                        if let Some(comment) =
                            this.graph.comments.iter_mut().find(|c| c.id == comment_id)
                        {
                            comment.text = text;
                            this.is_dirty = true;
                            cx.notify();
                        }
                    }
                }

                if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. })
                    && this.editing_comment.is_some()
                {
                    this.finish_comment_editing(cx);
                }
            },
        )
        .detach();
        Self {
            id,
            name,
            is_main,
            is_dirty: false,
            is_library_macro: false,
            library_id: None,
            panel,
            graph,
            undo_manager: UndoManager::new(),
            focus_handle: cx.focus_handle(),
            renderer: crate::rendering::gpu::BpRenderer::new(),
            surface: None,
            canvas_origin: Rc::new(RefCell::new(Point::new(0.0, 0.0))),
            element_bounds: None,
            graph_anim_start: std::time::Instant::now(),
            running_nodes: HashSet::new(),
            breakpoints: HashSet::new(),
            debug_session: None,
            show_debug_overlay: true,
            show_minimap: true,
            show_graph_controls: true,
            wire_active_test_mode: false,
            wire_hidden_test_mode: false,
            node_context_menu: None,
            pin_context_menu: None,
            quick_palette_open: false,
            quick_palette_focus_pending: false,
            quick_palette_connection_source: None,
            quick_palette_screen_pos: Point::default(),
            popup_palette_graph_pos: None,
            quick_palette_view,
            hovered_pin_tooltip: None,
            hovered_pin_tooltip_pos: None,
            hovered_connection: None,
            node_clipboard: None,
            dragging_node: None,
            drag_offset: Point::new(0.0, 0.0),
            initial_drag_positions: HashMap::new(),
            initial_comment_drag_positions: HashMap::new(),
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
            dragging_comment: None,
            resizing_comment: None,
            resizing_comment_start: None,
            editing_comment: None,
            last_comment_click_time: None,
            last_comment_click_pos: None,
            last_comment_click_id: None,
            comment_text_input,
            comment_color_bindings_dirty: true,
            pin_property_state: PropertyStateManager::new(),
            dragging_variable: None,
            variable_drop_menu_position: None,
            dragging_macro: None,
            macro_pin_add_mode: None,
            event_configurator_open: false,
            editing_event_node: None,
            event_name_input: cx.new(|cx| InputState::new(window, cx).placeholder("Event name...")),
            event_return_type_input: cx.new(|cx| InputState::new(window, cx).placeholder("Return type (e.g. f32)...")),
            event_fields: Vec::new(),
            field_name_inputs: Vec::new(),
            field_type_dropdowns: Vec::new(),
            subscriptions: Vec::new(),
        }
    }

    /// Get focus handle
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    /// Read the shell editor's library manager (shared data).
    pub fn library_manager(&self, cx: &App) -> Option<ui::graph::LibraryManager> {
        self.panel
            .upgrade()
            .map(|p| p.read(cx).library_manager.clone())
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

    pub fn update_node_input_property(
        &mut self,
        node_id: impl AsRef<str>,
        pin_id: impl AsRef<str>,
        value: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let Some(node) = self.graph.nodes.iter_mut().find(|n| n.id == node_id.as_ref()) else {
            return;
        };

        if value.is_null() {
            node.properties.remove(pin_id.as_ref());
        } else {
            let stored_value = match value {
                serde_json::Value::String(text) => text,
                other => other.to_string(),
            };
            node.properties
                .insert(pin_id.as_ref().to_string(), stored_value);
        }

        self.is_dirty = true;
        cx.notify();
    }
}

impl EventEmitter<PanelEvent> for GraphCanvasPanel {}

impl Render for GraphCanvasPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.refresh_comment_color_bindings(window, cx);
        crate::rendering::input::refresh_graph_cursor(window, self);
        div()
            .size_full()
            .child(NodeGraphRenderer::render(self, cx))
    }
}

impl Focusable for GraphCanvasPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for GraphCanvasPanel {
    fn panel_name(&self) -> &'static str {
        "graph-canvas"
    }

    fn title(&self, _window: &Window, _cx: &App) -> AnyElement {
        let title = if self.is_dirty {
            format!("{} *", self.name)
        } else {
            self.name.clone()
        };
        title.into_any_element()
    }
}

// ─── Comment / clipboard operations on GraphCanvasPanel ──────────────────────

impl GraphCanvasPanel {
    pub fn snap_comment_position(&self, position: Point<f32>) -> Point<f32> {
        crate::rendering::graph::NodeGraphRenderer::snap_to_grid(position)
    }

    fn snap_comment_size(size: gpui::Size<f32>) -> gpui::Size<f32> {
        let grid = 10.0;
        gpui::Size::new((size.width / grid).round() * grid, (size.height / grid).round() * grid)
    }

    fn snap_comment_bounds(comment: &mut crate::core::types::BlueprintComment) {
        let l = (comment.position.x / 10.0).round() * 10.0;
        let t = (comment.position.y / 10.0).round() * 10.0;
        let r = ((comment.position.x + comment.size.width) / 10.0).round() * 10.0;
        let b = ((comment.position.y + comment.size.height) / 10.0).round() * 10.0;
        comment.position = Point::new(l, t);
        comment.size = Self::snap_comment_size(gpui::Size::new((r - l).max(100.0), (b - t).max(50.0)));
    }

    pub(crate) fn refresh_comment_color_bindings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.comment_color_bindings_dirty { return; }
        self.subscriptions.clear();
        for comment in &self.graph.comments {
            if let Some(picker_state) = comment.color_picker_state.as_ref() {
                let comment_id = comment.id.clone();
                let sub = cx.subscribe_in(picker_state, window,
                    move |this: &mut GraphCanvasPanel, _picker, event: &ui::color_picker::ColorPickerEvent, _window, cx| {
                        if let ui::color_picker::ColorPickerEvent::Change(Some(color)) = event {
                            if let Some(c) = this.graph.comments.iter_mut().find(|c| c.id == comment_id) {
                                c.color = *color;
                                this.is_dirty = true;
                                cx.notify();
                            }
                        }
                    });
                self.subscriptions.push(sub);
            }
        }
        self.comment_color_bindings_dirty = false;
    }

    pub fn start_comment_drag(&mut self, comment_id: String, mouse_pos: Point<f32>, _cx: &mut Context<Self>) {
        let Some(comment) = self.graph.comments.iter().find(|c| c.id == comment_id) else {
            return;
        };

        self.editing_comment = None;
        self.dragging_comment = Some(comment_id.clone());
        self.drag_offset = Point::new(mouse_pos.x - comment.position.x, mouse_pos.y - comment.position.y);
        self.initial_drag_positions.clear();
        self.initial_comment_drag_positions.clear();

        let mut comment_ids = std::collections::HashSet::new();
        let mut node_ids = std::collections::HashSet::new();

        if self.graph.selected_comments.contains(&comment_id) {
            for sid in &self.graph.selected_comments {
                self.collect_comment_drag_group(sid, &mut comment_ids, &mut node_ids);
            }
            for nid in &self.graph.selected_nodes {
                node_ids.insert(nid.clone());
            }
        } else {
            self.collect_comment_drag_group(&comment_id, &mut comment_ids, &mut node_ids);
        }

        for nid in node_ids {
            if let Some(node) = self.graph.nodes.iter().find(|n| n.id == nid) {
                self.initial_drag_positions.insert(nid, node.position);
            }
        }
        for cid in comment_ids {
            if let Some(comment) = self.graph.comments.iter().find(|c| c.id == cid) {
                self.initial_comment_drag_positions
                    .insert(cid, comment.position);
            }
        }
    }

    pub fn start_comment_resize(
        &mut self,
        comment_id: String,
        handle: ResizeHandle,
        mouse_pos: Point<f32>,
        _cx: &mut Context<Self>,
    ) {
        let Some(comment) = self.graph.comments.iter().find(|c| c.id == comment_id) else {
            return;
        };

        self.editing_comment = None;
        self.resizing_comment = Some((comment_id, handle));
        self.resizing_comment_start = Some((comment.position, comment.size));
        self.drag_offset = mouse_pos;
    }

    pub fn start_comment_title_editing(
        &mut self,
        comment_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(comment) = self.graph.comments.iter().find(|c| c.id == comment_id) {
            self.editing_comment = Some(comment_id);
            self.comment_text_input.update(cx, |input, cx| {
                input.set_value(comment.text.clone(), window, cx);
                input.focus(window, cx);
            });
            cx.notify();
        }
    }

    pub(crate) fn collect_comment_drag_group(
        &self,
        root_id: &str,
        comment_ids: &mut std::collections::HashSet<String>,
        node_ids: &mut std::collections::HashSet<String>,
    ) {
        let Some(root) = self.graph.comments.iter().find(|c| c.id == root_id) else {
            return;
        };

        if !comment_ids.insert(root.id.clone()) {
            return;
        }

        for node in &self.graph.nodes {
            if root.contains_node(node) {
                node_ids.insert(node.id.clone());
            }
        }

        for child in &self.graph.comments {
            if child.id != root.id && root.contains_comment(child) {
                self.collect_comment_drag_group(&child.id, comment_ids, node_ids);
            }
        }
    }

    pub fn update_comment_drag(&mut self, mouse_pos: Point<f32>, cx: &mut Context<Self>) {
        if let Some(cid) = &self.dragging_comment.clone() {
            let raw = Point::new(mouse_pos.x - self.drag_offset.x, mouse_pos.y - self.drag_offset.y);
            let new_pos = self.snap_comment_position(raw);
            if let Some(ip) = self.initial_comment_drag_positions.get(cid) {
                let delta = Point::new(new_pos.x - ip.x, new_pos.y - ip.y);
                for (id, ip) in &self.initial_comment_drag_positions.clone() {
                    let np = self.snap_comment_position(Point::new(ip.x + delta.x, ip.y + delta.y));
                    if let Some(c) = self.graph.comments.iter_mut().find(|c| c.id == *id) { c.position = np; }
                }
                for (id, ip) in &self.initial_drag_positions.clone() {
                    if let Some(n) = self.graph.nodes.iter_mut().find(|n| n.id == *id) {
                        n.position = crate::rendering::graph::NodeGraphRenderer::snap_to_grid(
                            Point::new(ip.x + delta.x, ip.y + delta.y));
                    }
                }
                cx.notify();
            }
        }
    }

    pub fn end_comment_drag(&mut self, cx: &mut Context<Self>) { self.end_entity_drag(cx); }

    pub fn update_comment_resize(&mut self, mouse_pos: Point<f32>, cx: &mut Context<Self>) {
        use crate::editor::panel::ResizeHandle;
        if let Some((cid, handle)) = &self.resizing_comment.clone() {
            let Some((start_pos, start_size)) = self.resizing_comment_start else {
                return;
            };

            if let Some(c) = self.graph.comments.iter_mut().find(|c| c.id == *cid) {
                let min_width = 100.0;
                let min_height = 50.0;
                let mut left = start_pos.x;
                let mut top = start_pos.y;
                let mut right = start_pos.x + start_size.width;
                let mut bottom = start_pos.y + start_size.height;

                match handle {
                    ResizeHandle::TopLeft => {
                        left = mouse_pos.x;
                        top = mouse_pos.y;
                    }
                    ResizeHandle::TopRight => {
                        right = mouse_pos.x;
                        top = mouse_pos.y;
                    }
                    ResizeHandle::BottomLeft => {
                        left = mouse_pos.x;
                        bottom = mouse_pos.y;
                    }
                    ResizeHandle::BottomRight => {
                        right = mouse_pos.x;
                        bottom = mouse_pos.y;
                    }
                    ResizeHandle::Top => {
                        top = mouse_pos.y;
                    }
                    ResizeHandle::Bottom => {
                        bottom = mouse_pos.y;
                    }
                    ResizeHandle::Left => {
                        left = mouse_pos.x;
                    }
                    ResizeHandle::Right => {
                        right = mouse_pos.x;
                    }
                }

                if right - left < min_width {
                    match handle {
                        ResizeHandle::Left | ResizeHandle::TopLeft | ResizeHandle::BottomLeft => {
                            left = right - min_width;
                        }
                        _ => {
                            right = left + min_width;
                        }
                    }
                }

                if bottom - top < min_height {
                    match handle {
                        ResizeHandle::Top | ResizeHandle::TopLeft | ResizeHandle::TopRight => {
                            top = bottom - min_height;
                        }
                        _ => {
                            bottom = top + min_height;
                        }
                    }
                }

                c.position = Point::new(left, top);
                c.size = Size::new(right - left, bottom - top);
                Self::snap_comment_bounds(c);
                self.drag_offset = mouse_pos;
                cx.notify();
            }
        }
    }

    pub fn end_comment_resize(&mut self, cx: &mut Context<Self>) {
        if self.resizing_comment.is_some() {
            let nodes = self.graph.nodes.clone();
            for comment in self.graph.comments.iter_mut() {
                comment.update_contained_nodes(&nodes);
            }
        }
        self.resizing_comment = None;
        self.resizing_comment_start = None;
        cx.notify();
    }

    pub fn finish_comment_editing(&mut self, cx: &mut Context<Self>) {
        if let Some(cid) = &self.editing_comment.clone() {
            let text = self.comment_text_input.read(cx).text().to_string();
            if let Some(c) = self.graph.comments.iter_mut().find(|c| c.id == *cid) {
                c.text = text;
                self.is_dirty = true;
            }
            self.editing_comment = None; cx.notify();
        }
    }

    pub fn sync_comment_inspector_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(comment_id) = self.graph.selected_comments.first().cloned() else {
            return;
        };

        if let Some(comment) = self.graph.comments.iter().find(|c| c.id == comment_id) {
            let current_text = self.comment_text_input.read(cx).text().to_string();
            if current_text != comment.text {
                self.comment_text_input.update(cx, |input, cx| {
                    input.set_value(comment.text.clone(), window, cx);
                });
            }
        }
    }

    pub fn create_comment_at_center(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::features::viewport::coordinates::screen_to_graph_pos;
        let center = screen_to_graph_pos(Point::new(gpui::px(960.0), gpui::px(540.0)), &self.graph);
        self.add_comment(center, window, cx);
    }

    pub fn add_comment(&mut self, position: Point<f32>, window: &mut Window, cx: &mut Context<Self>) {
        let new_comment = crate::core::types::BlueprintComment::new(self.snap_comment_position(position), window, cx);
        let mut cmd = crate::features::undo::AddCommentCommand::new(new_comment.clone());
        cmd.execute(self, cx);
        self.push_undo_command(crate::features::undo::Command::AddComment(cmd));
        self.comment_color_bindings_dirty = true;
    }

    pub fn copy_selected_entities(&mut self, _cx: &mut Context<Self>) {
        use crate::features::clipboard::ClipboardData;
        if self.graph.selected_nodes.is_empty() && self.graph.selected_comments.is_empty() { return; }
        let data = ClipboardData::from_selection(&self.graph.nodes, &self.graph.comments, &self.graph.connections, &self.graph.selected_nodes, &self.graph.selected_comments);
        if let Ok(json) = data.to_json() { if let Ok(mut cb) = arboard::Clipboard::new() { let _ = cb.set_text(&json); } }
    }

    pub fn paste_entities(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::features::clipboard::ClipboardData;
        let json = match arboard::Clipboard::new().ok().and_then(|mut cb| cb.get_text().ok()) { Some(t) => t, None => return };
        let Ok(data) = ClipboardData::from_json(&json) else { return };
        let mwp = window.mouse_position();
        let mep = crate::rendering::graph::NodeGraphRenderer::window_to_graph_element_pos(mwp, self);
        let mgp = crate::rendering::graph::NodeGraphRenderer::screen_to_graph_pos(mep, &self.graph);
        let mut mn_x = f32::MAX; let mut mn_y = f32::MAX; let mut mx_x = f32::MIN; let mut mx_y = f32::MIN;
        for n in &data.nodes { mn_x = mn_x.min(n.position.0); mn_y = mn_y.min(n.position.1); mx_x = mx_x.max(n.position.0+n.size.0); mx_y = mx_y.max(n.position.1+n.size.1); }
        for c in &data.comments { mn_x = mn_x.min(c.position.0); mn_y = mn_y.min(c.position.1); mx_x = mx_x.max(c.position.0+c.size.0); mx_y = mx_y.max(c.position.1+c.size.1); }
        let off = if mn_x <= mx_x && mn_y <= mx_y { let sc = Point::new((mn_x+mx_x)/2.0,(mn_y+mx_y)/2.0); Point::new(mgp.x-sc.x, mgp.y-sc.y) } else { Point::new(50.0,50.0) };
        let (nodes, comments, conns) = data.to_graph_entities(off, window, cx);
        self.graph.selected_nodes.clear(); self.graph.selected_comments.clear();
        for n in &nodes { self.graph.nodes.push(n.clone()); self.graph.selected_nodes.push(n.id.clone()); }
        for c in &comments { self.graph.comments.push(c.clone()); self.graph.selected_comments.push(c.id.clone()); }
        for conn in &conns { self.graph.connections.push(conn.clone()); }
        self.comment_color_bindings_dirty = true; cx.notify();
    }

    /// Create a Dispatch node for a custom event from its On node's uid.
    pub fn create_custom_event_dispatch_node(
        &mut self,
        uid: String,
        position: Point<f32>,
        cx: &mut Context<Self>,
    ) {
        use crate::core::types::*;

        let on_def_id = format!("custom_event:{}", uid);
        let dispatch_def_id = format!("custom_event_dispatch:{}", uid);

        // Find the On node to copy its output pins
        let on_node = self.graph.nodes.iter().find(|n| n.definition_id == on_def_id);

        let mut dispatch_inputs: Vec<Pin> = vec![Pin {
            id: "exec".to_string(),
            name: String::new(),
            pin_type: PinType::Input,
            data_type: PinDataType::execution(),
        }];

        if let Some(on) = on_node {
            for pin in &on.outputs {
                if pin.data_type.is_execution() {
                    continue;
                }
                dispatch_inputs.push(Pin {
                    id: pin.id.clone(),
                    name: pin.name.clone(),
                    pin_type: PinType::Input,
                    data_type: pin.data_type.clone(),
                });
            }
        }

        let event_name = on_node
            .map(|n| n.title.strip_prefix("On ").unwrap_or(&n.title).to_string())
            .unwrap_or_else(|| uid.clone());

        let node = BlueprintNode {
            id: uuid::Uuid::new_v4().to_string(),
            definition_id: dispatch_def_id,
            title: format!("Dispatch {}", event_name),
            icon: "📡".to_string(),
            node_type: NodeType::CustomEventDispatch,
            position,
            size: Size::new(240.0, 60.0),
            inputs: dispatch_inputs,
            outputs: Vec::new(),
            properties: {
                let mut m = std::collections::HashMap::new();
                m.insert("event_uid".to_string(), uid);
                m
            },
            is_selected: false,
            description: format!("Dispatches the '{}' custom event", event_name),
            color: None,
        };
        self.add_node(node, cx);
    }

    // ── Event Configurator ────────────────────────────────────────────────────

    /// Open the event configurator. If `node_id` is Some, load existing event data
    /// from that node for editing. If None, create fresh state for a new event.
    pub fn open_event_configurator(
        &mut self,
        node_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::editor::panel::EventConfigField;
        use ui::dropdown::{DropdownState, SearchableVec};

        self.event_fields.clear();
        self.field_name_inputs.clear();
        self.field_type_dropdowns.clear();
        self.editing_event_node = node_id.clone();

        if let Some(ref nid) = node_id {
            // Load event data from the shared graph-level definition
            let event_uid = self
                .graph
                .nodes
                .iter()
                .find(|n| n.id == *nid)
                .and_then(|n| n.properties.get("event_uid"));
            let def = event_uid
                .and_then(|uid| self.graph.custom_event_defs.get(uid));

            if let Some(def) = def {
                self.event_name_input.update(cx, |input, cx| {
                    input.set_value(def.name.clone(), window, cx);
                });
                self.event_return_type_input.update(cx, |input, cx| {
                    input.set_value(def.return_type.clone(), window, cx);
                });
                for field in &def.fields {
                    self.event_fields.push(EventConfigField {
                        name: field.name.clone(),
                        type_name: field.type_name.clone(),
                    });
                    let inp = cx.new(|cx| {
                        InputState::new(window, cx).placeholder("Field name...")
                    });
                    inp.update(cx, |input, cx| {
                        input.set_value(field.name.clone(), window, cx);
                    });
                    self.field_name_inputs.push(inp);

                    let type_items: Vec<crate::editor::panel::EventFieldType> = vec![
                        "f32", "f64", "i32", "u32", "i64", "u64", "bool", "String",
                        "Vec3", "Entity",
                    ]
                    .into_iter()
                    .map(|n| crate::editor::panel::EventFieldType { name: n.to_string() })
                    .collect();
                    let dd = cx.new(|cx| {
                        DropdownState::new(SearchableVec::new(type_items), None, window, cx)
                    });
                    dd.update(cx, |state, cx| {
                        state.set_selected_value(&field.type_name, window, cx);
                    });
                    self.field_type_dropdowns.push(dd);
                }
            } else if let Some(node) = self.graph.nodes.iter().find(|n| n.id == *nid) {
                // Fallback: read directly from node outputs (legacy / out of sync)
                self.event_name_input.update(cx, |input, cx| {
                    input.set_value(
                        node.title.trim_start_matches("On ").to_string(),
                        window,
                        cx,
                    );
                });
                for pin in &node.outputs {
                    if pin.data_type.is_execution() {
                        continue;
                    }
                    self.event_fields.push(EventConfigField {
                        name: pin.name.clone(),
                        type_name: pin.data_type.type_name.clone(),
                    });
                    let inp = cx.new(|cx| {
                        InputState::new(window, cx).placeholder("Field name...")
                    });
                    inp.update(cx, |input, cx| {
                        input.set_value(pin.name.clone(), window, cx);
                    });
                    self.field_name_inputs.push(inp);

                    let type_items: Vec<crate::editor::panel::EventFieldType> = vec![
                        "f32", "f64", "i32", "u32", "i64", "u64", "bool", "String",
                        "Vec3", "Entity",
                    ]
                    .into_iter()
                    .map(|n| crate::editor::panel::EventFieldType { name: n.to_string() })
                    .collect();
                    let dd = cx.new(|cx| {
                        DropdownState::new(SearchableVec::new(type_items), None, window, cx)
                    });
                    dd.update(cx, |state, cx| {
                        state.set_selected_value(&pin.data_type.type_name, window, cx);
                    });
                    self.field_type_dropdowns.push(dd);
                }
            }
        } else {
            // Fresh state for a new event
            self.event_name_input.update(cx, |input, cx| {
                input.set_value(String::new(), window, cx);
                input.focus(window, cx);
            });
        }

        self.event_configurator_open = true;
        cx.notify();
    }

    /// Save the event configurator — store the shared definition and sync all
    /// On / Dispatch nodes that reference this event.
    pub fn save_event_configurator(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::core::graph::CustomEventField;
        use crate::core::types::PinDataType as DataType;
        use crate::core::types::*;

        let event_name = self.event_name_input.read(cx).text().to_string();
        if event_name.is_empty() {
            return;
        }
        let return_type = self.event_return_type_input.read(cx).text().to_string();

        // Compute uid from event name (kebab-case)
        let uid: String = event_name
            .chars()
            .map(|c| {
                if c.is_uppercase() {
                    format!("-{}", c.to_ascii_lowercase())
                } else if c == ' ' {
                    "-".to_string()
                } else {
                    c.to_string()
                }
            })
            .collect::<String>()
            .trim_start_matches('-')
            .to_string();
        let on_def_id = format!("custom_event:{}", uid);
        let dispatch_def_id = format!("custom_event_dispatch:{}", uid);

        // Capture old uid from the node being edited
        let old_uid = self.editing_event_node.as_ref().and_then(|nid| {
            self.graph.nodes.iter().find(|n| n.id == *nid)
                .and_then(|n| n.properties.get("event_uid").cloned())
        });

        // Build the shared definition
        let fields: Vec<CustomEventField> = self
            .event_fields
            .iter()
            .map(|f| CustomEventField {
                name: f.name.clone(),
                type_name: f.type_name.clone(),
            })
            .collect();
        let def = crate::core::graph::CustomEventDef {
            name: event_name.clone(),
            uid: uid.clone(),
            return_type: return_type.clone(),
            fields: fields.clone(),
        };

        // Remove old uid entry if renamed
        if let Some(ref old) = old_uid {
            if old != &uid {
                self.graph.custom_event_defs.remove(old);
            }
        }
        self.graph.custom_event_defs.insert(uid.clone(), def);

        // Build pins from fields
        let mut output_pins = vec![Pin {
            id: "Body".to_string(),
            name: "Body".to_string(),
            pin_type: PinType::Output,
            data_type: DataType::execution(),
        }];
        let mut dispatch_input_pins = vec![Pin {
            id: "exec".to_string(),
            name: String::new(),
            pin_type: PinType::Input,
            data_type: DataType::execution(),
        }];
        for field in &fields {
            let pin_id = field.name.clone();
            let data_type = DataType::from_type_str(&field.type_name);
            output_pins.push(Pin {
                id: pin_id.clone(),
                name: field.name.clone(),
                pin_type: PinType::Output,
                data_type: data_type.clone(),
            });
            dispatch_input_pins.push(Pin {
                id: pin_id.clone(),
                name: field.name.clone(),
                pin_type: PinType::Input,
                data_type,
            });
        }

        // Update or create the node being edited
        if let Some(ref nid) = self.editing_event_node {
            if let Some(node) = self.graph.nodes.iter_mut().find(|n| n.id == *nid) {
                node.title = format!("On {}", event_name);
                node.outputs = output_pins.clone();
                node.definition_id = on_def_id;
                node.properties.insert("event_uid".to_string(), uid.clone());
            }
        } else {
            self.graph.nodes.push(BlueprintNode {
                id: uuid::Uuid::new_v4().to_string(),
                definition_id: on_def_id,
                title: format!("On {}", event_name),
                icon: "📡".to_string(),
                node_type: NodeType::CustomEvent,
                position: Point::new(200.0, 200.0),
                size: Size::new(240.0, 60.0),
                inputs: Vec::new(),
                outputs: output_pins.clone(),
                properties: {
                    let mut m = std::collections::HashMap::new();
                    m.insert("event_uid".to_string(), uid.clone());
                    m
                },
                is_selected: false,
                description: format!("Custom event listener for '{}'", uid),
                color: None,
            });
        }

        // Update Dispatch nodes matching this event uid (old + new if renamed)
        let dispatch_ids: Vec<String> = self
            .graph
            .nodes
            .iter()
            .filter(|n| {
                let matches = n.definition_id == dispatch_def_id
                    || old_uid.as_ref().map_or(false, |old| {
                        n.definition_id == format!("custom_event_dispatch:{}", old)
                    });
                matches
            })
            .map(|n| n.id.clone())
            .collect();

        if dispatch_ids.is_empty() && self.editing_event_node.is_none() {
            // Auto-create a dispatch node for a brand-new event
            self.graph.nodes.push(BlueprintNode {
                id: uuid::Uuid::new_v4().to_string(),
                definition_id: dispatch_def_id,
                title: format!("Dispatch {}", event_name),
                icon: "📡".to_string(),
                node_type: NodeType::CustomEventDispatch,
                position: Point::new(500.0, 200.0),
                size: Size::new(240.0, 60.0),
                inputs: dispatch_input_pins,
                outputs: Vec::new(),
                properties: {
                    let mut m = std::collections::HashMap::new();
                    m.insert("event_uid".to_string(), uid.clone());
                    m
                },
                is_selected: false,
                description: format!("Dispatch the '{}' custom event", uid),
                color: None,
            });
        } else {
            for id in &dispatch_ids {
                if let Some(node) = self.graph.nodes.iter_mut().find(|n| n.id == *id) {
                    node.inputs = dispatch_input_pins.clone();
                    node.title = format!("Dispatch {}", event_name);
                    node.definition_id = dispatch_def_id.clone();
                    node.properties.insert("event_uid".to_string(), uid.clone());
                }
            }
        }

        self.is_dirty = true;
        self.event_configurator_open = false;
        self.editing_event_node = None;

        // Refresh palette so new/updated Dispatch entries appear
        let palette = self.quick_palette_view.clone();
        cx.defer(move |cx| {
            palette.update(cx, |view, cx| view.rebuild_items(cx));
        });

        cx.notify();
    }

    /// Cancel the event configurator without saving.
    pub fn cancel_event_configurator(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.event_configurator_open = false;
        self.editing_event_node = None;
        self.event_fields.clear();
        cx.notify();
    }

    /// Add a new empty field to the event configurator.
    pub fn add_event_field(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::editor::panel::EventConfigField;
        use ui::dropdown::{DropdownState, SearchableVec};

        let idx = self.event_fields.len();
        self.event_fields.push(EventConfigField {
            name: format!("field_{}", idx + 1),
            type_name: "f32".to_string(),
        });

        let inp = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Field name...")
        });
        inp.update(cx, |input, cx| {
            input.set_value(format!("field_{}", idx + 1), window, cx);
        });
        self.field_name_inputs.push(inp);

        let type_items: Vec<crate::editor::panel::EventFieldType> = vec![
            "f32", "f64", "i32", "u32", "i64", "u64", "bool", "String",
            "Vec3", "Entity",
        ]
        .into_iter()
        .map(|n| crate::editor::panel::EventFieldType { name: n.to_string() })
        .collect();
        let dd = cx.new(|cx| {
            DropdownState::new(SearchableVec::new(type_items), None, window, cx)
        });
        self.field_type_dropdowns.push(dd);

        cx.notify();
    }

    /// Remove a field from the event configurator by index.
    pub fn remove_event_field(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        if index < self.event_fields.len() {
            self.event_fields.remove(index);
            self.field_name_inputs.remove(index);
            self.field_type_dropdowns.remove(index);
            cx.notify();
        }
    }
}
