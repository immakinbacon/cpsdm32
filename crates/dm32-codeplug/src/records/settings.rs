//! Block 0x04: radio settings (power-on display, display/UI, GPS, DMR, keys, one-key operation,
//! APRS, passwords and menu enable flags).
//!
//! Sources: the spec (05-DATA-STRUCTURES "0x04"), qdmr (`dm32uv_codeplug.hh`
//! `GeneralSettingsElement` at 0x000, `APRSSettingsElement` at 0x300, `PasswordSettingsElement`
//! at 0x400, `MenuSettingElement` at 0x500) and the factory dump in the fixture. Each field notes
//! its confidence:
//! - **CONFIRMED**: anchored in a hardware capture or decodes sensibly in the factory dump and
//!   both sources agree;
//! - **DERIVED**: from one source only, or the sources disagree (the choice made is explained).
//!
//! [`Settings`] owns the whole 4 KB page; only the bits of known fields are ever changed.
//!
//! Option labels and orders come from the official CPS v1.60 `english.ini` option lists (e.g.
//! `[KeyFuncData]`, `[UtcZone]`, `[OptionHoldMode]`); that list order = raw value is inferred,
//! but matches every value in the factory dump.

use crate::encoding::{get_bits, get_str, get_u16, get_u24, put_str, put_u16, put_u24, set_bits};
use crate::image::{Image, PAGE, block};

/// Radio settings block, raw-backed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    raw: [u8; PAGE],
}

crate::raw_record!(Settings, PAGE, 0xFF);

crate::choice! {
    /// What the radio shows at power-on.
    pub enum PowerOnDisplay { Picture = 0 => "Power On Picture", Message = 1 => "Custom Message", Voltage = 2 => "Battery Volt" }
}

crate::choice! {
    pub enum AutoPowerOff {
        Off = 0 => "Off", Min30 = 1 => "30 Min", Min60 = 2 => "60 Min", Min120 = 3 => "120 Min",
        Min240 = 4 => "240 Min", Min480 = 5 => "480 Min",
    }
}

crate::choice! {
    /// Analog TX alert (roger) tone.
    pub enum RogerTone { Off = 0 => "Off", Tone = 1 => "TONE", Bdc = 2 => "BDC" }
}

crate::choice! {
    pub enum BacklightTime {
        Always = 0 => "Always On", S5 = 1 => "5 s", S10 = 2 => "10 s", S15 = 3 => "15 s",
        S20 = 4 => "20 s", S25 = 5 => "25 s", S30 = 6 => "30 s", M1 = 7 => "1 min",
        M2 = 8 => "2 min", M3 = 9 => "3 min", M4 = 10 => "4 min", M5 = 11 => "5 min",
    }
}

crate::choice! {
    pub enum DateFormat { Ymd = 0 => "yyy/m/d", Dmy = 1 => "d/m/yyy" }
}

crate::choice! {
    pub enum Color {
        White = 0 => "White", Black = 1 => "Black", Orange = 2 => "Orange", Red = 3 => "Red",
        Yellow = 4 => "Yellow", Green = 5 => "Green", Cyan = 6 => "Cyan", Blue = 7 => "Blue",
    }
}

crate::choice! {
    pub enum GpsMode { Gps = 0 => "GPS", Bds = 1 => "BDS", Both = 2 => "GPS+BDS" }
}

crate::choice! {
    pub enum DistanceUnit { Metric = 0 => "Metric System", British = 1 => "British System" }
}

crate::choice! {
    pub enum SpeedUnit { Kph = 0 => "Kilometer/h(Kph)", Mph = 1 => "Mile/h(Mph)", Kts = 2 => "Nautical Mile/h(Kts)" }
}

crate::choice! {
    pub enum PositionFormat { Degree = 0 => "Degree", Dms = 1 => "Degree/Min/Second" }
}

crate::choice! {
    pub enum RecordMode { Rx = 0 => "Receive Record", Tx = 1 => "Transmit Record", Both = 2 => "Receive+Transmit" }
}

crate::choice! {
    /// CPS "Data Service" (qdmr: SMS format Hytera / Motorola / DMR).
    pub enum SmsFormat { Hytera = 0 => "H-SMS", Motorola = 1 => "M-SMS", Dmr = 2 => "D-SMS" }
}

crate::choice! {
    /// CPS "Name Data Format".
    pub enum TalkerAliasEncoding { Iso8 = 0 => "ISO 8bit", Utf16 = 1 => "16 bit Unicode" }
}

crate::choice! {
    /// CPS "Name Display Priority" (qdmr: talker alias source, contact DB / over the air).
    pub enum NameDisplayPriority { Contact = 0 => "Contact Priority", Name = 1 => "Name Priority" }
}

crate::choice! {
    pub enum DualStandby { SingleVfo = 0 => "Single Mode", Double = 1 => "Double Wait", Single = 2 => "Single Wait" }
}

crate::choice! {
    pub enum Vfo { A = 0 => "A", B = 1 => "B" }
}

crate::choice! {
    pub enum VfoDisplay { Frequency = 0 => "Frequency", Name = 1 => "Channel" }
}

crate::choice! {
    /// CPS "A/B Range Mode". Order from the CPS list; qdmr has the opposite polarity
    /// (0 = channel). The factory 0xBE sets both bits, i.e. MR, which fits the CPS order.
    pub enum VfoMode { Vfo = 0 => "VFO", Mr = 1 => "MR" }
}

crate::choice! {
    pub enum PowerSave { Off = 0 => "None", R1 = 1 => "1:1", R2 = 2 => "1:2", R4 = 3 => "1:4" }
}

