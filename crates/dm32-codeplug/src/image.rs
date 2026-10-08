//! The raw memory image of a radio: logical config blocks, contact database pages and the boot
//! image, plus the identification read during the handshake.
//!
//! Two file formats are supported:
//! - `.dm32`, this program's own lossless format (header JSON + raw pages);
//! - `.data`, the official CPS save file (fixed page order, see [`data_file`]).

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const PAGE: usize = 0x1000;

/// One 4 KB page. The last byte of a config page is its logical block ID.
pub type Page = Box<[u8; PAGE]>;

pub fn blank_page(fill: u8) -> Page {
    Box::new([fill; PAGE])
}

pub fn page_from(data: &[u8]) -> Page {
    let mut p = blank_page(0xFF);
    let n = data.len().min(PAGE);
    p[..n].copy_from_slice(&data[..n]);
    p
}

/// Logical block IDs and the walk orders used by the official CPS.
pub mod block {
    use std::ops::RangeInclusive;

    pub const CALIBRATION: u8 = 0x02;
    pub const CALL_LIST: u8 = 0x03;
    pub const SETTINGS: u8 = 0x04;
    pub const ANALOG: u8 = 0x06;
    pub const QUICK_TEXT: u8 = 0x0A;
    pub const QUICK_CONTACTS: u8 = 0x0B;
    pub const RX_GROUPS: u8 = 0x0F;
    pub const EMERGENCY: u8 = 0x10;
    pub const SCAN_LISTS: u8 = 0x11;
    pub const CHANNELS: RangeInclusive<u8> = 0x12..=0x41;
    pub const TX_CONTACT_LOW: u8 = 0x42;
    pub const TX_CONTACT_HIGH: u8 = 0x43;
    pub const TALKGROUPS: RangeInclusive<u8> = 0x44..=0x48;
    pub const ZONES: RangeInclusive<u8> = 0x5C..=0x64;
    pub const ROAM_ZONES: u8 = 0x65;
    pub const ROAM_CHANNELS: u8 = 0x66;
    pub const RADIO_IDS: u8 = 0x67;

    /// Singleton blocks the CPS reads, in its read order.
    pub const SINGLES: [u8; 12] = [
        CALIBRATION,
        CALL_LIST,
        SETTINGS,
        ANALOG,
        QUICK_TEXT,
        QUICK_CONTACTS,
        RX_GROUPS,
        EMERGENCY,
        SCAN_LISTS,
        ROAM_ZONES,
        ROAM_CHANNELS,
        RADIO_IDS,
    ];

    /// Every logical block that is part of the codeplug, in the CPS read order.
    pub fn read_walk() -> Vec<u8> {
        let mut v: Vec<u8> = CHANNELS.collect();
        v.extend([TX_CONTACT_LOW, TX_CONTACT_HIGH]);
        v.extend(TALKGROUPS);
        v.extend(ZONES);
        v.extend(SINGLES);
        v
    }

    /// Blocks written back to the radio, in the CPS write order. Calibration is never written.
    pub fn write_walk() -> Vec<u8> {
        let mut v: Vec<u8> = SINGLES
            .iter()
            .copied()
            .filter(|&b| b != CALIBRATION)
            .collect();
        v.extend(ZONES);
        v.extend(CHANNELS);
        v.extend([TX_CONTACT_LOW, TX_CONTACT_HIGH]);
        v.extend(TALKGROUPS);
        v
    }

    pub fn is_codeplug(id: u8) -> bool {
        read_walk().contains(&id)
    }

    pub fn name(id: u8) -> String {
        match id {
            CALIBRATION => "calibration".into(),
            CALL_LIST => "call list".into(),
            SETTINGS => "radio settings".into(),
            ANALOG => "DTMF / analog contacts".into(),
            QUICK_TEXT => "quick text messages".into(),
            QUICK_CONTACTS => "quick access contacts".into(),
            RX_GROUPS => "RX group lists".into(),
            EMERGENCY => "emergency / encryption".into(),
            SCAN_LISTS => "scan lists".into(),
            TX_CONTACT_LOW | TX_CONTACT_HIGH => {
                format!("channel TX contacts {}", id - TX_CONTACT_LOW + 1)
            }
            ROAM_ZONES => "roaming zones".into(),
            ROAM_CHANNELS => "roaming channels".into(),
            RADIO_IDS => "DMR radio IDs".into(),
            id if CHANNELS.contains(&id) => format!("channels {}", id - CHANNELS.start() + 1),
            id if TALKGROUPS.contains(&id) => {
                format!("talk groups {}", id - TALKGROUPS.start() + 1)
            }
            id if ZONES.contains(&id) => format!("zones {}", id - ZONES.start() + 1),
            _ => format!("block {id:#04x}"),
        }
    }
}

