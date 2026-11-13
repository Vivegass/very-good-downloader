//! Настройки → «оформление»: выбор темы и редактор своих тем (файлы в папке themes).

use super::*;
use vgd::theme::{free_stem_in, read_from, slug, write_to, FILE_PREFIX, FONTS, ICON_SETS};

/// Состояние редактора тем.
#[derive(Default)]
pub(crate) struct ThemeUi {
    /// свои темы из папки (имя файла, тема); None — перечитать
    customs: Option<Vec<(String, Theme)>>,
    /// поле «название» (применяется кнопкой «переименовать»)
    name_edit: String,
    /// для какой темы заполнено name_edit
    name_for: String,
    /// тему меняли — записать в файл, когда отпустят мышь
    dirty: bool,
    /// «удалить» нажато один раз — ждём подтверждения
    confirm_delete: bool,
    msg: String,
}

impl App {
    /// Сменить тему (ключ как в Settings::theme) и сразу применить.
    fn select_theme(&mut self, ctx: &egui::Context, key: &str, t: Theme) {
        self.flush_theme();
        self.theme = t;
        self.s.theme = key.to_string();
        self.themes.confirm_delete = false;
        apply_theme(ctx, &self.theme);
    }

    fn custom_stem(&self) -> Option<String> {
        self.s.theme.strip_prefix(FILE_PREFIX).map(String::from)
    }

    /// Записать изменённую свою тему в файл.
    fn flush_theme(&mut self) {
        if !self.themes.dirty {
            return;
        }
        self.themes.dirty = false;
        if let Some(stem) = self.custom_stem() {
            if let Err(e) = write_to(&theme::dir().join(format!("{stem}.json")), &self.theme) {
                self.themes.msg = e;
            }
            self.themes.customs = None;
        }
    }

    /// Новая своя тема в папке (имя файла подбирается свободное) — и выбрать её.
    fn add_custom_theme(&mut self, ctx: &egui::Context, mut t: Theme) {
        t.sanitize();
        let dir = theme::dir();
        let stem = free_stem_in(&dir, &t.name);
        match write_to(&dir.join(format!("{stem}.json")), &t) {
            Ok(()) => {
                self.themes.customs = None;
                self.select_theme(ctx, &format!("{FILE_PREFIX}{stem}"), t);
            }
            Err(e) => self.themes.msg = e,
        }
    }

    pub(crate) fn ui_appearance(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.themes.dirty && !ctx.input(|i| i.pointer.any_down()) {
            self.flush_theme();
        }
        if self.themes.customs.is_none() {
            self.themes.customs = Some(theme::list_custom());
        }
        let custom = self.custom_stem();
        if self.themes.name_for != self.s.theme {
            self.themes.name_for = self.s.theme.clone();
            self.themes.name_edit = self.theme.name.clone();
        }

        card(ui, |ui| {
            row(ui, "тема", "встроенные темы не меняются. своя тема — файл в папке тем: им можно поделиться (кнопка «экспорт») или взять чужой («импорт»).", |ui| {
                let mut pick: Option<(String, Theme)> = None;
                let r = egui::ComboBox::from_id_salt("theme_pick")
                    .selected_text(RichText::new(&self.theme.name).size(12.5))
                    .width(190.0)
                    .show_ui(ui, |ui| {
                        for (key, t) in theme::builtins() {
                            if ui.selectable_label(self.s.theme == key, &t.name).clicked() {
                                pick = Some((key.to_string(), t));
                            }
                        }
                        let customs = self.themes.customs.clone().unwrap_or_default();
                        if !customs.is_empty() {
                            ui.separator();
                        }
                        for (stem, t) in customs {
                            let key = format!("{FILE_PREFIX}{stem}");
                            if ui.selectable_label(self.s.theme == key, &t.name).on_hover_text(format!("{stem}.json")).clicked() {
                                pick = Some((key, t));
                            }
                        }
                    });
                if r.response.clicked() {
                    // папку могли поменять руками — перечитать к следующему кадру
                    self.themes.customs = None;
                }
                if let Some((key, t)) = pick {
                    self.select_theme(&ctx, &key, t);
                }
            });
            divider(ui);
            row(ui, "свои темы", "«создать» — копия текущей темы: в ней можно менять цвета, скругление, шрифт и значки. раскладка (навигация сверху/снизу, вид карточек и кнопок, фон-градиент, размер шрифта) задаётся в файле темы — в папке тем лежит пример со всеми полями и пояснениями. «импорт» — добавить тему из файла .json.", |ui| {
                if small_pill(ui, "папка тем").clicked() {
                    let dir = theme::dir();
                    theme::ensure_example(&dir);
                    open_path(&dir);
                    self.themes.customs = None;
                }
                if small_pill(ui, "импорт…").clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("тема", &["json"]).pick_file() {
                        match read_from(&p) {
                            Ok(t) => {
                                self.themes.msg = format!("тема «{}» добавлена", t.name);
                                self.add_custom_theme(&ctx, t);
                            }
                            Err(e) => self.themes.msg = e,
                        }
                    }
                }
                if small_pill(ui, "создать").clicked() {
                    let mut t = self.theme.clone();
                    t.name = if custom.is_some() { format!("{} копия", t.name) } else { format!("моя {}", t.name) };
                    self.themes.msg.clear();
                    self.add_custom_theme(&ctx, t);
                }
            });
        });

