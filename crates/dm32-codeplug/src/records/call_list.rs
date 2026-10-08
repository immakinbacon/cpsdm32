//! The "Call" list (block `0x03`). Purpose unknown.
//!
//! The OEM CPS reads and writes this block. Entries are 40 bytes from `0x218`, the only
//! UTF-16LE names in the codeplug (`"Call 1"`..`"Call 5"` in the factory image). The factory
//! image shows 32 slots (`0x218-0x717`, each `FF`×8 + 32 zero bytes when unused; `0x718` onward
//! is different data); the spec's 92 is just "to the end of the block". Everything outside the
//! entry array (`0x001` count-like byte, `0x030` header, `0x730`+ tables) is unknown and
//! untouched.

use crate::encoding::{get_u16, get_utf16, put_u16, put_utf16};
use crate::image::{Image, block};
use crate::raw_record;

const BLOCK: u8 = block::CALL_LIST;
const BASE: usize = 0x218;

pub const SLOTS: usize = 32;

/// One entry (40 bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallEntry {
    raw: [u8; 40],
}

raw_record!(CallEntry, 40, 0xFF);

impl CallEntry {
    /// Name length in characters (32 bytes of UTF-16LE).
    pub const NAME_LEN: usize = 16;

    /// `+0x00`: `FE FF` on populated entries, `FF FF` on empty ones (the factory "Call 1" has
    /// `FF FF` despite its name).
    pub fn flags(&self) -> u16 {
        get_u16(&self.raw[0x00..])
    }

    pub fn set_flags(&mut self, v: u16) {
        put_u16(&mut self.raw[0x00..], v);
    }

    /// `+0x02`: reference A, referent unknown.
    pub fn ref_a(&self) -> u16 {
        get_u16(&self.raw[0x02..])
    }

    pub fn set_ref_a(&mut self, v: u16) {
        put_u16(&mut self.raw[0x02..], v);
    }

    /// `+0x04`: reference B, referent unknown.
    pub fn ref_b(&self) -> u16 {
        get_u16(&self.raw[0x04..])
    }

    pub fn set_ref_b(&mut self, v: u16) {
        put_u16(&mut self.raw[0x04..], v);
    }

    pub fn name(&self) -> String {
        get_utf16(&self.raw[0x08..0x28])
    }

    /// Set the name (UTF-16LE, zero-padded, at most 16 characters).
    pub fn set_name(&mut self, name: &str) {
        put_utf16(&mut self.raw[0x08..0x28], name);
    }
}

/// The entry array, positional.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CallList {
    pub entries: Vec<CallEntry>,
}

fn entry(i: usize) -> usize {
    BASE + i * CallEntry::SIZE
}

/// All 32 slots, or none if the block is absent.
pub fn decode(img: &Image) -> CallList {
    let Some(b) = img.block(BLOCK) else {
        return CallList::default();
    };
    CallList {
        entries: (0..SLOTS)
            .map(|i| CallEntry::from_bytes(&b[entry(i)..]))
            .collect(),
    }
}

/// Write the slots back (at most 32; slots beyond `entries` are left as they are).
pub fn encode(c: &CallList, img: &mut Image) {
    if img.block(BLOCK).is_none() && c.entries.is_empty() {
        return;
    }
    let b = img.block_mut(BLOCK);
    for (i, e) in c.entries.iter().take(SLOTS).enumerate() {
        b[entry(i)..entry(i + 1)].copy_from_slice(e.bytes());
    }
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
        let c = decode(&factory());
        assert_eq!(c.entries.len(), SLOTS);
        let names: Vec<String> = c.entries.iter().map(|e| e.name()).collect();
        assert_eq!(
            names[..6],
            ["Call 1", "Call 2", "Call 3", "Call 4", "Call 5", ""]
        );
        assert!(names[6..].iter().all(|n| n.is_empty()));
        let e = &c.entries[1];
        assert_eq!((e.flags(), e.ref_a(), e.ref_b()), (0xFFFE, 0x0C91, 0x2441));
        assert_eq!(c.entries[0].flags(), 0xFFFF);
        assert_eq!(c.entries[5].ref_b(), 0x4FD6);
    }

    #[test]
    fn round_trip() {
        let img = factory();
        let mut out = img.clone();
        encode(&decode(&img), &mut out);
        assert_eq!(out, img);
    }

    #[test]
    fn rename() {
        let img = factory();
        let mut c = decode(&img);
        c.entries[2].set_name("Ruf");
        let mut out = img.clone();
        encode(&c, &mut out);
        let (a, b) = (img.block(BLOCK).unwrap(), out.block(BLOCK).unwrap());
        let diff: Vec<usize> = (0..0x1000).filter(|&i| a[i] != b[i]).collect();
        // "Call 3" -> "Ruf": units 0-2 change, units 3-5 become NUL (low bytes only).
        assert_eq!(diff, [0x270, 0x272, 0x274, 0x276, 0x278, 0x27A]);
        assert_eq!(decode(&out).entries[2].name(), "Ruf");
    }
}
