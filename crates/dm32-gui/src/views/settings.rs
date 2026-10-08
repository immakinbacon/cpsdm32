//! Radio settings (block 0x04), laid out like the official CPS "Option Feature", "Key Function",
//! "One Key Operation" and "APRS" pages.

use dm32_codeplug::records::settings::{
    ANALOG_CALL_COUNT, APRS_CHANNELS, CallWay, FUN_COUNT, MENU_ITEMS, ONE_TOUCH_COUNT, Settings,
};
use eframe::egui;

use super::Doc;
use super::widgets;

/// Fields the codeplug crate marks DERIVED (one source only, or the sources disagree): shown
/// with a "(?)" because they are not confirmed on hardware.
const DERIVED: &[&str] = &[
    "allow_reset",
    "auto_power_off",
    "radio_silent",
    "key_tone",
    "sms_tone",
    "group_call_tone",
    "private_call_tone",
    "call_end_tone",
    "talk_permit_tone",
    "startup_tone",
    "voice_prompt",
    "low_battery_tone",
    "roger_tone",
    "backlight_time",
    "menu_exit_time",
    "show_clock",
    "date_format",
    "volume_prompt",
    "gps_interval",
    "record_mode",
    "recording",
    "call_hold_time",
    "active_wait_time",
    "sms_format",
    "remote_monitor_time",
    "talker_alias_encoding",
    "vfo_disabled",
    "vfo_mode_a",
    "vfo_mode_b",
    "display_a",
    "display_b",
    "main_vfo",
    "dual_standby",
    "dual_standby_hang_time",
    "auto_key_lock_delay",
    "long_press_time",
    "tot",
    "tot_alert",
    "vox_level",
    "vox_delay",
    "leds_off",
    "weather_alarm",
    "power_save",
    "ste",
    "tbst_freq",
    "fm_mic_gain",
    "dmr_mic_gain",
    "analog_call",
    "fun_call",
    "one_touch_object",
    "aprs_interval",
    "aprs_fixed",
    "aprs_channel",
    "aprs_tx_delay",
    "aprs_call_type",
    "aprs_id",
    "passwords",
    "menu",
];

const DERIVED_HINT: &str = "Not confirmed on hardware: the location or meaning of this field comes from \
    one source only, or the sources disagree.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    PowerOn,
    AlertTone,
    Display,
    Gps,
    Digital,
    WorkMode,
    Other,
    Password,
    Menu,
    Keys,
    OneKey,
    Aprs,
}

const PAGES: &[(Page, &str)] = &[
    (Page::PowerOn, "Power on/off"),
    (Page::AlertTone, "Alert Tone"),
    (Page::Display, "Display"),
    (Page::Gps, "GPS & Record"),
    (Page::Digital, "Digital"),
    (Page::WorkMode, "Work Mode"),
    (Page::Other, "Other"),
    (Page::Password, "Password"),
    (Page::Menu, "Menu"),
    (Page::Keys, "Key Function"),
    (Page::OneKey, "One Key Operation"),
    (Page::Aprs, "APRS"),
];

/// Grid label, with a "(?)" marker for DERIVED fields.
fn field_label(ui: &mut egui::Ui, label: &str, key: &str) {
    if DERIVED.contains(&key) {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.weak("(?)").on_hover_text(DERIVED_HINT);
        });
    } else {
        ui.label(label);
    }
}

fn heading(ui: &mut egui::Ui, text: &str, key: &str) {
    ui.horizontal(|ui| {
        ui.strong(text);
        if DERIVED.contains(&key) {
            ui.weak("(?)").on_hover_text(DERIVED_HINT);
        }
    });
}

/// Choice field row.
macro_rules! c_row {
    ($ui:expr, $label:expr, $s:expr, $get:ident, $set:ident) => {{
        field_label($ui, $label, stringify!($get));
        let mut v = $s.$get();
        let c = widgets::choice($ui, stringify!($get), &mut v);
        if c {
            $s.$set(v);
        }
        $ui.end_row();
        c
    }};
}

/// On/off field row.
macro_rules! f_row {
    ($ui:expr, $label:expr, $s:expr, $get:ident, $set:ident) => {{
        field_label($ui, $label, stringify!($get));
        let mut v = $s.$get();
        let c = $ui.checkbox(&mut v, "").changed();
        if c {
            $s.$set(v);
        }
        $ui.end_row();
        c
    }};
}

