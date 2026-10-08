//! Byte transports the session runs over: a real serial port, a tracing wrapper and a replay
//! transport that plays back a recorded session (used in tests).

use std::io::{self, Write};
use std::time::{Duration, Instant};

use crate::{Error, Result};

/// A bidirectional byte stream to the radio.
pub trait Transport: Send {
    /// Write all of `data`.
    fn send(&mut self, data: &[u8]) -> Result<()>;
    /// Fill `buf` completely, or fail with [`Error::Timeout`] after `timeout`.
    fn recv(&mut self, buf: &mut [u8], timeout: Duration) -> Result<()>;
    /// Drop any bytes the radio sent that have not been read yet.
    fn discard_input(&mut self) -> Result<()>;
    /// Wait. Replay transports override this to return immediately.
    fn delay(&mut self, d: Duration) {
        std::thread::sleep(d)
    }
}

impl<T: Transport + ?Sized> Transport for Box<T> {
    fn send(&mut self, data: &[u8]) -> Result<()> {
        (**self).send(data)
    }
    fn recv(&mut self, buf: &mut [u8], timeout: Duration) -> Result<()> {
        (**self).recv(buf, timeout)
    }
    fn discard_input(&mut self) -> Result<()> {
        (**self).discard_input()
    }
    fn delay(&mut self, d: Duration) {
        (**self).delay(d)
    }
}

/// Level of a modem control line while the port is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineLevel {
    /// Leave the line as the driver sets it on open.
    #[default]
    Default,
    High,
    Low,
}

impl std::str::FromStr for LineLevel {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "default" | "auto" => Ok(LineLevel::Default),
            "high" | "1" | "on" => Ok(LineLevel::High),
            "low" | "0" | "off" => Ok(LineLevel::Low),
            _ => Err(format!(
                "invalid line level '{s}', expected high, low or default"
            )),
        }
    }
}

/// RTS/DTR levels to drive while talking to the radio.
///
/// Firmware `NRF.01.049` is reported to need RTS high and DTR low; most others work with the
/// driver defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LineConfig {
    pub rts: LineLevel,
    pub dtr: LineLevel,
}

/// A serial port opened with the radio's line settings (115200 8N1, no flow control).
pub struct SerialTransport {
    port: Box<dyn serialport::SerialPort>,
}

impl SerialTransport {
    pub const BAUD: u32 = 115_200;

    pub fn open(path: &str, lines: LineConfig) -> Result<Self> {
        let mut port = serialport::new(path, Self::BAUD)
            .data_bits(serialport::DataBits::Eight)
            .parity(serialport::Parity::None)
            .stop_bits(serialport::StopBits::One)
            .flow_control(serialport::FlowControl::None)
            .timeout(Duration::from_millis(100))
            .open()
            .map_err(|e| Error::Port(format!("cannot open {path}: {e}")))?;
        match lines.rts {
            LineLevel::Default => {}
            l => port
                .write_request_to_send(l == LineLevel::High)
                .map_err(|e| Error::Port(format!("cannot set RTS: {e}")))?,
        }
        match lines.dtr {
            LineLevel::Default => {}
            l => port
                .write_data_terminal_ready(l == LineLevel::High)
                .map_err(|e| Error::Port(format!("cannot set DTR: {e}")))?,
        }
        Ok(SerialTransport { port })
    }

    /// Drop DTR, which resets the radio out of programming mode, then close the port.
    pub fn close_with_reset(mut self) {
        let _ = self.port.write_data_terminal_ready(false);
        std::thread::sleep(Duration::from_millis(100));
    }
}

impl Transport for SerialTransport {
    fn send(&mut self, data: &[u8]) -> Result<()> {
        self.port.write_all(data)?;
        self.port.flush()?;
        Ok(())
    }

