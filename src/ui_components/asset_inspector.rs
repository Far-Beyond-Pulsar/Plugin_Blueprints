//! Debounced live view of the sections serialized into a Blueprint save file.

use gpui::prelude::*;
use gpui::*;
use ui::{
    button::{Button, ButtonVariants as _},
    v_flex, ActiveTheme, Sizable,
};

use crate::editor::panel::BlueprintEditorPanel;

#[derive(Clone, Default)]
pub struct AssetInspectorSnapshot {
    pub format_version: u64,
    pub subgraph_count: usize,
    pub sections: Vec<SaveDataSection>,
    pub orphan_subgraphs: Vec<(String, String)>,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct SaveDataSection {
    pub name: &'static str,
    pub summary: String,
    pub json: String,
}

pub struct AssetInspectorRenderer;

impl AssetInspectorRenderer {
    /// Called by the panel's debounced task, never from render.
    pub fn build_snapshot(editor: &mut BlueprintEditorPanel, cx: &App) -> AssetInspectorSnapshot {
        editor.sync_all_canvases_to_tabs(cx);
        let asset = match editor.to_blueprint_asset() {
            Ok(asset) => asset,
            Err(error) => {
                return AssetInspectorSnapshot {
                    error: Some(format!("Could not build save preview: {error}")),
                    ..Default::default()
                };
            }
        };

        let value = match serde_json::to_value(&asset) {
            Ok(value) => value,
            Err(error) => {
                return AssetInspectorSnapshot {
                    error: Some(format!("Could not serialize save preview: {error}")),
                    ..Default::default()
                };
            }
        };

        let mut sections = Vec::new();
        for name in [
            "main_graph",
            "subgraphs",
            "local_events",
            "variables",
            "editor_state",
            "blueprint_metadata",
        ] {
            let section = value.get(name).cloned().unwrap_or_default();
            sections.push(SaveDataSection {
                name,
                summary: section_summary(name, &section),
                json: serde_json::to_string_pretty(&section)
                    .unwrap_or_else(|_| section.to_string()),
            });
        }

        let orphan_subgraphs = editor
            .subgraphs
            .iter()
            .filter(|subgraph| {
                subgraph.kind == blueprint_graph::SubGraphKind::Collapsed
                    && !subgraph_is_referenced(editor, &subgraph.id)
            })
            .map(|subgraph| (subgraph.id.clone(), subgraph.name.clone()))
            .collect();

        AssetInspectorSnapshot {
            format_version: value["format_version"].as_u64().unwrap_or_default(),
            subgraph_count: asset.subgraphs.len(),
            sections,
            orphan_subgraphs,
            error: None,
        }
    }

    pub fn render(
        editor: &mut BlueprintEditorPanel,
        snapshot: &AssetInspectorSnapshot,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let mut content = v_flex()
            .size_full()
            .gap_2()
            .p_3()
            .overflow_y_scroll()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Blueprint Save Data"),
            );

        if let Some(error) = &snapshot.error {
            content = content.child(
                div()
                    .text_sm()
                    .text_color(theme.danger)
                    .child(error.clone()),
            );
        } else {
            content = content.child(div().text_xs().text_color(theme.muted_foreground).child(
                format!(
                    "Debounced live preview · format version {} · {} subgraphs",
                    snapshot.format_version, snapshot.subgraph_count
                ),
            ));
        }

        if !snapshot.orphan_subgraphs.is_empty() {
            content = content.child(
                v_flex()
                    .gap_2()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.warning)
                    .child(div().text_sm().text_color(theme.warning).child(format!(
                        "{} unused collapsed graph(s)",
                        snapshot.orphan_subgraphs.len()
                    )))
                    .children(snapshot.orphan_subgraphs.iter().map(|(id, name)| {
                        let id = id.clone();
                        let name = name.clone();
                        Button::new(format!("remove-orphan-{}", stable_key(&id)))
                            .label(format!("Remove {name}"))
                            .danger()
                            .small()
                            .on_click(cx.listener(move |editor, _, window, cx| {
                                editor.sync_all_canvases_to_tabs(cx);
                                if editor.subgraphs.iter().any(|subgraph| {
                                    subgraph.id == id
                                        && subgraph.kind == blueprint_graph::SubGraphKind::Collapsed
                                        && !subgraph_is_referenced(editor, &id)
                                }) {
                                    let active_tab_id = editor
                                        .open_tabs
                                        .get(editor.active_tab_index)
                                        .map(|tab| tab.id.clone());
                                    editor.subgraphs.retain(|subgraph| subgraph.id != id);
                                    editor.open_tabs.retain(|tab| tab.id != id);
                                    editor.active_tab_index = active_tab_id
                                        .and_then(|active_id| {
                                            editor
                                                .open_tabs
                                                .iter()
                                                .position(|tab| tab.id == active_id)
                                        })
                                        .unwrap_or_else(|| {
                                            editor
                                                .active_tab_index
                                                .min(editor.open_tabs.len().saturating_sub(1))
                                        });
                                    editor.graph_workspace_tabs_dirty = true;
                                    editor.is_dirty = true;
                                    editor.invalidate_palette(cx);
                                    editor.refresh_graph_workspace_tabs(window, cx);
                                    cx.notify();
                                }
                            }))
                    })),
            );
        }

        content
            .children(snapshot.sections.iter().map(render_section))
            .into_any_element()
    }
}

fn render_section(section: &SaveDataSection) -> impl IntoElement {
    v_flex()
        .gap_1()
        .p_2()
        .rounded_md()
        .border_1()
        .border_color(gpui::rgba(0xFFFFFF20))
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .child(section.name),
        )
        .when(!section.summary.is_empty(), |element| {
            element.child(
                div()
                    .text_xs()
                    .text_color(gpui::rgba(0xFFFFFF99))
                    .child(section.summary.clone()),
            )
        })
        .child(
            div()
                .max_h(px(280.0))
                .overflow_y_scroll()
                .p_1()
                .bg(gpui::rgba(0x00000030))
                .text_xs()
                .font_family("JetBrainsMono-Regular")
                .child(section.json.clone()),
        )
}

fn section_summary(name: &str, value: &serde_json::Value) -> String {
    match name {
        "main_graph" => format!(
            "{} nodes · {} connections · {} comments",
            value["nodes"].as_object().map_or(0, serde_json::Map::len),
            value["connections"].as_array().map_or(0, Vec::len),
            value["comments"].as_array().map_or(0, Vec::len),
        ),
        "subgraphs" | "local_events" | "variables" => {
            format!("{} entries", value.as_array().map_or(0, Vec::len))
        }
        _ => String::new(),
    }
}

fn subgraph_is_referenced(editor: &BlueprintEditorPanel, graph_id: &str) -> bool {
    let definition_id = format!("macro:{graph_id}");
    editor
        .open_tabs
        .iter()
        .flat_map(|tab| tab.graph.nodes.iter())
        .any(|node| node.definition_id == definition_id)
        || editor.subgraphs.iter().any(|subgraph| {
            subgraph.id != graph_id
                && subgraph
                    .graph
                    .nodes
                    .values()
                    .any(|node| node.node_type == definition_id)
        })
}

fn stable_key(value: &str) -> u64 {
    value.bytes().fold(14695981039346656037_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(1099511628211)
    })
}
