//! Reading and writing a whole codeplug over a [`Session`].

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

use dm32_proto::{Error, PAGE as PAGE_U32, Range, Result, Session, Transport};

use crate::image::{Image, ImageInfo, PAGE, Page, block, hex, page_from};

pub use dm32_proto::SENTINEL;

/// Records per contact database page, and the record size.
pub const CONTACTS_PER_PAGE: usize = 44;
pub const CONTACT_SIZE: usize = 92;

#[derive(Debug, Clone, Copy, Default)]
pub struct ReadOptions {
    /// Also read the DMR contact database (can take several minutes when full).
    pub contacts: bool,
    /// Issue the same requests as the official CPS, including reads of the sentinel address.
    pub oem_compat: bool,
}

/// Progress callback payload.
#[derive(Debug, Clone)]
pub enum Progress {
    Stage(String),
    Step { done: usize, total: usize },
}

pub struct Ctx<'a> {
    pub progress: &'a mut dyn FnMut(Progress),
    pub cancel: &'a AtomicBool,
}

impl Ctx<'_> {
    fn stage(&mut self, s: impl Into<String>) -> Result<()> {
        self.check()?;
        (self.progress)(Progress::Stage(s.into()));
        Ok(())
    }

    fn step(&mut self, done: usize, total: usize) -> Result<()> {
        self.check()?;
        (self.progress)(Progress::Step { done, total });
        Ok(())
    }

    fn check(&self) -> Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }
}

fn image_info(info: &dm32_proto::RadioInfo) -> ImageInfo {
    ImageInfo {
        model: info.model.clone(),
        firmware: info.firmware.clone(),
        build_date: info.build_date.clone(),
        codeplug_version: info.codeplug_version.clone(),
        contacts_base: info.contacts.map(|r| r.start),
        vframes: info.vframes.iter().map(|(&k, v)| (k, hex(v))).collect(),
    }
}

/// Logical block ID → physical page address, from a scan of the config region.
pub type BlockMap = BTreeMap<u8, u32>;

fn block_map(ids: &[(u32, u8)]) -> BlockMap {
    let mut map = BlockMap::new();
    for &(addr, id) in ids {
        if id != 0x00 && id != 0xFF {
            map.entry(id).or_insert(addr);
        }
    }
    map
}

fn scan<T: Transport>(s: &mut Session<T>, cx: &mut Ctx) -> Result<BlockMap> {
    let config = s
        .info()
        .and_then(|i| i.config)
        .ok_or_else(|| Error::Protocol("no config range".into()))?;
    cx.stage("Scanning memory map")?;
    let mut err = None;
    let ids = s.scan_block_ids(config, |d, t| {
        if err.is_none() {
            err = cx.step(d, t).err();
        }
    })?;
    if let Some(e) = err {
        return Err(e);
    }
    Ok(block_map(&ids))
}

fn contact_pages(count: u32) -> u32 {
    (count as usize).div_ceil(CONTACTS_PER_PAGE).max(1) as u32
}

fn contacts_range<T: Transport>(s: &Session<T>) -> Option<Range> {
    s.info().and_then(|i| i.contacts)
}

/// Read the whole codeplug.
pub fn read<T: Transport>(s: &mut Session<T>, opts: ReadOptions, cx: &mut Ctx) -> Result<Image> {
    s.set_oem_compat(opts.oem_compat);
    cx.stage("Identifying radio")?;
    let info = s.identify()?.clone();
    s.enter_program()?;
    let contacts = contacts_range(s);

    // The CPS reads the contact count first; it tells how many contact pages are in use.
    let count = match contacts {
        Some(r) => u32::from_le_bytes(s.read(r.start, 4)?.try_into().unwrap()),
        None => 0,
    };

    let map = scan(s, cx)?;
    let mut img = Image {
        info: image_info(&info),
        ..Default::default()
    };

    cx.stage("Reading codeplug")?;
    let walk = block::read_walk();
    for (i, &id) in walk.iter().enumerate() {
        cx.step(i, walk.len())?;
        match map.get(&id) {
            Some(&addr) => {
                let data = s.read_page(addr)?;
                img.blocks.insert(id, page_from(&data));
            }
            None if opts.oem_compat => {
                s.read_page(SENTINEL)?;
            }
            None => {}
        }
    }

    if let Some(r) = contacts {
        let pages = if opts.contacts {
            contact_pages(count)
        } else {
            1
        };
        cx.stage(format!("Reading contacts ({count})"))?;
        for n in 0..pages {
            cx.step(n as usize, pages as usize)?;
            let data = s.read_page(r.start + n * PAGE_U32)?;
            img.contacts.insert(n * PAGE_U32, page_from(&data));
        }
    }
    img.info.vframes = s
        .info()
        .map(|i| i.vframes.iter().map(|(&k, v)| (k, hex(v))).collect())
        .unwrap_or_default();
    Ok(img)
}