    fn recv(&mut self, buf: &mut [u8], timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        let mut got = 0;
        while got < buf.len() {
            match self.port.read(&mut buf[got..]) {
                Ok(0) => {}
                Ok(n) => got += n,
                Err(e) if e.kind() == io::ErrorKind::TimedOut => {}
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e.into()),
            }
            if got < buf.len() && Instant::now() >= deadline {
                return Err(Error::Timeout {
                    wanted: buf.len(),
                    got: buf[..got].to_vec(),
                });
            }
        }
        Ok(())
    }

    fn discard_input(&mut self) -> Result<()> {
        self.port
            .clear(serialport::ClearBuffer::Input)
            .map_err(|e| Error::Port(e.to_string()))
    }
}

/// List serial ports that could be a programming cable.
pub fn list_ports() -> Vec<String> {
    let mut ports: Vec<String> = serialport::available_ports()
        .map(|v| v.into_iter().map(|p| p.port_name).collect())
        .unwrap_or_default();
    // Without libudev, enumeration can miss USB adapters; add the usual device nodes.
    if let Ok(dir) = std::fs::read_dir("/dev") {
        for entry in dir.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("ttyUSB") || name.starts_with("ttyACM") {
                let path = format!("/dev/{name}");
                if !ports.contains(&path) {
                    ports.push(path);
                }
            }
        }
    }
    ports.retain(|p| !p.starts_with("/dev/ttyS"));
    ports.sort();
    ports
}

/// Wraps a transport and logs every byte sent and received as hex lines.
///
/// Lines look like `+0.123 TX 50 53 45 ...`. The log can be fed back into
/// [`ReplayTransport::from_trace`].
pub struct TracingTransport<T> {
    inner: T,
    out: Box<dyn Write + Send>,
    start: Instant,
}

impl<T: Transport> TracingTransport<T> {
    pub fn new(inner: T, out: Box<dyn Write + Send>) -> Self {
        TracingTransport {
            inner,
            out,
            start: Instant::now(),
        }
    }

    pub fn into_inner(self) -> T {
        self.inner
    }

    fn log(&mut self, dir: &str, data: &[u8]) {
        let t = self.start.elapsed().as_secs_f64();
        let _ = writeln!(self.out, "+{t:.3} {dir} {}", hex(data));
        let _ = self.out.flush();
    }
}

impl<T: Transport> Transport for TracingTransport<T> {
    fn send(&mut self, data: &[u8]) -> Result<()> {
        self.log("TX", data);
        self.inner.send(data)
    }

    fn recv(&mut self, buf: &mut [u8], timeout: Duration) -> Result<()> {
        let r = self.inner.recv(buf, timeout);
        match &r {
            Ok(()) => self.log("RX", buf),
            Err(Error::Timeout { got, wanted }) => {
                let got = got.clone();
                self.log("RX", &got);
                let t = self.start.elapsed().as_secs_f64();
                let _ = writeln!(
                    self.out,
                    "+{t:.3} !! timeout: wanted {wanted} bytes, got {}",
                    got.len()
                );
            }
            Err(e) => {
                let t = self.start.elapsed().as_secs_f64();
                let _ = writeln!(self.out, "+{t:.3} !! error: {e}");
            }
        }
        r
    }

    fn discard_input(&mut self) -> Result<()> {
        let t = self.start.elapsed().as_secs_f64();
        let _ = writeln!(self.out, "+{t:.3} -- discard input");
        self.inner.discard_input()
    }

    fn delay(&mut self, d: Duration) {
        self.inner.delay(d)
    }
}

/// Plays back a recorded session.
///
/// Every `send` must match the next recorded TX bytes exactly; `recv` returns the recorded RX
/// stream. Delays are skipped.
#[derive(Debug, Default)]
pub struct ReplayTransport {
    tx: Vec<u8>,
    tx_pos: usize,
    rx: Vec<u8>,
    rx_pos: usize,
    /// `(rx_end, tx_needed)`: RX bytes up to `rx_end` are released once `tx_needed` TX bytes
    /// were sent, so replies only arrive after the request that caused them.
    rx_gate: Vec<(usize, usize)>,
}

