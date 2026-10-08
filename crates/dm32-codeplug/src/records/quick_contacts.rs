//! Quick access contact index (block `0x0B`): counts, a slot bitmap and two sorted index tables
//! over the talk groups in blocks `0x44..=0x48`. Rebuilt from the talk-group list whenever it
//! changes (see [`QuickContacts::build`]).
//!
//! | Offset  | Content                                                                  |
//! |---------|--------------------------------------------------------------------------|
//! | `0x000` | total talk groups (u16 LE)                                               |
//! | `0x002` | Group Call count (u16 LE)                                                |
//! | `0x004` | All Call count (u8) — see below                                          |
//! | `0x010` | slot bitmap, 800 bits, `0` = used (qdmr; the spec documents 16 bytes)    |
//! | `0x100` | index table sorted by name (byte-wise ASCII), up to 768 entries          |
//! | `0x740` | index table sorted by DMR ID, up to 736 entries                          |
//!
//! Byte `0x004`: qdmr writes the Private Call count there as a u16, but the factory image has
//! 4 private / 1 all-call talk groups and stores `01`, with `0x005` = `0xFF` in both captures.
//! The hardware wins: it is written as a single byte holding the All Call count.
//!
//! Bitmap: the spec says 16 bytes (128 slots) and leaves `0x020-0x0FF` unknown; qdmr uses 100
//! bytes (800 slots, the talk-group maximum). Both agree on every captured byte (unused bits
//! are `1`), so the qdmr size is used.

use crate::encoding::{get_u16, put_u16};
use crate::image::{Image, block};
use crate::{choice, raw_record};

const BLOCK: u8 = block::QUICK_CONTACTS;
const TOTAL: usize = 0x000;
const GROUPS: usize = 0x002;
const ALL_CALLS: usize = 0x004;
const BITMAP: usize = 0x010;
const BITMAP_LEN: usize = 100;
const BY_NAME: usize = 0x100;
const BY_ID: usize = 0x740;

/// Capacity of the name-sorted table (`0x100-0x6FF`).
pub const MAX_BY_NAME: usize = 768;
/// Capacity of the ID-sorted table (`0x740-0xCFF`).
pub const MAX_BY_ID: usize = 736;

choice! {
    /// Talk-group call type as stored in an index entry's high nibble.
    pub enum CallType {
        Private = 3 => "Private Call",
        Group = 4 => "Group Call",
        All = 5 => "All Call",
    }
}

/// One index entry: uint16 LE, bits 0-11 = 1-based talk-group slot, bits 12-15 = call type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexEntry {
    raw: [u8; 2],
}

raw_record!(IndexEntry, 2, 0xFF);

impl IndexEntry {
    pub fn new(slot: u16, t: CallType) -> Self {
        let mut e = IndexEntry { raw: [0; 2] };
        e.set_slot(slot);
        e.set_call_type(t);
        e
    }

    fn word(&self) -> u16 {
        u16::from_le_bytes(self.raw)
    }

    /// 1-based physical slot in the talk-group list (what channels' TX contact refers to).
    pub fn slot(&self) -> u16 {
        self.word() & 0x0FFF
    }

    pub fn set_slot(&mut self, slot: u16) {
        self.raw = ((self.word() & 0xF000) | (slot & 0x0FFF)).to_le_bytes();
    }

    pub fn call_type(&self) -> CallType {
        CallType::from_raw((self.word() >> 12) as u8)
    }

    pub fn set_call_type(&mut self, t: CallType) {
        let w = (self.word() & 0x0FFF) | (((t.to_raw() & 0x0F) as u16) << 12);
        self.raw = w.to_le_bytes();
    }
}

/// A talk group as seen by [`QuickContacts::build`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source<'a> {
    /// 1-based physical slot in the talk-group list.
    pub slot: u16,
    pub name: &'a str,
    pub number: u32,
    pub call_type: CallType,
}

/// Block `0x0B`. Counts and the bitmap are derived from `by_name` on encode.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuickContacts {
    pub by_name: Vec<IndexEntry>,
    pub by_id: Vec<IndexEntry>,
}

impl QuickContacts {
    /// Build both tables: by name (byte-wise ASCII, as the radio sorts) and by DMR ID.
    pub fn build(talk_groups: &[Source]) -> Self {
        let entry = |s: &Source| IndexEntry::new(s.slot, s.call_type);
        let mut by_name: Vec<&Source> = talk_groups.iter().collect();
        by_name.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));
        let mut by_id: Vec<&Source> = talk_groups.iter().collect();
        by_id.sort_by_key(|s| s.number);
        QuickContacts {
            by_name: by_name.into_iter().map(entry).collect(),
            by_id: by_id.into_iter().map(entry).collect(),
        }
    }

    pub fn count(&self) -> usize {
        self.by_name.len()
    }

    fn count_type(&self, t: CallType) -> usize {
        self.by_name.iter().filter(|e| e.call_type() == t).count()
    }
}

pub fn decode(img: &Image) -> QuickContacts {
    let Some(b) = img.block(BLOCK) else {
        return QuickContacts::default();
    };
    let n = get_u16(&b[TOTAL..]) as usize;
    let read = |base: usize, max: usize| {
        (0..n.min(max))
            .map(|i| IndexEntry::from_bytes(&b[base + 2 * i..]))
            .collect()
    };
    QuickContacts {
        by_name: read(BY_NAME, MAX_BY_NAME),
        by_id: read(BY_ID, MAX_BY_ID),
    }
}

