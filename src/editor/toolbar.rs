//! Blueprint editor toolbar.
//!
//! Keep the primary row focused on actions used while editing. Less frequent
//! graph and compile settings live in the same overflow menu at every width.

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use ui::{
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::DropdownMenu as _,
    ActiveTheme, Disableable, Icon, IconName, Sizable as _,
};

use crate::core::types::CompilationState;
use crate::editor::panel::BlueprintEditorPanel;

pub struct ToolbarRenderer;

impl ToolbarRenderer {
    pub fn render(
        panel: &BlueprintEditorPanel,
        window: &mut Window,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
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
        let compact = window.bounds().size.width < px(900.0);

        let debug_is_paused = panel
            .debug_session
            .as_ref()
            .map(|session| session.is_paused)
            .unwrap_or(false);
        let debug_session_active = panel.debug_session.is_some();
        let debug_can_back = panel
            .debug_session
            .as_ref()
            .map(|session| session.can_step_backward())
            .unwrap_or(false);
        let debug_can_fwd = panel
            .debug_session
            .as_ref()
            .map(|session| session.can_step_forward())
            .unwrap_or(false);
        let bp_count = panel.breakpoints.len();

        let compile_icon = match compile_state {
            CompilationState::Success => IconName::BadgeCheck,
            CompilationState::Error => IconName::X,
            _ => IconName::Flash,
        };
        let status_color = match compile_state {
            CompilationState::Compiling => cx.theme().warning,
            CompilationState::Success => cx.theme().success,
            CompilationState::Error => cx.theme().danger,
            CompilationState::Idle => cx.theme().muted_foreground,
        };
        let status_text = panel.compilation_status.message.clone();
        let show_status = compile_state != CompilationState::Idle;

        let active = panel.active_canvas().map(|canvas| canvas.read(cx));
        let history = active.map(|canvas| canvas.history.clone());
        let mut menus = plugin_editor_api::EditorMenuStrip::new("blueprint-editor-menus").menu(
            plugin_editor_api::EditorMenu::new("Edit").undo_redo(
                "Undo",
                "Redo",
                history.map(|history| history as _),
            ),
        );
        if let Some(canvas) = active {
            menus = menus.action_context(canvas.focus_handle.clone());
        }

        let panel_entity = cx.entity().clone();
        let mode_label = compile_mode.label();
        let show_minimap_label = if show_minimap {
            "Hide Minimap"
        } else {
            "Show Minimap"
        };
        let show_debug_label = if show_debug {
            "Hide Debug Overlay"
        } else {
            "Show Debug Overlay"
        };
        let show_controls_label = if show_controls {
            "Hide Graph Controls"
        } else {
            "Show Graph Controls"
        };
        let connection_label = if electronic_connections {
            "Use Curved Connections"
        } else {
            "Use Electronic Connections"
        };

        h_flex()
            .w_full()
            .h(px(44.0))
            .px_2()
            .gap_1()
            .items_center()
            .bg(cx.theme().sidebar.opacity(0.98))
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.8))
            .child(menus)
            .child(toolbar_separator(cx))
            .child(
                Button::new("toolbar-save")
                    .ghost()
                    .small()
                    .icon(IconName::FloppyDisk)
                    .disabled(is_saving)
                    .tooltip(if is_saving {
                        "Saving Blueprint…"
                    } else if is_dirty {
                        "Save Blueprint (Ctrl+S) · Unsaved changes"
                    } else {
                        "Save Blueprint (Ctrl+S)"
                    })
                    .on_click(cx.listener(|panel, _, window, cx| {
                        panel.plugin_save(window, cx);
                    })),
            )
            .child({
                let button = Button::new("toolbar-compile")
                    .small()
                    .icon(compile_icon)
                    .when(!compact, |button| {
                        button.label(if is_compiling {
                            "Compiling…"
                        } else {
                            "Compile"
                        })
                    })
                    .loading(is_compiling)
                    .disabled(is_compiling)
                    .tooltip("Compile Blueprint (F7)")
                    .on_click(cx.listener(|panel, _, _window, cx| {
                        panel.start_compilation(cx);
                    }));
                match compile_state {
                    CompilationState::Success => button.success(),
                    CompilationState::Error => button.danger(),
                    CompilationState::Compiling => button.warning(),
                    CompilationState::Idle => button,
                }
            })
            .child(toolbar_separator(cx))
            .child(
                Button::new("toolbar-add-comment")
                    .ghost()
                    .small()
                    .icon(IconName::Message)
                    .tooltip("Add Comment to Graph")
                    .on_click(cx.listener(|panel, _, window, cx| {
                        if let Some(canvas) = panel.active_canvas().cloned() {
                            canvas.update(cx, |canvas, cx| {
                                canvas.create_comment_at_center(window, cx)
                            });
                        }
                    })),
            )
            .child(
                Button::new("toolbar-find")
                    .ghost()
                    .small()
                    .icon(IconName::Search)
                    .tooltip("Find in Blueprint (Ctrl+F)")
                    .on_click(cx.listener(|panel, _, window, cx| {
                        panel.focus_find_panel(window, cx);
                    })),
            )
            .when(debug_session_active && !compact, |toolbar| {
                toolbar
                    .child(toolbar_separator(cx))
                    .when(bp_count > 0, |toolbar| {
                        toolbar.child(
                            div()
                                .px_2()
                                .h_7()
                                .flex()
                                .items_center()
                                .rounded(cx.theme().radius)
                                .bg(cx.theme().danger.opacity(0.12))
                                .text_xs()
                                .text_color(cx.theme().danger)
                                .child(format!("{} breakpoints", bp_count)),
                        )
                    })
                    .when(debug_is_paused, |toolbar| {
                        toolbar.child(
                            Button::new("toolbar-debug-continue")
                                .ghost()
                                .small()
                                .icon(IconName::Play)
                                .tooltip("Continue Execution (F5)")
                                .on_click(cx.listener(|panel, _, _, cx| {
                                    if let Some(canvas) = panel.active_canvas().cloned() {
                                        canvas.update(cx, |canvas, cx| canvas.debug_continue(cx));
                                    }
                                })),
                        )
                    })
                    .child(
                        Button::new("toolbar-debug-back")
                            .ghost()
                            .small()
                            .icon(IconName::ArrowLeft)
                            .tooltip("Step Back (Shift+F10)")
                            .disabled(!debug_can_back)
                            .on_click(cx.listener(|panel, _, _, cx| {
                                if let Some(canvas) = panel.active_canvas().cloned() {
                                    canvas.update(cx, |canvas, cx| canvas.debug_step_backward(cx));
                                }
                            })),
                    )
                    .child(
                        Button::new("toolbar-debug-forward")
                            .ghost()
                            .small()
                            .icon(IconName::ArrowRight)
                            .tooltip("Step Forward (F10)")
                            .disabled(!debug_can_fwd)
                            .on_click(cx.listener(|panel, _, _, cx| {
                                if let Some(canvas) = panel.active_canvas().cloned() {
                                    canvas.update(cx, |canvas, cx| canvas.debug_step_forward(cx));
                                }
                            })),
                    )
                    .child(
                        Button::new("toolbar-debug-stop")
                            .danger()
                            .small()
                            .icon(IconName::X)
                            .label("Stop")
                            .tooltip("Stop Debug Session (Shift+F5)")
                            .on_click(cx.listener(|panel, _, _, cx| {
                                if let Some(canvas) = panel.active_canvas().cloned() {
                                    canvas.update(cx, |canvas, cx| canvas.debug_stop(cx));
                                }
                            })),
                    )
            })
            .child(
                Button::new("toolbar-more-actions")
                    .ghost()
                    .small()
                    .icon(IconName::Settings)
                    .tooltip("Blueprint settings and graph options")
                    .popup_menu_with_anchor(Corner::BottomRight, move |menu, _, _| {
                        let mut menu = menu
                            .menu_handler_with_icon("Reload Blueprint", IconName::Refresh, {
                                let panel = panel_entity.clone();
                                move |window, cx| {
                                    let _ = panel
                                        .update(cx, |panel, cx| panel.plugin_reload(window, cx));
                                }
                            })
                            .menu_handler_with_icon(
                                format!("Compile Mode: {mode_label}"),
                                IconName::Flash,
                                {
                                    let panel = panel_entity.clone();
                                    move |_, cx| {
                                        let _ = panel.update(cx, |panel, cx| {
                                            panel.compile_mode = panel.compile_mode.toggle();
                                            cx.notify();
                                        });
                                    }
                                },
                            )
                            .separator()
                            .menu_handler_with_icon(show_minimap_label, IconName::Map, {
                                let panel = panel_entity.clone();
                                move |_, cx| {
                                    let _ = panel.update(cx, |panel, cx| {
                                        panel.show_minimap = !panel.show_minimap;
                                        cx.notify();
                                    });
                                }
                            })
                            .menu_handler_with_icon(show_debug_label, IconName::Bug, {
                                let panel = panel_entity.clone();
                                move |_, cx| {
                                    let _ = panel.update(cx, |panel, cx| {
                                        panel.show_debug_overlay = !panel.show_debug_overlay;
                                        cx.notify();
                                    });
                                }
                            })
                            .menu_handler_with_icon(show_controls_label, IconName::Settings, {
                                let panel = panel_entity.clone();
                                move |_, cx| {
                                    let _ = panel.update(cx, |panel, cx| {
                                        panel.show_graph_controls = !panel.show_graph_controls;
                                        cx.notify();
                                    });
                                }
                            })
                            .menu_handler_with_icon(connection_label, IconName::GitBranch, {
                                let panel = panel_entity.clone();
                                move |_, cx| {
                                    let _ = panel.update(cx, |panel, cx| {
                                        if let Some(canvas) = panel.active_canvas().cloned() {
                                            canvas.update(cx, |canvas, cx| {
                                                canvas.electronic_connections =
                                                    !canvas.electronic_connections;
                                                cx.notify();
                                            });
                                        }
                                        cx.notify();
                                    });
                                }
                            });

                        if debug_session_active && compact {
                            menu = menu.separator();
                            if debug_is_paused {
                                menu = menu.menu_handler_with_icon("Continue", IconName::Play, {
                                    let panel = panel_entity.clone();
                                    move |_, cx| {
                                        let _ = panel.update(cx, |panel, cx| {
                                            if let Some(canvas) = panel.active_canvas().cloned() {
                                                canvas.update(cx, |canvas, cx| {
                                                    canvas.debug_continue(cx)
                                                });
                                            }
                                        });
                                    }
                                });
                            }
                            menu = menu
                                .menu_handler_with_icon("Step Back", IconName::ArrowLeft, {
                                    let panel = panel_entity.clone();
                                    move |_, cx| {
                                        let _ = panel.update(cx, |panel, cx| {
                                            if let Some(canvas) = panel.active_canvas().cloned() {
                                                canvas.update(cx, |canvas, cx| {
                                                    canvas.debug_step_backward(cx)
                                                });
                                            }
                                        });
                                    }
                                })
                                .menu_handler_with_icon("Step Forward", IconName::ArrowRight, {
                                    let panel = panel_entity.clone();
                                    move |_, cx| {
                                        let _ = panel.update(cx, |panel, cx| {
                                            if let Some(canvas) = panel.active_canvas().cloned() {
                                                canvas.update(cx, |canvas, cx| {
                                                    canvas.debug_step_forward(cx)
                                                });
                                            }
                                        });
                                    }
                                })
                                .menu_handler_with_icon("Stop Debug Session", IconName::X, {
                                    let panel = panel_entity.clone();
                                    move |_, cx| {
                                        let _ = panel.update(cx, |panel, cx| {
                                            if let Some(canvas) = panel.active_canvas().cloned() {
                                                canvas
                                                    .update(cx, |canvas, cx| canvas.debug_stop(cx));
                                            }
                                        });
                                    }
                                });
                        }
                        menu
                    }),
            )
            .child(div().flex_1())
            .when(show_status, |toolbar| {
                toolbar.child(
                    div()
                        .max_w(px(280.0))
                        .overflow_hidden()
                        .text_xs()
                        .text_color(status_color)
                        .child(status_text),
                )
            })
            .child(toolbar_separator(cx))
            .child(
                h_flex()
                    .max_w(px(220.0))
                    .gap_2()
                    .items_center()
                    .overflow_hidden()
                    .child(Icon::new(IconName::Component).size(px(14.0)))
                    .child(
                        div()
                            .overflow_hidden()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(cx.theme().foreground)
                            .child(blueprint_name),
                    )
                    .when(is_dirty, |identity| {
                        identity.child(
                            div()
                                .w(px(7.0))
                                .h(px(7.0))
                                .rounded_full()
                                .bg(cx.theme().warning)
                                .flex_shrink_0(),
                        )
                    }),
            )
    }
}

fn toolbar_separator(cx: &mut Context<BlueprintEditorPanel>) -> impl IntoElement {
    div()
        .h_6()
        .w_px()
        .mx_1()
        .bg(cx.theme().border.opacity(0.5))
        .flex_shrink_0()
}