/// Text field row.
macro_rules! t_row {
    ($ui:expr, $label:expr, $s:expr, $get:ident, $set:ident, $max:expr) => {{
        field_label($ui, $label, stringify!($get));
        let mut v = $s.$get();
        let c = widgets::name($ui, &mut v, $max, 180.0);
        if c {
            $s.$set(&v);
        }
        $ui.end_row();
        c
    }};
}

/// Raw number row; `$hint` turns the raw value into what the radio shows.
macro_rules! n_row {
    ($ui:expr, $label:expr, $s:expr, $get:ident, $set:ident, $range:expr, $hint:expr) => {{
        field_label($ui, $label, stringify!($get));
        let mut v = $s.$get();
        let c = $ui
            .horizontal(|ui| {
                let c = widgets::number(ui, &mut v, $range);
                let hint: fn(u8) -> String = $hint;
                let h = hint(v);
                if !h.is_empty() {
                    ui.weak(h);
                }
                c
            })
            .inner;
        if c {
            $s.$set(v);
        }
        $ui.end_row();
        c
    }};
}

fn grid(ui: &mut egui::Ui, id: &str, add: impl FnOnce(&mut egui::Ui) -> bool) -> bool {
    egui::Grid::new(id)
        .num_columns(2)
        .striped(true)
        .spacing([16.0, 4.0])
        .show(ui, add)
        .inner
}

fn section(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> bool) -> bool {
    egui::CollapsingHeader::new(title)
        .default_open(true)
        .show(ui, add)
        .body_returned
        .unwrap_or(false)
}

/// Picker for a 1-based list reference stored as a number (0 = `zero_label`).
fn index_pick(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    value: &mut u16,
    names: &[String],
    zero_label: &str,
) -> bool {
    let label = |v: u16| match v {
        0 => zero_label.to_string(),
        n => names
            .get(n as usize - 1)
            .map_or(format!("#{n}"), |s| format!("{n}: {s}")),
    };
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .selected_text(label(*value))
        .width(180.0)
        .height(300.0)
        .show_ui(ui, |ui| {
            changed |= ui.selectable_value(value, 0, zero_label).changed();
            for n in 1..=names.len().min(u16::MAX as usize) as u16 {
                changed |= ui.selectable_value(value, n, label(n)).changed();
            }
        });
    changed
}

pub fn show(ui: &mut egui::Ui, doc: &mut Doc) -> bool {
    let channels = doc.channel_names();
    let talkgroups: Vec<String> = doc
        .cp
        .talkgroups
        .slots
        .iter()
        .map(|s| s.as_ref().map_or(String::new(), |t| t.name()))
        .collect();
    let analog_contacts: Vec<String> = doc.cp.analog.contacts.iter().map(|c| c.name()).collect();

    let Some(s) = doc.cp.settings.as_mut() else {
        ui.label("This codeplug has no radio settings block (0x04). Read the codeplug from a radio to edit its settings.");
        return false;
    };

    let page_id = egui::Id::new("settings_page");
    let mut page = ui
        .data(|d| d.get_temp::<Page>(page_id))
        .unwrap_or(Page::PowerOn);
    ui.horizontal_wrapped(|ui| {
        for (p, label) in PAGES {
            ui.selectable_value(&mut page, *p, *label);
        }
    });
    ui.data_mut(|d| d.insert_temp(page_id, page));
    ui.horizontal(|ui| {
        ui.weak("(?)");
        ui.weak("= not confirmed on hardware (hover for details)");
    });
    ui.separator();

    egui::ScrollArea::both()
        .auto_shrink(false)
        .show(ui, |ui| match page {
            Page::PowerOn => power_on(ui, s),
            Page::AlertTone => alert_tone(ui, s),
            Page::Display => display(ui, s),
            Page::Gps => gps(ui, s),
            Page::Digital => digital(ui, s),
            Page::WorkMode => work_mode(ui, s),
            Page::Other => other(ui, s),
            Page::Password => passwords(ui, s),
            Page::Menu => menu(ui, s),
            Page::Keys => keys(ui, s),
            Page::OneKey => one_key(ui, s, &talkgroups, &analog_contacts),
            Page::Aprs => aprs(ui, s, &channels),
        })
        .inner
}

