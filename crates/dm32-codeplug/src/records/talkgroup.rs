//! Talk groups — the CPS "contacts" list of name / DMR ID / call type (blocks `0x44..=0x48`).
//!
//! Each block holds 170 records of 24 bytes from offset 0 (qdmr's model). The spec describes the
//! same bytes as a 1-byte block header followed by 25- then 24-byte entries; both put every name
//! at `24 × n + 2`, so they are byte-identical. How records spread over blocks `0x45..=0x48` is
//! not established by any capture (only `0x44` is populated anywhere).
//!
//! A talk group's index (used by the channel TX contact in blocks `0x42`/`0x43`, by block `0x0B`
//! and as the 1-based number shown by the CPS) is its physical slot position, so slots are kept
//! positional. The count, slot bitmap and sorted index tables live in block `0x0B` (see the
//! `quick_contacts` module) and must be rebuilt by the caller after editing this list.
//! The reference implementation reports that the radio crashes with an empty list and injects
//! an "All Call" entry; callers should avoid writing zero talk groups.

use crate::encoding::{get_str_ff, get_u24, is_blank, put_str, put_u24};
use crate::image::{Image, block};
use crate::{choice, raw_record};

pub const MAX_TALKGROUPS: usize = 800;
pub const PER_BLOCK: usize = 170;
pub const NAME_MAX: usize = 16;

choice! {
    /// Call type (`0x16`), hardware-confirmed. CPS `[NormalCallType]` order with a +3 bias.
    pub enum CallType {
        Private = 3 => "Private Call",
        Group = 4 => "Group Call",
        All = 5 => "All Call",
    }
}

/// One 24-byte talk-group record.
///
/// | Offset | Field |
/// |---|---|
/// | `0x00` | ❓ in slot 0 the spec's "block header" (factory `0x00`, OEM CPS `0xFF`), else fill |
/// | `0x01` | ❓ flag (`0x00` factory/reference, `0xFF` OEM CPS) |
/// | `0x02` | name, 16 bytes, NUL-terminated, then `0x00` or `0xFF` padding |
/// | `0x12` | fill |
/// | `0x13` | DMR ID, u24 little-endian binary (`Contacts 10` = `FF FF FF`) |
/// | `0x16` | call type |
/// | `0x17` | fill |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TalkGroup {
    raw: [u8; 24],
}

raw_record!(TalkGroup, 24, 0x00);

impl TalkGroup {
    pub fn new(name: &str, id: u32, call_type: CallType) -> TalkGroup {
        let mut t = TalkGroup::default();
        t.set_name(name);
        t.set_id(id);
        t.set_call_type(call_type);
        t
    }

    /// True if every byte is `0x00`/`0xFF`, i.e. the slot is unused.
    pub fn is_empty(&self) -> bool {
        is_blank(&self.raw)
    }

    pub fn name(&self) -> String {
        get_str_ff(&self.raw[0x02..0x12])
    }

    pub fn set_name(&mut self, s: &str) {
        put_str(&mut self.raw[0x02..0x12], s, NAME_MAX, 0x00);
    }

    pub fn id(&self) -> u32 {
        get_u24(&self.raw[0x13..])
    }

    pub fn set_id(&mut self, id: u32) {
        put_u24(&mut self.raw[0x13..], id.min(0xFF_FFFF));
    }

    pub fn call_type(&self) -> CallType {
        CallType::from_raw(self.raw[0x16])
    }

    pub fn set_call_type(&mut self, v: CallType) {
        self.raw[0x16] = v.to_raw();
    }
}

/// All talk groups, by slot.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TalkGroups {
    /// Slot `n` is talk group index `n + 1`. Trailing empty slots are trimmed on decode.
    pub slots: Vec<Option<TalkGroup>>,
}

impl TalkGroups {
    /// The populated talk groups with their 1-based indexes.
    pub fn iter(&self) -> impl Iterator<Item = (usize, &TalkGroup)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, t)| t.as_ref().map(|t| (i + 1, t)))
    }
}

/// Block ID and in-block offset of slot `index` (0-based).
pub fn locate(index: usize) -> (u8, usize) {
    (
        block::TALKGROUPS.start() + (index / PER_BLOCK) as u8,
        (index % PER_BLOCK) * TalkGroup::SIZE,
    )
}

