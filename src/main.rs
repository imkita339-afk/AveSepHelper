#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod dialogs;
mod effects;
use eframe::egui::{self, Color32, FontId, RichText, Stroke, Vec2};
use sephiria_workshop_prep::{self as core, Report};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};

const BG: Color32 = Color32::from_rgb(244, 246, 248);
const INK: Color32 = Color32::from_rgb(28, 44, 57);
const MUTED: Color32 = Color32::from_rgb(103, 119, 133);
const LINE: Color32 = Color32::from_rgb(223, 230, 235);
const TEAL: Color32 = Color32::from_rgb(9, 123, 106);
const PALE: Color32 = Color32::from_rgb(231, 245, 239);
const NAV: Color32 = Color32::from_rgb(22, 41, 51);
const RED: Color32 = Color32::from_rgb(173, 57, 57);
const STEPS: [(&str, &str); 3] = [
    ("Файлы мода", "Выберите папку с модом"),
    ("Оформление", "Название, описание и обложка"),
    ("Подготовка", "Проверка и готовая папка"),
];
const MODULES: [(&str, &str, &str); 4] = [
    (
        "localization",
        "Перевод игры",
        "Папка с языковыми JSON-файлами",
    ),
    ("costume", "Костюмы", "Папка с файлами костюмов"),
    ("dll", "Мод с кодом", "Собранная DLL и класс точки входа"),
    ("data", "Пакет данных", "Папка с данными для других модов"),
];
const TAGS: [(&str, &str); 9] = [
    ("Item", "Предметы"),
    ("Costume", "Костюмы"),
    ("Weapon", "Оружие"),
    ("NPC", "Персонажи"),
    ("Enemy", "Враги"),
    ("Gameplay", "Геймплей"),
    ("QoL", "Удобство"),
    ("Library", "Библиотека"),
    ("Localization", "Перевод"),
];

enum Job {
    Discovered(core::Result<core::discovery::Discovery>),
    Checked(Report),
    Built(core::Result<core::Built>),
}
struct App {
    found: Option<core::discovery::Discovery>,
    module_selection: Vec<bool>,
    library_selection: Vec<bool>,
    fx: effects::Effects,
    rescan_meta: Option<Value>,
    rescan_cover: Option<PathBuf>,
    step: usize,
    source: String,
    output: String,
    create_workshop: bool,
    loaded: bool,
    meta: Value,
    cover: Option<PathBuf>,
    texture: Option<egui::TextureHandle>,
    report: Option<Report>,
    result: Option<PathBuf>,
    backup: Option<PathBuf>,
    notice: Option<String>,
    raw: String,
    raw_open: bool,
    modules_open: bool,
    receiver: Option<mpsc::Receiver<Job>>,
    busy: &'static str,
    screenshot: Option<PathBuf>,
    screenshot_requested: bool,
    frames: usize,
}
impl Default for App {
    fn default() -> Self {
        Self {
            found: None,
            module_selection: Vec::new(),
            library_selection: Vec::new(),
            fx: effects::Effects::default(),
            rescan_meta: None,
            rescan_cover: None,
            step: 0,
            source: String::new(),
            output: String::new(),
            create_workshop: false,
            loaded: false,
            meta: core::template(),
            cover: None,
            texture: None,
            report: None,
            result: None,
            backup: None,
            notice: None,
            raw: String::new(),
            raw_open: false,
            modules_open: false,
            receiver: None,
            busy: "",
            screenshot: None,
            screenshot_requested: false,
            frames: 0,
        }
    }
}
fn card(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0_f32, LINE))
        .corner_radius(14)
        .inner_margin(22.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            content(ui);
        });
    ui.add_space(16.0);
}
fn hint(ui: &mut egui::Ui, s: impl Into<String>) {
    ui.add(egui::Label::new(RichText::new(s.into()).size(13.0).color(MUTED)).wrap());
}
fn title(ui: &mut egui::Ui, s: &str) {
    ui.label(RichText::new(s).size(18.0).strong().color(INK));
    ui.add_space(10.0);
}
fn action(ui: &mut egui::Ui, s: &str, primary: bool) -> egui::Response {
    let response = ui.add(
        egui::Button::new(RichText::new(s).size(15.0).color(if primary {
            Color32::WHITE
        } else {
            INK
        }))
        .min_size(Vec2::new(0.0, 44.0))
        .corner_radius(9)
        .fill(if primary { TEAL } else { Color32::WHITE })
        .stroke(Stroke::new(1.0_f32, if primary { TEAL } else { LINE })),
    );
    effects::button_glow(&response);
    response
}
fn edit(ui: &mut egui::Ui, label: &str, v: &mut Value, placeholder: &str) -> bool {
    ui.label(RichText::new(label).strong().size(14.0));
    let mut text = v.as_str().unwrap_or("").to_owned();
    let changed = ui
        .add(
            egui::TextEdit::singleline(&mut text)
                .hint_text(placeholder)
                .desired_width(f32::INFINITY)
                .margin(Vec2::new(11.0, 10.0)),
        )
        .changed();
    if changed {
        *v = json!(text);
    }
    ui.add_space(12.0);
    changed
}
fn folder_icon(ui: &mut egui::Ui, size: f32) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, 16, PALE);
    let body = egui::Rect::from_min_size(
        r.min + Vec2::new(size * 0.22, size * 0.34),
        Vec2::new(size * 0.56, size * 0.37),
    );
    p.rect_filled(body, 4, TEAL);
    p.rect_filled(
        egui::Rect::from_min_size(
            body.min - Vec2::new(0.0, size * 0.08),
            Vec2::new(size * 0.25, size * 0.13),
        ),
        3,
        TEAL,
    );
}
fn open_folder(path: &Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer.exe").arg(path).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}
fn style(ctx: &egui::Context) {
    let mut s = egui::Style {
        visuals: egui::Visuals::light(),
        ..Default::default()
    };
    s.visuals.override_text_color = Some(INK);
    s.visuals.panel_fill = BG;
    s.visuals.window_fill = Color32::WHITE;
    s.visuals.faint_bg_color = BG;
    s.visuals.extreme_bg_color = Color32::from_rgb(249, 250, 252);
    s.visuals.selection.bg_fill = Color32::from_rgb(202, 231, 222);
    s.visuals.selection.stroke = Stroke::new(1.0_f32, TEAL);
    for w in [
        &mut s.visuals.widgets.inactive,
        &mut s.visuals.widgets.hovered,
        &mut s.visuals.widgets.active,
    ] {
        w.corner_radius = 7.into();
        w.bg_stroke = Stroke::new(1.0_f32, LINE);
    }
    s.visuals.widgets.inactive.bg_fill = Color32::WHITE;
    s.visuals.widgets.inactive.weak_bg_fill = Color32::WHITE;
    s.visuals.widgets.hovered.bg_fill = PALE;
    s.visuals.widgets.hovered.weak_bg_fill = PALE;
    s.visuals.widgets.active.bg_fill = Color32::from_rgb(207, 234, 223);
    s.spacing.item_spacing = Vec2::new(10.0, 8.0);
    s.spacing.button_padding = Vec2::new(16.0, 9.0);
    s.spacing.interact_size.y = 32.0;
    s.text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(15.0));
    s.text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(14.0));
    s.text_styles
        .insert(egui::TextStyle::Heading, FontId::proportional(28.0));
    s.text_styles
        .insert(egui::TextStyle::Small, FontId::proportional(12.0));
    ctx.set_style(s);
}

