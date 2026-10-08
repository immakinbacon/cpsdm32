//! Block `0x10`: digital emergency systems, analog emergency systems and encryption keys.
//!
//! | Offset  | Content                                              |
//! |---------|------------------------------------------------------|
//! | `0x000` | 8 digital emergency systems, 20 bytes (confirmed)   |
//! | `0x0A0` | unknown gap                                          |
//! | `0x0AC` | 16 analog emergency systems, 36 bytes (⚠️ base derived) |
//! | `0x300` | 4 encryption keys, 44 bytes (confirmed)             |
//! | `0x3B0` | unknown (not empty: `0A 0A` runs at `0xB00`)          |
//!
//! All three tables are fixed slot arrays, written back positionally; everything else in the
//! block is left untouched.
//!
//! The key count is unconfirmed: qdmr allows 32 (`0x300-0x87F`), the spec says 8, and the
//! factory codeplug fills exactly 4 (slot 5 starts `00 FF FF ..`). 4 is used here so the editor
//! never offers a key the radio might ignore; `0x3B0` onward is never touched, so keys stored
//! beyond slot 4 by another CPS survive unchanged.

use crate::encoding::{get_str, get_u16, is_blank, put_str, put_u16, set_bits};
use crate::image::{Image, block};
use crate::{choice, raw_record};

const BLOCK: u8 = block::EMERGENCY;
const DIGITAL_BASE: usize = 0x000;
const ANALOG_BASE: usize = 0x0AC;
const KEY_BASE: usize = 0x300;

pub const DIGITAL_SLOTS: usize = 8;
pub const ANALOG_SLOTS: usize = 16;
pub const KEY_SLOTS: usize = 4;

choice! {
    /// Digital emergency alarm type (`+0x0A`).
    pub enum AlarmType {
        None = 0 => "None",
        OnlyWhistle = 1 => "Only Whistle",
        Normal = 2 => "Normal",
        Secret = 3 => "Secret",
        SecretWithVoice = 4 => "Secret With Voice",
        AlarmWhistle = 5 => "Alarm Whistle",
    }
}

choice! {
    /// Digital emergency alarm mode (`+0x0B`). The CPS list (`[DigitalEmerMode]`) is 0-based;
    /// the stored value is option + 1 (spec; the factory image holds only 1 and 3).
    pub enum AlarmMode {
        EmergencyAlarm = 1 => "Emergency Alarm",
        AlarmCall = 2 => "Alarm Call",
        EmergencyCall = 3 => "Emergency Call",
    }
}

choice! {
    /// Encryption key type (`+0x0B`).
    pub enum KeyType {
        None = 0 => "None",
        Custom = 1 => "Custom",
        Arc4 = 2 => "ARC4",
        Aes128 = 3 => "AES128",
        Aes256 = 4 => "AES256",
    }
}

impl KeyType {
    /// Key length in bytes. ARC4/AES lengths per qdmr; ⚠️ Custom = 7 is inferred from the one
    /// factory sample (`DE FA BC 12 34 56 78`).
    pub fn key_len(self) -> usize {
        match self {
            KeyType::None => 0,
            KeyType::Custom => 7,
            KeyType::Arc4 => 5,
            KeyType::Aes128 => 16,
            KeyType::Aes256 | KeyType::Other(_) => 32,
        }
    }
}

/// A digital emergency system (20 bytes). Layout hardware-confirmed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigitalEmergency {
    raw: [u8; 20],
}

raw_record!(DigitalEmergency, 20, 0x00);

/// `(raw + 1) * 10`, raw 0-11.
fn tens_get(raw: u8) -> u16 {
    (raw as u16 + 1) * 10
}

fn tens_put(v: u16) -> u8 {
    (v.clamp(10, 120) / 10 - 1) as u8
}

impl DigitalEmergency {
    pub const NAME_LEN: usize = 10;

    pub fn name(&self) -> String {
        get_str(&self.raw[0x00..0x0A])
    }

    pub fn set_name(&mut self, name: &str) {
        put_str(&mut self.raw[0x00..0x0A], name, Self::NAME_LEN, 0x00);
    }

    pub fn alarm_type(&self) -> AlarmType {
        AlarmType::from_raw(self.raw[0x0A])
    }

    pub fn set_alarm_type(&mut self, v: AlarmType) {
        self.raw[0x0A] = v.to_raw();
    }

    pub fn alarm_mode(&self) -> AlarmMode {
        AlarmMode::from_raw(self.raw[0x0B])
    }

