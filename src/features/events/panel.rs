//! Events panel — sidebar panel for listing and managing custom events.
//!
//! Mirrors MacrosRenderer.  Each event shows its name and field count.
//! + button creates a new event. Click selects the event in the sidebar.

use super::hierarchy_item::EventHierarchyItem;
use gpui::*;
use std::sync::Arc;
use ui::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme as _, HierarchicalTreeView, HierarchyConfig, HierarchyLayout,
    IconName, StyledExt,
};

use crate::editor::panel::{BlueprintEditorPanel, RenameTarget};

pub struct EventsRenderer;

impl EventsRenderer {
    pub fn render(
        panel: &BlueprintEditorPanel,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        let add_button = Button::new("create-event")
            .icon(IconName::Plus)
            .ghost()
            .compact()
            .tooltip("Create New Event")
            .on_click(cx.listener(|panel, _, window, cx| {
                let uid = panel.create_event_def("NewEvent".to_string(), String::new());
                // Sync only the active graph tab — not all tabs
                if let Some(tab) = panel.open_tabs.get_mut(panel.active_tab_index) {
                    if let Some(def) = panel.local_event_defs.iter().find(|d| d.uid == uid) {
                        crate::editor::panel::BlueprintEditorPanel::sync_event_on_node_for_def(def, &uid, &mut tab.graph);
                    }
                }
                // Also sync the active live canvas if open
                let win_handle = window.window_handle();
                let panel_entity = cx.entity().clone();
                cx.defer(move |cx| {
                    let _ = cx.update_window(win_handle, |_, window, cx| {
                        panel_entity.update(cx, |panel, cx| {
                            if let Some((_, canvas)) = panel.graph_panels.iter().find(|(id, _)| {
                                panel.open_tabs.get(panel.active_tab_index).map(|t| &t.id) == Some(id)
                            }) {
                                canvas.update(cx, |canvas_panel, _cx| {
                                    if let Some(def) = panel.local_event_defs.iter().find(|d| d.uid == uid) {
                                        crate::editor::panel::BlueprintEditorPanel::sync_event_on_node_for_def(def, &uid, &mut canvas_panel.graph);
                                    }
                                });
                            }
                            cx.notify();
                        });
                    });
                });
                cx.notify();
            }))
            .into_any_element();

        Self::render_events_hierarchy(panel, add_button, cx)
    }

    fn render_events_hierarchy(
        panel: &BlueprintEditorPanel,
        add_button: AnyElement,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        let panel_entity = cx.entity().clone();
        let items: Vec<EventHierarchyItem> = panel
            .local_event_defs
            .iter()
            .enumerate()
            .map(|(index, def)| {
                let is_renaming = panel
                    .renaming_target
                    .as_ref()
                    .map_or(false, |t| matches!(t, RenameTarget::Event(uid) if *uid == def.uid));
                EventHierarchyItem {
                    def: def.clone(),
                    index,
                    is_selected: panel.selected_event == Some(index),
                    panel: cx.entity().downgrade(),
                    is_renaming,
                    rename_input: if is_renaming {
                        Some(panel.rename_input.clone())
                    } else {
                        None
                    },
                }
            })
            .collect();

        let root_ids: Vec<usize> = (0..items.len()).collect();

        let config = HierarchyConfig {
            items,
            root_ids,
            layout: HierarchyLayout::Panel,

            title: Some("Custom Events".to_string()),
            header_buttons: vec![add_button],

            root_drop_zone: None,

            widget_title: None,
            widget_icon: None,
            widget_add_button: None,
            empty_message: "No custom events yet\nClick + to create one".to_string(),

            disable_nesting: true,

            is_expanded: Arc::new(|_: &usize| false),
            on_toggle_expand: Arc::new(|_: &usize, _window, _cx| {}),
            on_select: Arc::new(move |id: &usize, window, cx| {
                let selected_id = *id;
                let panel = panel_entity.clone();
                let window_handle = window.window_handle();
                cx.defer(move |cx| {
                    let _ = cx.update_window(window_handle, |_, _window, cx| {
                        panel.update(cx, |panel, cx| {
                            panel.selected_event = Some(selected_id);
                            cx.notify();
                        });
                    });
                });
            }),
            on_drop: Arc::new(
                move |_payload, _target_id: &usize, _modifiers: &Modifiers, _window, _cx| {},
            ),
        };

        HierarchicalTreeView::new(config).render(cx)
    }
}