impl App {
    fn invalidate(&mut self) {
        self.report = None;
        self.result = None;
        self.notice = None;
    }
    fn load(&mut self, ctx: &egui::Context) {
        self.rescan_meta = None;
        self.rescan_cover = None;
        self.invalidate();
        self.loaded = false;
        self.cover = None;
        self.texture = None;
        self.found = None;
        self.module_selection.clear();
        self.library_selection.clear();
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.busy = "Ищем файлы внутри папки…";
        let (source, ctx) = (self.source.clone(), ctx.clone());
        std::thread::spawn(move || {
            let result = core::discovery::discover(Path::new(&source));
            let _ = tx.send(Job::Discovered(result));
            ctx.request_repaint();
        });
    }
    fn finish_discovery(&mut self, mut found: core::discovery::Discovery, ctx: &egui::Context) {
        if let Some(meta) = self.rescan_meta.take() {
            found.metadata = Some(meta);
            self.cover = self.rescan_cover.take();
        }
        if found.metadata.is_none() && found.packages.len() > 1 {
            self.found = Some(found);
            return;
        }
        self.source = found.root.display().to_string();
        let fresh = found.metadata.is_none();
        self.meta = found.metadata.clone().unwrap_or_else(core::template);
        if fresh {
            let hash = self
                .source
                .to_lowercase()
                .bytes()
                .fold(0xcbf29ce484222325_u64, |h, b| {
                    (h ^ b as u64).wrapping_mul(0x100000001b3)
                });
            self.meta["mod"]["modId"] = json!(format!("mod.{hash:016x}"));
        }
        let auto = !self.meta["mod"]["modules"]
            .as_array()
            .is_some_and(|a| !a.is_empty());
        self.module_selection = found
            .modules
            .iter_mut()
            .map(|m| {
                let existing = self.meta["mod"]["modules"].as_array().and_then(|a| {
                    a.iter()
                        .find(|v| v["type"] == m.module["type"] && v["path"] == m.module["path"])
                });
                if let Some(v) = existing {
                    if let Some(class) = v.get("entryClass") {
                        m.module["entryClass"] = class.clone();
                    }
                }
                existing.is_some() || auto && m.automatic
            })
            .collect();
        self.library_selection = found
            .libraries
            .iter()
            .map(|p| {
                (fresh
                    && found
                        .libraries
                        .iter()
                        .filter(|other| Path::new(other).file_name() == Path::new(p).file_name())
                        .count()
                        == 1)
                    || self.meta["mod"]["libraries"].as_array().is_some_and(|a| {
                        a.iter()
                            .any(|v| v.as_str() == Some(p) || v["path"].as_str() == Some(p))
                    })
            })
            .collect();
        if self.meta["info"].get("preview").is_none() {
            if let Some(cover) = found.covers.iter().find(|p| {
                Path::new(p).file_stem().is_some_and(|n| {
                    ["thumbnail", "preview", "cover", "banner"]
                        .iter()
                        .any(|s| n.eq_ignore_ascii_case(s))
                })
            }) {
                self.meta["info"]["preview"] = json!(cover);
            }
        }
        self.found = Some(found);
        self.apply_found();
        self.loaded = true;
        self.modules_open = !self.meta["mod"]["modules"]
            .as_array()
            .is_some_and(|a| !a.is_empty());
        self.fx.transition();
        self.refresh_cover(ctx);
    }
    fn apply_found(&mut self) {
        let Some(found) = &self.found else {
            return;
        };
        let previous = self.meta["mod"]["modules"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut modules: Vec<Value> = previous
            .iter()
            .filter(|v| {
                !found
                    .modules
                    .iter()
                    .any(|m| v["type"] == m.module["type"] && v["path"] == m.module["path"])
            })
            .cloned()
            .collect();
        for (i, m) in found.modules.iter().enumerate() {
            if self.module_selection[i] {
                let mut v = previous
                    .iter()
                    .find(|v| v["type"] == m.module["type"] && v["path"] == m.module["path"])
                    .cloned()
                    .unwrap_or_else(|| m.module.clone());
                if let (Some(dst), Some(src)) = (v.as_object_mut(), m.module.as_object()) {
                    dst.extend(src.clone());
                }
                modules.push(v);
            }
        }
        self.meta["mod"]["modules"] = json!(modules);
        let mut libraries = self.meta["mod"]["libraries"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        libraries.retain(|v| {
            !found
                .libraries
                .iter()
                .any(|p| v.as_str() == Some(p) || v["path"].as_str() == Some(p))
        });
        for (i, p) in found.libraries.iter().enumerate() {
            if self.library_selection[i] {
                libraries.push(json!(p));
            }
        }
        if !libraries.is_empty() || self.meta["mod"].get("libraries").is_some() {
            self.meta["mod"]["libraries"] = json!(libraries);
        }
        self.report = None;
        self.result = None;
    }
    fn refresh_cover(&mut self, ctx: &egui::Context) {
        self.texture = None;
        let path = self.cover.clone().or_else(|| {
            let root = Path::new(&self.source);
            if let Some(p) = self.meta["info"]["preview"].as_str() {
                core::exact_path(root, p).ok()
            } else {
                [
                    "thumbnail.png",
                    "preview.png",
                    "thumbnail.jpg",
                    "preview.jpg",
                ]
                .iter()
                .find_map(|p| core::exact_path(root, p).ok())
            }
        });
        let Some(path) = path else {
            return;
        };
        if std::fs::metadata(&path).map_or(true, |m| m.len() > 64 * 1024 * 1024) {
            return;
        }
        if let Ok(reader) = image::ImageReader::open(path) {
            if let Ok(mut reader) = reader.with_guessed_format() {
                let mut limits = image::Limits::default();
                limits.max_alloc = Some(128 * 1024 * 1024);
                limits.max_image_width = Some(16384);
                limits.max_image_height = Some(16384);
                reader.limits(limits);
                if let Ok(img) = reader.decode() {
                    let img = img.thumbnail(640, 360).to_rgba8();
                    self.texture = Some(ctx.load_texture(
                        "mod-cover",
                        egui::ColorImage::from_rgba_unmultiplied(
                            [img.width() as usize, img.height() as usize],
                            img.as_raw(),
                        ),
                        Default::default(),
                    ));
                }
            }
        }
    }
    fn check(&mut self, ctx: &egui::Context) {
        self.report = None;
        self.notice = None;
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.busy = "Проверяем файлы мода…";
        let (source, meta, cover, ctx) = (
            self.source.clone(),
            self.meta.clone(),
            self.cover.clone(),
            ctx.clone(),
        );
        std::thread::spawn(move || {
            let report = core::validate_selected(Path::new(&source), &meta, cover.as_deref());
            let _ = tx.send(Job::Checked(report));
            ctx.request_repaint();
        });
    }
    fn build(&mut self, ctx: &egui::Context) {
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.busy = "Готовим папку мода…";
        self.notice = None;
        let (source, output, meta, cover, create, ctx) = (
            self.source.clone(),
            self.output.clone(),
            self.meta.clone(),
            self.cover.clone(),
            self.create_workshop,
            ctx.clone(),
        );
        std::thread::spawn(move || {
            let result = (|| {
                if output.trim().is_empty() {
                    return Err("Выберите, куда сохранить готовый мод.".to_owned());
                }
                if create && !Path::new(&output).exists() {
                    std::fs::create_dir(&output)
                        .map_err(|e| format!("Не удалось создать WorkshopLocal: {e}"))?;
                }
                core::build_selected(
                    Path::new(&source),
                    Path::new(&output),
                    &meta,
                    cover.as_deref(),
                )
            })();
            let _ = tx.send(Job::Built(result));
            ctx.request_repaint();
        });
    }
    fn poll(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.receiver {
            match rx.try_recv() {
                Ok(job) => {
                    self.receiver = None;
                    match job {
                        Job::Discovered(Ok(found)) => self.finish_discovery(found, ctx),
                        Job::Discovered(Err(e)) => self.notice = Some(e),
                        Job::Checked(r) => self.report = Some(r),
                        Job::Built(Ok(b)) => {
                            self.result = Some(b.destination);
                            self.backup = b.backup;
                            self.fx.celebrate();
                            self.fx.transition();
                        }
                        Job::Built(Err(e)) => self.notice = Some(e),
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.receiver = None;
                    self.notice = Some(
                        "Операция прервалась. Ваши исходные файлы сохранены. Повторите попытку."
                            .into(),
                    );
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(100))
                }
            }
        }
    }
    fn navigate(&mut self, step: usize, ctx: &egui::Context) {
        if step > 0 && !self.loaded {
            return;
        }
        if step < 2 {
            self.result = None;
            self.report = None;
            self.backup = None;
        }
        self.notice = None;
        self.step = step;
        self.fx.transition();
        if step == 2 && self.result.is_none() {
            self.check(ctx);
        }
    }
    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("navigation").exact_width(228.0).resizable(false)
            .frame(egui::Frame::new().fill(NAV).inner_margin(20.0)).show(ctx, |ui| {
            ui.add_space(18.0);
            ui.label(RichText::new("AveSep Helper").size(22.0).strong().color(Color32::WHITE));
            ui.label(RichText::new("Workshop Prep").size(14.0).color(Color32::from_rgb(151, 181, 188)));
            ui.add_space(42.0);
            for (i, (name, detail)) in STEPS.iter().enumerate() {
                let enabled = self.receiver.is_none() && (i == 0 || self.loaded) && (i != 2 || !self.raw_open);
                let (r, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 86.0), egui::Sense::click());
                let selected = i == self.step;
                let p = ui.painter();
                if selected || response.hovered() && enabled { p.rect_filled(r, 10, Color32::from_rgb(37, 65, 74)); }
                let center = r.min + Vec2::new(24.0, 27.0);
                p.circle_filled(center, 14.0, if selected { Color32::from_rgb(93, 201, 170) } else { Color32::from_rgb(48, 71, 81) });
                p.text(center, egui::Align2::CENTER_CENTER, (i + 1).to_string(), FontId::proportional(14.0), if selected { NAV } else { Color32::from_rgb(176, 196, 202) });
                p.text(r.min + Vec2::new(48.0, 27.0), egui::Align2::LEFT_CENTER, *name, FontId::proportional(15.0), if selected { Color32::WHITE } else { Color32::from_rgb(178, 199, 205) });
                // Wrap the explanatory line using a real text layout, not clipped paint text.
                let galley = p.layout(detail.to_string(), FontId::proportional(12.0), Color32::from_rgb(145, 171, 180), r.width() - 28.0);
                p.galley(r.min + Vec2::new(14.0, 52.0), galley, Color32::from_rgb(145, 171, 180));
                if enabled && response.clicked() { self.navigate(i, ctx); }
                ui.add_space(8.0);
            }
            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                ui.add_space(16.0);
                ui.checkbox(&mut self.fx.enabled, RichText::new("Мягкие анимации").size(13.0).color(Color32::from_rgb(164, 196, 195)));
                ui.hyperlink_to(RichText::new("Руководство по модам ↗").size(13.0).color(Color32::from_rgb(153, 206, 195)), "https://github.com/team-horay/sephiria_modding/blob/main/docs/ru/publishing.md");
                ui.add_space(12.0);
                ui.label(RichText::new("Подготовка здесь.\nПубликация — в самой игре.").size(13.0).color(Color32::from_rgb(145, 171, 180)));
            });
        });
    }
    fn footer(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("footer")
            .frame(
                egui::Frame::new()
                    .fill(Color32::WHITE)
                    .stroke(Stroke::new(1.0_f32, LINE))
                    .inner_margin(egui::Margin::symmetric(28, 18)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.receiver.is_some() {
                        if self.fx.enabled {
                            ui.spinner();
                        } else {
                            ui.label("В работе");
                        }
                        ui.label(self.busy);
                        self.fx.busy_line(ui);
                        return;
                    }
                    if self.result.is_some() {
                        ui.label(RichText::new("Готово к загрузке через игру").color(TEAL));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if action(ui, "Открыть папку мода", true).clicked() {
                                open_folder(self.result.as_ref().unwrap());
                            }
                        });
                        return;
                    }
                    if self.step > 0 {
                        if action(ui, "Назад", false).clicked() {
                            self.navigate(self.step - 1, ctx);
                        }
                    } else {
                        hint(ui, "Исходные файлы останутся на месте");
                    }
                    if self.raw_open {
                        hint(ui, "Примените или отмените изменения JSON");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let enabled = !self.raw_open
                            && match self.step {
                                0 => self.loaded,
                                1 => self.loaded,
                                _ => {
                                    self.report.as_ref().is_some_and(Report::ok)
                                        && !self.output.is_empty()
                                }
                            };
                        ui.add_enabled_ui(enabled, |ui| {
                            let label = match self.step {
                                0 => "Далее: оформление",
                                1 => "Далее: проверка",
                                _ => {
                                    if Path::new(&self.output)
                                        .join(self.meta["mod"]["modId"].as_str().unwrap_or(""))
                                        .exists()
                                    {
                                        "Обновить папку мода"
                                    } else {
                                        "Создать папку мода"
                                    }
                                }
                            };
                            if action(ui, label, true).clicked() {
                                if self.step < 2 {
                                    self.navigate(self.step + 1, ctx);
                                } else {
                                    self.build(ctx);
                                }
                            }
                        });
                    });
                });
            });
    }
    fn files_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.receiver.is_some() {
            card(ui, |ui| {
                title(ui, "Ищем компоненты мода");
                hint(
                    ui,
                    "Проверяем подпапки, переводы, костюмы, DLL и возможные обложки.",
                );
                ui.add_space(18.0);
                self.fx.busy_line(ui);
            });
            return;
        }
        if !self.loaded {
            if let Some(found) = self.found.clone().filter(|d| d.packages.len() > 1) {
                card(ui, |ui| {
                    title(ui, "Внутри найдено несколько модов");
                    hint(ui, "Выберите один пакет. Мы не будем смешивать их файлы.");
                    for package in &found.packages {
                        ui.add_space(12.0);
                        let label = if package.title.is_empty() {
                            package
                                .root
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string()
                        } else {
                            package.title.clone()
                        };
                        if action(ui, &label, false).clicked() {
                            self.source = package.root.display().to_string();
                            self.load(ctx);
                        }
                        hint(ui, package.root.display().to_string());
                    }
                });
            }
        }
        card(ui, |ui| {
            if !self.loaded {
                ui.add_space(2.0);
                ui.vertical_centered(|ui| {
                    folder_icon(ui, 64.0);
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new("Начните с папки вашего мода")
                            .size(22.0)
                            .strong(),
                    );
                    hint(
                        ui,
                        "Выберите папку с готовыми файлами перевода, костюмов или DLL.",
                    );
                    hint(ui, "Если описания мода ещё нет, поможем его заполнить.");
                    ui.add_space(18.0);
                    if action(ui, "Выбрать папку мода", true).clicked() {
                        if let Some(p) = dialogs::pick_folder() {
                            self.source = p.display().to_string();
                            self.load(ctx);
                        }
                    }
                    ui.add_space(8.0);
                });
            } else {
                ui.horizontal(|ui| {
                    folder_icon(ui, 52.0);
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(
                                Path::new(&self.source)
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy(),
                            )
                            .size(20.0)
                            .strong(),
                        );
                        hint(ui, &self.source);
                    });
                });
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Папка выбрана").color(TEAL).strong());
                    if ui.link("Выбрать другую").clicked() {
                        if let Some(p) = dialogs::pick_folder() {
                            self.source = p.display().to_string();
                            self.load(ctx);
                        }
                    }
                });
            }
            ui.add_space(8.0);
            egui::CollapsingHeader::new("Указать путь вручную").show(ui, |ui| {
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut self.source)
                            .desired_width(f32::INFINITY)
                            .hint_text("C:/Мои моды/Название мода")
                            .margin(Vec2::splat(10.0)),
                    )
                    .changed()
                {
                    self.loaded = false;
                    self.report = None;
                    self.result = None;
                }
                if action(ui, "Открыть эту папку", false).clicked() {
                    self.load(ctx);
                }
            });
        });
        if self.loaded {
            self.discovery_card(ui, ctx);
            self.modules_card(ui);
        } else {
            card(ui, |ui| {
                title(ui, "Что будет дальше");
                for (number, heading, detail) in [
                    (
                        "1",
                        "Выберите файлы",
                        "Программа соберёт отдельную копию мода.",
                    ),
                    (
                        "2",
                        "Оформите страницу",
                        "Добавьте название, описание и картинку.",
                    ),
                    (
                        "3",
                        "Подготовьте к загрузке",
                        "Проверим файлы и создадим готовую папку.",
                    ),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(number).color(TEAL).strong());
                        ui.label(RichText::new(heading).strong());
                    });
                    hint(ui, detail);
                    ui.add_space(2.0);
                }
            });
        }
    }
    fn discovery_card(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(mut found) = self.found.take() else {
            return;
        };
        let mut apply = false;
        card(ui, |ui| {
            title(ui, "Автопоиск завершён");
            hint(
                ui,
                format!(
                    "Просмотрено файлов: {}. Найдено компонентов: {}, библиотек: {}, обложек: {}.",
                    found.file_count,
                    found.modules.len(),
                    found.libraries.len(),
                    found.covers.len()
                ),
            );
            if self.meta["mod"]["modules"]
                .as_array()
                .is_some_and(|a| !a.is_empty())
            {
                ui.add_space(8.0);
                ui.label(RichText::new("Распознанные компоненты уже добавлены").color(TEAL));
            }
            let needs_choice = found.modules.iter().any(|m| !m.automatic);
            egui::CollapsingHeader::new("Посмотреть и выбрать найденные файлы").default_open(needs_choice).show(ui, |ui| {
                for (i, m) in found.modules.iter_mut().enumerate() {
                    ui.add_space(12.0);
                    let ty = m.module["type"].as_str().unwrap_or("");
                    let kind = MODULES.iter().find(|v| v.0 == ty).map_or(ty, |v| v.1);
                    self.module_selection[i] = self.meta["mod"]["modules"].as_array().is_some_and(|a| a.iter().any(|v| v["type"] == m.module["type"] && v["path"] == m.module["path"]));
                    apply |= ui.checkbox(&mut self.module_selection[i], format!("{kind} · {}", m.module["path"].as_str().unwrap_or(""))).changed();
                    hint(ui, &m.reason);
                    if m.entry_classes.len() > 1 {
                        let mut class = m.module["entryClass"].as_str().unwrap_or("").to_owned();
                        let before = class.clone();
                        egui::ComboBox::from_id_salt(("entry-candidate", i)).selected_text(if class.is_empty() { "Выберите класс запуска" } else { &class }).show_ui(ui, |ui| { for candidate in &m.entry_classes { ui.selectable_value(&mut class, candidate.clone(), candidate); } });
                        m.module["entryClass"] = json!(class);
                        apply |= before != class && self.module_selection[i];
                    }
                }
                for (i, path) in found.libraries.iter().enumerate() {
                    self.library_selection[i] = self.meta["mod"]["libraries"].as_array().is_some_and(|a| a.iter().any(|v| v.as_str() == Some(path) || v["path"].as_str() == Some(path)));
                    ui.add_space(8.0); apply |= ui.checkbox(&mut self.library_selection[i], format!("Библиотека · {path}")).changed();
                }
                ui.add_space(12.0);
                hint(ui, "В пакет попадут только DLL из выбранных компонентов и библиотек. Другие DLL будут пропущены. Остальные файлы мода сохранятся.");
                hint(ui, "Выбор сохраняется автоматически.");
            });
            if !found.notes.is_empty() {
                ui.add_space(8.0);
                egui::CollapsingHeader::new(format!("Обратите внимание ({})", found.notes.len()))
                    .default_open(found.modules.is_empty())
                    .show(ui, |ui| {
                        for note in &found.notes {
                            hint(ui, note);
                            ui.add_space(4.0);
                        }
                    });
            }
            ui.add_space(8.0);
            if ui.link("Повторить поиск в папке").clicked() {
                let meta = self.meta.clone();
                let cover = self.cover.clone();
                self.load(ctx);
                self.rescan_meta = Some(meta);
                self.rescan_cover = cover;
            }
        });
        if self.receiver.is_none() {
            self.found = Some(found);
        }
        if apply {
            self.apply_found();
        }
    }
    fn modules_card(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            title(ui, "Что находится в моде");
            let count = self.meta["mod"]["modules"].as_array().map_or(0, Vec::len);
            if count == 0 {
                hint(ui, "Добавьте хотя бы один компонент. Например, папку перевода или DLL вашего мода.");
            } else {
                for module in self.meta["mod"]["modules"].as_array().into_iter().flatten() {
                    let ty = module["type"].as_str().unwrap_or("");
                    let name = MODULES
                        .iter()
                        .find(|m| m.0 == ty)
                        .map_or("Неизвестный компонент", |m| m.1);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(name).strong());
                        hint(ui, module["path"].as_str().unwrap_or("Путь не указан"));
                    });
                }
                hint(
                    ui,
                    "Если всё определилось верно, можно перейти к оформлению.",
                );
            }
            ui.add_space(8.0);
            egui::CollapsingHeader::new("Настроить состав мода").default_open(self.modules_open).show(ui, |ui| {
                let mut remove = None;
                if let Some(modules) = self.meta["mod"]["modules"].as_array_mut() {
                    for (i, module) in modules.iter_mut().enumerate() {
                        ui.push_id(i, |ui| {
                            if !module.is_object() { ui.colored_label(RED, "Некорректный компонент. Исправьте его в редакторе JSON."); return; }
                            ui.add_space(14.0);
                            let mut ty = module["type"].as_str().unwrap_or("localization").to_owned();
                            let label = MODULES.iter().find(|m| m.0 == ty).map_or("Выберите тип", |m| m.1);
                            ui.horizontal(|ui| {
                                egui::ComboBox::from_id_salt("module-type").selected_text(label).show_ui(ui, |ui| { for (key, name, _) in MODULES { if ui.selectable_value(&mut ty, key.to_string(), name).changed() { module["type"] = json!(ty); } } });
                                if ui.link("Убрать компонент").clicked() { remove = Some(i); }
                            });
                            hint(ui, MODULES.iter().find(|m| m.0 == ty).map_or("", |m| m.2));
                            edit(ui, "Папка или файл внутри мода", &mut module["path"], if ty == "dll" { "bin/MyMod.dll" } else { "Localization" });
                            if ui.link(if ty == "dll" { "Выбрать DLL…" } else { "Выбрать папку компонента…" }).clicked() {
                                let picked = if ty == "dll" { dialogs::pick_dll() } else { dialogs::pick_folder() };
                                if let Some(p) = picked {
                                    match Path::new(&self.source).canonicalize().and_then(|root| p.canonicalize().map(|p| (root, p))) {
                                        Ok((root, p)) => if let Ok(rel) = p.strip_prefix(root) { module["path"] = json!(rel.to_string_lossy().replace('\\', "/")); } else { self.notice = Some("Компонент должен находиться внутри выбранной папки мода.".into()); },
                                        Err(e) => self.notice = Some(e.to_string()),
                                    }
                                }
                            }
                            if ty == "dll" {
                                ui.add_space(10.0); edit(ui, "Класс запуска DLL (entryClass)", &mut module["entryClass"], "MyMod.Entry");
                                hint(ui, "Полное имя класса из кода мода. Например, MyMod.Entry. Его указывает автор DLL.");
                            }
                            ui.add_space(8.0); ui.separator();
                        });
                    }
                    if let Some(i) = remove { modules.remove(i); }
                }
                ui.add_space(12.0);
                if action(ui, "+ Добавить компонент", false).clicked() {
                    if !self.meta["mod"]["modules"].is_array() { self.meta["mod"]["modules"] = json!([]); }
                    self.meta["mod"]["modules"].as_array_mut().unwrap().push(json!({"type":"localization","path":"Localization"}));
                }
                hint(ui, "Пути задаются через / и с точным регистром букв. Полный список файлов проверим на последнем шаге.");
            });
        });
    }
    fn details_form(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            title(ui, "Как называется ваш мод?");
            let mut name = core::text(&self.meta["info"]["title"]);
            ui.label(RichText::new("Название").strong().size(14.0));
            if ui
                .add(
                    egui::TextEdit::singleline(&mut name)
                        .hint_text("Например, Новый перевод Sephiria")
                        .desired_width(f32::INFINITY)
                        .margin(Vec2::new(11.0, 10.0)),
                )
                .changed()
            {
                core::set_text(&mut self.meta["info"]["title"], name.clone());
            }
            hint(ui, format!("{} / 128 символов", name.chars().count()));
            ui.add_space(14.0);
            let mut description = core::text(&self.meta["info"]["description"]);
            ui.label(RichText::new("Описание").strong().size(14.0));
            if ui
                .add(
                    egui::TextEdit::multiline(&mut description)
                        .hint_text(
                            "Что добавляет мод? Как им пользоваться? Расскажите об этом игрокам.",
                        )
                        .desired_width(f32::INFINITY)
                        .desired_rows(4)
                        .margin(Vec2::splat(11.0)),
                )
                .changed()
            {
                core::set_text(&mut self.meta["info"]["description"], description.clone());
            }
            hint(
                ui,
                format!("{} / 7999 символов", description.chars().count()),
            );
            ui.add_space(14.0);
            edit(
                ui,
                "Автор",
                &mut self.meta["mod"]["author"],
                "Ваше имя или никнейм",
            );
            edit(ui, "Версия мода", &mut self.meta["mod"]["version"], "1.0.0");
            hint(
                ui,
                "Первый выпуск — 1.0.0. При обновлении увеличьте версию, например до 1.0.1.",
            );
            if self.meta["info"]["title"].is_object()
                || self.meta["info"]["description"].is_object()
            {
                ui.add_space(8.0);
                hint(ui, "Редактируется английская версия страницы (en-US). Другие переводы сохраняются.");
            }
        });
        card(ui, |ui| {
            title(ui, "Категории");
            hint(ui, "Выберите, к чему относится мод. Можно несколько.");
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                for (tag, label) in TAGS {
                    let selected = self.meta["tags"]
                        .as_array()
                        .is_some_and(|a| a.iter().any(|t| t == tag));
                    let button = egui::Button::new(RichText::new(label).color(if selected {
                        TEAL
                    } else {
                        MUTED
                    }))
                    .corner_radius(20)
                    .fill(if selected { PALE } else { BG })
                    .stroke(Stroke::NONE);
                    if ui.add(button).clicked() {
                        if !self.meta["tags"].is_array() {
                            self.meta["tags"] = json!([]);
                        }
                        let tags = self.meta["tags"].as_array_mut().unwrap();
                        tags.retain(|t| t != tag);
                        if !selected {
                            tags.push(json!(tag));
                        }
                    }
                }
            });
        });
    }
    fn cover_card(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        card(ui, |ui| {
            title(ui, "Обложка мода");
            hint(ui, "Изображение для страницы в Мастерской Steam.");
            ui.add_space(10.0);
            self.cover_image(ui);
            ui.add_space(10.0);
            if action(
                ui,
                if self.texture.is_some() {
                    "Заменить обложку"
                } else {
                    "Выбрать изображение"
                },
                false,
            )
            .clicked()
            {
                if let Some(p) = dialogs::pick_image() {
                    self.cover = Some(p);
                    self.refresh_cover(ctx);
                }
            }
            ui.add_space(6.0);
            hint(
                ui,
                "PNG или JPG. Подгоним размер и вес выбранной картинки автоматически.",
            );
            if let Some(found) = &self.found {
                if !found.covers.is_empty() {
                    ui.add_space(10.0);
                    let mut selected = self.meta["info"]["preview"]
                        .as_str()
                        .unwrap_or("")
                        .to_owned();
                    let before = selected.clone();
                    egui::ComboBox::from_id_salt("found-cover")
                        .selected_text("Обложки, найденные в папке")
                        .width(ui.available_width().min(280.0))
                        .show_ui(ui, |ui| {
                            for path in &found.covers {
                                ui.selectable_value(&mut selected, path.clone(), path);
                            }
                        });
                    if before != selected {
                        self.meta["info"]["preview"] = json!(selected);
                        self.cover = None;
                        self.refresh_cover(ctx);
                    }
                }
            }
            if self.cover.is_some() && ui.link("Вернуть обложку из папки мода").clicked()
            {
                self.cover = None;
                self.refresh_cover(ctx);
            }
        });
        card(ui, |ui| {
            hint(ui, "ПРИМЕР КАРТОЧКИ МОДА");
            ui.add_space(8.0);
            let name = core::text(&self.meta["info"]["title"]);
            ui.label(
                RichText::new(if name.trim().is_empty() {
                    "Название вашего мода"
                } else {
                    &name
                })
                .size(20.0)
                .strong(),
            );
            let author = self.meta["mod"]["author"].as_str().unwrap_or("");
            hint(
                ui,
                if author.is_empty() {
                    "Автор мода".to_owned()
                } else {
                    format!("Автор: {author}")
                },
            );
            ui.add_space(8.0);
            let description = core::text(&self.meta["info"]["description"]);
            let short: String = description.chars().take(160).collect();
            hint(
                ui,
                if short.is_empty() {
                    "Здесь появится краткое описание вашего мода."
                } else {
                    &short
                },
            );
        });
    }
    fn cover_image(&self, ui: &mut egui::Ui) {
        let width = ui.available_width();
        let (r, _) = ui.allocate_exact_size(Vec2::new(width, width * 0.56), egui::Sense::hover());
        ui.painter().rect_filled(r, 10, BG);
        if let Some(texture) = &self.texture {
            let size = texture.size_vec2();
            let scale = (r.width() / size.x).min(r.height() / size.y);
            let image = egui::Rect::from_center_size(r.center(), size * scale);
            ui.painter().image(
                texture.id(),
                image,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            ui.painter().text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                "Здесь будет обложка",
                FontId::proportional(14.0),
                MUTED,
            );
        }
    }
    fn design_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if ui.available_width() >= 790.0 {
            let width = ui.available_width();
            let left = (width - 22.0) * 0.61;
            let right = width - left - 22.0;
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(left, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        self.details_form(ui);
                    },
                );
                ui.add_space(12.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(right, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        self.cover_card(ui, ctx);
                    },
                );
            });
        } else {
            self.details_form(ui);
            self.cover_card(ui, ctx);
        }
        card(ui, |ui| {
            egui::CollapsingHeader::new(RichText::new("Другие настройки мода").strong()).show(ui, |ui| {
                ui.add_space(12.0);
                edit(ui, "Уникальное имя мода (modId)", &mut self.meta["mod"]["modId"], "yourname.my-mod");
                hint(ui, "Уникальный код связывает мод с публикацией. Его можно изменить до первой загрузки, после неё — сохраняйте прежним.");
                ui.add_space(18.0); ui.label(RichText::new("Кому нужен мод в мультиплеере?").strong());
                let current = self.meta["mod"]["multiplayer"]["installation"].as_str().unwrap_or("everyone").to_owned();
                let modes = [("everyone", "Всем игрокам", "Для новых предметов, костюмов и изменений игрового мира."), ("local_only", "Только мне", "Для перевода или интерфейса, которые видны только вам."), ("host_only", "Только хосту", "Для авторов серверных модов; убедитесь, что клиентам мод не нужен.")];
                for (key, label, detail) in modes {
                    if ui.radio(current == key, label).clicked() { self.meta["mod"]["multiplayer"] = json!({"installation":key}); }
                    hint(ui, detail);
                }
            });
            ui.add_space(6.0);
            egui::CollapsingHeader::new("Для опытных авторов: полный metadata.json").show(ui, |ui| {
                hint(ui, "Библиотеки, зависимости, переводы и дополнительные параметры. Изменения нужно применить к форме.");
                if !self.raw_open && ui.button("Открыть редактор JSON").clicked() { self.raw = serde_json::to_string_pretty(&self.meta).unwrap_or_default(); self.raw_open = true; }
                if self.raw_open {
                    ui.add(egui::TextEdit::multiline(&mut self.raw).code_editor().desired_width(f32::INFINITY).desired_rows(12));
                    ui.horizontal(|ui| {
                        if action(ui, "Применить изменения", false).clicked() {
                            match serde_json::from_str::<Value>(&self.raw) {
                                Ok(v) if v["info"].is_object() && v["mod"].is_object() => { self.meta = v; self.raw_open = false; self.invalidate(); self.refresh_cover(ctx); }
                                Ok(_) => self.notice = Some("В JSON нужны разделы info и mod.".into()), Err(e) => self.notice = Some(format!("Не удалось прочитать JSON: {e}")),
                            }
                        }
                        if ui.link("Отменить").clicked() { self.raw_open = false; }
                    });
                }
            });
        });
    }
    fn ready_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if let Some(result) = self.result.clone() {
            card(ui, |ui| {
                self.fx.success(ui);
                ui.label(
                    RichText::new("Мод готов к загрузке")
                        .size(24.0)
                        .strong()
                        .color(TEAL),
                );
                hint(ui, "Готовая папка создана. Осталось протестировать мод и загрузить его через игру.");
                ui.add_space(14.0);
                ui.label(RichText::new(result.display().to_string()).size(14.0));
                if ui.link("Скопировать путь").clicked() {
                    ctx.copy_text(result.display().to_string());
                }
                if let Some(backup) = &self.backup {
                    ui.add_space(10.0);
                    hint(
                        ui,
                        format!("Предыдущая версия сохранена: {}", backup.display()),
                    );
                }
            });
            card(ui, |ui| {
                title(ui, "Следующие шаги");
                for (i, heading, detail) in [(1, "Откройте Sephiria и проверьте мод", "Если выбрали другую папку, сначала перенесите мод в WorkshopLocal внутри папки игры."), (2, "Откройте «Мастерская Steam» → «Мои моды»", "Выберите свой мод и нажмите «Загрузить в Мастерскую»."), (3, "Проверьте копию из Steam и откройте доступ", "Подпишитесь на мод, проверьте его работу и смените видимость на «Для всех».")] {
                    ui.label(RichText::new(format!("{i}. {heading}")).strong()); hint(ui, detail); ui.add_space(14.0);
                }
                hint(ui, "Для проверки мультиплеера используйте подписку: локальные моды из WorkshopLocal блокируют сетевую игру.");
            });
            if ui.link("Вернуться к редактированию").clicked() {
                self.result = None;
                self.report = None;
                self.step = 1;
            }
            return;
        }
        card(ui, |ui| {
            title(ui, "Куда сохранить готовый мод?");
            hint(
                ui,
                "Лучше сразу в папку игры — тогда Sephiria увидит мод в своём меню.",
            );
            ui.add_space(14.0);
            ui.horizontal_wrapped(|ui| {
                if action(ui, "Выбрать папку игры", false).clicked() {
                    if let Some(p) = dialogs::pick_folder() {
                        self.output = p.join("WorkshopLocal").display().to_string();
                        self.create_workshop = true;
                        self.notice = None;
                    }
                }
                if ui.link("Сохранить в другую папку…").clicked() {
                    if let Some(p) = dialogs::pick_folder() {
                        self.output = p.display().to_string();
                        self.create_workshop = false;
                        self.notice = None;
                    }
                }
            });
            ui.add_space(12.0);
            if !self.output.is_empty() {
                ui.label(RichText::new("Готовая папка").strong());
                ui.add(
                    egui::Label::new(format!(
                        "{} / {}",
                        self.output,
                        self.meta["mod"]["modId"].as_str().unwrap_or("")
                    ))
                    .wrap(),
                );
                if self.create_workshop && !Path::new(&self.output).exists() {
                    hint(ui, "WorkshopLocal будет создана автоматически.");
                }
                if Path::new(&self.output)
                    .join(self.meta["mod"]["modId"].as_str().unwrap_or(""))
                    .exists()
                {
                    ui.add_space(8.0);
                    hint(ui, "Здесь уже есть этот мод. Сохраним резервную копию и билет публикации. Для обновления нужна новая версия.");
                }
            } else {
                hint(ui, "В Steam: Sephiria → Управление → Просмотреть локальные файлы. Выберите открывшуюся папку.");
            }
        });
        card(ui, |ui| {
            let Some(report) = &self.report else {
                title(ui, "Проверяем содержимое");
                hint(ui, "Проверим описание, пути, компоненты и обложку.");
                return;
            };
            if report.ok() {
                ui.label(
                    RichText::new("Проверка пройдена")
                        .size(20.0)
                        .strong()
                        .color(TEAL),
                );
                hint(
                    ui,
                    "Файлы готовы к подготовке. Работу самого мода нужно проверить в игре.",
                );
            } else {
                ui.label(
                    RichText::new(format!("Нужно исправить: {}", report.errors.len()))
                        .size(20.0)
                        .strong()
                        .color(RED),
                );
                hint(
                    ui,
                    "Вернитесь к файлам или оформлению, исправьте ошибки и повторите проверку.",
                );
                ui.add_space(10.0);
                for error in &report.errors {
                    ui.horizontal_top(|ui| {
                        ui.colored_label(RED, "•");
                        ui.add(egui::Label::new(error).wrap());
                    });
                }
            }
            ui.add_space(12.0);
            hint(
                ui,
                format!(
                    "{} файлов  ·  {:.2} МБ до упаковки",
                    report.files.len(),
                    report.bytes as f64 / 1_000_000.0
                ),
            );
            if !report.warnings.is_empty() {
                ui.add_space(8.0);
                egui::CollapsingHeader::new(format!(
                    "Примечания и исключённые файлы ({})",
                    report.warnings.len()
                ))
                .show(ui, |ui| {
                    for warning in &report.warnings {
                        hint(ui, warning);
                    }
                });
            }
            egui::CollapsingHeader::new("Какие файлы попадут в мод").show(ui, |ui| {
                for path in &report.files {
                    hint(ui, path.display().to_string());
                }
            });
            ui.add_space(10.0);
            if ui.link("Проверить ещё раз").clicked() {
                self.check(ctx);
            }
        });
        hint(ui, "Создаём отдельную папку и сохраняем исходник. В Steam ничего не загружается — это следующий шаг в игре.");
    }
    fn capture(&mut self, ctx: &egui::Context) {
        // Renderer-owned screenshots make UI layout review repeatable without desktop automation.
        if let Some(path) = &self.screenshot {
            self.frames += 1;
            let shot = ctx.input(|i| {
                i.events.iter().find_map(|e| {
                    if let egui::Event::Screenshot { image, .. } = e {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
            });
            if let Some(shot) = shot {
                let bytes: Vec<u8> = shot.pixels.iter().flat_map(|p| p.to_array()).collect();
                if let Some(image) =
                    image::RgbaImage::from_raw(shot.width() as u32, shot.height() as u32, bytes)
                {
                    if let Err(e) = image.save(path) {
                        eprintln!("Screenshot: {e}");
                    }
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else if self.frames >= 6
                && self.fx.opacity(ctx) >= 0.999
                && !self.screenshot_requested
            {
                self.screenshot_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            ctx.request_repaint_after(Duration::from_millis(150));
        }
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("motion-enabled"), self.fx.enabled));
        ctx.style_mut(|s| s.animation_time = if self.fx.enabled { 0.18 } else { 0.0 });
        if self.receiver.is_some() && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.notice = Some(
                "Дождитесь завершения текущей операции. После этого окно можно закрыть.".into(),
            );
        }
        self.poll(ctx);
        self.sidebar(ctx);
        self.footer(ctx);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(30, 26)),
            )
            .show(ctx, |ui| {
                ui.set_max_width(ui.available_width().min(1050.0));
                let fade = self.fx.opacity(ctx);
                ui.set_opacity(0.35 + fade * 0.65);
                ui.add_space((1.0 - fade) * 6.0);
                ui.label(
                    RichText::new(format!("ШАГ {} ИЗ 3", self.step + 1))
                        .size(12.0)
                        .strong()
                        .color(TEAL),
                );
                let (heading, detail) = match self.step {
                    0 => (
                        "Выберите ваш мод",
                        "Начнём с файлов. Оригинальная папка останется без изменений.",
                    ),
                    1 => (
                        "Расскажите о своём моде",
                        "Помогите игрокам понять, что добавляет мод и зачем его устанавливать.",
                    ),
                    _ if self.result.is_some() => (
                        "Всё готово",
                        "Папка подготовлена. Теперь мод можно загрузить через Sephiria.",
                    ),
                    _ => (
                        "Подготовьте мод к публикации",
                        "Выберите место для готовой папки. Перед сохранением проверим файлы.",
                    ),
                };
                ui.heading(heading);
                hint(ui, detail);
                ui.add_space(20.0);
                egui::ScrollArea::vertical()
                    .id_salt(("page", self.step))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if let Some(notice) = &self.notice {
                            egui::Frame::new()
                                .fill(Color32::from_rgb(253, 238, 236))
                                .inner_margin(16.0)
                                .corner_radius(10)
                                .show(ui, |ui| {
                                    ui.add(
                                        egui::Label::new(RichText::new(notice).color(RED)).wrap(),
                                    );
                                });
                            ui.add_space(14.0);
                        }
                        ui.add_enabled_ui(self.receiver.is_none(), |ui| match self.step {
                            0 => self.files_page(ui, ctx),
                            1 => self.design_page(ui, ctx),
                            _ => self.ready_page(ui, ctx),
                        });
                        ui.add_space(12.0);
                    });
            });
        self.capture(ctx);
    }
}
fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let value = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let screenshot = value("--screenshot").map(PathBuf::from);
    let preview_step: usize = value("--preview-step")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let value_source = value("--source");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 850.0])
            .with_min_inner_size([980.0, 680.0]),
        ..Default::default()
    };
    eframe::run_native(
        "AveSep Helper · Sephiria",
        options,
        Box::new(move |cc| {
            style(&cc.egui_ctx);
            let mut app = App {
                screenshot,
                ..Default::default()
            };
            if let Some(source) = value_source.clone() {
                app.source = source;
                app.load(&cc.egui_ctx);
            } else if app.screenshot.is_some() && preview_step > 0 {
                app.step = preview_step.min(2);
                app.loaded = true;
                app.source = "C:/Мои моды/SephiriaTranslation".into();
                app.output = "C:/Steam/steamapps/common/Sephiria/WorkshopLocal".into();
                app.meta["info"]["title"] = json!("Sephiria: новый перевод");
                app.meta["info"]["description"] = json!("Более понятные описания предметов, обновлённые диалоги и исправления перевода. Совместим с основной версией игры.");
                app.meta["mod"]["author"] = json!("Автор мода");
                app.meta["mod"]["modId"] = json!("author.translation");
                app.meta["tags"] = json!(["Localization"]);
                app.meta["mod"]["modules"] = json!([{"type":"localization","path":"Localization"}]);
                if preview_step == 2 {
                    app.report = Some(Report {
                        files: vec!["Localization/ru-RU.json".into(), "thumbnail.png".into()],
                        bytes: 358_000,
                        ..Default::default()
                    });
                }
            }
            Ok(Box::new(app))
        }),
    )
}