/// What the radio told us about itself.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ImageInfo {
    pub model: String,
    pub firmware: String,
    #[serde(default)]
    pub build_date: String,
    #[serde(default)]
    pub codeplug_version: String,
    /// Base address of the contact database (V-frame 0x0F).
    #[serde(default)]
    pub contacts_base: Option<u32>,
    /// Raw V-frame payloads (and the 0x47 probe under key 0x47), hex encoded.
    #[serde(default)]
    pub vframes: BTreeMap<u8, String>,
}

impl ImageInfo {
    pub fn vframe(&self, id: u8) -> Option<Vec<u8>> {
        self.vframes.get(&id).map(|h| unhex(h))
    }
}

/// A radio memory image.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Image {
    pub info: ImageInfo,
    /// Logical config blocks by ID. A missing ID means the slot has no page.
    pub blocks: BTreeMap<u8, Page>,
    /// Contact database pages, keyed by offset from the start of the contact region.
    pub contacts: BTreeMap<u32, Page>,
    /// Boot image, raw BGR565 (240×320), if read.
    pub boot_image: Option<Vec<u8>>,
    /// Pages of an imported `.data` file whose meaning is unknown, kept for export.
    pub data_extras: BTreeMap<usize, Page>,
}

const MAGIC: &[u8; 8] = b"DM32CPS\x01";

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PageRef {
    Block { id: u8 },
    Contacts { offset: u32 },
    DataExtra { index: usize },
}

#[derive(Serialize, Deserialize)]
struct Header {
    format: u32,
    info: ImageInfo,
    pages: Vec<PageRef>,
    #[serde(default)]
    boot_image_len: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("not a DM-32 codeplug file: {0}")]
    Format(String),
}

impl Image {
    /// Write the native `.dm32` format.
    pub fn write_native(&self, w: &mut impl Write) -> io::Result<()> {
        let mut pages = Vec::new();
        let mut body: Vec<&[u8]> = Vec::new();
        for (&id, p) in &self.blocks {
            pages.push(PageRef::Block { id });
            body.push(&p[..]);
        }
        for (&offset, p) in &self.contacts {
            pages.push(PageRef::Contacts { offset });
            body.push(&p[..]);
        }
        for (&index, p) in &self.data_extras {
            pages.push(PageRef::DataExtra { index });
            body.push(&p[..]);
        }
        let boot = self.boot_image.as_deref().unwrap_or(&[]);
        let header = Header {
            format: 1,
            info: self.info.clone(),
            pages,
            boot_image_len: boot.len(),
        };
        let json = serde_json::to_vec_pretty(&header)?;
        w.write_all(MAGIC)?;
        w.write_all(&(json.len() as u32).to_le_bytes())?;
        w.write_all(&json)?;
        for p in body {
            w.write_all(p)?;
        }
        w.write_all(boot)?;
        Ok(())
    }