crate::choice! {
    pub enum TbstFreq { Hz1000 = 0 => "1000Hz", Hz1450 = 1 => "1450Hz", Hz1750 = 2 => "1750Hz", Hz2100 = 3 => "2100Hz" }
}

crate::choice! {
    /// Squelch tail elimination.
    pub enum Ste { Off = 0 => "None", Deg120 = 1 => "120", Deg180 = 2 => "180", Hz55 = 3 => "55Hz" }
}

crate::choice! {
    /// Function of a programmable key press (SK1/SK2/P1/P2, short and long). CONFIRMED: the
    /// factory defaults all decode to sensible entries.
    pub enum KeyFunction {
        None = 0 => "None", PowerSelect = 1 => "Power Select", Volt = 2 => "Volt",
        Talkaround = 3 => "Talkaround", DigitalEncrypt = 4 => "Digital Encrypt", Call = 5 => "Call",
        Vox = 6 => "VOX", VM = 7 => "V/M", Alarm = 8 => "Alarm",
        OneTouch1 = 9 => "One Touch Call 1", OneTouch2 = 10 => "One Touch Call 2",
        OneTouch3 = 11 => "One Touch Call 3", OneTouch4 = 12 => "One Touch Call 4",
        OneTouch5 = 13 => "One Touch Call 5", Sms = 14 => "SMS", Contacts = 15 => "Contacts",
        ZoneUp = 16 => "Zone Up", ZoneDown = 17 => "Zone Down", Scan = 18 => "Scan",
        RecordSwitch = 19 => "Record Switch", PreviousRecord = 20 => "Previous Record",
        NextRecord = 21 => "Next Record", FmRadio = 22 => "FM Radio", FmSearch = 23 => "FM Search",
        GpsInformation = 24 => "GPS Information", Monitor = 25 => "Monitor",
        SwitchMainChannel = 26 => "Switch Main Channel", LoneWork = 27 => "Lone Work",
        KeypadLock = 28 => "Keypad Lock", NuisanceDelete = 29 => "Nuisance Channel Delete",
        TbstSend = 30 => "TBST Send", AprsSend = 31 => "APRS Send", ChannelType = 32 => "Channel Type",
        DisplayMode = 33 => "Display Mode", CtcScan = 34 => "CTC Scan", CtcSetting = 35 => "CTC Setting",
        SilentTone = 36 => "Silent Tone", Roaming = 37 => "Roaming", SubPtt = 38 => "Sub-PTT",
        ScrambleSwitch = 39 => "Analog Scramble Switch", OneKeyScanFreq = 40 => "One Key Scan Freq",
        Flashlight = 41 => "Flashlight", ManDown = 42 => "Man Down Alarm",
    }
}

crate::choice! {
    pub enum KeyLockMode { Manual = 0 => "Manual", Auto = 1 => "Auto" }
}

crate::choice! {
    /// Analog one-key call type (CPS `[AnalogShortcutCallType]`).
    pub enum AnalogCallType {
        Off = 0 => "Off", Dtmf = 1 => "DTMF Encode", TwoTone = 2 => "Two Tone Encode",
        FiveTone = 3 => "Five Tone Special Call",
    }
}

crate::choice! {
    /// One Touch Call / Fun+ call way.
    pub enum CallWay { Off = 0 => "Off", Analog = 1 => "Analog", Digital = 2 => "Digital" }
}

crate::choice! {
    pub enum DigitalCallType {
        Off = 0 => "Off", Private = 1 => "Private", Group = 2 => "Group", Message = 3 => "Message",
        CallAlert = 4 => "Call Alert", RadioCheck = 5 => "Radio Check", RemoteMonitor = 6 => "Remote Monitor",
        Active = 7 => "Active", Kill = 8 => "Kill",
    }
}

crate::choice! {
    pub enum FunMode { Call = 0 => "Call", Menu = 1 => "Menu" }
}

crate::choice! {
    pub enum FunMenu {
        Off = 0 => "Off", Sms = 1 => "SMS", NewSms = 2 => "New SMS", ShortcutText = 3 => "Shortcut Text",
        Inbox = 4 => "Inbox", Outbox = 5 => "Outbox", ContactList = 6 => "Contact List",
        ManualDial = 7 => "Manual Dial", CallLog = 8 => "Call Log", SentCall = 9 => "Sent Call",
        AnsweredCall = 10 => "Answered Call", MissedCall = 11 => "Missed Call", Zone = 12 => "Zone",
        RadioSetting = 13 => "Radio Setting",
    }
}

crate::choice! {
    pub enum AprsCallType { Private = 0 => "Private", Group = 1 => "Group" }
}

