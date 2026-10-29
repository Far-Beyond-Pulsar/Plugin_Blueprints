//! Workspace initialization and layout
//!
//! Handles setting up the docking workspace with sidebar panels

use gpui::*;
use std::sync::Arc;
use ui::dock::DockItem;
use ui::workspace::Workspace;

use crate::editor::panel::BlueprintEditorPanel;
use crate::editor::graph_canvas_panel::GraphCanvasPanel;
use crate::editor::tabs::TabDoc;
use crate::editor::workspace_panels::{
    AssetInspectorPanel, CompilerPanel, EventsPanel, FindPanel,
    ImplementedTraitsPanel, MacrosPanel, PrefabHierarchyPanel, PropertiesPanel, VariablesPanel,
};

impl BlueprintEditorPanel {
    /// Initialize the docking workspace with panels
    pub fn initialize_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.workspace.is_some() {
            return;
        }

        self.sync_profile_data();

        let editor_weak = cx.entity().downgrade();

        let mut center_tab_panel: Option<Entity<ui::dock::TabPanel>> = None;

        let workspace = cx.new(|cx| {
            Workspace::new_with_channel(
                "blueprint-editor-workspace",
                ui::dock::DockChannel(1),
                window,
                cx,
            )
        });

        workspace.update(cx, |workspace, cx| {
            let dock_area_weak = workspace.dock_area().downgrade();

            let variables_panel = cx.new(|cx| VariablesPanel::new(editor_weak.clone(), cx));
            let implemented_traits_panel =
                cx.new(|cx| ImplementedTraitsPanel::new(editor_weak.clone(), cx));
            let macros_panel = cx.new(|cx| MacrosPanel::new(editor_weak.clone(), cx));
            let events_panel = cx.new(|cx| EventsPanel::new(editor_weak.clone(), cx));
            let prefab_hierarchy_panel =
                cx.new(|cx| PrefabHierarchyPanel::new(editor_weak.clone(), cx));
            let compiler_panel = cx.new(|cx| CompilerPanel::new(editor_weak.clone(), cx));
            let asset_inspector_panel =
                cx.new(|cx| AssetInspectorPanel::new(editor_weak.clone(), cx));
            let find_panel = cx.new(|cx| FindPanel::new(editor_weak.clone(), cx));
            self.find_panel = Some(find_panel.clone());
            let properties_panel = cx.new(|cx| PropertiesPanel::new(editor_weak.clone(), cx));
            let pending: Vec<(String, String, bool, TabDoc)> = self
                .open_tabs
                .iter_mut()
                .filter_map(|tab| {
                    tab.pending
                        .take()
                        .map(|doc| (tab.id.clone(), tab.name.clone(), tab.is_main, doc))
                })
                .collect();
            let profile_data = self.profile_data.clone();
            let center_panels: Vec<(String, Entity<GraphCanvasPanel>)> = pending
                .into_iter()
                .map(|(tid, tname, tis_main, doc)| {
                    let ew = editor_weak.clone();
                    let profile_data = profile_data.clone();
                    let id = tid.clone();
                    let panel = cx.new(|cx| {
                        let canvas = GraphCanvasPanel::new(
                            ew,
                            tid.clone(),
                            tname,
                            tis_main,
                            doc,
                            profile_data,
                            window,
                            cx,
                        );
                        self.adopt_graph_history(&tid, &canvas.history);
                        canvas
                    });
                    (id, panel)
                })
                .collect();

            self.graph_panels = center_panels.clone();

            tracing::info!(
                ">>> initialize_workspace: graph_panels now has {} entries",
                self.graph_panels.len(),
            );

            let center = DockItem::tabs(
                center_panels
                    .iter()
                    .map(|(_, panel)| Arc::new(panel.clone()) as Arc<dyn ui::dock::PanelView>)
                    .collect(),
                Some(self.active_tab_index),
                &dock_area_weak,
                window,
                cx,
            );

            if let ui::dock::DockItem::Tabs { view, .. } = &center {
                center_tab_panel = Some(view.clone());
            }

            // Left sidebar: Components/Find (multi-tab), Macros/Events (multi-tab), Variables
            let left = DockItem::split(
                Axis::Vertical,
                vec![
                    DockItem::tabs(
                        vec![
                            Arc::new(prefab_hierarchy_panel) as Arc<dyn ui::dock::PanelView>,
                            Arc::new(find_panel) as Arc<dyn ui::dock::PanelView>,
                        ],
                        None,
                        &dock_area_weak,
                        window,
                        cx,
                    ),
                    DockItem::tabs(
                        vec![
                            Arc::new(macros_panel) as Arc<dyn ui::dock::PanelView>,
                            Arc::new(events_panel) as Arc<dyn ui::dock::PanelView>,
                        ],
                        None,
                        &dock_area_weak,
                        window,
                        cx,
                    ),
                    DockItem::tabs(
                        vec![
                            Arc::new(variables_panel) as Arc<dyn ui::dock::PanelView>,
                            Arc::new(implemented_traits_panel) as Arc<dyn ui::dock::PanelView>,
                        ],
                        None,
                        &dock_area_weak,
                        window,
                        cx,
                    ),
                ],
                &dock_area_weak,
                window,
                cx,
            );

            // Right sidebar: live properties, compiler output, and serialized save data.
            let right = DockItem::tabs(
                vec![
                    Arc::new(properties_panel) as Arc<dyn ui::dock::PanelView>,
                    Arc::new(compiler_panel) as Arc<dyn ui::dock::PanelView>,
                    Arc::new(asset_inspector_panel) as Arc<dyn ui::dock::PanelView>,
                ],
                None,
                &dock_area_weak,
                window,
                cx,
            );
            workspace.initialize(
                center,
                Some(left),
                Some(right),
                None,
                window,
                cx,
            );
        });

        self.workspace = Some(workspace);
        self.graph_workspace_tabs_dirty = false;

        // The dock's tab strip switches tabs entirely on its own (the user clicks a tab
        // header directly in the `TabPanel`), without ever calling our `switch_to_tab`.
        // That left `self.active_tab_index` stuck at whatever it was last set to
        // programmatically, so `active_canvas()` (and therefore the properties panel)
        // kept reading a different canvas than the one the user was actually looking at
        // and clicking in — breaking selection display for every graph as soon as a
        // second tab (e.g. a macro) existed. Mirror the dock's active tab back onto
        // `self.active_tab_index` by matching entity ids, so the two stay in sync
        // regardless of how the switch happened.
        if let Some(tab_panel) = center_tab_panel {
            let sub = cx.subscribe(
                &tab_panel,
                |this: &mut Self, tab_panel, event: &ui::dock::PanelEvent, cx| {
                    if !matches!(event, ui::dock::PanelEvent::TabChanged { .. }) {
                        return;
                    }
                    let Some(active_panel) = tab_panel.read(cx).active_panel(cx) else {
                        return;
                    };
                    let active_entity_id = active_panel.panel_id(cx);
                    let Some(tab_id) = this
                        .graph_panels
                        .iter()
                        .find(|(_, panel)| panel.entity_id() == active_entity_id)
                        .map(|(id, _)| id.clone())
                    else {
                        return;
                    };
                    let Some(new_index) = this.open_tabs.iter().position(|tab| tab.id == tab_id)
                    else {
                        return;
                    };
                    if new_index != this.active_tab_index {
                        this.active_tab_index = new_index;
                        cx.notify();
                    }
                },
            );
            self.subscriptions.push(sub);
        }
    }
}
