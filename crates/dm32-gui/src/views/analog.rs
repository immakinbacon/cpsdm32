//! DTMF codes and settings, analog contacts and BDC1200 contacts (block 0x06).

use dm32_codeplug::records::analog::{self, AnalogContact, BdcContact, DtmfCode};
use eframe::egui;

use super::Doc;
use super::widgets::{self, ListAction};

const CONTACTS_KEY: &str = "analog_contacts";
const BDC_KEY: &str = "bdc_contacts";

pub fn show(ui: &mut egui::Ui, doc: &mut Doc) -> bool {
    let mut changed = false;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            egui::CollapsingHeader::new("DTMF")
                .default_open(true)
                .show(ui, |ui| {
                    changed |= dtmf(ui, doc);
                });
            egui::CollapsingHeader::new("Analog Contacts")
                .default_open(true)
                .show(ui, |ui| {
                    changed |= contacts(ui, doc);
                });
            egui::CollapsingHeader::new("BDC1200 Contacts")
                .default_open(true)
                .show(ui, |ui| {
                    changed |= bdc(ui, doc);
                });
        });
    changed
}

/// DTMF digit field: characters that are not DTMF digits (0-9, A-D, *, #) are refused.
fn dtmf_text(ui: &mut egui::Ui, value: &mut String, max: usize, width: f32) -> bool {
    let resp = ui.add(
        egui::TextEdit::singleline(value)
            .char_limit(max)
            .desired_width(width)
            .font(egui::TextStyle::Monospace),
    );
    if resp.changed() {
        *value = value
            .chars()
            .filter(|&c| analog::dtmf_value(c).is_some())
            .map(|c| c.to_ascii_uppercase())
            .take(max)
            .collect();
    }
    resp.on_hover_text("DTMF digits: 0-9, A-D, *, #").changed()
}

fn code_row(ui: &mut egui::Ui, label: &str, code: &mut DtmfCode) -> bool {
    ui.label(label);
    let mut v = code.digits();
    let c = dtmf_text(ui, &mut v, DtmfCode::LEN, 160.0);
    if c {
        code.set_digits(&v);
    }
    ui.end_row();
    c
}

fn dtmf(ui: &mut egui::Ui, doc: &mut Doc) -> bool {
    let a = &mut doc.cp.analog;
    let mut changed = false;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.strong("DTMF Encode");
            egui::Grid::new("dtmf_codes").striped(true).num_columns(2).show(ui, |ui| {
                for (i, code) in a.codes.iter_mut().enumerate() {
                    changed |= code_row(ui, &format!("{}", i + 1), code);
                }
            });
        });
        ui.add_space(24.0);
        ui.vertical(|ui| {
            ui.strong("DTMF Settings");
            egui::Grid::new("dtmf_settings").striped(true).num_columns(2).show(ui, |ui| {
                ui.label("Self ID Code");
                let mut v = a.settings.self_id();
                if dtmf_text(ui, &mut v, 3, 60.0) {
                    a.settings.set_self_id(&v);
                    changed = true;
                }
                ui.end_row();
                changed |= choice_row!(ui, "Group Code", a.settings, group_code, set_group_code);
                changed |= choice_row!(ui, "DTMF Interval Sign", a.settings, interval_sign, set_interval_sign);
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.strong("DTMF Special Call");
                ui.weak("(?)").on_hover_text("The order of these four codes is inferred from the CPS DTMF page, not confirmed on hardware.");
            });
            egui::Grid::new("dtmf_special").striped(true).num_columns(2).show(ui, |ui| {
                for (code, label) in a.special.iter_mut().zip(["PTT ID Up Encode", "PTT ID Down Encode", "Stun Code", "Kill Code"]) {
                    changed |= code_row(ui, label, code);
                }
            });
        });
    });
    changed
}

/// Number column that selects the row when clicked.
fn select_cell(ui: &mut egui::Ui, key: &str, i: usize, sel: Option<usize>) {
    if ui
        .selectable_label(sel == Some(i), format!("{:>3}", i + 1))
        .clicked()
    {
        widgets::set_selected(ui, key, Some(i));
    }
}

