//! Block `0x06`: DTMF, analog contacts and BDC1200 contacts.
//!
//! | Offset  | Content                                                          |
//! |---------|------------------------------------------------------------------|
//! | `0x000` | 16 DTMF encode codes, 16 bytes each                              |
//! | `0x100` | DTMF settings (16 bytes, mostly unmapped)                        |
//! | `0x110` | 4 DTMF special codes (PTT ID up/down, stun, kill — ⚠️ inferred)   |
//! | `0x1FF` | analog contact count                                             |
//! | `0x200` | analog contacts, 32 bytes each                                   |
//! | `0xA20` | unknown (`01 00 01 00 0F 03 05 0A` in the factory image)          |
//! | `0xA90` | BDC1200 contact count                                            |
//! | `0xAA0` | BDC1200 contacts, 20 bytes each                                  |
//!
//! Code table, analog contact names at `0x200` and the `0xAA0` table are hardware anchors. The
//! spec says to leave `0x1FF` alone until identified; it equals the number of analog contacts
//! in both captures (factory 7, OEM write 1), so it is maintained as their count here. The
//! capacities (64 analog contacts, 64 BDC contacts) and the BDC count at `0xA90` are inferred
//! from the factory layout (the next populated bytes after the tables), not confirmed.

use crate::encoding::{get_str, get_u16, is_blank, put_str, put_u16};
use crate::image::{Image, block};
use crate::{choice, raw_record};

const BLOCK: u8 = block::ANALOG;
const CODES: usize = 0x000;
const SETTINGS: usize = 0x100;
const SPECIAL: usize = 0x110;
const CONTACT_COUNT: usize = 0x1FF;
const CONTACTS: usize = 0x200;
const BDC_COUNT: usize = 0xA90;
const BDC: usize = 0xAA0;
/// Fill for cleared contact slots (the factory image's fill in both tables).
const FILL: u8 = 0x00;

pub const CODE_SLOTS: usize = 16;
pub const MAX_CONTACTS: usize = 64;
pub const MAX_BDC: usize = 64;

/// DTMF digit for a stored value: `0x00-0x09` digits, `0x0A-0x0D` A-D, `0x0E` `*`, `0x0F` `#`.
pub fn dtmf_char(b: u8) -> Option<char> {
    match b {
        0x00..=0x09 => Some((b'0' + b) as char),
        0x0A..=0x0D => Some((b'A' + b - 0x0A) as char),
        0x0E => Some('*'),
        0x0F => Some('#'),
        _ => None,
    }
}

/// Stored value for a DTMF digit (case-insensitive).
pub fn dtmf_value(c: char) -> Option<u8> {
    match c.to_ascii_uppercase() {
        c @ '0'..='9' => Some(c as u8 - b'0'),
        c @ 'A'..='D' => Some(c as u8 - b'A' + 0x0A),
        '*' => Some(0x0E),
        '#' => Some(0x0F),
        _ => None,
    }
}

fn get_dtmf(b: &[u8]) -> String {
    b.iter().map_while(|&c| dtmf_char(c)).collect()
}

/// Write digits (invalid characters dropped, truncated), `0xFF`-padded.
fn put_dtmf(b: &mut [u8], s: &str) {
    b.fill(0xFF);
    for (d, v) in b.iter_mut().zip(s.chars().filter_map(dtmf_value)) {
        *d = v;
    }
}

/// A DTMF code sequence (16 bytes, digits then `0xFF`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DtmfCode {
    raw: [u8; 16],
}

raw_record!(DtmfCode, 16, 0xFF);

impl DtmfCode {
    pub const LEN: usize = 16;

    pub fn new(digits: &str) -> Self {
        let mut c = DtmfCode::default();
        c.set_digits(digits);
        c
    }

    /// The digits, up to the first byte that is not a DTMF value (empty slots start `0xFF`).
    pub fn digits(&self) -> String {
        get_dtmf(&self.raw)
    }

    pub fn set_digits(&mut self, s: &str) {
        put_dtmf(&mut self.raw, s);
    }
}

