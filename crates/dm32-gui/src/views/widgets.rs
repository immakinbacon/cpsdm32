//! Small editing widgets shared by the views. Each returns `true` when the value changed.

use dm32_codeplug::records::Choice;
use dm32_codeplug::{Freq, Tone};
use eframe::egui;

/// A drop-down for a [`Choice`] field.
pub fn choice<T: Choice>(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    value: &mut T,
) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .selected_text(value.label())
        .show_ui(ui, |ui| {
            for opt in T::options() {
                changed |= ui.selectable_value(value, *opt, opt.label()).changed();
            }
        });
    changed
}

/// A drop-down for an optional list reference (`None` shown as `none_label`).
pub fn reference(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    value: &mut Option<u8>,
    names: &[String],
    none_label: &str,
) -> bool {
    let label = |v: &Option<u8>| match v {
        None => none_label.to_string(),
        Some(i) => names
            .get(*i as usize)
            .map_or(format!("#{}", i + 1), |n| format!("{}: {n}", i + 1)),
    };
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .selected_text(label(value))
        .show_ui(ui, |ui| {
            changed |= ui.selectable_value(value, None, none_label).changed();
            for i in 0..names.len().min(256) {
                let v = Some(i as u8);
                changed |= ui.selectable_value(value, v, label(&v)).changed();
            }
        });
    changed
}

/// Text field that commits a parsed value when it is valid and focus leaves the field.
fn parsed<T: Clone + PartialEq + std::str::FromStr>(
    ui: &mut egui::Ui,
    id: egui::Id,
    value: &mut T,
    show: impl Fn(&T) -> String,
    width: f32,
) -> bool {
    let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| show(value));
    let resp = ui.add(egui::TextEdit::singleline(&mut text).desired_width(width));
    let parsed = text.parse::<T>();
    if resp.has_focus() || resp.changed() {
        ui.data_mut(|d| d.insert_temp(id, text.clone()));
        if parsed.is_err() {
            resp.clone().on_hover_text("Invalid value");
        }
    }
    // Apply every valid edit at once, so a value typed just before writing to the radio is
    // never lost; an invalid entry is discarded when the field loses focus.
    let mut changed = false;
    if resp.changed()
        && let Ok(v) = parsed
        && v != *value
    {
        *value = v;
        changed = true;
    }
    if !resp.has_focus() {
        ui.data_mut(|d| d.remove::<String>(id));
    }
    changed
}

pub fn freq(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    value: &mut Freq,
) -> bool {
    parsed(
        ui,
        egui::Id::new(("freq", id)),
        value,
        |f| f.to_string(),
        90.0,
    )
}

/// Optional frequency: empty text means `None` (receive only).
pub fn opt_freq(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    value: &mut Option<Freq>,
) -> bool {
    #[derive(Clone, PartialEq)]
    struct OptFreq(Option<Freq>);
    impl std::str::FromStr for OptFreq {
        type Err = String;
        fn from_str(s: &str) -> Result<Self, String> {
            if s.trim().is_empty() || s.trim() == "-" {
                Ok(OptFreq(None))
            } else {
                s.parse().map(|f| OptFreq(Some(f)))
            }
        }
    }
    let mut v = OptFreq(*value);
    let changed = parsed(
        ui,
        egui::Id::new(("ofreq", id)),
        &mut v,
        |f| f.0.map_or(String::new(), |f| f.to_string()),
        90.0,
    );
    *value = v.0;
    changed
}

/// Tone selector: drop-down of standard CTCSS/DCS values plus free text.
pub fn tone(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug + Copy,
    value: &mut Tone,
) -> bool {
    use dm32_codeplug::encoding::{CTCSS_TONES, DCS_CODES};
    let mut changed = false;
    egui::ComboBox::from_id_salt(("tone", id))
        .selected_text(value.to_string())
        .height(300.0)
        .show_ui(ui, |ui| {
            changed |= ui.selectable_value(value, Tone::Off, "Off").changed();
            for t in CTCSS_TONES {
                let v = Tone::Ctcss(t);
                changed |= ui.selectable_value(value, v, v.to_string()).changed();
            }
            for inverted in [false, true] {
                for code in DCS_CODES {
                    let v = Tone::Dcs { code, inverted };
                    changed |= ui.selectable_value(value, v, v.to_string()).changed();
                }
            }
        });
    changed
}