fn no_hint(_: u8) -> String {
    String::new()
}

fn power_on(ui: &mut egui::Ui, s: &mut Settings) -> bool {
    grid(ui, "set_power", |ui| {
        let mut c = false;
        c |= c_row!(
            ui,
            "Power On Interface",
            s,
            power_on_display,
            set_power_on_display
        );
        c |= t_row!(
            ui,
            "Power On Display Line 1",
            s,
            power_on_line1,
            set_power_on_line1,
            13
        );
        c |= t_row!(
            ui,
            "Power On Display Line 2",
            s,
            power_on_line2,
            set_power_on_line2,
            13
        );
        c |= c_row!(ui, "Auto Power Off", s, auto_power_off, set_auto_power_off);
        c |= f_row!(ui, "Allow Reset", s, allow_reset, set_allow_reset);
        c
    })
}

fn alert_tone(ui: &mut egui::Ui, s: &mut Settings) -> bool {
    grid(ui, "set_tones", |ui| {
        let mut c = false;
        c |= f_row!(ui, "Radio Silent", s, radio_silent, set_radio_silent);
        c |= f_row!(ui, "Key Tone", s, key_tone, set_key_tone);
        c |= f_row!(ui, "SMS Alert", s, sms_tone, set_sms_tone);
        c |= f_row!(
            ui,
            "Group Call Tone",
            s,
            group_call_tone,
            set_group_call_tone
        );
        c |= f_row!(
            ui,
            "Private Call Tone",
            s,
            private_call_tone,
            set_private_call_tone
        );
        c |= f_row!(ui, "Call End Tone", s, call_end_tone, set_call_end_tone);
        c |= f_row!(
            ui,
            "Talk Permit Tone",
            s,
            talk_permit_tone,
            set_talk_permit_tone
        );
        c |= f_row!(ui, "StartUp Sound", s, startup_tone, set_startup_tone);
        c |= f_row!(ui, "Voice Prompt", s, voice_prompt, set_voice_prompt);
        c |= f_row!(ui, "Battery Low", s, low_battery_tone, set_low_battery_tone);
        c |= c_row!(ui, "Analog Roger Tone", s, roger_tone, set_roger_tone);
        c
    })
}

fn display(ui: &mut egui::Ui, s: &mut Settings) -> bool {
    grid(ui, "set_display", |ui| {
        let mut c = false;
        c |= n_row!(
            ui,
            "Backlight Brightness",
            s,
            brightness,
            set_brightness,
            0..=5,
            |v| format!("level {}", v + 1)
        );
        c |= c_row!(
            ui,
            "Auto Backlight Duration",
            s,
            backlight_time,
            set_backlight_time
        );
        c |= n_row!(
            ui,
            "Menu Exit Time",
            s,
            menu_exit_time,
            set_menu_exit_time,
            0..=255,
            |v| {
                if v == 0 {
                    "never".into()
                } else {
                    format!("{} s", v as u32 * 5)
                }
            }
        );
        c |= f_row!(ui, "Time Display", s, show_clock, set_show_clock);
        c |= c_row!(ui, "Date Display Format", s, date_format, set_date_format);
        c |= f_row!(
            ui,
            "Volume Change Prompt",
            s,
            volume_prompt,
            set_volume_prompt
        );
        c |= c_row!(ui, "Call Display Color", s, call_color, set_call_color);
        c |= c_row!(
            ui,
            "Standby Character Color",
            s,
            standby_color,
            set_standby_color
        );
        c |= c_row!(
            ui,
            "A Channel Name Color",
            s,
            channel_a_color,
            set_channel_a_color
        );
        c |= c_row!(
            ui,
            "B Channel Name Color",
            s,
            channel_b_color,
            set_channel_b_color
        );
        c |= c_row!(ui, "Zone A Color", s, zone_a_color, set_zone_a_color);
        c |= c_row!(ui, "Zone B Color", s, zone_b_color, set_zone_b_color);
        c
    })
}

