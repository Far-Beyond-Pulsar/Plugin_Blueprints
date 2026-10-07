use super::*;

impl PropertiesRenderer {
    pub(super) fn render_event_details(
        panel: &mut BlueprintEditorPanel,
        index: usize,
        window: &mut Window,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
        let Some(event_def) = panel.local_event_defs.get(index).cloned() else {
            return Self::render_empty_state(cx);
        };
        let event_uid = event_def.uid.clone();

        v_flex()
            .gap_3()
            .child(Self::render_card(
                [
                    h_flex()
                        .w_full()
                        .p_3()
                        .gap_3()
                        .items_center()
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
                        )
                        .into_any_element(),
                    div()
                        .w_full()
                        .px_3()
                        .pb_3()
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
                        )
                        .into_any_element(),
                ],
                cx,
            ))
            .child(Self::render_event_fields_card(
                panel, &event_uid, window, cx,
            ))
            .when(!event_def.return_type.is_empty(), |el| {
                el.child(Self::render_card(
                    [
                        Self::render_section_header("Return Type", IconName::ArrowRight, cx)
                            .px_3()
                            .pt_3()
                            .into_any_element(),
                        div()
                            .w_full()
                            .p_3()
                            .text_sm()
                            .text_color(cx.theme().foreground)
                            .child(event_def.return_type.clone())
                            .into_any_element(),
                    ],
                    cx,
                ))
            })
            .child(Self::render_card(
                [
                    Self::render_section_header("Event Info", IconName::Info, cx)
                        .px_3()
                        .pt_3()
                        .into_any_element(),
                    v_flex()
                        .w_full()
                        .p_3()
                        .gap_2p5()
                        .child(Self::render_info_row("UID", &event_def.uid, cx))
                        .into_any_element(),
                ],
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_event_fields_card(
        panel: &mut BlueprintEditorPanel,
        event_uid: &str,
        window: &mut Window,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
        let Some(event_def) = panel
            .local_event_defs
            .iter()
            .find(|d| d.uid == event_uid)
            .cloned()
        else {
            return div().into_any_element();
        };
        let uid = event_uid.to_string();

        let on_add = cx.listener(
            move |this: &mut BlueprintEditorPanel, _: &gpui::ClickEvent, window, cx| {
                this.add_event_field(&uid, "new_field".to_string(), "?".to_string());
                this.sync_all_events(window, cx);
                cx.notify();
            },
        );

        Self::render_card(
            [
                Self::render_section_header("Fields", IconName::List, cx)
                    .px_3()
                    .pt_3()
                    .into_any_element(),
                v_flex()
                    .w_full()
                    .p_3()
                    .gap_1p5()
                    .children(event_def.fields.iter().enumerate().map(|(fi, field)| {
                        Self::render_editable_field_row(panel, event_uid, fi, field, window, cx)
                    }))
                    .when(event_def.fields.is_empty(), |el| {
                        el.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("No fields"),
                        )
                    })
                    .child(
                        h_flex().w_full().pt_1().child(
                            ui::button::Button::new(format!("add-event-field-{}", event_uid))
                                .label("Add Field")
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

    pub(super) fn render_editable_field_row(
        panel: &mut BlueprintEditorPanel,
        event_uid: &str,
        field_index: usize,
        field: &crate::core::graph::CustomEventField,
        window: &mut Window,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
        use ui::input::TextInput;
        let name_key = (event_uid.to_string(), field_index);
        let type_key = (event_uid.to_string(), field_index);

        if !panel.event_field_name_inputs.contains_key(&name_key) {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder("Name..."));
            input.update(cx, |state, cx| {
                state.set_value(&field.name, window, cx);
            });
            let sub_uid = event_uid.to_string();
            let sub_fi = field_index;
            cx.subscribe_in(
                &input,
                window,
                move |this: &mut BlueprintEditorPanel, state, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                        let new_name = state.read(cx).text().to_string().trim().to_string();
                        if !new_name.is_empty() {
                            if let Some(def) =
                                this.local_event_defs.iter_mut().find(|d| d.uid == sub_uid)
                            {
                                if let Some(f) = def.fields.get_mut(sub_fi) {
                                    f.name = new_name;
                                }
                            }
                            this.sync_all_events(window, cx);
                            cx.notify();
                        }
                    }
                },
            )
            .detach();
            panel
                .event_field_name_inputs
                .insert(name_key.clone(), input);
        }

        if !panel.event_field_type_inputs.contains_key(&type_key) {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder("Type..."));
            input.update(cx, |state, cx| {
                state.set_value(&field.type_name, window, cx);
            });
            let sub_uid = event_uid.to_string();
            let sub_fi = field_index;
            cx.subscribe_in(
                &input,
                window,
                move |this: &mut BlueprintEditorPanel, state, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                        let new_type = state.read(cx).text().to_string().trim().to_string();
                        if !new_type.is_empty() {
                            if let Some(def) =
                                this.local_event_defs.iter_mut().find(|d| d.uid == sub_uid)
                            {
                                if let Some(f) = def.fields.get_mut(sub_fi) {
                                    f.type_name = new_type;
                                }
                            }
                            this.sync_all_events(window, cx);
                            cx.notify();
                        }
                    }
                },
            )
            .detach();
            panel
                .event_field_type_inputs
                .insert(type_key.clone(), input);
        }

        let name_input = panel.event_field_name_inputs.get(&name_key).cloned();
        let type_input = panel.event_field_type_inputs.get(&type_key).cloned();
        let uid = event_uid.to_string();

        let on_remove = cx.listener(
            move |this: &mut BlueprintEditorPanel, _: &gpui::ClickEvent, window, cx| {
                let field_name = this
                    .local_event_defs
                    .iter()
                    .find(|d| d.uid == uid)
                    .and_then(|d| d.fields.get(field_index))
                    .map(|f| f.name.clone())
                    .unwrap_or_default();
                this.remove_event_field(&uid, &field_name);
                this.sync_all_events(window, cx);
                this.event_field_name_inputs
                    .remove(&(uid.clone(), field_index));
                this.event_field_type_inputs
                    .remove(&(uid.clone(), field_index));
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
                    "remove-event-field-{}-{}",
                    event_uid, field_index
                ))
                .icon(IconName::Xmark)
                .ghost()
                .xsmall()
                .on_click(on_remove),
            )
            .into_any_element()
    }

    // ── Variable details ─────────────────────────────────────────────────────
}
