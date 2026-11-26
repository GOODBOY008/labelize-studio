use super::*;

impl Studio {
    pub(super) fn template_options(&self, cx: &mut Context<Self>) -> Div {
        let lang = self.template_language;
        let current = self.design.as_ref().unwrap().printer_profile(lang);
        let mut panel =
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(section(self.t(Text::TemplateLanguage)))
                .child(
                    row()
                        .flex_wrap()
                        .children(TemplateLanguage::ALL.into_iter().enumerate().map(
                            |(i, target)| {
                                Button::new(("template-language", i))
                                    .small()
                                    .label(target.name())
                                    .selected(lang == target)
                                    .on_click(cx.listener(move |s, _, w, cx| {
                                        s.select_template_language(target, w, cx);
                                    }))
                            },
                        )),
                )
                .child(self.field(17, self.t(Text::DeviceFont)));
        let hint = match lang {
            TemplateLanguage::Zpl => self.t(Text::ZplFontHint),
            TemplateLanguage::Tspl => self.t(Text::TsplFontHint),
            TemplateLanguage::Epl => self.t(Text::EplFontHint),
            TemplateLanguage::Cpcl => self.t(Text::CpclFontHint),
            TemplateLanguage::Dpl => self.t(Text::DplFontHint),
        };
        panel = panel.child(div().text_xs().text_color(rgb(MUTED)).child(hint));
        if matches!(lang, TemplateLanguage::Cpcl | TemplateLanguage::Tspl) {
            panel = panel.child(
                row().children(
                    [TemplateEncoding::Ascii, TemplateEncoding::Utf8]
                        .into_iter()
                        .enumerate()
                        .map(|(i, encoding)| {
                            Button::new(("template-encoding", i))
                                .small()
                                .label(encoding.name())
                                .selected(current.encoding == encoding)
                                .on_click(cx.listener(move |s, _, w, cx| {
                                    s.set_template_encoding(encoding, w, cx);
                                }))
                        }),
                ),
            );
        }
        panel
    }
    pub(super) fn select_template_language(
        &mut self,
        target: TemplateLanguage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.template_language == target {
            return;
        }
        if let Err(e) = self.apply_fields(cx) {
            self.fail(e, cx);
            return;
        }
        self.template_language = target;
        self.persist_version += 1;
        if self.parameter_mode {
            self.invalidate_preview(cx);
        }
        self.sync_form(window, cx);
    }

    pub(super) fn set_template_encoding(
        &mut self,
        encoding: TemplateEncoding,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(e) = self.apply_fields(cx) {
            self.fail(e, cx);
            return;
        }
        let Some(mut design) = self.design.clone() else {
            return;
        };
        let lang = self.template_language;
        let mut profile = design.printer_profile(lang);
        if encoding == profile.encoding {
            return;
        }
        profile.encoding = encoding;
        if let Err(e) = design.set_printer_profile(lang, profile) {
            self.fail(e, cx);
            return;
        }
        self.remember();
        self.design = Some(design);
        self.changed(cx);
        self.sync_form(window, cx);
    }

    pub(super) fn read_samples(&self, design: &mut DesignDocument, cx: &App) {
        for (key, input) in &self.sample_inputs {
            design
                .samples
                .insert(key.clone(), input.read(cx).value().to_string());
        }
    }

    pub(super) fn sync_sample_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let names = self
            .selected_object()
            .and_then(|o| o.variable_names().ok())
            .unwrap_or_default();
        self.sample_inputs.retain(|key, _| names.contains(key));
        for name in names {
            let value = self
                .design
                .as_ref()
                .and_then(|d| d.samples.get(&name))
                .cloned()
                .unwrap_or_default();
            let input = self.sample_inputs.entry(name).or_insert_with(|| {
                let input = cx.new(|cx| InputState::new(window, cx).multi_line(true).rows(2));
                cx.subscribe(&input, |s, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) && s.form_changed(cx) {
                        s.changed(cx);
                    }
                })
                .detach();
                input
            });
            if input.read(cx).value().as_str() != value {
                input.update(cx, |i, cx| i.set_value(value, window, cx));
            }
        }
    }

    pub(super) fn export_template(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading || self.submitting || self.design.is_none() {
            return;
        }
        let bytes = match self.payload(cx) {
            Ok(bytes) => bytes,
            Err(e) => {
                self.fail(format!("{e:#}"), cx);
                return;
            }
        };
        if !self
            .design
            .as_ref()
            .is_some_and(|d| d.objects.iter().any(|o| o.variable && o.style.visible))
        {
            self.notice = Some(Err(Text::BindVisibleField.into()));
            cx.notify();
            return;
        }
        let source = self.file.clone();
        let filename = source
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("label");
        let name = format!(
            "{}.tpl.{}",
            filename.strip_suffix(".labelize.json").unwrap_or(filename),
            self.template_language.extension()
        );
        let directory = source
            .as_ref()
            .and_then(|p| p.parent())
            .unwrap_or(&self.options.output_dir)
            .to_owned();
        let paths = self.paths.clone();
        let dpmm = self.options.dpmm;
        let language = self.template_language;
        self.submitting = true;
        self.notice = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result: Result<_> = async {
                let template =
                    cx.background_executor()
                        .spawn(async move {
                            studio_worker::export_template(&paths, bytes, language, dpmm)
                        })
                        .await?;
                let picker = this.update_in(cx, |_, _, cx| {
                    cx.prompt_for_new_path(&directory, Some(&name))
                })?;
                let Some(path) = picker.await?? else {
                    return Ok(None);
                };
                ensure!(
                    !source.as_ref().is_some_and(
                        |p| session::canonical_path(p) == session::canonical_path(&path)
                    ),
                    "Choose a different path from the editable design"
                );
                cx.background_executor()
                    .spawn(async move {
                        atomic_write(&path, &template)?;
                        Ok(Some(path))
                    })
                    .await
            }
            .await;
            let _ = this.update_in(cx, |s, _, cx| {
                s.submitting = false;
                match result {
                    Ok(Some(path)) => {
                        s.notice = Some(Ok(Message::formatted(
                            Text::TemplateSavedTo,
                            [("path", path.display().to_string())],
                        )))
                    }
                    Ok(None) => {}
                    Err(e) => s.fail(format!("{e:#}"), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
