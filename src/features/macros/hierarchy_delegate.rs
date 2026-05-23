//! HierarchyDelegate implementation for macros panel

use super::hierarchy_item::{MacroDrag, MacroHierarchyItem};
use crate::editor::panel::BlueprintEditorPanel;
use gpui::*;
use ui::HierarchyDelegate;

impl HierarchyDelegate<MacroHierarchyItem> for BlueprintEditorPanel {
    fn hierarchy_is_expanded(&self, _id: &usize) -> bool {
        // Macros don't have expansion (flat list)
        false
    }

    fn hierarchy_toggle_expand(&mut self, _id: &usize, _cx: &mut Context<Self>) {
        // No-op for flat lists
    }

    fn hierarchy_select(&mut self, id: &usize, cx: &mut Context<Self>) {
        // Open the macro when clicked
        if let Some(subgraph) = self.local_macros.get(*id) {
            let macro_id = subgraph.id.clone();
            let macro_name = subgraph.name.clone();
            self.open_local_macro(macro_id, macro_name, cx.window(), cx);
        }
    }

    fn hierarchy_drop(&mut self, _payload: &MacroDrag, _target: &usize, _modifiers: &Modifiers, _cx: &mut Context<Self>) {
        // TODO: Implement macro reordering if needed
    }
}
