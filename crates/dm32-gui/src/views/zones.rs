//! Zones: the zone list, member channels and the current zone/channel selections.
//!
//! Also holds the member-list and searchable-picker widgets shared with the scan list and
//! roaming views.

use dm32_codeplug::records::zone::{self, Zone};
use eframe::egui;

use super::Doc;
use super::widgets::{self, ListAction};

const KEY: &str = "zones";
const MEMBER_KEY: &str = "zone_member";

/// An edit to an ordered member list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MemberAction {
    Remove(usize),
    Up(usize),
    Down(usize),
}

impl MemberAction {
    /// Apply the action to `list`, keeping the member selection (`key`) on the moved item.
    pub(super) fn apply<T>(self, ui: &egui::Ui, key: &str, list: &mut Vec<T>) {
        match self {
            MemberAction::Remove(i) => {
                list.remove(i);
                widgets::set_selected(
                    ui,
                    key,
                    if list.is_empty() {
                        None
                    } else {
                        Some(i.min(list.len() - 1))
                    },
                );
            }
            MemberAction::Up(i) => {
                list.swap(i, i - 1);
                widgets::set_selected(ui, key, Some(i - 1));
            }
            MemberAction::Down(i) => {
                list.swap(i, i + 1);
                widgets::set_selected(ui, key, Some(i + 1));
            }
        }
    }
}

/// A selectable list of member labels with Remove / Up / Down buttons.
pub(super) fn member_list(
    ui: &mut egui::Ui,
    key: &str,
    labels: &[String],
    max: usize,
    height: f32,
) -> Option<MemberAction> {
    let mut sel = widgets::selected(ui, key).filter(|&s| s < labels.len());
    let mut act = None;
    ui.horizontal(|ui| {
        let s = sel.unwrap_or(0);
        if ui
            .add_enabled(sel.is_some(), egui::Button::new("➖ Remove"))
            .clicked()
        {
            act = Some(MemberAction::Remove(s));
        }
        if ui
            .add_enabled(sel.is_some_and(|s| s > 0), egui::Button::new("⏶"))
            .on_hover_text("Move up")
            .clicked()
        {
            act = Some(MemberAction::Up(s));
        }
        if ui
            .add_enabled(
                sel.is_some_and(|s| s + 1 < labels.len()),
                egui::Button::new("⏷"),
            )
            .on_hover_text("Move down")
            .clicked()
        {
            act = Some(MemberAction::Down(s));
        }
        ui.label(format!("{} / {max}", labels.len()));
    });
    egui::Frame::group(ui.style()).show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt(("members", key))
            .max_height(height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                if labels.is_empty() {
                    ui.weak("No members.");
                }
                for (i, l) in labels.iter().enumerate() {
                    let resp = ui.selectable_label(sel == Some(i), l);
                    if resp.clicked() {
                        sel = Some(i);
                    }
                    if resp.double_clicked() {
                        act = Some(MemberAction::Remove(i));
                    }
                }
            });
    });
    widgets::set_selected(ui, key, sel);
    act
}

