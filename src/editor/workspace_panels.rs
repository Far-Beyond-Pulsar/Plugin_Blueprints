//! Dedicated panel components for the workspace docking system
//!
//! These panels wrap the editor entity and render specific content.

use gpui::*;
use ui::{
    dock::{Panel, PanelEvent},
    h_flex, v_flex, ActiveTheme,
};

use crate::editor::panel::BlueprintEditorPanel;
use crate::features::macros::panel::MacrosRenderer;
use crate::features::prefabs::panel::{PrefabHierarchyRenderer, PrefabPropertiesRenderer};
use crate::features::variables::rendering::VariablesRenderer;
use crate::rendering::graph::NodeGraphRenderer;
use crate::ui_components::palette_view::NodePaletteView;
use crate::ui_components::properties::PropertiesRenderer;

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
                .child(editor.update(cx, |editor, cx| PropertiesRenderer::render(editor, cx)))
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

/// Main graph canvas panel with tab bar
pub struct GraphCanvasPanel {
    editor: WeakEntity<BlueprintEditorPanel>,
    tab_id: String,
    focus_handle: FocusHandle,
}

impl GraphCanvasPanel {
    pub fn new(
        editor: WeakEntity<BlueprintEditorPanel>,
        tab_id: String,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            editor,
            tab_id,
            focus_handle: cx.focus_handle(),
        }
    }
}

impl EventEmitter<PanelEvent> for GraphCanvasPanel {}

impl Render for GraphCanvasPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(editor) = self.editor.upgrade() {
            div().size_full().child(editor.update(cx, |editor, cx| {
                editor.ensure_active_graph_panel_state(&self.tab_id);
                NodeGraphRenderer::render(editor, &self.tab_id, cx)
            }))
        } else {
            div().child("Editor not available")
        }
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

    fn title(&self, _window: &Window, cx: &App) -> AnyElement {
        if let Some(editor) = self.editor.upgrade() {
            let editor = editor.read(cx);
            if let Some(tab) = editor.open_tabs.iter().find(|tab| tab.id == self.tab_id) {
                let title = if tab.is_dirty {
                    format!("{} *", tab.name)
                } else {
                    tab.name.clone()
                };
                return title.into_any_element();
            }
        }

        "Event Graph".into_any_element()
    }
}
