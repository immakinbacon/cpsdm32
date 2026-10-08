//! The DMR contact database ("CSV Contacts" / "Big Contacts" in the CPS): up to 50,000 92-byte
//! records in the raw region at V-frame 0x0F (0x278000), held in [`Image::contacts`].
//!
//! # Region layout (spec 05 "DMR Contact Database", qdmr `dm32uv_callsigndb`)
//! - page 0: `u32` LE count, 12 bytes (0xFF on the radio), then records 0-43 from +0x10;
//! - page n ≥ 1: records 44n … 44n+43 from +0x000;
//! - records never straddle a page; everything after the last record is 0xFF; the last byte of a
//!   page is not a block ID and is preserved.
//!
//! Only page 0 is hardware-confirmed (one contact); the walk over later pages follows qdmr and
//! the spec's reference implementation. Neither sorts the list: qdmr writes it in its user-DB
//! order (sorted by distance from the user's own ID), so no ordering is enforced here.
//! [`ContactDb::sort_by_id`] is available since an ID-ordered list is the natural choice.
//!
//! # Record (92 bytes)
//! | Off | Size | Field |
//! |-----|------|-------|
//! | 0x00 | 16 | name |
//! | 0x10 | 3 | DMR ID, u24 LE |
//! | 0x13 | 1 | flags: `0xF0` in the only radio sample (call type "Private Call" / alert "None"?) |
//! | 0x14 | 8 | callsign (CPS column "Repeater"?) |
//! | 0x1C | 16 | city |
//! | 0x2C | 16 | province / state |
//! | 0x3C | 16 | country |
//! | 0x4C | 16 | remark |
//!
//! Strings: ASCII, NUL-terminated, 0xFF padded; empty fields are all 0xFF (as the OEM CPS
//! writes them). The ID is 24-bit because the radio stores `01 00 00 F0` for ID 1; qdmr writes a
//! u32 (top byte 0), which would clobber the flags byte.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::encoding::{get_str, get_u24, get_u32, put_str, put_u24, put_u32};
use crate::image::{Image, PAGE, Page, blank_page};
use crate::radio::{CONTACT_SIZE, CONTACTS_PER_PAGE};

/// Default maximum number of contacts (V-frame 0x10 on standard firmware).
pub const MAX_CONTACTS: usize = 50_000;
/// Offset of record 0 in page 0.
pub const HEADER_SIZE: usize = 0x10;
/// Flags byte value the radio stores for an ordinary (private-call) contact.
pub const DEFAULT_FLAGS: u8 = 0xF0;
/// First `.data` file page of the contact database.
pub const DATA_FILE_FIRST_PAGE: usize = 81;
/// `.data` page slots reserved for contacts (81..=160).
pub const DATA_FILE_PAGES: usize = 80;

/// One contact record, raw-backed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Contact {
    raw: [u8; CONTACT_SIZE],
}

crate::raw_record!(Contact, CONTACT_SIZE, 0xFF);

/// Latin-1 letters folded to ASCII before sanitising (RadioID.net data has many).
const FOLD_FROM: &str =
    "ÀÁÂÃÄÅàáâãäåÇçÈÉÊËèéêëÌÍÎÏìíîïÑñÒÓÔÕÖØòóôõöøÙÚÛÜùúûüÝýÿŠšŽžŁłŃńŚśŹźŻżĆćĘęĄąČčŘřĚěŮů";
const FOLD_TO: &str =
    "AAAAAAaaaaaaCcEEEEeeeeIIIIiiiiNnOOOOOOooooooUUUUuuuuYyySsZzLlNnSsZzZzCcEeAaCcRrEeUu";

fn fold(s: &str) -> String {
    s.chars()
        .map(|c| match FOLD_FROM.chars().position(|f| f == c) {
            Some(i) => FOLD_TO.as_bytes()[i] as char,
            None => c,
        })
        .collect()
}

/// Write a string field: empty → all 0xFF, else up to `len-1` chars + NUL + 0xFF padding.
fn put_field(b: &mut [u8], s: &str) {
    let s = fold(s.trim());
    if s.is_empty() {
        b.fill(0xFF);
    } else {
        let max = b.len() - 1;
        put_str(b, &s, max, 0xFF);
    }
}