/// One page to be written.
#[derive(Debug, Clone)]
pub struct PageWrite {
    pub addr: u32,
    /// Logical block ID, or `None` for a contact page.
    pub block: Option<u8>,
    pub data: Page,
    pub what: String,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct WriteOptions {
    /// Write every codeplug block, not only the ones that differ from the radio.
    pub all_blocks: bool,
    /// Never write the radio settings block (0x04).
    pub skip_settings: bool,
    /// Write contact database pages.
    pub contacts: bool,
    /// Read back every written page and compare.
    pub verify: bool,
}

/// Work out which pages to write, given what is on the radio now.
pub fn plan(
    current: &Image,
    map: &BlockMap,
    target: &Image,
    opts: WriteOptions,
    contacts_base: Option<u32>,
) -> Vec<PageWrite> {
    let mut out = Vec::new();

    if opts.contacts
        && let Some(base) = contacts_base
    {
        for (&off, page) in &target.contacts {
            if opts.all_blocks || current.contacts.get(&off) != Some(page) {
                out.push(PageWrite {
                    addr: base + off,
                    block: None,
                    data: page.clone(),
                    what: format!("contacts page {}", off / PAGE_U32 + 1),
                });
            }
        }
    }

    for id in block::write_walk() {
        if opts.skip_settings && id == block::SETTINGS {
            continue;
        }
        let Some(page) = target.blocks.get(&id) else {
            continue;
        };
        let mut data = page.clone();
        data[PAGE - 1] = id;
        let unchanged = current.blocks.get(&id).is_some_and(|c| c[..] == data[..]);
        if unchanged && !opts.all_blocks {
            continue;
        }
        let addr = map.get(&id).copied().unwrap_or(SENTINEL);
        out.push(PageWrite {
            addr,
            block: Some(id),
            data,
            what: block::name(id),
        });
    }
    out
}

/// The radio's current state and the pages that would be written.
#[derive(Debug, Clone)]
pub struct Prepared {
    /// What is on the radio now. Save it as a backup before calling [`execute`].
    pub before: Image,
    pub map: BlockMap,
    pub writes: Vec<PageWrite>,
}

/// Result of a write.
#[derive(Debug, Clone, Default)]
pub struct WriteReport {
    pub written: Vec<String>,
    /// Pages whose read-back differed: `(what, first differing offset)`.
    pub mismatches: Vec<(String, usize)>,
}

/// First half of a write: connect, read what is on the radio and plan the page writes.
/// Nothing is written.
pub fn prepare<T: Transport>(
    s: &mut Session<T>,
    target: &Image,
    opts: WriteOptions,
    cx: &mut Ctx,
) -> Result<Prepared> {
    cx.stage("Identifying radio")?;
    let info = s.identify()?.clone();
    s.enter_program()?;
    let contacts = contacts_range(s);
    let count = match contacts {
        Some(r) => u32::from_le_bytes(s.read(r.start, 4)?.try_into().unwrap()),
        None => 0,
    };
    let map = scan(s, cx)?;

    cx.stage("Reading current codeplug")?;
    let mut before = Image {
        info: image_info(&info),
        ..Default::default()
    };
    let walk = block::read_walk();
    for (i, &id) in walk.iter().enumerate() {
        cx.step(i, walk.len())?;
        if let Some(&addr) = map.get(&id) {
            before.blocks.insert(id, page_from(&s.read_page(addr)?));
        }
    }
    if opts.contacts
        && let Some(r) = contacts
    {
        let pages = contact_pages(count).max(target.contacts.len() as u32);
        for n in 0..pages {
            before.contacts.insert(
                n * PAGE_U32,
                page_from(&s.read_page(r.start + n * PAGE_U32)?),
            );
        }
    }

    let writes = plan(&before, &map, target, opts, contacts.map(|r| r.start));
    Ok(Prepared {
        before,
        map,
        writes,
    })
}

/// Second half of a write: write the planned pages, then optionally read them back.
pub fn execute<T: Transport>(
    s: &mut Session<T>,
    prepared: &Prepared,
    opts: WriteOptions,
    cx: &mut Ctx,
) -> Result<WriteReport> {
    let writes = &prepared.writes;
    let mut report = WriteReport::default();

    cx.stage(format!("Writing {} pages", writes.len()))?;
    for (i, w) in writes.iter().enumerate() {
        cx.step(i, writes.len())?;
        s.write(w.addr, &w.data[..])?;
        report.written.push(w.what.clone());
    }

    if opts.verify && !writes.is_empty() {
        cx.stage("Verifying")?;
        // Blocks written to the sentinel have been given a page by the radio: rescan.
        let new_map = scan(s, cx)?;
        for (i, w) in writes.iter().enumerate() {
            cx.step(i, writes.len())?;
            let addr = match w.block {
                Some(id) => match new_map.get(&id) {
                    Some(&a) => a,
                    None => {
                        report
                            .mismatches
                            .push((format!("{} (no page allocated)", w.what), 0));
                        continue;
                    }
                },
                None => w.addr,
            };
            let back = s.read_page(addr)?;
            if let Some(off) = back.iter().zip(w.data.iter()).position(|(a, b)| a != b) {
                report.mismatches.push((w.what.clone(), off));
            }
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Codeplug;
    use crate::records::channel::{ChannelEntry, ChannelMode};

    fn fixture() -> Image {
        let bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/factory-v1.60.data"
        ))
        .unwrap();
        crate::image::data_file::import(&bytes).unwrap()
    }

    fn map_of(img: &Image) -> BlockMap {
        img.blocks
            .keys()
            .enumerate()
            .map(|(i, &id)| (id, 0x1000 * (i as u32 + 1)))
            .collect()
    }

    #[test]
    fn plan_writes_only_changes() {
        let cur = fixture();
        let map = map_of(&cur);
        let opts = WriteOptions::default();
        assert!(plan(&cur, &map, &cur, opts, Some(0x278000)).is_empty());

        let mut target = cur.clone();
        let mut cp = Codeplug::decode(&target);
        cp.channels.list[0].channel.set_name("Changed");
        cp.encode(&mut target);
        let p = plan(&cur, &map, &target, opts, Some(0x278000));
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].block, Some(0x12));
        assert_eq!(p[0].addr, map[&0x12]);
        assert_eq!(p[0].data[PAGE - 1], 0x12);
    }

    #[test]
    fn new_blocks_go_to_the_sentinel() {
        let mut cur = fixture();
        // Pretend the second channel slot has no page on the radio.
        cur.blocks.remove(&0x13);
        let map = map_of(&cur);
        let mut target = cur.clone();
        let mut cp = Codeplug::decode(&target);
        while cp.channels.list.len() < 100 {
            cp.channels.list.push(ChannelEntry::new(
                "Extra",
                crate::Freq(14_500_000),
                ChannelMode::Analog,
            ));
        }
        cp.encode(&mut target);
        let p = plan(&cur, &map, &target, WriteOptions::default(), None);
        let w13 = p
            .iter()
            .find(|w| w.block == Some(0x13))
            .expect("slot 2 written");
        assert_eq!(w13.addr, SENTINEL);
        assert_eq!(w13.data[PAGE - 1], 0x13);
    }

    #[test]
    fn skip_settings_and_never_calibration() {
        let cur = fixture();
        let map = map_of(&cur);
        let mut target = cur.clone();
        target.block_mut(block::SETTINGS)[0x100] ^= 0xFF;
        target.block_mut(block::CALIBRATION)[0x10] ^= 0xFF;
        let p = plan(&cur, &map, &target, WriteOptions::default(), None);
        assert_eq!(
            p.iter().map(|w| w.block).collect::<Vec<_>>(),
            vec![Some(block::SETTINGS)]
        );
        let p = plan(
            &cur,
            &map,
            &target,
            WriteOptions {
                skip_settings: true,
                ..Default::default()
            },
            None,
        );
        assert!(p.is_empty());
    }
}
