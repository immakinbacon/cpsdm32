//! DMR roaming: roam channels and roam zones.

use dm32_codeplug::Freq;
use dm32_codeplug::records::roaming::{self, RoamChannel, RoamZone};
use eframe::egui;
use egui_extras::{Column, TableBuilder};

use super::Doc;
use super::widgets::{self, ListAction};
use super::zones::{member_list, picker};

const CH_KEY: &str = "roam_channels";
const ZONE_KEY: &str = "roam_zones";
const MEMBER_KEY: &str = "roam_member";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Channels,
    Zones,
}

pub fn show(ui: &mut egui::Ui, doc: &mut Doc) -> bool {
    let tab_id = egui::Id::new("roaming_tab");
    let mut tab: Tab = ui.data(|d| d.get_temp(tab_id)).unwrap_or_default();
    ui.horizontal(|ui| {
        ui.selectable_value(&mut tab, Tab::Channels, "Roaming Channel");
        ui.selectable_value(&mut tab, Tab::Zones, "Roaming Zone");
    });
    ui.data_mut(|d| d.insert_temp(tab_id, tab));
    ui.separator();
    match tab {
        Tab::Channels => channels(ui, doc),
        Tab::Zones => zones(ui, doc),
    }
}

/// Remap roam zone members after the roam channel list changed (`map` takes an old 0-based
/// index and returns the new one, or `None` if the channel is gone).
fn remap_members(doc: &mut Doc, map: impl Fn(usize) -> Option<usize>) {
    for z in &mut doc.cp.roam_zones.zones {
        let old = z.channels();
        let new: Vec<usize> = old.iter().filter_map(|&c| map(c)).collect();
        if new != old {
            z.set_channels(&new);
        }
    }
}

fn channels(ui: &mut egui::Ui, doc: &mut Doc) -> bool {
    let mut changed = false;
    let sel = widgets::selected(ui, CH_KEY);
    let len = doc.cp.roam_channels.channels.len();
    if let Some(act) = widgets::list_buttons(ui, sel, len, roaming::MAX_CHANNELS) {
        changed = true;
        let list = &mut doc.cp.roam_channels.channels;
        match act {
            ListAction::Add => {
                let at = sel.map_or(len, |s| s + 1);
                let mut c = sel
                    .map(|s| list[s].clone())
                    .unwrap_or_else(|| RoamChannel::new("", Freq(43_000_000), Freq(43_000_000)));
                c.set_name(&format!("Roam CH {}", len + 1));
                list.insert(at, c);
                remap_members(doc, |c| Some(if c >= at { c + 1 } else { c }));
                widgets::set_selected(ui, CH_KEY, Some(at));
            }
            ListAction::Duplicate => {
                let s = sel.unwrap();
                let c = list[s].clone();
                list.insert(s + 1, c);
                remap_members(doc, |c| Some(if c > s { c + 1 } else { c }));
                widgets::set_selected(ui, CH_KEY, Some(s + 1));
            }
            ListAction::Delete(s) => {
                list.remove(s);
                remap_members(doc, |c| match c.cmp(&s) {
                    std::cmp::Ordering::Less => Some(c),
                    std::cmp::Ordering::Equal => None,
                    std::cmp::Ordering::Greater => Some(c - 1),
                });
                let n = doc.cp.roam_channels.channels.len();
                widgets::set_selected(ui, CH_KEY, if n == 0 { None } else { Some(s.min(n - 1)) });
            }
            ListAction::Up(s) | ListAction::Down(s) => {
                let t = if matches!(act, ListAction::Up(_)) {
                    s - 1
                } else {
                    s + 1
                };
                list.swap(s, t);
                remap_members(doc, |c| {
                    Some(if c == s {
                        t
                    } else if c == t {
                        s
                    } else {
                        c
                    })
                });
                widgets::set_selected(ui, CH_KEY, Some(t));
            }
        }
    }
    let mut sel = widgets::selected(ui, CH_KEY);
    let order: Vec<usize> = (0..doc.cp.roam_channels.channels.len()).collect();
    let moved = widgets::arrow_nav(ui, sel, &order);
    if moved.is_some() {
        sel = moved;
        widgets::set_selected(ui, CH_KEY, sel);
    }

    let row_h = ui.spacing().interact_size.y + 4.0;
    let list = &mut doc.cp.roam_channels.channels;
    let mut clicked = None;
    widgets::scrolled(TableBuilder::new(ui), moved)
        .striped(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::exact(44.0))
        .column(Column::initial(170.0).at_least(80.0))
        .column(Column::initial(110.0))
        .column(Column::initial(110.0))
        .column(Column::initial(90.0))
        .column(Column::remainder().at_least(90.0))
        .header(row_h, |mut h| {
            for t in [
                "No.",
                "Name",
                "RX Frequency",
                "TX Frequency",
                "Color Code",
                "Time Slot",
            ] {
                h.col(|ui| {
                    ui.strong(t);
                });
            }
        })
        .body(|body| {
            body.rows(row_h, list.len(), |mut row| {
                let i = row.index();
                let c = &mut list[i];
                row.set_selected(sel == Some(i));
                row.col(|ui| {
                    if ui
                        .selectable_label(sel == Some(i), (i + 1).to_string())
                        .clicked()
                    {
                        clicked = Some(i);
                    }
                });
                row.col(|ui| {
                    let mut n = c.name();
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut n)
                            .char_limit(RoamChannel::NAME_LEN)
                            .desired_width(f32::INFINITY),
                    );
                    if resp.changed() {
                        c.set_name(&dm32_codeplug::encoding::sanitize(
                            &n,
                            RoamChannel::NAME_LEN,
                        ));
                        changed = true;
                    }
                    if resp.gained_focus() {
                        clicked = Some(i);
                    }
                });
                row.col(|ui| {
                    let mut f = c.rx_freq();
                    if widgets::opt_freq(ui, ("roam_rx", i), &mut f) {
                        c.set_rx_freq(f);
                        changed = true;
                    }
                });
                row.col(|ui| {
                    let mut f = c.tx_freq();
                    if widgets::opt_freq(ui, ("roam_tx", i), &mut f) {
                        c.set_tx_freq(f);
                        changed = true;
                    }
                });
                row.col(|ui| {
                    let mut cc = c.color_code();
                    if widgets::number(ui, &mut cc, 0..=15) {
                        c.set_color_code(cc);
                        changed = true;
                    }
                });
                row.col(|ui| {
                    let mut ts = c.time_slot();
                    if widgets::choice(ui, ("roam_ts", i), &mut ts) {
                        c.set_time_slot(ts);
                        changed = true;
                    }
                });
            });
        });
    if clicked.is_some() {
        widgets::release_focus(ui);
        widgets::set_selected(ui, CH_KEY, clicked);
    }
    changed
}

