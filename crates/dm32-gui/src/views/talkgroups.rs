//! Talk groups — the CPS "Digital contacts" list (name / TG or DMR ID / call type).

use dm32_codeplug::records::Choice;
use dm32_codeplug::records::talkgroup::{self, CallType, TalkGroup};
use eframe::egui;
use egui_extras::{Column, TableBuilder};

use super::Doc;
use super::widgets;

const KEY: &str = "talkgroups";
/// Highest regular DMR ID; IDs above are reserved (All Call is 16777215).
pub const MAX_DMR_ID: u32 = 16_776_415;
pub const ALL_CALL_ID: u32 = 16_777_215;

/// The DMR ID editor shared with the RX group and radio ID views: returns `true` if changed.
pub fn dmr_id_row(ui: &mut egui::Ui, id: &mut u32) -> bool {
    ui.label("TG/DMR ID");
    let c = ui
        .add(egui::DragValue::new(id).range(1..=ALL_CALL_ID).speed(1.0))
        .changed();
    ui.end_row();
    if *id > MAX_DMR_ID && *id != ALL_CALL_ID {
        ui.label("");
        ui.colored_label(
            ui.visuals().warn_fg_color,
            format!("IDs above {MAX_DMR_ID} are reserved"),
        );
        ui.end_row();
    }
    c
}

/// Lay out a table in the space left of the side panels, clipped so its rows cannot take
/// clicks meant for the panel. Cell labels are not text-selectable, so clicking on text selects
/// the row.
pub fn table_area<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(ui, |ui| {
            ui.style_mut().interaction.selectable_labels = false;
            add(ui)
        })
        .inner
}

pub fn show(ui: &mut egui::Ui, doc: &mut Doc) -> bool {
    let mut changed = false;
    let slots = &doc.cp.talkgroups.slots;
    let filled: Vec<usize> = (0..slots.len()).filter(|&i| slots[i].is_some()).collect();
    let mut sel = widgets::selected(ui, KEY).filter(|&s| slots.get(s).is_some_and(Option::is_some));
    let free = slots.iter().position(Option::is_none);
    let can_add = free.is_some() || slots.len() < talkgroup::MAX_TALKGROUPS;

    ui.horizontal(|ui| {
        if ui
            .add_enabled(can_add, egui::Button::new("➕ Add"))
            .clicked()
        {
            let at = free.unwrap_or(doc.cp.talkgroups.slots.len());
            let template = sel.and_then(|s| doc.cp.talkgroups.slots[s].clone());
            let mut t = template.unwrap_or_else(|| TalkGroup::new("", 1, CallType::Group));
            t.set_name(&format!("Contact {}", at + 1));
            if at == doc.cp.talkgroups.slots.len() {
                doc.cp.talkgroups.slots.push(Some(t));
            } else {
                doc.cp.talkgroups.slots[at] = Some(t);
            }
            sel = Some(at);
            changed = true;
        }
        if let Some(s) = sel
            && ui
                .button("🗑 Delete")
                .on_hover_text("Channels transmitting to it get no TX contact")
                .clicked()
        {
            doc.cp.delete_talkgroup(s);
            // Trim trailing empty slots, as decoding does.
            while doc.cp.talkgroups.slots.last().is_some_and(Option::is_none) {
                doc.cp.talkgroups.slots.pop();
            }
            sel = filled
                .iter()
                .copied()
                .filter(|&i| i != s)
                .min_by_key(|&i| i.abs_diff(s));
            changed = true;
        }
        ui.label(format!("{} / {}", filled.len(), talkgroup::MAX_TALKGROUPS));
        if filled.len() <= 1 {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "⚠ The radio needs at least one contact",
            );
        }
    });
    let slots = &doc.cp.talkgroups.slots;
    let filled: Vec<usize> = (0..slots.len()).filter(|&i| slots[i].is_some()).collect();
    let moved = widgets::arrow_nav(ui, sel, &filled);
    if moved.is_some() {
        sel = moved;
    }
    let scroll_row = moved.and_then(|m| filled.iter().position(|&x| x == m));
    widgets::set_selected(ui, KEY, sel);

    egui::Panel::right("talkgroup_detail")
        .resizable(true)
        .default_size(320.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            match sel {
                Some(i) => changed |= detail(ui, doc, i),
                None => {
                    ui.label("Select a contact to edit it.");
                }
            }
        });

    let row_h = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
    let slots = &doc.cp.talkgroups.slots;
    let mut clicked = None;
    table_area(ui, |ui| {
        widgets::scrolled(TableBuilder::new(ui), scroll_row)
            .id_salt(KEY)
            .striped(true)
            .resizable(true)
            .sense(egui::Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::exact(44.0))
            .column(Column::initial(160.0).at_least(60.0))
            .column(Column::initial(100.0))
            .column(Column::remainder().at_least(80.0))
            .header(row_h, |mut h| {
                for t in ["No.", "Name", "TG/DMR ID", "Call Type"] {
                    h.col(|ui| {
                        ui.strong(t);
                    });
                }
            })
            .body(|body| {
                body.rows(row_h, filled.len(), |mut row| {
                    let i = filled[row.index()];
                    let Some(t) = &slots[i] else { return };
                    row.set_selected(sel == Some(i));
                    row.col(|ui| {
                        ui.label((i + 1).to_string());
                    });
                    row.col(|ui| {
                        ui.label(t.name());
                    });
                    row.col(|ui| {
                        ui.monospace(t.id().to_string());
                    });
                    row.col(|ui| {
                        ui.label(t.call_type().label());
                    });
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
    changed
}

fn detail(ui: &mut egui::Ui, doc: &mut Doc, i: usize) -> bool {
    let users = doc
        .cp
        .channels
        .list
        .iter()
        .filter(|e| !e.is_empty() && e.tx_contact.index() == Some(i as u16))
        .count();
    let Some(t) = doc.cp.talkgroups.slots[i].as_mut() else {
        return false;
    };
    let mut changed = false;
    ui.heading(format!("Contact {}", i + 1));
    egui::Grid::new("tg_detail")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            changed |= text_row!(ui, "Name", t, name, set_name, talkgroup::NAME_MAX);
            let mut id = t.id();
            if dmr_id_row(ui, &mut id) {
                t.set_id(id);
                changed = true;
            }
            ui.label("Call Type");
            let mut ct = t.call_type();
            if widgets::choice(ui, "tg_call_type", &mut ct) {
                t.set_call_type(ct);
                if ct == CallType::All {
                    t.set_id(ALL_CALL_ID);
                }
                changed = true;
            }
            ui.end_row();
        });
    ui.add_space(6.0);
    ui.label(match users {
        0 => "Not used as TX contact by any channel.".to_string(),
        1 => "TX contact of 1 channel.".to_string(),
        n => format!("TX contact of {n} channels."),
    });
    changed
}
