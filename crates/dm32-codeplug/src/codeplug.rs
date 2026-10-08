//! The whole codeplug as typed records, with edit operations that keep cross-references
//! (zone and scan-list members, channel → list indexes) consistent.

use crate::image::{Image, block};
use crate::records::analog::{self, Analog};
use crate::records::channel::{self, ChannelEntry, Channels};
use crate::records::emergency::{self, Emergency};
use crate::records::quick_contacts::{self, Source};
use crate::records::quick_text::{self, QuickTexts};
use crate::records::radio_id::{self, RadioIds};
use crate::records::roaming::{self, RoamChannels, RoamZones};
use crate::records::rx_group::{self, RxGroups};
use crate::records::scan_list::{self, ScanLists, ScanMember};
use crate::records::settings::{self, Settings};
use crate::records::talkgroup::{self, TalkGroups};
use crate::records::zone::{self, Zones};

#[derive(Debug, Clone, PartialEq)]
pub struct Codeplug {
    pub channels: Channels,
    pub zones: Zones,
    pub scan_lists: ScanLists,
    pub rx_groups: RxGroups,
    pub talkgroups: TalkGroups,
    pub radio_ids: RadioIds,
    pub quick_texts: QuickTexts,
    pub roam_channels: RoamChannels,
    pub roam_zones: RoamZones,
    pub emergency: Emergency,
    pub analog: Analog,
    pub settings: Option<Settings>,
}

impl Codeplug {
    pub fn decode(img: &Image) -> Codeplug {
        Codeplug {
            channels: channel::decode(img),
            zones: zone::decode(img),
            scan_lists: scan_list::decode(img),
            rx_groups: rx_group::decode(img),
            talkgroups: talkgroup::decode(img),
            radio_ids: radio_id::decode(img),
            quick_texts: quick_text::decode(img),
            roam_channels: roaming::decode_channels(img),
            roam_zones: roaming::decode_zones(img),
            emergency: emergency::decode(img),
            analog: analog::decode(img),
            settings: settings::decode(img),
        }
    }

    /// Write everything back into `img`. Blocks that did not change come out byte-identical.
    pub fn encode(&self, img: &mut Image) {
        channel::encode(&self.channels, img);
        zone::encode(&self.zones, img);
        scan_list::encode(&self.scan_lists, img);
        rx_group::encode(&self.rx_groups, img);
        let talkgroups_changed = talkgroup::decode(img) != self.talkgroups;
        talkgroup::encode(&self.talkgroups, img);
        // The quick-access index (block 0x0B) is derived from the talk groups.
        if talkgroups_changed || !img.blocks.contains_key(&block::QUICK_CONTACTS) {
            let names: Vec<(usize, String, u32, _)> = self
                .talkgroups
                .iter()
                .map(|(i, t)| (i, t.name(), t.id(), t.call_type()))
                .collect();
            let sources: Vec<Source> = names
                .iter()
                .map(|(i, name, id, ct)| Source {
                    // `iter` yields 1-based slot numbers, which is what the index stores.
                    slot: *i as u16,
                    name,
                    number: *id,
                    call_type: quick_contacts::CallType::from_raw(ct.to_raw()),
                })
                .collect();
            quick_contacts::encode(&quick_contacts::QuickContacts::build(&sources), img);
        }
        radio_id::encode(&self.radio_ids, img);
        quick_text::encode(&self.quick_texts, img);
        roaming::encode_channels(&self.roam_channels, img);
        roaming::encode_zones(&self.roam_zones, img);
        emergency::encode(&self.emergency, img);
        analog::encode(&self.analog, img);
        if let Some(s) = &self.settings {
            settings::encode(s, img);
        }
    }

    /// Renumber channel references. `map` takes an old 1-based channel number and returns the
    /// new one, or `None` if the channel is gone.
    fn remap_channels(&mut self, map: impl Fn(u16) -> Option<u16>) {
        for z in &mut self.zones.zones {
            let chans: Vec<u16> = z.channels().into_iter().filter_map(&map).collect();
            if chans != z.channels() {
                z.set_channels(&chans);
            }
        }
        for l in &mut self.scan_lists.lists {
            let old = l.members();
            let new: Vec<ScanMember> = old
                .iter()
                .filter_map(|m| match *m {
                    ScanMember::Selected => Some(ScanMember::Selected),
                    ScanMember::Channel(c) => map(c).map(ScanMember::Channel),
                })
                .collect();
            if new != old {
                l.set_members(&new);
            }
            // Priority and designated TX channels: 1-based channel numbers, 0 = none.
            let fix = |v: u16| {
                if v == 0 || v == 0xFFFF {
                    v
                } else {
                    map(v).unwrap_or(0)
                }
            };
            let (p1, tx, p2) = (
                l.priority1_channel(),
                l.designated_tx_channel(),
                l.priority2_channel(),
            );
            if fix(p1) != p1 {
                l.set_priority1_channel(fix(p1));
            }
            if fix(tx) != tx {
                l.set_designated_tx_channel(fix(tx));
            }
            if fix(p2) != p2 {
                l.set_priority2_channel(fix(p2));
            }
        }
    }