fn zones(ui: &mut egui::Ui, doc: &mut Doc) -> bool {
    let mut changed = false;
    let zone_names: Vec<String> = doc.cp.roam_zones.zones.iter().map(|z| z.name()).collect();

    let h = &mut doc.cp.roam_zones.header;
    egui::Grid::new("roam_header")
        .num_columns(6)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            ui.label("Auto Roaming");
            let mut on = h.auto_roam();
            if ui.checkbox(&mut on, "").changed() {
                h.set_auto_roam(on);
                changed = true;
            }
            ui.label("Roaming Timer[min]");
            let mut d = h.roam_delay();
            if widgets::number(ui, &mut d, 0..=255) {
                h.set_roam_delay(d);
                changed = true;
            }
            ui.label("Roaming Zone");
            let mut z = h.default_zone().and_then(|z| u8::try_from(z).ok());
            if widgets::reference(ui, "roam_default_zone", &mut z, &zone_names, "None") {
                h.set_default_zone(z.map(usize::from));
                changed = true;
            }
            ui.end_row();
        });
    ui.separator();

    let sel = widgets::selected(ui, ZONE_KEY);
    let len = doc.cp.roam_zones.zones.len();
    if let Some(act) = widgets::list_buttons(ui, sel, len, roaming::MAX_ZONES) {
        changed = true;
        let rz = &mut doc.cp.roam_zones;
        let remap = |rz: &mut roaming::RoamZones, map: &dyn Fn(usize) -> Option<usize>| {
            let d = rz.header.default_zone();
            let n = d.and_then(map);
            if n != d {
                rz.header.set_default_zone(n);
            }
        };
        match act {
            ListAction::Add => {
                let at = sel.map_or(len, |s| s + 1);
                rz.zones
                    .insert(at, RoamZone::new(&format!("Roam Zone {}", len + 1)));
                remap(rz, &|z| Some(if z >= at { z + 1 } else { z }));
                widgets::set_selected(ui, ZONE_KEY, Some(at));
            }
            ListAction::Duplicate => {
                let s = sel.unwrap();
                let z = rz.zones[s].clone();
                rz.zones.insert(s + 1, z);
                remap(rz, &|z| Some(if z > s { z + 1 } else { z }));
                widgets::set_selected(ui, ZONE_KEY, Some(s + 1));
            }
            ListAction::Delete(s) => {
                rz.zones.remove(s);
                remap(rz, &|z| match z.cmp(&s) {
                    std::cmp::Ordering::Less => Some(z),
                    std::cmp::Ordering::Equal => None,
                    std::cmp::Ordering::Greater => Some(z - 1),
                });
                let n = rz.zones.len();
                widgets::set_selected(ui, ZONE_KEY, if n == 0 { None } else { Some(s.min(n - 1)) });
            }
            ListAction::Up(s) | ListAction::Down(s) => {
                let t = if matches!(act, ListAction::Up(_)) {
                    s - 1
                } else {
                    s + 1
                };
                rz.zones.swap(s, t);
                remap(rz, &|z| {
                    Some(if z == s {
                        t
                    } else if z == t {
                        s
                    } else {
                        z
                    })
                });
                widgets::set_selected(ui, ZONE_KEY, Some(t));
            }
        }
        widgets::set_selected(ui, MEMBER_KEY, None);
    }

    let names: Vec<String> = doc
        .cp
        .roam_zones
        .zones
        .iter()
        .map(|z| format!("{}  ({})", z.name(), z.channels().len()))
        .collect();
    egui::Panel::left("roam_zone_list")
        .resizable(true)
        .default_size(240.0)
        .show(ui, |ui| {
            let before = widgets::selected(ui, ZONE_KEY);
            if widgets::name_list(ui, ZONE_KEY, &names) != before {
                widgets::set_selected(ui, MEMBER_KEY, None);
            }
        });
    let sel = widgets::selected(ui, ZONE_KEY);
    egui::ScrollArea::vertical()
        .id_salt("roam_zone_list_detail")
        .show(ui, |ui| match sel {
            Some(i) if i < doc.cp.roam_zones.zones.len() => changed |= zone_detail(ui, doc, i),
            _ => {
                ui.label("Select a roaming zone to edit it.");
            }
        });
    changed
}

