//! The per-tab canvas panel: a Graphy canvas plus Blueprint-only state.
//!
//! The `GraphCanvas` entity owns the tab's document, view, selection and input.
//! This panel adds what is Blueprint's: the undo history, context menus, the
//! quick palette, comment title editing, and debug state. Every edit is a batch
//! of Graphy commands; the canvas reports it as `CanvasEvent::Edited`, which is
//! where undo, dirty state and validation hook in.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use gpui::*;
use graphy::{GraphCommand, GraphDescription, NodeInstance, Position};
use graphy_gpui::canvas::{CanvasEvent, GraphCanvas};
use graphy_gpui::{Action, GraphProfile as _, MenuTarget};
use graphy_render::Vec2;
use ui::dock::{Panel, PanelEvent};
use ui::input::{InputEvent, InputState};
use ui_common::reflected_properties_panel::PropertyStateManager;

use crate::canvas_profile::{
    BlueprintProfile, SharedProfileData, ADD_CUSTOM_EVENT, MENU_BREAKPOINT, MENU_COLLAPSE,
};
use crate::editor::panel::BlueprintEditorPanel;
use crate::editor::tabs::TabDoc;
use crate::features::undo::Command;
use crate::io::canvas_doc::{self, AuthoredSidecar};

/// Grid the canvas snaps nodes and comments to.
pub const GRID: f64 = 10.0;

fn snap(v: f64) -> f64 {
    (v / GRID).round() * GRID
}

/// One open tab's canvas.
pub struct GraphCanvasPanel {
    pub id: String,
    pub name: String,
    pub is_main: bool,
    pub is_dirty: bool,
    pub is_library_macro: bool,
    pub library_id: Option<String>,

    /// Read-only reference to the shell editor for shared data + cross-tab events.
    pub panel: WeakEntity<BlueprintEditorPanel>,

    /// The Graphy canvas: the only owner of this tab's graph.
    pub canvas: Entity<GraphCanvas>,
    /// What the authored schema carries that the document has no slot for.
    pub sidecar: AuthoredSidecar,
    pub bp_profile: Arc<BlueprintProfile>,
    /// Subgraph/library/event/running data the profile reads.
    pub profile_data: SharedProfileData,
    /// This graph's undo history, registered with the host once the editor
    /// knows its document.
    pub history: plugin_editor_api::SharedHistory<Command>,
    pub focus_handle: FocusHandle,

    // ── Runtime / debug ────────────────────────────────────────────────────
    pub breakpoints: HashSet<String>,
    pub debug_session: Option<crate::features::debug::DebugSession>,
    /// Latest Play-in-Editor values keyed by opaque source node and pin ids.
    pub debug_pin_values: HashMap<(String, String), String>,

    // ── Comments ───────────────────────────────────────────────────────────
    /// Index of the comment whose title is being edited.
    pub editing_comment: Option<usize>,
    pub comment_text_input: Entity<InputState>,
    /// Colour pickers shown in the inspector, by comment id.
    pub comment_pickers: HashMap<String, Entity<ui::color_picker::ColorPickerState>>,
    pub pin_property_state: PropertyStateManager,

    // ── Sidebar drags ──────────────────────────────────────────────────────
    pub dragging_variable: Option<crate::features::variables::VariableDrag>,
    pub variable_drop_menu_position: Option<Point<f32>>,
    pub dragging_macro: Option<crate::features::macros::MacroDrag>,
    pub macro_pin_add_mode: Option<bool>,

    /// Edits made by the editor itself (interface sync) that must not become
    /// undo steps; counted down as their `Edited` events arrive.
    silent_edits: usize,

    pub subscriptions: Vec<Subscription>,
}

