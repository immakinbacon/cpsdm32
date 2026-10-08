//! Zones (blocks `0x5C..=0x64`).
//!
//! The first zone block starts with a 16-byte header (global zone count + VFO selections); zone
//! records (145 bytes) follow at `0x010`. Later blocks have no header and hold zones from offset
//! 0. Each block holds 28 zones, so zone `n` (0-based) lives in block `0x5C + n / 28`.
//! The per-block packing and the header-less later blocks come from the spec/qdmr; no capture
//! has more than one populated zone block.

use crate::encoding::{get_str_ff, get_u16, put_str, put_u16};
use crate::image::{Image, block};
use crate::raw_record;

pub const MAX_ZONES: usize = 250;
pub const ZONES_PER_BLOCK: usize = 28;
pub const MAX_CHANNELS: usize = 64;
/// Longest name written. The field is 11 bytes per the spec (`"Zone 1" 00 FF..`), 16 per qdmr;
/// the fixture has `0xFF` through byte 15 so either fits. We read up to 16 bytes (stopping at
/// `0x00`/`0xFF`) and write at most 10 characters + NUL, which is valid under both readings.
pub const NAME_MAX: usize = 10;
const NAME_FIELD: usize = 11;
const HEADER_LEN: usize = 16;

/// One 145-byte zone record.
///
/// Layout: name `0x00` (see [`NAME_MAX`]), `0x0B..0x10` padding (`0xFF`), channel count `0x10`,
/// 64 × u16 LE absolute 1-based channel numbers at `0x11`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zone {
    raw: [u8; 145],
}

raw_record!(Zone, 145, 0xFF);

impl Zone {
    pub fn new(name: &str) -> Zone {
        let mut z = Zone::default();
        z.set_name(name);
        z.set_channels(&[]);
        z
    }

    pub fn name(&self) -> String {
        get_str_ff(&self.raw[..16])
    }

    pub fn set_name(&mut self, s: &str) {
        put_str(&mut self.raw[..NAME_FIELD], s, NAME_MAX, 0xFF);
    }

    /// The stored channel count byte (not range-checked).
    pub fn channel_count(&self) -> u8 {
        self.raw[0x10]
    }

    /// Member channels (absolute, 1-based). Reads `count` slots (capped at 64) and drops
    /// `0x0000`/`0xFFFF` fill, since captured records exist whose count is off by one.
    pub fn channels(&self) -> Vec<u16> {
        let n = (self.channel_count() as usize).min(MAX_CHANNELS);
        (0..n)
            .map(|i| get_u16(&self.raw[0x11 + 2 * i..]))
            .filter(|&c| c != 0 && c != 0xFFFF)
            .collect()
    }

    /// Replace the member list (truncated to 64). Unused slots are padded with `0xFF`: the radio
    /// reads `count` entries, and a `0x0000` terminator is known to make it show null slots.
    pub fn set_channels(&mut self, chans: &[u16]) {
        let n = chans.len().min(MAX_CHANNELS);
        self.raw[0x10] = n as u8;
        for (i, &c) in chans.iter().take(n).enumerate() {
            put_u16(&mut self.raw[0x11 + 2 * i..], c);
        }
        self.raw[0x11 + 2 * n..].fill(0xFF);
    }
}

/// The 16-byte header at the start of block `0x5C`.
///
/// Byte 0 is the global zone count (hardware-confirmed), maintained by [`encode`]. Bytes 1, 3,
/// 5, 7 are the VFO A/B channel and zone selections per qdmr (1-based, `0` = unset); the factory
/// image holds `09 00 06 00 01 00 01 00`. The OEM CPS writes `0xFF` into the odd bytes in between,
/// so these are treated as single bytes and the others are preserved untouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneHeader {
    raw: [u8; HEADER_LEN],
}

raw_record!(ZoneHeader, 16, 0xFF);

fn opt_u8(v: u8) -> Option<u8> {
    (v != 0 && v != 0xFF).then_some(v)
}

impl ZoneHeader {
    pub fn count(&self) -> u8 {
        self.raw[0]
    }
    pub fn channel_a(&self) -> Option<u8> {
        opt_u8(self.raw[1])
    }
    pub fn set_channel_a(&mut self, v: Option<u8>) {
        self.raw[1] = v.unwrap_or(0);
    }
    pub fn channel_b(&self) -> Option<u8> {
        opt_u8(self.raw[3])
    }
    pub fn set_channel_b(&mut self, v: Option<u8>) {
        self.raw[3] = v.unwrap_or(0);
    }
    pub fn zone_a(&self) -> Option<u8> {
        opt_u8(self.raw[5])
    }
    pub fn set_zone_a(&mut self, v: Option<u8>) {
        self.raw[5] = v.unwrap_or(0);
    }
    pub fn zone_b(&self) -> Option<u8> {
        opt_u8(self.raw[7])
    }
    pub fn set_zone_b(&mut self, v: Option<u8>) {
        self.raw[7] = v.unwrap_or(0);
    }
}

/// All zones.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Zones {
    pub header: ZoneHeader,
    pub zones: Vec<Zone>,
}

/// Block ID and in-block offset of zone `index` (0-based).
pub fn locate(index: usize) -> (u8, usize) {
    let b = index / ZONES_PER_BLOCK;
    let base = if b == 0 { HEADER_LEN } else { 0 };
    (
        block::ZONES.start() + b as u8,
        base + (index % ZONES_PER_BLOCK) * Zone::SIZE,
    )
}

