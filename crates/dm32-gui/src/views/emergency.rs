//! Digital / analog emergency systems and digital encryption keys (block 0x10).

use dm32_codeplug::records::emergency::{
    AlarmMode, AlarmType, DigitalEmergency, EncryptionKey, KeyType,
};
use eframe::egui;

use super::Doc;
use super::widgets;

pub fn show(ui: &mut egui::Ui, doc: &mut Doc) -> bool {
    let channels = doc.channel_names();
    let mut changed = false;
    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
        egui::CollapsingHeader::new("Digital Emergency Systems")
            .default_open(true)
            .show(ui, |ui| {
                changed |= digital(ui, doc, &channels);
            });
        egui::CollapsingHeader::new("Digital Encrypt Systems")
            .default_open(true)
            .show(ui, |ui| {
                changed |= keys(ui, doc);
            });
        egui::CollapsingHeader::new("Analog Emergency Systems")
            .default_open(false)
            .show(ui, |ui| {
                changed |= analog(ui, doc);
            });
    });
    changed
}

/// Picker for a channel stored as index + 1 (0 = None).
fn channel_pick(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    value: &mut u16,
    names: &[String],
) -> bool {
    let label = |v: u16| match v {
        0 => "None".to_string(),
        n => names
            .get(n as usize - 1)
            .map_or(format!("#{n}"), |s| format!("{n}: {s}")),
    };
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .selected_text(label(*value))
        .width(160.0)
        .height(300.0)
        .show_ui(ui, |ui| {
            changed |= ui.selectable_value(value, 0, "None").changed();
            for n in 1..=names.len().min(u16::MAX as usize) as u16 {
                changed |= ui.selectable_value(value, n, label(n)).changed();
            }
        });
    changed
}

/// Values 10, 20, … 120.
fn tens(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, value: &mut u16) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .selected_text(value.to_string())
        .width(50.0)
        .show_ui(ui, |ui| {
            for v in (10..=120).step_by(10) {
                changed |= ui.selectable_value(value, v, v.to_string()).changed();
            }
        });
    changed
}

fn new_digital(i: usize) -> DigitalEmergency {
    let mut d = DigitalEmergency::default();
    d.set_name(&format!("Sys {}", i + 1));
    d.set_alarm_type(AlarmType::Normal);
    d.set_alarm_mode(AlarmMode::EmergencyAlarm);
    d.set_retransmissions(5);
    d.set_hot_mic_duration(5);
    d.set_emergency_calls(10);
    d.set_rx_duration(1);
    d.set_auto_call_timer(10);
    d
}

fn digital(ui: &mut egui::Ui, doc: &mut Doc, channels: &[String]) -> bool {
    let mut changed = false;
    egui::Grid::new("dig_emer")
        .striped(true)
        .spacing([10.0, 4.0])
        .show(ui, |ui| {
            for t in [
                "No.",
                "Name",
                "Alarm Type",
                "Alarm Mode",
                "Revert Channel",
                "Retransmission",
                "HOT MIC [s]",
                "Calls Number",
                "RX Duration [s]",
                "Auto Call",
                "Enabled",
                "",
            ] {
                ui.strong(t);
            }
            ui.end_row();
            for (i, d) in doc.cp.emergency.digital.iter_mut().enumerate() {
                ui.label((i + 1).to_string());
                if d.is_empty() {
                    ui.weak("(empty)");
                    for _ in 0..9 {
                        ui.label("");
                    }
                    if ui.button("Create").clicked() {
                        *d = new_digital(i);
                        changed = true;
                    }
                    ui.end_row();
                    continue;
                }
                let mut name = d.name();
                if wide(ui, 90.0, |ui| {
                    widgets::name(ui, &mut name, DigitalEmergency::NAME_LEN, 90.0)
                }) {
                    d.set_name(&name);
                    changed = true;
                }
                let mut t = d.alarm_type();
                if widgets::choice(ui, ("de_type", i), &mut t) {
                    d.set_alarm_type(t);
                    changed = true;
                }
                let mut m = d.alarm_mode();
                if widgets::choice(ui, ("de_mode", i), &mut m) {
                    d.set_alarm_mode(m);
                    changed = true;
                }
                let mut ch = d.revert_channel();
                if channel_pick(ui, ("de_rev", i), &mut ch, channels) {
                    d.set_revert_channel(ch);
                    changed = true;
                }
                let mut v = d.retransmissions();
                if widgets::number(ui, &mut v, 1..=15) {
                    d.set_retransmissions(v);
                    changed = true;
                }
                let mut v = d.hot_mic_duration();
                if widgets::number(ui, &mut v, 1..=15) {
                    d.set_hot_mic_duration(v);
                    changed = true;
                }
                let mut v = d.emergency_calls();
                if tens(ui, ("de_calls", i), &mut v) {
                    d.set_emergency_calls(v);
                    changed = true;
                }
                let mut v = d.rx_duration();
                if widgets::number(ui, &mut v, 1..=255) {
                    d.set_rx_duration(v);
                    changed = true;
                }
                let mut v = d.auto_call_timer();
                if tens(ui, ("de_auto", i), &mut v) {
                    d.set_auto_call_timer(v);
                    changed = true;
                }
                let mut on = d.enabled();
                if ui.checkbox(&mut on, "").changed() {
                    d.set_enabled(on);
                    changed = true;
                }
                if ui
                    .button("Clear")
                    .on_hover_text("Empty this slot")
                    .clicked()
                {
                    *d = DigitalEmergency::default();
                    changed = true;
                }
                ui.end_row();
            }
        });
    changed
}

