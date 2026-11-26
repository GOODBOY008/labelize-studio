use super::*;
use crate::updates::{Phase, UpdateEvent};
use labelize_printer_updater::{self as updater, handoff, InstallSource};
use std::sync::atomic::Ordering;

impl PrinterApp {
    pub(super) fn receive_update(
        &mut self,
        event: UpdateEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            UpdateEvent::Detected(source, target, manual) => {
                self.updates.source = self.preferences.update_source.clone().unwrap_or(source);
                self.updates.manual_install = manual;
                match target {
                    Ok(t) => self.updates.target = Some(t),
                    Err(e) => self.updates.error = Some(e),
                }
                if updater::check_due(
                    self.preferences.automatic_updates,
                    self.preferences.last_update_check,
                    chrono::Utc::now().timestamp(),
                ) {
                    self.check_update(false, cx);
                }
            }
            UpdateEvent::Checked(result, _manual) => {
                self.updates.phase = Phase::Idle;
                match result {
                    Ok(offer) => {
                        // Invalidate a cached installer if the release was replaced or withdrawn.
                        if self.updates.verified.as_ref().is_some_and(|v| {
                            offer.as_ref().is_none_or(|o| {
                                o.version != v.offer.version
                                    || o.installer.digest != v.offer.installer.digest
                                    || o.installer.size != v.offer.installer.size
                                    || o.sums.digest != v.offer.sums.digest
                            })
                        }) {
                            self.updates.verified = None;
                        }
                        self.updates.offer = offer;
                        self.updates.checked = true;
                        self.updates.error = None;
                    }
                    Err(e) => {
                        log::warn!("Update check: {e}");
                        self.updates.error = Some(e);
                        // A delayed check must not navigate away from a document the user is editing.
                    }
                }
            }
            UpdateEvent::Progress(done, total) => self.updates.progress = (done, total),
            UpdateEvent::Downloaded(result) => {
                self.updates.phase = Phase::Idle;
                match result {
                    Ok(file) => {
                        self.updates.verified = Some(file);
                        self.updates.error = None;
                    }
                    Err(e) => {
                        if self.updates.cancel.load(Ordering::Acquire) {
                            self.updates.message = Some(Text::DownloadCancelled.into());
                        } else {
                            self.updates.error = Some(e);
                        }
                    }
                }
            }
            UpdateEvent::Prepared(result) => {
                self.modal = false;
                match result {
                    Ok(ticket) => {
                        self.updates.ticket = Some(ticket.clone());
                        // Re-save here: the user could have edited while verification ran.
                        if self.request_close(window, cx) {
                            match handoff::authorize(&ticket) {
                                Ok(()) => {
                                    cx.quit();
                                    return;
                                }
                                Err(error) => self.updates.error = Some(format!("{error:#}")),
                            }
                        }
                        handoff::cancel(&ticket);
                    }
                    Err(e) => self.updates.error = Some(e),
                }
                self.updates.ticket = None;
                self.updates.phase = Phase::Idle;
                self.studio
                    .update(cx, |s, cx| s.resume_update_previews(window, cx));
            }
            UpdateEvent::Pending(error) => {
                self.updates.recovery = Some(error);
                self.change_page(Page::Updates, window, cx);
            }
            UpdateEvent::Recovery(result) => {
                self.updates.phase = Phase::Idle;
                match result {
                    Ok(message) => {
                        self.updates.recovery = None;
                        self.updates.error = None;
                        self.updates.message = Some(message.into());
                        self.send(Work::Refresh, cx);
                    }
                    Err(e) => self.updates.recovery = Some(e),
                }
            }
        }
        cx.notify();
    }
    pub(super) fn check_update(&mut self, manual: bool, cx: &mut Context<Self>) {
        if self.updates.phase != Phase::Idle || self.updates.target.is_none() {
            return;
        }
        let mut prefs = self.preferences.clone();
        prefs.last_update_check = Some(chrono::Utc::now().timestamp());
        match prefs.save(&self.preferences_path) {
            Ok(()) => {
                self.preferences = prefs;
                self.updates.check(manual);
            }
            Err(e) => self.updates.error = Some(format!("{e:#}")),
        }
        cx.notify();
    }
    fn settings_dirty(&self, cx: &App) -> bool {
        self.snapshot.as_ref().is_some_and(|s| {
            self.draft != s.settings
                || self.output_dir.read(cx).value().as_str()
                    != s.settings.output_dir.to_string_lossy()
                || self.width.read(cx).value().trim().parse::<f64>().ok()
                    != Some(s.settings.width_mm)
                || self.height.read(cx).value().trim().parse::<f64>().ok()
                    != Some(s.settings.height_mm)
        })
    }
    pub(super) fn confirm_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.updates.phase != Phase::Idle || self.modal || self.activity.is_some() {
            return;
        }
        if self.settings_dirty(cx) {
            self.modal = true;
            let prompt = window.prompt(
                PromptLevel::Warning,
                self.text(Text::SaveOutputSettings),
                Some(self.text(Text::UnsavedUpdateSettings)),
                &[
                    PromptButton::cancel(self.text(Text::Cancel)),
                    PromptButton::new(self.text(Text::DiscardSettings)),
                    PromptButton::new(self.text(Text::SaveAndContinue)),
                ],
                cx,
            );
            cx.spawn_in(window, async move |this, cx| {
                let choice = prompt.await.ok();
                let _ = this.update_in(cx, |s, w, cx| {
                    s.modal = false;
                    match choice {
                        Some(1) => s.begin_update(w, cx),
                        Some(2) => {
                            s.submit_form(Activity::Save, cx);
                            s.update_after_save = s.activity == Some(Activity::Save);
                        }
                        _ => cx.notify(),
                    }
                });
            })
            .detach();
            return;
        }
        self.begin_update(window, cx);
    }
    fn begin_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(offer) = self.updates.offer.as_ref() else {
            return;
        };
        if self.updates.source == InstallSource::Unknown {
            return;
        }
        if self.updates.source == InstallSource::Package && self.updates.verified.is_none() {
            return;
        }
        let plan = handoff::Plan {
            source: self.updates.source.clone(),
            version: offer.version.clone(),
            installer: if self.updates.source == InstallSource::Package {
                self.updates.verified.clone()
            } else {
                None
            },
            manual: self.updates.manual_install || self.updates.source == InstallSource::Homebrew,
        };
        if !self.request_close(window, cx) {
            return;
        }
        let command = if plan.source == InstallSource::Homebrew {
            Some(updater::BREW_COMMAND.to_string())
        } else if plan.manual {
            plan.installer
                .as_ref()
                .map(|v| updater::platform::apt_command(&v.path))
        } else {
            None
        };
        let detail = if let Some(command) = &command {
            format!(
                "{}\n\n{command}\n\n{}",
                self.text(Text::BrewUpgradeHint),
                self.text(Text::ReopenAfterUpgrade)
            )
        } else {
            self.text(Text::InstallerHandoffHint).into()
        };
        self.modal = true;
        let prompt = window.prompt(
            PromptLevel::Info,
            self.text(Text::ReadyToInstall),
            Some(&detail),
            &[
                PromptButton::cancel(self.text(Text::Cancel)),
                PromptButton::new(self.text(Text::QuitAndContinue)),
            ],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let choice = prompt.await.ok();
            let _ = this.update_in(cx, |s, _w, cx| {
                s.modal = false;
                if choice != Some(1) {
                    cx.notify();
                    return;
                }
                if let Some(command) = command {
                    cx.write_to_clipboard(ClipboardItem::new_string(command));
                }
                s.updates.phase = Phase::Preparing;
                s.updates.error = None;
                s.modal = true;
                let queue = s.studio.read(cx).update_previews();
                queue.pause();
                let paths = s.updates.paths.clone();
                let tx = s.updates.events.clone();
                std::thread::spawn(move || {
                    let result = (|| -> anyhow::Result<handoff::Ticket> {
                        let until = std::time::Instant::now() + std::time::Duration::from_secs(15);
                        while !queue.idle() {
                            anyhow::ensure!(
                                std::time::Instant::now() < until,
                                "Preview renderer has not stopped; installation was not started"
                            );
                            std::thread::sleep(std::time::Duration::from_millis(50));
                        }
                        handoff::prepare(&paths, plan)
                    })()
                    .map_err(|e| format!("{e:#}"));
                    let _ = tx.send_blocking(UpdateEvent::Prepared(result));
                });
                cx.notify();
            });
        })
        .detach();
    }
    fn set_update_source(&mut self, source: InstallSource, cx: &mut Context<Self>) {
        let mut prefs = self.preferences.clone();
        prefs.update_source = Some(source.clone());
        match prefs.save(&self.preferences_path) {
            Ok(()) => {
                self.preferences = prefs;
                self.updates.source = source;
            }
            Err(e) => self.updates.error = Some(format!("{e:#}")),
        }
        cx.notify();
    }
    pub(super) fn update_page(&self, compact: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let idle = self.updates.phase == Phase::Idle;
        let mut page = div().flex().flex_col().gap_4().min_w_0();
        let status = if self.updates.phase == Phase::Checking {
            self.text(Text::CheckingUpdate)
        } else if self.updates.phase == Phase::Preparing {
            self.text(Text::PreparingInstall)
        } else if self.updates.phase == Phase::Downloading {
            self.text(Text::DownloadingVerified)
        } else if self.updates.verified.is_some() {
            self.text(Text::InstallerVerified)
        } else if self.updates.offer.is_some() {
            self.text(Text::UpdateAvailable)
        } else if self.updates.error.is_some() {
            self.text(Text::UpdateIncomplete)
        } else if self.updates.checked {
            self.text(Text::UpToDate)
        } else {
            self.text(Text::CheckStableUpdates)
        };
        let last = self
            .preferences
            .last_update_check
            .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
            .map(|t| {
                t.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
            })
            .unwrap_or_else(|| self.text(Text::NotChecked).into());
        page = page.child(
            card(compact)
                .child(section_title(status))
                .child(div().text_sm().child(self.preferences.language.format(
                    Text::CurrentVersion,
                    &[("version", env!("CARGO_PKG_VERSION"))],
                )))
                .child(
                    div().text_xs().text_color(rgb(MUTED)).child(
                        self.preferences
                            .language
                            .format(Text::LastCheck, &[("time", &last)]),
                    ),
                )
                .child(
                    button_row()
                        .child(
                            Button::new("check-update")
                                .label(self.text(Text::CheckUpdates))
                                .disabled(!idle || self.updates.target.is_none())
                                .on_click(cx.listener(|s, _, _, cx| s.check_update(true, cx))),
                        )
                        .child(
                            Button::new("automatic-updates")
                                .ghost()
                                .selected(self.preferences.automatic_updates)
                                .label(if self.preferences.automatic_updates {
                                    self.text(Text::AutomaticChecksOn)
                                } else {
                                    self.text(Text::AutomaticChecksOff)
                                })
                                .on_click(cx.listener(|s, _, _, cx| {
                                    let mut prefs = s.preferences.clone();
                                    prefs.automatic_updates = !prefs.automatic_updates;
                                    match prefs.save(&s.preferences_path) {
                                        Ok(()) => s.preferences = prefs,
                                        Err(e) => s.updates.error = Some(format!("{e:#}")),
                                    }
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(self.text(Text::AutomaticChecksHelp)),
                ),
        );
        if let Some(error) = &self.updates.recovery {
            page = page.child(
                card(compact)
                    .child(section_title(self.text(Text::VerifyInstallation)))
                    .child(banner(self.preferences.language.diagnostic(error), true))
                    .child(div().text_sm().child(self.text(Text::RetryInstallerHint)))
                    .child(
                        button_row()
                            .child(
                                Button::new("verify-update")
                                    .primary()
                                    .label(self.text(Text::VerifyNewVersion))
                                    .disabled(!idle)
                                    .on_click(cx.listener(|s, _, _, cx| {
                                        s.updates.recover(false);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("resume-old-version")
                                    .label(self.text(Text::ResumeCurrentVersion))
                                    .disabled(!idle)
                                    .on_click(cx.listener(|s, _, _, cx| {
                                        s.updates.recover(true);
                                        cx.notify();
                                    })),
                            ),
                    ),
            );
        }
        if let Some(error) = &self.updates.error {
            page = page.child(banner(self.preferences.language.diagnostic(error), true));
        }
        if let Some(message) = &self.updates.message {
            page = page.child(banner(message.display(self.preferences.language), false));
        }
        if self.updates.source == InstallSource::Unknown && self.updates.target.is_some() {
            page = page.child(
                card(compact)
                    .child(section_title(self.text(Text::ChooseInstallSource)))
                    .child(
                        div()
                            .text_sm()
                            .child(self.text(Text::UnavailableBrewInventory)),
                    )
                    .child(
                        button_row()
                            .child(
                                Button::new("source-pkg")
                                    .label(self.text(Text::DirectInstaller))
                                    .on_click(cx.listener(|s, _, _, cx| {
                                        s.set_update_source(InstallSource::Package, cx)
                                    })),
                            )
                            .child(Button::new("source-brew").label("Homebrew").on_click(
                                cx.listener(|s, _, _, cx| {
                                    s.set_update_source(InstallSource::Homebrew, cx)
                                }),
                            )),
                    ),
            );
        }
        if let Some(offer) = &self.updates.offer {
            let url = offer.release_url.clone();
            let mut release = card(compact)
                .child(
                    div().text_lg().font_weight(FontWeight::SEMIBOLD).child(
                        self.preferences
                            .language
                            .format(Text::NewVersion, &[("version", &offer.version)]),
                    ),
                )
                .child(
                    Button::new("update-release-notes")
                        .ghost()
                        .label(self.text(Text::ReleaseNotes))
                        .on_click(move |_, _, _| {
                            let url = url.clone();
                            std::thread::spawn(move || {
                                let _ = open::that(url);
                            });
                        }),
                );
            if self.updates.source == InstallSource::Homebrew {
                release = release
                    .child(div().text_sm().child(self.text(Text::BrewManagedHint)))
                    .child(div().text_xs().child(updater::BREW_COMMAND))
                    .child(
                        Button::new("copy-brew")
                            .label(self.text(Text::CopyCommand))
                            .on_click(|_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    updater::BREW_COMMAND.into(),
                                ))
                            }),
                    );
            } else if let Some(file) = &self.updates.verified {
                let folder = file.path.parent().unwrap().to_path_buf();
                release = release
                    .child(
                        div().text_sm().child(
                            self.preferences
                                .language
                                .format(Text::Verified, &[("file", &file.offer.installer.name)]),
                        ),
                    )
                    .child(
                        Button::new("show-update-download")
                            .ghost()
                            .label(self.text(Text::ShowInstaller))
                            .on_click(move |_, _, _| {
                                let folder = folder.clone();
                                std::thread::spawn(move || {
                                    let _ = open::that(folder);
                                });
                            }),
                    );
                if self.updates.manual_install {
                    let command = updater::platform::apt_command(&file.path);
                    release = release
                        .child(div().text_sm().child(self.text(Text::MissingDebInstaller)))
                        .child(div().text_xs().child(command.clone()))
                        .child(
                            Button::new("copy-apt")
                                .label(self.text(Text::CopyCommand))
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        command.clone(),
                                    ))
                                }),
                        );
                }
            } else if self.updates.phase == Phase::Downloading {
                let (done, total) = self.updates.progress;
                let ratio = if total == 0 {
                    0.0
                } else {
                    done as f32 / total as f32
                };
                release = release
                    .child(div().text_sm().child(format!(
                        "{:.1} / {:.1} MB · {:.0}%",
                        done as f64 / 1048576.,
                        total as f64 / 1048576.,
                        ratio * 100.
                    )))
                    .child(
                        div()
                            .w_full()
                            .h(px(6.))
                            .rounded_full()
                            .bg(rgb(BORDER))
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(ratio))
                                    .bg(rgb(ACCENT))
                                    .rounded_full(),
                            ),
                    )
                    .child(
                        Button::new("cancel-update-download")
                            .label(self.text(Text::CancelDownload))
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.updates.cancel.store(true, Ordering::Release);
                                cx.notify();
                            })),
                    );
            } else {
                release = release.child(
                    Button::new("download-update")
                        .primary()
                        .label(self.text(Text::DownloadUpdate))
                        .disabled(!idle || self.updates.source != InstallSource::Package)
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.updates.message = None;
                            s.updates.download();
                            cx.notify();
                        })),
                );
            }
            if self.updates.verified.is_some() || self.updates.source == InstallSource::Homebrew {
                release = release.child(
                    Button::new("install-update")
                        .primary()
                        .label(
                            if self.updates.source == InstallSource::Homebrew
                                || self.updates.manual_install
                            {
                                self.text(Text::QuitToUpgrade)
                            } else {
                                self.text(Text::QuitAndInstall)
                            },
                        )
                        .disabled(
                            !idle
                                || self.modal
                                || self.activity.is_some()
                                || !self.connected()
                                || self.updates.recovery.is_some(),
                        )
                        .on_click(cx.listener(|s, _, w, cx| s.confirm_update(w, cx))),
                );
            }
            release = release.child(
                div()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(self.text(Text::InstallerVerificationHint)),
            );
            page = page.child(release);
        }
        page
    }
}
