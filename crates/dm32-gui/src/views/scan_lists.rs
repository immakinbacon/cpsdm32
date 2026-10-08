//! Scan lists and the global (VFO) scan settings.

use dm32_codeplug::records::scan_list::{self, PriorityMode, ScanList, ScanMember, ScanTxMode};
use eframe::egui;

use super::Doc;
use super::widgets::{self, ListAction};
use super::zones::{channel_combo, member_list, picker};

const KEY: &str = "scan_lists";
const MEMBER_KEY: &str = "scan_member";

/// Renumber the channels' scan list references (0-based); `None` clears the reference.
fn remap_refs(doc: &mut Doc, map: impl Fn(u8) -> Option<u8>) {
    for e in &mut doc.cp.channels.list {
        if let Some(i) = e.channel.scan_list() {
            let n = map(i);
            if n != Some(i) {
                e.channel.set_scan_list(n);
            }
        }
    }
}

pub fn show(ui: &mut egui::Ui, doc: &mut Doc) -> bool {
    let mut changed = global(ui, doc);
    ui.separator();

    let sel = widgets::selected(ui, KEY);
    let len = doc.cp.scan_lists.lists.len();
    if let Some(act) = widgets::list_buttons(ui, sel, len, scan_list::MAX_SCAN_LISTS) {
        changed = true;
        let lists = &mut doc.cp.scan_lists.lists;
        match act {
            ListAction::Add => {
                lists.push(ScanList::new(&format!("Scan {}", len + 1)));
                widgets::set_selected(ui, KEY, Some(len));
            }
            ListAction::Duplicate => {
                let s = sel.unwrap();
                let l = lists[s].clone();
                lists.insert(s + 1, l);
                let s = s as u8;
                remap_refs(doc, |i| Some(if i > s { i + 1 } else { i }));
                widgets::set_selected(ui, KEY, Some(s as usize + 1));
            }
            ListAction::Delete(s) => {
                doc.cp.delete_scan_list(s);
                let n = doc.cp.scan_lists.lists.len();
                widgets::set_selected(ui, KEY, if n == 0 { None } else { Some(s.min(n - 1)) });
            }
            ListAction::Up(s) | ListAction::Down(s) => {
                let t = if matches!(act, ListAction::Up(_)) {
                    s - 1
                } else {
                    s + 1
                };
                lists.swap(s, t);
                let (a, b) = (s as u8, t as u8);
                remap_refs(doc, |i| {
                    Some(if i == a {
                        b
                    } else if i == b {
                        a
                    } else {
                        i
                    })
                });
                widgets::set_selected(ui, KEY, Some(t));
            }
        }
        widgets::set_selected(ui, MEMBER_KEY, None);
    }

    let names: Vec<String> = doc
        .cp
        .scan_lists
        .lists
        .iter()
        .map(|z| format!("{}  ({})", z.name(), z.members().len()))
        .collect();
    egui::Panel::left("scan_list_list")
        .resizable(true)
        .default_size(240.0)
        .show(ui, |ui| {
            let before = widgets::selected(ui, KEY);
            if widgets::name_list(ui, KEY, &names) != before {
                widgets::set_selected(ui, MEMBER_KEY, None);
            }
        });
    let sel = widgets::selected(ui, KEY);
    egui::ScrollArea::vertical()
        .id_salt("scan_list_list_detail")
        .show(ui, |ui| match sel {
            Some(i) if i < doc.cp.scan_lists.lists.len() => changed |= detail(ui, doc, i),
            _ => {
                ui.label("Select a scan list to edit it.");
            }
        });
    changed
}

/// Block-wide VFO scan settings.
fn global(ui: &mut egui::Ui, doc: &mut Doc) -> bool {
    let mut changed = false;
    let s = &mut doc.cp.scan_lists;
    egui::CollapsingHeader::new("VFO Scan")
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("vfo_scan")
                .num_columns(4)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    ui.label("VFO scan mode");
                    let mut m = s.scan_mode();
                    if widgets::choice(ui, "vfo_scan_mode", &mut m) {
                        s.set_scan_mode(m);
                        changed = true;
                    }
                    ui.end_row();
                    for (band, get, set) in [
                        (
                            "VHF",
                            scan_list::ScanLists::vhf_range
                                as fn(&scan_list::ScanLists) -> (u16, u16),
                            scan_list::ScanLists::set_vhf_range
                                as fn(&mut scan_list::ScanLists, (u16, u16)),
                        ),
                        (
                            "UHF",
                            scan_list::ScanLists::uhf_range,
                            scan_list::ScanLists::set_uhf_range,
                        ),
                    ] {
                        let (mut lo, mut hi) = get(s);
                        ui.label(format!("Scan Freq Lower({band})(MHz)"));
                        let c1 = widgets::number(ui, &mut lo, 0..=hi);
                        ui.label(format!("Scan Freq Upper({band})(MHz)"));
                        let c2 = widgets::number(ui, &mut hi, lo..=9999);
                        if c1 || c2 {
                            set(s, (lo, hi));
                            changed = true;
                        }
                        ui.end_row();
                    }
                });
        });
    changed
}

