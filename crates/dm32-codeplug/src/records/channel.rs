//! Channels: the channel bank (blocks `0x12..=0x41`), the per-channel TX contact (blocks
//! `0x42`/`0x43`) and the two VFO records.
//!
//! # Bank layout
//!
//! Block `0x12` starts with a 16-byte header (u16 LE channel count at `0x00`, bytes `0x02..0x10`
//! unknown and preserved) followed by 84 records; blocks `0x13..=0x41` hold 85 records from
//! offset 0. Records are positional: a blank record still takes its channel number, so holes are
//! kept as blank [`Channel`]s. The count governs what the radio reads; record bytes past it are
//! ignored.
//!
//! VFO A and B are ordinary 48-byte records at `0xF9F` / `0xFCF` of block `0x41`. They collide with
//! channel records 4078/4079, which the 4000-channel limit never reaches.
//!
//! # Field sources
//!
//! The spec (05-DATA-STRUCTURES) and qdmr disagree on several bits. The factory fixture settles
//! some of them (its channels are named after the feature they demonstrate, e.g.
//! "Digital Encrypt", "5 Tone Call"); where it does, that wins and the accessor says so.
//!
//! Option labels and order come from the CPS v1.60 UI strings (`[ChannelMode]`, `[PowerSelect]`,
//! `[Bandwidth]`, `[ChannelSlot]`, `[ChDigTxAdmit]`, `[ChAnaTxAdmit]`, `[RxSquelchMode]`,
//! `[ChannelPttId]`, `[ChannelStepFreq]`, `[ChannelSignalingType]`, `[ChannelAprsReport]`); that
//! the list position equals the stored value is inferred, and agrees with the fixture wherever it
//! can be checked.

use std::ops::{Deref, DerefMut};

use crate::choice;
use crate::encoding::{
    Freq, Tone, get_bits, get_freq, get_str, get_tone, get_u16, is_blank, put_freq, put_str,
    put_tone, put_u16, set_bits,
};
use crate::image::{Image, PAGE, block};
use crate::raw_record;

/// Channel limit of the radio (4079 records would fit).
pub const MAX_CHANNELS: usize = 4000;
/// Records in block `0x12` (after the 16-byte header).
pub const FIRST_SLOT_RECORDS: usize = 84;
/// Records in each of blocks `0x13..=0x41`.
pub const SLOT_RECORDS: usize = 85;
const HEADER: usize = 0x10;
/// Offsets of VFO A and VFO B in block `0x41`.
pub const VFO_OFFSETS: [usize; 2] = [0xF9F, 0xFCF];
/// Offsets of the VFO A / B TX contacts in block `0x43`.
pub const VFO_TX_CONTACT_OFFSETS: [usize; 2] = [0xFFA, 0xFFC];
/// TX contact entries per block (`0x42` holds channels 1..=2047, `0x43` the rest).
pub const TX_CONTACTS_PER_BLOCK: usize = 2047;

choice! {
    /// Byte `0x18` bits 7-4.
    pub enum ChannelMode {
        Analog = 0 => "Analog",
        Digital = 1 => "Digital",
        FixedAnalog = 2 => "Fixed Analog",
        FixedDigital = 3 => "Fixed Digital",
    }
}

impl ChannelMode {
    /// Digital or Fixed Digital. Selects the meaning of the dual-purpose bytes `0x1D`/`0x1F`.
    pub fn is_digital(self) -> bool {
        matches!(self, ChannelMode::Digital | ChannelMode::FixedDigital)
    }

    pub fn is_analog(self) -> bool {
        matches!(self, ChannelMode::Analog | ChannelMode::FixedAnalog)
    }
}

choice! {
    /// Byte `0x18` bits 2-1.
    pub enum Power { Low = 0 => "Low", Medium = 1 => "Middle", High = 2 => "High" }
}

choice! {
    /// Byte `0x19` bit 7. Analog only; DMR is always 12.5 kHz.
    pub enum Bandwidth { Narrow = 0 => "12.5KHz", Wide = 1 => "25KHz" }
}

choice! {
    /// Byte `0x1A` bits 5-4 on digital channels (CPS `[ChDigTxAdmit]`).
    pub enum DigitalTxAdmit {
        Always = 0 => "Always",
        ChannelIdle = 1 => "Channel Idle",
        ColorCodeIdle = 2 => "Color Code Idle",
    }
}

choice! {
    /// Byte `0x1A` bits 5-4 on analog channels (CPS `[ChAnaTxAdmit]`).
    pub enum AnalogTxAdmit {
        AllowTx = 0 => "Allow TX",
        ChannelIdle = 1 => "Channel Idle",
        MatchCtc = 2 => "Match CTC",
        NonMatchCtc = 3 => "Non Match CTC",
    }
}

choice! {
    /// Byte `0x1C` bits 3-2. The spec lists 2 = Analog; the CPS has that option commented out.
    pub enum AprsReport { Off = 0 => "Off", Digital = 1 => "Digital" }
}

choice! {
    /// Byte `0x1D` bit 4 (digital channels). Confirmed by channels named after their slot.
    pub enum TimeSlot { Ts1 = 0 => "Slot 1", Ts2 = 1 => "Slot 2" }
}

choice! {
    /// Byte `0x26` bits 6-4.
    pub enum RxSquelchMode {
        CarrierCtc = 0 => "Carrier/CTC",
        Optional = 1 => "Optional Signaling",
        CtcAndOptional = 2 => "CTC & Optional Signaling",
        CtcOrOptional = 3 => "CTC | Optional Signaling",
    }
}