    pub fn read_native(r: &mut impl Read) -> Result<Image, FileError> {
        let mut magic = [0; 8];
        r.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(FileError::Format("bad magic".into()));
        }
        let mut len = [0; 4];
        r.read_exact(&mut len)?;
        let mut json = vec![0; u32::from_le_bytes(len) as usize];
        r.read_exact(&mut json)?;
        let header: Header =
            serde_json::from_slice(&json).map_err(|e| FileError::Format(e.to_string()))?;
        let mut img = Image {
            info: header.info,
            ..Default::default()
        };
        for pref in header.pages {
            let mut p = blank_page(0);
            r.read_exact(&mut p[..])?;
            match pref {
                PageRef::Block { id } => img.blocks.insert(id, p),
                PageRef::Contacts { offset } => img.contacts.insert(offset, p),
                PageRef::DataExtra { index } => img.data_extras.insert(index, p),
            };
        }
        if header.boot_image_len > 0 {
            let mut b = vec![0; header.boot_image_len];
            r.read_exact(&mut b)?;
            img.boot_image = Some(b);
        }
        Ok(img)
    }

    /// Load a `.dm32` or official `.data` file, chosen by content.
    pub fn load(path: &Path) -> Result<Image, FileError> {
        let bytes = std::fs::read(path)?;
        if bytes.starts_with(MAGIC) {
            Image::read_native(&mut &bytes[..])
        } else {
            data_file::import(&bytes)
        }
    }

    /// Save as `.data` if the extension says so, otherwise in the native format.
    pub fn save(&self, path: &Path) -> Result<(), FileError> {
        let is_data = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("data"));
        let mut out = Vec::new();
        if is_data {
            out = data_file::export(self);
        } else {
            self.write_native(&mut out)?;
        }
        // Write to a temporary file first so a failed save never truncates the old file.
        let tmp = path.with_extension("tmp-save");
        std::fs::write(&tmp, &out)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn block(&self, id: u8) -> Option<&Page> {
        self.blocks.get(&id)
    }

    /// Mutable access to a block, creating it (0xFF-filled, ID byte set) if absent.
    pub fn block_mut(&mut self, id: u8) -> &mut Page {
        self.blocks.entry(id).or_insert_with(|| {
            let mut p = blank_page(0xFF);
            p[PAGE - 1] = id;
            p
        })
    }
}

/// The official CPS `.data` file.
///
/// Layout (worked out from a CPS v1.60 file matched page-for-page against an OEM read capture
/// of the same factory codeplug): 161 pages of 4 KB in a fixed order. Slots the radio has no
/// page for hold the buffer the radio returns for the sentinel address (all zero, last byte
/// `0x48`).
pub mod data_file {
    use super::*;