fn gps(ui: &mut egui::Ui, s: &mut Settings) -> bool {
    let mut c = section(ui, "GPS", |ui| {
        grid(ui, "set_gps", |ui| {
            let mut c = false;
            c |= f_row!(ui, "GPS Switch", s, gps_on, set_gps_on);
            c |= c_row!(ui, "GPS Mode", s, gps_mode, set_gps_mode);
            field_label(ui, "Time Zone", "time_zone");
            let mut tz = s.time_zone();
            let label = |z: u8| match z {
                12 => "UTC".to_string(),
                z if z <= 25 => format!("UTC {:+}:00", z as i8 - 12),
                z => format!("Unknown ({z})"),
            };
            egui::ComboBox::from_id_salt("time_zone")
                .selected_text(label(tz))
                .height(300.0)
                .show_ui(ui, |ui| {
                    for z in 0..=25u8 {
                        if ui.selectable_value(&mut tz, z, label(z)).changed() {
                            s.set_time_zone(tz);
                            c = true;
                        }
                    }
                });
            ui.end_row();
            c |= n_row!(
                ui,
                "Measure Period Interval [s]",
                s,
                gps_interval,
                set_gps_interval,
                0..=255,
                no_hint
            );
            c |= c_row!(
                ui,
                "GPS Display Format",
                s,
                position_format,
                set_position_format
            );
            c |= c_row!(ui, "Distance Unit", s, distance_unit, set_distance_unit);
            c |= c_row!(ui, "Speed Unit", s, speed_unit, set_speed_unit);
            c
        })
    });
    c |= section(ui, "Record", |ui| {
        grid(ui, "set_record", |ui| {
            let mut c = false;
            c |= f_row!(ui, "Record Switch", s, recording, set_recording);
            c |= c_row!(ui, "Record Type", s, record_mode, set_record_mode);
            c
        })
    });
    c
}

fn digital(ui: &mut egui::Ui, s: &mut Settings) -> bool {
    grid(ui, "set_digital", |ui| {
        let mut c = false;
        c |= f_row!(
            ui,
            "Private Call Match",
            s,
            private_call_match,
            set_private_call_match
        );
        c |= f_row!(
            ui,
            "Group Call Match",
            s,
            group_call_match,
            set_group_call_match
        );
        c |= n_row!(
            ui,
            "Call Hold Time (raw)",
            s,
            call_hold_time,
            set_call_hold_time,
            0..=255,
            |v| { format!("{v} s (spec) / {} ms (qdmr)", v as u32 * 500) }
        );
        c |= n_row!(
            ui,
            "Active Wait Time (raw)",
            s,
            active_wait_time,
            set_active_wait_time,
            0..=255,
            |v| { format!("≈ {} ms", v as u32 * 30 + 300) }
        );
        c |= n_row!(
            ui,
            "Active Retries Time",
            s,
            active_retries,
            set_active_retries,
            1..=8,
            no_hint
        );
        c |= n_row!(
            ui,
            "Pre-Carrier Time",
            s,
            pre_carrier_time,
            set_pre_carrier_time,
            0..=255,
            |v| { format!("{} ms", (v as u32 + 1) * 120) }
        );
        c |= f_row!(
            ui,
            "Missed Call Alert",
            s,
            missed_call_alert,
            set_missed_call_alert
        );
        c |= c_row!(ui, "SMS Format", s, sms_format, set_sms_format);
        c |= f_row!(
            ui,
            "Call Alert Decode",
            s,
            call_alert_decode,
            set_call_alert_decode
        );
        c |= f_row!(
            ui,
            "Radio Enable Decode",
            s,
            radio_enable_decode,
            set_radio_enable_decode
        );
        c |= f_row!(
            ui,
            "Radio Check Decode",
            s,
            radio_check_decode,
            set_radio_check_decode
        );
        c |= f_row!(
            ui,
            "Radio Disable Decode",
            s,
            radio_disable_decode,
            set_radio_disable_decode
        );
        c |= f_row!(
            ui,
            "Remote Monitor Decode",
            s,
            remote_monitor_decode,
            set_remote_monitor_decode
        );
        c |= n_row!(
            ui,
            "Remote Monitor Time",
            s,
            remote_monitor_time,
            set_remote_monitor_time,
            0..=255,
            |v| { format!("{} s", (v as u32 + 1) * 10) }
        );
        c |= c_row!(
            ui,
            "Name Display Priority",
            s,
            name_display_priority,
            set_name_display_priority
        );
        c |= f_row!(
            ui,
            "Send TX Name",
            s,
            send_talker_alias,
            set_send_talker_alias
        );
        c |= c_row!(
            ui,
            "Name Data Format",
            s,
            talker_alias_encoding,
            set_talker_alias_encoding
        );
        c
    })
}