fn zone_detail(ui: &mut egui::Ui, doc: &mut Doc, i: usize) -> bool {
    let names: Vec<String> = doc
        .cp
        .roam_channels
        .channels
        .iter()
        .map(|c| c.name())
        .collect();
    let mut changed = false;
    let z = &mut doc.cp.roam_zones.zones[i];
    ui.heading(format!("Roaming zone {}", i + 1));
    egui::Grid::new("roam_zone_general")
        .num_columns(2)
        .show(ui, |ui| {
            changed |= text_row!(ui, "Zone Name", z, name, set_name, RoamZone::NAME_LEN);
        });

    ui.add_space(6.0);
    ui.columns(2, |cols| {
        let ui = &mut cols[0];
        ui.strong("Channel Member");
        let mut members = z.channels();
        let labels: Vec<String> = members
            .iter()
            .map(|&c| {
                names.get(c).map_or(format!("{}: (missing)", c + 1), |n| {
                    format!("{}: {n}", c + 1)
                })
            })
            .collect();
        if let Some(act) = member_list(ui, MEMBER_KEY, &labels, RoamZone::CHANNELS, 420.0) {
            act.apply(ui, MEMBER_KEY, &mut members);
            z.set_channels(&members);
            changed = true;
        }

        let ui = &mut cols[1];
        ui.strong("Available Channel");
        let full = members.len() >= RoamZone::CHANNELS;
        if full {
            ui.weak(format!(
                "The maximum number of channels added is {}.",
                RoamZone::CHANNELS
            ));
        }
        if names.is_empty() {
            ui.weak("Add roaming channels first.");
        }
        let picked = picker(
            ui,
            "roam_add",
            &names,
            |_| true,
            |c| members.contains(&c),
            !full,
            420.0,
        );
        if !picked.is_empty() {
            let room = RoamZone::CHANNELS.saturating_sub(members.len());
            members.extend(picked.iter().take(room));
            z.set_channels(&members);
            widgets::set_selected(ui, MEMBER_KEY, Some(members.len() - 1));
            changed = true;
        }
    });
    changed
}