fn stored_count(img: &Image) -> usize {
    img.block(*block::ZONES.start())
        .map_or(0, |p| (p[0] as usize).min(MAX_ZONES))
}

pub fn decode(img: &Image) -> Zones {
    let Some(first) = img.block(*block::ZONES.start()) else {
        return Zones::default();
    };
    let header = ZoneHeader::from_bytes(&first[..HEADER_LEN]);
    let zones = (0..stored_count(img))
        .map(|i| {
            let (id, off) = locate(i);
            img.block(id)
                .map(|p| Zone::from_bytes(&p[off..]))
                .unwrap_or_default()
        })
        .collect();
    Zones { header, zones }
}

/// Write zones back (at most 250). Zone slots vacated by shrinking the list are filled with
/// `0xFF`; no end-of-zones marker is written (the OEM CPS writes none).
pub fn encode(z: &Zones, img: &mut Image) {
    let first = *block::ZONES.start();
    if z.zones.is_empty() && img.block(first).is_none() {
        return;
    }
    let old = stored_count(img);
    let n = z.zones.len().min(MAX_ZONES);
    let hdr = img.block_mut(first);
    hdr[..HEADER_LEN].copy_from_slice(z.header.bytes());
    hdr[0] = n as u8;
    for (i, zone) in z.zones.iter().take(n).enumerate() {
        let (id, off) = locate(i);
        img.block_mut(id)[off..off + Zone::SIZE].copy_from_slice(zone.bytes());
    }
    for i in n..old {
        let (id, off) = locate(i);
        if let Some(p) = img.blocks.get_mut(&id) {
            p[off..off + Zone::SIZE].fill(0xFF);
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

    fn diff(a: &Image, b: &Image) -> Vec<(u8, usize)> {
        let mut v = Vec::new();
        for id in a.blocks.keys().chain(b.blocks.keys()) {
            let blank = [0u8; PAGE];
            let pa = a.blocks.get(id).map_or(&blank[..], |p| &p[..]);
            let pb = b.blocks.get(id).map_or(&blank[..], |p| &p[..]);
            v.extend((0..PAGE).filter(|&i| pa[i] != pb[i]).map(|i| (*id, i)));
        }
        v.sort();
        v.dedup();
        v
    }

    #[test]
    fn decode_factory() {
        let z = decode(&factory());
        assert_eq!(z.zones.len(), 2);
        assert_eq!(z.zones[0].name(), "Zone 1");
        assert_eq!(z.zones[0].channels(), (1..=16).collect::<Vec<_>>());
        assert_eq!(z.zones[1].name(), "Func Demo");
        assert_eq!(z.zones[1].channels(), (17..=25).collect::<Vec<_>>());
        assert_eq!(z.header.count(), 2);
        assert_eq!(z.header.channel_a(), Some(9));
        assert_eq!(z.header.channel_b(), Some(6));
        assert_eq!(z.header.zone_a(), Some(1));
        assert_eq!(z.header.zone_b(), Some(1));
    }

    #[test]
    fn round_trip() {
        let img = factory();
        let mut out = img.clone();
        encode(&decode(&img), &mut out);
        assert_eq!(out, img);
    }

    #[test]
    fn edit_name_only_touches_name() {
        let img = factory();
        let mut z = decode(&img);
        z.zones[1].set_name("Hotspot");
        let mut out = img.clone();
        encode(&z, &mut out);
        let d = diff(&img, &out);
        assert!(!d.is_empty());
        assert!(
            d.iter()
                .all(|&(id, o)| id == 0x5C && (0xA1..0xA1 + 11).contains(&o))
        );
        assert_eq!(decode(&out).zones[1].name(), "Hotspot");
    }

    #[test]
    fn set_channels_pads_ff() {
        let mut z = Zone::new("Test");
        z.set_channels(&[3, 1, 128]);
        assert_eq!(z.channels(), vec![3, 1, 128]);
        assert_eq!(z.bytes()[0x10], 3);
        assert!(z.bytes()[0x17..].iter().all(|&b| b == 0xFF));
        assert_eq!(&z.bytes()[..6], b"Test\0\xFF");
    }

    #[test]
    fn grow_and_shrink() {
        let img = factory();
        let mut z = decode(&img);
        for i in 3..=30 {
            z.zones.push(Zone::new(&format!("Z{i}")));
        }
        let mut out = img.clone();
        encode(&z, &mut out);
        assert_eq!(out.block(0x5C).unwrap()[0], 30);
        let p = out.block(0x5D).unwrap();
        assert_eq!(&p[..4], b"Z29\0");
        assert_eq!(&p[145..149], b"Z30\0");
        assert_eq!(p[PAGE - 1], 0x5D);
        assert!(out.block(0x5F).is_none());
        let back = decode(&out);
        assert_eq!(back.zones.len(), 30);
        assert_eq!(back.zones[28].name(), "Z29");

        z.zones.truncate(1);
        encode(&z, &mut out);
        assert_eq!(out.block(0x5C).unwrap()[0], 1);
        let (_, off) = locate(1);
        assert!(
            out.block(0x5C).unwrap()[off..off + 145]
                .iter()
                .all(|&b| b == 0xFF)
        );
        assert!(
            out.block(0x5D).unwrap()[..2 * 145]
                .iter()
                .all(|&b| b == 0xFF)
        );
        assert_eq!(decode(&out).zones.len(), 1);
    }

    #[test]
    fn empty_image_untouched() {
        let mut img = Image::default();
        encode(&Zones::default(), &mut img);
        assert!(img.blocks.is_empty());
    }
}
