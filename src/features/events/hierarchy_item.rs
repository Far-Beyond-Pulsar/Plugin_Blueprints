//! HierarchyItem implementation for EventDefinition (Events)
//!
//! Mirrors MacroHierarchyItem — lets events display in HierarchicalTreeView.

use crate::editor::panel::RenameTarget;
use crate::core::graph::EventDefinition;
use gpui::*;
use std::sync::Arc;
use ui::{input::InputState, menu::popup_menu::PopupMenu, HierarchyItem, IconName};

#[derive(Clone)]
pub struct EventDrag {
    pub event_index: usize,
    pub event_uid: String,
    pub event_name: String,
}

#[derive(Clone)]
pub struct EventHierarchyItem {
    pub def: EventDefinition,
    pub index: usize,
    pub is_selected: bool,
    pub panel: gpui::WeakEntity<crate::editor::panel::BlueprintEditorPanel>,
    pub is_renaming: bool,
    pub rename_input: Option<Entity<InputState>>,
}

impl HierarchyItem for EventHierarchyItem {
    type Id = usize;
    type DragPayload = EventDrag;

    fn id(&self) -> Self::Id {
        self.index
    }

    fn name(&self) -> String {
        self.def.name.clone()
    }

    fn icon(&self) -> IconName {
        IconName::Activity
    }

    fn icon_color<V>(&self, cx: &Context<V>) -> Hsla
    where
        V: Render,
    {
        Hsla {
            h: 30.0 / 360.0,
            s: 0.6,
            l: 0.6,
            a: 1.0,
        }
    }

    fn children_ids(&self) -> Vec<Self::Id> {
        vec![]
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
        EventDrag {
            event_index: self.index,
            event_uid: self.def.uid.clone(),
            event_name: self.def.name.clone(),
        }
    }

    fn drag_drop_id(&self) -> String {
        format!("event-{}", self.index)
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
                    div()
                        .px_2()
                        .py_1()
                        .rounded(px(4.0))
                        .bg(cx.theme().success.opacity(0.15))
                        .text_xs()
                        .font_family("JetBrainsMono-Regular")
                        .text_color(cx.theme().success)
                        .child(format!("→ {}", self.def.fields.len())),
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
        let uid = self.def.uid.clone();
        let panel = self.panel.clone();
        let panel2 = panel.clone();
        let uid2 = uid.clone();

        menu.menu_handler("Rename Event", move |_window, cx| {
            if let Some(p) = panel.upgrade() {
                p.update(cx, |panel, cx| {
                    let current = panel
                        .local_event_defs
                        .iter()
                        .find(|d| d.uid == uid)
                        .map(|d| d.name.clone())
                        .unwrap_or_default();
                    panel.renaming_target = Some(RenameTarget::Event(uid.clone()));
                    panel.rename_input.update(cx, |input, cx| {
                        input.set_value(current, _window, cx);
                        input.focus(_window, cx);
                    });
                    cx.notify();
                });
            }
        })
        .menu_handler("Delete Event", move |_window, cx| {
            if let Some(p) = panel2.upgrade() {
                let win_handle = _window.window_handle();
                p.update(cx, |panel, cx| {
                    panel.delete_event_def(&uid2);
                    cx.notify();
                });
                let p2 = panel2.clone();
                cx.defer(move |cx| {
                    let _ = cx.update_window(win_handle, |_, window, cx| {
                        if let Some(p) = p2.upgrade() {
                            p.update(cx, |panel, cx| {
                                panel.sync_all_events(window, cx);
                            });
                        }
                    });
                });
            }
        })
    }
}

impl Render for EventDrag {
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
            .child(format!("📡 {}", self.event_name))
    }
}