/// A filterable list of candidates (`"No: name"`) with an Add button. `visible(i)` hides
/// unusable entries, `taken(i)` greys out entries that are already members. Click selects,
/// Ctrl+click toggles, Shift+click selects a range. Returns the 0-based indexes to add, in
/// list order (Add button, or double-click for a single entry); empty if nothing to add.
pub(super) fn picker(
    ui: &mut egui::Ui,
    key: &str,
    names: &[String],
    visible: impl Fn(usize) -> bool,
    taken: impl Fn(usize) -> bool,
    can_add: bool,
    height: f32,
) -> Vec<usize> {
    let filter_id = egui::Id::new(("pick_filter", key));
    let pick_key = format!("{key}_pick");
    let mut filter: String = ui.data(|d| d.get_temp(filter_id)).unwrap_or_default();
    let mut sel = widgets::MultiSel::load(ui, &pick_key);
    sel.retain(|s| s < names.len() && visible(s) && !taken(s));
    let mut add = Vec::new();
    ui.horizontal(|ui| {
        ui.label("🔍");
        if ui
            .add(
                egui::TextEdit::singleline(&mut filter)
                    .hint_text("Filter")
                    .desired_width(140.0),
            )
            .changed()
        {
            ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
        }
        let label = match sel.items.len() {
            0 | 1 => "➕ Add".to_string(),
            n => format!("➕ Add {n}"),
        };
        if ui
            .add_enabled(can_add && !sel.items.is_empty(), egui::Button::new(label))
            .on_hover_text("Ctrl+click to pick several, Shift+click for a range")
            .clicked()
        {
            add = sel.items.iter().copied().collect();
        }
    });
    let needle = filter.trim().to_lowercase();
    let shown: Vec<usize> = (0..names.len())
        .filter(|&i| visible(i))
        .filter(|&i| {
            needle.is_empty()
                || format!("{}: {}", i + 1, names[i])
                    .to_lowercase()
                    .contains(&needle)
        })
        .collect();
    // Shift ranges skip entries that are already members.
    let selectable: Vec<usize> = shown.iter().copied().filter(|&i| !taken(i)).collect();
    let row_h = ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt(("pick", key))
            .max_height(height)
            .auto_shrink([false, true])
            .show_rows(ui, row_h, shown.len(), |ui, range| {
                for &i in &shown[range] {
                    let enabled = can_add && !taken(i);
                    let resp = ui.add_enabled(
                        enabled,
                        egui::Button::selectable(
                            sel.contains(i),
                            format!("{}: {}", i + 1, names[i]),
                        ),
                    );
                    if resp.clicked() {
                        let modifiers = ui.input(|inp| inp.modifiers);
                        sel.click(i, modifiers, &selectable);
                    }
                    if resp.double_clicked() {
                        add = vec![i];
                    }
                }
            });
    });
    if !add.is_empty() {
        sel = widgets::MultiSel::default();
    }
    sel.store(ui, &pick_key);
    add
}

/// A drop-down for a stored 1-based channel number (`0` = none), with a filter box.
pub(super) fn channel_combo(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut u16,
    names: &[String],
    none_label: &str,
) -> bool {
    let label = |v: u16| match v {
        0 => none_label.to_string(),
        n => names
            .get(n as usize - 1)
            .map_or(format!("#{n}"), |s| format!("{n}: {s}")),
    };
    let filter_id = egui::Id::new(("combo_filter", id));
    let mut changed = false;
    egui::ComboBox::from_id_salt(("chcombo", id))
        .selected_text(label(*value))
        .width(200.0)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show_ui(ui, |ui| {
            let mut filter: String = ui.data(|d| d.get_temp(filter_id)).unwrap_or_default();
            if ui
                .add(egui::TextEdit::singleline(&mut filter).hint_text("Filter"))
                .changed()
            {
                ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
            }
            let needle = filter.trim().to_lowercase();
            let shown: Vec<u16> = std::iter::once(0)
                .chain((1..=names.len() as u16).filter(|&n| names[n as usize - 1] != "(empty)"))
                .filter(|&n| needle.is_empty() || label(n).to_lowercase().contains(&needle))
                .collect();
            let row_h = ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y;
            egui::ScrollArea::vertical().max_height(300.0).show_rows(
                ui,
                row_h,
                shown.len(),
                |ui, range| {
                    for &n in &shown[range] {
                        if ui.selectable_label(*value == n, label(n)).clicked() {
                            if *value != n {
                                *value = n;
                                changed = true;
                            }
                            ui.close();
                        }
                    }
                },
            );
        });
    changed
}

