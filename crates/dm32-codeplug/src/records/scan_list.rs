//! Scan lists (block `0x11`).
//!
//! Block layout: list count at `0x000`, 57-byte records from `0x001`, scan mode and VHF/UHF
//! scan ranges at `0xE00` (qdmr; the factory image holds `00 44 01 48 01 30 04 40 04` = time
//! mode, 144–148 / 430–440 MHz).
//!
//! # Channel list position (spec vs qdmr)
//!
//! The spec recommends reading 15 slots at `+0x1A` and treating `+0x18` as an opaque word, and
//! warns that `+0x0B` then disagrees with the number of channels. qdmr reads the list at `+0x18`
//! with `0` meaning "the selected channel". Under qdmr's reading the count byte matches the
//! populated slots in every captured list (factory: 16 and 9; OEM write: `00 00 0E 00 0F 00` with
//! count 3 = selected + 14 + 15), so that is what is implemented here.

use crate::encoding::{get_str, get_u16, put_str, put_u16, set_bits};
use crate::image::{Image, block};
use crate::{choice, raw_record};

pub const MAX_SCAN_LISTS: usize = 32;
/// Members written per list (hardware-verified limit). The member area `0x18..0x38` physically
/// has 16 slots and the factory list 1 uses all 16, so decoding accepts up to 16.
pub const MAX_CHANNELS: usize = 15;
const SLOTS: usize = 16;
/// Longest name written. The radio itself stores 11 characters unterminated (`"Scan List 1"`);
/// that is accepted on read.
pub const NAME_MAX: usize = 10;
const MEMBERS: usize = 0x18;
const TAIL: usize = 0xE00;
const TAIL_LEN: usize = 9;

choice! {
    /// CTCSS/DCS detection while scanning (byte `0x0C` bits 0-1). Labels and order from the CPS
    /// (`[CtcDcsScanMode]`); value = position is inferred (factory lists store 3).
    pub enum CtcScanMode {
        None = 0 => "Not Detection CTC",
        NonPriority = 1 => "Detection CTC Non Priority",
        Priority = 2 => "Detection CTC Priority",
        All = 3 => "Detection CTC",
    }
}

choice! {
    /// Channel to transmit on while scanning (byte `0x0C` bits 4-7 per qdmr; the spec puts it at
    /// bits 2-3. Both captures store 0, so neither is confirmed). CPS labels/order (`[ScanTxMode]`).
    pub enum ScanTxMode {
        Current = 0 => "Current Channel",
        LastActive = 1 => "Last Actived Channel",
        Designated = 2 => "Designed Channel",
    }
}

choice! {
    /// Priority channel selection (byte `0x0E`: priority 1 in bits 0-3, priority 2 in bits 4-7;
    /// spec reading, unverified. qdmr reads these nibbles as channel indexes). CPS labels/order
    /// (`[ScanPriorityType]`); value = position is inferred.
    pub enum PriorityMode {
        None = 0 => "None",
        Current = 1 => "Current Channel",
        Channel = 2 => "Other",
    }
}

choice! {
    /// Block-wide (VFO) scan mode (block offset `0xE00`, qdmr; CPS `[VfoScanMode]` order agrees).
    pub enum ScanMode {
        Time = 0 => "Time",
        Carrier = 1 => "Carrier",
        Search = 2 => "Search",
    }
}

/// One member of a scan list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScanMember {
    /// The channel selected when scanning starts (stored as `0`).
    Selected,
    /// An absolute 1-based channel number.
    Channel(u16),
}

/// One 57-byte scan list record.
///
/// | Offset | Field |
/// |---|---|
/// | `0x00` | name, 11 bytes |
/// | `0x0B` | member count |
/// | `0x0C` | CTC scan mode (bits 0-1), TX mode (bits 4-7) |
/// | `0x0D` | hang time |
/// | `0x0E` | priority 1 / 2 modes (nibbles) |
/// | `0x0F`, `0x11`, `0x13` | priority 1 / designated TX / priority 2 channel (u16 LE) |
/// | `0x15..0x18` | ❓ unknown, preserved (qdmr: priority sweep time in `0x15` bits 2-7) |
/// | `0x18..0x38` | 16 × u16 LE members, `0` = selected channel |
/// | `0x38` | ❓ unknown, preserved |
///
/// The CPS export also lists "Priority Sweep Time" and "Talkback"; where they live is unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanList {
    raw: [u8; 57],
}

raw_record!(ScanList, 57, 0x00);

