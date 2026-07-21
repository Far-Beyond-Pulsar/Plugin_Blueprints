//! Unified multi-mode Properties panel.
//!
//! Dispatches to the appropriate detail renderer based on the current
//! selection type (prefab component, macro, event, variable, graph node,
//! or comment).  Mutual exclusivity of selection types is enforced by the
//! sidebar / graph click handlers — the renderer simply reads whichever
//! selection field is populated.

use gpui::prelude::FluentBuilder;
use gpui::*;
use pulsar_reflection::{TypeStructure, REGISTRY};
use ui::{
    button::Button,
    color_picker::{ColorPickerEvent, ColorPickerState},
    input::{InputEvent, InputState},
    scroll::ScrollbarAxis,
    CollapsibleSection, IconName,
};
use ui::{
    button::ButtonVariants as _, h_flex, v_flex, ActiveTheme as _, Colorize, PixelsExt, Sizable,
    StyledExt,
};

use crate::core::types::{BlueprintComment, BlueprintNode, NodeType, Pin};
use crate::editor::panel::BlueprintEditorPanel;
use crate::editor::workspace_panels::GraphCanvasPanel;
use crate::features::connections::compatibility::is_pin_connected;
use crate::features::prefabs::panel::group_rows_by_category;
use ui_common::properties_inspector;
use ui_common::reflected_properties_panel::rgba_to_hsla;
use std::sync::Arc;

/// Unified multi-mode Properties panel renderer.
///
/// Dispatches to the appropriate sub-renderer based on selection priority:
/// prefab component → macro → event → variable → graph node → comment → empty.
pub struct PropertiesRenderer;

#[derive(Clone, Copy, Debug, PartialEq)]
enum SelectionKind {
    PrefabComponent,
    Macro,
    Event,
    Variable,
    GraphNode(usize),
    Comment(usize),
    Multi,
    None,
}

impl PropertiesRenderer {
    pub fn render(
        panel: &mut BlueprintEditorPanel,
        window: &mut Window,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        let active_canvas = panel.active_canvas().cloned();
        if let Some(canvas) = active_canvas.as_ref() {
            canvas.update(cx, |canvas, cx| canvas.sync_comment_inspector_state(window, cx));
        }

        let selection_kind = Self::active_selection_kind(panel, &active_canvas, cx);

        v_flex()
            .size_full()
            .bg(cx.theme().sidebar)
            .child(Self::render_header(selection_kind, cx))
            .child(
                v_flex()
                    .flex_1()
                    .overflow_hidden()
                    .child(
                        div().size_full().scrollable(ScrollbarAxis::Vertical).child(
                            Self::render_properties_content(panel, window, cx),
                        ),
                    ),
            )
    }

    fn active_selection_kind(
        panel: &BlueprintEditorPanel,
        active_canvas: &Option<Entity<GraphCanvasPanel>>,
        cx: &Context<BlueprintEditorPanel>,
    ) -> SelectionKind {
        if panel.selected_prefab_component.is_some() {
            return SelectionKind::PrefabComponent;
        }
        if panel.selected_macro.is_some() {
            return SelectionKind::Macro;
        }
        if panel.selected_event.is_some() {
            return SelectionKind::Event;
        }
        if panel.selected_variable.is_some() {
            return SelectionKind::Variable;
        }
        if let Some(canvas) = active_canvas {
            let graph = &canvas.read(cx).graph;
            let n = graph.selected_nodes.len();
            let c = graph.selected_comments.len();
            if n > 1 || (n > 0 && c > 0) || c > 1 {
                return SelectionKind::Multi;
            }
            if n == 1 {
                return SelectionKind::GraphNode(1);
            }
            if c == 1 {
                return SelectionKind::Comment(1);
            }
        }
        SelectionKind::None
    }

    fn render_header(selection_kind: SelectionKind, cx: &Context<BlueprintEditorPanel>) -> impl IntoElement {
        let (title, icon, badge) = match &selection_kind {
            SelectionKind::PrefabComponent => ("Properties", IconName::Component, "Component"),
            SelectionKind::Macro => ("Properties", IconName::GitBranch, "Macro"),
            SelectionKind::Event => ("Properties", IconName::Flash, "Event"),
            SelectionKind::Variable => ("Properties", IconName::Component, "Variable"),
            SelectionKind::GraphNode(_) => ("Properties", IconName::Component, "Node"),
            SelectionKind::Comment(_) => ("Properties", IconName::Info, "Comment"),
            SelectionKind::Multi => ("Properties", IconName::Copy, "Multiple"),
            SelectionKind::None => ("Properties", IconName::Settings, "None"),
        };

        let has_selection = !matches!(selection_kind, SelectionKind::None);

        properties_inspector::render_header(title, has_selection, badge, "properties-more", cx)
    }