fn work_mode(ui: &mut egui::Ui, s: &mut Settings) -> bool {
    grid(ui, "set_work", |ui| {
        let mut c = false;
        c |= f_row!(ui, "Only Channel Mode", s, vfo_disabled, set_vfo_disabled);
        c |= c_row!(ui, "Work Mode (A)", s, vfo_mode_a, set_vfo_mode_a);
        c |= c_row!(ui, "Work Mode (B)", s, vfo_mode_b, set_vfo_mode_b);
        c |= c_row!(ui, "A Channel Display Mode", s, display_a, set_display_a);
        c |= c_row!(ui, "B Channel Display Mode", s, display_b, set_display_b);
        c |= c_row!(ui, "Main Channel Setting", s, main_vfo, set_main_vfo);
        c |= c_row!(ui, "Dual Watch Mode", s, dual_standby, set_dual_standby);
        c |= n_row!(
            ui,
            "Dual Watch Hang Time (raw)",
            s,
            dual_standby_hang_time,
            set_dual_standby_hang_time,
            0..=255,
            |v| { format!("{} ms (qdmr)", v as u32 * 500) }
        );
        c
    })
}

fn other(ui: &mut egui::Ui, s: &mut Settings) -> bool {
    grid(ui, "set_other", |ui| {
        let mut c = false;
        c |= f_row!(ui, "Disable All LEDs", s, leds_off, set_leds_off);
        c |= f_row!(ui, "Weather Alarm", s, weather_alarm, set_weather_alarm);
        c |= n_row!(ui, "TOT", s, tot, set_tot, 0..=255, |v| {
            if v == 0 {
                "off".into()
            } else {
                format!("{} s", v as u32 * 5 + 15)
            }
        });
        c |= n_row!(
            ui,
            "TOT Pre Time [s]",
            s,
            tot_alert,
            set_tot_alert,
            0..=255,
            no_hint
        );
        c |= n_row!(ui, "VOX Level", s, vox_level, set_vox_level, 0..=5, |v| {
            if v == 0 { "off".into() } else { String::new() }
        });
        c |= n_row!(
            ui,
            "VOX Delay Time",
            s,
            vox_delay,
            set_vox_delay,
            0..=255,
            |v| format!("{:.1} s", v as f32 / 10.0)
        );
        c |= c_row!(ui, "Power Save", s, power_save, set_power_save);
        c |= c_row!(ui, "Tail Noise Reduction", s, ste, set_ste);
        c |= c_row!(ui, "TBST", s, tbst_freq, set_tbst_freq);
        c |= n_row!(
            ui,
            "Analog MIC Level",
            s,
            fm_mic_gain,
            set_fm_mic_gain,
            0..=4,
            no_hint
        );
        c |= n_row!(
            ui,
            "Digital MIC Level",
            s,
            dmr_mic_gain,
            set_dmr_mic_gain,
            0..=4,
            no_hint
        );
        c
    })
}

fn passwords(ui: &mut egui::Ui, s: &mut Settings) -> bool {
    heading(ui, "Passwords", "passwords");
    ui.add_space(4.0);
    egui::Grid::new("set_pw")
        .num_columns(3)
        .striped(true)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            let mut c = false;
            type Get = fn(&Settings) -> bool;
            type Set = fn(&mut Settings, bool);
            type GetS = fn(&Settings) -> String;
            type SetS = fn(&mut Settings, &str);
            let rows: [(&str, Get, Set, GetS, SetS); 3] = [
                (
                    "Power-on Password",
                    Settings::power_on_password_enabled,
                    Settings::set_power_on_password_enabled,
                    Settings::power_on_password,
                    Settings::set_power_on_password,
                ),
                (
                    "Read Data Password",
                    Settings::read_password_enabled,
                    Settings::set_read_password_enabled,
                    Settings::read_password,
                    Settings::set_read_password,
                ),
                (
                    "Write Data Password",
                    Settings::write_password_enabled,
                    Settings::set_write_password_enabled,
                    Settings::write_password,
                    Settings::set_write_password,
                ),
            ];
            for (label, get, set, get_s, set_s) in rows {
                ui.label(label);
                let mut on = get(s);
                if ui.checkbox(&mut on, "Enabled").changed() {
                    set(s, on);
                    c = true;
                }
                let mut pw = get_s(s);
                ui.add_enabled_ui(on, |ui| {
                    if widgets::name(ui, &mut pw, 8, 100.0) {
                        set_s(s, &pw);
                        c = true;
                    }
                });
                ui.end_row();
            }
            c
        })
        .inner
}

