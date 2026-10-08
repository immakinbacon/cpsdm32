//! A simulated radio, for tests and for trying the software without hardware.
//!
//! It answers the same requests as a DM-32UV and keeps a sparse 24-bit memory. Writes to the
//! sentinel address allocate a free page in the config region, as the radio does for logical
//! blocks that have no page yet.

use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;

use crate::transport::Transport;
use crate::{Error, Result, SENTINEL};

const PAGE: usize = 0x1000;
const CONFIG: (u32, u32) = (0x001000, 0x0C8FFF);
const CONTACTS: (u32, u32) = (0x278000, 0x6DBFFF);

#[derive(Debug, Clone)]
pub struct SimRadio {
    /// Page start address → contents.
    pub memory: BTreeMap<u32, Box<[u8; PAGE]>>,
    pub firmware: String,
    input: Vec<u8>,
    output: VecDeque<u8>,
    /// Number of 4 KB writes received.
    pub writes: usize,
}

impl Default for SimRadio {
    fn default() -> Self {
        SimRadio::new()
    }
}

impl SimRadio {
    /// An empty radio: every config page free (tagged 0xFF).
    pub fn new() -> Self {
        SimRadio {
            memory: BTreeMap::new(),
            firmware: "DM32.01.01.046".into(),
            input: Vec::new(),
            output: VecDeque::new(),
            writes: 0,
        }
    }

    /// Put a logical block on a specific page.
    pub fn put_block(&mut self, page: u32, data: &[u8]) {
        let mut p = Box::new([0xFF; PAGE]);
        p[..data.len().min(PAGE)].copy_from_slice(&data[..data.len().min(PAGE)]);
        self.memory.insert(page, p);
    }

    fn byte(&self, addr: u32) -> u8 {
        let page = addr & !(PAGE as u32 - 1);
        self.memory
            .get(&page)
            .map_or(0xFF, |p| p[(addr - page) as usize])
    }

    fn page_mut(&mut self, page: u32) -> &mut [u8; PAGE] {
        self.memory
            .entry(page)
            .or_insert_with(|| Box::new([0xFF; PAGE]))
    }

    fn reply(&mut self, b: &[u8]) {
        self.output.extend(b);
    }

    fn vframe(&self, id: u8) -> Vec<u8> {
        let range = |a: u32, b: u32| [a.to_le_bytes(), b.to_le_bytes()].concat();
        match id {
            0x01 => self.firmware.as_bytes().to_vec(),
            0x03 => b"2022-06-27".to_vec(),
            0x04 => b"D1.01.01.004".to_vec(),
            0x05 => b"R1.00.01.001".to_vec(),
            0x0A => range(CONFIG.0, CONFIG.1),
            0x0B => b"C1.00.01.001".to_vec(),
            0x0E => range(0x150000, 0x175FFF),
            0x0F => range(CONTACTS.0, CONTACTS.1),
            0x10 => vec![0x50, 0xC3, 0x00],
            _ => Vec::new(),
        }
    }

    /// Consume as many complete requests from the input as possible.
    fn process(&mut self) -> Result<()> {
        loop {
            let inp = &self.input;
            let Some(&first) = inp.first() else {
                return Ok(());
            };
            let need = match first {
                b'P' | b'S' => 7,
                0x56 => 5,
                0x47 | 0x52 => 6,
                0xFF => 12,
                0x02 | 0x06 => 1,
                0x57 if inp.len() >= 6 => 6 + u16::from_le_bytes([inp[4], inp[5]]) as usize,
                0x57 => 6,
                b => {
                    return Err(Error::Protocol(format!(
                        "simulator: unknown request byte {b:#04x}"
                    )));
                }
            };
            if inp.len() < need {
                return Ok(());
            }
            let req: Vec<u8> = self.input.drain(..need).collect();
            let addr = |r: &[u8]| u32::from_le_bytes([r[1], r[2], r[3], 0]);
            match req[0] {
                b'P' if &req[..] == b"PSEARCH" => self.reply(b"\x06DP570UV"),
                b'P' if &req[..] == b"PASSSTA" => self.reply(b"P\x00\x00"),
                b'S' if &req[..] == b"SYSINFO" => self.reply(b"\x06"),
                0x56 => {
                    let payload = self.vframe(req[4]);
                    self.reply(&[0x56, req[4], payload.len() as u8]);
                    self.reply(&payload);
                }
                0x47 => {
                    self.reply(&[0x53, req[1], req[2], req[3], req[4], req[5]]);
                    self.reply(&[0xFF; 256]);
                }
                0xFF => self.reply(b"\x06"),
                0x02 => self.reply(&[0xFF; 8]),
                0x06 => self.reply(b"\x06"),
                0x52 => {
                    let a = addr(&req);
                    let len = u16::from_le_bytes([req[4], req[5]]) as u32;
                    let data: Vec<u8> = if a == SENTINEL {
                        let mut d = vec![0; len as usize];
                        if let Some(last) = d.last_mut() {
                            *last = 0x48;
                        }
                        d
                    } else {
                        (a..a + len).map(|x| self.byte(x)).collect()
                    };
                    self.reply(
                        &req[..6]
                            .iter()
                            .enumerate()
                            .map(|(i, &b)| if i == 0 { 0x57 } else { b })
                            .collect::<Vec<_>>(),
                    );
                    self.reply(&data);
                }
                0x57 => {
                    let a = addr(&req);
                    let data = &req[6..];
                    let target = if a == SENTINEL {
                        let free = (CONFIG.0..=CONFIG.1 & !(PAGE as u32 - 1))
                            .step_by(PAGE)
                            .find(|&p| matches!(self.byte(p + PAGE as u32 - 1), 0xFF | 0x00));
                        match free {
                            Some(p) => p,
                            None => return Err(Error::Protocol("simulator: no free page".into())),
                        }
                    } else {
                        a
                    };
                    let page = target & !(PAGE as u32 - 1);
                    let off = (target - page) as usize;
                    self.page_mut(page)[off..off + data.len()].copy_from_slice(data);
                    self.writes += 1;
                    self.reply(b"\x06");
                }
                _ => {
                    return Err(Error::Protocol(format!(
                        "simulator: unknown request {:02x?}",
                        &req[..req.len().min(8)]
                    )));
                }
            }
        }
    }
}

impl Transport for SimRadio {
    fn send(&mut self, data: &[u8]) -> Result<()> {
        self.input.extend_from_slice(data);
        self.process()
    }

    fn recv(&mut self, buf: &mut [u8], _timeout: Duration) -> Result<()> {
        if self.output.len() < buf.len() {
            let got = self.output.drain(..).collect();
            return Err(Error::Timeout {
                wanted: buf.len(),
                got,
            });
        }
        for b in buf.iter_mut() {
            *b = self.output.pop_front().unwrap();
        }
        Ok(())
    }

    fn discard_input(&mut self) -> Result<()> {
        self.output.clear();
        Ok(())
    }

    fn delay(&mut self, _d: Duration) {}
}