fn detail(ui: &mut egui::Ui, doc: &mut Doc, i: usize) -> bool {
    let names = doc.channel_names();
    let usable: Vec<bool> = doc.cp.channels.list.iter().map(|e| !e.is_empty()).collect();
    let mut changed = false;
    let l = &mut doc.cp.scan_lists.lists[i];
    ui.heading(format!("Scan list {}", i + 1));

    egui::Grid::new("scan_general")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            changed |= text_row!(ui, "Scan Name", l, name, set_name, scan_list::NAME_MAX);
            changed |= choice_row!(ui, "CTC Scan Mode", l, ctc_mode, set_ctc_mode);
            changed |= choice_row!(ui, "Scan TX Mode", l, tx_mode, set_tx_mode);

            ui.label("Designated Tx Channel");
            ui.add_enabled_ui(l.tx_mode() == ScanTxMode::Designated, |ui| {
                let mut v = l.designated_tx_channel();
                if channel_combo(ui, "scan_designated", &mut v, &names, "None") {
                    l.set_designated_tx_channel(v);
                    changed = true;
                }
            })
            .response
            .on_hover_text("Stored as-is; the encoding of this field is unverified.");
            ui.end_row();

            ui.label("Hang Time[s]");
            ui.horizontal(|ui| {
                let mut v = l.hang_time();
                if ui
                    .add(
                        egui::DragValue::new(&mut v)
                            .range(0..=255)
                            .custom_formatter(|n, _| format!("{:.1}", n * 0.5))
                            .custom_parser(|s| {
                                s.trim().parse::<f64>().ok().map(|x| (x * 2.0).round())
                            }),
                    )
                    .on_hover_text("Stored in 0.5 s steps")
                    .changed()
                {
                    l.set_hang_time(v);
                    changed = true;
                }
            });
            ui.end_row();

            type PriGet = fn(&ScanList) -> PriorityMode;
            type PriSet = fn(&mut ScanList, PriorityMode);
            type ChGet = fn(&ScanList) -> u16;
            type ChSet = fn(&mut ScanList, u16);
            for (n, get_m, set_m, get_c, set_c) in [
                (
                    1,
                    ScanList::priority1_mode as PriGet,
                    ScanList::set_priority1_mode as PriSet,
                    ScanList::priority1_channel as ChGet,
                    ScanList::set_priority1_channel as ChSet,
                ),
                (
                    2,
                    ScanList::priority2_mode,
                    ScanList::set_priority2_mode,
                    ScanList::priority2_channel,
                    ScanList::set_priority2_channel,
                ),
            ] {
                ui.label(format!("Priority CH {n} Type"));
                let mut m = get_m(l);
                if widgets::choice(ui, ("scan_pri_mode", n), &mut m) {
                    set_m(l, m);
                    changed = true;
                }
                ui.end_row();
                ui.label(format!("Priority Channel {n}"));
                ui.add_enabled_ui(m == PriorityMode::Channel, |ui| {
                    let mut v = get_c(l);
                    if channel_combo(ui, &format!("scan_pri_ch{n}"), &mut v, &names, "None") {
                        set_c(l, v);
                        changed = true;
                    }
                });
                ui.end_row();
            }
        });

    ui.add_space(6.0);
    ui.columns(2, |cols| {
        let ui = &mut cols[0];
        ui.strong("Scan Channel Members");
        let mut members = l.members();
        if members.len() > scan_list::MAX_CHANNELS {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!(
                    "{} members stored; the radio accepts {}. Editing keeps only the first {}.",
                    members.len(),
                    scan_list::MAX_CHANNELS,
                    scan_list::MAX_CHANNELS
                ),
            );
        }
        let labels: Vec<String> = members
            .iter()
            .map(|m| match *m {
                ScanMember::Selected => "Selected channel".to_string(),
                ScanMember::Channel(c) => names
                    .get(c as usize - 1)
                    .map_or(format!("{c}: (missing)"), |n| format!("{c}: {n}")),
            })
            .collect();
        if let Some(act) = member_list(ui, MEMBER_KEY, &labels, scan_list::MAX_CHANNELS, 420.0) {
            act.apply(ui, MEMBER_KEY, &mut members);
            l.set_members(&members);
            changed = true;
        }

        let ui = &mut cols[1];
        ui.strong("Available Channel");
        let full = members.len() >= scan_list::MAX_CHANNELS;
        if full {
            ui.weak(format!(
                "A scan list holds at most {} channels.",
                scan_list::MAX_CHANNELS
            ));
        }
        let has_selected = members.contains(&ScanMember::Selected);
        if ui
            .add_enabled(
                !full && !has_selected,
                egui::Button::new("➕ Add \"Selected channel\""),
            )
            .clicked()
        {
            members.push(ScanMember::Selected);
            l.set_members(&members);
            widgets::set_selected(ui, MEMBER_KEY, Some(members.len() - 1));
            changed = true;
        }
        let taken = |c: usize| members.contains(&ScanMember::Channel(c as u16 + 1));
        let picked = picker(ui, "scan_add", &names, |c| usable[c], taken, !full, 420.0);
        if !picked.is_empty() {
            let room = scan_list::MAX_CHANNELS.saturating_sub(members.len());
            members.extend(
                picked
                    .iter()
                    .take(room)
                    .map(|&c| ScanMember::Channel(c as u16 + 1)),
            );
            l.set_members(&members);
            widgets::set_selected(ui, MEMBER_KEY, Some(members.len() - 1));
            changed = true;
        }
    });
    changed
}
