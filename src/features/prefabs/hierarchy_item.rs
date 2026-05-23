//! HierarchyItem implementation for ComponentInstance (Prefab Components)
//!
//! This allows prefab components to be displayed in the HierarchicalTreeView with nesting support

use engine_backend::ComponentInstance;
use gpui::*;
use std::sync::Arc;
use ui::{menu::popup_menu::PopupMenu, ActiveTheme, HierarchyItem, IconName};

/// Drag payload for components
#[derive(Clone)]
pub struct ComponentDrag {
    pub component_index: usize,
    pub class_name: String,
}

/// Shared state for component selection
pub type ComponentSelectionState = std::sync::Arc<parking_lot::RwLock<Option<usize>>>;

/// Wrapper for ComponentInstance that implements HierarchyItem
#[derive(Clone)]
pub struct ComponentHierarchyItem {
    pub component: ComponentInstance,
    pub index: usize,
    pub is_selected: bool,
    pub children_indices: Vec<usize>,
    pub selection_state: ComponentSelectionState,
}

impl HierarchyItem for ComponentHierarchyItem {
    type Id = usize;
    type DragPayload = ComponentDrag;

    fn id(&self) -> Self::Id {
        self.index
    }

    fn name(&self) -> String {
        self.component.class_name.clone()
    }

    fn icon(&self) -> IconName {
        IconName::Component
    }

    fn icon_color<V>(&self, cx: &Context<V>) -> Hsla
    where
        V: Render,
    {
        // Blue color for components
        cx.theme().accent
    }

    fn children_ids(&self) -> Vec<Self::Id> {
        self.children_indices.clone()
    }

    fn is_selected(&self) -> bool {
        self.is_selected
    }

    fn create_drag_payload(&self) -> Self::DragPayload {
        ComponentDrag {
            component_index: self.index,
            class_name: self.component.class_name.clone(),
        }
    }

    fn drag_drop_id(&self) -> String {
        format!("component-{}", self.index)
    }

    fn extra_row_content<V>(&self, cx: &mut Context<V>) -> Option<AnyElement>
    where
        V: Render,
    {
        use ui::{h_flex, ActiveTheme, StyledExt};

        Some(
            h_flex()
                .gap_1()
                .items_center()
                .child(
                    // Enabled/disabled indicator
                    div()
                        .w(px(8.0))
                        .h(px(8.0))
                        .rounded_full()
                        .bg(if self.component.enabled {
                            cx.theme().success
                        } else {
                            cx.theme().muted
                        }),
                )
                .into_any_element(),
        )
    }

    fn on_click_custom(&self) -> Option<Arc<dyn Fn()>> {
        let component_index = self.index;
        let selection_state = self.selection_state.clone();
        Some(Arc::new(move || {
            *selection_state.write() = Some(component_index);
        }))
    }

    fn build_context_menu(
        &self,
        menu: PopupMenu,
        _window: &mut Window,
        _cx: &mut Context<PopupMenu>,
    ) -> PopupMenu {
        // TODO: Add context menu items (duplicate, delete, toggle enabled, etc.)
        menu
    }
}

// Implement Render for ComponentDrag to show drag preview
impl Render for ComponentDrag {
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
            .child(format!("📦 {}", self.class_name))
    }
}
