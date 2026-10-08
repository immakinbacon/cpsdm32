//! Channel list and channel editor.

use dm32_codeplug::Freq;
use dm32_codeplug::records::Choice;
use dm32_codeplug::records::channel::{self, Channel, ChannelEntry, ChannelMode};
use dm32_codeplug::records::zone;
use eframe::egui;
use egui_extras::{Column, TableBuilder};

use super::Doc;
use super::widgets::{self, ListAction, MultiSel};

const KEY: &str = "channels";

/// Select exactly one channel (or none).
fn select_one(ui: &egui::Ui, i: Option<usize>) {
    MultiSel::single(i).store(ui, KEY);
}

pub fn show(ui: &mut egui::Ui, doc: &mut Doc) -> bool {
    let mut changed = false;
    let len = doc.cp.channels.list.len();
    let mut msel = MultiSel::load(ui, KEY);
    msel.retain(|i| i < len);
    msel.store(ui, KEY);
    // Single-item actions (duplicate, move) apply when exactly one channel is selected.
    let sel = if msel.items.len() == 1 {
        msel.items.first().copied()
    } else {
        None
    };

    ui.horizontal(|ui| {
        if let Some(act) = widgets::list_buttons(ui, sel, len, channel::MAX_CHANNELS) {
            changed = true;
            match act {
                ListAction::Add => {
                    let at = msel.anchor.map_or(len, |s| s + 1).min(len);
                    let template = msel.anchor.map(|s| doc.cp.channels.list[s].clone());
                    let mut e = template.unwrap_or_else(|| {
                        ChannelEntry::new("", Freq(14_500_000), ChannelMode::Analog)
                    });
                    e.channel.set_name(&format!("Channel {}", at + 1));
                    doc.cp.insert_channel(at, e);
                    select_one(ui, Some(at));
                }
                ListAction::Duplicate => {
                    let s = sel.unwrap();
                    let e = doc.cp.channels.list[s].clone();
                    doc.cp.insert_channel(s + 1, e);
                    select_one(ui, Some(s + 1));
                }
                ListAction::Delete(s) => {
                    doc.cp.delete_channel(s);
                    let n = doc.cp.channels.list.len();
                    select_one(ui, if n == 0 { None } else { Some(s.min(n - 1)) });
                }
                ListAction::Up(s) => {
                    doc.cp.move_channel(s, s - 1);
                    select_one(ui, Some(s - 1));
                }
                ListAction::Down(s) => {
                    doc.cp.move_channel(s, s + 1);
                    select_one(ui, Some(s + 1));
                }
            }
        }
        changed |= bulk_actions(ui, doc, &msel);
    });
    let msel = MultiSel::load(ui, KEY);
    let focus = msel
        .anchor
        .filter(|a| msel.contains(*a))
        .or(msel.items.first().copied());
    // Arrow keys move the selection (to a single channel).
    let order: Vec<usize> = (0..doc.cp.channels.list.len()).collect();
    let moved = widgets::arrow_nav(ui, focus, &order);
    let focus = if moved.is_some() {
        select_one(ui, moved);
        moved
    } else {
        focus
    };
    let msel = MultiSel::load(ui, KEY);

    egui::Panel::right("channel_detail")
        .resizable(true)
        .default_size(380.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            egui::ScrollArea::vertical().show(ui, |ui| match focus {
                Some(i) if i < doc.cp.channels.list.len() => changed |= detail(ui, doc, i),
                _ => {
                    ui.label("Select a channel to edit it.");
                    ui.weak("Ctrl+click selects several channels, Shift+click a range.");
                }
            });
        });

    let talkgroups: Vec<String> = (0..doc.cp.talkgroups.slots.len())
        .map(|i| {
            doc.cp.talkgroups.slots[i]
                .as_ref()
                .map_or(String::new(), |t| t.name())
        })
        .collect();

    let row_h = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
    let list = &doc.cp.channels.list;
    let mut clicked = None;
    super::talkgroups::table_area(ui, |ui| {
        widgets::scrolled(TableBuilder::new(ui), moved)
            .id_salt(KEY)
            .striped(true)
            .resizable(true)
            .sense(egui::Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::exact(44.0))
            .column(Column::initial(140.0).at_least(60.0))
            .column(Column::initial(64.0))
            .column(Column::initial(84.0))
            .column(Column::initial(84.0))
            .column(Column::initial(56.0))
            .column(Column::initial(90.0))
            .column(Column::initial(90.0))
            .column(Column::remainder().at_least(80.0))
            .header(row_h, |mut h| {
                for t in [
                    "No.",
                    "Name",
                    "Mode",
                    "RX MHz",
                    "TX MHz",
                    "Power",
                    "RX tone / CC",
                    "TX tone / slot",
                    "TX contact",
                ] {
                    h.col(|ui| {
                        ui.strong(t);
                    });
                }
            })
            .body(|body| {
                body.rows(row_h, list.len(), |mut row| {
                    let i = row.index();
                    let e = &list[i];
                    row.set_selected(msel.contains(i));
                    let empty = e.is_empty();
                    row.col(|ui| {
                        ui.label((i + 1).to_string());
                    });
                    row.col(|ui| {
                        ui.label(if empty { "(empty)".into() } else { e.name() });
                    });
                    if empty {
                        for _ in 0..7 {
                            row.col(|_| {});
                        }
                    } else {
                        let ch: &Channel = &e.channel;
                        row.col(|ui| {
                            ui.label(ch.mode().label());
                        });
                        row.col(|ui| {
                            ui.monospace(ch.rx_freq().to_string());
                        });
                        row.col(|ui| {
                            ui.monospace(ch.tx_freq().map_or("—".into(), |f| f.to_string()));
                        });
                        row.col(|ui| {
                            ui.label(ch.power().label());
                        });
                        let digital = ch.mode().is_digital();
                        row.col(|ui| {
                            ui.label(if digital {
                                format!("CC {}", ch.color_code().unwrap_or(0))
                            } else {
                                ch.rx_tone().to_string()
                            });
                        });
                        row.col(|ui| {
                            ui.label(if digital {
                                ch.time_slot().map_or(String::new(), |t| t.label())
                            } else {
                                ch.tx_tone().to_string()
                            });
                        });
                        row.col(|ui| {
                            if digital && let Some(t) = e.tx_contact.index() {
                                ui.label(
                                    talkgroups
                                        .get(t as usize)
                                        .cloned()
                                        .unwrap_or_else(|| format!("#{}", t + 1)),
                                );
                            }
                        });
                    }
                    if row.response().clicked() {
                        clicked = Some((i, row.response().ctx.input(|inp| inp.modifiers)));
                    }
                });
            });
    });
    if let Some((i, modifiers)) = clicked {
        widgets::release_focus(ui);
        let mut msel = MultiSel::load(ui, KEY);
        let order: Vec<usize> = (0..doc.cp.channels.list.len()).collect();
        msel.click(i, modifiers, &order);
        msel.store(ui, KEY);
    }
    changed
}

