use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use dm32_codeplug::Image;
use dm32_codeplug::radio::WriteOptions;
use dm32_proto::LineConfig;
use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::files::{self, Purpose};
use crate::job::{ConnSettings, Job, Outcome, Request};
use crate::views::{self, Doc, View};

/// Connection preferences, remembered between runs.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
struct Prefs {
    port: String,
    rts: String,
    dtr: String,
    trace: bool,
    read_contacts: bool,
    verify: bool,
    skip_settings: bool,
    recent: Vec<PathBuf>,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            port: String::new(),
            rts: "default".into(),
            dtr: "default".into(),
            trace: false,
            read_contacts: false,
            verify: true,
            skip_settings: false,
            recent: Vec::new(),
        }
    }
}

impl Prefs {
    fn lines(&self) -> LineConfig {
        LineConfig {
            rts: self.rts.parse().unwrap_or_default(),
            dtr: self.dtr.parse().unwrap_or_default(),
        }
    }
}

enum Dialog {
    Connection { ports: Vec<String> },
    ConfirmWrite,
    Message { title: String, text: String },
    ConfirmDiscard(PendingAction),
}

#[derive(Clone)]
enum PendingAction {
    Open,
    OpenPath(PathBuf),
    New,
    Read,
    Quit,
}

pub struct App {
    doc: Option<Doc>,
    view: View,
    prefs: Prefs,
    job: Option<Job>,
    dialog: Option<Dialog>,
    status: String,
    allow_close: bool,
    ctx: egui::Context,
    title: String,
}