    /// Insert a channel at 0-based position `at`, shifting later channels up by one.
    pub fn insert_channel(&mut self, at: usize, entry: ChannelEntry) {
        let at = at.min(self.channels.list.len());
        self.channels.list.insert(at, entry);
        let n = at as u16 + 1;
        self.remap_channels(|c| Some(if c >= n { c + 1 } else { c }));
    }

    /// Delete the channel at 0-based position `at`; references to it are removed.
    pub fn delete_channel(&mut self, at: usize) {
        if at >= self.channels.list.len() {
            return;
        }
        self.channels.list.remove(at);
        let n = at as u16 + 1;
        self.remap_channels(|c| match c.cmp(&n) {
            std::cmp::Ordering::Less => Some(c),
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater => Some(c - 1),
        });
    }

    /// Move the channel at `from` to position `to` (both 0-based), keeping references.
    pub fn move_channel(&mut self, from: usize, to: usize) {
        let len = self.channels.list.len();
        if from >= len || to >= len || from == to {
            return;
        }
        let e = self.channels.list.remove(from);
        self.channels.list.insert(to, e);
        let (f, t) = (from as u16 + 1, to as u16 + 1);
        self.remap_channels(|c| {
            Some(if c == f {
                t
            } else if f < t && c > f && c <= t {
                c - 1
            } else if t < f && c >= t && c < f {
                c + 1
            } else {
                c
            })
        });
    }

    /// Delete scan list `at` (0-based); channels pointing to later lists are renumbered.
    pub fn delete_scan_list(&mut self, at: usize) {
        if at >= self.scan_lists.lists.len() {
            return;
        }
        self.scan_lists.lists.remove(at);
        let at = at as u8;
        for e in &mut self.channels.list {
            match e.channel.scan_list() {
                Some(i) if i == at => e.channel.set_scan_list(None),
                Some(i) if i > at => e.channel.set_scan_list(Some(i - 1)),
                _ => {}
            }
        }
    }

    /// Delete the RX group in slot `at`; channels using it get none.
    pub fn delete_rx_group(&mut self, at: usize) {
        if let Some(s) = self.rx_groups.slots.get_mut(at) {
            *s = None;
        }
        for e in &mut self.channels.list {
            if e.channel.rx_group_list() == Some(Some(at as u8)) {
                e.channel.set_rx_group_list(None);
            }
        }
    }

    /// Delete the talk group in slot `at`; channels transmitting to it get none.
    pub fn delete_talkgroup(&mut self, at: usize) {
        if let Some(s) = self.talkgroups.slots.get_mut(at) {
            *s = None;
        }
        for e in &mut self.channels.list {
            if e.tx_contact.index() == Some(at as u16) {
                e.tx_contact.set_index(None);
            }
        }
    }

    /// Delete radio ID `at` (0-based); channels using it fall back to the default ID.
    pub fn delete_radio_id(&mut self, at: usize) {
        if at >= self.radio_ids.ids.len() {
            return;
        }
        self.radio_ids.ids.remove(at);
        let at = at as u8;
        for e in &mut self.channels.list {
            match e.channel.radio_id() {
                Some(i) if i == at => e.channel.set_radio_id(None),
                Some(i) if i > at => e.channel.set_radio_id(Some(i - 1)),
                _ => {}
            }
        }
    }

    /// Problems that would make the codeplug unusable or that the radio would reject.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        let n = self.channels.list.len();
        if n > channel::MAX_CHANNELS {
            out.push(format!(
                "{n} channels; the radio holds at most {}",
                channel::MAX_CHANNELS
            ));
        }
        if self.zones.zones.len() > zone::MAX_ZONES {
            out.push(format!(
                "{} zones; the radio holds at most {}",
                self.zones.zones.len(),
                zone::MAX_ZONES
            ));
        }
        for z in &self.zones.zones {
            if z.channels().len() > zone::MAX_CHANNELS {
                out.push(format!(
                    "zone '{}' has more than {} channels",
                    z.name(),
                    zone::MAX_CHANNELS
                ));
            }
            for c in z.channels() {
                if c as usize > n || self.channels.list[c as usize - 1].is_empty() {
                    out.push(format!("zone '{}' refers to empty channel {c}", z.name()));
                }
            }
        }
        for l in &self.scan_lists.lists {
            if l.members().len() > scan_list::MAX_CHANNELS + 1 {
                out.push(format!(
                    "scan list '{}' has more than {} channels",
                    l.name(),
                    scan_list::MAX_CHANNELS
                ));
            }
        }
        if self.talkgroups.iter().next().is_none() {
            out.push("there are no digital contacts (talk groups); the radio is reported to misbehave without at least one".into());
        }
        let in_zone: std::collections::HashSet<u16> =
            self.zones.zones.iter().flat_map(|z| z.channels()).collect();
        let orphans: Vec<String> = (0..n)
            .filter(|&i| !self.channels.list[i].is_empty() && !in_zone.contains(&(i as u16 + 1)))
            .map(|i| (i + 1).to_string())
            .collect();
        if !orphans.is_empty() {
            let shown = orphans
                .iter()
                .take(20)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            let more = if orphans.len() > 20 {
                format!(" and {} more", orphans.len() - 20)
            } else {
                String::new()
            };
            out.push(format!(
                "{} channel(s) are in no zone, so the radio cannot select them: {shown}{more}",
                orphans.len()
            ));
        }
        for (i, e) in self.channels.list.iter().enumerate() {
            if e.is_empty() {
                continue;
            }
            if let Some(Some(g)) = e.rx_group_list()
                && self
                    .rx_groups
                    .slots
                    .get(g as usize)
                    .is_none_or(|s| s.is_none())
            {
                out.push(format!(
                    "channel {} '{}' uses a missing RX group",
                    i + 1,
                    e.name()
                ));
            }
            if let Some(Some(k)) = e.encryption_key()
                && k as usize >= emergency::KEY_SLOTS
            {
                out.push(format!(
                    "channel {} '{}' uses encryption key {}, but only {} keys are supported",
                    i + 1,
                    e.name(),
                    k + 1,
                    emergency::KEY_SLOTS
                ));
            }
            if let Some(t) = e.tx_contact.index()
                && e.mode().is_digital()
                && self
                    .talkgroups
                    .slots
                    .get(t as usize)
                    .is_none_or(|s| s.is_none())
            {
                out.push(format!(
                    "channel {} '{}' transmits to a missing contact",
                    i + 1,
                    e.name()
                ));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Freq;
    use crate::records::channel::ChannelMode;

    fn fixture() -> Image {
        let bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/factory-v1.60.data"
        ))
        .unwrap();
        crate::image::data_file::import(&bytes).unwrap()
    }