choice! {
    /// Byte `0x26` bits 3-1 (optional signalling).
    pub enum Signaling {
        None = 0 => "None",
        Dtmf = 1 => "DTMF",
        TwoTone = 2 => "Two Tone",
        FiveTone = 3 => "Five Tone",
        Mdc1200 = 4 => "BDC1200",
    }
}

choice! {
    /// Byte `0x27` bits 7-4. ⚠️ Derived; not attested on hardware.
    pub enum Step {
        K2_5 = 0 => "2.5K",
        K5 = 1 => "5K",
        K6_25 = 2 => "6.25K",
        K10 = 3 => "10K",
        K12_5 = 4 => "12.5K",
        K25 = 5 => "25K",
        K50 = 6 => "50K",
        K100 = 7 => "100K",
    }
}

choice! {
    /// Byte `0x29` bits 3-2. ⚠️ See [`Channel::ptt_id`].
    pub enum PttId { Off = 0 => "OFF", Bot = 1 => "BOT", Eot = 2 => "EOT", Both = 3 => "BOTH" }
}

/// One 48-byte channel record (also used for the VFOs).
///
/// Index fields are exposed as 0-based positions in the referenced list, `None` for "none";
/// the stored base is documented per accessor. Bytes `0x1D` and `0x1F` mean different things on
/// analog and digital channels: their accessors return `None` when the channel is in the other
/// mode, and their setters write the shared bits regardless of mode. Changing the mode does not
/// rewrite them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    raw: [u8; 48],
}

raw_record!(Channel, 48, 0xFF);

impl Channel {
    /// A new channel modelled on the factory defaults (high power, squelch 3, no tones; analog
    /// wide, digital colour code 1 / slot 1).
    pub fn new(name: &str, rx: Freq, mode: ChannelMode) -> Channel {
        let mut c = Channel { raw: [0; 48] };
        c.set_name(name);
        c.set_rx_freq(rx);
        c.set_tx_freq(Some(rx));
        c.set_mode(mode);
        c.set_power(Power::High);
        c.set_squelch(3);
        c.set_rx_tone(Tone::Off);
        c.set_tx_tone(Tone::Off);
        if mode.is_digital() {
            c.set_color_code(1);
        } else {
            c.set_bandwidth(Bandwidth::Wide);
        }
        c
    }

    /// All `0x00`/`0xFF`: an unused slot.
    pub fn is_empty(&self) -> bool {
        is_blank(&self.raw)
    }

    fn bits(&self, off: usize, shift: u8, width: u8) -> u8 {
        get_bits(self.raw[off], shift, width)
    }

    fn put_bits(&mut self, off: usize, shift: u8, width: u8, v: u8) {
        set_bits(&mut self.raw[off], shift, width, v);
    }

    fn flag(&self, off: usize, bit: u8) -> bool {
        self.bits(off, bit, 1) != 0
    }

    fn set_flag(&mut self, off: usize, bit: u8, on: bool) {
        self.put_bits(off, bit, 1, on as u8);
    }

    fn digital(&self) -> bool {
        self.mode().is_digital()
    }

    /// 0x00-0x0F, ASCII, NUL-terminated when shorter.
    pub fn name(&self) -> String {
        get_str(&self.raw[..16])
    }

    /// Padding after the terminator follows the record's existing style (`0x00` factory,
    /// `0xFF` OEM CPS).
    pub fn set_name(&mut self, name: &str) {
        let pad = if self.raw[15] == 0xFF { 0xFF } else { 0x00 };
        put_str(&mut self.raw[..16], name, 16, pad);
    }

    /// 0x10-0x13, 8-digit BCD. Invalid BCD reads as 0.
    pub fn rx_freq(&self) -> Freq {
        get_freq(&self.raw[0x10..]).unwrap_or_default()
    }

    pub fn set_rx_freq(&mut self, f: Freq) {
        put_freq(&mut self.raw[0x10..], Some(f));
    }

    /// 0x14-0x17. `None` (`FF FF FF FF`) = no TX frequency.
    pub fn tx_freq(&self) -> Option<Freq> {
        get_freq(&self.raw[0x14..])
    }

    pub fn set_tx_freq(&mut self, f: Option<Freq>) {
        put_freq(&mut self.raw[0x14..], f);
    }

    /// 0x18 bits 7-4. qdmr reads only bits 5-4; the spec's 4-bit field is used here
    /// (values above 3 round-trip as `Other`).
    pub fn mode(&self) -> ChannelMode {
        ChannelMode::from_raw(self.bits(0x18, 4, 4))
    }

    pub fn set_mode(&mut self, m: ChannelMode) {
        self.put_bits(0x18, 4, 4, m.to_raw());
    }

    /// 0x18 bit 3: forbid TX ("RX only").
    pub fn rx_only(&self) -> bool {
        self.flag(0x18, 3)
    }

    pub fn set_rx_only(&mut self, on: bool) {
        self.set_flag(0x18, 3, on);
    }

    /// 0x18 bits 2-1.
    pub fn power(&self) -> Power {
        Power::from_raw(self.bits(0x18, 1, 2))
    }

    pub fn set_power(&mut self, p: Power) {
        self.put_bits(0x18, 1, 2, p.to_raw());
    }

    /// 0x18 bit 0.
    pub fn lone_worker(&self) -> bool {
        self.flag(0x18, 0)
    }

    pub fn set_lone_worker(&mut self, on: bool) {
        self.set_flag(0x18, 0, on);
    }

    /// 0x19 bit 7: 1 = 25 kHz. Factory analog channels store 1, digital 0.
    pub fn bandwidth(&self) -> Bandwidth {
        Bandwidth::from_raw(self.bits(0x19, 7, 1))
    }