fn menu(ui: &mut egui::Ui, s: &mut Settings) -> bool {
    heading(ui, "Menu items shown on the radio", "menu");
    ui.weak("Polarity (ticked = shown) and some labels are not confirmed on hardware.");
    let mut c = false;
    ui.horizontal(|ui| {
        if ui.button("Show all").clicked() {
            (0..MENU_ITEMS.len()).for_each(|i| s.set_menu_enabled(i, true));
            c = true;
        }
        if ui.button("Hide all").clicked() {
            (0..MENU_ITEMS.len()).for_each(|i| s.set_menu_enabled(i, false));
            c = true;
        }
    });
    ui.add_space(4.0);
    // CPS "Option Menu" groups, by byte.
    let groups: &[(&str, &[usize])] = &[
        ("Zone", &[0x500]),
        ("Enhance Business", &[0x501]),
        ("Radio Setting", &[0x502, 0x503]),
        ("Digital Contact", &[0x504]),
        ("Call Log", &[0x505]),
        ("Program", &[0x506, 0x507]),
    ];
    ui.horizontal_wrapped(|ui| {
        for (title, offs) in groups {
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.strong(*title);
                    for (i, (label, off, _)) in MENU_ITEMS.iter().enumerate() {
                        if offs.contains(off) {
                            let mut on = s.menu_enabled(i);
                            if ui.checkbox(&mut on, *label).changed() {
                                s.set_menu_enabled(i, on);
                                c = true;
                            }
                        }
                    }
                });
            });
        }
    });
    c
}

fn keys(ui: &mut egui::Ui, s: &mut Settings) -> bool {
    grid(ui, "set_keys", |ui| {
        let mut c = false;
        c |= c_row!(ui, "Lock Key", s, key_lock_mode, set_key_lock_mode);
        c |= f_row!(ui, "Knob Lock", s, knob_lock, set_knob_lock);
        c |= f_row!(ui, "Side Key Lock", s, side_key_lock, set_side_key_lock);
        c |= n_row!(
            ui,
            "Auto Keypad Lock Delay Time (raw)",
            s,
            auto_key_lock_delay,
            set_auto_key_lock_delay,
            0..=255,
            no_hint
        );
        c |= n_row!(
            ui,
            "Long Press Time",
            s,
            long_press_time,
            set_long_press_time,
            0..=4,
            |v| format!("{} s", v + 1)
        );
        c |= c_row!(ui, "SK1 Short", s, sk1_short, set_sk1_short);
        c |= c_row!(ui, "SK1 Long", s, sk1_long, set_sk1_long);
        c |= c_row!(ui, "SK2 Short", s, sk2_short, set_sk2_short);
        c |= c_row!(ui, "SK2 Long", s, sk2_long, set_sk2_long);
        c |= c_row!(ui, "P1 Short", s, p1_short, set_p1_short);
        c |= c_row!(ui, "P1 Long", s, p1_long, set_p1_long);
        c |= c_row!(ui, "P2 Short", s, p2_short, set_p2_short);
        c |= c_row!(ui, "P2 Long", s, p2_long, set_p2_long);
        c
    })
}

fn header(ui: &mut egui::Ui, cols: &[&str]) {
    for t in cols {
        ui.strong(*t);
    }
    ui.end_row();
}

/// The call object picker for One Touch Call / Fun+: a 1-based index into the digital contacts
/// (talk groups) or the analog contacts, depending on the call way.
fn call_object(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    way: CallWay,
    v: &mut u16,
    tgs: &[String],
    analog: &[String],
) -> bool {
    match way {
        CallWay::Digital => index_pick(ui, id, v, tgs, "None"),
        CallWay::Analog => index_pick(ui, id, v, analog, "None"),
        _ => widgets::number(ui, v, 0..=u16::MAX),
    }
}

