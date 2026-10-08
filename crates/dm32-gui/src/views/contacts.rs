//! The DMR contact database (CPS "Big Contacts" / CSV contacts), up to 50,000 entries.
//!
//! Kept fast by virtualising the table (`body.rows`) and caching the filtered index list in egui
//! temp memory, keyed by the filter text, the list length and an edit generation.

use crate::files::{self, Purpose};
use std::sync::Arc;

use dm32_codeplug::records::contact::{self, Contact, CsvImport};
use eframe::egui;
use egui_extras::{Column, TableBuilder};

use super::Doc;
use super::talkgroups::MAX_DMR_ID;
use super::widgets;

const KEY: &str = "contacts";

fn id(name: &str) -> egui::Id {
    egui::Id::new(("contacts_view", name))
}

fn temp<T: Clone + Send + Sync + 'static>(ui: &egui::Ui, name: &str) -> Option<T> {
    ui.data(|d| d.get_temp(id(name)))
}

fn set_temp<T: Clone + Send + Sync + 'static>(ui: &egui::Ui, name: &str, v: T) {
    ui.data_mut(|d| d.insert_temp(id(name), v));
}

/// Filtered row indexes for one (filter, length, generation) key.
#[derive(Clone)]
struct FilterCache {
    filter: String,
    len: usize,
    generation: u64,
    rows: Arc<Vec<u32>>,
}

/// A parsed CSV file waiting for "replace" or "append".
#[derive(Clone)]
struct PendingImport {
    file: String,
    data: Arc<CsvImport>,
}

fn matches(c: &Contact, needle: &str, needle_id: Option<&str>) -> bool {
    if let Some(digits) = needle_id {
        return c.id().to_string().contains(digits);
    }
    c.callsign().to_ascii_lowercase().contains(needle)
        || c.name().to_ascii_lowercase().contains(needle)
}

/// Mark the list as edited (invalidates the filter cache) and set a status message.
fn bump(ui: &egui::Ui) {
    let g: u64 = temp(ui, "gen").unwrap_or(0);
    set_temp(ui, "gen", g + 1);
}

fn status(ui: &egui::Ui, msg: impl Into<String>) {
    set_temp(ui, "status", msg.into());
}