    pub fn set_alarm_mode(&mut self, v: AlarmMode) {
        self.raw[0x0B] = v.to_raw();
    }

    /// Revert channel (uint16 LE; 0 = None).
    pub fn revert_channel(&self) -> u16 {
        get_u16(&self.raw[0x0C..])
    }

    pub fn set_revert_channel(&mut self, v: u16) {
        put_u16(&mut self.raw[0x0C..], v);
    }

    /// Retransmission count, 1-15.
    pub fn retransmissions(&self) -> u8 {
        self.raw[0x0E]
    }

    pub fn set_retransmissions(&mut self, v: u8) {
        self.raw[0x0E] = v;
    }

    /// HOT MIC duration in seconds, 1-15.
    pub fn hot_mic_duration(&self) -> u8 {
        self.raw[0x0F]
    }

    pub fn set_hot_mic_duration(&mut self, v: u8) {
        self.raw[0x0F] = v;
    }

    /// Emergency calls number, 10-120 in steps of 10.
    pub fn emergency_calls(&self) -> u16 {
        tens_get(self.raw[0x10])
    }

    pub fn set_emergency_calls(&mut self, v: u16) {
        self.raw[0x10] = tens_put(v);
    }

    /// Bit 0 of `+0x11`. Bits 7-1 carry unknown data on hardware and are preserved.
    pub fn enabled(&self) -> bool {
        self.raw[0x11] & 1 != 0
    }

    pub fn set_enabled(&mut self, on: bool) {
        set_bits(&mut self.raw[0x11], 0, 1, on as u8);
    }

    /// RX duration time in seconds, 1-255.
    pub fn rx_duration(&self) -> u8 {
        self.raw[0x12]
    }

    pub fn set_rx_duration(&mut self, v: u8) {
        self.raw[0x12] = v;
    }

    /// Auto emergency call timer, 10-120 in steps of 10.
    pub fn auto_call_timer(&self) -> u16 {
        tens_get(self.raw[0x13])
    }

    pub fn set_auto_call_timer(&mut self, v: u16) {
        self.raw[0x13] = tens_put(v);
    }

    pub fn is_empty(&self) -> bool {
        is_blank(&self.raw)
    }
}

/// An analog emergency system (36 bytes).
///
/// ⚠️ Only the entry size is confirmed (the OEM write capture has `"AEMER 1"` at `0x0D0` = slot
/// 1). The spec's intra-entry field table does not decode that entry under any alignment, so
/// only the name (at `+0x00`, as in the capture) is exposed; the rest is raw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalogEmergency {
    raw: [u8; 36],
}

raw_record!(AnalogEmergency, 36, 0x00);

impl AnalogEmergency {
    pub const NAME_LEN: usize = 16;

    pub fn name(&self) -> String {
        get_str(&self.raw[0x00..0x10])
    }

    pub fn set_name(&mut self, name: &str) {
        put_str(&mut self.raw[0x00..0x10], name, Self::NAME_LEN, 0x00);
    }

    pub fn is_empty(&self) -> bool {
        is_blank(&self.raw)
    }
}

/// An encryption key (44 bytes). Layout hardware-confirmed.
///
/// The key field is 32 bytes at `+0x0C`; the key occupies its first [`KeyType::key_len`] bytes.
/// The CPS treats keys as hex numbers: short AES keys are zero-padded on the left (the factory
/// AES256 key `ABCDEF1234567890` sits in the last 8 bytes of 32). qdmr reads the same bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptionKey {
    raw: [u8; 44],
}

raw_record!(EncryptionKey, 44, 0x00);

impl EncryptionKey {
    pub const NAME_LEN: usize = 10;

    pub fn new(name: &str, kind: KeyType, key: &[u8]) -> Self {
        let mut k = EncryptionKey::default();
        k.set_name(name);
        k.set_key_type(kind);
        k.set_key(key);
        k
    }

    /// Key ID (`+0x00`, CPS "Encryption ID"): the 1-based slot number for a populated key, 0 when unused.
    /// [`encode`] maintains it.
    pub fn key_id(&self) -> u8 {
        self.raw[0x00]
    }

    pub fn name(&self) -> String {
        get_str(&self.raw[0x01..0x0B])
    }

    pub fn set_name(&mut self, name: &str) {
        put_str(&mut self.raw[0x01..0x0B], name, Self::NAME_LEN, 0x00);
    }

    pub fn key_type(&self) -> KeyType {
        KeyType::from_raw(self.raw[0x0B])
    }