impl ScanList {
    /// A new list with the factory defaults (CTC detection on, hang time 6).
    pub fn new(name: &str) -> ScanList {
        let mut s = ScanList::default();
        s.set_name(name);
        s.set_ctc_mode(CtcScanMode::All);
        s.set_hang_time(6);
        s.set_members(&[]);
        s
    }

    /// The pattern the factory image holds in unused record slots.
    pub fn unused() -> ScanList {
        let mut s = ScanList::default();
        s.raw[..11].fill(0xFF);
        s.raw[0x0D] = 1;
        s
    }

    pub fn name(&self) -> String {
        get_str(&self.raw[..11])
    }

    pub fn set_name(&mut self, s: &str) {
        put_str(&mut self.raw[..11], s, NAME_MAX, 0x00);
    }

    /// The stored member count byte.
    pub fn channel_count(&self) -> u8 {
        self.raw[0x0B]
    }

    pub fn members(&self) -> Vec<ScanMember> {
        let n = (self.channel_count() as usize).min(SLOTS);
        (0..n)
            .map(|i| match get_u16(&self.raw[MEMBERS + 2 * i..]) {
                0 => ScanMember::Selected,
                c => ScanMember::Channel(c),
            })
            .collect()
    }

    /// Replace the members (truncated to 15). Unused slots are filled with `0xFFFF`, as the
    /// OEM CPS does (`0x0000` would read as "selected channel").
    pub fn set_members(&mut self, m: &[ScanMember]) {
        let n = m.len().min(MAX_CHANNELS);
        self.raw[0x0B] = n as u8;
        for (i, &mem) in m.iter().take(n).enumerate() {
            let v = match mem {
                ScanMember::Selected => 0,
                ScanMember::Channel(c) => c,
            };
            put_u16(&mut self.raw[MEMBERS + 2 * i..], v);
        }
        self.raw[MEMBERS + 2 * n..MEMBERS + 2 * SLOTS].fill(0xFF);
    }

    pub fn ctc_mode(&self) -> CtcScanMode {
        CtcScanMode::from_raw(self.raw[0x0C] & 0x03)
    }

    pub fn set_ctc_mode(&mut self, v: CtcScanMode) {
        set_bits(&mut self.raw[0x0C], 0, 2, v.to_raw());
    }

    pub fn tx_mode(&self) -> ScanTxMode {
        ScanTxMode::from_raw(self.raw[0x0C] >> 4)
    }

    pub fn set_tx_mode(&mut self, v: ScanTxMode) {
        set_bits(&mut self.raw[0x0C], 4, 4, v.to_raw());
    }

    /// Hang time — CPS "Hang Time" / "Stay Time[s]" — in 0.5 s steps per qdmr (factory value 6 = 3 s). The spec guesses 0.1 s steps
    /// but itself calls the resulting 0.6 s factory default implausible.
    pub fn hang_time(&self) -> u8 {
        self.raw[0x0D]
    }

    pub fn set_hang_time(&mut self, v: u8) {
        self.raw[0x0D] = v;
    }

    pub fn priority1_mode(&self) -> PriorityMode {
        PriorityMode::from_raw(self.raw[0x0E] & 0x0F)
    }

    pub fn set_priority1_mode(&mut self, v: PriorityMode) {
        set_bits(&mut self.raw[0x0E], 0, 4, v.to_raw());
    }

    pub fn priority2_mode(&self) -> PriorityMode {
        PriorityMode::from_raw(self.raw[0x0E] >> 4)
    }

    pub fn set_priority2_mode(&mut self, v: PriorityMode) {
        set_bits(&mut self.raw[0x0E], 4, 4, v.to_raw());
    }

    /// Priority channel 1 as stored (`0x0F`). Only meaningful when [`Self::priority1_mode`] is
    /// `Channel` (the factory image stores 1 with mode None). qdmr calls this field the
    /// revert channel (channel index + 1); the spec calls it priority channel 1 (channel number).
    pub fn priority1_channel(&self) -> u16 {
        get_u16(&self.raw[0x0F..])
    }

    pub fn set_priority1_channel(&mut self, v: u16) {
        put_u16(&mut self.raw[0x0F..], v);
    }

    /// Designated TX channel as stored (`0x11`). The spec claims a `channel − 2` encoding
    /// (unverified; all captures store 0).
    pub fn designated_tx_channel(&self) -> u16 {
        get_u16(&self.raw[0x11..])
    }