/// Remap the 1-based current-zone selections in the zone header.
fn remap_header(doc: &mut Doc, map: impl Fn(usize) -> Option<usize>) {
    let h = &mut doc.cp.zones.header;
    let f = |v: Option<u8>| {
        v.and_then(|z| map(z as usize - 1))
            .and_then(|z| u8::try_from(z + 1).ok())
    };
    let (a, b) = (f(h.zone_a()), f(h.zone_b()));
    if a != h.zone_a() {
        h.set_zone_a(a);
    }
    if b != h.zone_b() {
        h.set_zone_b(b);
    }
}

pub fn show(ui: &mut egui::Ui, doc: &mut Doc) -> bool {
    let mut changed = false;

    changed |= header(ui, doc);
    ui.separator();

    let sel = widgets::selected(ui, KEY);
    let len = doc.cp.zones.zones.len();
    if let Some(act) = widgets::list_buttons(ui, sel, len, zone::MAX_ZONES) {
        changed = true;
        match act {
            ListAction::Add => {
                let at = sel.map_or(len, |s| s + 1);
                doc.cp
                    .zones
                    .zones
                    .insert(at, Zone::new(&format!("Zone {}", len + 1)));
                remap_header(doc, |z| Some(if z >= at { z + 1 } else { z }));
                widgets::set_selected(ui, KEY, Some(at));
            }
            ListAction::Duplicate => {
                let s = sel.unwrap();
                let z = doc.cp.zones.zones[s].clone();
                doc.cp.zones.zones.insert(s + 1, z);
                remap_header(doc, |z| Some(if z > s { z + 1 } else { z }));
                widgets::set_selected(ui, KEY, Some(s + 1));
            }
            ListAction::Delete(s) => {
                doc.cp.zones.zones.remove(s);
                remap_header(doc, |z| match z.cmp(&s) {
                    std::cmp::Ordering::Less => Some(z),
                    std::cmp::Ordering::Equal => None,
                    std::cmp::Ordering::Greater => Some(z - 1),
                });
                let n = doc.cp.zones.zones.len();
                widgets::set_selected(ui, KEY, if n == 0 { None } else { Some(s.min(n - 1)) });
            }
            ListAction::Up(s) | ListAction::Down(s) => {
                let t = if matches!(act, ListAction::Up(_)) {
                    s - 1
                } else {
                    s + 1
                };
                doc.cp.zones.zones.swap(s, t);
                remap_header(doc, |z| {
                    Some(if z == s {
                        t
                    } else if z == t {
                        s
                    } else {
                        z
                    })
                });
                widgets::set_selected(ui, KEY, Some(t));
            }
        }
        widgets::set_selected(ui, MEMBER_KEY, None);
    }

    let names: Vec<String> = doc
        .cp
        .zones
        .zones
        .iter()
        .map(|z| format!("{}  ({})", z.name(), z.channels().len()))
        .collect();
    egui::Panel::left("zone_list")
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
        .id_salt("zone_list_detail")
        .show(ui, |ui| match sel {
            Some(i) if i < doc.cp.zones.zones.len() => changed |= detail(ui, doc, i),
            _ => {
                ui.label("Select a zone to edit it.");
            }
        });
    changed
}

type Get = fn(&zone::ZoneHeader) -> Option<u8>;
type Set = fn(&mut zone::ZoneHeader, Option<u8>);

