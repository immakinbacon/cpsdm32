//! DMR roaming: roam zones (block `0x65`) and roam channels (block `0x66`).
//!
//! Strides and name offsets are hardware-confirmed; the field layout follows qdmr, which the
//! factory image agrees with (zone count byte `0x03` = 3 zones, channel count at `0xFF0` = 3,
//! 400/410 MHz frequencies in BCD).
//!
//! The spec places the zone name at entry `+0x10`; it counts from the block start. Zones start
//! at `0x010` with the name at `+0x00`, the channel count at `+0x10` and 16 channel indexes at
//! `+0x11` (qdmr). The spec's "`+0x02`/`+0x03` stored value + 1, max 0x40" are the block header
//! bytes below (default roam zone, 64 zones).

use crate::encoding::{Freq, get_freq, get_str, put_freq, put_str};
use crate::image::{Image, block};
use crate::{choice, raw_record};

/// Cleared slots are written `0xFF` (the radio leaves one `0xFF` entry after the last one).
const CLEAR: u8 = 0xFF;

// ---------------------------------------------------------------------------------------------
// Roam channels (0x66)

const CH_BLOCK: u8 = block::ROAM_CHANNELS;
const CH_COUNT: usize = 0xFF0;

/// Maximum number of roam channels.
pub const MAX_CHANNELS: usize = 150;

choice! {
    /// Roam channel time slot.
    pub enum TimeSlot { Slot1 = 0 => "Slot 1", Slot2 = 1 => "Slot 2" }
}

/// One roam channel (26 bytes, packed from `0x000`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoamChannel {
    raw: [u8; 26],
}

raw_record!(RoamChannel, 26, 0x00);

impl RoamChannel {
    pub const NAME_LEN: usize = 16;

    pub fn new(name: &str, rx: Freq, tx: Freq) -> Self {
        let mut c = RoamChannel::default();
        c.set_name(name);
        c.set_rx_freq(Some(rx));
        c.set_tx_freq(Some(tx));
        c
    }

    pub fn name(&self) -> String {
        get_str(&self.raw[0x00..0x10])
    }

    pub fn set_name(&mut self, name: &str) {
        put_str(&mut self.raw[0x00..0x10], name, Self::NAME_LEN, 0x00);
    }

    pub fn rx_freq(&self) -> Option<Freq> {
        get_freq(&self.raw[0x10..])
    }

    pub fn set_rx_freq(&mut self, f: Option<Freq>) {
        put_freq(&mut self.raw[0x10..], f);
    }

    pub fn tx_freq(&self) -> Option<Freq> {
        get_freq(&self.raw[0x14..])
    }

    pub fn set_tx_freq(&mut self, f: Option<Freq>) {
        put_freq(&mut self.raw[0x14..], f);
    }

    /// Colour code, 0-15.
    pub fn color_code(&self) -> u8 {
        self.raw[0x18]
    }

    pub fn set_color_code(&mut self, cc: u8) {
        self.raw[0x18] = cc.min(15);
    }

    pub fn time_slot(&self) -> TimeSlot {
        TimeSlot::from_raw(self.raw[0x19])
    }

    pub fn set_time_slot(&mut self, ts: TimeSlot) {
        self.raw[0x19] = ts.to_raw();
    }
}

/// The roam channel list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoamChannels {
    pub channels: Vec<RoamChannel>,
}

fn ch_entry(i: usize) -> usize {
    i * RoamChannel::SIZE
}

pub fn decode_channels(img: &Image) -> RoamChannels {
    let Some(b) = img.block(CH_BLOCK) else {
        return RoamChannels::default();
    };
    let n = (b[CH_COUNT] as usize).min(MAX_CHANNELS);
    RoamChannels {
        channels: (0..n)
            .map(|i| RoamChannel::from_bytes(&b[ch_entry(i)..]))
            .collect(),
    }
}

/// Write the channels (truncated to [`MAX_CHANNELS`]) and the count at `0xFF0`; slots no longer
/// used are cleared to `0xFF`.
pub fn encode_channels(c: &RoamChannels, img: &mut Image) {
    if img.block(CH_BLOCK).is_none() && c.channels.is_empty() {
        return;
    }
    let b = img.block_mut(CH_BLOCK);
    let old = (b[CH_COUNT] as usize).min(MAX_CHANNELS);
    let n = c.channels.len().min(MAX_CHANNELS);
    for (i, ch) in c.channels.iter().take(n).enumerate() {
        b[ch_entry(i)..ch_entry(i + 1)].copy_from_slice(ch.bytes());
    }
    for i in n..old {
        b[ch_entry(i)..ch_entry(i + 1)].fill(CLEAR);
    }
    b[CH_COUNT] = n as u8;
    b[0xFFF] = CH_BLOCK;
}