    pub fn set_designated_tx_channel(&mut self, v: u16) {
        put_u16(&mut self.raw[0x11..], v);
    }

    /// Priority channel 2 as stored (`0x13`); same caveats as the designated TX channel.
    pub fn priority2_channel(&self) -> u16 {
        get_u16(&self.raw[0x13..])
    }

    pub fn set_priority2_channel(&mut self, v: u16) {
        put_u16(&mut self.raw[0x13..], v);
    }
}

/// All scan lists plus the block-wide scan settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanLists {
    pub lists: Vec<ScanList>,
    tail: [u8; TAIL_LEN],
}

impl Default for ScanLists {
    fn default() -> Self {
        let mut s = ScanLists {
            lists: Vec::new(),
            tail: [0; TAIL_LEN],
        };
        s.set_vhf_range((144, 148));
        s.set_uhf_range((430, 440));
        s
    }
}

fn get_bcd4(b: &[u8]) -> u16 {
    let w = get_u16(b);
    (0..4)
        .rev()
        .fold(0, |v, i| v * 10 + ((w >> (4 * i)) & 0xF).min(9))
}

fn put_bcd4(b: &mut [u8], v: u16) {
    let v = v.min(9999);
    let w = (0..4).fold(0u16, |w, i| w | ((v / 10u16.pow(i)) % 10) << (4 * i));
    put_u16(b, w);
}

impl ScanLists {
    pub fn scan_mode(&self) -> ScanMode {
        ScanMode::from_raw(self.tail[0])
    }

    pub fn set_scan_mode(&mut self, v: ScanMode) {
        self.tail[0] = v.to_raw();
    }

    /// VHF scan range `(lower, upper)` in whole MHz.
    pub fn vhf_range(&self) -> (u16, u16) {
        (get_bcd4(&self.tail[1..]), get_bcd4(&self.tail[3..]))
    }

    pub fn set_vhf_range(&mut self, r: (u16, u16)) {
        put_bcd4(&mut self.tail[1..], r.0);
        put_bcd4(&mut self.tail[3..], r.1);
    }

    /// UHF scan range `(lower, upper)` in whole MHz.
    pub fn uhf_range(&self) -> (u16, u16) {
        (get_bcd4(&self.tail[5..]), get_bcd4(&self.tail[7..]))
    }

    pub fn set_uhf_range(&mut self, r: (u16, u16)) {
        put_bcd4(&mut self.tail[5..], r.0);
        put_bcd4(&mut self.tail[7..], r.1);
    }
}

fn offset(i: usize) -> usize {
    1 + i * ScanList::SIZE
}

pub fn decode(img: &Image) -> ScanLists {
    let Some(p) = img.block(block::SCAN_LISTS) else {
        return ScanLists::default();
    };
    let n = (p[0] as usize).min(MAX_SCAN_LISTS);
    let mut tail = [0; TAIL_LEN];
    tail.copy_from_slice(&p[TAIL..TAIL + TAIL_LEN]);
    ScanLists {
        lists: (0..n)
            .map(|i| ScanList::from_bytes(&p[offset(i)..]))
            .collect(),
        tail,
    }
}

