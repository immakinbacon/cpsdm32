//! RX group lists: named sets of talk-group IDs a digital channel listens to.

use std::collections::HashMap;

use dm32_codeplug::records::rx_group::{self, RxGroup};
use eframe::egui;
use egui_extras::{Column, TableBuilder};

use super::Doc;
use super::talkgroups::{ALL_CALL_ID, MAX_DMR_ID};
use super::widgets;

const KEY: &str = "rx_groups";
const MEMBER_KEY: &str = "rx_group_member";

pub fn show(ui: &mut egui::Ui, doc: &mut Doc) -> bool {
    let mut changed = false;
    let slots = &doc.cp.rx_groups.slots;
    let count = slots.iter().filter(|s| s.is_some()).count();
    let mut sel = widgets::selected(ui, KEY).filter(|&s| slots.get(s).is_some_and(Option::is_some));
    let free = slots.iter().position(Option::is_none);
    let can_add = free.is_some() || slots.len() < rx_group::MAX_GROUPS;

    ui.horizontal(|ui| {
        if ui
            .add_enabled(can_add, egui::Button::new("➕ Add"))
            .clicked()
        {
            let slots = &mut doc.cp.rx_groups.slots;
            let at = free.unwrap_or(slots.len());
            let g = Some(RxGroup::new(&format!("RX Group {}", at + 1)));
            if at == slots.len() {
                slots.push(g);
            } else {
                slots[at] = g;
            }
            sel = Some(at);
            changed = true;
        }
        if let Some(s) = sel
            && ui
                .button("🗑 Delete")
                .on_hover_text("Channels using it get no RX group list")
                .clicked()
        {
            doc.cp.delete_rx_group(s);
            let slots = &mut doc.cp.rx_groups.slots;
            while slots.last().is_some_and(Option::is_none) {
                slots.pop();
            }
            sel = (0..slots.len())
                .filter(|&i| slots[i].is_some())
                .min_by_key(|&i| i.abs_diff(s));
            changed = true;
        }
        ui.label(format!("{count} / {}", rx_group::MAX_GROUPS));
    });
    let order: Vec<usize> = {
        let slots = &doc.cp.rx_groups.slots;
        (0..slots.len()).filter(|&i| slots[i].is_some()).collect()
    };
    let moved = widgets::arrow_nav(ui, sel, &order);
    if moved.is_some() {
        sel = moved;
    }
    let scroll_row = moved.and_then(|m| order.iter().position(|&x| x == m));
    widgets::set_selected(ui, KEY, sel);

    let tg_names: HashMap<u32, String> = doc
        .cp
        .talkgroups
        .iter()
        .map(|(_, t)| (t.id(), t.name()))
        .collect();

    egui::Panel::right("rx_group_detail")
        .resizable(true)
        .default_size(380.0)
        .show(ui, |ui| {
            // Fill the panel so it keeps its size instead of shrinking to the content.
            ui.set_min_width(ui.available_width());
            egui::ScrollArea::vertical().show(ui, |ui| match sel {
                Some(i) => changed |= detail(ui, doc, i, &tg_names),
                None => {
                    ui.label("Select an RX group list to edit it.");
                }
            });
        });

    let slots = &doc.cp.rx_groups.slots;
    let filled: Vec<usize> = (0..slots.len()).filter(|&i| slots[i].is_some()).collect();
    let row_h = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
    let mut clicked = None;
    super::talkgroups::table_area(ui, |ui| {
        widgets::scrolled(TableBuilder::new(ui), scroll_row)
            .id_salt(KEY)
            .striped(true)
            .resizable(true)
            .sense(egui::Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::exact(44.0))
            .column(Column::initial(120.0).at_least(60.0))
            .column(Column::initial(70.0))
            .column(Column::remainder().at_least(80.0))
            .header(row_h, |mut h| {
                for t in ["No.", "Name", "Members", "Contacts"] {
                    h.col(|ui| {
                        ui.strong(t);
                    });
                }
            })
            .body(|body| {
                body.rows(row_h, filled.len(), |mut row| {
                    let i = filled[row.index()];
                    let Some(g) = &slots[i] else { return };
                    let ids = g.ids();
                    row.set_selected(sel == Some(i));
                    row.col(|ui| {
                        ui.label((i + 1).to_string());
                    });
                    row.col(|ui| {
                        ui.label(g.name());
                    });
                    row.col(|ui| {
                        ui.label(ids.len().to_string());
                    });
                    row.col(|ui| {
                        let text: Vec<String> = ids
                            .iter()
                            .map(|id| tg_names.get(id).cloned().unwrap_or_else(|| id.to_string()))
                            .collect();
                        ui.add(egui::Label::new(text.join(", ")).truncate());
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

fn detail(ui: &mut egui::Ui, doc: &mut Doc, i: usize, tg_names: &HashMap<u32, String>) -> bool {
    let talkgroups: Vec<(u32, String)> = doc
        .cp
        .talkgroups
        .iter()
        .map(|(_, t)| (t.id(), t.name()))
        .collect();
    let users = doc
        .cp
        .channels
        .list
        .iter()
        .filter(|e| !e.is_empty() && e.channel.rx_group_list() == Some(Some(i as u8)))
        .count();
    let Some(g) = doc.cp.rx_groups.slots[i].as_mut() else {
        return false;
    };
    let mut changed = false;
    ui.heading(format!("RX group list {}", i + 1));
    egui::Grid::new("rxg_detail")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            changed |= text_row!(ui, "Name", g, name, set_name, rx_group::NAME_MAX);
        });
    ui.label(match users {
        0 => "Not used by any channel.".to_string(),
        1 => "Used by 1 channel.".to_string(),
        n => format!("Used by {n} channels."),
    });
    ui.separator();

    let mut ids = g.ids();
    let full = ids.len() >= rx_group::MAX_IDS;
    ui.strong(format!("Contacts ({} / {})", ids.len(), rx_group::MAX_IDS));

    // Add a talk group by name.
    ui.horizontal(|ui| {
        ui.add_enabled_ui(!full, |ui| {
            egui::ComboBox::from_id_salt("rxg_add_tg")
                .selected_text("Add contact…")
                .height(300.0)
                .show_ui(ui, |ui| {
                    for (id, name) in &talkgroups {
                        let present = ids.contains(id);
                        if ui
                            .add_enabled(
                                !present,
                                egui::Button::selectable(false, format!("{name} ({id})")),
                            )
                            .clicked()
                        {
                            ids.push(*id);
                            changed = true;
                        }
                    }
                });
        });
    });
    // Or by DMR ID.
    ui.horizontal(|ui| {
        let tid = egui::Id::new("rxg_add_id");
        let mut text: String = ui.data(|d| d.get_temp(tid)).unwrap_or_default();
        let resp = ui.add_enabled(
            !full,
            egui::TextEdit::singleline(&mut text)
                .hint_text("DMR ID")
                .desired_width(90.0),
        );
        let parsed = text
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|&v| (1..=MAX_DMR_ID).contains(&v) || v == ALL_CALL_ID);
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let ok = parsed.is_some_and(|v| !ids.contains(&v));
        if (ui
            .add_enabled(!full && ok, egui::Button::new("Add ID"))
            .clicked()
            || (enter && ok))
            && let Some(v) = parsed
        {
            ids.push(v);
            changed = true;
            text.clear();
        }
        if !text.is_empty() && parsed.is_none() {
            ui.colored_label(ui.visuals().warn_fg_color, "Invalid ID");
        }
        ui.data_mut(|d| d.insert_temp(tid, text));
    });
    ui.add_space(4.0);

    // Member list.
    let msel = widgets::selected(ui, MEMBER_KEY).filter(|&m| m < ids.len());
    let mut new_sel = msel;
    ui.horizontal(|ui| {
        if let Some(m) = msel {
            if ui.button("🗑 Remove").clicked() {
                ids.remove(m);
                new_sel = if ids.is_empty() {
                    None
                } else {
                    Some(m.min(ids.len() - 1))
                };
                changed = true;
            }
            if ui
                .add_enabled(m > 0, egui::Button::new("⏶"))
                .on_hover_text("Move up")
                .clicked()
            {
                ids.swap(m, m - 1);
                new_sel = Some(m - 1);
                changed = true;
            }
            if ui
                .add_enabled(m + 1 < ids.len(), egui::Button::new("⏷"))
                .on_hover_text("Move down")
                .clicked()
            {
                ids.swap(m, m + 1);
                new_sel = Some(m + 1);
                changed = true;
            }
        } else {
            ui.weak("Select a member to remove or move it.");
        }
    });
    egui::Grid::new("rxg_members")
        .num_columns(3)
        .striped(true)
        .show(ui, |ui| {
            for (n, id) in ids.iter().enumerate() {
                ui.label(format!("{}", n + 1));
                if ui
                    .selectable_label(new_sel == Some(n), id.to_string())
                    .clicked()
                {
                    new_sel = Some(n);
                }
                match tg_names.get(id) {
                    Some(name) => ui.label(name),
                    None => ui.weak("(not in contacts)"),
                };
                ui.end_row();
            }
        });
    widgets::set_selected(ui, MEMBER_KEY, new_sel);
    if changed && ids != g.ids() {
        g.set_ids(&ids);
    }
    changed
}