choice! {
    /// DTMF group code (`0x109`): stored as the DTMF value of the character, Off = `0x09`.
    pub enum DtmfGroupCode {
        Off = 0x09 => "Off",
        A = 0x0A => "A",
        B = 0x0B => "B",
        C = 0x0C => "C",
        D = 0x0D => "D",
        Star = 0x0E => "*",
        Hash = 0x0F => "#",
    }
}

choice! {
    /// DTMF interval sign (`0x10A`), stored as the DTMF value of the character.
    pub enum DtmfIntervalSign {
        A = 0x0A => "A",
        B = 0x0B => "B",
        C = 0x0C => "C",
        D = 0x0D => "D",
        Star = 0x0E => "*",
        Hash = 0x0F => "#",
    }
}

/// DTMF settings at `0x100` (16 bytes).
///
/// Only the group code and interval sign agree with both the spec and the factory image
/// (`0A` = A, `0E` = *). The spec's other derived offsets (timing bytes, auto answer stored
/// "value + 4" at `0x10C`, which the factory `00` contradicts) are left raw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DtmfSettings {
    raw: [u8; 16],
}

raw_record!(DtmfSettings, 16, 0x00);

impl DtmfSettings {
    /// Self ID code (`0x106-0x108`, three DTMF digits). ⚠️ Inferred: the spec calls these bytes
    /// a duration, but the factory holds `01 02 03` and the CPS "Self ID Code" is 3 digits.
    pub fn self_id(&self) -> String {
        get_dtmf(&self.raw[0x06..0x09])
    }

    pub fn set_self_id(&mut self, s: &str) {
        put_dtmf(&mut self.raw[0x06..0x09], s);
    }

    pub fn group_code(&self) -> DtmfGroupCode {
        DtmfGroupCode::from_raw(self.raw[0x09])
    }

    pub fn set_group_code(&mut self, v: DtmfGroupCode) {
        self.raw[0x09] = v.to_raw();
    }

    pub fn interval_sign(&self) -> DtmfIntervalSign {
        DtmfIntervalSign::from_raw(self.raw[0x0A])
    }

    pub fn set_interval_sign(&mut self, v: DtmfIntervalSign) {
        self.raw[0x0A] = v.to_raw();
    }
}

/// An analog (DTMF) contact (32 bytes).
///
/// The CPS shows Name and ID. The name is at `+0x00`; how the ID is stored in `+0x10-+0x1F` is
/// unknown (factory: `09 06 00 00 nn` where `nn` counts 0..6), so those bytes are kept raw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalogContact {
    raw: [u8; 32],
}

raw_record!(AnalogContact, 32, FILL);

impl AnalogContact {
    pub const NAME_LEN: usize = 16;

    pub fn new(name: &str) -> Self {
        let mut c = AnalogContact::default();
        c.set_name(name);
        c
    }

    pub fn name(&self) -> String {
        get_str(&self.raw[0x00..0x10])
    }

    pub fn set_name(&mut self, name: &str) {
        put_str(&mut self.raw[0x00..0x10], name, Self::NAME_LEN, FILL);
    }

    pub fn is_empty(&self) -> bool {
        is_blank(&self.raw)
    }
}

choice! {
    /// BDC1200 contact call type (`+0x12`, ⚠️ inferred from the CPS `[Mdc1200CallType]` list;
    /// every factory entry holds 0).
    pub enum BdcCallType { Private = 0 => "Private", Group = 1 => "Group", All = 2 => "All" }
}

/// A BDC1200 (MDC1200-style) contact (20 bytes): name, ID, type, one unknown byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BdcContact {
    raw: [u8; 20],
}

raw_record!(BdcContact, 20, FILL);

impl BdcContact {
    pub const NAME_LEN: usize = 16;

    pub fn new(name: &str, id: u16) -> Self {
        let mut c = BdcContact::default();
        c.set_name(name);
        c.set_id(id);
        c
    }

    pub fn name(&self) -> String {
        get_str(&self.raw[0x00..0x10])
    }