fn read_slots(img: &Image) -> Vec<Option<TalkGroup>> {
    let mut slots: Vec<_> = (0..MAX_TALKGROUPS)
        .map(|i| {
            let (id, off) = locate(i);
            img.block(id)
                .map(|p| TalkGroup::from_bytes(&p[off..]))
                .filter(|t| !t.is_empty())
        })
        .collect();
    while slots.last().is_some_and(Option::is_none) {
        slots.pop();
    }
    slots
}

pub fn decode(img: &Image) -> TalkGroups {
    TalkGroups {
        slots: read_slots(img),
    }
}

/// Write talk groups back (at most 800). Slots that held a talk group but are now empty are
/// zeroed (the factory / reference fill); blocks are only created for slots that are used.
pub fn encode(t: &TalkGroups, img: &mut Image) {
    let old = read_slots(img).len();
    let n = t.slots.len().min(MAX_TALKGROUPS);
    for i in 0..n.max(old) {
        let (id, off) = locate(i);
        let range = off..off + TalkGroup::SIZE;
        match t.slots.get(i).filter(|_| i < n).and_then(Option::as_ref) {
            Some(r) => img.block_mut(id)[range].copy_from_slice(r.bytes()),
            None => {
                if let Some(p) = img.blocks.get_mut(&id)
                    && !is_blank(&p[range.clone()])
                {
                    p[range].fill(0);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::PAGE;

    fn factory() -> Image {
        crate::image::data_file::import(include_bytes!("../../tests/fixtures/factory-v1.60.data"))
            .unwrap()
    }

    #[test]
    fn decode_factory() {
        let t = decode(&factory());
        assert_eq!(t.slots.len(), 10);
        for (n, tg) in t.iter() {
            assert_eq!(tg.name(), format!("Contacts {n}"));
        }
        let ids: Vec<_> = t.iter().map(|(_, g)| g.id()).collect();
        assert_eq!(ids, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 0xFF_FFFF]);
        let types: Vec<_> = t.iter().map(|(_, g)| g.call_type()).collect();
        use CallType::*;
        assert_eq!(
            types,
            vec![
                Group, Group, Group, Group, Group, Private, Private, Private, Private, All
            ]
        );
    }

    #[test]
    fn round_trip() {
        let img = factory();
        let mut out = img.clone();
        encode(&decode(&img), &mut out);
        assert_eq!(out, img);
    }

    #[test]
    fn edit_id_only() {
        let img = factory();
        let mut t = decode(&img);
        t.slots[2].as_mut().unwrap().set_id(3154);
        let mut out = img.clone();
        encode(&t, &mut out);
        let (a, b) = (img.block(0x44).unwrap(), out.block(0x44).unwrap());
        let d: Vec<_> = (0..PAGE).filter(|&i| a[i] != b[i]).collect();
        assert_eq!(d, vec![48 + 0x13, 48 + 0x14]);
        assert_eq!(&b[48 + 0x13..48 + 0x16], &[0x52, 0x0C, 0x00]);
    }

    #[test]
    fn name_sixteen_chars() {
        let mut t = TalkGroup::new("ABCDEFGHIJKLMNOPQ", 9, CallType::Private);
        assert_eq!(t.name(), "ABCDEFGHIJKLMNOP");
        t.set_name("Short");
        assert_eq!(&t.bytes()[2..9], b"Short\0\0");
        assert_eq!(t.bytes()[0x16], 3);
    }

    #[test]
    fn grow_and_shrink() {
        let img = factory();
        let mut t = decode(&img);
        t.slots.resize(PER_BLOCK + 1, None);
        t.slots
            .push(Some(TalkGroup::new("Far", 91, CallType::Group)));
        let mut out = img.clone();
        encode(&t, &mut out);
        assert!(!out.blocks.contains_key(&0x46));
        let p = out.block(0x45).unwrap();
        assert_eq!(p[PAGE - 1], 0x45);
        assert_eq!(&p[24 + 2..24 + 6], b"Far\0");
        let back = decode(&out);
        assert_eq!(back.slots.len(), PER_BLOCK + 2);
        assert_eq!(back.slots[PER_BLOCK + 1].as_ref().unwrap().name(), "Far");

        t.slots.truncate(3);
        t.slots[1] = None;
        encode(&t, &mut out);
        let back = decode(&out);
        assert_eq!(back.slots.len(), 3);
        assert!(back.slots[1].is_none());
        assert!(out.block(0x44).unwrap()[24..48].iter().all(|&b| b == 0));
        assert!(out.block(0x44).unwrap()[72..0xFFF].iter().all(|&b| b == 0));
        assert!(out.block(0x45).unwrap()[24..48].iter().all(|&b| b == 0));
    }
}