/// Single-line text limited to printable ASCII and `max` characters. While the field has
/// focus the typed text is kept as-is (stored strings drop trailing spaces).
pub fn name(ui: &mut egui::Ui, value: &mut String, max: usize, width: f32) -> bool {
    let id = ui.next_auto_id().with("name_buf");
    let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| value.clone());
    let resp = ui.add(
        egui::TextEdit::singleline(&mut text)
            .char_limit(max)
            .desired_width(width),
    );
    if resp.has_focus() {
        text = dm32_codeplug::encoding::sanitize(&text, max);
        ui.data_mut(|d| d.insert_temp(id, text.clone()));
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
    }
    if resp.changed() && text.trim_end() != value.as_str() {
        *value = text;
        return true;
    }
    false
}

pub fn number<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    value: &mut T,
    range: std::ops::RangeInclusive<T>,
) -> bool {
    ui.add(egui::DragValue::new(value).range(range)).changed()
}

/// Form-row macros for a 2-column `egui::Grid`. Each evaluates to `true` if the value changed.
///
/// `choice_row!(ui, "Power", ch, power, set_power)` — a [`Choice`] getter/setter pair.
#[macro_export]
macro_rules! choice_row {
    ($ui:expr, $label:expr, $rec:expr, $get:ident, $set:ident) => {{
        $ui.label($label);
        let mut v = $rec.$get();
        let c = $crate::views::widgets::choice($ui, ($label, stringify!($get)), &mut v);
        if c {
            $rec.$set(v);
        }
        $ui.end_row();
        c
    }};
}

/// `bool_row!(ui, "Talkaround", ch, talkaround, set_talkaround)`
#[macro_export]
macro_rules! bool_row {
    ($ui:expr, $label:expr, $rec:expr, $get:ident, $set:ident) => {{
        $ui.label($label);
        let mut v = $rec.$get();
        let c = $ui.checkbox(&mut v, "").changed();
        if c {
            $rec.$set(v);
        }
        $ui.end_row();
        c
    }};
}

/// `num_row!(ui, "Squelch", ch, squelch, set_squelch, 0..=9)`
#[macro_export]
macro_rules! num_row {
    ($ui:expr, $label:expr, $rec:expr, $get:ident, $set:ident, $range:expr) => {{
        $ui.label($label);
        let mut v = $rec.$get();
        let c = $crate::views::widgets::number($ui, &mut v, $range);
        if c {
            $rec.$set(v);
        }
        $ui.end_row();
        c
    }};
}

/// `text_row!(ui, "Name", ch, name, set_name, 16)`
#[macro_export]
macro_rules! text_row {
    ($ui:expr, $label:expr, $rec:expr, $get:ident, $set:ident, $max:expr) => {{
        $ui.label($label);
        let mut v = $rec.$get();
        let c = $crate::views::widgets::name($ui, &mut v, $max, 180.0);
        if c {
            $rec.$set(&v);
        }
        $ui.end_row();
        c
    }};
}

/// Selection state of a list view, kept in egui's temporary memory.
pub fn selected(ui: &egui::Ui, key: &str) -> Option<usize> {
    ui.data(|d| d.get_temp(egui::Id::new(("sel", key))))
        .flatten()
}

pub fn set_selected(ui: &egui::Ui, key: &str, v: Option<usize>) {
    ui.data_mut(|d| d.insert_temp(egui::Id::new(("sel", key)), v));
}