        // Образец: всё окно и так меняется сразу, а здесь — все виды кнопок рядом.
        ui.add_space(10.0);
        ui.label(RichText::new("пример").size(12.0).color(pal().muted));
        ui.add_space(4.0);
        card(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                pill(ui, Some(I_DOWNLOAD), "главная", true, true);
                pill(ui, Some(I_PASTE), "обычная", false, true);
                small_pill(ui, "маленькая");
                let mut on = true;
                toggle(ui, &mut on);
                let mut off = false;
                toggle(ui, &mut off);
                ui.label(RichText::new("готово").size(12.0).color(pal().ok));
                ui.label(RichText::new("ошибка").size(12.0).color(pal().err));
                ui.label(RichText::new("внимание").size(12.0).color(pal().warn));
                ui.label(RichText::new("приглушённый").size(12.0).color(pal().muted));
                ui.label(RichText::new("бледный").size(12.0).color(pal().faint));
            });
            ui.add_space(6.0);
        });

        let Some(stem) = custom else {
            if !self.themes.msg.is_empty() {
                hint(ui, &self.themes.msg.clone());
            }
            return;
        };

        ui.add_space(6.0);
        let mut changed = false;
        card(ui, |ui| {
            row(ui, "название", "", |ui| {
                if small_pill(ui, "сохранить").clicked() {
                    self.rename_theme(&stem);
                }
                ui.add(egui::TextEdit::singleline(&mut self.themes.name_edit).desired_width(180.0));
            });
            divider(ui);
            row(ui, "скругление углов", "0 — квадратные углы, 1 — как в тёмной теме, 3 — совсем круглые.", |ui| {
                // справа налево: число, левее — ползунок
                ui.label(RichText::new(format!("{:.1}", self.theme.radius)).size(12.5).color(pal().text));
                if slider(ui, &mut self.theme.radius, 0.0..=3.0, 0.1) {
                    changed = true;
                }
            });
            divider(ui);
            row(ui, "шрифт", "", |ui| {
                let opts: Vec<(String, String)> = FONTS.iter().map(|(k, l)| (k.to_string(), l.to_string())).collect();
                if segmented(ui, &mut self.theme.font, &opts) {
                    changed = true;
                }
            });
            divider(ui);
            row(ui, "значки", "«тонкие» — значки windows (segoe mdl2), «символы» — обычные знаки ↓ ⚙ ☰ ✕.", |ui| {
                let opts: Vec<(String, String)> = ICON_SETS.iter().map(|(k, l)| (k.to_string(), l.to_string())).collect();
                if segmented(ui, &mut self.theme.icons, &opts) {
                    changed = true;
                }
            });
            divider(ui);
            ui.add_space(8.0);
            ui.label(RichText::new("цвета — щелчок по квадратику открывает пипетку").size(12.0).color(pal().muted));
            ui.add_space(6.0);
            egui::Grid::new("theme_colors").num_columns(2).spacing([18.0, 6.0]).show(ui, |ui| {
                for (i, (label, c)) in self.theme.colors_mut().into_iter().enumerate() {
                    ui.horizontal(|ui| {
                        let r = egui::color_picker::color_edit_button_srgb(ui, &mut c.0).on_hover_text(c.to_hex());
                        if r.changed() {
                            changed = true;
                        }
                        ui.label(RichText::new(label).size(12.0).color(pal().text));
                    });
                    if i % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
            ui.add_space(6.0);
            divider(ui);
            row(ui, "файл темы", "одна тема — один файл .json: его можно отправить другу, а он добавит его кнопкой «импорт».", |ui| {
                let del = if self.themes.confirm_delete { "точно удалить?" } else { "удалить" };
                if small_pill(ui, del).clicked() {
                    if self.themes.confirm_delete {
                        self.delete_theme(&ctx, &stem);
                    } else {
                        self.themes.confirm_delete = true;
                    }
                }
                if small_pill(ui, "экспорт…").clicked() {
                    self.flush_theme();
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("тема", &["json"])
                        .set_file_name(format!("{}.json", slug(&self.theme.name)))
                        .save_file()
                    {
                        self.themes.msg = match write_to(&p, &self.theme) {
                            Ok(()) => format!("сохранено: {}", p.display()),
                            Err(e) => e,
                        };
                    }
                }
            });
        });
        if !self.themes.msg.is_empty() {
            hint(ui, &self.themes.msg.clone());
        }
        if changed {
            self.theme.sanitize();
            self.themes.dirty = true;
            apply_theme(&ctx, &self.theme);
        }
    }

    fn rename_theme(&mut self, stem: &str) {
        let name = self.themes.name_edit.trim().to_string();
        if name.is_empty() {
            return;
        }
        self.theme.name = name.clone();
        let dir = theme::dir();
        let old = dir.join(format!("{stem}.json"));
        // Имя файла — по названию, если такое свободно.
        let new_stem = if slug(&name) == stem { stem.to_string() } else { free_stem_in(&dir, &name) };
        let new = dir.join(format!("{new_stem}.json"));
        match write_to(&new, &self.theme) {
            Ok(()) => {
                if new != old {
                    let _ = std::fs::remove_file(&old);
                }
                self.s.theme = format!("{FILE_PREFIX}{new_stem}");
                self.themes.name_for = self.s.theme.clone();
                self.themes.dirty = false;
                self.themes.customs = None;
                self.themes.msg.clear();
            }
            Err(e) => self.themes.msg = e,
        }
    }

    fn delete_theme(&mut self, ctx: &egui::Context, stem: &str) {
        self.themes.dirty = false;
        let path = theme::dir().join(format!("{stem}.json"));
        match std::fs::remove_file(&path) {
            Ok(()) => {
                self.themes.msg = format!("тема «{}» удалена", self.theme.name);
                self.themes.customs = None;
                self.select_theme(ctx, "", theme::dark());
            }
            Err(e) => self.themes.msg = format!("не удалить {}: {e}", path.display()),
        }
    }
}

/// Горизонтальный ползунок: полоса, заполненная часть цветом акцента и кружок; тянуть или щёлкнуть.
fn slider(ui: &mut egui::Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, step: f32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(170.0, 22.0), Sense::click_and_drag());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let (lo, hi) = (*range.start(), *range.end());
    let track = rect.shrink2(Vec2::new(8.0, 0.0));
    let mut changed = false;
    if let Some(pos) = resp.interact_pointer_pos() {
        if resp.dragged() || resp.clicked() {
            let t = ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0);
            let v = ((lo + t * (hi - lo)) / step).round() * step;
            if (v - *value).abs() > f32::EPSILON {
                *value = v.clamp(lo, hi);
                changed = true;
            }
        }
    }
    let t = ((*value - lo) / (hi - lo)).clamp(0.0, 1.0);
    let x = egui::lerp(track.left()..=track.right(), t);
    let y = rect.center().y;
    let p = ui.painter();
    let bar = egui::Rect::from_min_max(egui::pos2(track.left(), y - 2.0), egui::pos2(track.right(), y + 2.0));
    p.rect_filled(bar, 2, pal().track);
    p.rect_filled(egui::Rect::from_min_max(bar.min, egui::pos2(x, bar.max.y)), 2, pal().accent);
    let hot = resp.hovered() || resp.dragged();
    p.circle_filled(egui::pos2(x, y), if hot { 8.0 } else { 7.0 }, pal().accent);
    p.circle_filled(egui::pos2(x, y), 3.0, pal().on_accent);
    changed
}
