//! Field codecs shared by all record types.
//!
//! Numeric rule (spec 06-ENCODING): BCD digits are packed most-significant-first into a word and
//! the word is stored little-endian. Plain integers are binary little-endian.

use std::fmt;

/// A frequency in units of 10 Hz (the radio's resolution).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Freq(pub u32);

impl Freq {
    pub const MAX: Freq = Freq(99_999_999);

    pub fn from_mhz(mhz: f64) -> Option<Freq> {
        let v = (mhz * 100_000.0).round();
        (0.0..=Self::MAX.0 as f64)
            .contains(&v)
            .then_some(Freq(v as u32))
    }

    pub fn mhz(self) -> f64 {
        self.0 as f64 / 100_000.0
    }

    pub fn hz(self) -> u64 {
        self.0 as u64 * 10
    }
}

impl fmt::Display for Freq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{:05}", self.0 / 100_000, self.0 % 100_000)
    }
}

impl std::str::FromStr for Freq {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let s = s.trim().trim_end_matches("MHz").trim();
        let (int, frac) = s.split_once('.').unwrap_or((s, ""));
        if int.is_empty()
            || int.len() > 3
            || frac.len() > 5
            || !(int.bytes().chain(frac.bytes())).all(|b| b.is_ascii_digit())
        {
            return Err(format!(
                "invalid frequency '{s}' (expected MHz, e.g. 145.350)"
            ));
        }
        let frac = format!("{frac:0<5}");
        Ok(Freq(
            int.parse::<u32>().unwrap() * 100_000 + frac.parse::<u32>().unwrap(),
        ))
    }
}

fn bcd_digits(mut v: u32, n: usize) -> u32 {
    let mut out = 0;
    for i in 0..n {
        out |= (v % 10) << (4 * i);
        v /= 10;
    }
    out
}

fn from_bcd(mut w: u32, n: usize) -> Option<u32> {
    let mut v = 0;
    let mut mul = 1;
    for _ in 0..n {
        let d = w & 0xF;
        if d > 9 {
            return None;
        }
        v += d * mul;
        mul *= 10;
        w >>= 4;
    }
    Some(v)
}

/// Decode a 4-byte BCD frequency. `FF FF FF FF` (no TX) and invalid BCD give `None`.
pub fn get_freq(b: &[u8]) -> Option<Freq> {
    let w = u32::from_le_bytes(b[..4].try_into().unwrap());
    if w == 0xFFFF_FFFF {
        return None;
    }
    from_bcd(w, 8).map(Freq)
}

/// Encode a frequency; `None` writes the "no TX" sentinel `FF FF FF FF`.
pub fn put_freq(b: &mut [u8], f: Option<Freq>) {
    let w = match f {
        Some(f) => bcd_digits(f.0.min(Freq::MAX.0), 8),
        None => 0xFFFF_FFFF,
    };
    b[..4].copy_from_slice(&w.to_le_bytes());
}

/// A CTCSS tone or DCS code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Tone {
    #[default]
    Off,
    /// CTCSS tone in tenths of a Hz (e.g. 1273 = 127.3 Hz).
    Ctcss(u16),
    /// DCS code given as its octal digits read as decimal (e.g. 23 for D023), and inversion.
    Dcs { code: u16, inverted: bool },
}

/// The 50 standard CTCSS tones (tenths of a Hz).
pub const CTCSS_TONES: [u16; 50] = [
    670, 693, 719, 744, 770, 797, 825, 854, 885, 915, 948, 974, 1000, 1035, 1072, 1109, 1148, 1188,
    1230, 1273, 1318, 1365, 1413, 1462, 1500, 1567, 1598, 1622, 1655, 1679, 1713, 1738, 1773, 1799,
    1835, 1862, 1899, 1928, 1966, 1995, 2035, 2065, 2107, 2181, 2257, 2291, 2336, 2418, 2503, 2541,
];

/// The 104 standard DCS codes (octal digits written as decimal).
pub const DCS_CODES: [u16; 104] = [
    23, 25, 26, 31, 32, 36, 43, 47, 51, 53, 54, 65, 71, 72, 73, 74, 114, 115, 116, 122, 125, 131,
    132, 134, 143, 145, 152, 155, 156, 162, 165, 172, 174, 205, 212, 223, 225, 226, 243, 244, 245,
    246, 251, 252, 255, 261, 263, 265, 266, 271, 274, 306, 311, 315, 325, 331, 332, 343, 346, 351,
    356, 364, 365, 371, 411, 412, 413, 423, 431, 432, 445, 446, 452, 454, 455, 462, 464, 465, 466,
    503, 506, 516, 523, 526, 532, 546, 565, 606, 612, 624, 627, 631, 632, 654, 662, 664, 703, 712,
    723, 731, 732, 734, 743, 754,
];