fn analog(ui: &mut egui::Ui, doc: &mut Doc) -> bool {
    ui.weak("Only the name is decoded; the other fields of analog emergency systems are kept unchanged.");
    let mut changed = false;
    egui::Grid::new("ana_emer")
        .striped(true)
        .spacing([10.0, 4.0])
        .show(ui, |ui| {
            ui.strong("No.");
            ui.strong("Emergency Name");
            ui.end_row();
            for (i, a) in doc.cp.emergency.analog.iter_mut().enumerate() {
                ui.label((i + 1).to_string());
                let mut name = a.name();
                if widgets::name(
                    ui,
                    &mut name,
                    dm32_codeplug::records::emergency::AnalogEmergency::NAME_LEN,
                    140.0,
                ) {
                    a.set_name(&name);
                    changed = true;
                }
                ui.end_row();
            }
        });
    changed
}

/// Hex key field. The text being typed lives in egui memory and is applied when focus leaves
/// the field and it is valid.
fn key_field(ui: &mut egui::Ui, i: usize, k: &mut EncryptionKey) -> bool {
    let id = egui::Id::new(("key_hex", i));
    let t = k.key_type();
    let digits = t.key_len() * 2;
    let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| k.key_hex());
    let valid = |s: &str| {
        let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        if !s.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("only hex digits 0-9, A-F".to_string());
        }
        match t {
            // AES keys are hex numbers: shorter keys are zero-padded on the left.
            KeyType::Aes128 | KeyType::Aes256 if s.len() <= digits && !s.is_empty() => Ok(()),
            _ if s.len() == digits => Ok(()),
            KeyType::Aes128 | KeyType::Aes256 => Err(format!("1-{digits} hex digits")),
            _ => Err(format!("exactly {digits} hex digits")),
        }
    };
    let mut changed = false;
    ui.horizontal(|ui| {
        let resp = ui.add_enabled(
            digits > 0,
            egui::TextEdit::singleline(&mut text)
                .font(egui::TextStyle::Monospace)
                .char_limit(digits + 8)
                .desired_width(if digits > 16 { 300.0 } else { 160.0 }),
        );
        let check = valid(&text);
        if resp.has_focus() || resp.changed() {
            ui.data_mut(|d| d.insert_temp(id, text.clone()));
        }
        if let Err(e) = &check
            && digits > 0
        {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
        if resp.lost_focus() {
            if check.is_ok() && text.to_uppercase() != k.key_hex() && k.set_key_hex(&text).is_ok() {
                changed = true;
            }
            ui.data_mut(|d| d.remove::<String>(id));
        } else if !resp.has_focus() {
            ui.data_mut(|d| d.remove::<String>(id));
        }
    });
    changed
}