    pub fn set_bandwidth(&mut self, b: Bandwidth) {
        self.put_bits(0x19, 7, 1, b.to_raw());
    }

    /// 0x19 bit 6: "Auto Scan".
    pub fn auto_scan(&self) -> bool {
        self.flag(0x19, 6)
    }

    pub fn set_auto_scan(&mut self, on: bool) {
        self.set_flag(0x19, 6, on);
    }

    /// 0x19 bits 5-2, stored 1-based (0 = none). ⚠️ Not attested on hardware (all factory
    /// channels store 0). qdmr reads bits 6-2 as one 5-bit index (31 lists); the spec's split
    /// is used because the CPS has a separate "Auto Scan" option that needs a home, which
    /// limits channels to scan lists 1-15.
    pub fn scan_list(&self) -> Option<u8> {
        self.bits(0x19, 2, 4).checked_sub(1)
    }

    pub fn set_scan_list(&mut self, idx: Option<u8>) {
        self.put_bits(0x19, 2, 4, idx.map_or(0, |i| i.min(14) + 1));
    }

    /// 0x1A bit 7 is "forbid talkaround"; this returns its inverse.
    pub fn talkaround(&self) -> bool {
        !self.flag(0x1A, 7)
    }

    pub fn set_talkaround(&mut self, on: bool) {
        self.set_flag(0x1A, 7, !on);
    }

    /// 0x1A bits 5-4 (digital). Position from qdmr; the spec calls bits 6-4 an unknown 0-3
    /// setting (OEM CPS writes 1), consistent with this. Option order from the CPS, inferred to
    /// equal the stored value.
    pub fn digital_tx_admit(&self) -> Option<DigitalTxAdmit> {
        self.digital()
            .then(|| DigitalTxAdmit::from_raw(self.bits(0x1A, 4, 2)))
    }

    pub fn set_digital_tx_admit(&mut self, a: DigitalTxAdmit) {
        self.put_bits(0x1A, 4, 2, a.to_raw());
    }

    /// 0x1A bits 5-4 (analog). Same bits as [`Self::digital_tx_admit`].
    pub fn analog_tx_admit(&self) -> Option<AnalogTxAdmit> {
        (!self.digital()).then(|| AnalogTxAdmit::from_raw(self.bits(0x1A, 4, 2)))
    }

    pub fn set_analog_tx_admit(&mut self, a: AnalogTxAdmit) {
        self.put_bits(0x1A, 4, 2, a.to_raw());
    }

    /// 0x1A bit 2: receive APRS (set on the factory "APRS Test" channel).
    pub fn aprs_rx(&self) -> bool {
        self.flag(0x1A, 2)
    }

    pub fn set_aprs_rx(&mut self, on: bool) {
        self.set_flag(0x1A, 2, on);
    }

    /// 0x1B bit 7: emergency indicator / notification.
    pub fn emergency_indicator(&self) -> bool {
        self.flag(0x1B, 7)
    }

    pub fn set_emergency_indicator(&mut self, on: bool) {
        self.set_flag(0x1B, 7, on);
    }

    /// 0x1B bit 6.
    pub fn emergency_ack(&self) -> bool {
        self.flag(0x1B, 6)
    }

    pub fn set_emergency_ack(&mut self, on: bool) {
        self.set_flag(0x1B, 6, on);
    }

    /// 0x1B bits 4-0, stored 1-based (0 = none); the factory "Digital Alarm" channel stores 1
    /// with 8 systems defined. qdmr reads only 3 bits, too narrow for 8 systems; the spec's
    /// 5-bit width is used. Bit 5 (set by the OEM CPS) is left alone.
    pub fn emergency_system(&self) -> Option<u8> {
        self.bits(0x1B, 0, 5).checked_sub(1)
    }

    pub fn set_emergency_system(&mut self, idx: Option<u8>) {
        self.put_bits(0x1B, 0, 5, idx.map_or(0, |i| i.min(30) + 1));
    }

    /// 0x1C bits 7-4.
    pub fn squelch(&self) -> u8 {
        self.bits(0x1C, 4, 4)
    }

    pub fn set_squelch(&mut self, level: u8) {
        self.put_bits(0x1C, 4, 4, level.min(15));
    }

    /// 0x1C bits 3-2 (spec). qdmr treats bit 2 alone as "DMR APRS on", which agrees for
    /// Off/Digital.
    pub fn aprs_report(&self) -> AprsReport {
        AprsReport::from_raw(self.bits(0x1C, 2, 2))
    }

    pub fn set_aprs_report(&mut self, r: AprsReport) {
        self.put_bits(0x1C, 2, 2, r.to_raw());
    }

    // ---- 0x1D, digital: private call ack / data ack / DCDM / slot / colour code ----
    //
    // Spec has bit 7 = encryption enable and bit 6 of 0x1F = private call confirm; qdmr swaps
    // them. The factory "Digital Encrypt" channel stores 0x1D = 0x01, 0x1E = 0x01, 0x1F = 0x41,
    // so encryption is 0x1F bit 6 with the key at 0x1E, as qdmr has it.

    /// 0x1D bit 7 (digital).
    pub fn private_call_ack(&self) -> Option<bool> {
        self.digital().then(|| self.flag(0x1D, 7))
    }

    pub fn set_private_call_ack(&mut self, on: bool) {
        self.set_flag(0x1D, 7, on);
    }

    /// 0x1D bit 6 (digital): data / short-data confirm.
    pub fn data_ack(&self) -> Option<bool> {
        self.digital().then(|| self.flag(0x1D, 6))
    }