fn one_key(ui: &mut egui::Ui, s: &mut Settings, tgs: &[String], analog: &[String]) -> bool {
    let mut c = false;
    c |= section(ui, "Analog Call", |ui| {
        heading(ui, "Analog one-key calls", "analog_call");
        egui::Grid::new("set_acall")
            .striped(true)
            .spacing([16.0, 4.0])
            .show(ui, |ui| {
                let mut c = false;
                header(ui, &["No.", "Call Type", "Call ID"]);
                for i in 0..ANALOG_CALL_COUNT {
                    ui.label((i + 1).to_string());
                    let mut t = s.analog_call_type(i);
                    if widgets::choice(ui, ("acall_t", i), &mut t) {
                        s.set_analog_call_type(i, t);
                        c = true;
                    }
                    let mut id = s.analog_call_id(i);
                    if widgets::number(ui, &mut id, 0..=255) {
                        s.set_analog_call_id(i, id);
                        c = true;
                    }
                    ui.end_row();
                }
                c
            })
            .inner
    });
    c |= section(ui, "One Touch Call", |ui| {
        heading(
            ui,
            "One Touch Call 1-5 (call object: 1-based contact number)",
            "one_touch_object",
        );
        egui::Grid::new("set_otc")
            .striped(true)
            .spacing([16.0, 4.0])
            .show(ui, |ui| {
                let mut c = false;
                header(
                    ui,
                    &[
                        "No.",
                        "Call Type",
                        "Call Object",
                        "Digital Call Type",
                        "SMS",
                    ],
                );
                for i in 0..ONE_TOUCH_COUNT {
                    ui.label((i + 1).to_string());
                    let mut way = s.one_touch_way(i);
                    if widgets::choice(ui, ("otc_w", i), &mut way) {
                        s.set_one_touch_way(i, way);
                        c = true;
                    }
                    let mut obj = s.one_touch_object(i);
                    if call_object(ui, ("otc_o", i), way, &mut obj, tgs, analog) {
                        s.set_one_touch_object(i, obj);
                        c = true;
                    }
                    let mut t = s.one_touch_call_type(i);
                    if widgets::choice(ui, ("otc_t", i), &mut t) {
                        s.set_one_touch_call_type(i, t);
                        c = true;
                    }
                    let mut sms = s.one_touch_sms(i);
                    if widgets::number(ui, &mut sms, 0..=255) {
                        s.set_one_touch_sms(i, sms);
                        c = true;
                    }
                    ui.end_row();
                }
                c
            })
            .inner
    });
    c |= section(ui, "Fun+ Shortcuts", |ui| {
        heading(ui, "Fun+0 … Fun+9 (call columns)", "fun_call");
        egui::Grid::new("set_fun")
            .striped(true)
            .spacing([16.0, 4.0])
            .show(ui, |ui| {
                let mut c = false;
                header(
                    ui,
                    &[
                        "Fun+Number",
                        "Operate Mode",
                        "Menu Select",
                        "Call Way",
                        "Call Object",
                        "Digital Call Type",
                        "SMS",
                    ],
                );
                for i in 0..FUN_COUNT {
                    ui.label(format!("Fun+{i}"));
                    let mut m = s.fun_mode(i);
                    if widgets::choice(ui, ("fun_m", i), &mut m) {
                        s.set_fun_mode(i, m);
                        c = true;
                    }
                    let mut menu = s.fun_menu(i);
                    if widgets::choice(ui, ("fun_menu", i), &mut menu) {
                        s.set_fun_menu(i, menu);
                        c = true;
                    }
                    let mut way = s.fun_way(i);
                    if widgets::choice(ui, ("fun_w", i), &mut way) {
                        s.set_fun_way(i, way);
                        c = true;
                    }
                    let mut obj = s.fun_object(i) as u16;
                    let tgs = &tgs[..tgs.len().min(255)];
                    let analog = &analog[..analog.len().min(255)];
                    if call_object(ui, ("fun_o", i), way, &mut obj, tgs, analog) {
                        s.set_fun_object(i, obj.min(255) as u8);
                        c = true;
                    }
                    let mut t = s.fun_call_type(i);
                    if widgets::choice(ui, ("fun_t", i), &mut t) {
                        s.set_fun_call_type(i, t);
                        c = true;
                    }
                    let mut sms = s.fun_sms(i);
                    if widgets::number(ui, &mut sms, 0..=255) {
                        s.set_fun_sms(i, sms);
                        c = true;
                    }
                    ui.end_row();
                }
                c
            })
            .inner
    });
    c
}