impl ReplayTransport {
    fn push(&mut self, dir: Dir, bytes: &[u8]) {
        match dir {
            Dir::Tx => {
                self.tx.extend_from_slice(bytes);
            }
            Dir::Rx => {
                // RX bytes become readable once all TX bytes recorded before them were sent.
                self.rx.extend_from_slice(bytes);
                self.rx_gate.push((self.rx.len(), self.tx.len()));
            }
        }
    }

    /// Parse a capture as written by the serial port monitor used for the spec captures
    /// (`Written data` / `Read data` headings followed by hex dump lines).
    pub fn from_port_monitor(text: &str) -> Self {
        let mut r = ReplayTransport::default();
        let mut dir = None;
        for line in text.lines() {
            if line.contains("] Written data") {
                dir = Some(Dir::Tx);
            } else if line.contains("] Read data") {
                dir = Some(Dir::Rx);
            } else if line.starts_with('[') || line.starts_with('#') {
                dir = None;
            } else if let Some(d) = dir {
                // Hex columns occupy the first 52 characters; the ASCII column starts at 54.
                let hexpart: String = line.chars().take(53).collect();
                let bytes = parse_hex(&hexpart);
                r.push(d, &bytes);
            }
        }
        r
    }

    /// Parse a trace written by [`TracingTransport`].
    pub fn from_trace(text: &str) -> Self {
        let mut r = ReplayTransport::default();
        for line in text.lines() {
            let mut parts = line.splitn(3, ' ');
            let (_t, dir, rest) = (parts.next(), parts.next(), parts.next().unwrap_or(""));
            match dir {
                Some("TX") => r.push(Dir::Tx, &parse_hex(rest)),
                Some("RX") => r.push(Dir::Rx, &parse_hex(rest)),
                _ => {}
            }
        }
        r
    }

    /// Number of recorded TX bytes not yet sent.
    pub fn remaining_tx(&self) -> usize {
        self.tx.len() - self.tx_pos
    }

    /// Number of RX bytes the radio has "sent" given how far the host has got.
    fn rx_released(&self) -> usize {
        self.rx_gate
            .iter()
            .take_while(|&&(_, tx_needed)| tx_needed <= self.tx_pos)
            .last()
            .map_or(0, |&(rx_end, _)| rx_end)
    }
}

#[derive(Clone, Copy)]
enum Dir {
    Tx,
    Rx,
}

impl Transport for ReplayTransport {
    fn send(&mut self, data: &[u8]) -> Result<()> {
        let end = self.tx_pos + data.len();
        let expected = self
            .tx
            .get(self.tx_pos..end.min(self.tx.len()))
            .unwrap_or(&[]);
        if expected != data {
            return Err(Error::Port(format!(
                "replay mismatch at TX byte {}: sent [{}], recorded [{}]",
                self.tx_pos,
                hex(&data[..data.len().min(16)]),
                hex(&expected[..expected.len().min(16)])
            )));
        }
        self.tx_pos = end;
        Ok(())
    }

    fn recv(&mut self, buf: &mut [u8], _timeout: Duration) -> Result<()> {
        let released = self.rx_released();
        if self.rx_pos + buf.len() > released {
            let got = self.rx[self.rx_pos..released.max(self.rx_pos)].to_vec();
            return Err(Error::Timeout {
                wanted: buf.len(),
                got,
            });
        }
        buf.copy_from_slice(&self.rx[self.rx_pos..self.rx_pos + buf.len()]);
        self.rx_pos += buf.len();
        Ok(())
    }

    fn discard_input(&mut self) -> Result<()> {
        Ok(())
    }

    fn delay(&mut self, _d: Duration) {}
}

pub(crate) fn hex(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len() * 3);
    for (i, b) in data.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn parse_hex(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .filter_map(|t| u8::from_str_radix(t, 16).ok().filter(|_| t.len() == 2))
        .collect()
}