    pub fn set_data_ack(&mut self, on: bool) {
        self.set_flag(0x1D, 6, on);
    }

    /// 0x1D bit 5 (digital): DCDM / TDMA direct mode.
    pub fn dcdm(&self) -> Option<bool> {
        self.digital().then(|| self.flag(0x1D, 5))
    }

    pub fn set_dcdm(&mut self, on: bool) {
        self.set_flag(0x1D, 5, on);
    }

    /// 0x1D bit 4 (digital).
    pub fn time_slot(&self) -> Option<TimeSlot> {
        self.digital()
            .then(|| TimeSlot::from_raw(self.bits(0x1D, 4, 1)))
    }

    pub fn set_time_slot(&mut self, ts: TimeSlot) {
        self.put_bits(0x1D, 4, 1, ts.to_raw());
    }

    /// 0x1D bits 3-0 (digital). Hardware-confirmed; byte 0x20 is not the colour code.
    pub fn color_code(&self) -> Option<u8> {
        self.digital().then(|| self.bits(0x1D, 0, 4))
    }

    pub fn set_color_code(&mut self, cc: u8) {
        self.put_bits(0x1D, 0, 4, cc.min(15));
    }

    // ---- 0x1D, analog: scramble / compander ----
    //
    // Bits 3-0 on analog channels are unknown (factory analog channels store 1 there). Bit 7 is
    // VOX per the spec, but VOX is modelled at 0x25 bit 4 (qdmr; mode-independent). The spec's
    // "talkback" at bit 4 is not exposed: the CPS has no channel Talkback option (only a
    // scan-list one).

    /// 0x1D bit 6 (analog).
    pub fn scramble(&self) -> Option<bool> {
        (!self.digital()).then(|| self.flag(0x1D, 6))
    }

    pub fn set_scramble(&mut self, on: bool) {
        self.set_flag(0x1D, 6, on);
    }

    /// 0x1D bit 5 (analog). Duplicated at 0x25 bit 5; the setter writes both copies.
    pub fn compander(&self) -> Option<bool> {
        (!self.digital()).then(|| self.flag(0x1D, 5))
    }

    pub fn set_compander(&mut self, on: bool) {
        self.set_flag(0x1D, 5, on);
        self.set_flag(0x25, 5, on);
    }

    /// 0x1E (digital): encryption key, stored 1-based (0 = none). From qdmr and the factory
    /// "Digital Encrypt" channel (0x1E = 1); the spec calls the byte unknown and puts the key
    /// at 0x2A (all zero in the fixture), which is left alone.
    pub fn encryption_key(&self) -> Option<Option<u8>> {
        self.digital().then(|| self.raw[0x1E].checked_sub(1))
    }

    pub fn set_encryption_key(&mut self, idx: Option<u8>) {
        self.raw[0x1E] = idx.map_or(0, |i| i.saturating_add(1));
    }

    // ---- 0x1F: bit 7 unknown (preserved) ----

    /// 0x1F bit 6 (digital): encryption enable.
    pub fn encryption(&self) -> Option<bool> {
        self.digital().then(|| self.flag(0x1F, 6))
    }

    pub fn set_encryption(&mut self, on: bool) {
        self.set_flag(0x1F, 6, on);
    }

    /// 0x1F bits 5-0 (digital): RX group list, stored 1-based (0 = none).
    pub fn rx_group_list(&self) -> Option<Option<u8>> {
        self.digital().then(|| self.bits(0x1F, 0, 6).checked_sub(1))
    }

    pub fn set_rx_group_list(&mut self, idx: Option<u8>) {
        self.put_bits(0x1F, 0, 6, idx.map_or(0, |i| i.min(62) + 1));
    }

    /// 0x1F bit 6 (analog): show PTT ID. Duplicated at 0x26 bit 7; the setter writes both.
    pub fn ptt_id_display(&self) -> Option<bool> {
        (!self.digital()).then(|| self.flag(0x1F, 6))
    }

    pub fn set_ptt_id_display(&mut self, on: bool) {
        self.set_flag(0x1F, 6, on);
        self.set_flag(0x26, 7, on);
    }

    /// 0x1F bits 5-0 (analog): PTT ID number, raw 0-63. ⚠️ Derived (spec); base unknown. The
    /// CPS analog page also has "RX/TX Signaling System" selectors, which may be what this holds.
    pub fn ptt_id_number(&self) -> Option<u8> {
        (!self.digital()).then(|| self.bits(0x1F, 0, 6))
    }

    pub fn set_ptt_id_number(&mut self, v: u8) {
        self.put_bits(0x1F, 0, 6, v);
    }

    /// 0x20 bits 3-0: DMR APRS report channel, 0-based (qdmr only; the spec calls the byte
    /// unknown, zero in all captures).
    pub fn aprs_channel(&self) -> u8 {
        self.bits(0x20, 0, 4)
    }

    pub fn set_aprs_channel(&mut self, idx: u8) {
        self.put_bits(0x20, 0, 4, idx);
    }

    /// 0x21-0x22: RX CTCSS/DCS ("decode").
    pub fn rx_tone(&self) -> Tone {
        get_tone(&self.raw[0x21..])
    }

    pub fn set_rx_tone(&mut self, t: Tone) {
        put_tone(&mut self.raw[0x21..], t);
    }

    /// 0x23-0x24: TX CTCSS/DCS ("encode").
    pub fn tx_tone(&self) -> Tone {
        get_tone(&self.raw[0x23..])
    }

