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
use graphy_gpui::picker::{NodePicker, PickerEvent};
use graphy_gpui::GraphProfile as _;

use crate::canvas_profile::{BlueprintProfile, ADD_CUSTOM_EVENT};
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

/// Palette panel: the shared node picker docked beside the graph. Choosing a
/// node adds it at the centre of the active canvas.
pub struct PalettePanel {
    focus_handle: FocusHandle,
    editor: WeakEntity<BlueprintEditorPanel>,
    picker: Entity<NodePicker>,
    profile: BlueprintProfile,
    catalog_version: u64,
    _subscription: Subscription,
}

impl PalettePanel {
    pub fn new(
        editor: WeakEntity<BlueprintEditorPanel>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let data = editor
            .upgrade()
            .map(|editor| editor.read(cx).profile_data.clone())
            .unwrap_or_default();
        let profile = BlueprintProfile::blueprint(data);
        let entries = profile.catalog();
        let picker = cx.new(|cx| NodePicker::new(entries, None, window, cx));
        let subscription = cx.subscribe_in(
            &picker,
            window,
            |this, _picker, event: &PickerEvent, window, cx| {
                if let PickerEvent::Chosen(definition) = event {
                    this.place(definition, window, cx);
                }
            },
        );
        Self {
            focus_handle: cx.focus_handle(),
            editor,
            picker,
            profile,
            catalog_version: 0,
            _subscription: subscription,
        }
    }

    /// Adds `definition` to the active canvas, staggered so repeats do not stack.
    fn place(&mut self, definition: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(canvas) = self
            .editor
            .upgrade()
            .and_then(|editor| editor.read(cx).active_canvas().cloned())
        else {
            return;
        };
        let definition = definition.to_string();
        canvas.update(cx, |canvas, cx| {
            if definition == ADD_CUSTOM_EVENT {
                canvas.add_custom_event_from_picker(window, cx);
                return;
            }
            let base = canvas.view_center(cx);
            let stagger = (canvas.graph(cx).nodes.len() % 8) as f32 * 18.0;
            let at = graphy_render::Vec2::new(base.x + stagger, base.y + stagger);
            canvas.add_definition_node(&definition, at, None, cx);
        });
    }
}

impl EventEmitter<PanelEvent> for PalettePanel {}

impl Render for PalettePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let version = self.profile.data_version();
        if version != self.catalog_version {
            self.catalog_version = version;
            let entries = self.profile.catalog();
            self.picker
                .update(cx, |picker, cx| picker.set_entries(entries, cx));
        }
        div().size_full().child(self.picker.clone())
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

