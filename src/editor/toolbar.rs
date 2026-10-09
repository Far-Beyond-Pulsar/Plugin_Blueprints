//! Blueprint Editor Toolbar
//!
//! Top toolbar rendered above all workspace panels. Follows the exact same
//! 48-px, separator-grouped, icon-button language used by the Level Editor
//! toolbar so the two feel visually continuous.
//!
//! # Group layout (left → right)
//!
//! ```text
//! [ Save ] | [ 🔨 Compile ] | [ Comment ] | [ 🔍 Find ] | [ Map Bug ⚙ ] ···flex··· status  [ 📦 Name ● ]
//! ```
//!
//! The compile button changes colour and label reactively:
//! - **Idle**      – neutral secondary (needs compile)
//! - **Compiling** – warning / loading spinner
//! - **Success**   – success green
//! - **Error**     – danger red

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use ui::{
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::DropdownMenu as _,
    ActiveTheme, Disableable, Icon, IconName,
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
        window: &mut Window,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        // ── Snapshot mutable state before building the element tree ──────────
        // (avoids multiple re-borrows of `panel` inside closures)
        let compile_state = panel.compilation_status.state.clone();
        let is_compiling = panel.compilation_status.is_compiling;
        let is_dirty = panel.is_dirty;
        let is_saving = panel.is_saving;
        let show_minimap = panel.show_minimap;
        let show_debug = panel.show_debug_overlay;
        let show_controls = panel.show_graph_controls;
        let electronic_connections = panel
            .active_canvas()
            .map(|canvas| canvas.read(cx).electronic_connections)
            .unwrap_or(false);
        let blueprint_name = panel
            .tab_title
            .clone()
            .unwrap_or_else(|| "Blueprint Editor".to_string());
        let compile_mode = panel.compile_mode.clone();
        let toolbar_width = window.bounds().size.width;
        let compact = toolbar_width < px(960.0);
        let very_compact = toolbar_width < px(620.0);

        // ── Debug session state ──────────────────────────────────────────────
        let debug_is_paused = panel
            .debug_session
            .as_ref()
            .map(|s| s.is_paused)
            .unwrap_or(false);
        let debug_session_active = panel.debug_session.is_some();
        let debug_can_back = panel
            .debug_session
            .as_ref()
            .map(|s| s.can_step_backward())
            .unwrap_or(false);
        let debug_can_fwd = panel
            .debug_session
            .as_ref()
            .map(|s| s.can_step_forward())
            .unwrap_or(false);
        let bp_count = panel.breakpoints.len();

        // ── Compile-button icon (reflects last result) ───────────────────────
        let compile_icon = match &compile_state {
            CompilationState::Success => IconName::BadgeCheck,
            CompilationState::Error => IconName::X,
            _ => IconName::Flash,
        };

        // ── Right-side status badge ──────────────────────────────────────────
        let show_status = compile_state != CompilationState::Idle;
        let status_text = panel.compilation_status.message.clone();
        let status_color: Hsla = match &compile_state {
            CompilationState::Compiling => cx.theme().warning,
            CompilationState::Success => cx.theme().success,
            CompilationState::Error => cx.theme().danger,
            CompilationState::Idle => cx.theme().muted_foreground,
        };

        // ── Assemble toolbar ─────────────────────────────────────────────────
        h_flex()
            .w_full()
            .h(px(48.0))
            .px_2()
            .gap_2()
            .items_center()
            // Same surface treatment as the level-editor toolbar
            .bg(cx.theme().sidebar.opacity(0.98))
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.8))
            .shadow_sm()
            // ── Group 1 · File ───────────────────────────────────────────────
            .child(
                h_flex().gap_1p5().items_center().child(
                    Button::new("toolbar-save")
                        .icon(IconName::FloppyDisk)
                        .disabled(is_saving)
                        // Unsaved-changes dot keeps the user informed without a modal
                        .tooltip(if is_saving {
                            "Saving Blueprint…"
                        } else if is_dirty {
                            "Save Blueprint (Ctrl+S)  ●"
                        } else {
                            "Save Blueprint (Ctrl+S)"
                        })
                        .on_click(cx.listener(|panel, _, window, cx| {
                            panel.plugin_save(window, cx);
                        })),
                ),
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
                        let btn = Button::new("toolbar-compile").icon(compile_icon);
                        let btn = if very_compact {
                            btn
                        } else {
                            btn.label(if is_compiling {
                                "Compiling…"
                            } else {
                                "Compile"
                            })
                        };
                        let btn = btn
                            .loading(is_compiling)
                            .disabled(is_compiling)
                            .tooltip("Compile Blueprint (F7)")
                            .on_click(cx.listener(|panel, _, _window, cx| {
                                panel.start_compilation(cx);
                            }));

                        match compile_state {
                            CompilationState::Success => btn.success(),
                            CompilationState::Error => btn.danger(),
                            CompilationState::Compiling => btn.warning(),
                            CompilationState::Idle => btn,
                        }
                    })
                    // Mode toggle: "Rust" ↔ "VM"
                    .when(!compact, |group| group.child({
                        use crate::core::types::CompileMode;
                        let mode_label = compile_mode.label();
                        let tooltip = match &compile_mode {
                            CompileMode::DirectRust => "Mode: Direct Rust codegen — click to switch to Bytecode VM",
                            CompileMode::BytecodeVm => "Mode: Bytecode VM (pulsar_std cdylib) — click to switch to Direct Rust",
                        };
                        let btn = Button::new("toolbar-compile-mode")
                            .label(mode_label)
                            .tooltip(tooltip)
                            .on_click(cx.listener(|panel, _, _window, cx| {
                                panel.compile_mode = panel.compile_mode.toggle();
                                cx.notify();
                            }));
                        match &compile_mode {
                            CompileMode::BytecodeVm => btn.primary(),
                            CompileMode::DirectRust => btn,
                        }
                    })),
            )
            .child(toolbar_separator(cx))
            // ── Group 3 · Blueprint Graph Editing ────────────────────────────
            .child(
                h_flex()
                    .gap_1p5()
                    .items_center()
                    // Reload resets the graph to the last saved version
                    .when(!compact, |group| group.child(
                        Button::new("toolbar-reload")
                            .icon(IconName::Refresh)
                            .tooltip("Reload Blueprint from Disk")
                            .on_click(cx.listener(|panel, _, window, cx| {
                                panel.plugin_reload(window, cx);
                            })),
                    ))
                    // Add Comment box at the centre of the current viewport
                    .when(!very_compact, |group| group.child(
                        Button::new("toolbar-add-comment")
                            .icon(IconName::Message)
                            .tooltip("Add Comment to Graph")
                            .on_click(cx.listener(|panel, _, window, cx| {
                                if let Some(c) = panel.active_canvas().cloned() { c.update(cx, |canvas, cx| canvas.create_comment_at_center(window, cx)); }
                            })),
                    )),
            )
            .child(toolbar_separator(cx))
            // ── Group 4 · Find & Navigate ────────────────────────────────────
            .child(
                h_flex().gap_1p5().items_center().child(
                    Button::new("toolbar-find")
                        .icon(IconName::Search)
                        .tooltip("Find in Blueprint (Ctrl+F)")
                        .on_click(cx.listener(|panel, _, window, cx| {
                            panel.focus_find_panel(window, cx);
                        })),
                ),
            )
            .when(!compact, |toolbar| {
                toolbar.child(toolbar_separator(cx)).child(
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
                        .child({
                            let btn = Button::new("toolbar-electronic-connections")
                                .icon(IconName::GitBranch)
                                .tooltip("Toggle Electronic-Style Connections")
                                .on_click(cx.listener(|panel, _, _, cx| {
                                    if let Some(canvas) = panel.active_canvas().cloned() {
                                        canvas.update(cx, |canvas, cx| {
                                            canvas.electronic_connections = !canvas.electronic_connections;
                                            cx.notify();
                                        });
                                        cx.notify();
                                    }
                                }));
                            if electronic_connections { btn.primary() } else { btn }
                        }),
                )
            })
            .when(compact, |toolbar| {
                let panel = cx.entity().clone();
                let mode_label = compile_mode.label();
                toolbar.child(
                    Button::new("toolbar-more-actions")
                        .icon(IconName::Ellipsis)
                        .tooltip("More Blueprint actions")
                        .popup_menu_with_anchor(Corner::TopRight, move |menu, _, _| {
                            let mut menu = menu.menu_handler_with_icon("Reload Blueprint", IconName::Refresh, {
                                let panel = panel.clone();
                                move |window, cx| { let _ = panel.update(cx, |panel, cx| panel.plugin_reload(window, cx)); }
                            })
                            .menu_handler_with_icon(format!("Compile Mode: {mode_label}"), IconName::Flash, {
                                let panel = panel.clone();
                                move |_, cx| { let _ = panel.update(cx, |panel, cx| {
                                    panel.compile_mode = panel.compile_mode.toggle();
                                    cx.notify();
                                }); }
                            })
                            .separator()
                            .menu_handler_with_icon("Add Comment", IconName::Message, {
                                let panel = panel.clone();
                                move |window, cx| { let _ = panel.update(cx, |panel, cx| {
                                    if let Some(canvas) = panel.active_canvas().cloned() {
                                        canvas.update(cx, |canvas, cx| canvas.create_comment_at_center(window, cx));
                                    }
                                }); }
                            })
                            .menu_handler_with_icon("Toggle Minimap", IconName::Map, {
                                let panel = panel.clone();
                                move |_, cx| { let _ = panel.update(cx, |panel, cx| {
                                    panel.show_minimap = !panel.show_minimap;
                                    cx.notify();
                                }); }
                            })
                            .menu_handler_with_icon("Toggle Debug Overlay", IconName::Bug, {
                                let panel = panel.clone();
                                move |_, cx| { let _ = panel.update(cx, |panel, cx| {
                                    panel.show_debug_overlay = !panel.show_debug_overlay;
                                    cx.notify();
                                }); }
                            })
                            .menu_handler_with_icon("Toggle Graph Controls", IconName::Settings, {
                                let panel = panel.clone();
                                move |_, cx| { let _ = panel.update(cx, |panel, cx| {
                                    panel.show_graph_controls = !panel.show_graph_controls;
                                    cx.notify();
                                }); }
                            })
                            .menu_handler_with_icon("Toggle Electronic Connections", IconName::GitBranch, {
                                let panel = panel.clone();
                                move |_, cx| { let _ = panel.update(cx, |panel, cx| {
                                    if let Some(canvas) = panel.active_canvas().cloned() {
                                        canvas.update(cx, |canvas, cx| {
                                            canvas.electronic_connections = !canvas.electronic_connections;
                                            cx.notify();
                                        });
                                    }
                                    cx.notify();
                                }); }
                            });
                            if debug_session_active {
                                menu = menu.separator()
                                    .menu_handler_with_icon("Continue", IconName::Play, {
                                        let panel = panel.clone();
                                        move |_, cx| { let _ = panel.update(cx, |panel, cx| {
                                            if let Some(canvas) = panel.active_canvas().cloned() {
                                                canvas.update(cx, |canvas, cx| canvas.debug_continue(cx));
                                            }
                                        }); }
                                    })
                                    .menu_handler_with_icon("Step Back", IconName::ArrowLeft, {
                                        let panel = panel.clone();
                                        move |_, cx| { let _ = panel.update(cx, |panel, cx| {
                                            if let Some(canvas) = panel.active_canvas().cloned() {
                                                canvas.update(cx, |canvas, cx| canvas.debug_step_backward(cx));
                                            }
                                        }); }
                                    })
                                    .menu_handler_with_icon("Step Forward", IconName::ArrowRight, {
                                        let panel = panel.clone();
                                        move |_, cx| { let _ = panel.update(cx, |panel, cx| {
                                            if let Some(canvas) = panel.active_canvas().cloned() {
                                                canvas.update(cx, |canvas, cx| canvas.debug_step_forward(cx));
                                            }
                                        }); }
                                    })
                                    .menu_handler_with_icon("Stop Debug Session", IconName::X, {
                                        let panel = panel.clone();
                                        move |_, cx| { let _ = panel.update(cx, |panel, cx| {
                                            if let Some(canvas) = panel.active_canvas().cloned() {
                                                canvas.update(cx, |canvas, cx| canvas.debug_stop(cx));
                                            }
                                        }); }
                                    });
                            }
                            if bp_count > 0 && !debug_session_active {
                                menu = menu.menu_handler_with_icon_and_disabled(
                                    format!("Breakpoints: {bp_count}"),
                                    IconName::Bug,
                                    true,
                                    |_, _| {},
                                );
                            }
                            menu
                        }),
                )
            })
            // ── Group 6 · Debugger controls ──────────────────────────────────
            .when(!compact && (debug_session_active || bp_count > 0), |el| {
                el.child(toolbar_separator(cx)).child(
                    h_flex()
                        .gap_1p5()
                        .items_center()
                        // Breakpoint counter badge
                        .when(bp_count > 0, |el| {
                            el.child(
                                div()
                                    .px_2()
                                    .h_7()
                                    .flex()
                                    .items_center()
                                    .rounded(cx.theme().radius)
                                    .bg(cx.theme().danger.opacity(0.15))
                                    .border_1()
                                    .border_color(cx.theme().danger.opacity(0.45))
                                    .text_size(gpui::px(11.0))
                                    .text_color(cx.theme().danger)
                                    .child(format!("⏹ {}", bp_count)),
                            )
                        })
                        // Continue button (only when paused)
                        .when(debug_is_paused, |el| {
                            el.child({
                                Button::new("toolbar-debug-continue")
                                    .icon(IconName::Play)
                                    .label("Continue")
                                    .tooltip("Continue Execution (F5)")
                                    .success()
                                    .on_click(cx.listener(|panel, _, _, cx| {
                                        if let Some(c) = panel.active_canvas().cloned() { c.update(cx, |canvas, cx| canvas.debug_continue(cx)); }
                                    }))
                            })
                        })
                        // Step Back
                        .when(debug_session_active, |el| {
                            el.child({
                                let btn = Button::new("toolbar-debug-back")
                                    .icon(IconName::ArrowLeft)
                                    .tooltip("Step Back (Shift+F10)")
                                    .on_click(cx.listener(|panel, _, _, cx| {
                                        if let Some(c) = panel.active_canvas().cloned() { c.update(cx, |canvas, cx| canvas.debug_step_backward(cx)); }
                                    }))
                                    .disabled(!debug_can_back);
                                btn
                            })
                        })
                        // Step Forward / Single-step
                        .when(debug_session_active, |el| {
                            el.child({
                                let btn = Button::new("toolbar-debug-fwd")
                                    .icon(IconName::ArrowRight)
                                    .tooltip(if debug_is_paused {
                                        "Step (F10) — execute one node then pause again"
                                    } else {
                                        "Step Forward (F10) — go to next recorded frame"
                                    })
                                    .on_click(cx.listener(|panel, _, _, cx| {
                                        if let Some(c) = panel.active_canvas().cloned() { c.update(cx, |canvas, cx| canvas.debug_step_forward(cx)); }
                                    }))
                                    .disabled(!debug_can_fwd);
                                btn
                            })
                        })
                        // Stop session
                        .when(debug_session_active, |el| {
                            el.child(
                                Button::new("toolbar-debug-stop")
                                    .icon(IconName::X)
                                    .label("Stop")
                                    .tooltip("Stop Debug Session (Shift+F5)")
                                    .danger()
                                    .on_click(cx.listener(|panel, _, _, cx| {
                                        if let Some(c) = panel.active_canvas().cloned() { c.update(cx, |canvas, cx| canvas.debug_stop(cx)); }
                                    })),
                            )
                        }),
                )
            })
            // ── Flex spacer pushes right-side content to the edge ────────────
            .child(div().flex_1())
            // ── Right side · Compile status + Blueprint name pill ────────────
            .child(
                h_flex()
                    .gap_2()
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
                                    .size(px(14.0)),
                            )
                            .when(!very_compact, |pill| pill.child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(cx.theme().foreground)
                                    .child(blueprint_name),
                            ))
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