    /// Change the type only; call [`set_key`](Self::set_key) to re-place the key bytes.
    pub fn set_key_type(&mut self, t: KeyType) {
        self.raw[0x0B] = t.to_raw();
    }

    /// The key bytes for the current type.
    pub fn key(&self) -> &[u8] {
        &self.raw[0x0C..0x0C + self.key_type().key_len()]
    }

    /// The whole 32-byte key field.
    pub fn key_field(&self) -> &[u8] {
        &self.raw[0x0C..0x2C]
    }

    /// Store a key for the current type (truncated to its length). A short AES key is
    /// right-aligned (left zero padding), others are left-aligned; the rest of the field is
    /// zeroed.
    pub fn set_key(&mut self, key: &[u8]) {
        // An erased (0xFF) slot has no valid type; give it one before sizing the key.
        if matches!(self.key_type(), KeyType::None | KeyType::Other(_)) {
            self.set_key_type(KeyType::Arc4);
        }
        let t = self.key_type();
        let len = t.key_len();
        let key = &key[..key.len().min(len)];
        let start = match t {
            KeyType::Aes128 | KeyType::Aes256 => len - key.len(),
            _ => 0,
        };
        let field = &mut self.raw[0x0C..0x2C];
        field.fill(0);
        field[start..start + key.len()].copy_from_slice(key);
    }

    /// The key as upper-case hex.
    pub fn key_hex(&self) -> String {
        self.key().iter().map(|b| format!("{b:02X}")).collect()
    }

    /// Set the key from hex digits (an odd count is left-padded with `0`).
    pub fn set_key_hex(&mut self, s: &str) -> Result<(), String> {
        let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        if !s.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!("invalid hex key '{s}'"));
        }
        if matches!(self.key_type(), KeyType::None | KeyType::Other(_)) {
            self.set_key_type(KeyType::Arc4);
        }
        let max = self.key_type().key_len() * 2;
        if s.len() > max {
            return Err(format!("key too long ({} hex digits, max {max})", s.len()));
        }
        let s = if s.len() % 2 == 1 { format!("0{s}") } else { s };
        let bytes: Vec<u8> = (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect();
        self.set_key(&bytes);
        Ok(())
    }

    /// Unused slot (bytes all `0x00`/`0xFF`; the factory image mixes both).
    pub fn is_empty(&self) -> bool {
        is_blank(&self.raw)
    }
}

/// Block `0x10`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Emergency {
    pub digital: [DigitalEmergency; DIGITAL_SLOTS],
    pub analog: [AnalogEmergency; ANALOG_SLOTS],
    pub keys: [EncryptionKey; KEY_SLOTS],
}

fn read<T, const N: usize>(b: &[u8], base: usize, size: usize, f: fn(&[u8]) -> T) -> [T; N] {
    std::array::from_fn(|i| f(&b[base + i * size..]))
}

pub fn decode(img: &Image) -> Emergency {
    let Some(b) = img.block(BLOCK) else {
        return Emergency::default();
    };
    Emergency {
        digital: read(
            &b[..],
            DIGITAL_BASE,
            DigitalEmergency::SIZE,
            DigitalEmergency::from_bytes,
        ),
        analog: read(
            &b[..],
            ANALOG_BASE,
            AnalogEmergency::SIZE,
            AnalogEmergency::from_bytes,
        ),
        keys: read(
            &b[..],
            KEY_BASE,
            EncryptionKey::SIZE,
            EncryptionKey::from_bytes,
        ),
    }
}