macro_rules! field {
    ($(#[$m:meta])* $get:ident, $set:ident, $off:expr, $len:expr) => {
        $(#[$m])*
        pub fn $get(&self) -> String {
            get_str(&self.raw[$off..$off + $len])
        }
        pub fn $set(&mut self, s: &str) {
            if self.$get() != s {
                put_field(&mut self.raw[$off..$off + $len], s);
            }
        }
    };
}

impl Contact {
    /// A new contact as the radio would store it (0xFF fill, flags 0xF0).
    pub fn new(id: u32, name: &str) -> Contact {
        let mut c = Contact::default();
        c.raw[0x13] = DEFAULT_FLAGS;
        c.set_id(id);
        c.set_name(name);
        c
    }

    field!(
        /// 0x00, 16 bytes (15 chars + NUL written).
        name, set_name, 0x00, 16
    );
    field!(
        /// 0x14, 8 bytes (7 chars + NUL written).
        callsign, set_callsign, 0x14, 8
    );
    field!(
        /// 0x1C, 16 bytes.
        city, set_city, 0x1C, 16
    );
    field!(
        /// 0x2C, 16 bytes.
        state, set_state, 0x2C, 16
    );
    field!(
        /// 0x3C, 16 bytes.
        country, set_country, 0x3C, 16
    );
    field!(
        /// 0x4C, 16 bytes.
        remark, set_remark, 0x4C, 16
    );

    /// DMR ID (24-bit).
    pub fn id(&self) -> u32 {
        get_u24(&self.raw[0x10..])
    }

    pub fn set_id(&mut self, id: u32) {
        put_u24(&mut self.raw[0x10..], id.min(0xFF_FFFF));
    }

    /// Byte 0x13, meaning unknown (radio: 0xF0; `.data` file: 0x00). Probably packs the CPS
    /// "Type" (only "Private Call" offered) and "Alert Call" (None / Ring / Online Tips).
    pub fn flags(&self) -> u8 {
        self.raw[0x13]
    }

    pub fn set_flags(&mut self, v: u8) {
        self.raw[0x13] = v;
    }
}

/// The contact database.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContactDb {
    pub list: Vec<Contact>,
}

impl ContactDb {
    /// Sort by DMR ID (stable).
    pub fn sort_by_id(&mut self) {
        self.list.sort_by_key(|c| c.id());
    }

    /// Drop later duplicates of the same DMR ID, keeping order.
    pub fn dedup_ids(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.list.retain(|c| seen.insert(c.id()));
    }

    pub fn find(&self, id: u32) -> Option<&Contact> {
        self.list.iter().find(|c| c.id() == id)
    }
}

/// Pages needed for `n` contacts (page 0 always exists).
pub fn page_count(n: usize) -> usize {
    n.div_ceil(CONTACTS_PER_PAGE).max(1)
}

/// Page index and byte offset within the page of contact `i`.
pub fn location(i: usize) -> (usize, usize) {
    let (page, slot) = (i / CONTACTS_PER_PAGE, i % CONTACTS_PER_PAGE);
    let base = if page == 0 { HEADER_SIZE } else { 0 };
    (page, base + slot * CONTACT_SIZE)
}

/// Maximum number of contacts the radio accepts: V-frame 0x10 if known, else 50,000.
pub fn max_contacts(img: &Image) -> usize {
    img.info
        .vframe(0x10)
        .filter(|v| !v.is_empty())
        .map(|v| {
            v.iter()
                .take(4)
                .enumerate()
                .fold(0usize, |acc, (i, &b)| acc | (b as usize) << (8 * i))
        })
        .filter(|&n| n > 0)
        .unwrap_or(MAX_CONTACTS)
}

/// Parse `count` records out of a page set; stops at the first missing page.
fn parse_pages(count: usize, page: impl Fn(usize) -> Option<Page>) -> Vec<Contact> {
    let mut out = Vec::with_capacity(count.min(MAX_CONTACTS * 3));
    let mut cur: Option<(usize, Page)> = None;
    for i in 0..count {
        let (p, off) = location(i);
        if cur.as_ref().map(|c| c.0) != Some(p) {
            match page(p) {
                Some(pg) => cur = Some((p, pg)),
                None => break,
            }
        }
        let pg = &cur.as_ref().unwrap().1;
        out.push(Contact::from_bytes(&pg[off..off + CONTACT_SIZE]));
    }
    out
}

fn header_count(p: &[u8]) -> usize {
    match get_u32(p) {
        0xFFFF_FFFF => 0,
        n => n as usize,
    }
}

/// Decode the contact database from the radio pages in `img.contacts`.
pub fn decode(img: &Image) -> ContactDb {
    let Some(p0) = img.contacts.get(&0) else {
        return ContactDb::default();
    };
    // Generous cap so a corrupt count cannot run away; L01 firmware allows 150,000.
    let count = header_count(&p0[..]).min(max_contacts(img).max(150_000));
    let list = parse_pages(count, |p| img.contacts.get(&((p * PAGE) as u32)).cloned());
    ContactDb { list }
}

/// Encode into `img.contacts`: the minimal number of pages, count in the header, unused space
/// 0xFF. Header bytes 4..16 and each page's last byte are kept from the existing pages; pages
/// beyond the new end are removed. Contacts past [`max_contacts`] are dropped.
pub fn encode(db: &ContactDb, img: &mut Image) {
    let n = db.list.len().min(max_contacts(img));
    let pages = page_count(n);
    let mut out: BTreeMap<u32, Page> = BTreeMap::new();
    for p in 0..pages {
        let off = (p * PAGE) as u32;
        let mut page = blank_page(0xFF);
        if let Some(old) = img.contacts.get(&off) {
            page[PAGE - 1] = old[PAGE - 1];
            if p == 0 {
                page[4..HEADER_SIZE].copy_from_slice(&old[4..HEADER_SIZE]);
            }
        }
        out.insert(off, page);
    }
    put_u32(&mut out.get_mut(&0).unwrap()[..], n as u32);
    for (i, c) in db.list.iter().take(n).enumerate() {
        let (p, off) = location(i);
        let page = out.get_mut(&((p * PAGE) as u32)).unwrap();
        page[off..off + CONTACT_SIZE].copy_from_slice(c.bytes());
    }
    img.contacts = out;
}

// ---- Official CPS `.data` file form ----
//
// The CPS re-serialises the contact DB into `.data` pages 81.. instead of saving the radio
// bytes. Comparing the factory v1.60 file (page 81) with the radio page of the same 1-contact
// codeplug (read and write captures) gives these differences, everything else being equal:
//
// | | radio | `.data` |
// |-|-------|---------|
// | header bytes 4..16 | 0xFF | 0x00 |
// | name padding after the NUL | 0xFF | 0x00 |
// | record byte 0x13 | 0xF0 | 0x00 |
// | after the last record | 0xFF | 0x00 |
//
// Inferred, not verified: that the same 44-per-page walk applies to the file, that only the
// name (not callsign/city/…, which were empty in the sample) is zero padded, and how >80 pages
// (>3,516 contacts) are stored — the known file layout has 80 contact slots.

/// Read the contact DB from an imported `.data` file (`img.data_extras` pages 81..), converted to
/// the radio form. `None` if the file holds no contact pages.
pub fn from_data_file(img: &Image) -> Option<ContactDb> {
    let p0 = img.data_extras.get(&DATA_FILE_FIRST_PAGE)?;
    let count = get_u32(&p0[..]);
    if count == 0xFFFF_FFFF {
        return None;
    }
    let count = (count as usize).min(DATA_FILE_PAGES * CONTACTS_PER_PAGE);
    let mut list = parse_pages(count, |p| {
        img.data_extras.get(&(DATA_FILE_FIRST_PAGE + p)).cloned()
    });
    for c in &mut list {
        let name = c.name();
        put_field(&mut c.raw[0x00..0x10], &name);
        if c.flags() == 0x00 {
            c.set_flags(DEFAULT_FLAGS);
        }
    }
    Some(ContactDb { list })
}

/// Serialise `db` in the `.data` form: `(file page index, page)` from page 81, minimal count.
/// Callers storing them in `data_extras` should drop stale higher contact pages.
pub fn to_data_file_pages(db: &ContactDb) -> Vec<(usize, Page)> {
    let n = db.list.len().min(DATA_FILE_PAGES * CONTACTS_PER_PAGE);
    let mut pages: Vec<Page> = (0..page_count(n)).map(|_| blank_page(0)).collect();
    put_u32(&mut pages[0][..], n as u32);
    for (i, c) in db.list.iter().take(n).enumerate() {
        let mut raw = c.raw;
        let name = c.name();
        if name.is_empty() {
            raw[..0x10].fill(0);
        } else {
            put_str(&mut raw[..0x10], &name, 15, 0x00);
        }
        if raw[0x13] == DEFAULT_FLAGS {
            raw[0x13] = 0;
        }
        let (p, off) = location(i);
        pages[p][off..off + CONTACT_SIZE].copy_from_slice(&raw);
    }
    pages
        .into_iter()
        .enumerate()
        .map(|(i, p)| (DATA_FILE_FIRST_PAGE + i, p))
        .collect()
}

// ---- CSV ----

/// Split CSV text (RFC 4180: quoted fields, doubled quotes, CRLF/LF, optional BOM) into rows.
pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    field.push('"');
                    chars.next();
                }
                '"' => quoted = false,
                c => field.push(c),
            }
            continue;
        }
        match c {
            '"' => quoted = true,
            ',' => row.push(std::mem::take(&mut field)),
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\r' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            c => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows.retain(|r| r.iter().any(|f| !f.trim().is_empty()));
    rows
}