    /// What each page of a `.data` file holds.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Slot {
        /// File header: firmware version string at 0x30.
        Header,
        /// A logical config block.
        Block(u8),
        /// Logical block stored with its last byte replaced by 0xFF (seen for 0x0B).
        BlockNoId(u8),
        /// Raw V-frame payload (0x0D) or the 0x47 probe reply.
        VFrame(u8),
        /// Contact database page `n`, in the CPS's own re-serialised form.
        Contacts(u32),
        /// Unknown; kept verbatim.
        Unknown,
    }

    pub const PAGES: usize = 161;

    pub fn slot(index: usize) -> Slot {
        use super::block::*;
        match index {
            0 => Slot::Header,
            1 => Slot::Unknown,
            2 => Slot::Block(CALIBRATION),
            3 => Slot::VFrame(0x0D),
            4 => Slot::VFrame(0x47),
            5 => Slot::Block(SETTINGS),
            6 => Slot::Block(RADIO_IDS),
            7 => Slot::Block(QUICK_TEXT),
            8 => Slot::Block(SCAN_LISTS),
            9 => Slot::Block(ROAM_ZONES),
            10 => Slot::Block(ROAM_CHANNELS),
            11 => Slot::Block(RX_GROUPS),
            12 => Slot::Block(CALL_LIST),
            13 => Slot::Block(EMERGENCY),
            14 => Slot::Block(ANALOG),
            15 => Slot::Block(TX_CONTACT_LOW),
            16 => Slot::Block(TX_CONTACT_HIGH),
            17..=25 => Slot::Block(0x5C + (index - 17) as u8),
            26 => Slot::Unknown,
            27 => Slot::BlockNoId(QUICK_CONTACTS),
            28..=32 => Slot::Block(0x44 + (index - 28) as u8),
            33..=80 => Slot::Block(0x12 + (index - 33) as u8),
            81..=160 => Slot::Contacts((index - 81) as u32),
            _ => Slot::Unknown,
        }
    }

    const SENTINEL_ID: u8 = 0x48;

    fn is_sentinel(p: &[u8]) -> bool {
        p[PAGE - 1] == SENTINEL_ID && p[..PAGE - 1].iter().all(|&b| b == 0)
    }

    pub fn import(bytes: &[u8]) -> Result<Image, FileError> {
        if bytes.len() < 81 * PAGE || !bytes.len().is_multiple_of(PAGE) {
            return Err(FileError::Format(format!(
                "unexpected size {} bytes (an official CPS .data file is a multiple of 4096 bytes, at least 81 pages)",
                bytes.len()
            )));
        }
        let mut img = Image::default();
        for (i, chunk) in bytes.as_chunks::<PAGE>().0.iter().enumerate() {
            let page = page_from(chunk);
            match slot(i) {
                Slot::Header => {
                    let fw = &chunk[0x30..0x50];
                    img.info.firmware = crate::encoding::get_str(fw);
                    img.data_extras.insert(i, page);
                }
                Slot::Block(id) => {
                    if !is_sentinel(chunk) {
                        let mut p = page;
                        p[PAGE - 1] = id;
                        img.blocks.insert(id, p);
                    }
                }
                Slot::BlockNoId(id) => {
                    if !is_sentinel(chunk) && !chunk.iter().all(|&b| b == 0) {
                        let mut p = page;
                        p[PAGE - 1] = id;
                        img.blocks.insert(id, p);
                    }
                }
                Slot::VFrame(id) => {
                    let len = if id == 0x47 { 256 } else { 64 };
                    img.info.vframes.insert(id, hex(&chunk[..len]));
                }
                // The CPS stores contacts re-serialised (zero padding, different flag bytes);
                // keep those pages verbatim until they are converted.
                Slot::Contacts(_) | Slot::Unknown => {
                    if chunk.iter().any(|&b| b != 0) {
                        img.data_extras.insert(i, page);
                    }
                }
            }
        }
        if img.info.model.is_empty() {
            img.info.model = "DP570UV".into();
        }
        // Convert the CPS form of the contact database to the radio form.
        if let Some(db) = crate::records::contact::from_data_file(&img) {
            crate::records::contact::encode(&db, &mut img);
            img.data_extras
                .retain(|&i, _| !matches!(slot(i), Slot::Contacts(_)));
        }
        Ok(img)
    }

    pub fn export(img: &Image) -> Vec<u8> {
        let contact_pages: BTreeMap<usize, Page> = if img.contacts.is_empty() {
            BTreeMap::new()
        } else {
            crate::records::contact::to_data_file_pages(&crate::records::contact::decode(img))
                .into_iter()
                .collect()
        };
        let mut out = Vec::with_capacity(PAGES * PAGE);
        for i in 0..PAGES {
            let mut p = blank_page(0);
            match slot(i) {
                Slot::Header => {
                    if let Some(h) = img.data_extras.get(&i) {
                        p.copy_from_slice(&h[..]);
                    }
                    p[0x30..0x50].fill(0);
                    let fw = img.info.firmware.as_bytes();
                    p[0x30..0x30 + fw.len().min(0x20)].copy_from_slice(&fw[..fw.len().min(0x20)]);
                }
                Slot::Block(id) => match img.blocks.get(&id) {
                    Some(b) => p.copy_from_slice(&b[..]),
                    None => p[PAGE - 1] = SENTINEL_ID,
                },
                Slot::BlockNoId(id) => match img.blocks.get(&id) {
                    Some(b) => {
                        p.copy_from_slice(&b[..]);
                        p[PAGE - 1] = 0xFF;
                    }
                    None => p[PAGE - 1] = SENTINEL_ID,
                },
                Slot::VFrame(id) => {
                    if let Some(v) = img.info.vframe(id) {
                        p[..v.len().min(PAGE)].copy_from_slice(&v[..v.len().min(PAGE)]);
                    }
                }
                Slot::Contacts(_) if !contact_pages.is_empty() => {
                    if let Some(c) = contact_pages.get(&i) {
                        p.copy_from_slice(&c[..]);
                    }
                }
                Slot::Contacts(_) | Slot::Unknown => {
                    if let Some(x) = img.data_extras.get(&i) {
                        p.copy_from_slice(&x[..]);
                    } else if i == 26 {
                        p.fill(0xFF);
                    }
                }
            }
            out.extend_from_slice(&p[..]);
        }
        out
    }
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .filter_map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok())
        .collect()
}