fn names<T>(slots: impl Iterator<Item = Option<T>>, f: impl Fn(T) -> String) -> Vec<String> {
    slots.map(|s| s.map_or(String::new(), &f)).collect()
}

fn detail(ui: &mut egui::Ui, doc: &mut Doc, i: usize) -> bool {
    let scan_lists: Vec<String> = doc.cp.scan_lists.lists.iter().map(|l| l.name()).collect();
    let rx_groups = names(doc.cp.rx_groups.slots.iter().map(|s| s.as_ref()), |g| {
        g.name()
    });
    let talkgroups = names(doc.cp.talkgroups.slots.iter().map(|s| s.as_ref()), |t| {
        format!("{} ({})", t.name(), t.id())
    });
    let radio_ids: Vec<String> = doc
        .cp
        .radio_ids
        .ids
        .iter()
        .map(|r| format!("{} ({})", r.name(), r.id()))
        .collect();
    let keys: Vec<String> = doc.cp.emergency.keys.iter().map(|k| k.name()).collect();
    let emergencies: Vec<String> = doc.cp.emergency.digital.iter().map(|d| d.name()).collect();

    let e = &mut doc.cp.channels.list[i];
    let mut changed = false;
    ui.heading(format!("Channel {}", i + 1));
    if e.is_empty() {
        ui.label("This slot is empty.");
        if ui.button("Create channel here").clicked() {
            *e = ChannelEntry::new(
                &format!("Channel {}", i + 1),
                Freq(14_500_000),
                ChannelMode::Analog,
            );
            return true;
        }
        return false;
    }

    egui::CollapsingHeader::new("General")
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("ch_general")
                .num_columns(2)
                .striped(true)
                .show(ui, |ui| {
                    let ch = &mut e.channel;
                    changed |= text_row!(ui, "Name", ch, name, set_name, 16);
                    changed |= choice_row!(ui, "Channel type", ch, mode, set_mode);
                    ui.label("RX frequency (MHz)");
                    let mut rx = ch.rx_freq();
                    if widgets::freq(ui, ("rx", i), &mut rx) {
                        ch.set_rx_freq(rx);
                        changed = true;
                    }
                    ui.end_row();
                    ui.label("TX frequency (MHz)");
                    let mut tx = ch.tx_freq();
                    if widgets::opt_freq(ui, ("tx", i), &mut tx) {
                        ch.set_tx_freq(tx);
                        changed = true;
                    }
                    ui.end_row();
                    changed |= choice_row!(ui, "Power", ch, power, set_power);
                    changed |= choice_row!(ui, "Bandwidth", ch, bandwidth, set_bandwidth);
                    changed |= choice_row!(ui, "Step", ch, step, set_step);
                    changed |= num_row!(ui, "Squelch level", ch, squelch, set_squelch, 0..=9);
                    changed |= bool_row!(ui, "RX only", ch, rx_only, set_rx_only);
                    changed |= bool_row!(ui, "Talkaround", ch, talkaround, set_talkaround);
                    changed |= bool_row!(ui, "Lone worker", ch, lone_worker, set_lone_worker);
                    changed |= bool_row!(ui, "VOX", ch, vox, set_vox);
                    ui.label("Scan list");
                    let mut sl = ch.scan_list();
                    if widgets::reference(ui, "scan_list", &mut sl, &scan_lists, "None") {
                        ch.set_scan_list(sl);
                        changed = true;
                    }
                    ui.end_row();
                    changed |= bool_row!(ui, "Auto scan", ch, auto_scan, set_auto_scan);
                });
        });

    let digital = e.channel.mode().is_digital();
    if digital {
        egui::CollapsingHeader::new("Digital")
            .default_open(true)
            .show(ui, |ui| {
                egui::Grid::new("ch_digital")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label("TX contact");
                        let mut tc = e.tx_contact.index().and_then(|v| u8::try_from(v).ok());
                        // Talk groups beyond 255 are picked by number.
                        if talkgroups.len() <= 256 {
                            if widgets::reference(ui, "tx_contact", &mut tc, &talkgroups, "None") {
                                e.tx_contact.set_index(tc.map(u16::from));
                                changed = true;
                            }
                        } else {
                            let mut n = e.tx_contact.index().map_or(0, |v| v + 1);
                            if widgets::number(ui, &mut n, 0..=talkgroups.len() as u16) {
                                e.tx_contact.set_index(n.checked_sub(1));
                                changed = true;
                            }
                        }
                        ui.end_row();
                        let ch = &mut e.channel;
                        if let Some(mut cc) = ch.color_code() {
                            ui.label("Colour code");
                            if widgets::number(ui, &mut cc, 0..=15) {
                                ch.set_color_code(cc);
                                changed = true;
                            }
                            ui.end_row();
                        }
                        if let Some(mut ts) = ch.time_slot() {
                            ui.label("Time slot");
                            if widgets::choice(ui, "ts", &mut ts) {
                                ch.set_time_slot(ts);
                                changed = true;
                            }
                            ui.end_row();
                        }
                        if let Some(mut g) = ch.rx_group_list() {
                            ui.label("RX group list");
                            if widgets::reference(ui, "rxg", &mut g, &rx_groups, "None") {
                                ch.set_rx_group_list(g);
                                changed = true;
                            }
                            ui.end_row();
                        }
                        ui.label("Radio ID");
                        let mut rid = ch.radio_id();
                        if widgets::reference(ui, "rid", &mut rid, &radio_ids, "Default") {
                            ch.set_radio_id(rid);
                            changed = true;
                        }
                        ui.end_row();
                        if let Some(mut a) = ch.digital_tx_admit() {
                            ui.label("TX admit");
                            if widgets::choice(ui, "dadmit", &mut a) {
                                ch.set_digital_tx_admit(a);
                                changed = true;
                            }
                            ui.end_row();
                        }
                        for (label, get, set) in [
                            (
                                "Private call confirm",
                                Channel::private_call_ack as fn(&Channel) -> Option<bool>,
                                Channel::set_private_call_ack as fn(&mut Channel, bool),
                            ),
                            (
                                "Data call confirm",
                                Channel::data_ack,
                                Channel::set_data_ack,
                            ),
                            ("DCDM", Channel::dcdm, Channel::set_dcdm),
                            ("Encryption", Channel::encryption, Channel::set_encryption),
                        ] {
                            if let Some(mut v) = get(ch) {
                                ui.label(label);
                                if ui.checkbox(&mut v, "").changed() {
                                    set(ch, v);
                                    changed = true;
                                }
                                ui.end_row();
                            }
                        }
                        if let Some(mut k) = ch.encryption_key() {
                            ui.label("Encryption key");
                            if widgets::reference(ui, "key", &mut k, &keys, "None") {
                                ch.set_encryption_key(k);
                                changed = true;
                            }
                            ui.end_row();
                        }
                        ui.label("Emergency system");
                        let mut es = ch.emergency_system();
                        if widgets::reference(ui, "emsys", &mut es, &emergencies, "None") {
                            ch.set_emergency_system(es);
                            changed = true;
                        }
                        ui.end_row();
                        changed |= bool_row!(
                            ui,
                            "Emergency indicator",
                            ch,
                            emergency_indicator,
                            set_emergency_indicator
                        );
                        changed |=
                            bool_row!(ui, "Emergency ACK", ch, emergency_ack, set_emergency_ack);
                        changed |= choice_row!(ui, "APRS report", ch, aprs_report, set_aprs_report);
                        changed |= bool_row!(ui, "APRS receive", ch, aprs_rx, set_aprs_rx);
                    });
            });
    } else {
        egui::CollapsingHeader::new("Analog")
            .default_open(true)
            .show(ui, |ui| {
                egui::Grid::new("ch_analog")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        let ch = &mut e.channel;
                        ui.label("RX CTCSS/DCS");
                        let mut t = ch.rx_tone();
                        if widgets::tone(ui, ("rxt", i), &mut t) {
                            ch.set_rx_tone(t);
                            changed = true;
                        }
                        ui.end_row();
                        ui.label("TX CTCSS/DCS");
                        let mut t = ch.tx_tone();
                        if widgets::tone(ui, ("txt", i), &mut t) {
                            ch.set_tx_tone(t);
                            changed = true;
                        }
                        ui.end_row();
                        changed |= choice_row!(
                            ui,
                            "RX squelch mode",
                            ch,
                            rx_squelch_mode,
                            set_rx_squelch_mode
                        );
                        changed |=
                            choice_row!(ui, "Optional signalling", ch, signaling, set_signaling);
                        changed |= choice_row!(ui, "PTT ID", ch, ptt_id, set_ptt_id);
                        if let Some(mut a) = ch.analog_tx_admit() {
                            ui.label("TX admit");
                            if widgets::choice(ui, "aadmit", &mut a) {
                                ch.set_analog_tx_admit(a);
                                changed = true;
                            }
                            ui.end_row();
                        }
                        for (label, get, set) in [
                            (
                                "Scramble",
                                Channel::scramble as fn(&Channel) -> Option<bool>,
                                Channel::set_scramble as fn(&mut Channel, bool),
                            ),
                            ("Compander", Channel::compander, Channel::set_compander),
                            (
                                "Show PTT ID",
                                Channel::ptt_id_display,
                                Channel::set_ptt_id_display,
                            ),
                        ] {
                            if let Some(mut v) = get(ch) {
                                ui.label(label);
                                if ui.checkbox(&mut v, "").changed() {
                                    set(ch, v);
                                    changed = true;
                                }
                                ui.end_row();
                            }
                        }
                        changed |= bool_row!(
                            ui,
                            "Emergency indicator",
                            ch,
                            emergency_indicator,
                            set_emergency_indicator
                        );
                    });
            });
    }
    changed |= zone_membership(ui, doc, i);
    changed
}