fn csv_field(out: &mut String, s: &str) {
    if s.contains([',', '"', '\n', '\r']) {
        out.push('"');
        out.push_str(&s.replace('"', "\"\""));
        out.push('"');
    } else {
        out.push_str(s);
    }
}

/// Result of a CSV import.
#[derive(Debug, Clone, Default)]
pub struct CsvImport {
    pub contacts: Vec<Contact>,
    /// `(1-based line, reason)` for rows that were skipped.
    pub skipped: Vec<(usize, String)>,
}

#[derive(Default)]
struct Columns {
    id: Option<usize>,
    name: Option<usize>,
    first: Option<usize>,
    last: Option<usize>,
    call: Option<usize>,
    city: Option<usize>,
    state: Option<usize>,
    country: Option<usize>,
    remark: Option<usize>,
}

/// Import contacts from CSV with a header row. Columns are matched by name, case-insensitively
/// and ignoring spaces, `_`, `/` and `.`, so these formats all work:
/// - RadioID.net `user.csv`: `RADIO_ID,CALLSIGN,FIRST_NAME,LAST_NAME,CITY,STATE,COUNTRY`
///   (+ `REMARKS` in some versions); the name becomes "FIRST LAST";
/// - the official CPS export (`[BigContactExport]`):
///   `No.,ID,Repeater,Name,City,Province,Country,Remark,Type,Alert Call` — "Repeater" is taken to
///   be the callsign field (inferred);
/// - [`export_csv`]'s output.
///
/// Rows with a missing or invalid ID (not 1..=16777215) are skipped and reported.
pub fn import_csv(text: &str) -> Result<CsvImport, String> {
    let rows = parse_csv(text);
    let Some((header, body)) = rows.split_first() else {
        return Err("empty CSV file".into());
    };
    let mut col = Columns::default();
    for (i, h) in header.iter().enumerate() {
        let key: String = h
            .chars()
            .filter(|c| !matches!(c, ' ' | '_' | '/' | '.' | '-'))
            .collect::<String>()
            .to_ascii_lowercase();
        let slot = match key.as_str() {
            "radioid" | "id" | "dmrid" | "tgdmrid" | "callid" => &mut col.id,
            "name" | "contactname" => &mut col.name,
            "firstname" => &mut col.first,
            "lastname" | "surname" => &mut col.last,
            "callsign" | "call" | "repeater" => &mut col.call,
            "city" => &mut col.city,
            "state" | "province" | "stateprov" => &mut col.state,
            "country" => &mut col.country,
            "remark" | "remarks" | "comment" | "notes" => &mut col.remark,
            _ => continue,
        };
        slot.get_or_insert(i);
    }
    let Some(id_col) = col.id else {
        return Err("no ID column (expected RADIO_ID or ID) in the CSV header".into());
    };
    let mut res = CsvImport::default();
    for (n, row) in body.iter().enumerate() {
        let line = n + 2;
        let get = |c: Option<usize>| c.and_then(|c| row.get(c)).map(|s| s.trim()).unwrap_or("");
        let id = match get(Some(id_col)).parse::<u32>() {
            Ok(id) if (1..=0xFF_FFFF).contains(&id) => id,
            _ => {
                res.skipped
                    .push((line, format!("invalid ID '{}'", get(Some(id_col)))));
                continue;
            }
        };
        let name = match get(col.name) {
            "" => format!("{} {}", get(col.first), get(col.last))
                .trim()
                .to_string(),
            s => s.to_string(),
        };
        let mut c = Contact::new(id, &name);
        c.set_callsign(get(col.call));
        c.set_city(get(col.city));
        c.set_state(get(col.state));
        c.set_country(get(col.country));
        c.set_remark(get(col.remark));
        res.contacts.push(c);
    }
    Ok(res)
}