const PREFS_KEY: &str = "prefs";

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, file: Option<PathBuf>) -> Self {
        let prefs = cc
            .storage
            .and_then(|s| eframe::get_value(s, PREFS_KEY))
            .unwrap_or_default();
        let mut app = App {
            doc: None,
            view: View::default(),
            prefs,
            job: None,
            dialog: None,
            status: "Open a codeplug file or read one from the radio.".into(),
            allow_close: false,
            ctx: cc.egui_ctx.clone(),
            title: String::new(),
        };
        if let Some(f) = file {
            app.open_path(&f);
        }
        app
    }

    fn message(&mut self, title: &str, text: impl Into<String>) {
        self.dialog = Some(Dialog::Message {
            title: title.into(),
            text: text.into(),
        });
    }

    fn dirty(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.dirty)
    }

    /// Run `action` now, or ask first if there are unsaved changes.
    fn guarded(&mut self, action: PendingAction) {
        if self.dirty() {
            self.dialog = Some(Dialog::ConfirmDiscard(action));
        } else {
            self.perform(action);
        }
    }

    fn perform(&mut self, action: PendingAction) {
        match action {
            PendingAction::Open => files::request(Purpose::Open),
            PendingAction::OpenPath(p) => self.open_path(&p),
            PendingAction::New => {
                let bytes = include_bytes!("../../dm32-codeplug/tests/fixtures/factory-v1.60.data");
                match dm32_codeplug::image::data_file::import(bytes) {
                    Ok(img) => {
                        self.doc = Some(Doc::new(img, None));
                        self.status = "New codeplug from factory defaults.".into();
                    }
                    Err(e) => self.message("Error", e.to_string()),
                }
            }
            PendingAction::Read => self.start_job(
                "Reading from radio",
                Request::Read {
                    contacts: self.prefs.read_contacts,
                },
            ),
            PendingAction::Quit => {
                self.allow_close = true;
            }
        }
    }

    fn open_path(&mut self, p: &Path) {
        match Image::load(p) {
            Ok(img) => {
                // An official .data file is saved back in this program's own format.
                let keep = p.extension().is_some_and(|e| e == "dm32");
                self.doc = Some(Doc::new(img, keep.then(|| p.to_path_buf())));
                self.status = format!("Opened {}", p.display());
                self.remember(p);
            }
            Err(e) => self.message("Cannot open file", format!("{}: {e}", p.display())),
        }
    }

    fn remember(&mut self, p: &Path) {
        let p = p.to_path_buf();
        self.prefs.recent.retain(|r| r != &p);
        self.prefs.recent.insert(0, p);
        self.prefs.recent.truncate(8);
    }

    /// Save to the current file, or ask for a name first.
    fn save(&mut self, save_as: bool) {
        let Some(doc) = self.doc.as_ref() else { return };
        match (&doc.path, save_as) {
            (Some(p), false) => {
                let p = p.clone();
                self.save_to(&p);
            }
            _ => files::request(Purpose::SaveAs),
        }
    }

    fn save_to(&mut self, path: &Path) {
        let Some(doc) = self.doc.as_mut() else { return };
        match doc.image.save(path) {
            Ok(()) => {
                doc.path = Some(path.to_path_buf());
                doc.dirty = false;
                self.status = format!("Saved {}", path.display());
                self.remember(path);
            }
            Err(e) => self.message("Cannot save", e.to_string()),
        }
    }

    fn export_data_to(&mut self, path: &Path) {
        let Some(doc) = self.doc.as_ref() else { return };
        match doc.image.save(path) {
            Ok(()) => self.status = format!("Exported {}", path.display()),
            Err(e) => self.message("Cannot export", e.to_string()),
        }
    }

    /// Act on paths chosen in file dialogs.
    fn file_results(&mut self) {
        if let Some(p) = files::take(Purpose::Open) {
            self.open_path(&p);
        }
        if let Some(p) = files::take(Purpose::SaveAs) {
            self.save_to(&p);
        }
        if let Some(p) = files::take(Purpose::ExportData) {
            self.export_data_to(&p);
        }
    }

    fn conn(&self) -> Option<ConnSettings> {
        let port = if self.prefs.port.is_empty() {
            match dm32_proto::list_ports().as_slice() {
                [p] => p.clone(),
                _ => return None,
            }
        } else {
            self.prefs.port.clone()
        };
        let trace = self
            .prefs
            .trace
            .then(|| data_dir().join(format!("trace-{}.log", timestamp())));
        Some(ConnSettings {
            port,
            lines: self.prefs.lines(),
            trace,
        })
    }

    fn start_job(&mut self, title: &str, req: Request) {
        let Some(conn) = self.conn() else {
            self.dialog = Some(Dialog::Connection {
                ports: dm32_proto::list_ports(),
            });
            self.status = "Choose the serial port of the programming cable.".into();
            return;
        };
        let ctx = self.ctx.clone();
        self.job = Some(Job::start(title, conn, req, move || ctx.request_repaint()));
    }

    fn start_write(&mut self) {
        let Some(doc) = self.doc.as_ref() else { return };
        let opts = WriteOptions {
            all_blocks: false,
            skip_settings: self.prefs.skip_settings,
            contacts: !doc.image.contacts.is_empty() && doc.contacts_dirty,
            verify: self.prefs.verify,
        };
        let backup = data_dir()
            .join("backups")
            .join(format!("radio-{}.dm32", timestamp()));
        let req = Request::Write {
            image: Box::new(doc.image.clone()),
            opts,
            backup,
        };
        self.start_job("Writing to radio", req);
    }

    fn finish_job(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Probed(info) => {
                let text = format!(
                    "Model: {}\nFirmware: {}\nBuild date: {}\nDSP: {}\nRadio: {}\nCodeplug: {}\nMax contacts: {}",
                    info.model,
                    info.firmware,
                    info.build_date,
                    info.dsp_version,
                    info.radio_version,
                    info.codeplug_version,
                    info.max_contacts.map(|n| n.to_string()).unwrap_or_default()
                );
                self.message("Radio found", text);
            }
            Outcome::Read(img) => {
                self.status = format!(
                    "Read {} blocks from the radio ({}).",
                    img.blocks.len(),
                    img.info.firmware
                );
                let mut doc = Doc::new(*img, None);
                doc.dirty = true;
                self.doc = Some(doc);
            }
            Outcome::Wrote { report, backup } => {
                if let Some(d) = self.doc.as_mut() {
                    d.contacts_dirty = false;
                }
                let mut text = format!(
                    "Wrote {} blocks.\nThe radio's previous contents were saved to\n{}",
                    report.written.len(),
                    backup.display()
                );
                if report.mismatches.is_empty() {
                    if self.prefs.verify && !report.written.is_empty() {
                        text.push_str("\n\nRead-back verification passed.");
                    }
                    self.message("Write complete", text);
                } else {
                    text.push_str("\n\nVERIFICATION FAILED for:\n");
                    for (what, off) in &report.mismatches {
                        text.push_str(&format!("  {what} (offset {off:#05x})\n"));
                    }
                    self.message("Write finished with errors", text);
                }
            }
            Outcome::Failed(e) => self.message("Radio error", e),
            Outcome::Cancelled => self.status = "Cancelled.".into(),
        }
    }

    fn menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            let busy = self.job.is_some();
            let has_doc = self.doc.is_some();
            ui.menu_button("File", |ui| {
                if ui.button("New (factory defaults)").clicked() {
                    self.guarded(PendingAction::New);
                }
                if ui.button("Open…").clicked() {
                    self.guarded(PendingAction::Open);
                }
                ui.menu_button("Open recent", |ui| {
                    for p in self.prefs.recent.clone() {
                        if ui.button(p.display().to_string()).clicked() {
                            self.guarded(PendingAction::OpenPath(p));
                        }
                    }
                });
                ui.separator();
                if ui.add_enabled(has_doc, egui::Button::new("Save").shortcut_text("Ctrl+S")).clicked() {
                    self.save(false);
                }
                if ui.add_enabled(has_doc, egui::Button::new("Save as…")).clicked() {
                    self.save(true);
                }
                if ui.add_enabled(has_doc, egui::Button::new("Export official CPS file (.data)…")).clicked() {
                    files::request(Purpose::ExportData);
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Edit", |ui| {
                let (can_undo, can_redo) = self.doc.as_ref().map_or((false, false), |d| (d.can_undo(), d.can_redo()));
                if ui.add_enabled(can_undo, egui::Button::new("Undo").shortcut_text("Ctrl+Z")).clicked()
                    && let Some(d) = self.doc.as_mut()
                {
                    d.undo();
                }
                if ui.add_enabled(can_redo, egui::Button::new("Redo").shortcut_text("Ctrl+Y")).clicked()
                    && let Some(d) = self.doc.as_mut()
                {
                    d.redo();
                }
            });
            ui.menu_button("Radio", |ui| {
                if ui.add_enabled(!busy, egui::Button::new("Read from radio")).clicked() {
                    self.guarded(PendingAction::Read);
                }
                if ui.add_enabled(!busy && has_doc, egui::Button::new("Write to radio…")).clicked() {
                    self.dialog = Some(Dialog::ConfirmWrite);
                }
                ui.separator();
                if ui.add_enabled(!busy, egui::Button::new("Identify radio")).clicked() {
                    self.start_job("Identifying radio", Request::Probe);
                }
                if ui.button("Connection settings…").clicked() {
                    self.dialog = Some(Dialog::Connection { ports: dm32_proto::list_ports() });
                }
            });
            ui.menu_button("Help", |ui| {
                if ui.button("About").clicked() {
                    self.message(
                        "About",
                        format!(
                            "DM-32UV CPS {} (build {})\nOpen-source programming software for the Baofeng DM-32UV.\n\
                             Licensed under the GPL-3.0-or-later.\n\nData folder: {}",
                            env!("CARGO_PKG_VERSION"),
                            env!("DM32_BUILD"),
                            data_dir().display()
                        ),
                    );
                }
            });
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.dialog.take() else {
            return;
        };
        let mut keep = true;
        let mut next: Option<Dialog> = None;
        match &dialog {
            Dialog::Message { title, text } => {
                egui::Modal::new(egui::Id::new("message")).show(ctx, |ui| {
                    ui.set_max_width(520.0);
                    ui.heading(title);
                    ui.add_space(6.0);
                    ui.label(text);
                    ui.add_space(6.0);
                    if ui.button("OK").clicked() {
                        keep = false;
                    }
                });
            }
            Dialog::ConfirmDiscard(action) => {
                egui::Modal::new(egui::Id::new("discard")).show(ctx, |ui| {
                    ui.heading("Unsaved changes");
                    ui.label("The current codeplug has unsaved changes.");
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            self.save(false);
                            keep = false;
                            if !self.dirty() {
                                self.perform(action.clone());
                            }
                        }
                        if ui.button("Discard").clicked() {
                            keep = false;
                            if let Some(d) = self.doc.as_mut() {
                                d.dirty = false;
                            }
                            self.perform(action.clone());
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            }
            Dialog::ConfirmWrite => {
                egui::Modal::new(egui::Id::new("write")).show(ctx, |ui| {
                    ui.set_max_width(480.0);
                    ui.heading("Write to radio");
                    ui.label(
                        "Only blocks that differ from what is on the radio are written. The radio's current \
                         contents are read and saved as a backup first.",
                    );
                    ui.checkbox(&mut self.prefs.verify, "Verify by reading back");
                    ui.checkbox(&mut self.prefs.skip_settings, "Do not change radio settings");
                    ui.horizontal(|ui| {
                        if ui.button("Write").clicked() {
                            keep = false;
                            self.start_write();
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            }
            Dialog::Connection { ports } => {
                let mut ports = ports.clone();
                egui::Modal::new(egui::Id::new("conn")).show(ctx, |ui| {
                    ui.heading("Connection settings");
                    egui::Grid::new("conn_grid").num_columns(2).show(ui, |ui| {
                        ui.label("Serial port");
                        ui.horizontal(|ui| {
                            egui::ComboBox::from_id_salt("port")
                                .selected_text(if self.prefs.port.is_empty() {
                                    "Automatic"
                                } else {
                                    &self.prefs.port
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut self.prefs.port,
                                        String::new(),
                                        "Automatic",
                                    );
                                    for p in &ports {
                                        ui.selectable_value(&mut self.prefs.port, p.clone(), p);
                                    }
                                });
                            if ui.button("⟳").on_hover_text("Rescan ports").clicked() {
                                ports = dm32_proto::list_ports();
                            }
                        });
                        ui.end_row();
                        for (label, value) in
                            [("RTS", &mut self.prefs.rts), ("DTR", &mut self.prefs.dtr)]
                        {
                            ui.label(label);
                            ui.horizontal(|ui| {
                                for (v, text) in
                                    [("default", "Default"), ("high", "High"), ("low", "Low")]
                                {
                                    ui.radio_value(value, v.to_string(), text);
                                }
                            });
                            ui.end_row();
                        }
                    });
                    ui.label(
                        egui::RichText::new(
                            "Firmware NRF.01.049 is reported to need RTS high and DTR low.",
                        )
                        .small(),
                    );
                    ui.checkbox(
                        &mut self.prefs.read_contacts,
                        "Read the DMR contact database too (slow)",
                    );
                    ui.checkbox(
                        &mut self.prefs.trace,
                        "Log serial traffic (for bug reports)",
                    );
                    if ui.button("Close").clicked() {
                        keep = false;
                    }
                });
                if keep {
                    next = Some(Dialog::Connection { ports });
                }
            }
        }
        if keep {
            self.dialog = Some(next.unwrap_or(dialog));
        }
    }

    fn job_window(&mut self, ctx: &egui::Context) {
        let Some(job) = self.job.as_mut() else { return };
        if let Some(outcome) = job.poll() {
            self.job = None;
            self.finish_job(outcome);
            return;
        }
        egui::Modal::new(egui::Id::new("job")).show(ctx, |ui| {
            ui.set_min_width(360.0);
            ui.heading(&job.title);
            ui.label(&job.stage);
            match job.fraction {
                Some(f) => ui.add(egui::ProgressBar::new(f).show_percentage()),
                None => ui.add(egui::ProgressBar::new(0.0).animate(true)),
            };
            if ui
                .add_enabled(!job.is_cancelling(), egui::Button::new("Cancel"))
                .clicked()
            {
                job.cancel();
            }
        });
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Close requests: ask about unsaved changes first.
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty() && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.dialog = Some(Dialog::ConfirmDiscard(PendingAction::Quit));
        }
        if self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        let (save, undo, redo) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::S),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y),
            )
        });
        if save {
            self.save(false);
        }
        if let Some(d) = self.doc.as_mut() {
            if undo {
                d.undo();
            }
            if redo {
                d.redo();
            }
        }

        let title = match &self.doc {
            Some(d) => format!(
                "DM-32UV CPS — {}{}",
                d.path
                    .as_ref()
                    .map_or("untitled".into(), |p| p.display().to_string()),
                if d.dirty { " *" } else { "" }
            ),
            None => "DM-32UV CPS".into(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }

        egui::Panel::top("menu").show(ui, |ui| self.menu(ui));
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak(format!("build {}", env!("DM32_BUILD")));
                    if let Some(d) = &self.doc {
                        ui.label(format!("{} {}", d.image.info.model, d.image.info.firmware));
                    }
                });
            });
        });

        match self.doc.as_mut() {
            Some(doc) => {
                egui::Panel::left("nav")
                    .resizable(true)
                    .default_size(190.0)
                    .show(ui, |ui| {
                        views::nav(ui, &mut self.view);
                    });
                egui::CentralPanel::default().show(ui, |ui| {
                    views::show(ui, &mut self.view, doc);
                });
            }
            None => {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(ui.available_height() / 3.0);
                        ui.heading("DM-32UV CPS");
                        ui.add_space(12.0);
                        if ui.button("Read from radio").clicked() {
                            self.perform(PendingAction::Read);
                        }
                        if ui.button("Open file…").clicked() {
                            self.perform(PendingAction::Open);
                        }
                        if ui.button("New from factory defaults").clicked() {
                            self.perform(PendingAction::New);
                        }
                    });
                });
            }
        }

        self.dialogs(&ctx);
        self.job_window(&ctx);
        files::show(&ctx);
        self.file_results();
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, PREFS_KEY, &self.prefs);
    }
}

/// Folder for backups and traces.
pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("dm32cps");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    secs.to_string()
}
