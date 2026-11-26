use super::*;

type Values = BTreeMap<String, String>;

/// This editor belongs to one live tab. None of its inputs are part of the saved draft.
pub(super) struct RecordEditor {
    pub(super) values: Values,
    pub(super) json_error: Option<Message>,
    json: Entity<InputState>,
    json_text: String,
    fields: Vec<(String, Entity<InputState>)>,
}

fn parse_record(text: &str) -> Result<Values, Message> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|error: serde_json::Error| {
            Message::formatted(
                Text::RecordJsonSyntax,
                [
                    ("line", error.line().max(1).to_string()),
                    ("column", error.column().max(1).to_string()),
                ],
            )
        })?;
    let object = value
        .as_object()
        .ok_or(Message::Label(Text::RecordJsonObject))?;
    object
        .iter()
        .map(|(key, value)| {
            let text = value.as_str().ok_or_else(|| {
                Message::formatted(Text::RecordFieldString, [("field", key.clone())])
            })?;
            Ok((key.clone(), text.to_owned()))
        })
        .collect()
}

impl RecordEditor {
    pub(super) fn new(values: Values, window: &mut Window, cx: &mut Context<Studio>) -> Self {
        let json_text = serde_json::to_string_pretty(&values).expect("string map");
        let json = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .rows(4)
                .default_value(json_text.clone())
        });
        cx.subscribe_in(
            &json,
            window,
            |studio, input, event: &InputEvent, window, cx| {
                if !matches!(event, InputEvent::Change) || !studio.parameter_mode {
                    return;
                }
                let Some(editor) = &mut studio.record_editor else {
                    return;
                };
                let text = input.read(cx).value().to_string();
                if text == editor.json_text {
                    return;
                }
                editor.json_text = text;
                match parse_record(&editor.json_text) {
                    Ok(values) => {
                        editor.values = values;
                        editor.json_error = None;
                        editor.sync_values(window, cx);
                    }
                    Err(error) => editor.json_error = Some(error),
                }
                studio.invalidate_preview(cx);
            },
        )
        .detach();
        Self {
            values,
            json,
            json_text,
            json_error: None,
            fields: Vec::new(),
        }
    }

    fn sync_values(&self, window: &mut Window, cx: &mut Context<Studio>) {
        for (name, input) in &self.fields {
            let text = self.values.get(name).map_or("", String::as_str);
            if input.read(cx).value().as_str() != text {
                input.update(cx, |input, cx| input.set_value(text.to_owned(), window, cx));
            }
        }
    }

    pub(super) fn sync_fields(
        &mut self,
        names: &[String],
        window: &mut Window,
        cx: &mut Context<Studio>,
    ) {
        if self.fields.iter().map(|(name, _)| name).eq(names) {
            return;
        }
        let mut previous: BTreeMap<_, _> = self.fields.drain(..).collect();
        for name in names {
            let input = previous.remove(name).unwrap_or_else(|| {
                let initial = self.values.get(name).cloned().unwrap_or_default();
                let input = cx.new(|cx| {
                    InputState::new(window, cx)
                        .multi_line(true)
                        .rows(2)
                        .default_value(initial)
                });
                let key = name.clone();
                cx.subscribe_in(
                    &input,
                    window,
                    move |studio, field, event: &InputEvent, window, cx| {
                        if !matches!(event, InputEvent::Change) || !studio.parameter_mode {
                            return;
                        }
                        let Some(editor) = &mut studio.record_editor else {
                            return;
                        };
                        let text = field.read(cx).value().to_string();
                        if editor.values.get(&key).map_or("", String::as_str) == text {
                            return;
                        }
                        editor.values.insert(key.clone(), text);
                        editor.json_error = None;
                        editor.json_text =
                            serde_json::to_string_pretty(&editor.values).expect("string map");
                        editor.json.update(cx, |input, cx| {
                            input.set_value(editor.json_text.clone(), window, cx)
                        });
                        studio.invalidate_preview(cx);
                    },
                )
                .detach();
                input
            });
            self.fields.push((name.clone(), input));
        }
    }
}