/// Export in the official CPS column layout (`[BigContactExport]`, inferred from the CPS UI
/// strings): `No.,ID,Repeater,Name,City,Province,Country,Remark,Type,Alert Call`. Type and Alert
/// Call are written as the only values the CPS offers / the default ("Private Call", "None").
pub fn export_csv(db: &ContactDb) -> String {
    let mut out =
        String::from("No.,ID,Repeater,Name,City,Province,Country,Remark,Type,Alert Call\r\n");
    for (i, c) in db.list.iter().enumerate() {
        let _ = write!(out, "{},{},", i + 1, c.id());
        for s in [
            c.callsign(),
            c.name(),
            c.city(),
            c.state(),
            c.country(),
            c.remark(),
        ] {
            csv_field(&mut out, &s);
            out.push(',');
        }
        out.push_str("Private Call,None\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 4 KB page read at 0x278000 in the OEM read capture.
    fn capture_page() -> Vec<u8> {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../dm32-proto/tests/fixtures/oem_read_capture.txt"
        );
        let text = String::from_utf8_lossy(&std::fs::read(path).unwrap()).into_owned();
        let lines: Vec<&str> = text.lines().collect();
        let req = lines
            .iter()
            .position(|l| l.trim_start().starts_with("52 00 80 27 00 10"))
            .expect("contact page read request");
        let start = req
            + 1
            + lines[req + 1..]
                .iter()
                .position(|l| l.contains("Read data"))
                .unwrap();
        let mut bytes = Vec::new();
        for l in &lines[start + 1..] {
            if l.starts_with('[') {
                break;
            }
            let hex = &l[..l.len().min(52)];
            bytes.extend(
                hex.split_whitespace()
                    .map(|h| u8::from_str_radix(h, 16).unwrap()),
            );
        }
        assert_eq!(bytes.len(), 6 + PAGE);
        assert_eq!(&bytes[..6], &[0x57, 0x00, 0x80, 0x27, 0x00, 0x10]);
        bytes[6..].to_vec()
    }

    fn radio_image(page: &[u8]) -> Image {
        let mut img = Image::default();
        img.contacts.insert(0, crate::image::page_from(page));
        img
    }

    #[test]
    fn capture_page_round_trip() {
        let page = capture_page();
        let img = radio_image(&page);
        let db = decode(&img);
        assert_eq!(db.list.len(), 1);
        let c = &db.list[0];
        assert_eq!(c.name(), "Contacts 1");
        assert_eq!(c.id(), 1);
        assert_eq!(c.flags(), 0xF0);
        assert_eq!(c.callsign(), "");

        let mut out = img.clone();
        encode(&db, &mut out);
        assert!(out == img);

        // A fresh image with a contact built from scratch gives the same bytes.
        let mut fresh = Image::default();
        encode(
            &ContactDb {
                list: vec![Contact::new(1, "Contacts 1")],
            },
            &mut fresh,
        );
        assert!(fresh.contacts[&0][..] == page[..]);
    }

    #[test]
    fn data_file_form() {
        // Import converts the CPS pages; rebuild an image holding them raw.
        let bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/factory-v1.60.data"
        ))
        .unwrap();
        let raw81 = crate::image::page_from(&bytes[81 * PAGE..82 * PAGE]);
        let mut img = Image::default();
        img.data_extras.insert(81, raw81);
        let db = from_data_file(&img).unwrap();
        assert_eq!(db.list.len(), 1);
        assert_eq!(db.list[0].name(), "Contacts 1");
        assert_eq!(db.list[0].id(), 1);

        // .data → radio form equals the radio page of the same codeplug.
        let mut radio = Image::default();
        encode(&db, &mut radio);
        assert!(radio.contacts[&0][..] == capture_page()[..]);

        // And back to the .data form, byte-identical to the file.
        let pages = to_data_file_pages(&db);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].0, 81);
        assert!(pages[0].1[..] == img.data_extras[&81][..]);
    }

    #[test]
    fn page_math() {
        assert_eq!(page_count(0), 1);
        assert_eq!(page_count(44), 1);
        assert_eq!(page_count(45), 2);
        assert_eq!(page_count(50_000), 1137);
        assert_eq!(location(0), (0, 0x10));
        assert_eq!(location(43), (0, 0x10 + 43 * 92));
        assert_eq!(location(44), (1, 0));
        assert_eq!(location(89), (2, 92));
        // Last record of page 0 ends at 0xFE0, of later pages at 0xFD0.
        assert_eq!(location(43).1 + CONTACT_SIZE, 0xFE0);
        assert_eq!(location(87).1 + CONTACT_SIZE, 0xFD0);
    }

    #[test]
    fn large_round_trip() {
        let list: Vec<Contact> = (0..1000u32)
            .map(|i| {
                let mut c = Contact::new(3_100_000 + i, &format!("Op {i}"));
                c.set_callsign(&format!("K{i}AB"));
                c.set_city("Springfield");
                c.set_state("IL");
                c.set_country("United States");
                c.set_remark("DMR");
                c
            })
            .collect();
        let db = ContactDb { list };
        let mut img = Image::default();
        // Stale pages beyond the end must go.
        img.contacts.insert(40 * PAGE as u32, blank_page(0));
        encode(&db, &mut img);
        assert_eq!(img.contacts.len(), page_count(1000));
        assert_eq!(img.contacts.len(), 23);
        assert_eq!(get_u32(&img.contacts[&0][..]), 1000);
        let last = &img.contacts[&(22 * PAGE as u32)];
        let end = location(999).1 + CONTACT_SIZE;
        assert!(last[end..].iter().all(|&b| b == 0xFF));
        let back = decode(&img);
        assert_eq!(back, db);
        assert_eq!(back.list[999].callsign(), "K999AB");
        assert_eq!(back.list[500].country(), "United States");

        // Shrinking removes pages.
        encode(&ContactDb::default(), &mut img);
        assert_eq!(img.contacts.len(), 1);
        assert_eq!(decode(&img).list.len(), 0);
    }

    #[test]
    fn radioid_csv() {
        let csv = "\u{FEFF}RADIO_ID,CALLSIGN,FIRST_NAME,LAST_NAME,CITY,STATE,COUNTRY\r\n\
                   1023001,VE3THW,Wayne,Holmes,Mississauga,Ontario,Canada\r\n\
                   3100001,K1ABC,\"José\",\"O\"\"Brien, Jr\",Boston,Massachusetts,United States\r\n\
                   bad,X1X,A,B,C,D,E\r\n\
                   \r\n";
        let r = import_csv(csv).unwrap();
        assert_eq!(r.contacts.len(), 2);
        assert_eq!(r.skipped.len(), 1);
        assert_eq!(r.skipped[0].0, 4);
        let c = &r.contacts[0];
        assert_eq!(c.id(), 1023001);
        assert_eq!(c.callsign(), "VE3THW");
        assert_eq!(c.name(), "Wayne Holmes");
        assert_eq!(c.city(), "Mississauga");
        assert_eq!(c.state(), "Ontario");
        assert_eq!(c.country(), "Canada");
        assert_eq!(c.remark(), "");
        assert!(c.bytes()[0x4C..].iter().all(|&b| b == 0xFF));
        let c = &r.contacts[1];
        // Folded to ASCII and truncated to 15 characters.
        assert_eq!(c.name(), "Jose O\"Brien, J");
        assert_eq!(c.country(), "United States");

        // Export → import round trip (CPS column layout).
        let db = ContactDb {
            list: r.contacts.clone(),
        };
        let text = export_csv(&db);
        assert!(text.starts_with("No.,ID,Repeater,Name,"));
        let again = import_csv(&text).unwrap();
        assert!(again.skipped.is_empty());
        assert_eq!(again.contacts, r.contacts);
    }

    #[test]
    fn csv_parser() {
        let rows = parse_csv("a,\"b,c\",\"d\"\"e\"\n\"multi\nline\",,x");
        assert_eq!(rows[0], vec!["a", "b,c", "d\"e"]);
        assert_eq!(rows[1], vec!["multi\nline", "", "x"]);
        assert!(import_csv("NAME,CITY\nfoo,bar\n").is_err());
    }
}
