use gpui::prelude::*;
use gpui::*;
use ui::{
    button::{Button, ButtonVariants as _},
    dropdown::SearchableList,
    h_flex,
    popover::Popover,
    v_flex, ActiveTheme, IconName, Sizable, StyledExt,
};

use crate::editor::panel::BlueprintEditorPanel;

#[derive(Clone, Debug)]
pub struct TraitAssetSummary {
    pub path: String,
    pub name: String,
    pub display_name: String,
    pub description: String,
}

/// Renderer for the Blueprint's implemented-trait sidebar tab.
pub struct ImplementedTraitsRenderer;

impl ImplementedTraitsRenderer {
    pub fn render(
        editor: &mut BlueprintEditorPanel,
        _window: &mut Window,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        if !editor.implemented_trait_catalog_loaded && !editor.implemented_trait_catalog_loading {
            editor.refresh_implemented_trait_catalog(cx);
        }

        let theme = cx.theme();
        let selected = editor.blueprint_metadata.implemented_traits.clone();
        let catalog = editor.implemented_trait_catalog.clone();
        let catalog_error = editor.implemented_trait_catalog_error.clone();
        let status = editor.implemented_trait_status.clone();
        let picker = editor.implemented_trait_picker.clone();
        let add_trait =
            Popover::<SearchableList<TraitAssetSummary>>::new("implemented-trait-picker")
                .anchor(Corner::BottomRight)
                .trigger(
                    Button::new("implemented-trait-add")
                        .label("Add Trait")
                        .icon(IconName::Plus)
                        .small()
                        .dropdown_caret(true),
                )
                .content(move |_window, _cx| picker.clone());
        let refresh = Button::new("refresh-trait-catalog")
            .label("Refresh")
            .ghost()
            .on_click(cx.listener(|editor, _, _, cx| {
                editor.implemented_trait_catalog_loaded = false;
                editor.refresh_implemented_trait_catalog(cx);
                cx.notify();
            }));

        v_flex()
            .size_full()
            .gap_3()
            .p_3()
            .overflow_y_scroll()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_weight(FontWeight::BOLD).child("Implemented Traits"))
                    .child(refresh),
            )
            .when_some(catalog_error, |element, error| {
                element.child(
                    div()
                        .p_2()
                        .rounded_md()
                        .bg(theme.danger.opacity(0.12))
                        .text_sm()
                        .text_color(theme.danger)
                        .child(error),
                )
            })
            .when(editor.implemented_trait_catalog_loading, |element| {
                element.child(div().text_sm().text_color(theme.muted_foreground).child("Loading trait assets…"))
            })
            .child(div().text_xs().text_color(theme.muted_foreground).child(
                "Trait assignments are stored as normalized project-relative paths and saved with this Blueprint.",
            ))
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child("Assigned"))
                    .child(add_trait),
            )
            .when(selected.is_empty(), |element| {
                element.child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("No traits are implemented yet."),
                )
            })
            .children(selected.iter().map(|path| {
                let summary = catalog.iter().find(|asset| &asset.path == path);
                let title = summary
                    .map(|asset| asset.display_name.clone())
                    .unwrap_or_else(|| format!("Missing trait: {}", path));
                let detail = summary
                    .map(|asset| format!("{} · {}", asset.name, asset.path))
                    .unwrap_or_else(|| path.clone());
                let path = path.clone();
                let remove = Button::new(format!("remove-trait-{}", stable_button_key(&path)))
                    .label("Remove")
                    .danger()
                    .on_click(cx.listener(move |editor, _, window, cx| {
                        editor.remove_implemented_trait(&path, window, cx);
                    }));
                v_flex()
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(title))
                            .child(remove),
                    )
                    .child(div().text_xs().text_color(theme.muted_foreground).child(detail))
            }))
            .when_some(status, |element, status| {
                let color = if status.starts_with("Could not") || status.starts_with("Cannot") {
                    theme.danger
                } else {
                    theme.success
                };
                element.child(div().text_xs().text_color(color).child(status))
            })
    }
}

fn stable_button_key(path: &str) -> u64 {
    path.bytes().fold(14695981039346656037_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(1099511628211)
    })
}
