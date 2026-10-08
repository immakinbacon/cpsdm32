//! Full read → edit → write → verify cycles against the simulated radio.

use std::sync::atomic::AtomicBool;

use dm32_codeplug::radio::{self, Ctx, ReadOptions, WriteOptions};
use dm32_codeplug::records::channel::{ChannelEntry, ChannelMode};
use dm32_codeplug::records::contact::{self, Contact, ContactDb};
use dm32_codeplug::{Codeplug, Freq, Image};
use dm32_proto::sim::SimRadio;
use dm32_proto::{Session, Timing};

fn fixture() -> Image {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/factory-v1.60.data"
    ))
    .unwrap();
    dm32_codeplug::image::data_file::import(&bytes).unwrap()
}

/// A radio holding the fixture, with blocks on scattered pages.
fn radio_with(img: &Image) -> SimRadio {
    let mut r = SimRadio::new();
    for (i, (_, page)) in img.blocks.iter().enumerate() {
        r.put_block(0x001000 + ((i as u32 * 7) % 190) * 0x1000, &page[..]);
    }
    for (off, page) in &img.contacts {
        r.put_block(0x278000 + off, &page[..]);
    }
    r
}

fn session(r: SimRadio) -> Session<SimRadio> {
    Session::new(r, Timing::none())
}

fn ctx_run<T>(f: impl FnOnce(&mut Ctx) -> dm32_proto::Result<T>) -> T {
    let cancel = AtomicBool::new(false);
    let mut p = |_| {};
    f(&mut Ctx {
        progress: &mut p,
        cancel: &cancel,
    })
    .unwrap()
}

fn read(r: SimRadio, contacts: bool) -> (Image, SimRadio) {
    let mut s = session(r);
    let img = ctx_run(|cx| {
        radio::read(
            &mut s,
            ReadOptions {
                contacts,
                oem_compat: true,
            },
            cx,
        )
    });
    (img, s.into_transport())
}

#[test]
fn read_returns_what_the_radio_holds() {
    let fx = fixture();
    let (img, _) = read(radio_with(&fx), true);
    for (id, p) in &fx.blocks {
        assert!(img.blocks[id][..] == p[..], "block {id:#04x}");
    }
    assert_eq!(contact::decode(&img).list.len(), 1);
}

#[test]
fn write_grows_codeplug_and_verifies() {
    let fx = fixture();
    let (mut img, radio) = read(radio_with(&fx), false);

    let mut cp = Codeplug::decode(&img);
    for i in cp.channels.list.len()..300 {
        cp.channels.list.push(ChannelEntry::new(
            &format!("CH{i}"),
            Freq(43_000_000 + i as u32 * 1250),
            ChannelMode::Analog,
        ));
    }
    cp.channels.list[0].channel.set_name("First");
    cp.encode(&mut img);

    let opts = WriteOptions {
        verify: true,
        ..Default::default()
    };
    let mut s = session(radio);
    let prepared = ctx_run(|cx| radio::prepare(&mut s, &img, opts, cx));
    assert!(
        prepared.writes.iter().any(|w| w.addr == radio::SENTINEL),
        "new slots need allocation"
    );
    assert!(
        prepared.before.blocks[&0x12][..] == fx.blocks[&0x12][..],
        "backup holds the old radio contents"
    );
    let report = ctx_run(|cx| radio::execute(&mut s, &prepared, opts, cx));
    assert!(report.mismatches.is_empty(), "{:?}", report.mismatches);

    // A fresh session (the radio was reset by closing the port) reads back the new codeplug.
    let (back, _) = read(s.into_transport(), false);
    assert_eq!(Codeplug::decode(&back), cp);
}

#[test]
fn unchanged_write_is_a_no_op() {
    let fx = fixture();
    let (img, radio) = read(radio_with(&fx), false);
    let mut s = session(radio);
    let prepared = ctx_run(|cx| radio::prepare(&mut s, &img, WriteOptions::default(), cx));
    assert!(prepared.writes.is_empty());
}

#[test]
fn contact_database_write() {
    let fx = fixture();
    let (mut img, radio) = read(radio_with(&fx), true);
    let db = ContactDb {
        list: (1..=500)
            .map(|i| Contact::new(3_100_000 + i, &format!("Op {i}")))
            .collect(),
    };
    contact::encode(&db, &mut img);
    let opts = WriteOptions {
        contacts: true,
        verify: true,
        ..Default::default()
    };
    let mut s = session(radio);
    let prepared = ctx_run(|cx| radio::prepare(&mut s, &img, opts, cx));
    let report = ctx_run(|cx| radio::execute(&mut s, &prepared, opts, cx));
    assert!(report.mismatches.is_empty());
    let (back, _) = read(s.into_transport(), true);
    assert_eq!(contact::decode(&back).list.len(), 500);
}