    pub fn set_name(&mut self, name: &str) {
        put_str(&mut self.raw[0x00..0x10], name, Self::NAME_LEN, FILL);
    }

    /// The 4-hex-digit BDC1200 ID (uint16 LE at `+0x10`; factory IDs read `E001`..`E010`).
    /// ⚠️ Inferred.
    pub fn id(&self) -> u16 {
        get_u16(&self.raw[0x10..])
    }

    pub fn set_id(&mut self, id: u16) {
        put_u16(&mut self.raw[0x10..], id);
    }

    pub fn call_type(&self) -> BdcCallType {
        BdcCallType::from_raw(self.raw[0x12])
    }

    pub fn set_call_type(&mut self, t: BdcCallType) {
        self.raw[0x12] = t.to_raw();
    }
}

/// Block `0x06`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Analog {
    /// DTMF encode list, positional.
    pub codes: [DtmfCode; CODE_SLOTS],
    pub settings: DtmfSettings,
    /// Special codes at `0x110`: PTT ID up, PTT ID down, stun, kill (⚠️ order inferred from the
    /// CPS DTMF page).
    pub special: [DtmfCode; 4],
    pub contacts: Vec<AnalogContact>,
    pub bdc: Vec<BdcContact>,
}

pub fn decode(img: &Image) -> Analog {
    let Some(b) = img.block(BLOCK) else {
        return Analog::default();
    };
    let nc = (b[CONTACT_COUNT] as usize).min(MAX_CONTACTS);
    let nb = (b[BDC_COUNT] as usize).min(MAX_BDC);
    Analog {
        codes: std::array::from_fn(|i| DtmfCode::from_bytes(&b[CODES + i * 16..])),
        settings: DtmfSettings::from_bytes(&b[SETTINGS..]),
        special: std::array::from_fn(|i| DtmfCode::from_bytes(&b[SPECIAL + i * 16..])),
        contacts: (0..nc)
            .map(|i| AnalogContact::from_bytes(&b[CONTACTS + i * AnalogContact::SIZE..]))
            .collect(),
        bdc: (0..nb)
            .map(|i| BdcContact::from_bytes(&b[BDC + i * BdcContact::SIZE..]))
            .collect(),
    }
}

/// Write `n` records of `size` at `base`, clearing slots `n..old` to [`FILL`].
fn write_list<'a>(
    b: &mut [u8],
    base: usize,
    size: usize,
    recs: impl Iterator<Item = &'a [u8]>,
    old: usize,
) -> usize {
    let mut n = 0;
    for r in recs {
        b[base + n * size..base + (n + 1) * size].copy_from_slice(r);
        n += 1;
    }
    if old > n {
        b[base + n * size..base + old * size].fill(FILL);
    }
    n
}