// ---------------------------------------------------------------------------------------------
// Roam zones (0x65)

const ZONE_BLOCK: u8 = block::ROAM_ZONES;
const ZONE_BASE: usize = 0x010;

/// Maximum number of roam zones.
pub const MAX_ZONES: usize = 64;

/// The 16-byte header of block `0x65`. Byte `0x000` is the zone count, maintained by
/// [`encode_zones`]; `0x004-0x00F` are unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoamHeader {
    raw: [u8; 16],
}

raw_record!(RoamHeader, 16, 0x00);

impl RoamHeader {
    /// Auto roaming (`0x001`, `1` = on).
    pub fn auto_roam(&self) -> bool {
        self.raw[0x01] == 1
    }

    pub fn set_auto_roam(&mut self, on: bool) {
        self.raw[0x01] = on as u8;
    }

    /// Roaming delay (`0x002`), in minutes per qdmr. ⚠️ The spec (CPS decompilation) says this
    /// byte is stored as "value + 1"; the factory value is 1.
    pub fn roam_delay(&self) -> u8 {
        self.raw[0x02]
    }

    pub fn set_roam_delay(&mut self, v: u8) {
        self.raw[0x02] = v;
    }

    /// Default roam zone (`0x003`, 0-based index stored + 1; 0 = none).
    pub fn default_zone(&self) -> Option<usize> {
        (self.raw[0x03] as usize).checked_sub(1)
    }

    pub fn set_default_zone(&mut self, z: Option<usize>) {
        self.raw[0x03] = z.map_or(0, |i| (i.min(MAX_ZONES - 1) + 1) as u8);
    }
}

/// One roam zone (33 bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoamZone {
    raw: [u8; 33],
}

raw_record!(RoamZone, 33, 0x00);

impl RoamZone {
    pub const NAME_LEN: usize = 16;
    /// Maximum channels per zone.
    pub const CHANNELS: usize = 16;

    /// An empty zone as the radio stores one (unused channel indexes `0xFF`).
    pub fn new(name: &str) -> Self {
        let mut z = RoamZone::default();
        z.set_name(name);
        z.set_channels(&[]);
        z
    }

    pub fn name(&self) -> String {
        get_str(&self.raw[0x00..0x10])
    }

    pub fn set_name(&mut self, name: &str) {
        put_str(&mut self.raw[0x00..0x10], name, Self::NAME_LEN, 0x00);
    }

    /// Member roam channels (0-based indexes into [`RoamChannels`]).
    pub fn channels(&self) -> Vec<usize> {
        let n = (self.raw[0x10] as usize).min(Self::CHANNELS);
        self.raw[0x11..0x11 + n]
            .iter()
            .filter(|&&c| c != 0 && c != 0xFF)
            .map(|&c| c as usize - 1)
            .collect()
    }

    /// Set the members (at most 16) and the count; unused index bytes become `0xFF`.
    pub fn set_channels(&mut self, chans: &[usize]) {
        let n = chans.len().min(Self::CHANNELS);
        self.raw[0x10] = n as u8;
        for (i, slot) in self.raw[0x11..0x21].iter_mut().enumerate() {
            *slot = match chans.get(i) {
                Some(&c) if i < n => (c.min(MAX_CHANNELS - 1) + 1) as u8,
                _ => 0xFF,
            };
        }
    }
}

/// Block `0x65`: header and zone list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoamZones {
    pub header: RoamHeader,
    pub zones: Vec<RoamZone>,
}

fn zone_entry(i: usize) -> usize {
    ZONE_BASE + i * RoamZone::SIZE
}

pub fn decode_zones(img: &Image) -> RoamZones {
    let Some(b) = img.block(ZONE_BLOCK) else {
        return RoamZones::default();
    };
    let n = (b[0] as usize).min(MAX_ZONES);
    RoamZones {
        header: RoamHeader::from_bytes(&b[..]),
        zones: (0..n)
            .map(|i| RoamZone::from_bytes(&b[zone_entry(i)..]))
            .collect(),
    }
}

