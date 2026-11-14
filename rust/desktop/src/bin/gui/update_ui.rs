//! Обновление программы: проверка при запуске, плашка «доступна версия X» на главной,
//! строки в настройках → «доступ и обновления», скачивание и установка (vgd::update + vgd::updater).

use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use vgd::update::{self, Platform, Prefs, Release, Version};
use vgd::updater::{self, AfterUpdate, Ready};

/// Что сейчас с обновлением.
#[derive(Clone, Debug, Default)]
pub(crate) enum Phase {
    #[default]
    Idle,
    Checking,
    Available(Release),
    Downloading { rel: Release, done: u64, total: u64 },
    /// скачано и проверено; ждёт конца загрузок («обновить после загрузок»)
    Ready { rel: Release, ready: Ready },
    /// установка запущена, программа выходит
    Installing(Release),
    Failed { rel: Option<Release>, err: String },
}

pub(crate) enum UpdEvent {
    Checked { manual: bool, result: Result<Option<Release>, String> },
    Progress(u64, u64),
    Downloaded(Result<Ready, String>),
}

pub(crate) struct UpdateUi {
    prefs: Prefs,
    platform: Platform,
    current: Version,
    phase: Phase,
    started: bool,
    /// «позже»: плашка скрыта до следующего запуска
    later: bool,
    notes_open: bool,
    /// идут загрузки — спросить «сейчас или после загрузок»
    ask: bool,
    /// установить, когда очередь опустеет
    after_downloads: bool,
    cancel: Arc<AtomicBool>,
    /// итог «проверить обновления» для строки в настройках
    msg: String,
    /// о какой версии уже сказали уведомлением Windows
    toasted: String,
}

impl Default for UpdateUi {
    fn default() -> Self {
        UpdateUi {
            prefs: Prefs::load(),
            platform: updater::platform(),
            current: Version::parse(APP_VERSION).unwrap_or_default(),
            phase: Phase::Idle,
            started: false,
            later: false,
            notes_open: false,
            ask: false,
            after_downloads: false,
            cancel: Arc::new(AtomicBool::new(false)),
            msg: String::new(),
            toasted: String::new(),
        }
    }
}

impl UpdateUi {
    fn busy(&self) -> bool {
        matches!(self.phase, Phase::Checking | Phase::Downloading { .. } | Phase::Installing(_))
    }
}

/// «84 МБ»
fn mb(b: u64) -> String {
    format!("{:.0} МБ", b as f64 / 1048576.0)
}

/// Markdown релиза → простой текст для окна: без решёток заголовков, списки точками.
fn plain_notes(md: &str) -> String {
    let mut out = Vec::new();
    for l in md.lines() {
        let t = l.trim_end();
        let s = t.trim_start();
        let line = if let Some(h) = s.strip_prefix('#') {
            h.trim_start_matches('#').trim().to_string()
        } else if let Some(i) = s.strip_prefix("- ").or_else(|| s.strip_prefix("* ")) {
            let indent = (t.len() - s.len()).min(8);
            format!("{}• {i}", " ".repeat(indent))
        } else {
            t.to_string()
        };
        out.push(line.replace("**", "").replace('`', ""));
    }
    let s = out.join("\n");
    let s = s.trim();
    if s.is_empty() { "описания нет — подробности на странице релиза.".into() } else { s.to_string() }
}

impl App {
    /// Один раз после запуска (и в фоне, без окна): итог прошлого обновления, автопроверка.
    pub(crate) fn update_start(&mut self) {
        if self.upd.started {
            return;
        }
        self.upd.started = true;
        let keep = self.upd.prefs.known.as_ref().map(|r| r.version.clone());
        match updater::after_start(keep.as_deref()) {
            Some(AfterUpdate::Updated { from, to }) => {
                self.add_log(format!("Программа обновлена: {from} → {to}"));
                self.status = format!("обновлено до версии {to}");
            }
            Some(AfterUpdate::Failed { to, log }) => {
                self.add_log(format!("Ошибка: обновление до {to} не установилось. Журнал {}:\n{log}", updater::log_file().display()));
                self.upd.phase = Phase::Failed { rel: None, err: format!("версия {to} не установилась — подробности в журнале") };
            }
            None => {}
        }
        if self.upd.prefs.due(update::now()) {
            self.check_update(false);
        } else if let Some(r) = self.upd.prefs.pending(&self.upd.current).cloned() {
            if matches!(self.upd.phase, Phase::Idle) {
                self.upd.phase = Phase::Available(r);
            }
        }
    }