pub fn show(ui: &mut egui::Ui, doc: &mut Doc) -> bool {
    let max = contact::max_contacts(&doc.image);
    let mut edited = false;
    let mut filter: String = temp(ui, "filter").unwrap_or_default();
    let mut scroll_to: Option<usize> = None;

    // ---- Toolbar ----
    {
        let db = doc.contacts();
        let len = db.list.len();
        let mut sel = widgets::selected(ui, KEY).filter(|&s| s < len);
        ui.horizontal_wrapped(|ui| {
            ui.label("🔍");
            ui.add(
                egui::TextEdit::singleline(&mut filter)
                    .hint_text("Name, callsign or ID")
                    .desired_width(180.0),
            );
            if !filter.is_empty()
                && ui
                    .small_button("✖")
                    .on_hover_text("Clear the filter")
                    .clicked()
            {
                filter.clear();
            }
            ui.separator();
            if ui
                .add_enabled(len < max, egui::Button::new("➕ Add"))
                .clicked()
            {
                let new_id = db
                    .list
                    .iter()
                    .map(|c| c.id())
                    .max()
                    .map_or(1, |m| (m + 1).min(MAX_DMR_ID));
                db.list.push(Contact::new(new_id, "New contact"));
                sel = Some(len);
                scroll_to = sel;
                filter.clear();
                edited = true;
            }
            if let Some(s) = sel
                && ui.button("🗑 Delete").clicked()
            {
                db.list.remove(s);
                sel = if db.list.is_empty() {
                    None
                } else {
                    Some(s.min(db.list.len() - 1))
                };
                edited = true;
            }
            ui.separator();
            if ui
                .button("📂 Import CSV…")
                .on_hover_text("RadioID.net user.csv or CPS contact export")
                .clicked()
            {
                files::request(Purpose::ImportCsv);
            }
            if let Some(path) = files::take(Purpose::ImportCsv) {
                match std::fs::read(&path) {
                    Err(e) => status(ui, format!("Cannot read {}: {e}", path.display())),
                    Ok(bytes) => match contact::import_csv(&String::from_utf8_lossy(&bytes)) {
                        Err(e) => status(ui, format!("Import failed: {e}")),
                        Ok(data) => set_temp(
                            ui,
                            "pending",
                            Some(PendingImport {
                                file: path
                                    .file_name()
                                    .map_or_else(String::new, |f| f.to_string_lossy().into_owned()),
                                data: Arc::new(data),
                            }),
                        ),
                    },
                }
            }
            if ui
                .add_enabled(len > 0, egui::Button::new("💾 Export CSV…"))
                .clicked()
            {
                files::request(Purpose::ExportCsv);
            }
            if let Some(path) = files::take(Purpose::ExportCsv) {
                match std::fs::write(&path, contact::export_csv(db)) {
                    Ok(()) => status(ui, format!("Exported {len} contacts to {}", path.display())),
                    Err(e) => status(ui, format!("Cannot write {}: {e}", path.display())),
                }
            }
            ui.separator();
            if ui
                .add_enabled(len > 1, egui::Button::new("Sort by ID"))
                .clicked()
            {
                db.sort_by_id();
                sel = None;
                edited = true;
            }
            if ui
                .add_enabled(len > 1, egui::Button::new("Remove duplicate IDs"))
                .clicked()
            {
                db.dedup_ids();
                let removed = len - db.list.len();
                status(
                    ui,
                    format!(
                        "Removed {removed} duplicate{}",
                        if removed == 1 { "" } else { "s" }
                    ),
                );
                sel = None;
                edited = removed > 0;
            }
        });
        widgets::set_selected(ui, KEY, sel);

        // Capacity and status line.
        let len = db.list.len();
        ui.horizontal(|ui| {
            let frac = len as f32 / max as f32;
            ui.add(egui::ProgressBar::new(frac.min(1.0)).desired_width(160.0).text(format!("{len} / {max}")));
            if len > max {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("⚠ {} contacts over capacity; they will not be written", len - max),
                );
                if ui.button("Trim to capacity").clicked() {
                    db.list.truncate(max);
                    edited = true;
                }
            }
            ui.weak("ℹ The contact database is written to the radio only if it was changed (opt-in in the write dialog).");
        });
        if let Some(msg) = temp::<String>(ui, "status") {
            ui.horizontal(|ui| {
                ui.label(&msg);
                if ui.small_button("✖").clicked() {
                    ui.data_mut(|d| d.remove::<String>(id("status")));
                }
            });
        }
    }
    set_temp(ui, "filter", filter.clone());

    // ---- Pending import: replace or append ----
    if let Some(p) = temp::<Option<PendingImport>>(ui, "pending").flatten() {
        let mut close = false;
        let modal = egui::Modal::new(id("import_modal")).show(ui.ctx(), |ui| {
            ui.heading("Import contacts");
            let n = p.data.contacts.len();
            ui.label(format!("{}: {n} contacts read.", p.file));
            if !p.data.skipped.is_empty() {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("{} lines skipped.", p.data.skipped.len()),
                );
                egui::CollapsingHeader::new("Skipped lines").show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(150.0)
                        .show(ui, |ui| {
                            for (line, why) in p.data.skipped.iter().take(200) {
                                ui.label(format!("Line {line}: {why}"));
                            }
                            if p.data.skipped.len() > 200 {
                                ui.label("…");
                            }
                        });
                });
            }
            let current = doc.contacts().list.len();
            if n > max || current + n > max {
                ui.weak(format!(
                    "The radio holds {max} contacts; any beyond that are not written."
                ));
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .button(format!("Replace all ({current} → {n})"))
                    .clicked()
                {
                    doc.contacts().list = p.data.contacts.clone();
                    status(
                        ui,
                        format!(
                            "Replaced the contact list with {n} contacts from {}",
                            p.file
                        ),
                    );
                    edited = true;
                    close = true;
                }
                if ui
                    .button(format!("Append ({} total)", current + n))
                    .clicked()
                {
                    doc.contacts().list.extend(p.data.contacts.iter().cloned());
                    status(ui, format!("Appended {n} contacts from {}", p.file));
                    edited = true;
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
        if close || modal.should_close() {
            set_temp::<Option<PendingImport>>(ui, "pending", None);
            if edited {
                widgets::set_selected(ui, KEY, None);
            }
        }
    }

    if edited {
        bump(ui);
        doc.commit_contacts();
        edited = false;
    }

    // ---- Detail panel ----
    let sel = widgets::selected(ui, KEY);
    egui::Panel::right("contact_detail")
        .resizable(true)
        .default_size(300.0)
        .show(ui, |ui| {
            // Fill the panel so it keeps its size instead of shrinking to the content.
            ui.set_min_width(ui.available_width());
            let db = doc.contacts();
            match sel {
                Some(i) if i < db.list.len() => edited |= detail(ui, &mut db.list[i], i),
                _ => {
                    ui.label("Select a contact to edit it.");
                }
            }
        });
    if edited {
        bump(ui);
        doc.commit_contacts();
    }

    // ---- Filtered, virtualised table ----
    let db = doc.contacts();
    let generation: u64 = temp(ui, "gen").unwrap_or(0);
    let needle = filter.trim().to_ascii_lowercase();
    let rows: Option<Arc<Vec<u32>>> = if needle.is_empty() {
        None
    } else {
        let cached = temp::<FilterCache>(ui, "cache")
            .filter(|c| c.filter == needle && c.len == db.list.len() && c.generation == generation);
        Some(match cached {
            Some(c) => c.rows,
            None => {
                let digits = needle
                    .bytes()
                    .all(|b| b.is_ascii_digit())
                    .then_some(needle.as_str());
                let rows: Arc<Vec<u32>> = Arc::new(
                    (0..db.list.len() as u32)
                        .filter(|&i| matches(&db.list[i as usize], &needle, digits))
                        .collect(),
                );
                set_temp(
                    ui,
                    "cache",
                    FilterCache {
                        filter: needle.clone(),
                        len: db.list.len(),
                        generation,
                        rows: rows.clone(),
                    },
                );
                rows
            }
        })
    };
    let n_rows = rows.as_ref().map_or(db.list.len(), |r| r.len());
    // Arrow keys move through the (filtered) list; the order is only built on a key press.
    if widgets::nav_key_pressed(ui) {
        let order: Vec<usize> = match &rows {
            Some(r) => r.iter().map(|&i| i as usize).collect(),
            None => (0..db.list.len()).collect(),
        };
        let cur = widgets::selected(ui, KEY);
        if let Some(m) = widgets::arrow_nav(ui, cur, &order) {
            widgets::set_selected(ui, KEY, Some(m));
            scroll_to = order.iter().position(|&x| x == m);
            ui.ctx().request_repaint();
        }
    }
    if let Some(r) = &rows {
        ui.weak(format!("{} of {} contacts match", r.len(), db.list.len()));
    }

    let row_h = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
    let list = &db.list;
    let mut clicked = None;
    super::talkgroups::table_area(ui, |ui| {
        let mut table = TableBuilder::new(ui)
            .id_salt(KEY)
            .striped(true)
            .resizable(true)
            .sense(egui::Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::exact(52.0))
            .column(Column::initial(76.0))
            .column(Column::initial(70.0))
            .column(Column::initial(130.0).at_least(50.0))
            .column(Column::initial(100.0).at_least(40.0))
            .column(Column::initial(90.0).at_least(40.0))
            .column(Column::initial(100.0).at_least(40.0))
            .column(Column::remainder().at_least(60.0));
        if let Some(r) = scroll_to {
            table = table.scroll_to_row(r, Some(egui::Align::Center));
        }
        table
            .header(row_h, |mut h| {
                for t in [
                    "No.", "ID", "Callsign", "Name", "City", "State", "Country", "Remark",
                ] {
                    h.col(|ui| {
                        ui.strong(t);
                    });
                }
            })
            .body(|body| {
                body.rows(row_h, n_rows, |mut row| {
                    let i = rows
                        .as_ref()
                        .map_or(row.index(), |r| r[row.index()] as usize);
                    let c = &list[i];
                    row.set_selected(sel == Some(i));
                    row.col(|ui| {
                        ui.label((i + 1).to_string());
                    });
                    row.col(|ui| {
                        ui.monospace(c.id().to_string());
                    });
                    for s in [
                        c.callsign(),
                        c.name(),
                        c.city(),
                        c.state(),
                        c.country(),
                        c.remark(),
                    ] {
                        row.col(|ui| {
                            ui.add(egui::Label::new(s).truncate());
                        });
                    }
                    if row.response().clicked() {
                        clicked = Some(i);
                    }
                });
            });
    });
    if clicked.is_some() {
        widgets::release_focus(ui);
        widgets::set_selected(ui, KEY, clicked);
    }
    // The contact database is not part of `doc.cp`.
    false
}