impl GraphCanvasPanel {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        panel: WeakEntity<BlueprintEditorPanel>,
        id: String,
        name: String,
        is_main: bool,
        doc: TabDoc,
        profile_data: SharedProfileData,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let TabDoc {
            graph,
            sidecar,
            view,
        } = doc;
        let bp_profile = Arc::new(
            BlueprintProfile::blueprint(profile_data.clone())
                .editing_macro((!is_main).then(|| id.clone())),
        );
        let canvas = {
            let bp_profile = bp_profile.clone();
            cx.new(|cx| {
                let mut canvas = GraphCanvas::new(
                    graph,
                    bp_profile,
                    graphy_gpui::canvas::system_fonts(),
                    crate::theme::graph_theme(cx),
                    cx,
                )
                    .expect("the bundled canvas font parses");
                if let Some(((pan_x, pan_y), zoom)) = view {
                    canvas.set_view(Vec2::new(pan_x, pan_y), zoom, cx);
                }
                canvas
            })
        };
        cx.subscribe_in(&canvas, window, Self::on_canvas_event)
            .detach();

        let comment_text_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Comment text..."));
        cx.subscribe_in(
            &comment_text_input,
            window,
            |this, _state: &Entity<InputState>, event: &InputEvent, _window, cx| {
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
            canvas,
            sidecar,
            bp_profile,
            profile_data,
            history: crate::features::undo::new_history(),
            focus_handle: cx.focus_handle(),
            breakpoints: HashSet::new(),
            debug_session: None,
            debug_pin_values: HashMap::new(),
            editing_comment: None,
            comment_text_input,
            comment_pickers: HashMap::new(),
            pin_property_state: PropertyStateManager::new(),
            dragging_variable: None,
            variable_drop_menu_position: None,
            dragging_macro: None,
            macro_pin_add_mode: None,
            silent_edits: 0,
            subscriptions: Vec::new(),
        }
    }

    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    // ── The document ───────────────────────────────────────────────────────

