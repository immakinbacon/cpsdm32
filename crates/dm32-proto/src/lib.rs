//! Serial programming protocol for the Baofeng DM-32UV (model string `DP570UV`).
//!
//! The radio is programmed over a USB-serial cable at 115200 8N1. A [`Session`] performs the
//! handshake, enters programming mode and then reads/writes 4 KB flash pages. There is no exit
//! command: closing the port (dropping DTR) resets the radio.

mod session;
pub mod sim;
mod transport;

pub use session::{PAGE, RadioInfo, Range, SENTINEL, Session, Timing, vframe};
pub use transport::{
    LineConfig, LineLevel, ReplayTransport, SerialTransport, TracingTransport, Transport,
    list_ports,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("serial port: {0}")]
    Port(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("timed out waiting for {wanted} bytes (got {} bytes)", got.len())]
    Timeout { wanted: usize, got: Vec<u8> },
    #[error(
        "no reply from the radio; switch it on normally with no buttons held (PTT + SK1 at power-on is firmware update mode, which does not answer), push the cable in fully, and power-cycle it if it was used in another session"
    )]
    NoReply,
    #[error("unsupported radio model '{0}'")]
    UnsupportedModel(String),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("cancelled")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, Error>;
