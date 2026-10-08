//! RX group lists (block `0x0F`).
//!
//! Block layout: u32 LE slot bitmap at `0x000` (bit `n` = slot `n` in use; the factory image
//! holds `0x1F` for 5 groups), ❓ unknown bytes `0x004..0x010` (preserved), a flag byte `0x010`
//! (`0x01` in both captures, preserved), then 32 record slots of 109 bytes from `0x011`.
//!
//! A channel's RX group list ID is the 1-based slot number, so slots are kept positional
//! ([`RxGroups::slots`]); a clear bitmap bit is a `None` slot.

use crate::encoding::{get_str, get_u24, get_u32, put_str, put_u24, put_u32};
use crate::image::{Image, block};
use crate::raw_record;

pub const MAX_GROUPS: usize = 32;
pub const MAX_IDS: usize = 32;
/// Longest name written (11-byte field; NUL-terminated, zero-padded).
pub const NAME_MAX: usize = 10;
const HEADER_LEN: usize = 0x11;

/// One 109-byte RX group record.
///
/// Name at `0x00` (11 bytes), 32 × u24 LE DMR IDs at `0x0B` (raw talk-group numbers, not
/// talk-group indexes — confirmed by the OEM write capture), ❓ unknown `0x6B`/`0x6C` (carry
/// data on hardware, preserved).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RxGroup {
    raw: [u8; 109],
}

raw_record!(RxGroup, 109, 0x00);

impl RxGroup {
    pub fn new(name: &str) -> RxGroup {
        let mut g = RxGroup::default();
        g.set_name(name);
        g
    }

    pub fn name(&self) -> String {
        get_str(&self.raw[..11])
    }

    pub fn set_name(&mut self, s: &str) {
        put_str(&mut self.raw[..11], s, NAME_MAX, 0x00);
    }

    /// Member DMR IDs, up to the first empty (`0` or `0xFFFFFF`) slot.
    pub fn ids(&self) -> Vec<u32> {
        (0..MAX_IDS)
            .map(|i| get_u24(&self.raw[0x0B + 3 * i..]))
            .take_while(|&id| id != 0 && id != 0xFF_FFFF)
            .collect()
    }

    /// Replace the member IDs (truncated to 32); the remaining slots are zeroed.
    pub fn set_ids(&mut self, ids: &[u32]) {
        for i in 0..MAX_IDS {
            put_u24(
                &mut self.raw[0x0B + 3 * i..],
                ids.get(i).copied().unwrap_or(0),
            );
        }
    }
}

/// All RX group lists, by slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RxGroups {
    /// Slot `n` is RX group list ID `n + 1`. Trailing empty slots are trimmed on decode.
    pub slots: Vec<Option<RxGroup>>,
    header: [u8; HEADER_LEN],
}

impl Default for RxGroups {
    fn default() -> Self {
        let mut header = [0; HEADER_LEN];
        header[0x10] = 0x01;
        RxGroups {
            slots: Vec::new(),
            header,
        }
    }
}

impl RxGroups {
    /// The populated groups with their 1-based IDs.
    pub fn iter(&self) -> impl Iterator<Item = (usize, &RxGroup)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, g)| g.as_ref().map(|g| (i + 1, g)))
    }
}

fn offset(i: usize) -> usize {
    HEADER_LEN + i * RxGroup::SIZE
}

pub fn decode(img: &Image) -> RxGroups {
    let Some(p) = img.block(block::RX_GROUPS) else {
        return RxGroups::default();
    };
    let bitmap = get_u32(&p[..]);
    let mut slots: Vec<_> = (0..MAX_GROUPS)
        .map(|i| (bitmap >> i & 1 != 0).then(|| RxGroup::from_bytes(&p[offset(i)..])))
        .collect();
    while slots.last().is_some_and(Option::is_none) {
        slots.pop();
    }
    let mut header = [0; HEADER_LEN];
    header.copy_from_slice(&p[..HEADER_LEN]);
    RxGroups { slots, header }
}

/// Write RX groups back: rebuilds the bitmap from the slots (at most 32) and fills slots that
/// were in use but are now empty with `0xFF`.
pub fn encode(g: &RxGroups, img: &mut Image) {
    if g.slots.iter().all(Option::is_none) && img.block(block::RX_GROUPS).is_none() {
        return;
    }
    let p = img.block_mut(block::RX_GROUPS);
    let old = get_u32(&p[..]);
    p[4..HEADER_LEN].copy_from_slice(&g.header[4..]);
    let mut bitmap = 0u32;
    for i in 0..MAX_GROUPS {
        let range = offset(i)..offset(i) + RxGroup::SIZE;
        match g.slots.get(i).and_then(Option::as_ref) {
            Some(r) => {
                bitmap |= 1 << i;
                p[range].copy_from_slice(r.bytes());
            }
            None if old >> i & 1 != 0 => p[range].fill(0xFF),
            None => {}
        }
    }
    put_u32(&mut p[..], bitmap);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn factory() -> Image {
        crate::image::data_file::import(include_bytes!("../../tests/fixtures/factory-v1.60.data"))
            .unwrap()
    }

    #[test]
    fn decode_factory() {
        let g = decode(&factory());
        assert_eq!(g.slots.len(), 5);
        for (n, grp) in g.iter() {
            assert_eq!(grp.name(), format!("RX Group {n}"));
            assert_eq!(grp.ids(), (n as u32..=5).collect::<Vec<_>>());
        }
        assert_eq!(g.iter().count(), 5);
    }

    #[test]
    fn round_trip() {
        let img = factory();
        let mut out = img.clone();
        encode(&decode(&img), &mut out);
        assert_eq!(out, img);
    }

    #[test]
    fn edit_ids_only() {
        let img = factory();
        let mut g = decode(&img);
        g.slots[1].as_mut().unwrap().set_ids(&[3154, 91, 0xFF_FFFE]);
        let mut out = img.clone();
        encode(&g, &mut out);
        let (a, b) = (img.block(0x0F).unwrap(), out.block(0x0F).unwrap());
        let ids = offset(1) + 0x0B..offset(1) + 0x0B + 96;
        assert!((0..a.len()).all(|i| a[i] == b[i] || ids.contains(&i)));
        assert_eq!(
            decode(&out).slots[1].as_ref().unwrap().ids(),
            vec![3154, 91, 0xFF_FFFE]
        );
    }

    #[test]
    fn grow_and_shrink() {
        let img = factory();
        let mut g = decode(&img);
        g.slots.push(None);
        g.slots.push(Some(RxGroup::new("Seventh")));
        let mut out = img.clone();
        encode(&g, &mut out);
        assert_eq!(get_u32(&out.block(0x0F).unwrap()[..]), 0b101_1111);
        let back = decode(&out);
        assert_eq!(back.slots.len(), 7);
        assert!(back.slots[5].is_none());
        assert_eq!(back.slots[6].as_ref().unwrap().name(), "Seventh");

        g.slots[2] = None;
        g.slots.truncate(4);
        encode(&g, &mut out);
        let p = out.block(0x0F).unwrap();
        assert_eq!(get_u32(&p[..]), 0b1011);
        for i in [2, 4, 6] {
            assert!(p[offset(i)..offset(i) + 109].iter().all(|&b| b == 0xFF));
        }
        // Slot 5 was never in use: left as it was.
        assert!(p[offset(5)..offset(5) + 109].iter().all(|&b| b == 0));
        assert_eq!(&p[4..0x11], &img.block(0x0F).unwrap()[4..0x11]);
    }
}
