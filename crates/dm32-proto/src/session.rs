//! The DM-32UV programming session: handshake, V-frame queries, program mode, memory access.
//!
//! Wire format and timings follow the DM32-Protocol-Spec (connection sequence and commands)
//! and the neonplug/qdmr implementations.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::transport::{Transport, hex};
use crate::{Error, Result};

/// Size of a flash page. Reads and writes are done in units of at most one page.
pub const PAGE: u32 = 0x1000;

/// Address the official CPS writes a logical block to when it has no physical page yet; the
/// radio allocates a page for the block ID in the last byte of the payload. Deliberately
/// unaligned.
pub const SENTINEL: u32 = 0xFFF001;

const ACK: u8 = 0x06;

/// V-frame IDs, queried with `56 00 00 <hint> <id>`.
pub mod vframe {
    pub const FIRMWARE: u8 = 0x01;
    pub const UNKNOWN_02: u8 = 0x02;
    pub const BUILD_DATE: u8 = 0x03;
    pub const DSP_VERSION: u8 = 0x04;
    pub const RADIO_VERSION: u8 = 0x05;
    pub const RANGE_06: u8 = 0x06;
    pub const RANGE_07: u8 = 0x07;
    pub const RANGE_08: u8 = 0x08;
    pub const RANGE_09: u8 = 0x09;
    pub const MAIN_CONFIG: u8 = 0x0A;
    pub const CODEPLUG_VERSION: u8 = 0x0B;
    pub const UNKNOWN_0D: u8 = 0x0D;
    pub const BOOT_IMAGE: u8 = 0x0E;
    pub const CONTACTS: u8 = 0x0F;
    pub const MAX_CONTACTS: u8 = 0x10;

    /// The order the OEM CPS and neonplug query frames in (0x0C is never queried).
    pub const ALL: [u8; 15] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0D, 0x0E, 0x0F, 0x10,
    ];
}

/// Inter-command delays. [`Timing::hardware`] uses the values tuned against real radios;
/// [`Timing::none`] is for replay tests.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    pub init: Duration,
    pub flush: Duration,
    pub psearch_reply: Duration,
    pub handshake_step: Duration,
    pub vframe: Duration,
    pub program_step: Duration,
    pub before_read: Duration,
    pub after_read: Duration,
    pub between_probes: Duration,
    pub between_pages: Duration,
    pub timeout: Duration,
    pub page_timeout: Duration,
}

impl Timing {
    pub fn hardware() -> Self {
        let ms = Duration::from_millis;
        Timing {
            init: ms(400),
            flush: ms(200),
            psearch_reply: ms(150),
            handshake_step: ms(50),
            vframe: ms(50),
            program_step: ms(10),
            before_read: ms(25),
            after_read: ms(30),
            between_probes: ms(5),
            between_pages: ms(150),
            timeout: ms(5000),
            page_timeout: ms(15000),
        }
    }

    pub fn none() -> Self {
        let z = Duration::ZERO;
        Timing {
            init: z,
            flush: z,
            psearch_reply: z,
            handshake_step: z,
            vframe: z,
            program_step: z,
            before_read: z,
            after_read: z,
            between_probes: z,
            between_pages: z,
            timeout: Duration::from_millis(1),
            page_timeout: Duration::from_millis(1),
        }
    }
}

impl Default for Timing {
    fn default() -> Self {
        Timing::hardware()
    }
}

/// An inclusive address range as reported by a V-frame (`end` is the last byte).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub start: u32,
    pub end: u32,
}

impl Range {
    /// Start addresses of every 4 KB page in the range.
    pub fn pages(&self) -> impl Iterator<Item = u32> + use<> {
        let last = self.end / PAGE * PAGE;
        (self.start..=last).step_by(PAGE as usize)
    }

    pub fn len(&self) -> u32 {
        self.end + 1 - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.end < self.start
    }