/// Menu items that can be hidden: `(label, offset, bit)`. Bit set = item shown. DERIVED: the
/// polarity is not pinned by a capture (factory: all 0xFF). Labels follow the spec; qdmr swaps
/// 0x502 bits 6/7 (Display Mode / Match Group) and 0x504 bits 3/4.
pub const MENU_ITEMS: &[(&str, usize, u8)] = &[
    ("Zone List", 0x500, 0),
    ("New Zone", 0x500, 1),
    ("Call Alert", 0x501, 0),
    ("Radio Check", 0x501, 1),
    ("Remote Monitor", 0x501, 2),
    ("Radio Enable", 0x501, 3),
    ("Radio Disable", 0x501, 4),
    ("Measure Period", 0x501, 5),
    ("Talkaround", 0x502, 0),
    ("Alert Tone", 0x502, 1),
    ("TX Power", 0x502, 2),
    ("Start Display", 0x502, 3),
    ("Lang Select", 0x502, 4),
    ("Match Private", 0x502, 5),
    ("Match Group", 0x502, 6),
    ("Display Mode", 0x502, 7),
    ("SMS Format", 0x503, 0),
    ("Sub Channel Mode", 0x503, 1),
    ("Power Save", 0x503, 2),
    ("FM Radio", 0x503, 3),
    ("GPS", 0x503, 4),
    ("APRS", 0x503, 5),
    ("Record", 0x503, 6),
    ("Add Contact", 0x504, 0),
    ("Del Contact", 0x504, 1),
    ("Edit Contact", 0x504, 2),
    ("Send Message", 0x504, 3),
    ("Functionality", 0x504, 4),
    ("Manual Dial", 0x504, 5),
    ("CSV Contacts", 0x504, 6),
    ("Missed Call", 0x505, 0),
    ("Answered Call", 0x505, 1),
    ("Sent Call", 0x505, 2),
    ("Del Log", 0x505, 3),
    ("RX Frequency", 0x506, 0),
    ("TX Frequency", 0x506, 1),
    ("CTC/DCS", 0x506, 2),
    ("TX Contact", 0x506, 3),
    ("Color Code", 0x506, 4),
    ("Time Slot", 0x506, 5),
    ("Radio ID", 0x506, 6),
    ("Radio Name", 0x506, 7),
    ("Channel Type", 0x507, 0),
    ("TDMA Direct Mode", 0x507, 1),
    ("RX Group List", 0x507, 2),
    ("Add Channel", 0x507, 3),
    ("Channel Name", 0x507, 4),
];

/// Number of One Touch Call entries (5 bytes each at 0x200).
pub const ONE_TOUCH_COUNT: usize = 5;
/// Number of Fun+ entries (7 bytes each at 0x230).
pub const FUN_COUNT: usize = 10;
/// Number of analog one-key call entries (2 bytes each at 0x120).
pub const ANALOG_CALL_COUNT: usize = 4;
/// Number of APRS report channels (u16 each at 0x320).
pub const APRS_CHANNELS: usize = 8;

const ONE_TOUCH: usize = 0x200;
const FUN: usize = 0x230;
const ANALOG_CALL: usize = 0x120;
/// Marker qdmr writes to enable a password.
const PASSWORD_ON: u8 = 0xA5;