/// The radio's current zone and channel for the A and B sides.
///
/// The current channel is stored 1-based (0 = unset) in one byte. It is treated as the
/// position within the current zone: a byte cannot hold a channel number on a 4000-channel
/// radio, and a zone holds at most 64 channels. (The factory codeplug cannot tell the two
/// readings apart: its zone 1 is channels 1-16.)
fn header(ui: &mut egui::Ui, doc: &mut Doc) -> bool {
    let mut changed = false;
    let zone_names: Vec<String> = doc.cp.zones.zones.iter().map(|z| z.name()).collect();
    let channel_names = doc.channel_names();
    let members: Vec<Vec<String>> = doc
        .cp
        .zones
        .zones
        .iter()
        .map(|z| {
            z.channels()
                .iter()
                .map(|&c| {
                    let name = channel_names
                        .get(c as usize - 1)
                        .map_or("(missing)", |n| n.as_str());
                    format!("CH {c} {name}")
                })
                .collect()
        })
        .collect();
    let h = &mut doc.cp.zones.header;
    egui::CollapsingHeader::new("Current selections")
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("zone_header")
                .num_columns(4)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    let sides: [(&str, Get, Set, Get, Set); 2] = [
                        (
                            "A",
                            zone::ZoneHeader::zone_a,
                            zone::ZoneHeader::set_zone_a,
                            zone::ZoneHeader::channel_a,
                            zone::ZoneHeader::set_channel_a,
                        ),
                        (
                            "B",
                            zone::ZoneHeader::zone_b,
                            zone::ZoneHeader::set_zone_b,
                            zone::ZoneHeader::channel_b,
                            zone::ZoneHeader::set_channel_b,
                        ),
                    ];
                    for (side, zone_get, zone_set, ch_get, ch_set) in sides {
                        ui.label(format!("Current Zone {side}"));
                        let mut z = zone_get(h).map(|z| z - 1);
                        if widgets::reference(ui, ("cur_zone", side), &mut z, &zone_names, "None") {
                            zone_set(h, z.map(|z| z + 1));
                            // Start at the zone's first channel rather than a position it may not have.
                            ch_set(h, z.filter(|&z| !members[z as usize].is_empty()).map(|_| 1));
                            changed = true;
                        }
                        ui.label(format!("Current Channel {side}"));
                        match z.and_then(|z| members.get(z as usize)) {
                            Some(list) if !list.is_empty() => {
                                let mut c = ch_get(h).map(|c| c - 1);
                                if widgets::reference(ui, ("cur_ch", side), &mut c, list, "None") {
                                    ch_set(h, c.map(|c| c + 1));
                                    changed = true;
                                }
                            }
                            Some(_) => {
                                ui.weak("Zone has no channels");
                            }
                            None => {
                                ui.weak("Choose a zone first");
                            }
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
    let z = &mut doc.cp.zones.zones[i];
    ui.heading(format!("Zone {}", i + 1));
    egui::Grid::new("zone_general")
        .num_columns(2)
        .show(ui, |ui| {
            changed |= text_row!(ui, "Zone Name", z, name, set_name, zone::NAME_MAX);
        });

    ui.add_space(6.0);
    ui.columns(2, |cols| {
        let ui = &mut cols[0];
        ui.strong("Channel Members");
        let mut members = z.channels();
        let labels: Vec<String> = members
            .iter()
            .map(|&c| {
                names
                    .get(c as usize - 1)
                    .map_or(format!("{c}: (missing)"), |n| format!("{c}: {n}"))
            })
            .collect();
        if let Some(act) = member_list(ui, MEMBER_KEY, &labels, zone::MAX_CHANNELS, 420.0) {
            act.apply(ui, MEMBER_KEY, &mut members);
            z.set_channels(&members);
            changed = true;
        }

        let ui = &mut cols[1];
        ui.strong("Available Channel");
        let full = members.len() >= zone::MAX_CHANNELS;
        if full {
            ui.weak(format!(
                "A zone holds at most {} channels.",
                zone::MAX_CHANNELS
            ));
        }
        let taken = |c: usize| members.contains(&(c as u16 + 1));
        let picked = picker(ui, "zone_add", &names, |c| usable[c], taken, !full, 420.0);
        if !picked.is_empty() {
            let room = zone::MAX_CHANNELS.saturating_sub(members.len());
            members.extend(picked.iter().take(room).map(|&c| c as u16 + 1));
            z.set_channels(&members);
            widgets::set_selected(ui, MEMBER_KEY, Some(members.len() - 1));
            changed = true;
        }
    });
    changed
}