/// ASCII direction byte ('N'/'S', 'E'/'W', 0 = unset).
fn direction(ui: &mut egui::Ui, id: &str, v: &mut u8, opts: [u8; 2]) -> bool {
    let label = |b: u8| {
        if b == 0 {
            "-".to_string()
        } else if b.is_ascii_graphic() {
            (b as char).to_string()
        } else {
            format!("0x{b:02X}")
        }
    };
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .selected_text(label(*v))
        .width(50.0)
        .show_ui(ui, |ui| {
            for o in [0, opts[0], opts[1]] {
                changed |= ui.selectable_value(v, o, label(o)).changed();
            }
        });
    changed
}

fn coordinate(ui: &mut egui::Ui, label: &str, s: &mut Settings, lat: bool) -> bool {
    ui.label(label);
    let mut c = false;
    ui.horizontal(|ui| {
        let mut v = if lat { s.latitude() } else { s.longitude() };
        if widgets::name(ui, &mut v, 9, 100.0) {
            if lat {
                s.set_latitude(&v)
            } else {
                s.set_longitude(&v)
            }
            c = true;
        }
        let limit = if lat { 90.0 } else { 180.0 };
        if !v.is_empty()
            && !v
                .trim()
                .parse::<f64>()
                .is_ok_and(|x| (0.0..=limit).contains(&x))
        {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("expected 0-{limit} degrees"),
            );
        }
        let mut d = if lat {
            s.latitude_dir()
        } else {
            s.longitude_dir()
        };
        let opts = if lat { *b"NS" } else { *b"EW" };
        if direction(ui, if lat { "lat_dir" } else { "lon_dir" }, &mut d, opts) {
            if lat {
                s.set_latitude_dir(d)
            } else {
                s.set_longitude_dir(d)
            }
            c = true;
        }
    });
    ui.end_row();
    c
}

fn aprs(ui: &mut egui::Ui, s: &mut Settings, channels: &[String]) -> bool {
    let mut c = section(ui, "APRS", |ui| {
        grid(ui, "set_aprs", |ui| {
            let mut c = false;
            c |= n_row!(
                ui,
                "Scheduled Send Time",
                s,
                aprs_interval,
                set_aprs_interval,
                0..=255,
                |v| {
                    if v == 0 {
                        "off".into()
                    } else {
                        format!("{} s", v as u32 * 30)
                    }
                }
            );
            c |= f_row!(ui, "Fixed Beacon", s, aprs_fixed, set_aprs_fixed);
            c |= coordinate(ui, "Latitude", s, true);
            c |= coordinate(ui, "Longitude", s, false);
            c
        })
    });
    c |= section(ui, "Digital APRS", |ui| {
        grid(ui, "set_daprs", |ui| {
            let mut c = false;
            c |= n_row!(
                ui,
                "Repeater Active Delay",
                s,
                aprs_tx_delay,
                set_aprs_tx_delay,
                0..=255,
                |v| { format!("{} ms", v as u32 * 100) }
            );
            c |= c_row!(ui, "Call Type", s, aprs_call_type, set_aprs_call_type);
            field_label(ui, "Upload Number", "aprs_id");
            let mut id = s.aprs_id();
            if widgets::number(ui, &mut id, 0..=0xFF_FFFF) {
                s.set_aprs_id(id);
                c = true;
            }
            ui.end_row();
            for i in 0..APRS_CHANNELS {
                field_label(ui, &format!("Report Channel {}", i + 1), "aprs_channel");
                let mut ch = s.aprs_channel(i);
                if index_pick(ui, ("aprs_ch", i), &mut ch, channels, "Current Channel") {
                    s.set_aprs_channel(i, ch);
                    c = true;
                }
                ui.end_row();
            }
            c
        })
    });
    c
}