impl fmt::Display for Tone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Tone::Off => f.write_str("Off"),
            Tone::Ctcss(t) => write!(f, "{}.{}", t / 10, t % 10),
            Tone::Dcs { code, inverted } => {
                write!(f, "D{code:03}{}", if inverted { 'I' } else { 'N' })
            }
        }
    }
}

impl std::str::FromStr for Tone {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let s = s.trim();
        if s.is_empty() || s.eq_ignore_ascii_case("off") || s.eq_ignore_ascii_case("none") {
            return Ok(Tone::Off);
        }
        let up = s.to_ascii_uppercase();
        if let Some(rest) = up.strip_prefix('D') {
            let (digits, inverted) = match rest.chars().last() {
                Some('N') => (&rest[..rest.len() - 1], false),
                Some('I') | Some('R') | Some('P') => (&rest[..rest.len() - 1], true),
                _ => (rest, false),
            };
            let code: u16 = digits
                .parse()
                .map_err(|_| format!("invalid DCS code '{s}'"))?;
            if code > 777 || digits.bytes().any(|b| b > b'7') {
                return Err(format!("invalid DCS code '{s}' (digits must be octal)"));
            }
            return Ok(Tone::Dcs { code, inverted });
        }
        let hz: f64 = s
            .trim_end_matches("Hz")
            .trim()
            .parse()
            .map_err(|_| format!("invalid tone '{s}'"))?;
        let t = (hz * 10.0).round();
        if !(1.0..=9999.0).contains(&t) {
            return Err(format!("CTCSS tone '{s}' out of range"));
        }
        Ok(Tone::Ctcss(t as u16))
    }
}

/// Decode a 2-byte CTCSS/DCS field. Both `FF FF` and `00 00` mean off.
pub fn get_tone(b: &[u8]) -> Tone {
    let (lo, hi) = (b[0], b[1]);
    if lo == 0xFF && hi == 0xFF {
        return Tone::Off;
    }
    if hi & 0x80 != 0 {
        let code = (hi & 0x0F) as u16 * 100 + (lo >> 4) as u16 * 10 + (lo & 0x0F) as u16;
        return Tone::Dcs {
            code,
            inverted: hi & 0x40 != 0,
        };
    }
    match from_bcd(u16::from_le_bytes([lo, hi]) as u32, 4) {
        Some(0) | None => Tone::Off,
        Some(t) => Tone::Ctcss(t as u16),
    }
}

/// Encode a tone. Off is written as `FF FF`, as the radio itself stores it.
pub fn put_tone(b: &mut [u8], t: Tone) {
    let w: u16 = match t {
        Tone::Off => 0xFFFF,
        Tone::Ctcss(t) => bcd_digits(t.min(9999) as u32, 4) as u16,
        Tone::Dcs { code, inverted } => {
            let d = bcd_digits(code.min(777) as u32, 3) as u16;
            d | if inverted { 0xC000 } else { 0x8000 }
        }
    };
    b[..2].copy_from_slice(&w.to_le_bytes());
}

/// Decode an ASCII string field: up to the first NUL (or the whole field), with `0xFF` and
/// other non-printing bytes dropped.
pub fn get_str(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    let s: String = b[..end]
        .iter()
        .filter(|&&c| (0x20..0x7F).contains(&c))
        .map(|&c| c as char)
        .collect();
    s.trim_end().to_string()
}

/// Like [`get_str`] but also stops at the first `0xFF` (talk groups, quick messages).
pub fn get_str_ff(b: &[u8]) -> String {
    let end = b
        .iter()
        .position(|&c| c == 0 || c == 0xFF)
        .unwrap_or(b.len());
    get_str(&b[..end])
}

/// Make a string safe for a field of `max` bytes: printable ASCII only, truncated.
pub fn sanitize(s: &str, max: usize) -> String {
    s.chars()
        .map(|c| if (' '..='~').contains(&c) { c } else { '?' })
        .take(max)
        .collect()
}

/// Encode an ASCII string into `b`. Up to `max` characters are written; if shorter than the
/// field a NUL terminator follows and the rest is filled with `pad`.
pub fn put_str(b: &mut [u8], s: &str, max: usize, pad: u8) {
    let s = sanitize(s, max.min(b.len()));
    let n = s.len();
    b[..n].copy_from_slice(s.as_bytes());
    if n < b.len() {
        b[n] = 0;
        b[n + 1..].fill(pad);
    }
}

/// Decode a UTF-16LE string field, up to the first NUL code unit.
pub fn get_utf16(b: &[u8]) -> String {
    let units: Vec<u16> = b
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&c| u16::from_le_bytes(c))
        .take_while(|&u| u != 0 && u != 0xFFFF)
        .collect();
    String::from_utf16_lossy(&units)
}

