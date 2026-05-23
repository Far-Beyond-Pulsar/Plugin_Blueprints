//! HierarchyItem implementation for ClassVariable
//!
//! This allows variables to be displayed in the HierarchicalTreeView component

use super::types::{ClassVariable, VariableDrag};
use gpui::*;
use std::sync::Arc;
use ui::{menu::popup_menu::PopupMenu, HierarchyItem, IconName};

/// Wrapper for ClassVariable that implements HierarchyItem
#[derive(Clone)]
pub struct VariableHierarchyItem {
    pub variable: ClassVariable,
    pub index: usize, // Index in the variables list
    pub is_selected: bool,
}

impl HierarchyItem for VariableHierarchyItem {
    type Id = usize;
    type DragPayload = VariableDrag;

    fn id(&self) -> Self::Id {
        self.index
    }

    fn name(&self) -> String {
        self.variable.name.clone()
    }

    fn icon(&self) -> IconName {
        IconName::Code // Use Code icon for variables
    }

    fn icon_color<V>(&self, _cx: &Context<V>) -> Hsla
    where
        V: Render,
    {
        // Default color - not used when accent_color is present
        Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.5,
            a: 1.0,
        }
    }

    fn accent_color<V>(&self, _cx: &Context<V>) -> Option<Hsla>
    where
        V: Render,
    {
        // Get the type color
        let type_info = ui::graph::TypeInfo::parse(&self.variable.var_type);
        let pin_color = type_info.generate_color();

        Some(Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.0,
            a: 1.0,
        }.blend(Hsla {
            h: pin_color.r as f32 * 360.0,
            s: pin_color.g as f32,
            l: pin_color.b as f32,
            a: pin_color.a as f32,
        }))
    }

    fn children_ids(&self) -> Vec<Self::Id> {
        vec![] // Variables don't have children (flat list)
    }

    fn is_selected(&self) -> bool {
        self.is_selected
    }

    fn create_drag_payload(&self) -> Self::DragPayload {
        VariableDrag {
            var_index: self.index,
            var_name: self.variable.name.clone(),
            var_type: self.variable.var_type.clone(),
        }
    }

    fn drag_drop_id(&self) -> String {
        format!("variable-{}", self.index)
    }

    fn extra_row_content<V>(&self, _cx: &mut Context<V>) -> Option<AnyElement>
    where
        V: Render,
    {
        // Type badge
        let type_info = ui::graph::TypeInfo::parse(&self.variable.var_type);
        let pin_color = type_info.generate_color();

        Some(
            // Type badge only - delete handled via context menu
            div()
                .px_1p5()
                .py_0p5()
                .rounded(px(3.0))
                .bg(gpui::Rgba {
                    r: pin_color.r,
                    g: pin_color.g,
                    b: pin_color.b,
                    a: 0.15,
                })
                .child(
                    div()
                        .text_xs()
                        .text_color(gpui::Rgba {
                            r: pin_color.r,
                            g: pin_color.g,
                            b: pin_color.b,
                            a: 1.0,
                        })
                        .child(self.variable.var_type.clone()),
                )
                .into_any_element(),
        )
    }

    fn build_context_menu(
        &self,
        menu: PopupMenu,
        _window: &mut Window,
        _cx: &mut Context<PopupMenu>,
    ) -> PopupMenu {
        // TODO: Add context menu items for variables (duplicate, rename, etc.)
        menu
    }
}

// Implement Render for VariableDrag to show drag preview
impl Render for VariableDrag {
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
            .child(format!("{} ({})", self.var_name, self.var_type))
    }
}