/// Checkboxes for the zones the channel belongs to. A channel in no zone cannot be selected on
/// the radio.
fn zone_membership(ui: &mut egui::Ui, doc: &mut Doc, i: usize) -> bool {
    let n = i as u16 + 1;
    let mut changed = false;
    egui::CollapsingHeader::new("Zones")
        .default_open(true)
        .show(ui, |ui| {
            let zones = &mut doc.cp.zones.zones;
            if zones.is_empty() {
                ui.weak("There are no zones yet (Channels → Zones).");
            } else if !zones.iter().any(|z| z.channels().contains(&n)) {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    "In no zone: the radio cannot select this channel.",
                );
            }
            for z in zones.iter_mut() {
                let mut members = z.channels();
                let mut on = members.contains(&n);
                let full = !on && members.len() >= zone::MAX_CHANNELS;
                let resp = ui
                    .add_enabled(!full, egui::Checkbox::new(&mut on, z.name()))
                    .on_disabled_hover_text(format!(
                        "Zone is full ({} channels)",
                        zone::MAX_CHANNELS
                    ));
                if resp.changed() {
                    if on {
                        members.push(n);
                    } else {
                        members.retain(|&c| c != n);
                    }
                    z.set_channels(&members);
                    changed = true;
                }
            }
        });
    changed
}