    /// Проверить в фоне. `manual` — кнопка в настройках: показываем и пропущенную версию, пишем итог.
    pub(crate) fn check_update(&mut self, manual: bool) {
        if self.upd.busy() {
            return;
        }
        self.upd.phase = Phase::Checking;
        self.upd.msg = if manual { "проверяю…".into() } else { String::new() };
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        let (current, platform, beta) = (self.upd.current.clone(), self.upd.platform, self.upd.prefs.beta);
        std::thread::spawn(move || {
            let result = update::check(&current, platform, beta);
            let _ = tx.send(Event::Update(UpdEvent::Checked { manual, result }));
            ctx.request_repaint();
        });
    }

    pub(crate) fn on_update_event(&mut self, ev: UpdEvent) {
        match ev {
            UpdEvent::Checked { manual, result } => match result {
                Ok(found) => {
                    let now = update::now();
                    self.upd.prefs.record(now, found.as_ref());
                    self.upd.prefs.save();
                    match found {
                        Some(r) if manual || !self.upd.prefs.skipped(&r) => {
                            self.upd.msg = format!("доступна версия {}", r.version);
                            self.upd.later = false;
                            if self.hidden && self.upd.toasted != r.version {
                                self.upd.toasted = r.version.clone();
                                self.toast_update(&r);
                            }
                            self.add_log(format!("Доступна новая версия: {} (сейчас {APP_VERSION})", r.version));
                            self.upd.phase = Phase::Available(r);
                        }
                        _ => {
                            self.upd.msg = if manual { "установлена последняя версия".into() } else { String::new() };
                            self.upd.phase = Phase::Idle;
                        }
                    }
                }
                Err(e) => {
                    self.upd.phase = Phase::Idle;
                    if manual {
                        self.upd.msg = format!("не удалось проверить: {e}");
                    } else {
                        self.add_log(format!("Проверка обновлений: {e}"));
                    }
                }
            },
            UpdEvent::Progress(done, total) => {
                if let Phase::Downloading { done: d, total: t, .. } = &mut self.upd.phase {
                    *d = done;
                    *t = total;
                }
            }
            UpdEvent::Downloaded(result) => {
                let rel = match &self.upd.phase {
                    Phase::Downloading { rel, .. } => rel.clone(),
                    _ => return, // отменили
                };
                match result {
                    Ok(ready) => {
                        self.add_log(format!("Обновление {} скачано, контрольная сумма совпала", rel.version));
                        self.upd.phase = Phase::Ready { rel, ready };
                        if !(self.upd.after_downloads && self.q.active()) {
                            self.install_update();
                        }
                    }
                    Err(e) if e == "отменено" => self.upd.phase = Phase::Available(rel),
                    Err(e) => {
                        self.add_log(format!("Ошибка: обновление {}: {e}", rel.version));
                        self.upd.phase = Phase::Failed { rel: Some(rel), err: e };
                    }
                }
            }
        }
    }

    /// Уведомление Windows «доступна версия»: щелчок открывает окно (если есть протокол vgd://)
    /// или страницу релиза.
    fn toast_update(&self, r: &Release) {
        let exe = std::env::current_exe().unwrap_or_default();
        let uri = if ipc::protocol_registered(&exe) { "vgd://show".to_string() } else { r.page_url.clone() };
        let uri = if uri.is_empty() { update::releases_page() } else { uri };
        vgd::appinfo::toast_uri(
            &format!("Доступна версия {}", r.version),
            &format!("{APP_NAME} {APP_VERSION} → {}. Откройте окно программы, чтобы обновить.", r.version),
            &uri,
        );
    }

