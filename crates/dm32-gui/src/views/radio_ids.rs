//! The radio's own DMR IDs.

use dm32_codeplug::records::radio_id::{self, RadioId};
use eframe::egui;
use egui_extras::{Column, TableBuilder};

use super::Doc;
use super::talkgroups::MAX_DMR_ID;
use super::widgets;

const KEY: &str = "radio_ids";

pub fn show(ui: &mut egui::Ui, doc: &mut Doc) -> bool {
    let mut changed = false;
    let len = doc.cp.radio_ids.ids.len();
    let mut sel = widgets::selected(ui, KEY).filter(|&s| s < len);

    // Channels reference radio IDs by position, so entries are appended and never reordered.
    ui.horizontal(|ui| {
        if ui
            .add_enabled(len < radio_id::MAX, egui::Button::new("➕ Add"))
            .clicked()
        {
            let id = sel.map_or(1, |s| doc.cp.radio_ids.ids[s].id());
            doc.cp
                .radio_ids
                .ids
                .push(RadioId::new(id, &format!("Radio {}", len + 1)));
            sel = Some(len);
            changed = true;
        }
        if let Some(s) = sel
            && ui
                .button("🗑 Delete")
                .on_hover_text("Channels using it fall back to the default ID")
                .clicked()
        {
            doc.cp.delete_radio_id(s);
            let n = doc.cp.radio_ids.ids.len();
            sel = if n == 0 { None } else { Some(s.min(n - 1)) };
            changed = true;
        }
        ui.label(format!(
            "{} / {}",
            doc.cp.radio_ids.ids.len(),
            radio_id::MAX
        ));
        if doc.cp.radio_ids.ids.is_empty() {
            ui.colored_label(ui.visuals().warn_fg_color, "⚠ Add your DMR ID");
        }
    });
    let order: Vec<usize> = (0..doc.cp.radio_ids.ids.len()).collect();
    let moved = widgets::arrow_nav(ui, sel, &order);
    if moved.is_some() {
        sel = moved;
    }
    widgets::set_selected(ui, KEY, sel);

    egui::Panel::right("radio_id_detail")
        .resizable(true)
        .default_size(320.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            match sel {
                Some(i) => changed |= detail(ui, doc, i),
                None => {
                    ui.label("Select a radio ID to edit it.");
                }
            }
        });

    let row_h = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
    let ids = &doc.cp.radio_ids.ids;
    let mut clicked = None;
    super::talkgroups::table_area(ui, |ui| {
        widgets::scrolled(TableBuilder::new(ui), moved)
            .id_salt(KEY)
            .striped(true)
            .resizable(true)
            .sense(egui::Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::exact(44.0))
            .column(Column::initial(100.0))
            .column(Column::remainder().at_least(80.0))
            .header(row_h, |mut h| {
                for t in ["No.", "ID", "Name"] {
                    h.col(|ui| {
                        ui.strong(t);
                    });
                }
            })
            .body(|body| {
                body.rows(row_h, ids.len(), |mut row| {
                    let i = row.index();
                    let r = &ids[i];
                    row.set_selected(sel == Some(i));
                    row.col(|ui| {
                        ui.label((i + 1).to_string());
                    });
                    row.col(|ui| {
                        ui.monospace(r.id().to_string());
                    });
                    row.col(|ui| {
                        ui.label(r.name());
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
        .filter(|e| !e.is_empty() && e.channel.radio_id() == Some(i as u8))
        .count();
    let r = &mut doc.cp.radio_ids.ids[i];
    let mut changed = false;
    ui.heading(format!("Radio ID {}", i + 1));
    egui::Grid::new("rid_detail")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            changed |= text_row!(ui, "Name", r, name, set_name, RadioId::NAME_LEN);
            ui.label("ID");
            let mut id = r.id();
            if ui
                .add(egui::DragValue::new(&mut id).range(1..=MAX_DMR_ID))
                .changed()
            {
                r.set_id(id);
                changed = true;
            }
            ui.end_row();
        });
    ui.add_space(6.0);
    ui.label(match users {
        0 => "Not selected by any channel.".to_string(),
        1 => "Selected by 1 channel.".to_string(),
        n => format!("Selected by {n} channels."),
    });
    changed
}