macro_rules! byte {
    ($(#[$m:meta])* $get:ident, $set:ident, $off:expr) => {
        $(#[$m])*
        pub fn $get(&self) -> u8 {
            self.raw[$off]
        }
        pub fn $set(&mut self, v: u8) {
            self.raw[$off] = v;
        }
    };
}

macro_rules! flag {
    ($(#[$m:meta])* $get:ident, $set:ident, $off:expr, $bit:expr) => {
        $(#[$m])*
        pub fn $get(&self) -> bool {
            get_bits(self.raw[$off], $bit, 1) != 0
        }
        pub fn $set(&mut self, v: bool) {
            set_bits(&mut self.raw[$off], $bit, 1, v as u8);
        }
    };
}

macro_rules! pick {
    ($(#[$m:meta])* $get:ident, $set:ident, $ty:ident, $off:expr, $shift:expr, $width:expr) => {
        $(#[$m])*
        pub fn $get(&self) -> $ty {
            $ty::from_raw(get_bits(self.raw[$off], $shift, $width))
        }
        pub fn $set(&mut self, v: $ty) {
            set_bits(&mut self.raw[$off], $shift, $width, v.to_raw());
        }
    };
}

macro_rules! text {
    ($(#[$m:meta])* $get:ident, $set:ident, $off:expr, $len:expr, $max:expr) => {
        $(#[$m])*
        pub fn $get(&self) -> String {
            get_str(&self.raw[$off..$off + $len])
        }
        /// Unchanged text leaves the field alone (bytes after the NUL are not always clean).
        pub fn $set(&mut self, s: &str) {
            if self.$get() == s {
                return;
            }
            put_str(&mut self.raw[$off..$off + $len], s, $max, 0x00);
        }
    };
}

impl Settings {
    // ---- Power-on (0x000-0x02F) ----

    pick!(
        /// 0x000. CONFIRMED.
        power_on_display, set_power_on_display, PowerOnDisplay, 0x000, 0, 8
    );
    text!(
        /// 0x001, 14-byte field, 13 chars + NUL written. CONFIRMED.
        power_on_line1, set_power_on_line1, 0x001, 14, 13
    );
    text!(
        /// 0x00F, 14-byte field, 13 chars + NUL written. CONFIRMED.
        power_on_line2, set_power_on_line2, 0x00F, 14, 13
    );
    flag!(
        /// 0x01D bit 0 "Allow Reset" (qdmr: MCU reset, whole byte 0/1). DERIVED.
        allow_reset, set_allow_reset, 0x01D, 0
    );
    pick!(
        /// 0x01E. DERIVED (spec and qdmr agree).
        auto_power_off, set_auto_power_off, AutoPowerOff, 0x01E, 0, 8
    );

    // Tones: 0x020/0x021 bit assignment from qdmr. The spec's labels (from another tool's UI
    // schema) leave 0x021 bits 6-7 unexplained although the factory dump sets them; qdmr's map
    // explains every set bit of the factory 0x7F/0xC0 sensibly. DERIVED.
    flag!(
        /// 0x020 bit 7: all tones off.
        radio_silent, set_radio_silent, 0x020, 7
    );
    flag!(
        /// 0x020 bit 6.
        key_tone, set_key_tone, 0x020, 6
    );
    flag!(
        /// 0x020 bit 5.
        sms_tone, set_sms_tone, 0x020, 5
    );
    flag!(
        /// 0x020 bit 4.
        group_call_tone, set_group_call_tone, 0x020, 4
    );
    flag!(
        /// 0x020 bit 3.
        private_call_tone, set_private_call_tone, 0x020, 3
    );
    flag!(
        /// 0x020 bit 2: call end (EOT) tone.
        call_end_tone, set_call_end_tone, 0x020, 2
    );
    flag!(
        /// 0x020 bit 1.
        talk_permit_tone, set_talk_permit_tone, 0x020, 1
    );
    flag!(
        /// 0x020 bit 0: start-up sound.
        startup_tone, set_startup_tone, 0x020, 0
    );
    flag!(
        /// 0x021 bit 7.
        voice_prompt, set_voice_prompt, 0x021, 7
    );
    flag!(
        /// 0x021 bit 6.
        low_battery_tone, set_low_battery_tone, 0x021, 6
    );
    pick!(
        /// 0x021 bits 1-2.
        roger_tone, set_roger_tone, RogerTone, 0x021, 1, 2
    );

    // ---- Display / UI (0x030-0x03B) ----

    byte!(
        /// 0x030 backlight brightness, stored 0-5, shown 1-6. CONFIRMED.
        brightness, set_brightness, 0x030
    );
    pick!(
        /// 0x031. qdmr's 12-step list; the spec's "(raw+1)*5 s" is DERIVED from model comments.
        backlight_time, set_backlight_time, BacklightTime, 0x031, 0, 8
    );
    byte!(
        /// 0x032 menu exit time in units of 5 s, 0 = never (qdmr). The spec has it unknown and
        /// puts "Menu Exit Time" at 0x036, which the factory dump (0x00) contradicts. DERIVED.
        menu_exit_time, set_menu_exit_time, 0x032
    );
    flag!(
        /// 0x033 bit 0 (qdmr; spec: "Volume Change Prompt"). DERIVED.
        show_clock, set_show_clock, 0x033, 0
    );
    pick!(
        /// 0x033 bit 3. Both sources agree. DERIVED.
        date_format, set_date_format, DateFormat, 0x033, 3, 1
    );
    flag!(
        /// 0x033 bit 4 (qdmr; spec leaves it unknown although set in the factory dump). DERIVED.
        volume_prompt, set_volume_prompt, 0x033, 4
    );
    pick!(
        /// 0x034 low nibble. CONFIRMED.
        call_color, set_call_color, Color, 0x034, 0, 4
    );
    pick!(
        /// 0x035 low nibble. CONFIRMED.
        standby_color, set_standby_color, Color, 0x035, 0, 4
    );
    pick!(
        /// 0x038 low nibble. CONFIRMED.
        channel_a_color, set_channel_a_color, Color, 0x038, 0, 4
    );
    pick!(
        /// 0x039 low nibble. CONFIRMED.
        channel_b_color, set_channel_b_color, Color, 0x039, 0, 4
    );
    pick!(
        /// 0x03A low nibble. CONFIRMED.
        zone_a_color, set_zone_a_color, Color, 0x03A, 0, 4
    );
    pick!(
        /// 0x03B low nibble. CONFIRMED.
        zone_b_color, set_zone_b_color, Color, 0x03B, 0, 4
    );

    // ---- GPS (0x040-0x045) ----
    // Spec bit layout ("confirmed via CPS RE"). qdmr puts the GNSS mode at bits 3-4 and has no
    // unit fields; its time zone is raw-11, which would make the factory value UTC+9 instead of
    // the spec's (Chinese factory default) UTC+8.

    flag!(
        /// 0x040 bit 0. CONFIRMED.
        gps_on, set_gps_on, 0x040, 0
    );
    pick!(
        /// 0x040 bit 1. CONFIRMED (spec).
        distance_unit, set_distance_unit, DistanceUnit, 0x040, 1, 1
    );
    pick!(
        /// 0x040 bits 2-3. CONFIRMED (spec); qdmr disagrees (bits 3-4).
        gps_mode, set_gps_mode, GpsMode, 0x040, 2, 2
    );
    pick!(
        /// 0x040 bits 4-5. CONFIRMED (spec).
        speed_unit, set_speed_unit, SpeedUnit, 0x040, 4, 2
    );
    pick!(
        /// 0x040 bit 6. CONFIRMED.
        position_format, set_position_format, PositionFormat, 0x040, 6, 1
    );
    byte!(
        /// 0x041 time zone index 0-25 = UTC-12 … UTC+13 (see [`Settings::utc_offset`]). CONFIRMED.
        time_zone, set_time_zone, 0x041
    );
    byte!(
        /// 0x042 GPS report interval, seconds (spec; qdmr says minutes). CONFIRMED offset.
        gps_interval, set_gps_interval, 0x042
    );
    pick!(
        /// 0x043 bits 2-3 (qdmr only). DERIVED.
        record_mode, set_record_mode, RecordMode, 0x043, 2, 2
    );
    flag!(
        /// 0x045 bit 0 voice recording (qdmr only; factory 0x01). DERIVED.
        recording, set_recording, 0x045, 0
    );

    /// Time zone as hours from UTC.
    pub fn utc_offset(&self) -> i8 {
        self.time_zone() as i8 - 12
    }

    pub fn set_utc_offset(&mut self, hours: i8) {
        self.set_time_zone((hours.clamp(-12, 13) + 12) as u8);
    }

    /// Time zone as the CPS shows it (`UTC`, `UTC +8:00`, …).
    pub fn utc_label(&self) -> String {
        match self.time_zone() {
            12 => "UTC".into(),
            z if z <= 25 => format!("UTC {:+}:00", z as i8 - 12),
            z => format!("Unknown ({z})"),
        }
    }

    // ---- Digital (0x060-0x067) ----

    flag!(
        /// 0x060 bit 0. CONFIRMED.
        private_call_match, set_private_call_match, 0x060, 0
    );
    flag!(
        /// 0x060 bit 1. CONFIRMED.
        group_call_match, set_group_call_match, 0x060, 1
    );
    byte!(
        /// 0x061 call hold time (spec: seconds; qdmr: ×500 ms). Raw.
        call_hold_time, set_call_hold_time, 0x061
    );
    byte!(
        /// 0x062 active wait time; ms ≈ raw*30 + 300 (qdmr) or (raw-1)*30 + 300 (spec). Raw.
        active_wait_time, set_active_wait_time, 0x062
    );
    byte!(
        /// 0x063 active retries, 1-8. CONFIRMED.
        active_retries, set_active_retries, 0x063
    );
    byte!(
        /// 0x064 pre-carrier (preamble) time, ms = (raw+1)*120. Both sources agree.
        pre_carrier_time, set_pre_carrier_time, 0x064
    );
    flag!(
        /// 0x065 bit 0. CONFIRMED.
        missed_call_alert, set_missed_call_alert, 0x065, 0
    );
    pick!(
        /// 0x065 bits 1-2 (qdmr; spec: unenumerated "Data Service"). DERIVED.
        sms_format, set_sms_format, SmsFormat, 0x065, 1, 2
    );
    flag!(
        /// 0x065 bit 3. CONFIRMED.
        call_alert_decode, set_call_alert_decode, 0x065, 3
    );
    flag!(
        /// 0x065 bit 4. CONFIRMED.
        radio_enable_decode, set_radio_enable_decode, 0x065, 4
    );
    flag!(
        /// 0x065 bit 5. CONFIRMED.
        radio_check_decode, set_radio_check_decode, 0x065, 5
    );
    flag!(
        /// 0x065 bit 6. CONFIRMED.
        radio_disable_decode, set_radio_disable_decode, 0x065, 6
    );
    flag!(
        /// 0x065 bit 7. CONFIRMED.
        remote_monitor_decode, set_remote_monitor_decode, 0x065, 7
    );
    byte!(
        /// 0x066 remote monitor duration, s = (raw+1)*10 (qdmr; spec calls it "SMS Format" with
        /// the same scaling). DERIVED.
        remote_monitor_time, set_remote_monitor_time, 0x066
    );
    pick!(
        /// 0x067 bit 2. Both sources agree on the bit.
        name_display_priority, set_name_display_priority, NameDisplayPriority, 0x067, 2, 1
    );
    flag!(
        /// 0x067 bit 3 CPS "TX Call Name" (talker alias). Both agree.
        send_talker_alias, set_send_talker_alias, 0x067, 3
    );
    pick!(
        /// 0x067 bit 4 (qdmr; spec has the "Name Data Format" at bits 6-7). DERIVED.
        talker_alias_encoding, set_talker_alias_encoding, TalkerAliasEncoding, 0x067, 4, 1
    );

    // ---- VFO / dual standby (0x080-0x081), qdmr only. DERIVED. ----

    flag!(
        /// 0x080 bit 0.
        vfo_disabled, set_vfo_disabled, 0x080, 0
    );
    pick!(
        /// 0x080 bit 1.
        vfo_mode_a, set_vfo_mode_a, VfoMode, 0x080, 1, 1
    );
    pick!(
        /// 0x080 bit 2.
        vfo_mode_b, set_vfo_mode_b, VfoMode, 0x080, 2, 1
    );
    pick!(
        /// 0x080 bit 3.
        display_a, set_display_a, VfoDisplay, 0x080, 3, 1
    );
    pick!(
        /// 0x080 bit 4.
        display_b, set_display_b, VfoDisplay, 0x080, 4, 1
    );
    pick!(
        /// 0x080 bit 5.
        main_vfo, set_main_vfo, Vfo, 0x080, 5, 1
    );
    pick!(
        /// 0x080 bits 6-7.
        dual_standby, set_dual_standby, DualStandby, 0x080, 6, 2
    );
    byte!(
        /// 0x081 (qdmr: dual-standby hang time ×500 ms; spec: "TX Dwell Time"). Raw.
        dual_standby_hang_time, set_dual_standby_hang_time, 0x081
    );

    // ---- Keys (0x085-0x093) ----

    pick!(
        /// 0x085 bit 0. CONFIRMED.
        key_lock_mode, set_key_lock_mode, KeyLockMode, 0x085, 0, 1
    );
    flag!(
        /// 0x085 bit 1. CONFIRMED.
        knob_lock, set_knob_lock, 0x085, 1
    );
    flag!(
        /// 0x085 bit 2. CONFIRMED.
        side_key_lock, set_side_key_lock, 0x085, 2
    );
    byte!(
        /// 0x086 auto keypad lock delay (spec: seconds 5-60; qdmr: minutes raw+5). Raw.
        auto_key_lock_delay, set_auto_key_lock_delay, 0x086
    );
    pick!(
        /// 0x087. CONFIRMED.
        sk1_short, set_sk1_short, KeyFunction, 0x087, 0, 8
    );
    pick!(
        /// 0x088. CONFIRMED.
        sk1_long, set_sk1_long, KeyFunction, 0x088, 0, 8
    );
    pick!(
        /// 0x089. CONFIRMED.
        sk2_short, set_sk2_short, KeyFunction, 0x089, 0, 8
    );
    pick!(
        /// 0x08A. CONFIRMED.
        sk2_long, set_sk2_long, KeyFunction, 0x08A, 0, 8
    );
    pick!(
        /// 0x08D. CONFIRMED.
        p1_short, set_p1_short, KeyFunction, 0x08D, 0, 8
    );
    pick!(
        /// 0x08E. CONFIRMED.
        p1_long, set_p1_long, KeyFunction, 0x08E, 0, 8
    );
    pick!(
        /// 0x08F. CONFIRMED.
        p2_short, set_p2_short, KeyFunction, 0x08F, 0, 8
    );
    pick!(
        /// 0x090. CONFIRMED.
        p2_long, set_p2_long, KeyFunction, 0x090, 0, 8
    );
    byte!(
        /// 0x093 long-press time, stored 0-4, shown 1-5 s (spec; factory 0x02). qdmr reads 0x094,
        /// which is 0x00 in the factory dump. DERIVED.
        long_press_time, set_long_press_time, 0x093
    );

    // ---- TX / audio (0x0A0-0x0A7): qdmr only; the spec treats it as an opaque blob. The
    // factory values (TOT 22, VOX off, delay 10, TBST 1750 Hz, mic 4/3) are plausible. DERIVED.

    byte!(
        /// 0x0A0 TX timeout, 0 = off, else s = raw*5 + 15.
        tot, set_tot, 0x0A0
    );
    byte!(
        /// 0x0A1 TX timeout pre-alert, seconds.
        tot_alert, set_tot_alert, 0x0A1
    );
    byte!(
        /// 0x0A2 VOX level, 0 = off, 1-5.
        vox_level, set_vox_level, 0x0A2
    );
    byte!(
        /// 0x0A3 VOX delay, ×100 ms.
        vox_delay, set_vox_delay, 0x0A3
    );
    flag!(
        /// 0x0A4 bit 0: all LEDs off.
        leds_off, set_leds_off, 0x0A4, 0
    );
    flag!(
        /// 0x0A4 bit 2: NOAA weather alarm.
        weather_alarm, set_weather_alarm, 0x0A4, 2
    );
    pick!(
        /// 0x0A4 bits 4-5.
        power_save, set_power_save, PowerSave, 0x0A4, 4, 2
    );
    pick!(
        /// 0x0A5 bits 0-1.
        ste, set_ste, Ste, 0x0A5, 0, 2
    );
    pick!(
        /// 0x0A5 bits 4-5.
        tbst_freq, set_tbst_freq, TbstFreq, 0x0A5, 4, 2
    );
    byte!(
        /// 0x0A6 analog mic gain, 0-4.
        fm_mic_gain, set_fm_mic_gain, 0x0A6
    );
    byte!(
        /// 0x0A7 digital mic gain, 0-4.
        dmr_mic_gain, set_dmr_mic_gain, 0x0A7
    );

    // ---- One-key operation ----

    /// Analog one-key call `i` (0-3, 2 bytes at 0x120): call type. The factory dump
    /// `01 01 02 01 03 01 00 00` reads as types DTMF / Two Tone / Five Tone, each with call ID 1,
    /// once the CPS type list (4 options, not the spec's 3) is known. DERIVED.
    pub fn analog_call_type(&self, i: usize) -> AnalogCallType {
        AnalogCallType::from_raw(self.raw[ANALOG_CALL + 2 * i])
    }

    pub fn set_analog_call_type(&mut self, i: usize, v: AnalogCallType) {
        self.raw[ANALOG_CALL + 2 * i] = v.to_raw();
    }

    /// Analog one-key call `i`: call ID (index into the selected code list). DERIVED.
    pub fn analog_call_id(&self, i: usize) -> u8 {
        self.raw[ANALOG_CALL + 2 * i + 1]
    }

    pub fn set_analog_call_id(&mut self, i: usize, v: u8) {
        self.raw[ANALOG_CALL + 2 * i + 1] = v;
    }

    /// One Touch Call `i` (0-4): call way. Layout CONFIRMED by the factory dump.
    pub fn one_touch_way(&self, i: usize) -> CallWay {
        CallWay::from_raw(self.raw[ONE_TOUCH + 5 * i])
    }

    pub fn set_one_touch_way(&mut self, i: usize, v: CallWay) {
        self.raw[ONE_TOUCH + 5 * i] = v.to_raw();
    }

    /// One Touch Call `i`: call object (contact / talk group index), u16 LE.
    pub fn one_touch_object(&self, i: usize) -> u16 {
        get_u16(&self.raw[ONE_TOUCH + 5 * i + 1..])
    }

    pub fn set_one_touch_object(&mut self, i: usize, v: u16) {
        put_u16(&mut self.raw[ONE_TOUCH + 5 * i + 1..], v);
    }

    /// One Touch Call `i`: digital call type.
    pub fn one_touch_call_type(&self, i: usize) -> DigitalCallType {
        DigitalCallType::from_raw(self.raw[ONE_TOUCH + 5 * i + 3])
    }

    pub fn set_one_touch_call_type(&mut self, i: usize, v: DigitalCallType) {
        self.raw[ONE_TOUCH + 5 * i + 3] = v.to_raw();
    }

    /// One Touch Call `i`: SMS (quick text) number.
    pub fn one_touch_sms(&self, i: usize) -> u8 {
        self.raw[ONE_TOUCH + 5 * i + 4]
    }

    pub fn set_one_touch_sms(&mut self, i: usize, v: u8) {
        self.raw[ONE_TOUCH + 5 * i + 4] = v;
    }

    /// Fun+ entry `i` (0-9): operate mode. CONFIRMED by the factory dump.
    pub fn fun_mode(&self, i: usize) -> FunMode {
        FunMode::from_raw(self.raw[FUN + 7 * i])
    }

    pub fn set_fun_mode(&mut self, i: usize, v: FunMode) {
        self.raw[FUN + 7 * i] = v.to_raw();
    }

    /// Fun+ entry `i`: menu shortcut. CONFIRMED.
    pub fn fun_menu(&self, i: usize) -> FunMenu {
        FunMenu::from_raw(self.raw[FUN + 7 * i + 1])
    }

    pub fn set_fun_menu(&mut self, i: usize, v: FunMenu) {
        self.raw[FUN + 7 * i + 1] = v.to_raw();
    }

    /// Fun+ entry `i`: call way. DERIVED.
    pub fn fun_way(&self, i: usize) -> CallWay {
        CallWay::from_raw(self.raw[FUN + 7 * i + 3])
    }

    pub fn set_fun_way(&mut self, i: usize, v: CallWay) {
        self.raw[FUN + 7 * i + 3] = v.to_raw();
    }

    /// Fun+ entry `i`: call object (single byte). DERIVED.
    pub fn fun_object(&self, i: usize) -> u8 {
        self.raw[FUN + 7 * i + 4]
    }

    pub fn set_fun_object(&mut self, i: usize, v: u8) {
        self.raw[FUN + 7 * i + 4] = v;
    }

    /// Fun+ entry `i`: digital call type. DERIVED.
    pub fn fun_call_type(&self, i: usize) -> DigitalCallType {
        DigitalCallType::from_raw(self.raw[FUN + 7 * i + 5])
    }

    pub fn set_fun_call_type(&mut self, i: usize, v: DigitalCallType) {
        self.raw[FUN + 7 * i + 5] = v.to_raw();
    }

    /// Fun+ entry `i`: SMS number. DERIVED.
    pub fn fun_sms(&self, i: usize) -> u8 {
        self.raw[FUN + 7 * i + 6]
    }

    pub fn set_fun_sms(&mut self, i: usize, v: u8) {
        self.raw[FUN + 7 * i + 6] = v;
    }

    // ---- APRS and fixed position (0x300-0x334) ----

    byte!(
        /// 0x301 APRS scheduled send, 0 = off, else ×30 s. Both sources agree. DERIVED.
        aprs_interval, set_aprs_interval, 0x301
    );
    flag!(
        /// 0x302 bit 0 fixed beacon / fixed position (qdmr: byte != 0). DERIVED.
        aprs_fixed, set_aprs_fixed, 0x302, 0
    );
    text!(
        /// 0x306 fixed latitude, 9 ASCII chars (e.g. "23.000000"). CONFIRMED.
        latitude, set_latitude, 0x306, 9, 9
    );
    text!(
        /// 0x310 fixed longitude, 9 ASCII chars (e.g. "118.00000"). CONFIRMED.
        longitude, set_longitude, 0x310, 9, 9
    );
    byte!(
        /// 0x30F latitude direction, ASCII 'N'/'S' (0 = unset). CONFIRMED.
        latitude_dir, set_latitude_dir, 0x30F
    );
    byte!(
        /// 0x319 longitude direction, ASCII 'E'/'W' (0 = unset). CONFIRMED.
        longitude_dir, set_longitude_dir, 0x319
    );

    /// APRS report channel `i` (0-7), u16 LE at 0x320: 0 = current channel, else index + 1.
    /// DERIVED.
    pub fn aprs_channel(&self, i: usize) -> u16 {
        get_u16(&self.raw[0x320 + 2 * i..])
    }

    pub fn set_aprs_channel(&mut self, i: usize, v: u16) {
        put_u16(&mut self.raw[0x320 + 2 * i..], v);
    }

    byte!(
        /// 0x330 repeater activation (pre-wave) delay, ×100 ms. DERIVED.
        aprs_tx_delay, set_aprs_tx_delay, 0x330
    );
    pick!(
        /// 0x331 bit 0. DERIVED.
        aprs_call_type, set_aprs_call_type, AprsCallType, 0x331, 0, 1
    );

    /// 0x332 APRS upload (destination) ID, 24-bit LE (factory 456; qdmr also LE). DERIVED.
    pub fn aprs_id(&self) -> u32 {
        get_u24(&self.raw[0x332..])
    }

    pub fn set_aprs_id(&mut self, v: u32) {
        put_u24(&mut self.raw[0x332..], v.min(0xFF_FFFF));
    }

    // ---- Passwords (0x430-0x44A), qdmr only: enabled when the flag byte is 0xA5. DERIVED. ----

    /// 0x430 power-on password enabled.
    pub fn power_on_password_enabled(&self) -> bool {
        self.raw[0x430] == PASSWORD_ON
    }

    pub fn set_power_on_password_enabled(&mut self, on: bool) {
        self.raw[0x430] = if on { PASSWORD_ON } else { 0 };
    }

    text!(
        /// 0x431, 8 chars.
        power_on_password, set_power_on_password, 0x431, 8, 8
    );

    /// 0x439 programming (write) password enabled.
    pub fn write_password_enabled(&self) -> bool {
        self.raw[0x439] == PASSWORD_ON
    }

    pub fn set_write_password_enabled(&mut self, on: bool) {
        self.raw[0x439] = if on { PASSWORD_ON } else { 0 };
    }

    /// 0x43A read password enabled.
    pub fn read_password_enabled(&self) -> bool {
        self.raw[0x43A] == PASSWORD_ON
    }

    pub fn set_read_password_enabled(&mut self, on: bool) {
        self.raw[0x43A] = if on { PASSWORD_ON } else { 0 };
    }

    text!(
        /// 0x43B, 8 chars.
        write_password, set_write_password, 0x43B, 8, 8
    );
    text!(
        /// 0x443, 8 chars.
        read_password, set_read_password, 0x443, 8, 8
    );

    // ---- Menu (0x500-0x507) ----

    /// Whether menu item `i` of [`MENU_ITEMS`] is shown.
    pub fn menu_enabled(&self, i: usize) -> bool {
        let (_, off, bit) = MENU_ITEMS[i];
        get_bits(self.raw[off], bit, 1) != 0
    }

    pub fn set_menu_enabled(&mut self, i: usize, on: bool) {
        let (_, off, bit) = MENU_ITEMS[i];
        set_bits(&mut self.raw[off], bit, 1, on as u8);
    }
}

/// The settings block, or `None` if the image has none. Never fabricate one: unknown bytes must
/// come from the radio.
pub fn decode(img: &Image) -> Option<Settings> {
    img.block(block::SETTINGS)
        .map(|p| Settings::from_bytes(&p[..]))
}

/// Store `s` as block 0x04 (all bytes come from `s`, which started as the block itself).
pub fn encode(s: &Settings, img: &mut Image) {
    let p = img.block_mut(block::SETTINGS);
    p.copy_from_slice(&s.raw);
    p[PAGE - 1] = block::SETTINGS;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::Choice;

    fn fixture() -> Image {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/factory-v1.60.data"
        );
        crate::image::data_file::import(&std::fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn decode_fixture() {
        let img = fixture();
        let s = decode(&img).unwrap();
        assert_eq!(s.power_on_display(), PowerOnDisplay::Picture);
        assert_eq!(s.power_on_line1(), "Welcome");
        assert_eq!(s.power_on_line2(), "DM-32UV");
        assert_eq!(s.sk1_short(), KeyFunction::FmRadio);
        assert_eq!(s.sk1_long(), KeyFunction::GpsInformation);
        assert_eq!(s.sk2_short(), KeyFunction::Monitor);
        assert_eq!(s.sk2_long(), KeyFunction::PowerSelect);
        assert_eq!(s.p1_short(), KeyFunction::Volt);
        assert_eq!(s.p1_long(), KeyFunction::Contacts);
        assert_eq!(s.p2_short(), KeyFunction::VM);
        assert_eq!(s.p2_long(), KeyFunction::OneKeyScanFreq);
        assert_eq!(s.long_press_time(), 2);
        assert!(s.gps_on());
        assert_eq!(s.utc_offset(), 1);
        assert_eq!(s.gps_interval(), 5);
        assert!(s.private_call_match() && s.group_call_match());
        assert_eq!(s.active_retries(), 3);
        assert_eq!(s.latitude(), "23.000000");
        assert_eq!(s.longitude(), "118.00000");
        assert_eq!(s.aprs_id(), 456);
        assert_eq!(s.utc_label(), "UTC +1:00");
        assert_eq!(s.analog_call_type(2), AnalogCallType::FiveTone);
        assert_eq!(s.analog_call_id(2), 1);
        assert_eq!(s.analog_call_type(3), AnalogCallType::Off);
        assert_eq!(s.dual_standby(), DualStandby::Single);
        assert_eq!(s.vfo_mode_a(), VfoMode::Mr);
        assert_eq!(s.one_touch_way(0), CallWay::Digital);
        assert_eq!(s.one_touch_call_type(1), DigitalCallType::Message);
        assert_eq!(s.one_touch_sms(1), 4);
        assert_eq!(s.fun_mode(1), FunMode::Menu);
        assert_eq!(s.fun_menu(9), FunMenu::RadioSetting);
        assert_eq!(s.tbst_freq(), TbstFreq::Hz1750);
        assert!(s.key_tone() && !s.radio_silent() && s.voice_prompt());
        assert!((0..MENU_ITEMS.len()).all(|i| s.menu_enabled(i)));
        assert!(!s.power_on_password_enabled());
    }

    #[test]
    fn round_trip_identical() {
        let img = fixture();
        let s = decode(&img).unwrap();
        let mut out = img.clone();
        encode(&s, &mut out);
        assert!(out == img);

        // Writing every field back with its own value changes nothing.
        let mut t = s.clone();
        t.set_power_on_line1(&s.power_on_line1());
        t.set_power_on_line2(&s.power_on_line2());
        t.set_power_on_display(s.power_on_display());
        t.set_backlight_time(s.backlight_time());
        t.set_call_color(s.call_color());
        t.set_gps_mode(s.gps_mode());
        t.set_utc_offset(s.utc_offset());
        t.set_sms_format(s.sms_format());
        t.set_dual_standby(s.dual_standby());
        t.set_sk1_short(s.sk1_short());
        t.set_power_save(s.power_save());
        t.set_ste(s.ste());
        t.set_latitude(&s.latitude());
        t.set_longitude(&s.longitude());
        t.set_aprs_id(s.aprs_id());
        for i in 0..ONE_TOUCH_COUNT {
            t.set_one_touch_way(i, s.one_touch_way(i));
            t.set_one_touch_object(i, s.one_touch_object(i));
        }
        for i in 0..FUN_COUNT {
            t.set_fun_menu(i, s.fun_menu(i));
        }
        for i in 0..MENU_ITEMS.len() {
            t.set_menu_enabled(i, s.menu_enabled(i));
        }
        assert!(t == s, "setters changed bytes");
    }

    #[test]
    fn setters_touch_only_their_bits() {
        let mut s = Settings::from_bytes(&[0xA5; PAGE]);
        s.set_power_on_line1("Hi");
        assert_eq!(&s.bytes()[0x01..0x0F], b"Hi\0\0\0\0\0\0\0\0\0\0\0\0");
        assert_eq!(s.bytes()[0x00], 0xA5);
        assert_eq!(s.bytes()[0x0F], 0xA5);
        s.set_call_color(Color::Blue);
        assert_eq!(s.bytes()[0x034], 0xA7);
        s.set_gps_mode(GpsMode::Both);
        assert_eq!(s.bytes()[0x040], 0b1010_1001);
        s.set_sk2_long(KeyFunction::Other(99));
        assert_eq!(s.sk2_long().label(), "Unknown (99)");
        s.set_latitude("12.345678");
        s.set_latitude_dir(b'S');
        assert_eq!(&s.bytes()[0x306..0x310], b"12.345678S");
        s.set_utc_offset(-5);
        assert_eq!(s.time_zone(), 7);
    }
}