/// Encode a UTF-16LE string field, NUL-terminated if it fits, padded with zeros.
pub fn put_utf16(b: &mut [u8], s: &str) {
    b.fill(0);
    for (i, u) in s.encode_utf16().take(b.len() / 2).enumerate() {
        b[2 * i..2 * i + 2].copy_from_slice(&u.to_le_bytes());
    }
}

pub fn get_u16(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}

pub fn put_u16(b: &mut [u8], v: u16) {
    b[..2].copy_from_slice(&v.to_le_bytes());
}

pub fn get_u24(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], 0])
}

pub fn put_u24(b: &mut [u8], v: u32) {
    b[..3].copy_from_slice(&v.to_le_bytes()[..3]);
}

pub fn get_u32(b: &[u8]) -> u32 {
    u32::from_le_bytes(b[..4].try_into().unwrap())
}

pub fn put_u32(b: &mut [u8], v: u32) {
    b[..4].copy_from_slice(&v.to_le_bytes());
}

/// Read a bit field `width` bits wide starting at bit `shift` of a byte.
pub fn get_bits(byte: u8, shift: u8, width: u8) -> u8 {
    (byte >> shift) & ((1u16 << width) - 1) as u8
}

/// Write a bit field, leaving the other bits of the byte untouched.
pub fn set_bits(byte: &mut u8, shift: u8, width: u8, v: u8) {
    let mask = (((1u16 << width) - 1) as u8) << shift;
    *byte = (*byte & !mask) | ((v << shift) & mask);
}

/// True if every byte is `0x00` or `0xFF` (the permissive "empty record" test).
pub fn is_blank(b: &[u8]) -> bool {
    b.iter().all(|&c| c == 0 || c == 0xFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freq_bytes() {
        let mut b = [0; 4];
        put_freq(&mut b, Some("432.025".parse().unwrap()));
        assert_eq!(b, [0x00, 0x25, 0x20, 0x43]);
        put_freq(&mut b, Freq::from_mhz(462.5625));
        assert_eq!(b, [0x50, 0x62, 0x25, 0x46]);
        assert_eq!(get_freq(&b).unwrap().to_string(), "462.56250");
        assert_eq!(get_freq(&[0xFF; 4]), None);
        put_freq(&mut b, None);
        assert_eq!(b, [0xFF; 4]);
        assert!("12345".parse::<Freq>().is_err());
    }

    #[test]
    fn tone_bytes() {
        let cases: [(&str, [u8; 2]); 6] = [
            ("67.0", [0x70, 0x06]),
            ("127.3", [0x73, 0x12]),
            ("D023N", [0x23, 0x80]),
            ("D023I", [0x23, 0xC0]),
            ("D754N", [0x54, 0x87]),
            ("D754I", [0x54, 0xC7]),
        ];
        for (s, bytes) in cases {
            let t: Tone = s.parse().unwrap();
            let mut b = [0; 2];
            put_tone(&mut b, t);
            assert_eq!(b, bytes, "{s}");
            assert_eq!(get_tone(&b), t);
        }
        assert_eq!(get_tone(&[0, 0]), Tone::Off);
        assert_eq!(get_tone(&[0xFF, 0xFF]), Tone::Off);
        for code in DCS_CODES {
            for inverted in [false, true] {
                let mut b = [0; 2];
                put_tone(&mut b, Tone::Dcs { code, inverted });
                assert_eq!(get_tone(&b), Tone::Dcs { code, inverted });
            }
        }
        for t in CTCSS_TONES {
            let mut b = [0; 2];
            put_tone(&mut b, Tone::Ctcss(t));
            assert_eq!(get_tone(&b), Tone::Ctcss(t));
        }
    }

    #[test]
    fn strings() {
        let mut b = [0xAA; 16];
        put_str(&mut b, "Channel 1", 16, 0xFF);
        assert_eq!(&b[..10], b"Channel 1\0");
        assert!(b[10..].iter().all(|&c| c == 0xFF));
        assert_eq!(get_str(&b), "Channel 1");
        put_str(&mut b, "ABCDEFGHIJKLMNOPQRS", 16, 0);
        assert_eq!(get_str(&b), "ABCDEFGHIJKLMNOP");
        let mut u = [0; 12];
        put_utf16(&mut u, "Call 1");
        assert_eq!(get_utf16(&u), "Call 1");
    }

    #[test]
    fn bits() {
        let mut b = 0b1010_0101;
        assert_eq!(get_bits(b, 4, 4), 0b1010);
        set_bits(&mut b, 0, 4, 0b0011);
        assert_eq!(b, 0b1010_0011);
    }
}
