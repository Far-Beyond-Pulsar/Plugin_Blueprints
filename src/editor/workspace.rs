//! Workspace initialization and layout
//!
//! Handles setting up the docking workspace with sidebar panels

use gpui::*;
use std::sync::Arc;
use ui::dock::DockItem;
use ui::workspace::Workspace;

use crate::editor::panel::BlueprintEditorPanel;
use crate::editor::workspace_panels::{
    CompilerPanel, FindPanel, GraphCanvasPanel, MacrosPanel, PrefabHierarchyPanel, PrefabsPanel,
    PropertiesPanel, VariablesPanel,
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
            let macros_panel = cx.new(|cx| MacrosPanel::new(editor_weak.clone(), cx));
            let prefab_hierarchy_panel =
                cx.new(|cx| PrefabHierarchyPanel::new(editor_weak.clone(), cx));
            let compiler_panel = cx.new(|cx| CompilerPanel::new(editor_weak.clone(), cx));
            let find_panel = cx.new(|cx| FindPanel::new(editor_weak.clone(), cx));
            let properties_panel = cx.new(|cx| PropertiesPanel::new(editor_weak.clone(), cx));
            let prefabs_panel = cx.new(|cx| PrefabsPanel::new(editor_weak.clone(), cx));
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
                        GraphCanvasPanel::new(ew, tid.clone(), tname, tis_main, tgraph, window, cx)
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

            // Stack the three left-side panels vertically, each given an equal
            // (1/3) share of the available height, instead of tabbing them
            // together — they're all small reference lists the user wants
            // visible side-by-side rather than switched between.
            let left = DockItem::split(
                Axis::Vertical,
                vec![
                    DockItem::tabs(
                        vec![Arc::new(prefab_hierarchy_panel)],
                        None,
                        &dock_area_weak,
                        window,
                        cx,
                    ),
                    DockItem::tabs(
                        vec![Arc::new(variables_panel)],
                        None,
                        &dock_area_weak,
                        window,
                        cx,
                    ),
                    DockItem::tabs(
                        vec![Arc::new(macros_panel)],
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

            // The node palette is now reachable via the canvas's quick palette
            // (right-click), so the dedicated palette tab is redundant here.
            // Stack the two remaining panels vertically rather than tabbing them.
            let right = DockItem::split(
                Axis::Vertical,
                vec![
                    DockItem::tabs(vec![Arc::new(prefabs_panel)], None, &dock_area_weak, window, cx),
                    DockItem::tabs(
                        vec![Arc::new(properties_panel)],
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

            // Side-by-side 50/50 instead of tabbed.
            let bottom = DockItem::split(
                Axis::Horizontal,
                vec![
                    DockItem::tabs(vec![Arc::new(compiler_panel)], None, &dock_area_weak, window, cx),
                    DockItem::tabs(vec![Arc::new(find_panel)], None, &dock_area_weak, window, cx),
                ],
                &dock_area_weak,
                window,
                cx,
            );

            workspace.initialize(center, Some(left), Some(right), None, window, cx);

            // `Workspace::initialize` doesn't expose a custom size for the bottom
            // dock — set it directly afterwards so we can shrink it 10% below the
            // default 400px height.
            workspace.dock_area().update(cx, |dock_area, cx| {
                dock_area.set_bottom_dock(bottom, Some(px(360.0)), true, window, cx);
            });
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