    pub fn set_tx_tone(&mut self, t: Tone) {
        put_tone(&mut self.raw[0x23..], t);
    }

    /// 0x25 bit 4 (qdmr). The spec also links the CPS "VOX" control to 0x1D bit 7 on analog
    /// channels; that bit is not touched.
    pub fn vox(&self) -> bool {
        self.flag(0x25, 4)
    }

    pub fn set_vox(&mut self, on: bool) {
        self.set_flag(0x25, 4, on);
    }

    /// 0x26 bits 6-4. The factory "5 Tone Call" channel stores 1 (optional signalling).
    pub fn rx_squelch_mode(&self) -> RxSquelchMode {
        RxSquelchMode::from_raw(self.bits(0x26, 4, 3))
    }

    pub fn set_rx_squelch_mode(&mut self, m: RxSquelchMode) {
        self.put_bits(0x26, 4, 3, m.to_raw());
    }

    /// 0x26 bits 3-1. Established from the factory channels "DTMF Call" (1), "5 Tone Call" (3)
    /// and "BDC1200" (4); the spec puts signalling at 0x27 bits 3-0, which is zero on all of
    /// them.
    pub fn signaling(&self) -> Signaling {
        Signaling::from_raw(self.bits(0x26, 1, 3))
    }

    pub fn set_signaling(&mut self, s: Signaling) {
        self.put_bits(0x26, 1, 3, s.to_raw());
    }

    /// 0x27 bits 7-4. ⚠️ Derived (spec only).
    pub fn step(&self) -> Step {
        Step::from_raw(self.bits(0x27, 4, 4))
    }

    pub fn set_step(&mut self, s: Step) {
        self.put_bits(0x27, 4, 4, s.to_raw());
    }

    /// 0x29 bits 3-2. ⚠️ Uncertain: qdmr has a single "PTT ID enable" bit 2, the spec a "PTT ID
    /// type" in bits 7-4 (which the OEM CPS fills with 15). The factory "BDC1200" channel
    /// stores 2 in bits 3-2 and 0 in bits 7-4, so bits 3-2 are taken as the PTT ID mode.
    pub fn ptt_id(&self) -> PttId {
        PttId::from_raw(self.bits(0x29, 2, 2))
    }

    pub fn set_ptt_id(&mut self, p: PttId) {
        self.put_bits(0x29, 2, 2, p.to_raw());
    }

    /// 0x2B: the radio's own DMR ID (block `0x67`), stored 1-based; 0 (factory) and `0xFF`
    /// mean "default ID" and give `None`. The base follows the OEM write capture (113 of 128
    /// channels store 1 with a single Radio ID defined); qdmr and the spec's reference
    /// implementation read it 0-based. `set_radio_id(None)` writes 0.
    pub fn radio_id(&self) -> Option<u8> {
        match self.raw[0x2B] {
            0 | 0xFF => None,
            v => Some(v - 1),
        }
    }

    pub fn set_radio_id(&mut self, idx: Option<u8>) {
        self.raw[0x2B] = idx.map_or(0, |i| i.min(0xFD) + 1);
    }
}

/// A channel's TX contact (2 bytes in block `0x42`/`0x43`).
///
/// `byte0` bits 7-4 = index bits 11-8, bits 3-1 unknown (preserved), bit 0 "digital" flag;
/// `byte1` = index bits 7-0. The index is 1-based into the talk-group list (block `0x44`), with
/// 0 = none; `0xFFF` (the OEM CPS's `FF FF` fill) is also read as none. The factory image
/// stores contact 1 on some analog channels (with the digital flag clear), so the entry is kept
/// as-is whatever the mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TxContact {
    raw: [u8; 2],
}

raw_record!(TxContact, 2, 0xFF);

impl TxContact {
    /// 0-based talk-group index.
    pub fn index(&self) -> Option<u16> {
        match ((self.raw[0] as u16 >> 4) << 8) | self.raw[1] as u16 {
            0 | 0xFFF => None,
            v => Some(v - 1),
        }
    }

    pub fn set_index(&mut self, idx: Option<u16>) {
        let v = idx.map_or(0, |i| i.min(0xFFD) + 1);
        set_bits(&mut self.raw[0], 4, 4, (v >> 8) as u8);
        self.raw[1] = v as u8;
    }

    pub fn digital(&self) -> bool {
        self.raw[0] & 1 != 0
    }

    pub fn set_digital(&mut self, on: bool) {
        set_bits(&mut self.raw[0], 0, 1, on as u8);
    }
}

/// A channel together with its TX contact; derefs to the [`Channel`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChannelEntry {
    pub channel: Channel,
    pub tx_contact: TxContact,
}

impl ChannelEntry {
    /// A new channel (see [`Channel::new`]) with no TX contact.
    pub fn new(name: &str, rx: Freq, mode: ChannelMode) -> ChannelEntry {
        let mut tx_contact = TxContact { raw: [0; 2] };
        tx_contact.set_digital(mode.is_digital());
        ChannelEntry {
            channel: Channel::new(name, rx, mode),
            tx_contact,
        }
    }
}

impl Deref for ChannelEntry {
    type Target = Channel;
    fn deref(&self) -> &Channel {
        &self.channel
    }
}

impl DerefMut for ChannelEntry {
    fn deref_mut(&mut self) -> &mut Channel {
        &mut self.channel
    }
}

/// The channel list and the two VFOs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Channels {
    /// Channel `n` (1-based, as zones and scan lists refer to it) is `list[n - 1]`. Blank
    /// entries are holes that keep later numbers stable. At most [`MAX_CHANNELS`] are encoded.
    pub list: Vec<ChannelEntry>,
    pub vfo_a: ChannelEntry,
    pub vfo_b: ChannelEntry,
}