/// Actions on all selected channels: add to a zone, delete.
fn bulk_actions(ui: &mut egui::Ui, doc: &mut Doc, msel: &MultiSel) -> bool {
    let n = msel.items.len();
    if n == 0 {
        return false;
    }
    let mut changed = false;
    // Blank slots cannot go into zones.
    let picked: Vec<u16> = msel
        .items
        .iter()
        .filter(|&&i| !doc.cp.channels.list[i].is_empty())
        .map(|&i| i as u16 + 1)
        .collect();
    ui.separator();
    let label = if n == 1 {
        "Add to zone".to_string()
    } else {
        format!("Add {n} to zone")
    };
    ui.add_enabled_ui(!picked.is_empty() && !doc.cp.zones.zones.is_empty(), |ui| {
        ui.menu_button(label, |ui| {
            for z in &mut doc.cp.zones.zones {
                let mut members = z.channels();
                let new: Vec<u16> = picked
                    .iter()
                    .copied()
                    .filter(|c| !members.contains(c))
                    .collect();
                let room = zone::MAX_CHANNELS.saturating_sub(members.len());
                let text = if new.is_empty() {
                    format!("{} (already in it)", z.name())
                } else if room < new.len() {
                    format!("{} ({} of {} fit)", z.name(), room, new.len())
                } else {
                    format!("{} (+{})", z.name(), new.len())
                };
                if ui
                    .add_enabled(!new.is_empty() && room > 0, egui::Button::new(text))
                    .clicked()
                {
                    members.extend(new.into_iter().take(room));
                    z.set_channels(&members);
                    changed = true;
                    ui.close();
                }
            }
        });
    });
    if n > 1 && ui.button(format!("🗑 Delete {n}")).clicked() {
        // From the end, so earlier indexes stay valid.
        for &i in msel.items.iter().rev() {
            doc.cp.delete_channel(i);
        }
        select_one(ui, None);
        changed = true;
    }
    changed
}