impl Studio {
    pub(super) fn toggle_parameters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.design.is_none() {
            return;
        }
        if let Err(e) = self.apply_fields(cx) {
            self.fail(e, cx);
            return;
        }
        self.parameter_mode = !self.parameter_mode;
        if self.parameter_mode {
            let samples = &self.design.as_ref().expect("design tab").samples;
            self.record_editor
                .get_or_insert_with(|| RecordEditor::new(samples.clone(), window, cx));
            if self.options.format == OutputFormat::Zpl {
                self.options.format = OutputFormat::Png;
                self.persist_version += 1;
            }
            self.layers_open = false;
            self.inspector_open = window.viewport_size().width < px(700.);
        }
        self.invalidate_preview(cx);
    }

    pub(super) fn record_panel(&self) -> Div {
        let mut panel = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(section(self.t(Text::PreviewParameters)))
            .when(self.design.is_none(), |panel| {
                panel.child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(self.t(Text::MatchTemplateMedia)),
                )
            });
        let Some(editor) = &self.record_editor else {
            return panel;
        };
        let problems = self
            .preview
            .as_ref()
            .and_then(|p| p.template.as_ref())
            .map(|r| r.problems.as_slice())
            .unwrap_or_default();
        for (name, input) in &editor.fields {
            panel = panel
                .child(div().text_sm().child(name.clone()))
                .child(Input::new(input));
            for problem in problems.iter().filter(|p| p.fields.contains(name)) {
                panel = panel.child(div().text_xs().text_color(rgb(0xa22a34)).child(
                    self.language.format(
                        Text::RecordFieldProblem,
                        &[
                            ("object", &problem.object.to_string()),
                            ("kind", &self.language.field_kind(&problem.kind)),
                            ("reason", &self.language.diagnostic(&problem.message)),
                        ],
                    ),
                ));
            }
        }
        panel = panel
            .child(div().text_sm().child(self.t(Text::RecordJson)))
            .child(Input::new(&editor.json));
        if let Some(message) = &editor.json_error {
            panel = panel.child(
                div()
                    .text_xs()
                    .text_color(rgb(0xa22a34))
                    .child(message.display(self.language)),
            );
        }
        panel
    }
}

#[cfg(test)]
mod tests {
    use super::{
        parse_record, AppPaths, Draft, InputState, Language, Settings, Studio, TemplateLanguage,
    };
    use gpui::{App, AppContext as _, Entity, TestAppContext, VisualTestContext, Window};
    use std::sync::Arc;