/// A simple selectable list of names, navigable with the arrow keys; returns the selection.
pub fn name_list(ui: &mut egui::Ui, key: &str, names: &[String]) -> Option<usize> {
    let mut sel = selected(ui, key).filter(|&s| s < names.len());
    let order: Vec<usize> = (0..names.len()).collect();
    let moved = arrow_nav(ui, sel, &order);
    if moved.is_some() {
        sel = moved;
    }
    egui::ScrollArea::vertical()
        .id_salt(("list", key))
        .auto_shrink(false)
        .show(ui, |ui| {
            for (i, n) in names.iter().enumerate() {
                let resp = ui.selectable_label(sel == Some(i), format!("{:>3}  {n}", i + 1));
                if resp.clicked() {
                    sel = Some(i);
                }
                if moved == Some(i) {
                    resp.scroll_to_me(None);
                }
            }
        });
    set_selected(ui, key, sel);
    sel
}

/// Add / delete / up / down buttons for a list. Returns the action taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListAction {
    Add,
    Duplicate,
    Delete(usize),
    Up(usize),
    Down(usize),
}

pub fn list_buttons(
    ui: &mut egui::Ui,
    sel: Option<usize>,
    len: usize,
    max: usize,
) -> Option<ListAction> {
    let mut act = None;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(len < max, egui::Button::new("➕ Add"))
            .clicked()
        {
            act = Some(ListAction::Add);
        }
        if ui
            .add_enabled(sel.is_some() && len < max, egui::Button::new("⎘ Duplicate"))
            .clicked()
        {
            act = Some(ListAction::Duplicate);
        }
        if let Some(s) = sel {
            if ui.button("🗑 Delete").clicked() {
                act = Some(ListAction::Delete(s));
            }
            if ui
                .add_enabled(s > 0, egui::Button::new("⏶"))
                .on_hover_text("Move up")
                .clicked()
            {
                act = Some(ListAction::Up(s));
            }
            if ui
                .add_enabled(s + 1 < len, egui::Button::new("⏷"))
                .on_hover_text("Move down")
                .clicked()
            {
                act = Some(ListAction::Down(s));
            }
        }
        ui.label(format!("{len} / {max}"));
    });
    act
}

/// Apply a [`ListAction`] to a plain vector (no cross-references). Returns `true` if changed.
pub fn apply<T: Clone>(
    ui: &egui::Ui,
    key: &str,
    list: &mut Vec<T>,
    act: ListAction,
    new: impl FnOnce() -> T,
) -> bool {
    match act {
        ListAction::Add => {
            list.push(new());
            set_selected(ui, key, Some(list.len() - 1));
        }
        ListAction::Duplicate => {
            let Some(s) = selected(ui, key) else {
                return false;
            };
            let item = list[s].clone();
            list.insert(s + 1, item);
            set_selected(ui, key, Some(s + 1));
        }
        ListAction::Delete(s) => {
            list.remove(s);
            set_selected(
                ui,
                key,
                if list.is_empty() {
                    None
                } else {
                    Some(s.min(list.len() - 1))
                },
            );
        }
        ListAction::Up(s) => {
            list.swap(s, s - 1);
            set_selected(ui, key, Some(s - 1));
        }
        ListAction::Down(s) => {
            list.swap(s, s + 1);
            set_selected(ui, key, Some(s + 1));
        }
    }
    true
}

/// A multi-selection (click, Ctrl+click to toggle, Shift+click for a range), kept in egui's
/// temporary memory under `key`.
#[derive(Debug, Clone, Default)]
pub struct MultiSel {
    pub items: std::collections::BTreeSet<usize>,
    /// The last item clicked without Shift: the start of a Shift range, and the item shown in
    /// a detail panel.
    pub anchor: Option<usize>,
}

impl MultiSel {
    pub fn load(ui: &egui::Ui, key: &str) -> MultiSel {
        ui.data(|d| d.get_temp(egui::Id::new(("msel", key))))
            .unwrap_or_default()
    }

