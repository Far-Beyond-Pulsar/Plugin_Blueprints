//! Dedicated panel components for the workspace docking system
//!
//! These panels wrap the editor entity and render specific content.

use gpui::*;
use std::collections::HashSet;
use ui::{
    dock::{Panel, PanelEvent},
    h_flex,
    input::{InputEvent, InputState},
    v_flex, ActiveTheme, PixelsExt,
};

use crate::editor::graph_canvas_panel::GraphCanvasPanel;
use crate::editor::panel::BlueprintEditorPanel;
use crate::features::events::panel::EventsRenderer;
use crate::features::macros::panel::MacrosRenderer;
use crate::features::prefabs::panel::PrefabHierarchyRenderer;
use crate::features::traits::ImplementedTraitsRenderer;
use crate::features::variables::rendering::VariablesRenderer;
use crate::ui_components::asset_inspector::{AssetInspectorRenderer, AssetInspectorSnapshot};
use crate::ui_components::palette_view::NodePaletteView;
use crate::ui_components::properties::PropertiesRenderer;

macro_rules! editor_panel_shell {
    ($panel:ident, $panel_name:literal, $title:literal) => {
        pub struct $panel {
            editor: WeakEntity<BlueprintEditorPanel>,
            focus_handle: FocusHandle,
        }

        impl $panel {
            pub fn new(editor: WeakEntity<BlueprintEditorPanel>, cx: &mut Context<Self>) -> Self {
                Self {
                    editor,
                    focus_handle: cx.focus_handle(),
                }
            }
        }

        impl EventEmitter<PanelEvent> for $panel {}

        impl Focusable for $panel {
            fn focus_handle(&self, _cx: &App) -> FocusHandle {
                self.focus_handle.clone()
            }
        }

        impl Panel for $panel {
            fn panel_name(&self) -> &'static str {
                $panel_name
            }

            fn title(&self, _window: &Window, _cx: &App) -> AnyElement {
                $title.into_any_element()
            }
        }
    };
}

/// Variables Panel
editor_panel_shell!(VariablesPanel, "variables", "Variables");

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

/// Trait implementation assignments for the current Blueprint class.
editor_panel_shell!(ImplementedTraitsPanel, "implemented-traits", "Traits");

impl Render for ImplementedTraitsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(editor) = self.editor.upgrade() {
            div()
                .size_full()
                .bg(cx.theme().sidebar)
                .child(editor.update(cx, |editor, cx| {
                    ImplementedTraitsRenderer::render(editor, window, cx)
                }))
        } else {
            div().child("Editor not available")
        }
    }
}

/// Macros Panel
editor_panel_shell!(MacrosPanel, "macros", "Macros");

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

/// Events Panel
editor_panel_shell!(EventsPanel, "events", "Events");

impl Render for EventsPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(editor) = self.editor.upgrade() {
            div()
                .size_full()
                .bg(cx.theme().sidebar)
                .child(editor.update(cx, |editor, cx| EventsRenderer::render(editor, cx)))
        } else {
            div().child("Editor not available")
        }
    }
}

/// Compiler Panel
editor_panel_shell!(CompilerPanel, "compiler", "Compiler");

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

/// Live view of the sections that will be written to the Blueprint save file.
pub struct AssetInspectorPanel {
    editor: WeakEntity<BlueprintEditorPanel>,
    focus_handle: FocusHandle,
    snapshot: AssetInspectorSnapshot,
    refresh_task: Option<Task<()>>,
    observed_canvas_ids: HashSet<String>,
    _subscriptions: Vec<Subscription>,
}

impl AssetInspectorPanel {
    pub fn new(editor: WeakEntity<BlueprintEditorPanel>, cx: &mut Context<Self>) -> Self {
        let mut panel = Self {
            editor,
            focus_handle: cx.focus_handle(),
            snapshot: AssetInspectorSnapshot::default(),
            refresh_task: None,
            observed_canvas_ids: HashSet::new(),
            _subscriptions: Vec::new(),
        };

        if let Some(editor_entity) = panel.editor.upgrade() {
            panel
                ._subscriptions
                .push(cx.observe(&editor_entity, |panel, _, cx| {
                    panel.schedule_preview_refresh(cx);
                }));
        }
        panel.schedule_preview_refresh(cx);
        panel
    }

    fn observe_canvases(
        &mut self,
        canvases: &[(String, Entity<GraphCanvasPanel>)],
        cx: &mut Context<Self>,
    ) {
        for (tab_id, canvas) in canvases {
            if self.observed_canvas_ids.insert(tab_id.clone()) {
                self._subscriptions.push(cx.observe(canvas, |panel, _, cx| {
                    panel.schedule_preview_refresh(cx);
                }));
            }
        }
    }

    fn schedule_preview_refresh(&mut self, cx: &mut Context<Self>) {
        let editor = self.editor.clone();
        self.refresh_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(400))
                .await;
            let (snapshot, canvases) = if let Some(editor) = editor.upgrade() {
                editor.update(cx, |editor, cx| {
                    let snapshot = AssetInspectorRenderer::build_snapshot(editor, cx);
                    (snapshot, editor.graph_panels.clone())
                })
            } else {
                (
                    AssetInspectorSnapshot {
                        error: Some("Editor not available".to_string()),
                        ..Default::default()
                    },
                    Vec::new(),
                )
            };
            let _ = this.update(cx, |panel, cx| {
                panel.observe_canvases(&canvases, cx);
                panel.snapshot = snapshot;
                panel.refresh_task = None;
                cx.notify();
            });
        }));
    }
}

impl EventEmitter<PanelEvent> for AssetInspectorPanel {}

impl Render for AssetInspectorPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(editor) = self.editor.upgrade() {
            let snapshot = self.snapshot.clone();
            div()
                .size_full()
                .bg(cx.theme().sidebar)
                .child(editor.update(cx, |editor, cx| {
                    AssetInspectorRenderer::render(editor, &snapshot, cx)
                }))
        } else {
            div().child("Editor not available")
        }
    }
}

impl Focusable for AssetInspectorPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for AssetInspectorPanel {
    fn panel_name(&self) -> &'static str {
        "asset-save-data"
    }

    fn title(&self, _window: &Window, _cx: &App) -> AnyElement {
        "Save Data".into_any_element()
    }
}

/// Find Panel
editor_panel_shell!(FindPanel, "find", "Find");

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

/// Properties Panel
editor_panel_shell!(PropertiesPanel, "properties", "Properties");

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

/// Prefab Hierarchy Panel
editor_panel_shell!(PrefabHierarchyPanel, "prefab-hierarchy", "Components");

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