/// Write counts, bitmap and both tables. Table entries no longer used become `FF FF`; the
/// unknown bytes `0x005-0x00F`, `0x074-0x0FF` and `0x700-0x73F` are untouched.
pub fn encode(q: &QuickContacts, img: &mut Image) {
    if img.block(BLOCK).is_none() && q.by_name.is_empty() && q.by_id.is_empty() {
        return;
    }
    let groups = q.count_type(CallType::Group);
    let alls = q.count_type(CallType::All);
    let b = img.block_mut(BLOCK);
    let old = get_u16(&b[TOTAL..]) as usize;

    let write = |b: &mut [u8], base: usize, max: usize, list: &[IndexEntry]| {
        let n = list.len().min(max);
        for (i, e) in list.iter().take(n).enumerate() {
            b[base + 2 * i..base + 2 * i + 2].copy_from_slice(e.bytes());
        }
        if old.min(max) > n {
            b[base + 2 * n..base + 2 * old.min(max)].fill(0xFF);
        }
    };
    write(&mut b[..], BY_NAME, MAX_BY_NAME, &q.by_name);
    write(&mut b[..], BY_ID, MAX_BY_ID, &q.by_id);

    let bitmap = &mut b[BITMAP..BITMAP + BITMAP_LEN];
    bitmap.fill(0xFF);
    for e in &q.by_name {
        let s = e.slot() as usize;
        if (1..=BITMAP_LEN * 8).contains(&s) {
            bitmap[(s - 1) / 8] &= !(1 << ((s - 1) % 8));
        }
    }

    put_u16(&mut b[TOTAL..], q.count().min(MAX_BY_NAME) as u16);
    put_u16(&mut b[GROUPS..], groups as u16);
    b[ALL_CALLS] = alls.min(255) as u8;
    b[0xFFF] = BLOCK;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::data_file;

    fn factory() -> Image {
        data_file::import(include_bytes!("../../tests/fixtures/factory-v1.60.data")).unwrap()
    }

    /// The factory talk groups: "Contacts 1".."Contacts 10"; 1-5 group calls with numbers 1-5,
    /// 6-9 private, 10 all call (number 0xFFFFFF).
    fn factory_sources() -> Vec<(String, u32, CallType)> {
        (1..=10u32)
            .map(|i| {
                let t = match i {
                    1..=5 => CallType::Group,
                    6..=9 => CallType::Private,
                    _ => CallType::All,
                };
                (
                    format!("Contacts {i}"),
                    if i == 10 { 0xFF_FFFF } else { i },
                    t,
                )
            })
            .collect()
    }

    #[test]
    fn decode_factory() {
        let q = decode(&factory());
        let slots: Vec<u16> = q.by_name.iter().map(|e| e.slot()).collect();
        assert_eq!(slots, [1, 10, 2, 3, 4, 5, 6, 7, 8, 9]);
        let ids: Vec<u16> = q.by_id.iter().map(|e| e.slot()).collect();
        assert_eq!(ids, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        assert_eq!(q.by_name[1].call_type(), CallType::All);
        assert_eq!(q.by_name[6].call_type(), CallType::Private);
        assert_eq!(q.by_name[0].call_type(), CallType::Group);
    }

    #[test]
    fn round_trip() {
        let img = factory();
        let mut out = img.clone();
        encode(&decode(&img), &mut out);
        assert_eq!(out, img);
    }

    #[test]
    fn build_reproduces_factory() {
        let img = factory();
        let src = factory_sources();
        let sources: Vec<Source> = src
            .iter()
            .enumerate()
            .map(|(i, (name, number, t))| Source {
                slot: i as u16 + 1,
                name,
                number: *number,
                call_type: *t,
            })
            .collect();
        let q = QuickContacts::build(&sources);
        assert_eq!(q, decode(&img));
        let mut out = img.clone();
        encode(&q, &mut out);
        assert_eq!(out, img);
    }

    #[test]
    fn add_remove() {
        let img = factory();
        let mut q = decode(&img);
        q.by_name.push(IndexEntry::new(11, CallType::Group));
        q.by_id.push(IndexEntry::new(11, CallType::Group));
        let mut out = img.clone();
        encode(&q, &mut out);
        let b = out.block(BLOCK).unwrap();
        assert_eq!(&b[0..5], &[11, 0, 6, 0, 1]);
        assert_eq!(&b[0x10..0x12], &[0x00, 0xF8]);
        assert_eq!(&b[0x114..0x116], &[0x0B, 0x40]);

        q.by_name.truncate(2);
        q.by_id.truncate(2);
        encode(&q, &mut out);
        let b = out.block(BLOCK).unwrap();
        assert_eq!(&b[0..5], &[2, 0, 1, 0, 1]);
        assert_eq!(&b[0x10..0x12], &[0xFE, 0xFD]); // slots 1 and 10: bits 0 and 9
        assert!(b[0x104..0x120].iter().all(|&c| c == 0xFF));
        assert!(b[0x744..0x760].iter().all(|&c| c == 0xFF));
        assert_eq!(b[0x05..0x10], img.block(BLOCK).unwrap()[0x05..0x10]);
    }

    #[test]
    fn entry_bits() {
        let mut e = IndexEntry::new(0x123, CallType::Private);
        assert_eq!(e.bytes(), &[0x23, 0x31]);
        e.set_call_type(CallType::All);
        assert_eq!(e.slot(), 0x123);
        assert_eq!(e.bytes(), &[0x23, 0x51]);
    }
}