    fn parse(payload: &[u8]) -> Option<Range> {
        if payload.len() < 8 {
            return None;
        }
        let start = u32::from_le_bytes(payload[0..4].try_into().unwrap());
        let end = u32::from_le_bytes(payload[4..8].try_into().unwrap());
        Some(Range { start, end })
    }
}

/// Identification returned by the handshake.
#[derive(Debug, Clone, Default)]
pub struct RadioInfo {
    pub model: String,
    pub firmware: String,
    pub build_date: String,
    pub dsp_version: String,
    pub radio_version: String,
    pub codeplug_version: String,
    /// Main configuration region (logical blocks), V-frame 0x0A.
    pub config: Option<Range>,
    /// DMR contact (call-sign) database, V-frame 0x0F.
    pub contacts: Option<Range>,
    /// Boot image region, V-frame 0x0E.
    pub boot_image: Option<Range>,
    /// Maximum number of DMR contacts, V-frame 0x10.
    pub max_contacts: Option<u32>,
    /// Raw payload of every V-frame that answered.
    pub vframes: BTreeMap<u8, Vec<u8>>,
}

impl RadioInfo {
    fn from_vframes(model: String, vframes: BTreeMap<u8, Vec<u8>>) -> Self {
        let text = |id| {
            vframes
                .get(&id)
                .map(|p: &Vec<u8>| {
                    String::from_utf8_lossy(p)
                        .trim_matches(['\0', ' '])
                        .to_string()
                })
                .unwrap_or_default()
        };
        let range = |id| vframes.get(&id).and_then(|p| Range::parse(p));
        let max_contacts = vframes
            .get(&vframe::MAX_CONTACTS)
            .filter(|p| !p.is_empty())
            .map(|p| {
                p.iter()
                    .take(4)
                    .enumerate()
                    .fold(0u32, |acc, (i, &b)| acc | (b as u32) << (8 * i))
            });
        RadioInfo {
            model,
            firmware: text(vframe::FIRMWARE),
            build_date: text(vframe::BUILD_DATE),
            dsp_version: text(vframe::DSP_VERSION),
            radio_version: text(vframe::RADIO_VERSION),
            codeplug_version: text(vframe::CODEPLUG_VERSION),
            config: range(vframe::MAIN_CONFIG),
            contacts: range(vframe::CONTACTS),
            boot_image: range(vframe::BOOT_IMAGE),
            max_contacts,
            vframes,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Fresh,
    Identified,
    Program,
}

/// A programming session on top of some [`Transport`].
///
/// A session must be fresh: the radio answers the handshake once per connection, and there is
/// no exit command, so leaving program mode means closing (resetting) the port.
pub struct Session<T: Transport> {
    t: T,
    timing: Timing,
    state: State,
    info: Option<RadioInfo>,
    oem_compat: bool,
}

impl<T: Transport> Session<T> {
    pub fn new(transport: T, timing: Timing) -> Self {
        Session {
            t: transport,
            timing,
            state: State::Fresh,
            info: None,
            oem_compat: true,
        }
    }

    /// Mirror the official CPS byte for byte (the length-hinted 0x0D V-frame first and the 0x47
    /// probe before PROGRAM). Neither is known to be required, but this is the sequence the
    /// radio is known to accept. On by default.
    pub fn set_oem_compat(&mut self, on: bool) {
        self.oem_compat = on;
    }

    pub fn into_transport(self) -> T {
        self.t
    }

    pub fn info(&self) -> Option<&RadioInfo> {
        self.info.as_ref()
    }

    fn recv(&mut self, n: usize) -> Result<Vec<u8>> {
        let mut buf = vec![0; n];
        self.t.recv(&mut buf, self.timing.timeout)?;
        Ok(buf)
    }

    fn expect(&mut self, what: &'static str, expected: &[u8]) -> Result<()> {
        let got = self.recv(expected.len())?;
        if got != expected {
            return Err(Error::Protocol(format!(
                "{what}: expected [{}], got [{}]",
                hex(expected),
                hex(&got)
            )));
        }
        Ok(())
    }

    /// Settle, run PSEARCH / PASSSTA / SYSINFO and query all V-frames.
    pub fn identify(&mut self) -> Result<&RadioInfo> {
        if self.state != State::Fresh {
            return Ok(self.info.as_ref().expect("identified"));
        }
        let tm = self.timing;

        // The radio needs ~800 ms of quiet after the port opens before it answers PSEARCH.
        self.t.delay(tm.init);
        self.t.discard_input()?;
        self.t.delay(tm.flush);
        self.t.delay(tm.flush);

        self.t.send(b"PSEARCH")?;
        self.t.delay(tm.psearch_reply);
        let reply = match self.recv(8) {
            Ok(r) => r,
            Err(Error::Timeout { got, .. }) if got.is_empty() => return Err(Error::NoReply),
            Err(e) => return Err(e),
        };
        if reply[0] != ACK {
            return Err(Error::Protocol(format!(
                "PSEARCH: unexpected reply [{}]",
                hex(&reply)
            )));
        }
        let model = String::from_utf8_lossy(&reply[1..])
            .trim_matches(['\0', ' '])
            .to_string();
        if !["DP570", "DM32", "DM-32"].iter().any(|m| model.contains(m)) {
            return Err(Error::UnsupportedModel(model));
        }

        self.t.delay(tm.handshake_step);
        self.t.send(b"PASSSTA")?;
        self.t.delay(tm.handshake_step);
        let pass = self.recv(3)?;
        if pass[0] != b'P' {
            return Err(Error::Protocol(format!(
                "PASSSTA: unexpected reply [{}]",
                hex(&pass)
            )));
        }

        self.t.delay(tm.handshake_step);
        self.t.send(b"SYSINFO")?;
        self.t.delay(tm.handshake_step);
        self.expect("SYSINFO", &[ACK])?;

        let mut frames = BTreeMap::new();
        if self.oem_compat {
            let p = self.query_vframe(vframe::UNKNOWN_0D, 0x40)?;
            frames.insert(vframe::UNKNOWN_0D, p);
        }
        for id in vframe::ALL {
            let p = self.query_vframe(id, 0)?;
            // The plain 0x0D query answers empty; keep the hinted payload if we have it.
            if !p.is_empty() || !frames.contains_key(&id) {
                frames.insert(id, p);
            }
        }
        let info = RadioInfo::from_vframes(model, frames);
        if info.config.is_none() {
            return Err(Error::Protocol(
                "radio did not report its configuration memory range".into(),
            ));
        }
        self.info = Some(info);
        self.state = State::Identified;
        Ok(self.info.as_ref().unwrap())
    }

    /// Query one V-frame. `hint` is the requested payload length (0 for "default").
    pub fn query_vframe(&mut self, id: u8, hint: u8) -> Result<Vec<u8>> {
        self.t.send(&[0x56, 0x00, 0x00, hint, id])?;
        self.t.delay(self.timing.vframe);
        let head = self.recv(3)?;
        if head[0] != 0x56 || head[1] != id {
            return Err(Error::Protocol(format!(
                "V-frame {id:#04x}: bad header [{}]",
                hex(&head)
            )));
        }
        let payload = self.recv(head[2] as usize)?;
        if !payload.is_empty() {
            self.t.delay(self.timing.vframe);
        }
        Ok(payload)
    }

    /// Enter programming mode (identifying the radio first if needed).
    pub fn enter_program(&mut self) -> Result<()> {
        match self.state {
            State::Program => return Ok(()),
            State::Fresh => {
                self.identify()?;
            }
            State::Identified => {}
        }
        if self.oem_compat {
            self.probe_47()?;
        }
        let step = self.timing.program_step;
        self.t.send(b"\xff\xff\xff\xff\x0cPROGRAM")?;
        self.expect("PROGRAM", &[ACK])?;
        self.t.delay(step);
        self.t.send(&[0x02])?;
        self.expect("program mode 02", &[0xff; 8])?;
        self.t.delay(step);
        self.t.send(&[ACK])?;
        self.expect("program mode ACK", &[ACK])?;
        self.t.delay(step);
        self.state = State::Program;
        Ok(())
    }

    /// The OEM CPS's `47 00 00 00 00 01` request: a 256-byte read from what looks like a
    /// separate address space. Purpose unknown; the reply is kept in the V-frame map under 0x47.
    fn probe_47(&mut self) -> Result<()> {
        self.t.send(&[0x47, 0x00, 0x00, 0x00, 0x00, 0x01])?;
        self.t.delay(self.timing.before_read);
        self.expect("0x47 probe", &[0x53, 0x00, 0x00, 0x00, 0x00, 0x01])?;
        let data = self.recv(256)?;
        if let Some(info) = self.info.as_mut() {
            info.vframes.insert(0x47, data);
        }
        Ok(())
    }

    /// Read `len` (≤ 4096) bytes at `addr`.
    pub fn read(&mut self, addr: u32, len: u16) -> Result<Vec<u8>> {
        self.enter_program()?;
        let a = addr.to_le_bytes();
        let l = len.to_le_bytes();
        self.t.send(&[0x52, a[0], a[1], a[2], l[0], l[1]])?;
        self.t.delay(self.timing.before_read);
        let head = self.recv(6)?;
        if head[0] != 0x57 || head[1..4] != a[..3] {
            return Err(Error::Protocol(format!(
                "read {addr:#08x}: bad header [{}]",
                hex(&head)
            )));
        }
        let got = u16::from_le_bytes([head[4], head[5]]);
        if got == 0 || got > len {
            return Err(Error::Protocol(format!(
                "read {addr:#08x}: radio returned {got} bytes, asked {len}"
            )));
        }
        let mut data = vec![0; got as usize];
        self.t.recv(&mut data, self.timing.page_timeout)?;
        self.t.delay(self.timing.after_read);
        if got < len {
            return Err(Error::Protocol(format!(
                "read {addr:#08x}: short read, {got} of {len} bytes"
            )));
        }
        Ok(data)
    }

    /// Read one full 4 KB page.
    pub fn read_page(&mut self, addr: u32) -> Result<Vec<u8>> {
        let data = self.read(addr, PAGE as u16)?;
        self.t.delay(self.timing.between_pages);
        Ok(data)
    }

    /// Write `data` (≤ 4096 bytes, must not cross a page boundary) at `addr`, or a full page
    /// to [`SENTINEL`].
    pub fn write(&mut self, addr: u32, data: &[u8]) -> Result<()> {
        assert!(
            data.len() as u32 <= PAGE
                && (addr == SENTINEL || (addr % PAGE) as usize + data.len() <= PAGE as usize),
            "write of {} bytes at {addr:#08x} crosses a page boundary",
            data.len()
        );
        self.enter_program()?;
        let a = addr.to_le_bytes();
        let l = (data.len() as u16).to_le_bytes();
        let mut frame = Vec::with_capacity(6 + data.len());
        frame.extend_from_slice(&[0x57, a[0], a[1], a[2], l[0], l[1]]);
        frame.extend_from_slice(data);
        self.t.send(&frame)?;
        let ack = self.recv(1)?;
        if ack[0] != ACK {
            return Err(Error::Protocol(format!(
                "write {addr:#08x}: radio answered {:#04x} instead of ACK",
                ack[0]
            )));
        }
        self.t.delay(self.timing.between_pages);
        Ok(())
    }

    /// Read the logical block ID (the last byte) of every page in `range`.
    pub fn scan_block_ids(
        &mut self,
        range: Range,
        mut progress: impl FnMut(usize, usize),
    ) -> Result<Vec<(u32, u8)>> {
        let pages: Vec<u32> = range.pages().collect();
        let mut out = Vec::with_capacity(pages.len());
        for (i, page) in pages.iter().enumerate() {
            let id = self.read(page + PAGE - 1, 1)?[0];
            out.push((*page, id));
            self.t.delay(self.timing.between_probes);
            progress(i + 1, pages.len());
        }
        Ok(out)
    }
}
