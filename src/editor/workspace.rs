//! Workspace initialization and layout
//!
//! Handles setting up the docking workspace with sidebar panels

use gpui::*;
use std::sync::Arc;
use ui::dock::DockItem;
use ui::workspace::Workspace;

use crate::editor::panel::BlueprintEditorPanel;
use crate::editor::workspace_panels::{
    AssetInspectorPanel, CompilerPanel, EventsPanel, FindPanel, GraphCanvasPanel,
    ImplementedTraitsPanel, MacrosPanel, PrefabHierarchyPanel, PropertiesPanel, VariablesPanel,
};

impl BlueprintEditorPanel {
    /// Initialize the docking workspace with panels
    pub fn initialize_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.workspace.is_some() {
            return;
        }

        tracing::info!(
            ">>> initialize_workspace: open_tabs={}, self.graph.nodes={}, graph_panels before={}",
            self.open_tabs.len(),
            self.graph.nodes.len(),
            self.graph_panels.len(),
        );

        for tab in &self.open_tabs {
            tracing::info!(
                ">>> initialize_workspace: tab id={} is_main={} nodes={} connections={}",
                tab.id,
                tab.is_main,
                tab.graph.nodes.len(),
                tab.graph.connections.len(),
            );
        }

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
            let center_panels: Vec<(String, Entity<GraphCanvasPanel>)> = self
                .open_tabs
                .iter()
                .map(|tab| {
                    let tid = tab.id.clone();
                    let tname = tab.name.clone();
                    let tis_main = tab.is_main;
                    let tgraph = tab.graph.clone();
                    let tracing_id = tid.clone();
                    let tracing_nodes = tgraph.nodes.len();
                    tracing::info!(
                        ">>> initialize_workspace: creating canvas for tab {} with {} nodes",
                        tracing_id,
                        tracing_nodes,
                    );
                    let ew = editor_weak.clone();
                    let panel = cx.new(|cx| {
                        let canvas =
                            GraphCanvasPanel::new(
                            ew,
                            tid.clone(),
                            tname,
                            tis_main,
                            tgraph,
                            self.profile.clone(),
                            window,
                            cx,
                        );
                        self.adopt_graph_history(&tid, &canvas.history);
                        canvas
                    });
                    (tab.id.clone(), panel)
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
            // A profile without Blueprint concepts keeps only the canvas and the
            // property panel (a node's parameters are edited there).
            let stripped = self.profile.as_ref().is_some_and(|p| !p.side_panels());
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
            let mut right_panels = vec![Arc::new(properties_panel) as Arc<dyn ui::dock::PanelView>];
            if !stripped {
                right_panels.push(Arc::new(compiler_panel) as Arc<dyn ui::dock::PanelView>);
                right_panels.push(Arc::new(asset_inspector_panel) as Arc<dyn ui::dock::PanelView>);
            }
            let properties = DockItem::tabs(right_panels, None, &dock_area_weak, window, cx);
            // A profile may put a preview above the Properties panel.
            let preview = self.profile.clone().and_then(|profile| {
                let file = self.get_graph_file_path();
                profile.preview_panel(file.as_deref(), window, cx)
            });
            let right = match preview {
                Some(preview) => DockItem::split(
                    Axis::Vertical,
                    vec![
                        DockItem::tabs(vec![preview], None, &dock_area_weak, window, cx),
                        properties,
                    ],
                    &dock_area_weak,
                    window,
                    cx,
                ),
                None => properties,
            };

            workspace.initialize(
                center,
                (!stripped).then_some(left),
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
                        if let Some((_, canvas)) =
                            this.graph_panels.iter().find(|(id, _)| id == &tab_id)
                        {
                            this.graph = canvas.read(cx).graph.clone();
                        }
                        cx.notify();
                    }
                },
            );
            self.subscriptions.push(sub);
        }
    }
}