fn contacts(ui: &mut egui::Ui, doc: &mut Doc) -> bool {
    let list = &mut doc.cp.analog.contacts;
    let mut changed = false;
    let sel = widgets::selected(ui, CONTACTS_KEY).filter(|&s| s < list.len());
    if let Some(act) = widgets::list_buttons(ui, sel, list.len(), analog::MAX_CONTACTS) {
        let n = list.len();
        changed |= widgets::apply(ui, CONTACTS_KEY, list, act, || {
            AnalogContact::new(&format!("AContacts {}", n + 1))
        });
    }
    ui.weak("The contact ID is not decoded yet and is kept unchanged.");
    let sel = widgets::selected(ui, CONTACTS_KEY);
    egui::Grid::new("analog_contacts")
        .striped(true)
        .num_columns(2)
        .show(ui, |ui| {
            ui.strong("No.");
            ui.strong("Name");
            ui.end_row();
            for (i, c) in list.iter_mut().enumerate() {
                select_cell(ui, CONTACTS_KEY, i, sel);
                let mut name = c.name();
                if widgets::name(ui, &mut name, AnalogContact::NAME_LEN, 160.0) {
                    c.set_name(&name);
                    changed = true;
                }
                ui.end_row();
            }
        });
    changed
}

/// 4-digit hex ID field; the text being typed lives in egui memory and is applied when valid.
fn hex_id(ui: &mut egui::Ui, i: usize, value: &mut u16) -> bool {
    let id = egui::Id::new(("bdc_id", i));
    let mut text: String = ui
        .data(|d| d.get_temp(id))
        .unwrap_or_else(|| format!("{value:04X}"));
    let resp = ui.add(
        egui::TextEdit::singleline(&mut text)
            .char_limit(4)
            .desired_width(50.0)
            .font(egui::TextStyle::Monospace),
    );
    if resp.changed() {
        text = text
            .chars()
            .filter(|c| c.is_ascii_hexdigit())
            .map(|c| c.to_ascii_uppercase())
            .collect();
    }
    if resp.has_focus() || resp.changed() {
        ui.data_mut(|d| d.insert_temp(id, text.clone()));
    }
    let parsed = if text.is_empty() {
        None
    } else {
        u16::from_str_radix(&text, 16).ok()
    };
    let resp = if parsed.is_none() {
        resp.on_hover_text("Enter 1-4 hex digits")
    } else {
        resp.on_hover_text("Hex ID")
    };
    let mut changed = false;
    if resp.lost_focus() {
        if let Some(v) = parsed
            && v != *value
        {
            *value = v;
            changed = true;
        }
        ui.data_mut(|d| d.remove::<String>(id));
    } else if !resp.has_focus() {
        ui.data_mut(|d| d.remove::<String>(id));
    }
    changed
}

fn bdc(ui: &mut egui::Ui, doc: &mut Doc) -> bool {
    let list = &mut doc.cp.analog.bdc;
    let mut changed = false;
    let sel = widgets::selected(ui, BDC_KEY).filter(|&s| s < list.len());
    if let Some(act) = widgets::list_buttons(ui, sel, list.len(), analog::MAX_BDC) {
        let n = list.len();
        if act == ListAction::Add {
            let id = list
                .iter()
                .map(|c| c.id())
                .max()
                .map_or(0xE001, |m| m.wrapping_add(1));
            list.push(BdcContact::new(&format!("BDC Contacts {}", n + 1), id));
            widgets::set_selected(ui, BDC_KEY, Some(n));
            changed = true;
        } else {
            changed |= widgets::apply(ui, BDC_KEY, list, act, || BdcContact::new("", 0));
        }
    }
    let sel = widgets::selected(ui, BDC_KEY);
    egui::Grid::new("bdc_contacts")
        .striped(true)
        .num_columns(4)
        .show(ui, |ui| {
            for t in ["No.", "Name", "Type", "ID (hex)"] {
                ui.strong(t);
            }
            ui.end_row();
            for (i, c) in list.iter_mut().enumerate() {
                select_cell(ui, BDC_KEY, i, sel);
                let mut name = c.name();
                if wide(ui, 160.0, |ui| {
                    widgets::name(ui, &mut name, BdcContact::NAME_LEN, 160.0)
                }) {
                    c.set_name(&name);
                    changed = true;
                }
                let mut t = c.call_type();
                if widgets::choice(ui, ("bdc_type", i), &mut t) {
                    c.set_call_type(t);
                    changed = true;
                }
                let mut id = c.id();
                if hex_id(ui, i, &mut id) {
                    c.set_id(id);
                    changed = true;
                }
                ui.end_row();
            }
        });
    changed
}

/// Run `f` in a grid cell at least `w` wide (grid cells are otherwise only as wide as the
/// column was last frame, which squeezes text fields).
fn wide<R>(ui: &mut egui::Ui, w: f32, f: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope(|ui| {
        ui.set_min_width(w);
        f(ui)
    })
    .inner
}