    /// This tab's graph.
    pub fn graph<'a>(&self, cx: &'a App) -> &'a GraphDescription {
        self.canvas.read(cx).graph()
    }

    pub fn selected_nodes(&self, cx: &App) -> Vec<String> {
        self.canvas
            .read(cx)
            .controller()
            .selection
            .nodes
            .iter()
            .cloned()
            .collect()
    }

    pub fn selected_comments(&self, cx: &App) -> Vec<usize> {
        self.canvas
            .read(cx)
            .controller()
            .selection
            .comments
            .iter()
            .copied()
            .collect()
    }

    /// Applies commands to the document as one undoable edit. The history, dirty
    /// state and validation hear about it through the canvas's `Edited` event.
    pub fn apply(
        &mut self,
        label: &'static str,
        commands: Vec<GraphCommand>,
        cx: &mut Context<Self>,
    ) -> Result<(), graphy::EditError> {
        if commands.is_empty() {
            return Ok(());
        }
        self.canvas
            .update(cx, |canvas, cx| canvas.apply(label, &commands, cx))
    }

    /// Applies commands the editor itself generates (macro interface sync): they
    /// mark the graph changed but are not undo steps.
    pub fn apply_system(
        &mut self,
        label: &'static str,
        commands: Vec<GraphCommand>,
        cx: &mut Context<Self>,
    ) {
        if commands.is_empty() {
            return;
        }
        self.silent_edits += 1;
        if self.apply(label, commands, cx).is_err() {
            self.silent_edits -= 1;
        }
    }

    /// Replaces the whole document as one undoable edit (AI edits, reverts).
    pub fn replace_graph(
        &mut self,
        label: &'static str,
        after: &GraphDescription,
        cx: &mut Context<Self>,
    ) -> Result<(), graphy::EditError> {
        let commands = graphy::diff_commands(self.graph(cx), after);
        self.apply(label, commands, cx)
    }

    /// The tab's authored graph for saving, compiling and AI snapshots. Comment
    /// containment is recomputed from the laid-out geometry.
    pub fn authored_graph(&self, cx: &App) -> graphy_blueprint::GraphDescription {
        let canvas = self.canvas.read(cx);
        let mut doc = canvas.graph().clone();
        let scene = canvas.scene();
        for (comment, placed) in doc.comments.iter_mut().zip(&scene.comments) {
            comment.contained_node_ids = scene
                .nodes
                .iter()
                .filter(|n| {
                    let r = n.rect();
                    r.min.x >= placed.rect.min.x
                        && r.min.y >= placed.rect.min.y
                        && r.max.x <= placed.rect.max.x
                        && r.max.y <= placed.rect.max.y
                })
                .map(|n| n.id.clone())
                .collect();
        }
        canvas_doc::to_authored(&doc, &self.sidecar)
    }

    /// Saved `(pan, zoom)` of the view.
    pub fn view_state(&self, cx: &App) -> ((f32, f32), f32) {
        let (pan, zoom) = self.canvas.read(cx).view();
        ((pan.x, pan.y), zoom)
    }

    // ── Canvas events ──────────────────────────────────────────────────────

    fn on_canvas_event(
        &mut self,
        _canvas: &Entity<GraphCanvas>,
        event: &CanvasEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            CanvasEvent::Edited {
                label,
                commands,
                changes,
            } => {
                if self.silent_edits > 0 {
                    self.silent_edits -= 1;
                } else {
                    self.push_undo_command(Command::new(*label, commands.clone(), changes.clone()));
                }
                self.notify_editor_changed(cx);
                cx.notify();
            }
            CanvasEvent::Rejected(why) => tracing::warn!("graph edit rejected: {why}"),
            CanvasEvent::SelectionChanged => {
                self.clear_sidebar_selections(cx);
                self.sync_comment_inspector_state(window, cx);
                self.notify_editor_view(cx);
                cx.notify();
            }
            CanvasEvent::ViewChanged => {}
            CanvasEvent::Request(action) => self.on_canvas_request(action.clone(), window, cx),
            CanvasEvent::NodePicked {
                definition,
                graph_pos,
                source,
            } => self.place_picked(definition, *graph_pos, source.clone(), window, cx),
        }
    }

    fn on_canvas_request(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            Action::OpenGraph(target) => {
                let panel = self.panel.clone();
                let window_handle = window.window_handle();
                cx.defer(move |cx| {
                    let _ = cx.update_window(window_handle, |_, window, cx| {
                        if let Some(panel) = panel.upgrade() {
                            panel.update(cx, |panel, cx| {
                                panel.open_macro_from_instance(&target.graph_id, window, cx);
                            });
                        }
                    });
                });
            }
            Action::NodeActivated(id) => self.node_activated(&id, cx),
            Action::CommentActivated(index) => self.start_comment_title_editing(index, window, cx),
            Action::Custom { id, target } => self.run_menu_command(&id, target, window, cx),
            Action::ConnectionRefused(why) => tracing::debug!("connection refused: {why}"),
            _ => {}
        }
    }

    /// Double-click on a custom event node: jump between dispatch and handler,
    /// or select the event in the sidebar.
    fn node_activated(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(node) = self.graph(cx).nodes.get(id).cloned() else {
            return;
        };
        let uid = node
            .properties
            .get("event_uid")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        match crate::core::types::NodeType::of(&node.node_type) {
            crate::core::types::NodeType::CustomEventDispatch => {
                if let Some(uid) = uid {
                    self.animate_pan_to_node_by_def_id(&format!("custom_event:{uid}"), cx);
                }
            }
            crate::core::types::NodeType::CustomEvent => {
                if let (Some(uid), Some(panel)) = (uid, self.panel.upgrade()) {
                    panel.update(cx, |panel, cx| {
                        if let Some(idx) = panel.local_event_defs.iter().position(|d| d.uid == uid)
                        {
                            panel.selected_event = Some(idx);
                            cx.notify();
                        }
                    });
                }
            }
            _ => {}
        }
    }

    pub(crate) fn is_collapsed_graph_node(&self, id: &str, cx: &App) -> bool {
        let Some(node) = self.graph(cx).nodes.get(id) else {
            return false;
        };
        let data = self.profile_data.read().unwrap_or_else(|e| e.into_inner());
        crate::core::subgraph_ref::SubGraphReference::decode(&node.node_type, &data.subgraphs)
            .is_some_and(|r| r.kind == graphy_blueprint::SubGraphKind::Collapsed)
    }

    /// Runs a Blueprint entry of a node menu (the profile lists them).
    fn run_menu_command(
        &mut self,
        id: &str,
        target: MenuTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match (id, &target) {
            (MENU_BREAKPOINT, MenuTarget::Node(node)) => self.toggle_breakpoint(node.clone(), cx),
            (MENU_COLLAPSE, MenuTarget::Node(node)) => {
                if self.is_collapsed_graph_node(node, cx) {
                    self.expand_collapsed_node(node.clone(), window, cx);
                } else {
                    self.collapse_selected_nodes(cx);
                }
            }
            _ => {}
        }
        cx.notify();
    }

    /// Adds the node picked in the canvas's picker where it was opened, wired to
    /// the dragged wire's source if there was one.
    fn place_picked(
        &mut self,
        definition: &str,
        at: Vec2,
        source: Option<graphy::PinRef>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if definition == ADD_CUSTOM_EVENT {
            self.add_custom_event_from_picker(window, cx);
            return;
        }
        self.add_definition_node(definition, at, source, cx);
    }

    /// The picker's "Add Custom Event": creates the event and its On node.
    pub fn add_custom_event_from_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let panel = self.panel.clone();
        let canvas_id = self.id.clone();
        let window_handle = window.window_handle();
        cx.defer(move |cx| {
            let _ = cx.update_window(window_handle, |_, _, cx| {
                if let Some(panel) = panel.upgrade() {
                    panel.update(cx, |panel, cx| {
                        let uid = panel.create_event_def("NewEvent".to_string(), String::new());
                        panel.sync_event_to_tab(&uid, &canvas_id, cx);
                        cx.notify();
                    });
                }
            });
        });
    }

    // ── Editor notifications ───────────────────────────────────────────────

    fn notify_editor_changed(&mut self, cx: &mut Context<Self>) {
        self.is_dirty = true;
        let id = self.id.clone();
        if let Some(panel) = self.panel.upgrade() {
            panel.update(cx, |panel, cx| panel.on_graph_edited(&id, cx));
        }
    }

    fn notify_editor_view(&self, cx: &mut Context<Self>) {
        if let Some(panel) = self.panel.upgrade() {
            panel.update(cx, |_, cx| cx.notify());
        }
    }

    /// Clear sidebar selections (variable, macro, event, prefab) on the parent
    /// panel for mutual exclusivity with graph selections.
    pub fn clear_sidebar_selections(&self, cx: &mut Context<Self>) {
        if let Some(panel) = self.panel.upgrade() {
            panel.update(cx, |panel, cx| {
                panel.clear_sidebar_selections(false, false, false, false);
                cx.notify();
            });
        }
    }

    /// Read the shell editor's library manager (shared data).
    pub fn library_manager(&self, cx: &App) -> Option<graphy_blueprint::LibraryManager> {
        self.panel
            .upgrade()
            .map(|p| p.read(cx).library_manager.clone())
    }

    // ── Runtime ────────────────────────────────────────────────────────────

    /// Replace the current runtime execution set used by node highlighting.
    pub fn set_running_nodes<I, S>(&mut self, node_ids: I, cx: &mut Context<Self>)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        {
            let mut data = self.profile_data.write().unwrap_or_else(|e| e.into_inner());
            data.running = node_ids
                .into_iter()
                .map(|id| id.as_ref().to_string())
                .collect();
        }
        self.canvas.update(cx, |canvas, cx| canvas.refresh_scene(cx));
        cx.notify();
    }

    /// Re-presents every node after profile-side data (subgraph names, event
    /// names) changed.
    pub fn refresh_presentation(&mut self, cx: &mut Context<Self>) {
        self.canvas.update(cx, |canvas, cx| canvas.refresh_scene(cx));
    }

    // ── Nodes ──────────────────────────────────────────────────────────────

    /// Adds a node, snapped to the grid.
    pub fn add_node(&mut self, mut node: NodeInstance, cx: &mut Context<Self>) {
        node.position = Position::new(snap(node.position.x), snap(node.position.y));
        let _ = self.apply("Add node", vec![GraphCommand::AddNode(node)], cx);
    }

    /// The node a palette entry `definition` stands for: a catalogue node, a
    /// macro call, or a custom event dispatch.
    fn palette_node(&self, definition: &str, pos: Position, cx: &App) -> Option<NodeInstance> {
        let id = uuid::Uuid::new_v4().to_string();
        if let Some(uid) = definition.strip_prefix("custom_event_dispatch:") {
            let inputs = self.panel.upgrade().map_or_else(Vec::new, |panel| {
                panel.read(cx).find_event_def(uid).map_or_else(
                    || {
                        vec![crate::features::macros::operations::pin_instance(
                            "exec",
                            "",
                            graphy::DataType::Exec,
                            graphy::PinType::Input,
                        )]
                    },
                    BlueprintEditorPanel::event_dispatch_input_pins,
                )
            });
            let mut node = NodeInstance::new(id, definition, pos);
            node.inputs = inputs;
            node.set_property("event_uid", uid.to_string());
            return Some(node);
        }
        let reference = {
            let data = self.profile_data.read().unwrap_or_else(|e| e.into_inner());
            crate::core::subgraph_ref::SubGraphReference::decode(definition, &data.subgraphs)
                .and_then(|reference| {
                    data.subgraphs
                        .iter()
                        .find(|s| s.id == reference.id && s.kind == graphy_blueprint::SubGraphKind::Macro)
                        .cloned()
                })
        };
        if let Some(macro_def) = reference {
            let (inputs, outputs) = crate::features::macros::operations::call_node_pins(&macro_def);
            let mut node = NodeInstance::new(
                id,
                crate::core::subgraph_ref::SubGraphReference::new(
                    macro_def.id.clone(),
                    graphy_blueprint::SubGraphKind::Macro,
                )
                .encode(),
                pos,
            );
            node.inputs = inputs;
            node.outputs = outputs;
            return Some(node);
        }
        self.bp_profile.create_node(definition, &id, pos)
    }

    /// Centre of the visible canvas, in graph units.
    pub fn view_center(&self, cx: &App) -> Vec2 {
        let canvas = self.canvas.read(cx);
        let (pan, zoom) = canvas.view();
        let vp = canvas.viewport();
        Vec2::new(vp.x * 0.5 / zoom - pan.x, vp.y * 0.5 / zoom - pan.y)
    }

    /// Adds the node a palette entry stands for at a graph position, optionally
    /// wiring it to the pin a connection was dragged from. Returns its id.
    pub fn add_definition_node(
        &mut self,
        definition: &str,
        at: Vec2,
        source: Option<graphy::PinRef>,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let pos = Position::new(snap(f64::from(at.x)), snap(f64::from(at.y)));
        let node = self.palette_node(definition, pos, cx)?;
        let id = node.id.clone();
        let mut commands = vec![GraphCommand::AddNode(node.clone())];
        if let Some(source) = source {
            let mut wiring =
                graphy_gpui::connect_new_node_commands(&*self.bp_profile, self.graph(cx), &source, &node);
            for command in &mut wiring {
                if let GraphCommand::Connect(conn) = command {
                    conn.id = Some(uuid::Uuid::new_v4().to_string());
                }
            }
            commands.extend(wiring);
        }
        self.apply("Add node", commands, cx).ok()?;
        Some(id)
    }

    /// Sets (or, for `null`, removes) a node property from an editor value.
    pub fn update_node_input_property(
        &mut self,
        node_id: impl AsRef<str>,
        pin_id: impl AsRef<str>,
        value: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let value = match value {
            serde_json::Value::Null => None,
            serde_json::Value::String(text) => Some(canvas_doc::property_value(&text)),
            other => Some(other),
        };
        let _ = self.apply(
            "Edit property",
            vec![GraphCommand::SetProperty {
                node: node_id.as_ref().to_string(),
                key: pin_id.as_ref().to_string(),
                value,
            }],
            cx,
        );
    }

    // ── View ───────────────────────────────────────────────────────────────

    /// Centres the view on `node_id`.
    pub fn animate_pan_to_node(&mut self, node_id: &str, cx: &mut Context<Self>) {
        self.canvas.update(cx, |canvas, cx| {
            canvas.center_on(node_id, cx);
        });
    }

    /// Centres the view on the first node of type `definition_id`.
    pub fn animate_pan_to_node_by_def_id(&mut self, definition_id: &str, cx: &mut Context<Self>) {
        let id = self
            .graph(cx)
            .nodes
            .values()
            .find(|n| n.node_type == definition_id)
            .map(|n| n.id.clone());
        if let Some(id) = id {
            self.animate_pan_to_node(&id, cx);
        }
    }

    // ── Undo / redo ────────────────────────────────────────────────────────

    /// Undo the last operation
    pub fn undo(&mut self, cx: &mut Context<Self>) {
        let history = self.history.clone();
        let canvas = self.canvas.clone();
        let undone = plugin_editor_api::undo::lock(&history)
            .undo(|command| {
                canvas.update(cx, |canvas, cx| canvas.undo_changes(&command.changes, cx));
                true
            })
            .is_some();
        if undone {
            self.notify_editor_changed(cx);
            cx.notify();
        }
    }

    /// Redo the last undone operation
    pub fn redo(&mut self, cx: &mut Context<Self>) {
        let history = self.history.clone();
        let canvas = self.canvas.clone();
        let redone = plugin_editor_api::undo::lock(&history)
            .redo(|command| {
                canvas
                    .update(cx, |canvas, cx| canvas.redo_commands(&command.commands, cx))
                    .is_ok()
            })
            .is_some();
        if redone {
            self.notify_editor_changed(cx);
            cx.notify();
        }
    }

    /// Push a command to the undo stack
    pub fn push_undo_command(&mut self, command: Command) {
        let mut history = plugin_editor_api::undo::lock(&self.history);
        history.push(command);
        history.seal();
        self.is_dirty = true;
    }

    pub fn can_undo(&self) -> bool {
        plugin_editor_api::undo::lock(&self.history).can_undo()
    }

    pub fn can_redo(&self) -> bool {
        plugin_editor_api::undo::lock(&self.history).can_redo()
    }

    // ── Comments ───────────────────────────────────────────────────────────

    /// Adds a comment at a graph position; around the selected nodes if any.
    pub fn add_comment(&mut self, at: Vec2, cx: &mut Context<Self>) {
        let mut comment = graphy::core::GraphComment::new(
            "Comment",
            Position::new(snap(f64::from(at.x)), snap(f64::from(at.y))),
            (300.0, 200.0),
        );
        comment.id = uuid::Uuid::new_v4().to_string();
        comment.color = [0.2, 0.4, 0.8, 0.35];
        let selected = self.selected_nodes(cx);
        if !selected.is_empty() {
            const PADDING: f32 = 30.0;
            let canvas = self.canvas.read(cx);
            let rects: Vec<_> = selected
                .iter()
                .filter_map(|id| canvas.scene().node(id))
                .map(|n| n.rect())
                .collect();
            if !rects.is_empty() {
                let left = rects.iter().map(|r| r.min.x).fold(f32::INFINITY, f32::min) - PADDING;
                let top = rects.iter().map(|r| r.min.y).fold(f32::INFINITY, f32::min) - PADDING;
                let right =
                    rects.iter().map(|r| r.max.x).fold(f32::NEG_INFINITY, f32::max) + PADDING;
                let bottom =
                    rects.iter().map(|r| r.max.y).fold(f32::NEG_INFINITY, f32::max) + PADDING;
                comment.position = Position::new(snap(f64::from(left)), snap(f64::from(top)));
                comment.size = (
                    snap(f64::from(right - left)).max(100.0),
                    snap(f64::from(bottom - top)).max(50.0),
                );
                comment.contained_node_ids = selected;
            }
        }
        let index = self.graph(cx).comments.len();
        let _ = self.apply(
            "Add comment",
            vec![GraphCommand::AddComment { index, comment }],
            cx,
        );
    }

    /// Adds a comment at the centre of the visible canvas.
    pub fn create_comment_at_center(&mut self, cx: &mut Context<Self>) {
        let canvas = self.canvas.read(cx);
        let (pan, zoom) = canvas.view();
        let vp = canvas.viewport();
        let centre = Vec2::new(vp.x * 0.5 / zoom - pan.x, vp.y * 0.5 / zoom - pan.y);
        self.add_comment(centre, cx);
    }

    pub fn start_comment_title_editing(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = self.graph(cx).comments.get(index).map(|c| c.text.clone()) else {
            return;
        };
        self.editing_comment = Some(index);
        self.comment_text_input
            .update(cx, |input, cx| input.set_value(text, window, cx));
        let handle = self.comment_text_input.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
        cx.notify();
    }

    pub fn finish_comment_editing(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.editing_comment.take() else {
            return;
        };
        let text = self.comment_text_input.read(cx).text().to_string();
        self.set_comment_text(index, text, cx);
        cx.notify();
    }

    pub fn set_comment_text(&mut self, index: usize, text: String, cx: &mut Context<Self>) {
        if self
            .graph(cx)
            .comments
            .get(index)
            .is_some_and(|c| c.text != text)
        {
            let _ = self.apply(
                "Edit comment",
                vec![GraphCommand::EditComment {
                    index,
                    text: Some(text),
                    color: None,
                    contained_node_ids: None,
                }],
                cx,
            );
        }
    }

    pub fn set_comment_color(&mut self, index: usize, color: [f32; 4], cx: &mut Context<Self>) {
        if self
            .graph(cx)
            .comments
            .get(index)
            .is_some_and(|c| c.color != color)
        {
            let _ = self.apply(
                "Edit comment colour",
                vec![GraphCommand::EditComment {
                    index,
                    text: None,
                    color: Some(color),
                    contained_node_ids: None,
                }],
                cx,
            );
        }
    }

    /// Keeps the inspector's comment text field in step with the selection.
    pub fn sync_comment_inspector_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(&index) = self.selected_comments(cx).first() else {
            return;
        };
        let Some(text) = self.graph(cx).comments.get(index).map(|c| c.text.clone()) else {
            return;
        };
        if self.comment_text_input.read(cx).text().to_string() != text {
            self.comment_text_input
                .update(cx, |input, cx| input.set_value(text, window, cx));
        }
    }

    // ── Clipboard ──────────────────────────────────────────────────────────

    /// Copies the selected nodes to the system clipboard.
    pub fn copy_selected_entities(&mut self, cx: &mut Context<Self>) {
        let selected: std::collections::BTreeSet<String> =
            self.selected_nodes(cx).into_iter().collect();
        if let Some(text) = graphy_gpui::clipboard::copy(self.graph(cx), &selected) {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }
}

impl EventEmitter<PanelEvent> for GraphCanvasPanel {}

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

impl GraphCanvasPanel {
    /// Whether wires are drawn as right-angle routes.
    pub fn angular_wires(&self) -> bool {
        self.profile_data
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .angular_wires
    }

    pub fn toggle_angular_wires(&mut self, cx: &mut Context<Self>) {
        {
            let mut data = self.profile_data.write().unwrap_or_else(|e| e.into_inner());
            data.angular_wires = !data.angular_wires;
        }
        self.canvas.update(cx, |canvas, cx| canvas.refresh_scene(cx));
        cx.notify();
    }
}
