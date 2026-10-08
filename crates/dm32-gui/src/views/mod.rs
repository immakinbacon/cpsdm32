//! The editor views and the document they edit.
//!
//! Views edit `doc.cp` (the decoded codeplug) and return `true` when they changed something;
//! the caller then calls [`Doc::commit`], which records an undo step and re-encodes the
//! codeplug into the image.

#[macro_use]
pub mod widgets;

mod analog;
mod channels;
mod contacts;
mod emergency;
mod memory;
mod overview;
mod quick_texts;
mod radio_ids;
mod roaming;
mod rx_groups;
mod scan_lists;
mod settings;
mod talkgroups;
mod zones;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use dm32_codeplug::records::contact::{self, ContactDb};
use dm32_codeplug::{Codeplug, Image, Page};
use eframe::egui;

/// Maximum number of undo steps kept.
const UNDO_DEPTH: usize = 200;
/// Edits closer together than this are undone as one step (typing a name).
const COALESCE: Duration = Duration::from_millis(800);

type Snapshot = BTreeMap<u8, Page>;

/// An open codeplug.
pub struct Doc {
    pub image: Image,
    pub cp: Codeplug,
    /// Decoded contact database, loaded on first use (it can hold 50,000 entries).
    contacts: Option<ContactDb>,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    /// The contact database was edited and should be written to the radio.
    pub contacts_dirty: bool,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last_commit: Option<Instant>,
}

impl Doc {
    pub fn new(image: Image, path: Option<PathBuf>) -> Doc {
        let cp = Codeplug::decode(&image);
        Doc {
            image,
            cp,
            contacts: None,
            path,
            dirty: false,
            contacts_dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
            last_commit: None,
        }
    }

    /// Apply edits made to `cp` to the image, recording an undo step.
    pub fn commit(&mut self) {
        let coalesce =
            self.last_commit.is_some_and(|t| t.elapsed() < COALESCE) && !self.undo.is_empty();
        if !coalesce {
            self.undo.push(self.image.blocks.clone());
            if self.undo.len() > UNDO_DEPTH {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.cp.encode(&mut self.image);
        self.dirty = true;
        self.last_commit = Some(Instant::now());
    }

    pub fn contacts(&mut self) -> &mut ContactDb {
        self.contacts
            .get_or_insert_with(|| contact::decode(&self.image))
    }

    /// Apply edits made to the contact database (not undoable).
    pub fn commit_contacts(&mut self) {
        if let Some(db) = &self.contacts {
            contact::encode(db, &mut self.image);
            self.dirty = true;
            self.contacts_dirty = true;
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo
                .push(std::mem::replace(&mut self.image.blocks, prev));
            self.reload();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo
                .push(std::mem::replace(&mut self.image.blocks, next));
            self.reload();
        }
    }

    fn reload(&mut self) {
        self.cp = Codeplug::decode(&self.image);
        self.dirty = true;
        self.last_commit = None;
    }

    /// Display names of the channels, for pickers (`"12: Name"`).
    pub fn channel_names(&self) -> Vec<String> {
        self.cp
            .channels
            .list
            .iter()
            .map(|e| {
                if e.is_empty() {
                    "(empty)".into()
                } else {
                    e.name()
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Overview,
    Settings,
    Channels,
    Zones,
    ScanLists,
    Roaming,
    TalkGroups,
    RxGroups,
    Contacts,
    RadioIds,
    QuickTexts,
    Emergency,
    Analog,
    Memory,
}

const NAV: &[(&str, &[(View, &str)])] = &[
    (
        "Radio",
        &[
            (View::Overview, "Overview"),
            (View::Settings, "Radio settings"),
            (View::RadioIds, "Radio IDs"),
        ],
    ),
    (
        "Channels",
        &[
            (View::Channels, "Channels"),
            (View::Zones, "Zones"),
            (View::ScanLists, "Scan lists"),
            (View::Roaming, "Roaming"),
        ],
    ),
    (
        "Digital",
        &[
            (View::TalkGroups, "Digital contacts"),
            (View::RxGroups, "RX group lists"),
            (View::Contacts, "Contact database"),
            (View::QuickTexts, "Quick text messages"),
            (View::Emergency, "Emergency & encryption"),
        ],
    ),
    ("Analog", &[(View::Analog, "DTMF & analog contacts")]),
    ("Advanced", &[(View::Memory, "Memory blocks")]),
];

pub fn nav(ui: &mut egui::Ui, view: &mut View) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        for (group, items) in NAV {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(*group).strong());
            for (v, label) in *items {
                ui.selectable_value(view, *v, *label);
            }
        }
    });
}

pub fn show(ui: &mut egui::Ui, view: &mut View, doc: &mut Doc) {
    let changed = match view {
        View::Overview => overview::show(ui, doc),
        View::Channels => channels::show(ui, doc),
        View::Zones => zones::show(ui, doc),
        View::ScanLists => scan_lists::show(ui, doc),
        View::Roaming => roaming::show(ui, doc),
        View::TalkGroups => talkgroups::show(ui, doc),
        View::RxGroups => rx_groups::show(ui, doc),
        View::Contacts => contacts::show(ui, doc),
        View::RadioIds => radio_ids::show(ui, doc),
        View::QuickTexts => quick_texts::show(ui, doc),
        View::Settings => settings::show(ui, doc),
        View::Emergency => emergency::show(ui, doc),
        View::Analog => analog::show(ui, doc),
        View::Memory => {
            memory::show(ui, doc);
            false
        }
    };
    if changed {
        doc.commit();
    }
}
