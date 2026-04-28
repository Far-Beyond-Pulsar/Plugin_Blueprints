//! Blueprint Editor Toolbar
//!
//! Top toolbar rendered above all workspace panels. Follows the exact same
//! 48-px, separator-grouped, icon-button language used by the Level Editor
//! toolbar so the two feel visually continuous.
//!
//! # Group layout (left → right)
//!
//! ```
//! [ Save ] | [ 🔨 Compile ] | [ Comment ] | [ 🔍 Find ] | [ Map Bug ⚙ ] ···flex··· status  [ 📦 Name ● ]
//! ```
//!
//! The compile button changes colour and label reactively:
//! - **Idle**      – neutral secondary (needs compile)
//! - **Compiling** – warning / loading spinner
//! - **Success**   – success green
//! - **Error**     – danger red

use gpui::*;
use gpui::prelude::FluentBuilder as _;
use ui::{
    h_flex, Icon, IconName,
    ActiveTheme, Disableable,
    button::{Button, ButtonVariants as _},
};

use crate::core::types::CompilationState;
use crate::editor::panel::BlueprintEditorPanel;

// ─────────────────────────────────────────────────────────────────────────────
// Public renderer
// ─────────────────────────────────────────────────────────────────────────────

pub struct ToolbarRenderer;