    pub fn store(&self, ui: &egui::Ui, key: &str) {
        ui.data_mut(|d| d.insert_temp(egui::Id::new(("msel", key)), self.clone()));
    }

    pub fn single(i: Option<usize>) -> MultiSel {
        MultiSel {
            items: i.into_iter().collect(),
            anchor: i,
        }
    }

    pub fn contains(&self, i: usize) -> bool {
        self.items.contains(&i)
    }

    /// Handle a click on item `i`. `order` is the items in the order they are displayed
    /// (after filtering), which defines a Shift range.
    pub fn click(&mut self, i: usize, modifiers: egui::Modifiers, order: &[usize]) {
        if modifiers.shift
            && let Some(a) = self.anchor
            && let (Some(pa), Some(pi)) = (
                order.iter().position(|&x| x == a),
                order.iter().position(|&x| x == i),
            )
        {
            let (lo, hi) = (pa.min(pi), pa.max(pi));
            if !modifiers.command {
                self.items.clear();
            }
            self.items.extend(&order[lo..=hi]);
        } else if modifiers.command {
            if !self.items.remove(&i) {
                self.items.insert(i);
            }
            self.anchor = Some(i);
        } else {
            self.items = [i].into();
            self.anchor = Some(i);
        }
    }

    /// Drop items that no longer exist or are not allowed.
    pub fn retain(&mut self, keep: impl Fn(usize) -> bool) {
        self.items.retain(|&i| keep(i));
        if self.anchor.is_some_and(|a| !keep(a)) {
            self.anchor = None;
        }
    }
}

/// Keyboard navigation for a view's main list: Up/Down move one row, Page Up/Down ten, Home/End
/// to the ends. `order` is the selectable items in display order. Ignored while a text field
/// has keyboard focus. Returns the newly selected item if the selection moved.
pub fn arrow_nav(ui: &egui::Ui, current: Option<usize>, order: &[usize]) -> Option<usize> {
    if order.is_empty() || ui.ctx().egui_wants_keyboard_input() {
        return None;
    }
    let pos = current.and_then(|c| order.iter().position(|&x| x == c));
    let last = order.len() - 1;
    let target = ui.input_mut(|i| {
        let mut key = |k| i.consume_key(egui::Modifiers::NONE, k);
        if key(egui::Key::ArrowDown) {
            Some(pos.map_or(0, |p| (p + 1).min(last)))
        } else if key(egui::Key::ArrowUp) {
            Some(pos.map_or(0, |p| p.saturating_sub(1)))
        } else if key(egui::Key::PageDown) {
            Some(pos.map_or(0, |p| (p + 10).min(last)))
        } else if key(egui::Key::PageUp) {
            Some(pos.map_or(0, |p| p.saturating_sub(10)))
        } else if key(egui::Key::Home) {
            Some(0)
        } else if key(egui::Key::End) {
            Some(last)
        } else {
            None
        }
    })?;
    let item = order[target];
    (Some(item) != current).then_some(item)
}

/// Take keyboard focus away from whatever field has it (call when a list row is clicked), so
/// the arrow keys move the list instead of a text cursor or a number field.
pub fn release_focus(ui: &egui::Ui) {
    ui.memory_mut(|m| {
        if let Some(id) = m.focused() {
            m.surrender_focus(id);
        }
    });
}

/// Whether a list navigation key was pressed this frame (cheap check before building an order).
pub fn nav_key_pressed(ui: &egui::Ui) -> bool {
    use egui::Key::*;
    ui.input(|i| {
        [ArrowUp, ArrowDown, PageUp, PageDown, Home, End]
            .iter()
            .any(|&k| i.key_pressed(k))
    })
}

/// Scroll a table to `row` (its display position) when set.
pub fn scrolled(
    t: egui_extras::TableBuilder<'_>,
    row: Option<usize>,
) -> egui_extras::TableBuilder<'_> {
    match row {
        Some(r) => t.scroll_to_row(r, None),
        None => t,
    }
}
