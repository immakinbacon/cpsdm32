//! The radio's own DMR radio IDs (block `0x67`).
//!
//! Layout (hardware-confirmed): count byte at `0x000`, header bytes `0x001-0x00F` unknown and
//! preserved, 16-byte entries from `0x010`, up to 250. A channel references a radio ID by its
//! physical slot, so the list is positional.

use crate::encoding::{get_str, get_u24, is_blank, put_str, put_u24};
use crate::image::{Image, block};
use crate::raw_record;

const BLOCK: u8 = block::RADIO_IDS;
const COUNT: usize = 0x000;
const BASE: usize = 0x010;
/// Fill byte for cleared entries (the factory image and the reference implementation use 0x00).
const FILL: u8 = 0x00;

/// Maximum number of radio IDs.
pub const MAX: usize = 250;

/// One radio ID (16 bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RadioId {
    raw: [u8; 16],
}

raw_record!(RadioId, 16, FILL);

impl RadioId {
    /// Longest name written. The field is 12 bytes; qdmr writes 12 characters unterminated, the
    /// reference implementation at most 11 plus a NUL. We keep the terminator.
    pub const NAME_LEN: usize = 11;

    pub fn new(id: u32, name: &str) -> Self {
        let mut r = RadioId::default();
        r.set_id(id);
        r.set_name(name);
        r
    }

    /// The DMR ID (uint24 little-endian binary, not BCD).
    pub fn id(&self) -> u32 {
        get_u24(&self.raw[0x00..])
    }

    pub fn set_id(&mut self, id: u32) {
        put_u24(&mut self.raw[0x00..], id.min(0xFF_FFFF));
    }

    pub fn name(&self) -> String {
        get_str(&self.raw[0x03..0x0F])
    }

    pub fn set_name(&mut self, name: &str) {
        put_str(&mut self.raw[0x03..0x0F], name, Self::NAME_LEN, FILL);
    }

    /// All bytes `0x00` / `0xFF`: an unused slot.
    pub fn is_empty(&self) -> bool {
        is_blank(&self.raw)
    }
}

/// The radio ID list, in slot order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RadioIds {
    pub ids: Vec<RadioId>,
}

fn entry(i: usize) -> usize {
    BASE + i * RadioId::SIZE
}

pub fn decode(img: &Image) -> RadioIds {
    let Some(b) = img.block(BLOCK) else {
        return RadioIds::default();
    };
    let n = (b[COUNT] as usize).min(MAX);
    RadioIds {
        ids: (0..n)
            .map(|i| RadioId::from_bytes(&b[entry(i)..]))
            .collect(),
    }
}

/// Write the list (truncated to [`MAX`]): count byte, entries, and slots no longer used are
/// cleared to `0x00`. Header bytes `0x001-0x00F` are left alone.
pub fn encode(ids: &RadioIds, img: &mut Image) {
    if img.block(BLOCK).is_none() && ids.ids.is_empty() {
        return;
    }
    let b = img.block_mut(BLOCK);
    let old = (b[COUNT] as usize).min(MAX);
    let n = ids.ids.len().min(MAX);
    for (i, id) in ids.ids.iter().take(n).enumerate() {
        b[entry(i)..entry(i + 1)].copy_from_slice(id.bytes());
    }
    for i in n..old {
        b[entry(i)..entry(i + 1)].fill(FILL);
    }
    b[COUNT] = n as u8;
    b[0xFFF] = BLOCK;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::data_file;

    fn factory() -> Image {
        data_file::import(include_bytes!("../../tests/fixtures/factory-v1.60.data")).unwrap()
    }

    #[test]
    fn decode_factory() {
        let ids = decode(&factory());
        let got: Vec<(u32, String)> = ids.ids.iter().map(|r| (r.id(), r.name())).collect();
        assert_eq!(
            got,
            [
                (123, "Radio 1".to_string()),
                (2, "Radio 2".into()),
                (3, "Radio 3".into()),
                (4, "Radio 4".into()),
                (5, "Radio 5".into()),
            ]
        );
    }

    #[test]
    fn round_trip() {
        let img = factory();
        let mut out = img.clone();
        encode(&decode(&img), &mut out);
        assert_eq!(out.block(BLOCK), img.block(BLOCK));
    }

    #[test]
    fn edit_only_touches_field() {
        let img = factory();
        let mut ids = decode(&img);
        ids.ids[1].set_id(1234567);
        let mut out = img.clone();
        encode(&ids, &mut out);
        let (a, b) = (img.block(BLOCK).unwrap(), out.block(BLOCK).unwrap());
        let diff: Vec<usize> = (0..0x1000).filter(|&i| a[i] != b[i]).collect();
        assert_eq!(diff, [0x20, 0x21, 0x22]);
        assert_eq!(&b[0x20..0x23], &[0x87, 0xD6, 0x12]);
        assert_eq!(decode(&out).ids[1].id(), 1234567);
    }

    #[test]
    fn add_remove() {
        let img = factory();
        let mut ids = decode(&img);
        ids.ids.push(RadioId::new(3100000, "New ID"));
        let mut out = img.clone();
        encode(&ids, &mut out);
        assert_eq!(out.block(BLOCK).unwrap()[0], 6);
        assert_eq!(decode(&out).ids[5].name(), "New ID");

        ids.ids.truncate(2);
        encode(&ids, &mut out);
        let b = out.block(BLOCK).unwrap();
        assert_eq!(b[0], 2);
        assert!(b[0x30..0x80].iter().all(|&c| c == 0));
        assert_eq!(decode(&out).ids.len(), 2);
    }

    #[test]
    fn name_limit() {
        let mut r = RadioId::default();
        r.set_name("ABCDEFGHIJKLMNOP");
        assert_eq!(r.name(), "ABCDEFGHIJK");
        assert_eq!(r.bytes()[0x0E], 0);
        assert_eq!(r.bytes()[0x0F], 0);
    }

    #[test]
    fn absent_block_not_created() {
        let mut img = Image::default();
        encode(&RadioIds::default(), &mut img);
        assert!(img.block(BLOCK).is_none());
    }
}