/// Block and offset of channel record `index` (0-based).
pub fn record_location(index: usize) -> (u8, usize) {
    let (slot, pos, base) = if index < FIRST_SLOT_RECORDS {
        (0, index, HEADER)
    } else {
        let i = index - FIRST_SLOT_RECORDS;
        (1 + i / SLOT_RECORDS, i % SLOT_RECORDS, 0)
    };
    (
        block::CHANNELS.start() + slot as u8,
        base + pos * Channel::SIZE,
    )
}

/// Block and offset of the TX contact of channel `index` (0-based).
pub fn tx_contact_location(index: usize) -> (u8, usize) {
    if index < TX_CONTACTS_PER_BLOCK {
        (block::TX_CONTACT_LOW, index * 2)
    } else {
        (block::TX_CONTACT_HIGH, (index - TX_CONTACTS_PER_BLOCK) * 2)
    }
}

/// Channel count from the bank header. `FF FF` (uninitialised) reads as 0.
fn stored_count(img: &Image) -> usize {
    match img.block(*block::CHANNELS.start()) {
        Some(b) => match get_u16(&b[..2]) {
            0xFFFF => 0,
            n => (n as usize).min(MAX_CHANNELS),
        },
        None => 0,
    }
}

fn read<const N: usize>(img: &Image, id: u8, off: usize) -> Option<&[u8]> {
    img.block(id).map(|b| &b[off..off + N])
}

fn decode_entry(img: &Image, rec: (u8, usize), tx: (u8, usize)) -> ChannelEntry {
    ChannelEntry {
        channel: read::<48>(img, rec.0, rec.1)
            .map(Channel::from_bytes)
            .unwrap_or_default(),
        tx_contact: read::<2>(img, tx.0, tx.1)
            .map(TxContact::from_bytes)
            .unwrap_or_default(),
    }
}

pub fn decode(img: &Image) -> Channels {
    let vfo = |i: usize| {
        decode_entry(
            img,
            (*block::CHANNELS.end(), VFO_OFFSETS[i]),
            (block::TX_CONTACT_HIGH, VFO_TX_CONTACT_OFFSETS[i]),
        )
    };
    Channels {
        list: (0..stored_count(img))
            .map(|i| decode_entry(img, record_location(i), tx_contact_location(i)))
            .collect(),
        vfo_a: vfo(0),
        vfo_b: vfo(1),
    }
}

/// Write `bytes` at `off` of block `id`. An absent block is created only if `bytes` is not
/// all-`0xFF` (what a fresh block holds anyway).
fn write_lazy(img: &mut Image, id: u8, off: usize, bytes: &[u8]) {
    if img.block(id).is_none() && bytes.iter().all(|&b| b == 0xFF) {
        return;
    }
    let b = img.block_mut(id);
    b[off..off + bytes.len()].copy_from_slice(bytes);
    b[PAGE - 1] = id;
}