    #[test]
    fn round_trip_is_byte_identical() {
        let img = fixture();
        let cp = Codeplug::decode(&img);
        let mut out = img.clone();
        cp.encode(&mut out);
        for (id, page) in &img.blocks {
            assert!(out.blocks[id][..] == page[..], "block {id:#04x} changed");
        }
        assert_eq!(out.blocks.len(), img.blocks.len());
    }

    #[test]
    fn channel_edits_keep_references() {
        let img = fixture();
        let mut cp = Codeplug::decode(&img);
        let zone0 = cp.zones.zones[0].channels();
        assert_eq!(zone0[0], 1);
        cp.insert_channel(
            0,
            ChannelEntry::new("New", Freq(14_500_000), ChannelMode::Analog),
        );
        assert_eq!(cp.zones.zones[0].channels()[0], 2);
        cp.delete_channel(1);
        assert!(
            !cp.zones.zones[0].channels().contains(&1)
                || zone0.len() == cp.zones.zones[0].channels().len()
        );
        cp.delete_channel(0);
        assert_eq!(
            cp.zones.zones[0].channels(),
            zone0
                .iter()
                .filter(|&&c| c != 1)
                .map(|c| c - 1)
                .collect::<Vec<_>>()
        );

        let mut cp = Codeplug::decode(&img);
        let names: Vec<String> = cp.channels.list.iter().map(|e| e.name()).collect();
        cp.move_channel(0, 4);
        assert_eq!(cp.channels.list[4].name(), names[0]);
        // Zone 1 lists channels 1..; the old channel 1 is now number 5.
        assert!(cp.zones.zones[0].channels().contains(&5));
        let mut out = img.clone();
        cp.encode(&mut out);
        assert_eq!(Codeplug::decode(&out), cp);
    }

    #[test]
    fn talkgroup_edit_rebuilds_quick_index() {
        let img = fixture();
        let mut cp = Codeplug::decode(&img);
        cp.delete_talkgroup(0);
        let mut out = img.clone();
        cp.encode(&mut out);
        let q = quick_contacts::decode(&out);
        assert_eq!(q.count(), cp.talkgroups.iter().count());
    }

    #[test]
    fn quick_index_rebuild_matches_factory() {
        let img = fixture();
        let mut cp = Codeplug::decode(&img);
        // Force a rebuild by editing and restoring a talk group.
        let orig = cp.talkgroups.clone();
        cp.talkgroups.slots[0] = None;
        let mut out = img.clone();
        cp.encode(&mut out);
        cp.talkgroups = orig;
        cp.encode(&mut out);
        assert!(out.blocks[&block::QUICK_CONTACTS][..] == img.blocks[&block::QUICK_CONTACTS][..]);
    }

    #[test]
    fn channels_outside_zones_are_reported() {
        let mut cp = Codeplug::decode(&fixture());
        cp.channels.list.push(ChannelEntry::new(
            "Lonely",
            Freq(14_500_000),
            ChannelMode::Analog,
        ));
        let p = cp.problems();
        assert_eq!(p.len(), 1);
        assert!(
            p[0].contains("in no zone") && p[0].ends_with(": 26"),
            "{p:?}"
        );
        let mut chans = cp.zones.zones[0].channels();
        chans.push(26);
        cp.zones.zones[0].set_channels(&chans);
        assert!(cp.problems().is_empty());
    }

    #[test]
    fn factory_codeplug_has_no_problems() {
        let cp = Codeplug::decode(&fixture());
        assert_eq!(cp.problems(), Vec::<String>::new());
    }
}
