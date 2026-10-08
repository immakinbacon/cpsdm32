//! Typed views of the codeplug records.
//!
//! # Pattern
//!
//! Every record type is a newtype over its raw bytes (`raw: [u8; SIZE]`) with getter/setter
//! pairs for the fields that are understood. Setters change only the bits of their own field,
//! so unknown bytes and bits survive a decode → edit → encode cycle unchanged.
//!
//! Each block module provides `decode(&Image) -> T` and `encode(&T, &mut Image)`. `encode`
//! starts from the block's current bytes (read-modify-write), rewrites what the module
//! understands and leaves everything else in the block untouched.
//!
//! Multiple-choice fields are Rust enums implementing [`Choice`], which gives the GUI the list
//! of options and their labels (matching the official CPS wording where known). Values the
//! radio stores that are not a known option round-trip through an `Other(u8)` variant.

pub mod analog;
pub mod call_list;
pub mod channel;
pub mod contact;
pub mod emergency;
pub mod quick_contacts;
pub mod quick_text;
pub mod radio_id;
pub mod roaming;
pub mod rx_group;
pub mod scan_list;
pub mod settings;
pub mod talkgroup;
pub mod zone;

/// A multiple-choice field.
pub trait Choice: Sized + Copy + PartialEq + 'static {
    /// The known options, in the order the CPS lists them.
    fn options() -> &'static [Self];
    /// Human-readable label.
    fn label(&self) -> String;
}

/// Declare a [`Choice`] enum stored as a small integer, with an `Other(u8)` catch-all.
///
/// ```ignore
/// choice! { pub enum Power { Low = 0 => "Low", High = 1 => "High" } }
/// ```
#[macro_export]
macro_rules! choice {
    ($(#[$m:meta])* pub enum $name:ident { $($var:ident = $val:literal => $label:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name { $($var,)+ Other(u8) }

        impl $name {
            pub fn from_raw(v: u8) -> Self {
                match v { $($val => $name::$var,)+ v => $name::Other(v) }
            }
            pub fn to_raw(self) -> u8 {
                match self { $($name::$var => $val,)+ $name::Other(v) => v }
            }
        }

        impl $crate::records::Choice for $name {
            fn options() -> &'static [Self] {
                &[$($name::$var),+]
            }
            fn label(&self) -> String {
                match self { $($name::$var => $label.to_string(),)+ $name::Other(v) => format!("Unknown ({v})") }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&$crate::records::Choice::label(self))
            }
        }
    };
}

/// Implement `Default` (all `fill`), `from_bytes` and `bytes` for a raw-backed record.
#[macro_export]
macro_rules! raw_record {
    ($name:ident, $size:expr, $fill:expr) => {
        impl $name {
            pub const SIZE: usize = $size;

            pub fn from_bytes(b: &[u8]) -> Self {
                let mut raw = [$fill; $size];
                raw.copy_from_slice(&b[..$size]);
                $name { raw }
            }

            pub fn bytes(&self) -> &[u8; $size] {
                &self.raw
            }
        }

        impl Default for $name {
            fn default() -> Self {
                $name {
                    raw: [$fill; $size],
                }
            }
        }
    };
}