/// Write header, zones (truncated to [`MAX_ZONES`]) and the count; slots no longer used are
/// cleared to `0xFF`.
pub fn encode_zones(z: &RoamZones, img: &mut Image) {
    if img.block(ZONE_BLOCK).is_none() && z.zones.is_empty() && z.header == RoamHeader::default() {
        return;
    }
    let b = img.block_mut(ZONE_BLOCK);
    let old = (b[0] as usize).min(MAX_ZONES);
    let n = z.zones.len().min(MAX_ZONES);
    b[1..RoamHeader::SIZE].copy_from_slice(&z.header.bytes()[1..]);
    for (i, zone) in z.zones.iter().take(n).enumerate() {
        b[zone_entry(i)..zone_entry(i + 1)].copy_from_slice(zone.bytes());
    }
    for i in n..old {
        b[zone_entry(i)..zone_entry(i + 1)].fill(CLEAR);
    }
    b[0] = n as u8;
    b[0xFFF] = ZONE_BLOCK;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::data_file;

    fn factory() -> Image {
        data_file::import(include_bytes!("../../tests/fixtures/factory-v1.60.data")).unwrap()
    }

    fn diff(a: &Image, b: &Image, id: u8) -> Vec<usize> {
        let (a, b) = (a.block(id).unwrap(), b.block(id).unwrap());
        (0..0x1000).filter(|&i| a[i] != b[i]).collect()
    }

    #[test]
    fn decode_factory_zones() {
        let z = decode_zones(&factory());
        let names: Vec<String> = z.zones.iter().map(|z| z.name()).collect();
        assert_eq!(names, ["Roam Zone 1", "Roam Zone 2", "Roam Zone 3"]);
        assert!(z.zones.iter().all(|z| z.channels().is_empty()));
        assert!(!z.header.auto_roam());
        assert_eq!(z.header.roam_delay(), 1);
        assert_eq!(z.header.default_zone(), Some(0));
    }

    #[test]
    fn decode_factory_channels() {
        let c = decode_channels(&factory());
        assert_eq!(c.channels.len(), 3);
        for (i, ch) in c.channels.iter().enumerate() {
            assert_eq!(ch.name(), format!("Roam CH {}", i + 1));
            assert_eq!(ch.rx_freq().unwrap().to_string(), "400.00000");
            assert_eq!(ch.tx_freq().unwrap().to_string(), "410.00000");
            assert_eq!(ch.color_code(), 0);
            assert_eq!(ch.time_slot(), TimeSlot::Slot1);
        }
    }

    #[test]
    fn round_trip() {
        let img = factory();
        let mut out = img.clone();
        encode_zones(&decode_zones(&img), &mut out);
        encode_channels(&decode_channels(&img), &mut out);
        assert_eq!(out, img);
        // A fresh zone matches the radio's own empty-zone bytes.
        assert_eq!(
            RoamZone::new("Roam Zone 1").bytes()[..],
            img.block(ZONE_BLOCK).unwrap()[0x10..0x31]
        );
    }

    #[test]
    fn edits() {
        let img = factory();
        let mut z = decode_zones(&img);
        z.zones[1].set_channels(&[0, 2]);
        z.header.set_auto_roam(true);
        let mut out = img.clone();
        encode_zones(&z, &mut out);
        assert_eq!(diff(&img, &out, ZONE_BLOCK), [0x01, 0x41, 0x42, 0x43]);
        assert_eq!(decode_zones(&out).zones[1].channels(), [0, 2]);

        let mut c = decode_channels(&img);
        c.channels[2].set_time_slot(TimeSlot::Slot2);
        c.channels[2].set_color_code(7);
        encode_channels(&c, &mut out);
        assert_eq!(diff(&img, &out, CH_BLOCK), [0x4C, 0x4D]);
    }

    #[test]
    fn add_remove() {
        let img = factory();
        let mut out = img.clone();
        let mut c = decode_channels(&img);
        c.channels.push(RoamChannel::new(
            "New",
            "439.5".parse().unwrap(),
            "430.1".parse().unwrap(),
        ));
        encode_channels(&c, &mut out);
        assert_eq!(out.block(CH_BLOCK).unwrap()[CH_COUNT], 4);
        assert_eq!(decode_channels(&out).channels[3].name(), "New");
        c.channels.truncate(1);
        encode_channels(&c, &mut out);
        let b = out.block(CH_BLOCK).unwrap();
        assert_eq!(b[CH_COUNT], 1);
        assert!(b[26..26 * 4].iter().all(|&x| x == 0xFF));

        let mut z = decode_zones(&img);
        z.zones.push(RoamZone::new("Z4"));
        encode_zones(&z, &mut out);
        assert_eq!(out.block(ZONE_BLOCK).unwrap()[0], 4);
        z.zones.clear();
        encode_zones(&z, &mut out);
        assert_eq!(out.block(ZONE_BLOCK).unwrap()[0], 0);
        assert!(decode_zones(&out).zones.is_empty());
    }
}
