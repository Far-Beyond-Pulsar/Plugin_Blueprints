//! Workspace initialization and layout
//!
//! Handles setting up the docking workspace with sidebar panels

use gpui::*;
use std::sync::Arc;
use ui::dock::DockItem;
use ui::workspace::Workspace;

use crate::editor::panel::BlueprintEditorPanel;
use crate::editor::workspace_panels::{
    CompilerPanel, FindPanel, GraphCanvasPanel, MacrosPanel, PalettePanel, PrefabHierarchyPanel,
    PrefabsPanel, PropertiesPanel, VariablesPanel,
};

impl BlueprintEditorPanel {
    /// Initialize the docking workspace with panels
    pub fn initialize_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.workspace.is_some() {
            return;
        }

        let editor_weak = cx.entity().downgrade();

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
            let palette_panel = cx.new(|cx| PalettePanel::new(editor_weak.clone(), window, cx));
            let center_panels: Vec<(String, Entity<GraphCanvasPanel>)> = self
                .open_tabs
                .iter()
                .map(|tab| {
                    let tid = tab.id.clone();
                    let tname = tab.name.clone();
                    let tis_main = tab.is_main;
                    let tgraph = tab.graph.clone();
                    let ew = editor_weak.clone();
                    let panel = cx.new(|cx| {
                        GraphCanvasPanel::new(ew, tid.clone(), tname, tis_main, tgraph, window, cx)
                    });
                    (tab.id.clone(), panel)
                })
                .collect();

            self.graph_panels = center_panels.clone();

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

            let left = DockItem::tabs(
                vec![
                    Arc::new(prefab_hierarchy_panel),
                    Arc::new(variables_panel),
                    Arc::new(macros_panel),
                ],
                None,
                &dock_area_weak,
                window,
                cx,
            );

            let right = DockItem::tabs(
                vec![
                    Arc::new(prefabs_panel),
                    Arc::new(properties_panel),
                    Arc::new(palette_panel),
                ],
                None,
                &dock_area_weak,
                window,
                cx,
            );

            let bottom = DockItem::tabs(
                vec![Arc::new(compiler_panel), Arc::new(find_panel)],
                None,
                &dock_area_weak,
                window,
                cx,
            );

            workspace.initialize(center, Some(left), Some(right), Some(bottom), window, cx);
        });

        self.workspace = Some(workspace);
        self.graph_workspace_tabs_dirty = false;
        self.sync_active_canvas_entity();
    }
}
