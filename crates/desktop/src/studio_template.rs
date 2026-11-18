use super::*;

impl Studio {
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
        if !self.can_export() || self.design.is_none() {
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
            self.fail(
                self.t(
                    "先为一个可见的文字或条码绑定变量。",
                    "Bind a visible text or barcode to a variable first.",
                ),
                cx,
            );
            return;
        }
        let source = self.file.clone();
        let filename = source
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("label");
        let name = format!(
            "{}.tpl.zpl",
            filename.strip_suffix(".labelize.json").unwrap_or(filename)
        );
        let picker = cx.prompt_for_new_path(
            source
                .as_ref()
                .and_then(|p| p.parent())
                .unwrap_or(&self.options.output_dir),
            Some(&name),
        );
        let paths = self.paths.clone();
        let dpmm = self.options.dpmm;
        self.submitting = true;
        self.notice = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result: Result<_> = async {
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
                        studio_worker::export_template(&paths, bytes, dpmm, &path)?;
                        Ok(Some(path))
                    })
                    .await
            }
            .await;
            let _ = this.update_in(cx, |s, _, cx| {
                s.submitting = false;
                match result {
                    Ok(Some(path)) => {
                        s.notice = Some(Ok(format!(
                            "{} {}",
                            s.t("ZPL 模板已保存到", "ZPL template saved to"),
                            path.display()
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