impl ToolbarRenderer {
    pub fn render(
        panel: &BlueprintEditorPanel,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        // ── Snapshot mutable state before building the element tree ──────────
        // (avoids multiple re-borrows of `panel` inside closures)
        let compile_state  = panel.compilation_status.state.clone();
        let is_compiling   = panel.compilation_status.is_compiling;
        let is_dirty       = panel.is_dirty;
        let show_minimap   = panel.show_minimap;
        let show_debug     = panel.show_debug_overlay;
        let show_controls  = panel.show_graph_controls;
        let blueprint_name = panel
            .tab_title
            .clone()
            .unwrap_or_else(|| "Blueprint Editor".to_string());

        // ── Compile-button icon (reflects last result) ───────────────────────
        let compile_icon = match &compile_state {
            CompilationState::Success => IconName::BadgeCheck,
            CompilationState::Error   => IconName::X,
            _                         => IconName::Flash,
        };

        // ── Right-side status badge ──────────────────────────────────────────
        let show_status  = compile_state != CompilationState::Idle;
        let status_text  = panel.compilation_status.message.clone();
        let status_color: Hsla = match &compile_state {
            CompilationState::Compiling => cx.theme().warning,
            CompilationState::Success   => cx.theme().success,
            CompilationState::Error     => cx.theme().danger,
            CompilationState::Idle      => cx.theme().muted_foreground,
        };

        // ── Assemble toolbar ─────────────────────────────────────────────────
        h_flex()
            .w_full()
            .h(px(48.0))
            .px_4()
            .gap_3()
            .items_center()
            // Same surface treatment as the level-editor toolbar
            .bg(cx.theme().sidebar.opacity(0.98))
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.8))
            .shadow_sm()

            // ── Group 1 · File ───────────────────────────────────────────────
            .child(
                h_flex()
                    .gap_1p5()
                    .items_center()
                    .child(
                        Button::new("toolbar-save")
                            .icon(IconName::FloppyDisk)
                            // Unsaved-changes dot keeps the user informed without a modal
                            .tooltip(if is_dirty {
                                "Save Blueprint (Ctrl+S)  ●"
                            } else {
                                "Save Blueprint (Ctrl+S)"
                            })
                            .on_click(cx.listener(|panel, _, window, cx| {
                                panel.plugin_save(window, cx);
                            }))
                    )
            )

            .child(toolbar_separator(cx))

            // ── Group 2 · Compile ────────────────────────────────────────────
            .child(
                h_flex()
                    .gap_1p5()
                    .items_center()
                    // Build the base button, then apply colour variant in a
                    // single match so the type stays `Button` throughout.
                    .child({
                        let btn = Button::new("toolbar-compile")
                            .icon(compile_icon)
                            .label(if is_compiling { "Compiling…" } else { "Compile" })
                            .loading(is_compiling)
                            .disabled(is_compiling)
                            .tooltip("Compile Blueprint (F7)")
                            .on_click(cx.listener(|panel, _, _window, cx| {
                                panel.start_compilation(cx);
                            }));

                        match compile_state {
                            CompilationState::Success   => btn.success(),
                            CompilationState::Error     => btn.danger(),
                            CompilationState::Compiling => btn.warning(),
                            CompilationState::Idle      => btn,
                        }
                    })
            )

            .child(toolbar_separator(cx))

            // ── Group 3 · Blueprint Graph Editing ────────────────────────────
            .child(
                h_flex()
                    .gap_1p5()
                    .items_center()
                    // Reload resets the graph to the last saved version
                    .child(
                        Button::new("toolbar-reload")
                            .icon(IconName::Refresh)
                            .tooltip("Reload Blueprint from Disk")
                            .on_click(cx.listener(|panel, _, window, cx| {
                                panel.plugin_reload(window, cx);
                            }))
                    )
                    // Add Comment box at the centre of the current viewport
                    .child(
                        Button::new("toolbar-add-comment")
                            .icon(IconName::Message)
                            .tooltip("Add Comment to Graph")
                            .on_click(cx.listener(|panel, _, window, cx| {
                                panel.create_comment_at_center(window, cx);
                            }))
                    )
            )

            .child(toolbar_separator(cx))

            // ── Group 4 · Find & Navigate ────────────────────────────────────
            .child(
                h_flex()
                    .gap_1p5()
                    .items_center()
                    .child(
                        Button::new("toolbar-find")
                            .icon(IconName::Search)
                            .tooltip("Find in Blueprint (Ctrl+F)")
                            .on_click(cx.listener(|_panel, _, _window, _cx| {
                                // TODO: focus the Find Results panel in the
                                // workspace dock when the workspace API supports
                                // programmatic panel activation.
                            }))
                    )
            )

            .child(toolbar_separator(cx))

            // ── Group 5 · View Toggles ───────────────────────────────────────
            // Matches the level-editor toggle pattern:
            //   inactive → secondary (default), active → primary
            .child(
                h_flex()
                    .gap_1p5()
                    .items_center()
                    .child({
                        let btn = Button::new("toolbar-minimap")
                            .icon(IconName::Map)
                            .tooltip("Toggle Minimap")
                            .on_click(cx.listener(|panel, _, _, cx| {
                                panel.show_minimap = !panel.show_minimap;
                                cx.notify();
                            }));
                        if show_minimap { btn.primary() } else { btn }
                    })
                    .child({
                        let btn = Button::new("toolbar-debug")
                            .icon(IconName::Bug)
                            .tooltip("Toggle Debug Overlay")
                            .on_click(cx.listener(|panel, _, _, cx| {
                                panel.show_debug_overlay = !panel.show_debug_overlay;
                                cx.notify();
                            }));
                        if show_debug { btn.primary() } else { btn }
                    })
                    .child({
                        let btn = Button::new("toolbar-graph-controls")
                            .icon(IconName::Settings)
                            .tooltip("Toggle Graph Controls")
                            .on_click(cx.listener(|panel, _, _, cx| {
                                panel.show_graph_controls = !panel.show_graph_controls;
                                cx.notify();
                            }));
                        if show_controls { btn.primary() } else { btn }
                    })
            )

            // ── Flex spacer pushes right-side content to the edge ────────────
            .child(div().flex_1())

            // ── Right side · Compile status + Blueprint name pill ────────────
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    // Live compile status text (hidden when idle)
                    .when(show_status, |el| {
                        el.child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(status_color)
                                .child(status_text),
                        )
                    })
                    // Blueprint identity pill ─────────────────────────────────
                    // Shows the class name and an unsaved-changes dot
                    .child(
                        h_flex()
                            .gap_1p5()
                            .items_center()
                            .px_3()
                            .h_8()
                            .rounded(cx.theme().radius)
                            .bg(cx.theme().muted.opacity(0.3))
                            .border_1()
                            .border_color(cx.theme().border.opacity(0.5))
                            .child(
                                Icon::new(IconName::Component)
                                    .size(px(14.0))
                                    .text_color(cx.theme().accent),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(cx.theme().foreground)
                                    .child(blueprint_name),
                            )
                            // Amber dot when there are unsaved changes
                            .when(is_dirty, |el| {
                                el.child(
                                    div()
                                        .w(px(6.0))
                                        .h(px(6.0))
                                        .rounded_full()
                                        .bg(cx.theme().warning)
                                        .flex_shrink_0(),
                                )
                            }),
                    ),
            )
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Thin vertical separator – identical to the level-editor toolbar separator.
fn toolbar_separator(cx: &mut Context<BlueprintEditorPanel>) -> impl IntoElement {
    div()
        .h_6()
        .w_px()
        .bg(cx.theme().border.opacity(0.4))
        .flex_shrink_0()
}