/// A text row. While the field has focus the typed text is kept in a buffer, because the
/// contact setters trim whitespace (typing "John Smith" would otherwise lose the space).
fn text(ui: &mut egui::Ui, label: &str, value: String, max: usize, set: impl FnOnce(&str)) -> bool {
    ui.label(label);
    let buf = egui::Id::new(("contact_field", label));
    let mut v: String = ui.data(|d| d.get_temp(buf)).unwrap_or(value);
    let resp = ui.add(
        egui::TextEdit::singleline(&mut v)
            .char_limit(max)
            .desired_width(160.0),
    );
    let c = resp.changed();
    if c {
        v = dm32_codeplug::encoding::sanitize(&v, max);
        set(&v);
    }
    if resp.has_focus() {
        ui.data_mut(|d| d.insert_temp(buf, v));
    } else {
        ui.data_mut(|d| d.remove::<String>(buf));
    }
    ui.end_row();
    c
}

fn detail(ui: &mut egui::Ui, c: &mut Contact, i: usize) -> bool {
    let mut changed = false;
    ui.heading(format!("Contact {}", i + 1));
    egui::Grid::new("contact_detail_grid")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            ui.label("ID");
            let mut v = c.id();
            if ui
                .add(egui::DragValue::new(&mut v).range(1..=MAX_DMR_ID))
                .changed()
            {
                c.set_id(v);
                changed = true;
            }
            ui.end_row();
            // Field widths: 15 characters, callsign 7 (one byte is the terminator).
            changed |= text(ui, "Callsign", c.callsign(), 7, |s| c.set_callsign(s));
            changed |= text(ui, "Name", c.name(), 15, |s| c.set_name(s));
            changed |= text(ui, "City", c.city(), 15, |s| c.set_city(s));
            changed |= text(ui, "State / province", c.state(), 15, |s| c.set_state(s));
            changed |= text(ui, "Country", c.country(), 15, |s| c.set_country(s));
            changed |= text(ui, "Remark", c.remark(), 15, |s| c.set_remark(s));
        });
    changed
}
