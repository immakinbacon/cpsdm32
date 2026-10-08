//! Codeplug model for the Baofeng DM-32UV.
//!
//! [`Image`] is the raw memory (logical 4 KB blocks); record types wrap the raw bytes of one
//! record and expose typed accessors, so fields that are not understood yet survive a
//! read-modify-write unchanged.

pub mod codeplug;
pub mod encoding;
pub mod image;
pub mod radio;
pub mod records;

pub use codeplug::Codeplug;
pub use encoding::{Freq, Tone};
pub use image::{Image, ImageInfo, PAGE, Page, block};