/// Write scan lists back (at most 32). Record slots vacated by shrinking the list get the
/// factory "unused" pattern ([`ScanList::unused`]).
pub fn encode(s: &ScanLists, img: &mut Image) {
    if s.lists.is_empty() && img.block(block::SCAN_LISTS).is_none() {
        return;
    }
    let p = img.block_mut(block::SCAN_LISTS);
    let old = (p[0] as usize).min(MAX_SCAN_LISTS);
    let n = s.lists.len().min(MAX_SCAN_LISTS);
    p[0] = n as u8;
    for (i, l) in s.lists.iter().take(n).enumerate() {
        p[offset(i)..offset(i) + ScanList::SIZE].copy_from_slice(l.bytes());
    }
    let unused = ScanList::unused();
    for i in n..old {
        p[offset(i)..offset(i) + ScanList::SIZE].copy_from_slice(unused.bytes());
    }
    p[TAIL..TAIL + TAIL_LEN].copy_from_slice(&s.tail);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn factory() -> Image {
        crate::image::data_file::import(include_bytes!("../../tests/fixtures/factory-v1.60.data"))
            .unwrap()
    }

    fn changed(a: &Image, b: &Image) -> Vec<usize> {
        let (pa, pb) = (a.block(0x11).unwrap(), b.block(0x11).unwrap());
        (0..pa.len()).filter(|&i| pa[i] != pb[i]).collect()
    }

    #[test]
    fn decode_factory() {
        let s = decode(&factory());
        assert_eq!(s.lists.len(), 2);
        let l = &s.lists[0];
        assert_eq!(l.name(), "Scan List 1");
        let mut want: Vec<_> = (1..=15).map(ScanMember::Channel).collect();
        want.push(ScanMember::Channel(15));
        assert_eq!(l.members(), want);
        assert_eq!(l.ctc_mode(), CtcScanMode::All);
        assert_eq!(l.tx_mode(), ScanTxMode::Current);
        assert_eq!(l.hang_time(), 6);
        assert_eq!(l.priority1_mode(), PriorityMode::None);
        assert_eq!(l.priority2_mode(), PriorityMode::None);
        assert_eq!(l.priority1_channel(), 1);
        assert_eq!(s.lists[1].name(), "Scan List 2");
        assert_eq!(
            s.lists[1].members(),
            (1..=9).map(ScanMember::Channel).collect::<Vec<_>>()
        );
        assert_eq!(s.scan_mode(), ScanMode::Time);
        assert_eq!(s.vhf_range(), (144, 148));
        assert_eq!(s.uhf_range(), (430, 440));
    }

    #[test]
    fn round_trip() {
        let img = factory();
        let mut out = img.clone();
        encode(&decode(&img), &mut out);
        assert_eq!(out, img);
    }

    #[test]
    fn edits_touch_only_their_bits() {
        let img = factory();
        let mut s = decode(&img);
        s.lists[1].set_tx_mode(ScanTxMode::Designated);
        s.lists[1].set_priority2_mode(PriorityMode::Channel);
        let mut out = img.clone();
        encode(&s, &mut out);
        assert_eq!(changed(&img, &out), vec![0x3A + 0x0C, 0x3A + 0x0E]);
        let back = decode(&out);
        assert_eq!(back.lists[1].tx_mode(), ScanTxMode::Designated);
        assert_eq!(back.lists[1].ctc_mode(), CtcScanMode::All);
        assert_eq!(back.lists[1].priority1_mode(), PriorityMode::None);

        s.set_uhf_range((420, 450));
        encode(&s, &mut out);
        assert_eq!(decode(&out).uhf_range(), (420, 450));
        assert_eq!(
            &out.block(0x11).unwrap()[0xE05..0xE09],
            &[0x20, 0x04, 0x50, 0x04]
        );
    }

    #[test]
    fn name_eleven_chars() {
        let mut l = ScanList::new("ABCDEFGHIJKL");
        assert_eq!(l.name(), "ABCDEFGHIJ");
        assert_eq!(l.bytes()[10], 0);
        l.raw[..11].copy_from_slice(b"RIC Mon All");
        assert_eq!(l.name(), "RIC Mon All");
    }

    #[test]
    fn members_with_selected() {
        let mut l = ScanList::new("Mon");
        l.set_members(&[
            ScanMember::Selected,
            ScanMember::Channel(14),
            ScanMember::Channel(15),
        ]);
        assert_eq!(l.channel_count(), 3);
        assert_eq!(&l.bytes()[0x18..0x20], &[0, 0, 14, 0, 15, 0, 0xFF, 0xFF]);
        assert_eq!(l.members()[0], ScanMember::Selected);
        let many: Vec<_> = (1..=20).map(ScanMember::Channel).collect();
        l.set_members(&many);
        assert_eq!(l.members().len(), MAX_CHANNELS);
        assert_eq!(&l.bytes()[0x36..0x38], &[0xFF, 0xFF]);
    }

    #[test]
    fn grow_and_shrink() {
        let img = factory();
        let mut s = decode(&img);
        s.lists.push(ScanList::new("Third"));
        let mut out = img.clone();
        encode(&s, &mut out);
        assert_eq!(out.block(0x11).unwrap()[0], 3);
        assert_eq!(decode(&out).lists[2].name(), "Third");

        s.lists.truncate(1);
        encode(&s, &mut out);
        let p = out.block(0x11).unwrap();
        assert_eq!(p[0], 1);
        for i in 1..3 {
            assert_eq!(&p[offset(i)..offset(i) + 57], ScanList::unused().bytes());
        }
        // The factory image's own unused slots look the same.
        assert_eq!(
            &img.block(0x11).unwrap()[offset(2)..offset(3)],
            ScanList::unused().bytes()
        );
        assert_eq!(decode(&out).lists.len(), 1);
    }
}