/// Write codes, settings, special codes, both contact lists (truncated to their maxima) and
/// their counts; contact slots no longer used are cleared to `0x00`.
pub fn encode(a: &Analog, img: &mut Image) {
    if img.block(BLOCK).is_none() && *a == Analog::default() {
        return;
    }
    let b = img.block_mut(BLOCK);
    for (i, c) in a.codes.iter().enumerate() {
        b[CODES + i * 16..CODES + (i + 1) * 16].copy_from_slice(c.bytes());
    }
    b[SETTINGS..SETTINGS + 16].copy_from_slice(a.settings.bytes());
    for (i, c) in a.special.iter().enumerate() {
        b[SPECIAL + i * 16..SPECIAL + (i + 1) * 16].copy_from_slice(c.bytes());
    }

    let old = (b[CONTACT_COUNT] as usize).min(MAX_CONTACTS);
    let recs = a.contacts.iter().take(MAX_CONTACTS).map(|c| &c.bytes()[..]);
    b[CONTACT_COUNT] = write_list(&mut b[..], CONTACTS, AnalogContact::SIZE, recs, old) as u8;

    let old = (b[BDC_COUNT] as usize).min(MAX_BDC);
    let recs = a.bdc.iter().take(MAX_BDC).map(|c| &c.bytes()[..]);
    b[BDC_COUNT] = write_list(&mut b[..], BDC, BdcContact::SIZE, recs, old) as u8;

    b[0xFFF] = BLOCK;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::data_file;

    fn factory() -> Image {
        data_file::import(include_bytes!("../../tests/fixtures/factory-v1.60.data")).unwrap()
    }

    fn diff(a: &Image, b: &Image) -> Vec<usize> {
        let (a, b) = (a.block(BLOCK).unwrap(), b.block(BLOCK).unwrap());
        (0..0x1000).filter(|&i| a[i] != b[i]).collect()
    }

    #[test]
    fn decode_factory() {
        let a = decode(&factory());
        let codes: Vec<String> = a.codes.iter().map(|c| c.digits()).collect();
        assert_eq!(
            codes[..7],
            [
                "456*123",
                "45A*123",
                "4A0*123",
                "AAA*123",
                "B223456789#",
                "C123456789#",
                ""
            ]
        );
        assert!(codes[6..].iter().all(|c| c.is_empty()));
        let special: Vec<String> = a.special.iter().map(|c| c.digits()).collect();
        assert_eq!(special, ["456D123", "789D123", "B223456789", "C123456789"]);
        assert_eq!(a.settings.self_id(), "123");
        assert_eq!(a.settings.group_code(), DtmfGroupCode::A);
        assert_eq!(a.settings.interval_sign(), DtmfIntervalSign::Star);

        let names: Vec<String> = a.contacts.iter().map(|c| c.name()).collect();
        assert_eq!(names[0], "AContact 1");
        assert_eq!(
            names[1..],
            [
                "AContacts 1",
                "AContacts 2",
                "AContacts 3",
                "AContacts 4",
                "AContacts 5",
                "AContacts 6"
            ]
        );

        assert_eq!(a.bdc.len(), 10);
        for (i, c) in a.bdc.iter().enumerate() {
            assert_eq!(c.name(), format!("BDC Cotnacts {}", i + 1));
            assert_eq!(c.call_type(), BdcCallType::Private);
        }
        assert_eq!(a.bdc[0].id(), 0xE001);
        assert_eq!(a.bdc[9].id(), 0xE010);
    }

    #[test]
    fn round_trip() {
        let img = factory();
        let mut out = img.clone();
        encode(&decode(&img), &mut out);
        assert_eq!(out, img);
    }

    #[test]
    fn edits_touch_only_their_bytes() {
        let img = factory();
        let mut a = decode(&img);
        a.codes[7].set_digits("12#");
        a.settings.set_group_code(DtmfGroupCode::Hash);
        a.bdc[2].set_id(0x1234);
        let mut out = img.clone();
        encode(&a, &mut out);
        assert_eq!(diff(&img, &out), [0x70, 0x71, 0x72, 0x109, 0xAD8, 0xAD9]);
        assert_eq!(decode(&out).codes[7].digits(), "12#");
    }

    #[test]
    fn add_remove() {
        let img = factory();
        let mut a = decode(&img);
        a.contacts.push(AnalogContact::new("Base"));
        a.bdc.truncate(3);
        let mut out = img.clone();
        encode(&a, &mut out);
        let b = out.block(BLOCK).unwrap();
        assert_eq!(b[CONTACT_COUNT], 8);
        assert_eq!(b[BDC_COUNT], 3);
        assert!(b[BDC + 3 * 20..BDC + 10 * 20].iter().all(|&c| c == 0));
        let d = decode(&out);
        assert_eq!(d.contacts[7].name(), "Base");
        assert_eq!(d.bdc.len(), 3);
        // Unknown bytes between the tables survive.
        assert_eq!(b[0xA20..0xA90], img.block(BLOCK).unwrap()[0xA20..0xA90]);
    }

    #[test]
    fn dtmf_chars() {
        for v in 0..16u8 {
            assert_eq!(dtmf_value(dtmf_char(v).unwrap()), Some(v));
        }
        assert_eq!(DtmfCode::new("1x2").digits(), "12");
    }
}
