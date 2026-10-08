use dm32_proto::{Range, ReplayTransport, Session, Timing};

fn oem_read() -> Session<ReplayTransport> {
    let text = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/oem_read_capture.txt"
    ))
    .unwrap();
    let replay = ReplayTransport::from_port_monitor(&String::from_utf8_lossy(&text));
    Session::new(replay, Timing::none())
}

#[test]
fn identify_from_oem_capture() {
    let mut s = oem_read();
    let info = s.identify().unwrap().clone();
    assert_eq!(info.model, "DP570UV");
    assert_eq!(info.firmware, "DM32.01.01.046");
    assert_eq!(info.build_date, "2022-06-27");
    assert_eq!(info.codeplug_version, "C1.00.01.001");
    assert_eq!(
        info.config,
        Some(Range {
            start: 0x001000,
            end: 0x0C8FFF
        })
    );
    assert_eq!(
        info.contacts,
        Some(Range {
            start: 0x278000,
            end: 0x6DBFFF
        })
    );
    assert_eq!(
        info.boot_image,
        Some(Range {
            start: 0x150000,
            end: 0x175FFF
        })
    );
    assert_eq!(info.max_contacts, Some(50_000));
    assert_eq!(info.vframes[&0x0D].len(), 64);
}

#[test]
fn program_mode_scan_and_read_from_oem_capture() {
    let mut s = oem_read();
    let config = s.identify().unwrap().config.unwrap();
    s.enter_program().unwrap();
    assert_eq!(s.info().unwrap().vframes[&0x47].len(), 256);

    let count = s.read(0x278000, 4).unwrap();
    assert_eq!(count, [1, 0, 0, 0]);

    let ids = s.scan_block_ids(config, |_, _| {}).unwrap();
    assert_eq!(ids.len(), 200);
    let addr_of = |id| ids.iter().find(|&&(_, i)| i == id).map(|&(a, _)| a);
    assert_eq!(addr_of(0x07), Some(0x001000));
    assert_eq!(addr_of(0x12), Some(0x0AA000));
    assert_eq!(addr_of(0x04), Some(0x026000));

    let page = s.read_page(0x0AA000).unwrap();
    assert_eq!(page.len(), 4096);
    assert_eq!(u16::from_le_bytes([page[0], page[1]]), 25);
    assert_eq!(&page[0x10..0x19], b"Channel 1");
    assert_eq!(page[0xFFF], 0x12);
}

#[test]
fn replay_rejects_unexpected_request() {
    let mut s = oem_read();
    s.identify().unwrap();
    s.set_oem_compat(false);
    // Without the 0x47 probe the next bytes differ from what the OEM CPS sent.
    assert!(s.enter_program().is_err());
}