    /// Кнопка «обновить»: идут загрузки — спросить; нет — скачать и поставить.
    fn begin_update(&mut self, rel: Release) {
        if self.q.active() {
            self.upd.ask = true;
            self.upd.phase = Phase::Available(rel);
            return;
        }
        self.upd.after_downloads = false;
        self.download_update(rel);
    }

    fn download_update(&mut self, rel: Release) {
        if let Phase::Ready { rel: r, .. } = &self.upd.phase {
            if r.version == rel.version {
                if !(self.upd.after_downloads && self.q.active()) {
                    self.install_update();
                }
                return;
            }
        }
        self.upd.cancel = Arc::new(AtomicBool::new(false));
        self.upd.phase = Phase::Downloading { rel: rel.clone(), done: 0, total: rel.size };
        let (tx, ctx, cancel, platform) = (self.tx.clone(), self.ctx.clone(), self.upd.cancel.clone(), self.upd.platform);
        self.add_log(format!("Скачиваю обновление {}: {}", rel.version, rel.asset_name));
        std::thread::spawn(move || {
            let mut last = std::time::Instant::now();
            let ptx = tx.clone();
            let pctx = ctx.clone();
            let r = update::download(&rel, &update::dir(), &mut |done, total| {
                if last.elapsed().as_millis() >= 150 || done == total {
                    last = std::time::Instant::now();
                    let _ = ptx.send(Event::Update(UpdEvent::Progress(done, total)));
                    pctx.request_repaint();
                }
                !cancel.load(Ordering::Relaxed)
            })
            .and_then(|file| updater::prepare(platform, &file, &rel.version));
            let _ = tx.send(Event::Update(UpdEvent::Downloaded(r)));
            ctx.request_repaint();
        });
    }

