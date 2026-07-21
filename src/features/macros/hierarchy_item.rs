//! HierarchyItem implementation for SubGraphDefinition (Macros)
//!
//! This allows macros to be displayed in the HierarchicalTreeView component

use crate::editor::panel::RenameTarget;
use gpui::*;
use std::sync::Arc;
use ui::{input::InputState, menu::popup_menu::PopupMenu, HierarchyItem, IconName};

/// Drag payload for macros
#[derive(Clone)]
pub struct MacroDrag {
    pub macro_index: usize,
    pub macro_id: String,
    pub macro_name: String,
}

/// Wrapper for SubGraphDefinition that implements HierarchyItem
#[derive(Clone)]
pub struct MacroHierarchyItem {
    pub subgraph: ui::graph::SubGraphDefinition,
    pub index: usize,
    pub is_selected: bool,
    /// Weak back-ref so context-menu actions can reach the panel.
    pub panel: gpui::WeakEntity<crate::editor::panel::BlueprintEditorPanel>,
    pub is_renaming: bool,
    pub rename_input: Option<Entity<InputState>>,
}

impl HierarchyItem for MacroHierarchyItem {
    type Id = usize;
    type DragPayload = MacroDrag;

    fn id(&self) -> Self::Id {
        self.index
    }

    fn name(&self) -> String {
        self.subgraph.name.clone()
    }

    fn icon(&self) -> IconName {
        IconName::Component // Use Component icon for macros
    }

    fn icon_color<V>(&self, cx: &Context<V>) -> Hsla
    where
        V: Render,
    {
        // Purple color for macros
        Hsla {
            h: 280.0 / 360.0, // Purple hue
            s: 0.5,
            l: 0.6,
            a: 1.0,
        }
    }

    fn children_ids(&self) -> Vec<Self::Id> {
        vec![] // Macros don't have children (flat list)
    }

    fn is_selected(&self) -> bool {
        self.is_selected
    }

    fn is_renaming(&self) -> bool {
        self.is_renaming
    }

    fn rename_input(&self) -> Option<Entity<InputState>> {
        self.rename_input.clone()
    }

    fn create_drag_payload(&self) -> Self::DragPayload {
        MacroDrag {
            macro_index: self.index,
            macro_id: self.subgraph.id.clone(),
            macro_name: self.subgraph.name.clone(),
        }
    }

    fn drag_drop_id(&self) -> String {
        format!("macro-{}", self.index)
    }

    fn extra_row_content<V>(&self, cx: &mut Context<V>) -> Option<AnyElement>
    where
        V: Render,
    {
        use ui::{h_flex, ActiveTheme, StyledExt};

        if self.is_renaming {
            return None;
        }

        Some(
            h_flex()
                .gap_1()
                .items_center()
                .child(
                    // Input count badge
                    div()
                        .px_2()
                        .py_1()
                        .rounded(px(4.0))
                        .bg(cx.theme().success.opacity(0.15))
                        .text_xs()
                        .font_family("JetBrainsMono-Regular")
                        .text_color(cx.theme().success)
                        .child(format!("→ {}", self.subgraph.interface.inputs.len())),
                )
                .child(
                    // Output count badge
                    div()
                        .px_2()
                        .py_1()
                        .rounded(px(4.0))
                        .bg(cx.theme().warning.opacity(0.15))
                        .text_xs()
                        .font_family("JetBrainsMono-Regular")
                        .text_color(cx.theme().warning)
                        .child(format!("← {}", self.subgraph.interface.outputs.len())),
                )
                // Delete handled via context menu
                .into_any_element(),
        )
    }

    fn build_context_menu(
        &self,
        menu: PopupMenu,
        _window: &mut Window,
        _cx: &mut Context<PopupMenu>,
    ) -> PopupMenu {
        let macro_id = self.subgraph.id.clone();
        let panel = self.panel.clone();
        let panel2 = panel.clone();
        let mid2 = macro_id.clone();

        menu.menu_handler("Rename Macro", move |_window, cx| {
            if let Some(p) = panel.upgrade() {
                p.update(cx, |panel, cx| {
                    let current = panel
                        .local_macros
                        .iter()
                        .find(|m| m.id == macro_id)
                        .map(|m| m.name.clone())
                        .unwrap_or_default();
                    panel.renaming_target = Some(RenameTarget::Macro(macro_id.clone()));
                    panel.rename_input.update(cx, |input, cx| {
                        input.set_value(current, _window, cx);
                        input.focus(_window, cx);
                    });
                    cx.notify();
                });
            }
        })
        .menu_handler("Delete Macro", move |_window, cx| {
            if let Some(p) = panel2.upgrade() {
                p.update(cx, |panel, cx| {
                    panel.delete_local_macro(&mid2, cx);
                });
            }
        })
    }
}

// Implement Render for MacroDrag to show drag preview
impl Render for MacroDrag {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use ui::{h_flex, ActiveTheme, StyledExt};

        h_flex()
            .px_3()
            .py_1()
            .rounded(px(4.0))
            .bg(cx.theme().background)
            .border_1()
            .border_color(cx.theme().border)
            .text_sm()
            .text_color(cx.theme().foreground)
            .child(format!("📦 {}", self.macro_name))
    }
}
