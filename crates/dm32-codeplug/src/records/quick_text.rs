//! Quick text messages (block `0x0A`).
//!
//! Layout (hardware-confirmed): count byte at `0x000`, header `0x001-0x00F` unknown, then up to
//! 20 entries of 129 bytes from `0x010` (a character-count byte and a 128-byte text field).
//! `0x0A24-0x0FFE` is unknown and preserved.

use crate::encoding::{get_str_ff, is_blank, put_str};
use crate::image::{Image, block};
use crate::raw_record;

const BLOCK: u8 = block::QUICK_TEXT;
const COUNT: usize = 0x000;
const BASE: usize = 0x010;
/// Cleared slots are written `0xFF`, as the OEM CPS and the reference implementation do.
const CLEAR: u8 = 0xFF;

/// Maximum number of messages.
pub const MAX: usize = 20;

/// One message (129 bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickText {
    raw: [u8; 129],
}

raw_record!(QuickText, 129, 0x00);

impl QuickText {
    /// Longest text. The last byte of the 128-byte field is always a NUL (the OEM CPS writes
    /// one there; qdmr limits texts to 126 characters, the reference implementation to 127).
    pub const TEXT_LEN: usize = 127;

    pub fn new(text: &str) -> Self {
        let mut q = QuickText::default();
        q.set_text(text);
        q
    }

    /// The message. Ends at the first `0x00` or `0xFF` (both fill conventions are in use).
    pub fn text(&self) -> String {
        get_str_ff(&self.raw[0x01..0x81])
    }

    /// Set the text and the character-count byte. Padded with `0x00` like the radio does.
    pub fn set_text(&mut self, text: &str) {
        put_str(&mut self.raw[0x01..0x80], text, Self::TEXT_LEN, 0x00);
        self.raw[0x80] = 0;
        self.raw[0x00] = self.text().len() as u8;
    }

    /// The stored character count (`+0x00`).
    pub fn length(&self) -> u8 {
        self.raw[0x00]
    }

    pub fn is_empty(&self) -> bool {
        is_blank(&self.raw[0x01..])
    }
}

/// The message list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuickTexts {
    pub messages: Vec<QuickText>,
}

fn entry(i: usize) -> usize {
    BASE + i * QuickText::SIZE
}

pub fn decode(img: &Image) -> QuickTexts {
    let Some(b) = img.block(BLOCK) else {
        return QuickTexts::default();
    };
    let n = (b[COUNT] as usize).min(MAX);
    QuickTexts {
        messages: (0..n)
            .map(|i| QuickText::from_bytes(&b[entry(i)..]))
            .collect(),
    }
}

/// Write the messages (truncated to [`MAX`]) and the count; slots no longer used are cleared
/// to `0xFF`.
pub fn encode(t: &QuickTexts, img: &mut Image) {
    if img.block(BLOCK).is_none() && t.messages.is_empty() {
        return;
    }
    let b = img.block_mut(BLOCK);
    let old = (b[COUNT] as usize).min(MAX);
    let n = t.messages.len().min(MAX);
    for (i, m) in t.messages.iter().take(n).enumerate() {
        b[entry(i)..entry(i + 1)].copy_from_slice(m.bytes());
    }
    for i in n..old {
        b[entry(i)..entry(i + 1)].fill(CLEAR);
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
        let t = decode(&factory());
        let texts: Vec<String> = t.messages.iter().map(|m| m.text()).collect();
        assert_eq!(
            texts,
            [
                "How are you?",
                "Nice to meet you",
                "How have you been",
                "Are you making progress?",
                "How do you feel today?"
            ]
        );
        for m in &t.messages {
            assert_eq!(m.length() as usize, m.text().len());
        }
    }

    #[test]
    fn round_trip() {
        let img = factory();
        let mut out = img.clone();
        encode(&decode(&img), &mut out);
        assert_eq!(out.block(BLOCK), img.block(BLOCK));
    }

    #[test]
    fn set_text_matches_radio_bytes() {
        let img = factory();
        let mut t = decode(&img);
        // Rewriting the same text reproduces the factory bytes exactly.
        t.messages[0].set_text("How are you?");
        let mut out = img.clone();
        encode(&t, &mut out);
        assert_eq!(out.block(BLOCK), img.block(BLOCK));

        t.messages[0].set_text("Hi");
        encode(&t, &mut out);
        let (a, b) = (img.block(BLOCK).unwrap(), out.block(BLOCK).unwrap());
        let diff: Vec<usize> = (0..0x1000).filter(|&i| a[i] != b[i]).collect();
        // Count byte, then "ow are you?" -> "i" + NULs ('H' is unchanged).
        let want: Vec<usize> = [0x10].into_iter().chain(0x12..=0x1C).collect();
        assert_eq!(diff, want);
        assert_eq!(b[0x10], 2);
        assert_eq!(decode(&out).messages[0].text(), "Hi");
    }

    #[test]
    fn long_text() {
        let s = "x".repeat(200);
        let q = QuickText::new(&s);
        assert_eq!(q.text().len(), 127);
        assert_eq!(q.length(), 127);
        assert_eq!(q.bytes()[0x80], 0);
    }

    #[test]
    fn add_remove() {
        let img = factory();
        let mut t = decode(&img);
        t.messages.push(QuickText::new("73"));
        let mut out = img.clone();
        encode(&t, &mut out);
        assert_eq!(out.block(BLOCK).unwrap()[0], 6);
        assert_eq!(decode(&out).messages[5].text(), "73");

        t.messages.truncate(1);
        encode(&t, &mut out);
        let b = out.block(BLOCK).unwrap();
        assert_eq!(b[0], 1);
        assert!(b[entry(1)..entry(6)].iter().all(|&c| c == 0xFF));
        assert!(b[entry(6)..0xFFF].iter().all(|&c| c == 0));
        assert_eq!(decode(&out).messages.len(), 1);
    }
}
