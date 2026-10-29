//! The per-tab canvas view: mounts the shared Graphy canvas and layers the
//! Blueprint-only chrome over it (menus, quick palette, comment title editor,
//! macro pin editor, breakpoint badges, debug HUD).

use gpui::prelude::*;
use gpui::*;

use crate::editor::graph_canvas_panel::GraphCanvasPanel;

impl GraphCanvasPanel {
    // Shown as a top-right overlay when the active tab is a local macro.
    // Lets the user add and remove interface pins (inputs / outputs) and reflects
    // changes immediately in the Macro Entry / Exit nodes and all instances.

    pub(crate) fn render_macro_pin_editor(
        canvas: &GraphCanvasPanel,
        cx: &mut Context<GraphCanvasPanel>,
    ) -> AnyElement {
        use ui::{h_flex, v_flex, ActiveTheme};

        // Only show when we're inside a macro graph tab.
        if canvas.is_main {
            return div().into_any_element();
        }
        let macro_id = canvas.id.clone();

        // subgraphs live on the shared panel
        let subgraphs = canvas
            .panel
            .upgrade()
            .map(|p| p.read(cx).subgraphs.clone())
            .unwrap_or_default();
        let macro_def = match subgraphs.iter().find(|m| m.id == macro_id) {
            Some(m) => m.clone(),
            None => return div().into_any_element(),
        };

        // UI inputs for pin name / type also live on the shared panel
        let (name_input_opt, type_dd_opt) = canvas
            .panel
            .upgrade()
            .map(|p| {
                let r = p.read(cx);
                (
                    r.variable_name_input.clone(),
                    r.variable_type_dropdown.clone(),
                )
            })
            .unzip();
        let (Some(name_input_shared), Some(type_dd_shared)) = (name_input_opt, type_dd_opt) else {
            return div().into_any_element();
        };

        let pe = cx.entity().clone();
        let mid = macro_id.clone();

        // ── Add-pin form (shown when macro_pin_add_mode is Some) ──────────────
        let add_form = if let Some(is_input) = canvas.macro_pin_add_mode {
            let pe_submit = pe.clone();
            let pe_cancel = pe.clone();
            let mid2 = mid.clone();
            let name_input = name_input_shared.clone();
            let type_dd = type_dd_shared.clone();
            let dir_label = if is_input { "Input" } else { "Output" };

            h_flex()
                .gap(px(6.0))
                .items_center()
                .px(px(8.0))
                .py(px(4.0))
                .bg(cx.theme().muted.opacity(0.4))
                .rounded(px(4.0))
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("+ {} pin:", dir_label)),
                )
                .child(
                    div()
                        .w(px(110.0))
                        .h(px(24.0))
                        .child(ui::input::TextInput::new(&name_input)),
                )
                .child(
                    div()
                        .w(px(90.0))
                        .h(px(24.0))
                        .child(ui::dropdown::Dropdown::new(&type_dd)),
                )
                // Confirm
                .child(
                    div()
                        .px(px(8.0))
                        .py(px(3.0))
                        .text_size(px(10.0))
                        .rounded(px(3.0))
                        .bg(gpui::rgba(0x1A4A1AFF))
                        .text_color(gpui::rgba(0x88FF88FF))
                        .cursor_pointer()
                        .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                            pe_submit.update(cx, |canvas, cx| {
                                let name = canvas
                                    .panel
                                    .upgrade()
                                    .map(|p| {
                                        p.read(cx).variable_name_input.read(cx).text().to_string()
                                    })
                                    .unwrap_or_default()
                                    .trim()
                                    .to_string();
                                let type_str = canvas
                                    .panel
                                    .upgrade()
                                    .and_then(|p| {
                                        p.read(cx)
                                            .variable_type_dropdown
                                            .read(cx)
                                            .selected_value()
                                            .map(|v| v.to_string())
                                    })
                                    .unwrap_or_else(|| "f32".to_string());
                                if !name.is_empty() {
                                    if let Some(p) = canvas.panel.upgrade() {
                                        p.update(cx, |panel, cx| {
                                            panel.add_macro_pin(&mid2, name, type_str, is_input, cx)
                                        });
                                    }
                                }
                                canvas.macro_pin_add_mode = None;
                                cx.notify();
                            });
                        })
                        .child("✓ Add"),
                )
                // Cancel
                .child(
                    div()
                        .px(px(8.0))
                        .py(px(3.0))
                        .text_size(px(10.0))
                        .rounded(px(3.0))
                        .bg(cx.theme().muted.opacity(0.3))
                        .text_color(cx.theme().muted_foreground)
                        .cursor_pointer()
                        .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                            pe_cancel.update(cx, |canvas, cx| {
                                canvas.macro_pin_add_mode = None;
                                cx.notify();
                            });
                        })
                        .child("✕"),
                )
                .into_any_element()
        } else {
            div().into_any_element()
        };

        // ── Pin list ──────────────────────────────────────────────────────────
        let inputs_col = {
            let pe_add = pe.clone();
            let mid_add = mid.clone();
            let mut col = v_flex().gap(px(3.0)).min_w(px(140.0)).child(
                h_flex()
                    .gap(px(4.0))
                    .items_center()
                    .child(
                        div()
                            .text_size(px(10.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(cx.theme().success)
                            .child("INPUTS"),
                    )
                    .child(
                        div()
                            .px(px(6.0))
                            .py(px(2.0))
                            .text_size(px(9.0))
                            .rounded(px(3.0))
                            .bg(cx.theme().success.opacity(0.15))
                            .text_color(cx.theme().success)
                            .cursor_pointer()
                            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                                pe_add.update(cx, |canvas, cx| {
                                    canvas.macro_pin_add_mode = Some(true);
                                    if let Some(p) = canvas.panel.upgrade() {
                                        p.update(cx, |panel, cx| {
                                            panel.start_creating_variable(window, cx)
                                        });
                                    };
                                    cx.notify();
                                });
                            })
                            .child("+ Add"),
                    ),
            );
            for pin in &macro_def.interface.inputs {
                let pe2 = pe.clone();
                let mid2 = mid.clone();
                let pin_id = pin.id.clone();
                let pin_name = pin.name.clone();
                let type_label = format!("{:?}", pin.data_type)
                    .split('(')
                    .next()
                    .unwrap_or("?")
                    .to_string();
                col = col.child(
                    h_flex()
                        .gap(px(4.0))
                        .items_center()
                        .px(px(4.0))
                        .py(px(2.0))
                        .rounded(px(3.0))
                        .bg(cx.theme().muted.opacity(0.2))
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(cx.theme().foreground)
                                .child(pin_name),
                        )
                        .child(
                            div()
                                .text_size(px(9.0))
                                .text_color(cx.theme().muted_foreground)
                                .child(format!(":{}", type_label)),
                        )
                        .child(div().flex_1())
                        .child(
                            div()
                                .w(px(14.0))
                                .h(px(14.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(9.0))
                                .text_color(cx.theme().muted_foreground)
                                .cursor_pointer()
                                .hover(|s| s.text_color(gpui::rgba(0xFF6666FF)))
                                .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                                    pe2.update(cx, |canvas, cx| {
                                        if let Some(p) = canvas.panel.upgrade() {
                                            p.update(cx, |panel, cx| {
                                                panel.remove_macro_pin(&mid2, &pin_id, true, cx)
                                            });
                                        }
                                    });
                                })
                                .child("✕"),
                        ),
                );
            }
            col
        };

        let outputs_col = {
            let pe_add = pe.clone();
            let mid_add = mid.clone();
            let mut col = v_flex().gap(px(3.0)).min_w(px(140.0)).child(
                h_flex()
                    .gap(px(4.0))
                    .items_center()
                    .child(
                        div()
                            .text_size(px(10.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(cx.theme().warning)
                            .child("OUTPUTS"),
                    )
                    .child(
                        div()
                            .px(px(6.0))
                            .py(px(2.0))
                            .text_size(px(9.0))
                            .rounded(px(3.0))
                            .bg(cx.theme().warning.opacity(0.15))
                            .text_color(cx.theme().warning)
                            .cursor_pointer()
                            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                                pe_add.update(cx, |canvas, cx| {
                                    canvas.macro_pin_add_mode = Some(false);
                                    if let Some(p) = canvas.panel.upgrade() {
                                        p.update(cx, |panel, cx| {
                                            panel.start_creating_variable(window, cx)
                                        });
                                    };
                                    cx.notify();
                                });
                            })
                            .child("+ Add"),
                    ),
            );
            for pin in &macro_def.interface.outputs {
                let pe2 = pe.clone();
                let mid2 = mid.clone();
                let pin_id = pin.id.clone();
                let pin_name = pin.name.clone();
                let type_label = format!("{:?}", pin.data_type)
                    .split('(')
                    .next()
                    .unwrap_or("?")
                    .to_string();
                col = col.child(
                    h_flex()
                        .gap(px(4.0))
                        .items_center()
                        .px(px(4.0))
                        .py(px(2.0))
                        .rounded(px(3.0))
                        .bg(cx.theme().muted.opacity(0.2))
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(cx.theme().foreground)
                                .child(pin_name),
                        )
                        .child(
                            div()
                                .text_size(px(9.0))
                                .text_color(cx.theme().muted_foreground)
                                .child(format!(":{}", type_label)),
                        )
                        .child(div().flex_1())
                        .child(
                            div()
                                .w(px(14.0))
                                .h(px(14.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(9.0))
                                .text_color(cx.theme().muted_foreground)
                                .cursor_pointer()
                                .hover(|s| s.text_color(gpui::rgba(0xFF6666FF)))
                                .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                                    pe2.update(cx, |canvas, cx| {
                                        if let Some(p) = canvas.panel.upgrade() {
                                            p.update(cx, |panel, cx| {
                                                panel.remove_macro_pin(&mid2, &pin_id, false, cx)
                                            });
                                        }
                                    });
                                })
                                .child("✕"),
                        ),
                );
            }
            col
        };

        // ── Outer panel (top-right of canvas) ─────────────────────────────────
        div()
            .absolute()
            .top(px(8.0))
            .right(px(8.0))
            .child(
                v_flex()
                    .occlude()
                    .gap(px(6.0))
                    .p(px(10.0))
                    .bg(cx.theme().popover.opacity(0.95))
                    .border_1()
                    .border_color(gpui::rgba(0x9B59B644))
                    .shadow_lg()
                    .rounded(px(8.0))
                    .child(
                        h_flex()
                            .gap(px(6.0))
                            .items_center()
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(gpui::rgba(0xC084FCFF))
                                    .child("📦 Macro Interface"),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("— {}", macro_def.name)),
                            ),
                    )
                    .child(add_form)
                    .child(
                        h_flex()
                            .gap(px(16.0))
                            .items_start()
                            .child(inputs_col)
                            .child(
                                div()
                                    .w(px(1.0))
                                    .self_stretch()
                                    .bg(cx.theme().border.opacity(0.4)),
                            )
                            .child(outputs_col),
                    ),
            )
            .into_any_element()
    }
    //
    // Shown at the bottom-centre of the graph canvas when a debug session is
    // paused.  Displays the current frame's pin values and navigation controls.

    pub(crate) fn render_debug_hud(
        canvas: &GraphCanvasPanel,
        cx: &mut Context<GraphCanvasPanel>,
    ) -> AnyElement {
        use ui::{h_flex, v_flex, ActiveTheme};

        let Some(ref session) = canvas.debug_session else {
            return div().into_any_element();
        };
        let Some(ref frame) = session.current() else {
            return div().into_any_element();
        };

        let frame_title = frame.node_title.clone();
        let frame_step = frame.step_index;
        let pin_values = frame.pin_values.clone();
        let current_idx = session.current_frame;
        let total_frames = session.frames.len();
        let can_back = session.can_step_backward();
        let can_fwd = session.can_step_forward();
        let is_paused = session.is_paused;

        let pe = cx.entity().clone();
        let pe_continue = pe.clone();
        let pe_fwd = pe.clone();
        let pe_back = pe.clone();
        let pe_stop = pe.clone();

        let hud = div()
            .absolute()
            .bottom(px(16.0))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(
                v_flex()
                    .occlude()
                    .w(px(520.0))
                    .bg(cx.theme().popover.opacity(0.96))
                    .border_1()
                    .border_color(gpui::rgba(0xFF440044))
                    .shadow_xl()
                    .rounded(px(8.0))
                    .overflow_hidden()
                    // ── Header bar ──────────────────────────────────────────────
                    .child(
                        h_flex()
                            .px(px(12.0))
                            .py(px(6.0))
                            .bg(gpui::rgba(0x330000CC))
                            .justify_between()
                            .items_center()
                            .child(
                                h_flex()
                                    .gap(px(6.0))
                                    .items_center()
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(gpui::rgba(0xFF6666FF))
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .child("⏹ BREAKPOINT"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(cx.theme().popover_foreground)
                                            .child(frame_title.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!(
                                        "Frame {}/{} · Step {}",
                                        current_idx + 1,
                                        total_frames,
                                        frame_step
                                    )),
                            ),
                    )
                    // ── Pin values ───────────────────────────────────────────────
                    .when(!pin_values.is_empty(), |el| {
                        el.child(v_flex().px(px(12.0)).py(px(8.0)).gap(px(3.0)).children(
                            pin_values.iter().take(8).map(|pv| {
                                let pv = pv.clone();
                                h_flex()
                                    .gap(px(8.0))
                                    .items_center()
                                    .child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(cx.theme().muted_foreground)
                                            .w(px(80.0))
                                            .child(pv.pin_name.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(cx.theme().muted_foreground.opacity(0.7))
                                            .w(px(60.0))
                                            .child(format!("({})", pv.type_label)),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(gpui::rgba(0x88FF88FF))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .child(pv.value.clone()),
                                    )
                            }),
                        ))
                    })
                    .when(pin_values.is_empty(), |el| {
                        el.child(
                            div()
                                .px(px(12.0))
                                .py(px(8.0))
                                .text_size(px(11.0))
                                .text_color(cx.theme().muted_foreground)
                                .child("No data pins on this node"),
                        )
                    })
                    // ── Control bar ──────────────────────────────────────────────
                    .child(
                        h_flex()
                            .px(px(12.0))
                            .py(px(8.0))
                            .gap(px(8.0))
                            .items_center()
                            .border_t_1()
                            .border_color(cx.theme().border.opacity(0.4))
                            // ◀ History (navigate recorded frames backward)
                            .child(
                                div()
                                    .px(px(10.0))
                                    .py(px(4.0))
                                    .text_size(px(11.0))
                                    .rounded(px(4.0))
                                    .bg(if can_back {
                                        cx.theme().secondary
                                    } else {
                                        cx.theme().muted.opacity(0.3)
                                    })
                                    .text_color(if can_back {
                                        cx.theme().secondary_foreground
                                    } else {
                                        cx.theme().muted_foreground
                                    })
                                    .cursor_pointer()
                                    .when(can_back, |el| {
                                        el.on_mouse_down(gpui::MouseButton::Left, {
                                            let pe = pe_back.clone();
                                            move |_, _, cx| {
                                                pe.update(cx, |canvas, cx| {
                                                    canvas.debug_step_backward(cx);
                                                });
                                            }
                                        })
                                    })
                                    .child("◀ History"),
                            )
                            // ▶ Step — advances the executor by one graph node.
                            // Only active when execution is suspended (is_paused).
                            .child(
                                div()
                                    .px(px(10.0))
                                    .py(px(4.0))
                                    .text_size(px(11.0))
                                    .rounded(px(4.0))
                                    .bg(if can_fwd {
                                        gpui::rgba(0x1A3A4AFF) // blue tint to distinguish from Continue
                                    } else {
                                        gpui::rgba(0x22222233)
                                    })
                                    .text_color(if can_fwd {
                                        gpui::rgba(0x88CCFFFF)
                                    } else {
                                        gpui::rgba(0x66666688)
                                    })
                                    .cursor_pointer()
                                    .when(can_fwd, |el| {
                                        el.on_mouse_down(gpui::MouseButton::Left, {
                                            let pe = pe_fwd.clone();
                                            move |_, _, cx| {
                                                pe.update(cx, |canvas, cx| {
                                                    canvas.debug_step_forward(cx);
                                                });
                                            }
                                        })
                                    })
                                    .child("Step →"),
                            )
                            // ▶▶ Continue (only if paused)
                            .when(is_paused, |el| {
                                el.child(
                                    div()
                                        .px(px(10.0))
                                        .py(px(4.0))
                                        .text_size(px(11.0))
                                        .rounded(px(4.0))
                                        .bg(gpui::rgba(0x1A4A1AFF))
                                        .text_color(gpui::rgba(0x88FF88FF))
                                        .cursor_pointer()
                                        .on_mouse_down(gpui::MouseButton::Left, {
                                            let pe = pe_continue.clone();
                                            move |_, _, cx| {
                                                pe.update(cx, |canvas, cx| {
                                                    canvas.debug_continue(cx);
                                                });
                                            }
                                        })
                                        .child("▶ Continue"),
                                )
                            })
                            // flex spacer
                            .child(div().flex_1())
                            // ■ Stop
                            .child(
                                div()
                                    .px(px(10.0))
                                    .py(px(4.0))
                                    .text_size(px(11.0))
                                    .rounded(px(4.0))
                                    .bg(gpui::rgba(0x4A1A1AFF))
                                    .text_color(gpui::rgba(0xFF8888FF))
                                    .cursor_pointer()
                                    .on_mouse_down(gpui::MouseButton::Left, {
                                        move |_, _, cx| {
                                            pe_stop.update(cx, |canvas, cx| {
                                                canvas.debug_stop(cx);
                                            });
                                        }
                                    })
                                    .child("■ Stop"),
                            ),
                    ),
            );

        hud.into_any_element()
    }
}