    fn render_properties_content(
        panel: &mut BlueprintEditorPanel,
        window: &mut Window,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
        // ── Prefab component selected ──────────────────────────────────────
        if let Some(index) = panel.selected_prefab_component {
            return Self::render_prefab_component_properties(panel, index, window, cx);
        }

        // ── Macro selected ─────────────────────────────────────────────────
        if let Some(index) = panel.selected_macro {
            return Self::render_macro_details(panel, index, cx);
        }

        // ── Event selected ─────────────────────────────────────────────────
        if let Some(index) = panel.selected_event {
            return Self::render_event_details(panel, index, cx);
        }

        // ── Variable selected ──────────────────────────────────────────────
        if let Some(index) = panel.selected_variable {
            return Self::render_variable_details(panel, index, cx);
        }

        let active_canvas_opt = panel.active_canvas().cloned();
        let canvas_ref = active_canvas_opt.as_ref().map(|c| c.read(cx));
        let Some(canvas) = canvas_ref else {
            return Self::render_empty_state(cx);
        };

        let sel_nodes = canvas.graph.selected_nodes.clone();
        let sel_comments = canvas.graph.selected_comments.clone();
        let sel_count = sel_nodes.len();
        let com_count = sel_comments.len();

        // ── Single comment selected ──────────────────────────────────────
        if com_count == 1 && sel_count == 0 {
            let comment_id = &sel_comments[0];
            let selected_comment = canvas
                .graph
                .comments
                .iter()
                .find(|c| &c.id == comment_id)
                .cloned();
            if let Some(comment) = selected_comment {
                return Self::render_comment_properties(panel, &comment, window, cx);
            }
            return Self::render_empty_state(cx);
        }

        // ── Single node selected ──────────────────────────────────────────
        if sel_count == 1 && com_count == 0 {
            let selected_node_id = &sel_nodes[0];
            let node_found = canvas.graph.nodes.iter().any(|n| &n.id == selected_node_id);
            if !node_found {
                return Self::render_empty_state(cx);
            }
            if let Some(active_canvas) = active_canvas_opt {
                return active_canvas.update(cx, |canvas, cx| {
                    Self::render_selected_node_properties(canvas, window, cx)
                });
            }
        }

        // ── Multi-selection ───────────────────────────────────────────────
        if sel_count > 1 || com_count > 0 {
            return Self::render_multi_selection_state(sel_count, com_count, cx);
        }

        // ── Nothing selected ──────────────────────────────────────────────
        Self::render_empty_state(cx)
    }

    // ── Prefab component properties ──────────────────────────────────────────

    fn render_prefab_component_properties(
        panel: &mut BlueprintEditorPanel,
        index: usize,
        window: &mut Window,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
        let Some(component) = panel.prefab_asset.components.get(index).cloned() else {
            return Self::render_empty_state(cx);
        };

        let class_name = component.class_name.clone();
        let state_key = format!("{}#{}", index, class_name);
        let mut missing_in_registry = false;
        let mut row_data: Vec<(AnyElement, Option<String>, Option<String>, bool, Option<usize>)> =
            Vec::new();

        if let Some(instance) = REGISTRY.create_instance(&class_name) {
            let panel_entity = cx.entity().clone();
            let on_bool_toggle = Arc::new(
                move |prop_name: &str, checked: bool, _window: &mut Window, cx: &mut App| {
                    panel_entity.update(cx, |panel, cx| {
                        panel.update_prefab_component_property(
                            index,
                            prop_name,
                            serde_json::Value::Bool(checked),
                        );
                        cx.notify();
                    });
                },
            );

            let panel_entity = cx.entity().clone();
            let on_enum_select = Arc::new(
                move |prop_name: &str, ix: usize, _window: &mut Window, cx: &mut App| {
                    panel_entity.update(cx, |panel, cx| {
                        panel.update_prefab_component_property(
                            index,
                            prop_name,
                            serde_json::Value::from(ix as u64),
                        );
                        cx.notify();
                    });
                },
            );

            for prop in instance.get_properties() {
                let current_value = component
                    .data
                    .as_object()
                    .and_then(|obj| obj.get(prop.name))
                    .cloned()
                    .unwrap_or_else(|| {
                        let default_value = (prop.getter)(instance.as_ref());
                        pulsar_reflection::RUNTIME_TYPE_REGISTRY
                            .serialize_json_for_any(default_value.as_ref())
                            .unwrap_or(serde_json::json!(null))
                    });

                match &prop.type_info.structure {
                    TypeStructure::Primitive if prop.type_info.base_name() == "f32" => {
                        let v = current_value.as_f64().unwrap_or(0.0) as f32;
                        Self::ensure_numeric_input(panel, &state_key, prop.name, index, v, false, window, cx);
                    }
                    TypeStructure::Primitive if prop.type_info.base_name() == "i32" => {
                        let v = current_value.as_i64().unwrap_or(0) as f32;
                        Self::ensure_numeric_input(panel, &state_key, prop.name, index, v, true, window, cx);
                    }
                    _ => {}
                }

                let is_color = matches!(
                    &prop.type_info.structure,
                    TypeStructure::Primitive if prop.type_info.base_name() == "[f32; 4]"
                ) || ui_common::reflected_properties_panel::is_color_field_name(prop.name);

                if is_color {
                    let rgba = ui_common::reflected_properties_panel::json_to_rgba_fallback(&current_value);
                    Self::ensure_color_picker(panel, &state_key, prop.name, index, rgba, window, cx);
                }

                let widgets = panel.prefab_property_state.widget_map_for(&state_key, prop.name);

                let prop_bool = prop.name.to_string();
                let on_bool = on_bool_toggle.clone();
                let bool_callback = Arc::new(
                    move |checked: bool, window: &mut Window, cx: &mut App| {
                        (on_bool)(&prop_bool, checked, window, cx);
                    },
                );

                let prop_enum = prop.name.to_string();
                let on_enum = on_enum_select.clone();
                let enum_callback = Arc::new(move |ix: usize, window: &mut Window, cx: &mut App| {
                    (on_enum)(&prop_enum, ix, window, cx);
                });

                let row = ui_common::render_property_row_runtime(
                    "prefab",
                    &state_key,
                    &prop.display_name,
                    prop.name,
                    prop.type_info,
                    &current_value,
                    widgets,
                    bool_callback,
                    enum_callback,
                    cx,
                );

                row_data.push((
                    row,
                    prop.category.map(str::to_string),
                    prop.category_color.map(str::to_string),
                    prop.category_default_collapsed,
                    prop.category_order,
                ));
            }
        } else {
            missing_in_registry = true;
        }

        v_flex()
            .w_full()
            .p_3()
            .gap_4()
            .min_w_full()
            .child(
                CollapsibleSection::new(class_name.clone())
                    .icon(IconName::Settings)
                    .open(true)
                    .child(if missing_in_registry {
                        div()
                            .text_sm()
                            .text_color(cx.theme().warning)
                            .child(
                                "This component class is not available in the reflection registry.",
                            )
                            .into_any_element()
                    } else {
                        let (mut uncategorized, categorized) = group_rows_by_category(row_data);
                        let category_elements =
                            Self::render_categorized_rows(panel, index, categorized, cx);
                        uncategorized.extend(category_elements);

                        v_flex()
                            .gap_2()
                            .children(uncategorized)
                            .into_any_element()
                    }),
            )
            .into_any_element()
    }

