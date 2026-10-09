use super::*;

impl PropertiesRenderer {
    pub(super) fn render_macro_details(
        panel: &mut BlueprintEditorPanel,
        index: usize,
        window: &mut Window,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
        let Some(macro_def) = panel
            .subgraphs
            .get(index)
            .filter(|subgraph| subgraph.kind == blueprint_graph::SubGraphKind::Macro)
            .cloned()
        else {
            return Self::render_empty_state(cx);
        };
        let macro_id = macro_def.id.clone();

        v_flex()
            .gap_3()
            .child(Self::render_card(
                std::iter::once::<AnyElement>(
                    h_flex()
                        .w_full()
                        .p_3()
                        .gap_3()
                        .items_center()
                        .child(ui::Icon::new(IconName::GitBranch).size(px(18.0)))
                        .child(
                            div()
                                .text_lg()
                                .font_bold()
                                .text_color(cx.theme().foreground)
                                .child(macro_def.name.clone()),
                        )
                        .into_any_element(),
                )
                .chain(if !macro_def.description.is_empty() {
                    Some(
                        div()
                            .w_full()
                            .px_3()
                            .pb_3()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(macro_def.description.clone()),
                            )
                            .into_any_element(),
                    )
                } else {
                    None
                }),
                cx,
            ))
            .child(Self::render_macro_pin_card(
                panel, &macro_id, true, window, cx,
            ))
            .child(Self::render_macro_pin_card(
                panel, &macro_id, false, window, cx,
            ))
            .child(Self::render_card(
                [
                    Self::render_section_header("Macro Info", IconName::Info, cx)
                        .px_3()
                        .pt_3()
                        .into_any_element(),
                    v_flex()
                        .w_full()
                        .p_3()
                        .gap_2p5()
                        .child(Self::render_info_row("ID", &macro_def.id, cx))
                        .child(Self::render_info_row(
                            "Nodes",
                            &macro_def.graph.nodes.len().to_string(),
                            cx,
                        ))
                        .into_any_element(),
                ],
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_macro_pin_card(
        panel: &mut BlueprintEditorPanel,
        macro_id: &str,
        is_input: bool,
        window: &mut Window,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
        let Some(macro_def) = panel
            .subgraphs
            .iter()
            .find(|m| m.id == macro_id && m.kind == blueprint_graph::SubGraphKind::Macro)
            .cloned()
        else {
            return div().into_any_element();
        };
        let pins = if is_input {
            macro_def.interface.inputs.clone()
        } else {
            macro_def.interface.outputs.clone()
        };
        let (title, add_label) = if is_input {
            ("Inputs", "Add Input")
        } else {
            ("Outputs", "Add Output")
        };

        let mid = macro_id.to_string();
        let on_add = cx.listener(
            move |this: &mut BlueprintEditorPanel, _: &gpui::ClickEvent, _window, cx| {
                this.add_macro_pin(&mid, "new_pin".to_string(), "?".to_string(), is_input, cx);
                cx.notify();
            },
        );

        Self::render_card(
            [
                Self::render_section_header(title, IconName::ArrowRight, cx)
                    .px_3()
                    .pt_3()
                    .into_any_element(),
                v_flex()
                    .w_full()
                    .p_3()
                    .gap_1p5()
                    .children(pins.iter().enumerate().map(|(pi, pin)| {
                        Self::render_editable_pin_row(
                            panel, macro_id, pi, pin, is_input, window, cx,
                        )
                    }))
                    .when(pins.is_empty(), |el| {
                        el.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(if is_input { "No inputs" } else { "No outputs" }),
                        )
                    })
                    .child(
                        h_flex().w_full().pt_1().child(
                            ui::button::Button::new(format!(
                                "add-macro-pin-{}-{}",
                                macro_id, is_input
                            ))
                            .label(add_label)
                            .icon(IconName::Plus)
                            .ghost()
                            .xsmall()
                            .on_click(on_add),
                        ),
                    )
                    .into_any_element(),
            ],
            cx,
        )
        .into_any_element()
    }

    pub(super) fn render_editable_pin_row(
        panel: &mut BlueprintEditorPanel,
        macro_id: &str,
        pin_index: usize,
        pin: &blueprint_graph::SubGraphPin,
        is_input: bool,
        window: &mut Window,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
        use ui::input::TextInput;
        let name_key = (macro_id.to_string(), pin_index, is_input);
        let type_key = (macro_id.to_string(), pin_index, is_input);

        if !panel.macro_pin_name_inputs.contains_key(&name_key) {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder("Name..."));
            input.update(cx, |state, cx| {
                state.set_value(&pin.name, window, cx);
            });
            let sub_mid = macro_id.to_string();
            let sub_pid = pin.id.clone();
            let sub_input = is_input;
            cx.subscribe_in(
                &input,
                window,
                move |this: &mut BlueprintEditorPanel, state, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                        let new_name = state.read(cx).text().to_string().trim().to_string();
                        if !new_name.is_empty() {
                            if let Some(m) = this.subgraphs.iter_mut().find(|m| {
                                m.id == sub_mid && m.kind == blueprint_graph::SubGraphKind::Macro
                            }) {
                                let pins = if sub_input {
                                    &mut m.interface.inputs
                                } else {
                                    &mut m.interface.outputs
                                };
                                if let Some(p) = pins.iter_mut().find(|p| p.id == sub_pid) {
                                    p.name = new_name;
                                }
                            }
                            this.sync_entry_exit_in_active_graph(&sub_mid, cx);
                            this.sync_all_macro_instances(&sub_mid, cx);
                            this.invalidate_palette(cx);
                            cx.notify();
                        }
                    }
                },
            )
            .detach();
            panel.macro_pin_name_inputs.insert(name_key.clone(), input);
        }

        if !panel.macro_pin_type_inputs.contains_key(&type_key) {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder("Type..."));
            input.update(cx, |state, cx| {
                state.set_value(&pin.data_type.to_string(), window, cx);
            });
            let sub_mid = macro_id.to_string();
            let sub_pid = pin.id.clone();
            let sub_input = is_input;
            cx.subscribe_in(
                &input,
                window,
                move |this: &mut BlueprintEditorPanel, state, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                        let new_type = state.read(cx).text().to_string().trim().to_string();
                        if !new_type.is_empty() {
                            if let Some(m) = this.subgraphs.iter_mut().find(|m| {
                                m.id == sub_mid && m.kind == blueprint_graph::SubGraphKind::Macro
                            }) {
                                let pins = if sub_input {
                                    &mut m.interface.inputs
                                } else {
                                    &mut m.interface.outputs
                                };
                                if let Some(p) = pins.iter_mut().find(|p| p.id == sub_pid) {
                                    p.data_type =
                                        blueprint_graph::DataType::from_type_str(&new_type);
                                }
                            }
                            this.sync_entry_exit_in_active_graph(&sub_mid, cx);
                            this.sync_all_macro_instances(&sub_mid, cx);
                            this.invalidate_palette(cx);
                            cx.notify();
                        }
                    }
                },
            )
            .detach();
            panel.macro_pin_type_inputs.insert(type_key.clone(), input);
        }

        let name_input = panel.macro_pin_name_inputs.get(&name_key).cloned();
        let type_input = panel.macro_pin_type_inputs.get(&type_key).cloned();
        let pin_id = pin.id.clone();
        let mid = macro_id.to_string();
        let pin_is_input = is_input;

        let on_remove = cx.listener(
            move |this: &mut BlueprintEditorPanel, _: &gpui::ClickEvent, _window, cx| {
                this.remove_macro_pin(&mid, &pin_id, pin_is_input, cx);
                this.macro_pin_name_inputs
                    .remove(&(mid.clone(), pin_index, pin_is_input));
                this.macro_pin_type_inputs
                    .remove(&(mid.clone(), pin_index, pin_is_input));
                cx.notify();
            },
        );

        h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .child(
                div().flex_1().child(
                    name_input
                        .map(|input| TextInput::new(&input).text_xs().into_any_element())
                        .unwrap_or_else(|| div().into_any_element()),
                ),
            )
            .child(
                div().w(px(80.0)).child(
                    type_input
                        .map(|input| {
                            TextInput::new(&input)
                                .text_xs()
                                .font_family("JetBrainsMono-Regular")
                                .into_any_element()
                        })
                        .unwrap_or_else(|| div().into_any_element()),
                ),
            )
            .child(
                ui::button::Button::new(format!(
                    "remove-macro-pin-{}-{}-{}",
                    macro_id, pin_index, is_input
                ))
                .icon(IconName::Close)
                .ghost()
                .xsmall()
                .on_click(on_remove),
            )
            .into_any_element()
    }

    // ── Event details (editable fields) ────────────────────────────────────────
}