fn keys(ui: &mut egui::Ui, doc: &mut Doc) -> bool {
    let mut changed = false;
    if let Some(e) = ui.data(|d| d.get_temp::<String>(egui::Id::new("rng_error"))) {
        ui.colored_label(ui.visuals().error_fg_color, e);
    }
    egui::Grid::new("enc_keys")
        .striped(true)
        .spacing([10.0, 4.0])
        .show(ui, |ui| {
            for t in [
                "No.",
                "Name",
                "Type",
                "Encryption (hex)",
                "Encryption ID",
                "",
                "",
            ] {
                ui.strong(t);
            }
            ui.end_row();
            for (i, k) in doc.cp.emergency.keys.iter_mut().enumerate() {
                ui.label((i + 1).to_string());
                if k.is_empty() {
                    ui.weak("(empty)");
                    ui.label("");
                    ui.label("");
                    ui.label("");
                    if ui.button("Create").clicked() {
                        let mut new =
                            EncryptionKey::new(&format!("Encrypt {}", i + 1), KeyType::Aes256, &[]);
                        match generate(ui, i, &mut new) {
                            Ok(()) => {
                                *k = new;
                                changed = true;
                            }
                            Err(e) => report_rng_error(ui, &e),
                        }
                    }
                    ui.end_row();
                    continue;
                }
                let mut name = k.name();
                if wide(ui, 90.0, |ui| {
                    widgets::name(ui, &mut name, EncryptionKey::NAME_LEN, 90.0)
                }) {
                    k.set_name(&name);
                    changed = true;
                }
                let mut t = k.key_type();
                if widgets::choice(ui, ("key_type", i), &mut t) {
                    // Keep the key value (as a number for AES, as bytes otherwise) across the change.
                    let old = k.key().to_vec();
                    let old = match k.key_type() {
                        KeyType::Aes128 | KeyType::Aes256 => {
                            let skip = old.iter().take_while(|&&b| b == 0).count();
                            old[skip..].to_vec()
                        }
                        _ => old,
                    };
                    k.set_key_type(t);
                    k.set_key(&old);
                    changed = true;
                }
                changed |= wide(ui, 300.0, |ui| key_field(ui, i, k));
                ui.label(k.key_id().to_string())
                    .on_hover_text("Maintained automatically (slot number)");
                ui.horizontal(|ui| {
                    if ui
                        .button("🎲 Generate")
                        .on_hover_text(
                            "Replace this key with a new random one of the selected type. Every \
                             radio that should decode this channel needs the same key. \
                             Ctrl+Z undoes it.",
                        )
                        .clicked()
                    {
                        match generate(ui, i, k) {
                            Ok(()) => changed = true,
                            Err(e) => report_rng_error(ui, &e),
                        }
                    }
                    if ui
                        .button("📋 Copy")
                        .on_hover_text("Copy the key (hex) to the clipboard")
                        .clicked()
                    {
                        ui.ctx().copy_text(k.key_hex());
                    }
                });
                if ui
                    .button("Clear")
                    .on_hover_text("Empty this slot")
                    .clicked()
                {
                    *k = EncryptionKey::default();
                    changed = true;
                }
                ui.end_row();
            }
        });
    changed
}

/// Fill key slot `i` with random bytes from the operating system's secure random number
/// generator, using the full length of its key type. A slot without a usable type gets AES-256.
fn generate(ui: &egui::Ui, i: usize, k: &mut EncryptionKey) -> Result<(), getrandom::Error> {
    if matches!(k.key_type(), KeyType::None | KeyType::Other(_)) {
        k.set_key_type(KeyType::Aes256);
    }
    let mut key = vec![0u8; k.key_type().key_len()];
    getrandom::fill(&mut key)?;
    k.set_key(&key);
    // Drop any half-typed text so the field shows the new key.
    ui.data_mut(|d| d.remove::<String>(egui::Id::new(("key_hex", i))));
    Ok(())
}

fn report_rng_error(ui: &egui::Ui, e: &getrandom::Error) {
    let id = egui::Id::new("rng_error");
    ui.data_mut(|d| d.insert_temp(id, format!("Cannot generate a key: {e}")));
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