    fn ensure_numeric_input(
        panel: &mut BlueprintEditorPanel,
        state_key: &str,
        prop_name: &str,
        component_index: usize,
        initial: f32,
        is_integer: bool,
        window: &mut Window,
        cx: &mut Context<BlueprintEditorPanel>,
    ) {
        let key = (state_key.to_string(), prop_name.to_string());
        if panel.prefab_property_state.numeric_inputs.contains_key(&key) {
            return;
        }

        let text = if is_integer {
            format!("{}", initial as i64)
        } else {
            format!("{:.3}", initial)
        };
        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |state, cx| {
            state.set_value(&text, window, cx);
        });

        let pn = prop_name.to_string();
        cx.subscribe_in(
            &input,
            window,
            move |this: &mut BlueprintEditorPanel, state, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change | InputEvent::Blur) {
                    let text = state.read(cx).text().to_string();
                    let parsed = if is_integer {
                        text.trim().parse::<i32>().ok().map(serde_json::Value::from)
                    } else {
                        text.trim().parse::<f32>().ok().map(serde_json::Value::from)
                    };
                    if let Some(value) = parsed {
                        this.update_prefab_component_property(component_index, &pn, value);
                        cx.notify();
                    }
                }
            },
        )
        .detach();

        panel.prefab_property_state.numeric_inputs.insert(key, input);
    }

    fn ensure_color_picker(
        panel: &mut BlueprintEditorPanel,
        state_key: &str,
        prop_name: &str,
        component_index: usize,
        rgba: [f32; 4],
        window: &mut Window,
        cx: &mut Context<BlueprintEditorPanel>,
    ) {
        let key = (state_key.to_string(), prop_name.to_string());
        if panel.prefab_property_state.color_pickers.contains_key(&key) {
            return;
        }

        let picker = cx.new(|cx| {
            let mut state = ColorPickerState::new(window, cx);
            state.set_value(
                ui_common::reflected_properties_panel::rgba_to_hsla(rgba),
                window,
                cx,
            );
            state
        });

        let pn = prop_name.to_string();
        cx.subscribe_in(
            &picker,
            window,
            move |this: &mut BlueprintEditorPanel, _state, event: &ColorPickerEvent, _window, cx| {
                if let ColorPickerEvent::Change(Some(hsla)) = event {
                    this.update_prefab_component_property(
                        component_index,
                        &pn,
                        serde_json::json!(ui_common::reflected_properties_panel::hsla_to_rgba(*hsla)),
                    );
                    cx.notify();
                }
            },
        )
        .detach();

        panel.prefab_property_state.color_pickers.insert(key, picker);
    }

    fn render_categorized_rows(
        panel: &BlueprintEditorPanel,
        component_index: usize,
        mut categorized_rows: Vec<(String, Vec<AnyElement>, Option<String>, bool, usize)>,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> Vec<AnyElement> {
        categorized_rows.sort_by_key(|(_, _, _, _, order)| *order);

        categorized_rows
            .into_iter()
            .map(|(category_name, category_rows, category_color_hex, default_collapsed, _)| {
                let category_key = (component_index, category_name.clone());

                let is_collapsed = if panel.prefab_collapsed_categories.contains(&category_key) {
                    true
                } else if panel.prefab_expanded_categories.contains(&category_key) {
                    false
                } else {
                    default_collapsed
                };

                let toggle_key = category_key.clone();
                let was_collapsed = is_collapsed;
                let accent = category_color_hex
                    .as_deref()
                    .and_then(crate::features::viewport::coordinates::parse_hex_color);

                v_flex()
                    .w_full()
                    .gap_1()
                    .p_2()
                    .rounded(px(6.0))
                    .border_1()
                    .when_some(accent, |el, color| {
                        el.border_color(color.opacity(0.7)).bg(color.opacity(0.08))
                    })
                    .when(accent.is_none(), |el| {
                        el.border_color(cx.theme().border)
                            .bg(cx.theme().border.opacity(0.08))
                    })
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    if was_collapsed {
                                        this.prefab_collapsed_categories.remove(&toggle_key);
                                        this.prefab_expanded_categories
                                            .insert(toggle_key.clone());
                                    } else {
                                        this.prefab_expanded_categories.remove(&toggle_key);
                                        this.prefab_collapsed_categories
                                            .insert(toggle_key.clone());
                                    }
                                    cx.notify();
                                }),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .when_some(accent, |el, color| el.text_color(color))
                                    .when(accent.is_none(), |el| {
                                        el.text_color(cx.theme().muted_foreground)
                                    })
                                    .child(category_name),
                            )
                            .child(
                                ui::Icon::new(if is_collapsed {
                                    IconName::ChevronRight
                                } else {
                                    IconName::ChevronDown
                                })
                                .xsmall()
                                .when_some(accent, |el, color| el.text_color(color))
                                .when(accent.is_none(), |el| {
                                    el.text_color(cx.theme().muted_foreground)
                                }),
                            ),
                    )
                    .when(!is_collapsed, |el| el.children(category_rows))
                    .into_any_element()
            })
            .collect()
    }

    // ── Macro details ────────────────────────────────────────────────────────

    fn render_macro_details(
        panel: &BlueprintEditorPanel,
        index: usize,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
        let Some(macro_def) = panel.local_macros.get(index) else {
            return Self::render_empty_state(cx);
        };

        v_flex()
            .gap_4()
            .p_3()
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_3()
                            .child(
                                ui::Icon::new(IconName::GitBranch)
                                    .size(px(18.0))
                                    .text_color(cx.theme().accent),
                            )
                            .child(
                                div()
                                    .text_lg()
                                    .font_bold()
                                    .text_color(cx.theme().foreground)
                                    .child(macro_def.name.clone()),
                            ),
                    )
                    .when(!macro_def.description.is_empty(), |el| {
                        el.child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(macro_def.description.clone()),
                        )
                    }),
            )
            .child(Self::render_separator(cx))
            .child(
                v_flex()
                    .gap_3()
                    .child(Self::render_section_header("Inputs", IconName::ArrowRight, cx))
                    .child(
                        v_flex()
                            .gap_1p5()
                            .children(macro_def.interface.inputs.iter().map(|pin| {
                                Self::render_info_row(&pin.name, &pin.data_type.to_string(), cx)
                            }))
                            .when(macro_def.interface.inputs.is_empty(), |el| {
                                el.child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("No inputs"),
                                )
                            }),
                    ),
            )
            .child(Self::render_separator(cx))
            .child(
                v_flex()
                    .gap_3()
                    .child(Self::render_section_header("Outputs", IconName::ArrowRight, cx))
                    .child(
                        v_flex()
                            .gap_1p5()
                            .children(macro_def.interface.outputs.iter().map(|pin| {
                                Self::render_info_row(&pin.name, &pin.data_type.to_string(), cx)
                            }))
                            .when(macro_def.interface.outputs.is_empty(), |el| {
                                el.child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("No outputs"),
                                )
                            }),
                    ),
            )
            .child(Self::render_separator(cx))
            .child(
                v_flex()
                    .gap_3()
                    .child(Self::render_section_header("Macro Info", IconName::Info, cx))
                    .child(Self::render_info_row("ID", &macro_def.id, cx))
                    .child(Self::render_info_row(
                        "Nodes",
                        &macro_def.graph.nodes.len().to_string(),
                        cx,
                    )),
            )
            .into_any_element()
    }

    // ── Event details ────────────────────────────────────────────────────────

    fn render_event_details(
        panel: &BlueprintEditorPanel,
        index: usize,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
        let Some(event_def) = panel.local_event_defs.get(index) else {
            return Self::render_empty_state(cx);
        };

        v_flex()
            .gap_4()
            .p_3()
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_3()
                            .child(
                ui::Icon::new(IconName::Flash)
                    .size(px(18.0))
                    .text_color(cx.theme().warning),
                            )
                            .child(
                                div()
                                    .text_lg()
                                    .font_bold()
                                    .text_color(cx.theme().foreground)
                                    .child(event_def.name.clone()),
                            ),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded(px(4.0))
                            .bg(cx.theme().warning.opacity(0.15))
                            .border_1()
                            .border_color(cx.theme().warning.opacity(0.3))
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().warning)
                            .child("Custom Event"),
                    ),
            )
            .child(Self::render_separator(cx))
            .child(
                v_flex()
                    .gap_3()
                    .child(Self::render_section_header("Fields", IconName::List, cx))
                    .child(
                        v_flex()
                            .gap_1p5()
                            .children(event_def.fields.iter().map(|field| {
                                Self::render_info_row(&field.name, &field.type_name, cx)
                            }))
                            .when(event_def.fields.is_empty(), |el| {
                                el.child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("No fields"),
                                )
                            }),
                    ),
            )
            .when(!event_def.return_type.is_empty(), |el| {
                el.child(Self::render_separator(cx)).child(
                    v_flex()
                        .gap_3()
                        .child(Self::render_section_header("Return Type", IconName::ArrowRight, cx))
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().foreground)
                                .child(event_def.return_type.clone()),
                        ),
                )
            })
            .child(Self::render_separator(cx))
            .child(
                v_flex()
                    .gap_3()
                    .child(Self::render_section_header("Event Info", IconName::Info, cx))
                    .child(Self::render_info_row("UID", &event_def.uid, cx))
            )
            .into_any_element()
    }

    // ── Variable details ─────────────────────────────────────────────────────

    fn render_variable_details(
        panel: &BlueprintEditorPanel,
        index: usize,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
        let Some(var) = panel.class_variables.get(index) else {
            return Self::render_empty_state(cx);
        };

        v_flex()
            .gap_4()
            .p_3()
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_3()
                            .child(
                                ui::Icon::new(IconName::Component)
                                    .size(px(18.0))
                                    .text_color(cx.theme().info),
                            )
                            .child(
                                div()
                                    .text_lg()
                                    .font_bold()
                                    .text_color(cx.theme().foreground)
                                    .child(var.name.clone()),
                            ),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded(px(4.0))
                            .bg(cx.theme().info.opacity(0.15))
                            .border_1()
                            .border_color(cx.theme().info.opacity(0.3))
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().info)
                            .child("Variable"),
                    ),
            )
            .child(Self::render_separator(cx))
            .child(
                v_flex()
                    .gap_3()
                    .child(Self::render_section_header("Variable Info", IconName::Info, cx))
                    .child(Self::render_info_row("Type", &var.var_type, cx))
                    .child(
                        Self::render_info_row(
                            "Default Value",
                            &var.default_value.clone().unwrap_or_else(|| "—".to_string()),
                            cx,
                        ),
                    ),
            )
            .into_any_element()
    }

    fn render_selected_node_readonly<T>(
        selected_node: &BlueprintNode,
        cx: &mut Context<T>,
    ) -> AnyElement {
        v_flex()
            .gap_4()
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_3()
                            .child(div().text_2xl().child(selected_node.icon.clone()))
                            .child(
                                div()
                                    .text_lg()
                                    .font_bold()
                                    .text_color(cx.theme().foreground)
                                    .child(selected_node.title.clone()),
                            ),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded(px(4.0))
                            .bg(Self::get_node_type_color(&selected_node.node_type, cx).opacity(0.15))
                            .border_1()
                            .border_color(
                                Self::get_node_type_color(&selected_node.node_type, cx).opacity(0.3),
                            )
                            .text_xs()
                            .font_semibold()
                            .text_color(Self::get_node_type_color(&selected_node.node_type, cx))
                            .child(format!("{:?} Node", selected_node.node_type)),
                    ),
            )
            .when(!selected_node.inputs.is_empty(), |el| {
                el.child(Self::render_separator(cx)).child(
                    v_flex()
                        .gap_3()
                        .child(Self::render_section_header("Inputs", IconName::ArrowRight, cx))
                        .child(Self::render_pin_list(&selected_node.inputs, cx)),
                )
            })
            .when(!selected_node.outputs.is_empty(), |el| {
                el.child(Self::render_separator(cx)).child(
                    v_flex()
                        .gap_3()
                        .child(Self::render_section_header("Outputs", IconName::ArrowRight, cx))
                        .child(Self::render_pin_list(&selected_node.outputs, cx)),
                )
            })
            .when(!selected_node.properties.is_empty(), |el| {
                el.child(Self::render_separator(cx)).child(
                    v_flex()
                        .gap_3()
                        .child(Self::render_section_header("Properties", IconName::Settings, cx))
                        .child(Self::render_node_properties(selected_node, cx)),
                )
            })
            .child(Self::render_separator(cx))
            .child(
                v_flex()
                    .gap_3()
                    .child(Self::render_section_header("Node Info", IconName::Info, cx))
                    .child(Self::render_node_info(selected_node, cx)),
            )
            .into_any_element()
    }

    fn render_selected_node_properties(
        canvas: &mut GraphCanvasPanel,
        window: &mut Window,
        cx: &mut Context<GraphCanvasPanel>,
    ) -> AnyElement {
        let selected_node_id = canvas.graph.selected_nodes.first().cloned();
        let Some(selected_node_id) = selected_node_id else {
            return Self::render_empty_state(cx);
        };
        let Some(selected_node) = canvas.graph.nodes.iter().find(|n| n.id == selected_node_id).cloned()
        else {
            return Self::render_empty_state(cx);
        };

        let canvas_entity = cx.entity().clone();

        v_flex()
            .gap_4()
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_3()
                            .child(div().text_2xl().child(selected_node.icon.clone()))
                            .child(
                                div()
                                    .text_lg()
                                    .font_bold()
                                    .text_color(cx.theme().foreground)
                                    .child(selected_node.title.clone()),
                            ),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded(px(4.0))
                            .bg(Self::get_node_type_color(&selected_node.node_type, cx).opacity(0.15))
                            .border_1()
                            .border_color(
                                Self::get_node_type_color(&selected_node.node_type, cx).opacity(0.3),
                            )
                            .text_xs()
                            .font_semibold()
                            .text_color(Self::get_node_type_color(&selected_node.node_type, cx))
                            .child(format!("{:?} Node", selected_node.node_type)),
                    ),
            )
            .when(!selected_node.inputs.is_empty(), |el| {
                el.child(Self::render_separator(cx)).child(
                    v_flex()
                        .gap_3()
                        .child(Self::render_section_header("Inputs", IconName::ArrowRight, cx))
                        .child(Self::render_pin_editors(
                            canvas,
                            &canvas_entity,
                            &selected_node,
                            window,
                            cx,
                        )),
                )
            })
            .when(!selected_node.outputs.is_empty(), |el| {
                el.child(Self::render_separator(cx)).child(
                    v_flex()
                        .gap_3()
                        .child(Self::render_section_header("Outputs", IconName::ArrowRight, cx))
                        .child(Self::render_pin_list(&selected_node.outputs, cx)),
                )
            })
            .when(!selected_node.properties.is_empty(), |el| {
                el.child(Self::render_separator(cx)).child(
                    v_flex()
                        .gap_3()
                        .child(Self::render_section_header("Properties", IconName::Settings, cx))
                        .child(Self::render_node_properties(&selected_node, cx)),
                )
            })
            .child(Self::render_separator(cx))
            .child(
                v_flex()
                    .gap_3()
                    .child(Self::render_section_header("Node Info", IconName::Info, cx))
                    .child(Self::render_node_info(&selected_node, cx)),
            )
            .into_any_element()
    }

    fn render_comment_properties(
        panel: &BlueprintEditorPanel,
        comment: &BlueprintComment,
        _window: &mut Window,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
        let active_canvas = panel.active_canvas().cloned();
        let mut comment_color = comment.color;
        let mut color_picker = None;
        let mut comment_text_input = None;

        if let Some(canvas) = active_canvas {
            let canvas_state = canvas.read(cx);
            comment_text_input = Some(canvas_state.comment_text_input.clone());
            if let Some(selected) = canvas_state.graph.comments.iter().find(|c| c.id == comment.id) {
                comment_color = selected.color;
                color_picker = selected.color_picker_state.clone();
            }
        }

        v_flex()
            .gap_4()
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_3()
                            .child(
                                ui::Icon::new(IconName::Info)
                                    .size(px(18.0))
                                    .text_color(cx.theme().info),
                            )
                            .child(
                                div()
                                    .text_lg()
                                    .font_bold()
                                    .text_color(cx.theme().foreground)
                                    .child(comment.text.clone()),
                            ),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded(px(4.0))
                            .bg(comment_color.opacity(0.15))
                            .border_1()
                            .border_color(comment_color.opacity(0.3))
                            .text_xs()
                            .font_semibold()
                            .text_color(comment_color)
                            .child("Comment"),
                    ),
            )
            .child(Self::render_separator(cx))
            .child(
                v_flex()
                    .gap_3()
                    .child(Self::render_section_header(
                        "Comment Properties",
                        IconName::Settings,
                        cx,
                    ))
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Name"),
                            )
                            .child(
                                comment_text_input
                                    .map(|input| div().w_full().child(input).into_any_element())
                                    .unwrap_or_else(|| {
                                        div()
                                            .w_full()
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground)
                                            .child("No comment editor available")
                                            .into_any_element()
                                    }),
                            ),
                    )
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Color"),
                            )
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .w(px(24.0))
                                            .h(px(24.0))
                                            .rounded(px(4.0))
                                            .border_1()
                                            .border_color(cx.theme().border)
                                            .bg(comment_color)
                                            .into_any_element(),
                                    )
                                    .child(color_picker.map(|picker| {
                                        div().w_full().child(picker).into_any_element()
                                    }).unwrap_or_else(|| {
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child("Color picker unavailable")
                                            .into_any_element()
                                    })),
                            ),
                    ),
            )
            .child(Self::render_separator(cx))
            .child(
                v_flex()
                    .gap_3()
                    .child(Self::render_section_header("Comment Info", IconName::Info, cx))
                    .child(Self::render_info_row("Comment ID", &comment.id, cx))
                    .child(Self::render_info_row(
                        "Position",
                        &format!("({:.0}, {:.0})", comment.position.x, comment.position.y),
                        cx,
                    ))
                    .child(Self::render_info_row(
                        "Size",
                        &format!("{:.0} × {:.0} px", comment.size.width, comment.size.height),
                        cx,
                    ))
                    .child(Self::render_info_row(
                        "Contained Nodes",
                        &comment.contained_node_ids.len().to_string(),
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_multi_selection_state<T>(
        node_count: usize,
        comment_count: usize,
        cx: &mut Context<T>,
    ) -> AnyElement {
        let summary = match (node_count, comment_count) {
            (n, 0) => format!("{} nodes selected", n),
            (0, c) => format!("{} comments selected", c),
            (n, c) => format!("{} nodes, {} comments selected", n, c),
        };
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_2()
            .child(
                ui::Icon::new(IconName::Copy)
                    .size(px(20.0))
                    .text_color(cx.theme().muted_foreground.opacity(0.5)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(summary),
            )
            .into_any_element()
    }

    fn render_section_header<T>(
        title: &str,
        _icon: IconName,
        cx: &mut Context<T>,
    ) -> impl IntoElement {
        h_flex().items_center().gap_2().child(
            div()
                .text_xs()
                .font_bold()
                .text_color(cx.theme().accent)
                .child(title.to_uppercase()),
        )
    }

    fn render_separator<T>(cx: &mut Context<T>) -> impl IntoElement {
        div().w_full().h_px().bg(cx.theme().border.opacity(0.3))
    }

    fn get_node_type_color<T>(
        node_type: &NodeType,
        cx: &mut Context<T>,
    ) -> gpui::Hsla {
        match node_type {
            NodeType::Event => cx.theme().danger,
            NodeType::Logic => cx.theme().primary,
            NodeType::Math => cx.theme().success,
            NodeType::Object => cx.theme().warning,
            NodeType::Reroute => cx.theme().accent,
            NodeType::MacroEntry => gpui::Hsla {
                h: 0.75,
                s: 0.7,
                l: 0.6,
                a: 1.0,
            },
            NodeType::MacroExit => gpui::Hsla {
                h: 0.75,
                s: 0.7,
                l: 0.6,
                a: 1.0,
            },
            NodeType::MacroInstance => gpui::Hsla {
                h: 0.75,
                s: 0.5,
                l: 0.5,
                a: 1.0,
            },
            NodeType::CustomEvent => gpui::Hsla {
                h: 0.08,
                s: 0.8,
                l: 0.5,
                a: 1.0,
            },
            NodeType::CustomEventDispatch => gpui::Hsla {
                h: 0.55,
                s: 0.8,
                l: 0.5,
                a: 1.0,
            },
        }
    }

    /// Compact, centered placeholder shown when there's nothing to inspect —
    /// matches the empty-details state of professional editors (Unreal/Unity).
    fn render_empty_state<T>(cx: &mut Context<T>) -> AnyElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_2()
            .child(
                ui::Icon::new(IconName::Component)
                    .size(px(20.0))
                    .text_color(cx.theme().muted_foreground.opacity(0.5)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Select a node to view its properties"),
            )
            .into_any_element()
    }

    /// Render a pin list section — type display (badge color, resolved name)
    /// is sourced entirely from `PinDataType`/`RuntimeTypeInfo`, the same
    /// canonical reflection-backed lookup the graph view uses for pin colors,
    /// so the panel and graph always agree visually.
    fn render_pin_list<T>(
        pins: &[Pin],
        cx: &mut Context<T>,
    ) -> impl IntoElement {
        v_flex()
            .gap_1p5()
            .children(pins.iter().map(|pin| Self::render_pin_row(pin, cx)))
    }

    fn render_pin_editors(
        canvas: &mut GraphCanvasPanel,
        canvas_entity: &Entity<GraphCanvasPanel>,
        node: &BlueprintNode,
        window: &mut Window,
        cx: &mut Context<GraphCanvasPanel>,
    ) -> impl IntoElement {
        v_flex()
            .gap_1p5()
            .children(node.inputs.iter().map(|pin| {
                Self::render_input_pin_row(canvas, canvas_entity, node, pin, window, cx)
            }))
    }

    fn render_input_pin_row(
        canvas: &mut GraphCanvasPanel,
        canvas_entity: &Entity<GraphCanvasPanel>,
        node: &BlueprintNode,
        pin: &Pin,
        window: &mut Window,
        cx: &mut Context<GraphCanvasPanel>,
    ) -> AnyElement {
        let row = Self::render_pin_row(pin, cx);
        if is_pin_connected(&node.id, &pin.id, true, &canvas.graph) {
            return row.into_any_element();
        }

        let Some(type_info) = pin.data_type.runtime_type() else {
            return row.into_any_element();
        };

        let state_key = format!("{}#{}", node.id, pin.id);
        let widgets = canvas.pin_property_state.widget_map_for(&state_key, &pin.id);
        let current_value = Self::read_pin_property_value(node, &pin.id);
        let node_id = node.id.clone();
        let pin_id = pin.id.clone();
        let canvas_for_bool = canvas_entity.clone();
        let on_bool_toggle = Arc::new(
            move |checked: bool, _window: &mut Window, cx: &mut App| {
                canvas_for_bool.update(cx, |canvas, cx| {
                    canvas.update_node_input_property(&node_id, &pin_id, serde_json::Value::Bool(checked), cx);
                });
            },
        );

        let canvas_for_enum = canvas_entity.clone();
        let node_id_for_enum = node.id.clone();
        let pin_id_for_enum = pin.id.clone();
        let on_enum_select = Arc::new(
            move |ix: usize, _window: &mut Window, cx: &mut App| {
                canvas_for_enum.update(cx, |canvas, cx| {
                    canvas.update_node_input_property(
                        &node_id_for_enum,
                        &pin_id_for_enum,
                        serde_json::Value::from(ix as u64),
                        cx,
                    );
                });
            },
        );

        let editor = ui_common::render_property_row_runtime(
            "node-input",
            &state_key,
            &Self::format_property_name(&pin.name),
            &pin.id,
            type_info,
            &current_value,
            widgets,
            on_bool_toggle,
            on_enum_select,
            cx,
        );

        v_flex()
            .gap_1p5()
            .child(row)
            .child(editor)
            .into_any_element()
    }

    fn read_pin_property_value(node: &BlueprintNode, pin_id: &str) -> serde_json::Value {
        let Some(raw_value) = node.properties.get(pin_id) else {
            return serde_json::Value::Null;
        };

        serde_json::from_str(raw_value)
            .unwrap_or_else(|_| serde_json::Value::String(raw_value.clone()))
    }

    fn render_pin_row<T>(pin: &Pin, cx: &mut Context<T>) -> impl IntoElement {
        let badge_color: gpui::Hsla = rgba_to_hsla(pin.data_type.display_color()).into();

        let type_label = if pin.data_type.is_execution() {
            "Execution".to_string()
        } else if pin.data_type.is_wildcard() {
            "Wildcard".to_string()
        } else {
            pin.data_type.type_name.clone()
        };

        h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .px_3()
            .py_2()
            .rounded(px(4.0))
            .hover(|style| style.bg(cx.theme().muted.opacity(0.1)))
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .size(px(8.0))
                            .rounded_full()
                            .bg(badge_color),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(cx.theme().foreground)
                            .child(if pin.name.is_empty() {
                                "(unnamed)".to_string()
                            } else {
                                pin.name.clone()
                            }),
                    ),
            )
            .child(
                div()
                    .px_2()
                    .py_1()
                    .rounded(px(4.0))
                    .bg(badge_color.opacity(0.15))
                    .border_1()
                    .border_color(badge_color.opacity(0.4))
                    .text_xs()
                    .font_family("JetBrainsMono-Regular")
                    .text_color(badge_color)
                    .child(type_label),
            )
    }

    fn render_node_properties<T>(
        node: &BlueprintNode,
        cx: &mut Context<T>,
    ) -> impl IntoElement {
        v_flex().gap_3().children(
            node.properties
                .iter()
                .map(|(key, value)| Self::render_property_field(key, value, cx)),
        )
    }

    fn render_property_field<T>(
        key: &str,
        value: &str,
        cx: &mut Context<T>,
    ) -> impl IntoElement {
        v_flex()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .child(Self::format_property_name(key)),
            )
            .child(
                div()
                    .w_full()
                    .px_3()
                    .py_2p5()
                    .bg(cx.theme().input)
                    .border_1()
                    .border_color(cx.theme().border.opacity(0.6))
                    .rounded(px(6.0))
                    .text_sm()
                    .text_color(cx.theme().foreground)
                    .child(value.to_string())
                    .cursor_pointer()
                    .hover(|style| {
                        style
                            .border_color(cx.theme().accent.opacity(0.8))
                            .bg(cx.theme().input.lighten(0.02))
                    }),
            )
    }

    fn render_node_info<T>(
        node: &BlueprintNode,
        cx: &mut Context<T>,
    ) -> impl IntoElement {
        v_flex()
            .gap_2p5()
            .child(Self::render_info_row("Node ID", &node.id, cx))
            .child(Self::render_info_row(
                "Position",
                &format!("({:.0}, {:.0})", node.position.x, node.position.y),
                cx,
            ))
            .child(Self::render_info_row(
                "Size",
                &format!("{:.0} × {:.0} px", node.size.width, node.size.height),
                cx,
            ))
    }

    fn render_info_row<T>(
        label: &str,
        value: &str,
        cx: &mut Context<T>,
    ) -> impl IntoElement {
        h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .px_3()
            .py_2()
            .rounded(px(4.0))
            .hover(|style| style.bg(cx.theme().muted.opacity(0.1)))
            .child(
                div()
                    .text_xs()
                    .font_medium()
                    .text_color(cx.theme().muted_foreground)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .px_2()
                    .py_1()
                    .rounded(px(4.0))
                    .bg(cx.theme().muted.opacity(0.2))
                    .text_xs()
                    .font_family("JetBrainsMono-Regular")
                    .text_color(cx.theme().foreground)
                    .child(value.to_string()),
            )
    }

    fn format_property_name(key: &str) -> String {
        // Convert snake_case to Title Case
        key.split('_')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect::<Vec<String>>()
            .join(" ")
    }

}