    #[gpui::test]
    fn record_inputs_are_isolated_between_tabs_and_from_saved_drafts(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let cx = cx.add_empty_window();
        let directory = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(directory.path().to_owned()).unwrap();
        let mut design = Draft::new(Settings::default());
        let asset_label = design.design.as_mut().unwrap();
        asset_label.objects[0].text = "{{asset_id}} / {{location}}".into();
        asset_label.set_variable(0, true).unwrap();
        asset_label.samples = [
            ("asset_id".into(), "001234".into()),
            ("location".into(), "Lab".into()),
        ]
        .into();
        let samples = asset_label.samples.clone();
        let fields: Vec<_> = asset_label.variable_names().unwrap().into_iter().collect();
        design.dirty = false;
        let mut file = Draft::new(Settings::default());
        file.design = None;
        file.file = Some("shipping-logo.tpl.dpl".into());
        file.source =
            include_bytes!("../../../examples/print-templates/shipping-logo.tpl.dpl").to_vec();
        file.dirty = false;
        let make_tab = |draft, w: &mut Window, cx: &mut App| {
            cx.new(|cx| {
                Studio::new(
                    paths.clone(),
                    Language::English,
                    draft,
                    Arc::default(),
                    Arc::default(),
                    w,
                    cx,
                )
            })
        };
        let (first, second) = cx.update(|w, cx| (make_tab(design, w, cx), make_tab(file, w, cx)));
        let json = cx.update(|w, cx| {
            first.update(cx, |s, cx| {
                s.set_active(true, w, cx);
                s.toggle_parameters(w, cx);
                let editor = s.record_editor.as_mut().unwrap();
                assert_eq!(editor.values, samples);
                editor.sync_fields(&fields, w, cx);
                editor.json.clone()
            })
        });
        let change = |cx: &mut VisualTestContext, input: &Entity<InputState>, text: &str| {
            cx.update(|w, cx| input.update(cx, |i, cx| i.set_value(text.to_owned(), w, cx)));
            cx.run_until_parked();
        };
        change(cx, &json, r#"{"asset_id":"000123","location":"Lab"}"#);
        let (ticket, revision) =
            cx.update(|_, cx| first.update(cx, |s, _| (s.rendering.begin(s.revision), s.revision)));
        change(cx, &json, r#"{"asset_id":"000456"}"#);
        let field = cx.update(|_, cx| {
            first.update(cx, |s, cx| {
                assert!(s.revision > revision && !s.rendering.finish(ticket, s.revision));
                let editor = s.record_editor.as_ref().unwrap();
                assert_eq!(editor.values.len(), 1);
                assert_eq!(editor.fields[0].1.read(cx).value().as_str(), "000456");
                assert!(editor.fields[1].1.read(cx).value().is_empty());
                editor.fields[1].1.clone()
            })
        });
        change(cx, &json, r#"{"location":123}"#);
        cx.update(|w, cx| {
            first.update(cx, |s, cx| {
                s.render_preview(w, cx);
                assert!(s
                    .preview_error
                    .as_ref()
                    .unwrap()
                    .display(s.language)
                    .contains("location"));
                let revision = s.revision;
                s.language = Language::Chinese;
                assert!(s
                    .preview_error
                    .as_ref()
                    .unwrap()
                    .display(s.language)
                    .contains("必须是字符串"));
                assert_eq!(
                    s.record_editor.as_ref().unwrap().json_text,
                    r#"{"location":123}"#
                );
                assert_eq!(s.revision, revision);
                assert!(!s.can_export());
            })
        });
        change(cx, &field, "Private {{location}}");
        cx.update(|w, cx| {
            first.update(cx, |s, cx| {
                s.set_active(false, w, cx);
                assert!(!s.dirty);
                assert_eq!(s.source_revision, 0);
                assert!(s.record_editor.as_ref().unwrap().json_error.is_none());
                assert_eq!(
                    parse_record(&json.read(cx).value()).unwrap()["location"],
                    "Private {{location}}"
                );
                let snapshot = s.snapshot(cx);
                assert_eq!(snapshot.design.as_ref().unwrap().samples, samples);
                assert!(!serde_json::to_string(&snapshot)
                    .unwrap()
                    .contains("Private"));
            });
            let snapshot = second.update(cx, |s, cx| {
                s.set_active(true, w, cx);
                assert!(s.parameter_mode && !s.editable);
                assert_eq!(s.template_language, TemplateLanguage::Dpl);
                assert!(s.record_editor.as_ref().unwrap().values.is_empty());
                s.record_editor
                    .as_mut()
                    .unwrap()
                    .values
                    .insert("product".into(), "Thermal labels".into());
                s.snapshot(cx)
            });
            assert_eq!(
                snapshot.source,
                include_bytes!("../../../examples/print-templates/shipping-logo.tpl.dpl")
            );
            let restored = make_tab(snapshot, w, cx);
            assert!(restored
                .read(cx)
                .record_editor
                .as_ref()
                .unwrap()
                .values
                .is_empty());
            first.update(cx, |s, cx| {
                s.set_active(true, w, cx);
                assert_eq!(
                    s.record_editor.as_ref().unwrap().values["asset_id"],
                    "000456"
                );
            });
        });
    }
}