/// Write the channels back.
///
/// Writes the count and every record up to it, creating only the slot blocks needed. Records
/// that held channels before (below the old count) but no longer do are filled with `0xFF`;
/// stale bytes past both counts, and slot blocks past the last needed one, are left as they
/// are (the count governs). Header bytes 2..16 are preserved. TX contacts are written for the
/// channels in the list only; entries of removed channels are left alone. VFOs are written if
/// block `0x41` exists or they are not blank.
pub fn encode(ch: &Channels, img: &mut Image) {
    let n = ch.list.len().min(MAX_CHANNELS);
    let old = stored_count(img);
    let first = *block::CHANNELS.start();
    put_u16(&mut img.block_mut(first)[..2], n as u16);
    img.block_mut(first)[PAGE - 1] = first;

    let last_slot = record_location(n.max(1) - 1).0;
    for i in 0..n.max(old) {
        let (id, off) = record_location(i);
        if id > last_slot {
            break;
        }
        // Record 83 of block 0x41 overlaps VFO A; never reached under MAX_CHANNELS.
        let end = (off + Channel::SIZE).min(if id == *block::CHANNELS.end() {
            VFO_OFFSETS[0]
        } else {
            PAGE - 1
        });
        let b = img.block_mut(id);
        b[PAGE - 1] = id;
        if i < n {
            b[off..end].copy_from_slice(&ch.list[i].channel.raw[..end - off]);
        } else {
            b[off..end].fill(0xFF);
        }
    }

    for (i, e) in ch.list.iter().take(n).enumerate() {
        let (id, off) = tx_contact_location(i);
        write_lazy(img, id, off, &e.tx_contact.raw);
    }

    for (i, v) in [&ch.vfo_a, &ch.vfo_b].into_iter().enumerate() {
        write_lazy(img, *block::CHANNELS.end(), VFO_OFFSETS[i], &v.channel.raw);
        write_lazy(
            img,
            block::TX_CONTACT_HIGH,
            VFO_TX_CONTACT_OFFSETS[i],
            &v.tx_contact.raw,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::data_file;

    fn fixture() -> Image {
        let bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/factory-v1.60.data"
        ))
        .unwrap();
        data_file::import(&bytes).unwrap()
    }

    fn mhz(s: &str) -> Freq {
        s.parse().unwrap()
    }

    #[test]
    fn layout() {
        assert_eq!(record_location(0), (0x12, 0x10));
        assert_eq!(record_location(83), (0x12, 0xFA0));
        assert_eq!(record_location(84), (0x13, 0));
        assert_eq!(record_location(169), (0x14, 0));
        let (id, off) = record_location(MAX_CHANNELS - 1);
        assert_eq!(id, 0x41);
        assert!(off + 48 <= VFO_OFFSETS[0]);
        assert_eq!(VFO_OFFSETS[1] + 48, PAGE - 1);
        assert_eq!(tx_contact_location(2046), (0x42, 0xFFC));
        assert_eq!(tx_contact_location(2047), (0x43, 0));
        assert!(tx_contact_location(MAX_CHANNELS - 1).1 < VFO_TX_CONTACT_OFFSETS[0]);
    }

    #[test]
    fn decode_factory() {
        let c = decode(&fixture());
        assert_eq!(c.list.len(), 25);
        let names: Vec<String> = c.list.iter().map(|e| e.name()).collect();
        assert_eq!(names[0], "Channel 1");
        assert_eq!(names[2], "Channel  3");
        assert_eq!(names[16], "DMR Repeater");
        assert_eq!(names[21], "Digital Encrypt");
        assert_eq!(names[24], "TDMA Direct Mode");

        let ch1 = &c.list[0];
        assert_eq!(ch1.rx_freq(), mhz("430.0125"));
        assert_eq!(ch1.tx_freq(), Some(mhz("430.0125")));
        assert_eq!(ch1.mode(), ChannelMode::Digital);
        assert_eq!(ch1.power(), Power::High);
        assert_eq!(ch1.color_code(), Some(1));
        assert_eq!(ch1.time_slot(), Some(TimeSlot::Ts1));
        assert_eq!(ch1.rx_group_list(), Some(Some(0)));
        assert_eq!(ch1.scramble(), None);
        assert_eq!(ch1.squelch(), 3);
        assert_eq!(ch1.aprs_report(), AprsReport::Digital);
        assert_eq!(ch1.radio_id(), None);
        assert_eq!(ch1.tx_contact.index(), Some(0));
        assert!(ch1.tx_contact.digital());

        assert_eq!(c.list[5].rx_freq(), mhz("144.0225"));
        assert_eq!(c.list[8].rx_freq(), mhz("145.975"));
        assert_eq!(c.list[16].rx_freq(), mhz("439.11"));
        assert_eq!(c.list[16].color_code(), Some(0));

        let ch10 = &c.list[9];
        assert_eq!(ch10.mode(), ChannelMode::Analog);
        assert_eq!(ch10.bandwidth(), Bandwidth::Wide);
        assert_eq!(ch10.color_code(), None);
        assert_eq!(ch10.scramble(), Some(false));
        assert_eq!(ch10.rx_tone(), Tone::Off);
        assert_eq!(ch10.tx_contact.index(), None);
        assert_eq!(c.list[15].power(), Power::Low);

        assert_eq!(c.list[17].signaling(), Signaling::Dtmf);
        assert_eq!(c.list[17].tx_contact.index(), Some(0));
        assert!(!c.list[17].tx_contact.digital());
        assert_eq!(c.list[19].signaling(), Signaling::FiveTone);
        assert_eq!(c.list[19].rx_squelch_mode(), RxSquelchMode::Optional);
        assert_eq!(c.list[20].signaling(), Signaling::Mdc1200);
        assert_eq!(c.list[20].ptt_id(), PttId::Eot);

        let enc = &c.list[21];
        assert_eq!(enc.encryption(), Some(true));
        assert_eq!(enc.encryption_key(), Some(Some(0)));
        assert_eq!(enc.private_call_ack(), Some(false));

        let alarm = &c.list[22];
        assert!(alarm.emergency_indicator() && alarm.emergency_ack());
        assert_eq!(alarm.emergency_system(), Some(0));
        assert!(c.list[23].aprs_rx());

        assert_eq!(c.vfo_a.name(), "");
        assert_eq!(c.vfo_a.mode(), ChannelMode::Analog);
        assert_eq!(c.vfo_a.rx_freq(), mhz("434.33833"));
        assert_eq!(c.vfo_b.mode(), ChannelMode::Digital);
        assert_eq!(c.vfo_b.rx_freq(), mhz("432.025"));
        assert_eq!(c.vfo_a.tx_contact.index(), Some(0));
        assert!(c.vfo_b.tx_contact.digital());
    }

    #[test]
    fn round_trip_identical() {
        let img = fixture();
        let mut out = img.clone();
        encode(&decode(&img), &mut out);
        assert_eq!(
            out.blocks.keys().collect::<Vec<_>>(),
            img.blocks.keys().collect::<Vec<_>>()
        );
        for (id, b) in &img.blocks {
            assert!(out.blocks[id][..] == b[..], "block {id:#04x} changed");
        }
    }

    fn diff(a: &Image, b: &Image) -> Vec<(u8, usize)> {
        let mut d = Vec::new();
        for (id, pa) in &a.blocks {
            let pb = &b.blocks[id];
            d.extend((0..PAGE).filter(|&i| pa[i] != pb[i]).map(|i| (*id, i)));
        }
        d
    }

    #[test]
    fn edit_touches_only_its_bits() {
        let img = fixture();
        let mut c = decode(&img);
        c.list[0].set_time_slot(TimeSlot::Ts2);
        c.list[1].set_name("Renamed");
        c.list[2].tx_contact.set_index(Some(299));
        let mut out = img.clone();
        encode(&c, &mut out);
        let d = diff(&img, &out);
        let mut want = vec![(0x12, 0x10 + 0x1D)];
        let (a, b) = (
            &img.blocks[&0x12][0x40..0x50],
            &out.blocks[&0x12][0x40..0x50],
        );
        want.extend((0..16).filter(|&i| a[i] != b[i]).map(|i| (0x12, 0x40 + i)));
        want.extend([(0x42, 4), (0x42, 5)]);
        assert_eq!(d, want);
        assert!(d.len() > 5);
        assert_eq!(out.blocks[&0x12][0x10 + 0x1D], 0x11);
        assert_eq!(&out.blocks[&0x42][4..6], &[0x11, 0x2C]);
        let back = decode(&out);
        assert_eq!(back, c);
        assert_eq!(back.list[1].name(), "Renamed");
    }

    #[test]
    fn grow_and_shrink() {
        let img = fixture();
        let mut c = decode(&img);
        for i in c.list.len()..200 {
            let mut e = ChannelEntry::new(
                &format!("CH{}", i + 1),
                mhz("446.00625"),
                ChannelMode::Digital,
            );
            e.tx_contact.set_index(Some(i as u16 % 8));
            c.list.push(e);
        }
        let mut out = img.clone();
        encode(&c, &mut out);
        let blocks: Vec<u8> = out.blocks.keys().copied().collect();
        let orig: Vec<u8> = img.blocks.keys().copied().collect();
        assert_eq!(blocks, orig, "no new blocks: 0x13 and 0x14 already exist");
        assert_eq!(get_u16(&out.blocks[&0x12][..2]), 200);
        assert_eq!(out.blocks[&0x12][2..16], img.blocks[&0x12][2..16]);
        assert_eq!(get_str(&out.blocks[&0x12][0xFA0..0xFB0]), "CH84");
        assert_eq!(get_str(&out.blocks[&0x13][0..16]), "CH85");
        assert_eq!(get_str(&out.blocks[&0x13][84 * 48..84 * 48 + 16]), "CH169");
        assert_eq!(get_str(&out.blocks[&0x14][0..16]), "CH170");
        assert_eq!(get_str(&out.blocks[&0x14][30 * 48..30 * 48 + 16]), "CH200");
        assert!(
            out.blocks[&0x14][31 * 48..PAGE - 1]
                .iter()
                .all(|&b| b == 0xFF)
        );
        assert_eq!(decode(&out), c);

        // On an empty image only the needed blocks appear.
        let mut fresh = Image::default();
        let blank_vfos = Channels {
            list: c.list.clone(),
            ..Default::default()
        };
        encode(&blank_vfos, &mut fresh);
        assert_eq!(
            fresh.blocks.keys().copied().collect::<Vec<_>>(),
            vec![0x12, 0x13, 0x14, 0x42]
        );
        encode(&c, &mut fresh);
        assert_eq!(
            fresh.blocks.keys().copied().collect::<Vec<_>>(),
            vec![0x12, 0x13, 0x14, 0x41, 0x42, 0x43]
        );
        assert!(fresh.blocks.values().all(|b| b[PAGE - 1] != 0xFF));
        assert_eq!(decode(&fresh).list, c.list);

        // Shrink: removed records become 0xFF, the stale tail past the old count stays.
        let mut small = c.clone();
        small.list.truncate(20);
        let mut out2 = out.clone();
        encode(&small, &mut out2);
        assert_eq!(get_u16(&out2.blocks[&0x12][..2]), 20);
        let bad: Vec<usize> = (0x10 + 20 * 48..0xFD0)
            .filter(|&o| out2.blocks[&0x12][o] != 0xFF)
            .collect();
        assert!(bad.is_empty(), "{bad:x?}");
        assert_eq!(
            out2.blocks[&0x13], out.blocks[&0x13],
            "unneeded slot left alone"
        );
        assert_eq!(decode(&out2), small);

        // Shrinking the factory image only clears records 21..=25.
        let mut f = decode(&img);
        f.list.truncate(20);
        let mut out3 = img.clone();
        encode(&f, &mut out3);
        let d = diff(&img, &out3);
        assert!(
            d.iter()
                .all(|&(id, o)| id == 0x12
                    && (o < 2 || (0x10 + 20 * 48..0x10 + 25 * 48).contains(&o)))
        );
    }

    #[test]
    fn holes_keep_numbers() {
        let mut c = decode(&fixture());
        c.list[4] = ChannelEntry::default();
        let mut img = fixture();
        encode(&c, &mut img);
        let back = decode(&img);
        assert_eq!(back.list.len(), 25);
        assert!(back.list[4].is_empty());
        assert_eq!(back.list[5].name(), "Channel 6");
    }

    #[test]
    fn dual_purpose_bits() {
        let mut ch = Channel::new("A", mhz("145.5"), ChannelMode::Analog);
        ch.set_compander(true);
        ch.set_ptt_id_display(true);
        assert_eq!(ch.raw[0x1D], 0x20);
        assert_eq!(ch.raw[0x25], 0x20);
        assert_eq!(ch.raw[0x1F], 0x40);
        assert_eq!(ch.raw[0x26], 0x80);
        ch.set_mode(ChannelMode::Digital);
        assert_eq!(ch.compander(), None);
        assert_eq!(ch.dcdm(), Some(true));
        assert_eq!(ch.encryption(), Some(true));
        assert_eq!(ch.tx_freq(), Some(mhz("145.5")));
        ch.set_tx_freq(None);
        assert_eq!(&ch.raw[0x14..0x18], &[0xFF; 4]);
        let mut t = TxContact::from_bytes(&[0x0E, 0x01]);
        assert_eq!(t.index(), Some(0));
        t.set_index(None);
        assert_eq!(t.raw, [0x0E, 0x00]);
        assert_eq!(TxContact::default().index(), None);
    }
}
