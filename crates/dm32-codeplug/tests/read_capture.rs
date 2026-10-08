use std::sync::atomic::AtomicBool;

use dm32_codeplug::encoding::is_blank;
use dm32_codeplug::image::data_file;
use dm32_codeplug::radio::{self, Ctx, ReadOptions};
use dm32_codeplug::{Image, block};
use dm32_proto::{ReplayTransport, Session, Timing};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn read_capture() -> Image {
    let text = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../dm32-proto/tests/fixtures/oem_read_capture.txt"
    ))
    .unwrap();
    let replay = ReplayTransport::from_port_monitor(&String::from_utf8_lossy(&text));
    let mut s = Session::new(replay, Timing::none());
    let cancel = AtomicBool::new(false);
    let mut progress = |_| {};
    let mut cx = Ctx {
        progress: &mut progress,
        cancel: &cancel,
    };
    let img = radio::read(
        &mut s,
        ReadOptions {
            contacts: true,
            oem_compat: true,
        },
        &mut cx,
    )
    .unwrap();
    assert_eq!(
        s.into_transport().remaining_tx(),
        0,
        "replay not fully consumed"
    );
    img
}

#[test]
fn read_whole_codeplug_from_oem_capture() {
    let img = read_capture();
    assert_eq!(img.info.firmware, "DM32.01.01.046");
    assert_eq!(img.blocks.len(), 40);
    assert_eq!(img.contacts.len(), 1);
    assert_eq!(&img.blocks[&0x12][0x10..0x19], b"Channel 1");
}

/// The factory `.data` saved by CPS v1.60 holds the same codeplug as the read capture.
#[test]
fn data_file_matches_radio_read() {
    let radio = read_capture();
    let file = data_file::import(&fixture("factory-v1.60.data")).unwrap();
    for id in block::read_walk() {
        let (a, b) = (radio.blocks.get(&id), file.blocks.get(&id));
        match (a, b) {
            // Blank slots are zero-filled on one radio and 0xFF-filled in the file.
            (Some(a), Some(b)) if is_blank(&a[..4095]) && is_blank(&b[..4095]) => {}
            (Some(a), Some(b)) => {
                let diffs: Vec<String> = a
                    .iter()
                    .zip(b.iter())
                    .enumerate()
                    .filter(|(_, (x, y))| x != y)
                    .map(|(i, (x, y))| format!("{i:#05x}:{x:02x}/{y:02x}"))
                    .collect();
                let diff = diffs.len();
                if diff > 0 {
                    eprintln!("block {id:#04x}: {:?}", &diffs[..diff.min(8)]);
                }
                // Different radios: calibration and settings differ, plus a few state bytes
                // (current zone/channel, one channel flag).
                let allowed = if id == block::CALIBRATION || id == block::SETTINGS {
                    128
                } else {
                    4
                };
                assert!(diff <= allowed, "block {id:#04x}: {diff} bytes differ");
            }
            (None, None) => {}
            // Allocated but blank on the radio, absent in the file (or vice versa).
            (Some(p), None) | (None, Some(p)) => {
                assert!(
                    p[..4095].iter().all(|&b| b == 0 || b == 0xFF),
                    "block {id:#04x} only on one side"
                )
            }
        }
    }
}

#[test]
fn data_file_round_trip() {
    let bytes = fixture("factory-v1.60.data");
    let img = data_file::import(&bytes).unwrap();
    assert_eq!(img.info.firmware, "DM32.01.01.046");
    assert_eq!(data_file::export(&img), bytes);
}

#[test]
fn native_round_trip() {
    let img = read_capture();
    let mut buf = Vec::new();
    img.write_native(&mut buf).unwrap();
    assert_eq!(Image::read_native(&mut &buf[..]).unwrap(), img);
}