    /// Запустить установку и выйти. Вызывается, когда обновление скачано и загрузок нет
    /// (или пользователь сказал «сейчас»).
    fn install_update(&mut self) {
        let Phase::Ready { rel, ready } = self.upd.phase.clone() else { return };
        match updater::launch(&ready, &rel.version, self.hidden) {
            Ok(()) => {
                self.add_log(format!("Устанавливаю обновление {} — программа перезапустится", rel.version));
                self.upd.phase = Phase::Installing(rel);
                self.exiting = true;
                self.q.stop_all();
                self.save_settings();
                self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(e) => {
                self.add_log(format!("Ошибка: {e}"));
                self.upd.phase = Phase::Failed { rel: Some(rel), err: e };
            }
        }
    }

    /// Каждый кадр (и когда окно в трее): «обновить после загрузок» — загрузки кончились, ставим.
    pub(crate) fn update_tick(&mut self) {
        if self.upd.after_downloads && matches!(self.upd.phase, Phase::Ready { .. }) && !self.q.active() {
            self.upd.after_downloads = false;
            self.install_update();
        }
        if matches!(self.upd.phase, Phase::Downloading { .. } | Phase::Ready { .. }) {
            self.ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
    }

    /// Плашка вверху главного экрана.
    pub(crate) fn ui_update_banner(&mut self, ui: &mut egui::Ui) {
        let phase = self.upd.phase.clone();
        let show = match &phase {
            Phase::Available(_) => !self.upd.later,
            Phase::Downloading { .. } | Phase::Ready { .. } | Phase::Installing(_) => true,
            Phase::Failed { .. } => !self.upd.later,
            _ => false,
        };
        if !show {
            return;
        }
        let c = pal();
        egui::Frame::new()
            .fill(c.surface)
            .stroke(Stroke::new(1.0, c.accent.gamma_multiply(0.6)))
            .corner_radius(rad(10))
            .inner_margin(Margin::symmetric(12, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                match phase {
                    Phase::Available(rel) => {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(format!("доступна версия {}", rel.version)).size(12.5).color(c.text));
                            ui.label(RichText::new("·").color(c.faint));
                            if link(ui, if self.upd.notes_open { "скрыть" } else { "что нового" }).clicked() {
                                self.upd.notes_open = !self.upd.notes_open;
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if icon_button(ui, I_CLOSE, "позже").clicked() {
                                    self.upd.later = true;
                                    self.upd.notes_open = false;
                                }
                                if small_pill(ui, "позже").clicked() {
                                    self.upd.later = true;
                                    self.upd.notes_open = false;
                                }
                                if small_pill(ui, "обновить").on_hover_text(format!("{} · {}", rel.asset_name, mb(rel.size))).clicked() {
                                    self.begin_update(rel.clone());
                                }
                            });
                        });
                        if self.upd.notes_open {
                            ui.add_space(4.0);
                            egui::ScrollArea::vertical().id_salt("upd_notes").max_height(180.0).show(ui, |ui| {
                                ui.add(egui::Label::new(RichText::new(plain_notes(&rel.notes)).size(12.0).color(c.muted)).wrap());
                            });
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                if !rel.page_url.is_empty() && link(ui, "страница релиза").clicked() {
                                    vgd::appinfo::open_url(&rel.page_url);
                                }
                                ui.label(RichText::new("·").color(c.faint));
                                if link(ui, "пропустить эту версию").clicked() {
                                    self.upd.prefs.skip = rel.version.clone();
                                    self.upd.prefs.save();
                                    self.upd.phase = Phase::Idle;
                                    self.upd.notes_open = false;
                                }
                            });
                        }
                    }
                    Phase::Downloading { rel, done, total } => {
                        ui.horizontal(|ui| {
                            let pct = if total > 0 { done as f64 * 100.0 / total as f64 } else { 0.0 };
                            ui.label(
                                RichText::new(format!("скачиваю версию {} — {pct:.0}% ({} из {})", rel.version, mb(done), mb(total)))
                                    .size(12.5)
                                    .color(c.text),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if small_pill(ui, "отменить").clicked() {
                                    self.upd.cancel.store(true, Ordering::Relaxed);
                                    self.upd.after_downloads = false;
                                    self.upd.phase = Phase::Available(rel.clone());
                                }
                            });
                        });
                        progress_line(ui, if total > 0 { done as f32 / total as f32 } else { 0.0 });
                    }
                    Phase::Ready { rel, .. } => {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new(format!("версия {} скачана — установится, когда закончатся загрузки", rel.version))
                                    .size(12.5)
                                    .color(c.text),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if small_pill(ui, "отменить").clicked() {
                                    self.upd.after_downloads = false;
                                    self.upd.phase = Phase::Available(rel.clone());
                                }
                                if small_pill(ui, "обновить сейчас").on_hover_text("загрузки прервутся и докачаются после перезапуска").clicked() {
                                    self.upd.after_downloads = false;
                                    self.install_update();
                                }
                            });
                        });
                    }
                    Phase::Installing(rel) => {
                        ui.label(RichText::new(format!("устанавливаю версию {} — программа перезапустится…", rel.version)).size(12.5).color(c.text));
                    }
                    Phase::Failed { rel, err } => {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(format!("не удалось обновить: {err}")).size(12.5).color(c.err));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if icon_button(ui, I_CLOSE, "скрыть").clicked() {
                                    self.upd.later = true;
                                }
                                if let Some(rel) = rel {
                                    if small_pill(ui, "повторить").clicked() {
                                        self.upd.phase = Phase::Available(rel.clone());
                                        self.begin_update(rel);
                                    }
                                }
                            });
                        });
                    }
                    _ => {}
                }
            });
        ui.add_space(6.0);
    }

    /// Окно «идут загрузки»: обновить сейчас или после них.
    pub(crate) fn ui_update_ask(&mut self, ctx: &egui::Context) {
        if !self.upd.ask {
            return;
        }
        let Phase::Available(rel) = self.upd.phase.clone() else {
            self.upd.ask = false;
            return;
        };
        egui::Modal::new(egui::Id::unique("update_ask")).show(ctx, |ui| {
            ui.set_width(360.0);
            ui.label(heading(&format!("обновление до {}", rel.version), 18.0));
            ui.add_space(4.0);
            ui.add(
                egui::Label::new(
                    RichText::new("сейчас идут загрузки. обновление скачается сразу, а установится, когда они закончатся. можно и сейчас — прерванное докачается после перезапуска.")
                        .size(12.5)
                        .color(pal().muted),
                )
                .wrap(),
            );
            ui.add_space(14.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if pill(ui, None, "после загрузок", true, true).clicked() {
                    self.upd.ask = false;
                    self.upd.after_downloads = true;
                    self.download_update(rel.clone());
                }
                if pill(ui, None, "сейчас", false, true).clicked() {
                    self.upd.ask = false;
                    self.upd.after_downloads = false;
                    self.download_update(rel.clone());
                }
                if pill(ui, None, "отмена", false, true).clicked() {
                    self.upd.ask = false;
                }
            });
        });
    }

    /// Строки в настройках → «доступ и обновления» (внутри карточки).
    pub(crate) fn ui_update_rows(&mut self, ui: &mut egui::Ui) {
        let title = format!("{APP_NAME} {APP_VERSION}");
        let kind = match self.upd.platform {
            Platform::WindowsSetup => "установленная версия: обновление ставится установщиком, тихо, без вопросов.",
            _ => "версия без установки (portable): новые файлы распаковываются и заменяют старые после выхода программы.",
        };
        let tip = format!(
            "{kind} программа скачивает обновление с github ({}), проверяет контрольную сумму sha-256 и перезапускается. настройки и очередь сохраняются.",
            update::REPO
        );
        divider(ui);
        row(ui, &title, &tip, |ui| {
            let busy = self.upd.busy();
            let label = if matches!(self.upd.phase, Phase::Checking) { "проверяю…" } else { "проверить обновления" };
            if small_pill_enabled(ui, label, !busy).clicked() {
                self.upd.later = false;
                self.check_update(true);
            }
        });
        if !self.upd.msg.is_empty() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&self.upd.msg).size(11.5).color(pal().muted));
                if let Phase::Available(rel) = self.upd.phase.clone() {
                    if link(ui, "обновить").clicked() {
                        self.upd.later = false;
                        self.begin_update(rel);
                    }
                }
            });
            ui.add_space(4.0);
        }
        divider(ui);
        row(ui, "проверять обновления автоматически", "при запуске программы, не чаще раза в 6 часов. найдётся новая версия — на главной появится плашка, а если окно спрятано в трей — уведомление windows.", |ui| {
            if toggle(ui, &mut self.upd.prefs.auto) {
                self.upd.prefs.save();
            }
        });
        divider(ui);
        row(ui, "бета-версии", "предлагать и предварительные версии (pre-release на github): новое раньше, но возможны ошибки.", |ui| {
            if toggle(ui, &mut self.upd.prefs.beta) {
                // другой набор версий — следующая проверка не ждёт 6 часов
                self.upd.prefs.last_check = 0;
                self.upd.prefs.known = None;
                if !self.upd.prefs.beta {
                    if let Phase::Available(r) = &self.upd.phase {
                        if r.prerelease || r.version().is_some_and(|v| v.is_pre()) {
                            self.upd.phase = Phase::Idle;
                            self.upd.msg.clear();
                        }
                    }
                }
                self.upd.prefs.save();
            }
        });
    }
}

/// Тонкая полоска прогресса.
fn progress_line(ui: &mut egui::Ui, frac: f32) {
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 4.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 2, pal().track);
    let mut done = rect;
    done.set_width(rect.width() * frac.clamp(0.0, 1.0));
    p.rect_filled(done, 2, pal().accent);
    ui.add_space(2.0);
}

#[cfg(test)]
mod tests {
    use super::plain_notes;

    #[test]
    fn notes_to_plain_text() {
        let md = "## Что нового\r\n- **быстрее** загрузка\n  * вложенный `пункт`\n\nТекст";
        assert_eq!(plain_notes(&md.replace("\r\n", "\n")), "Что нового\n• быстрее загрузка\n  • вложенный пункт\n\nТекст");
        assert!(plain_notes("  ").contains("описания нет"));
    }
}