/// Write all three tables slot by slot. Key IDs are set to the slot number for populated keys.
pub fn encode(e: &Emergency, img: &mut Image) {
    if img.block(BLOCK).is_none() && *e == Emergency::default() {
        return;
    }
    let b = img.block_mut(BLOCK);
    for (i, r) in e.digital.iter().enumerate() {
        let o = DIGITAL_BASE + i * DigitalEmergency::SIZE;
        b[o..o + DigitalEmergency::SIZE].copy_from_slice(r.bytes());
    }
    for (i, r) in e.analog.iter().enumerate() {
        let o = ANALOG_BASE + i * AnalogEmergency::SIZE;
        b[o..o + AnalogEmergency::SIZE].copy_from_slice(r.bytes());
    }
    for (i, k) in e.keys.iter().enumerate() {
        let o = KEY_BASE + i * EncryptionKey::SIZE;
        b[o..o + EncryptionKey::SIZE].copy_from_slice(k.bytes());
        if !k.is_empty() {
            b[o] = i as u8 + 1;
        }
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

    fn diff(a: &Image, b: &Image) -> Vec<usize> {
        let (a, b) = (a.block(BLOCK).unwrap(), b.block(BLOCK).unwrap());
        (0..0x1000).filter(|&i| a[i] != b[i]).collect()
    }

    #[test]
    fn decode_digital() {
        let e = decode(&factory());
        for (i, d) in e.digital.iter().enumerate() {
            assert_eq!(d.name(), format!("DEmer {}", i + 1));
        }
        let types: Vec<u8> = e.digital.iter().map(|d| d.alarm_type().to_raw()).collect();
        assert_eq!(types, [1, 2, 3, 4, 5, 2, 4, 0]);
        let d = &e.digital[0];
        assert_eq!(d.alarm_type(), AlarmType::OnlyWhistle);
        assert_eq!(d.alarm_mode(), AlarmMode::EmergencyCall);
        assert_eq!(d.revert_channel(), 0);
        assert_eq!(d.retransmissions(), 5);
        assert_eq!(d.hot_mic_duration(), 5);
        assert_eq!(d.emergency_calls(), 10);
        assert!(!d.enabled());
        assert_eq!(d.rx_duration(), 1);
        assert_eq!(d.auto_call_timer(), 10);
        assert_eq!(e.digital[3].alarm_mode(), AlarmMode::EmergencyAlarm);
        // DEmer 3 carries 0xFF in its enable byte.
        assert!(e.digital[2].enabled());
        assert_eq!(e.digital[2].rx_duration(), 3);
        assert!(e.analog.iter().all(|a| a.name().is_empty()));
    }

    #[test]
    fn decode_keys() {
        let e = decode(&factory());
        let k = &e.keys;
        assert_eq!(k[0].name(), "Encrypt 1");
        assert_eq!(k[0].key_type(), KeyType::Aes256);
        assert_eq!(
            k[0].key_hex(),
            format!("{}ABCDEF1234567890", "0".repeat(48))
        );
        assert_eq!(k[1].key_type(), KeyType::Aes128);
        assert_eq!(k[1].key_hex(), "0000000000000000BCDEFA1234567890");
        assert_eq!(k[2].key_type(), KeyType::Arc4);
        assert_eq!(k[2].key_hex(), "CDEFAB1234");
        assert_eq!(k[3].key_type(), KeyType::Custom);
        assert_eq!(k[3].key_hex(), "DEFABC12345678");
        for (i, key) in k[..4].iter().enumerate() {
            assert_eq!(key.key_id() as usize, i + 1);
        }
        assert!(k[4..].iter().all(|k| k.is_empty()));
    }

    #[test]
    fn round_trip() {
        let img = factory();
        let mut out = img.clone();
        encode(&decode(&img), &mut out);
        assert_eq!(out, img);
    }

    #[test]
    fn edits_touch_only_their_bits() {
        let img = factory();
        let mut e = decode(&img);
        e.digital[1].set_enabled(true);
        e.digital[1].set_emergency_calls(50);
        let mut out = img.clone();
        encode(&e, &mut out);
        assert_eq!(diff(&img, &out), [0x24, 0x25]);
        let b = out.block(BLOCK).unwrap();
        assert_eq!((b[0x24], b[0x25]), (4, 1));

        let mut e = decode(&img);
        e.keys[2].set_key_hex("0102030405").unwrap();
        let mut out = img.clone();
        encode(&e, &mut out);
        assert_eq!(diff(&img, &out), (0x364..0x369).collect::<Vec<_>>());
    }

    #[test]
    fn add_key() {
        let img = factory();
        let mut e = decode(&img);
        e.keys[3] = EncryptionKey::new("Mine", KeyType::Aes128, &[0x11; 4]);
        let mut out = img.clone();
        encode(&e, &mut out);
        let k = &decode(&out).keys[3];
        assert_eq!(k.key_id(), 4);
        assert_eq!(k.key_hex(), format!("{}11111111", "0".repeat(24)));
        assert!(EncryptionKey::default().set_key_hex("zz").is_err());
        // Everything after the 4 key slots is untouched.
        assert_eq!(
            out.block(BLOCK).unwrap()[0x3B0..],
            img.block(BLOCK).unwrap()[0x3B0..]
        );
    }

    #[test]
    fn absent_block_not_created() {
        let mut img = Image::default();
        encode(&Emergency::default(), &mut img);
        assert!(img.block(BLOCK).is_none());
    }
}
