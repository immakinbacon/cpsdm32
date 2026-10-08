//! Quick text messages (preset SMS texts).

use dm32_codeplug::encoding::sanitize;
use dm32_codeplug::records::quick_text::{self, QuickText};
use eframe::egui;
use egui_extras::{Column, TableBuilder};

use super::Doc;
use super::widgets;

const KEY: &str = "quick_texts";

pub fn show(ui: &mut egui::Ui, doc: &mut Doc) -> bool {
    let mut changed = false;
    let mut sel = widgets::selected(ui, KEY).filter(|&s| s < doc.cp.quick_texts.messages.len());
    let order: Vec<usize> = (0..doc.cp.quick_texts.messages.len()).collect();
    let moved = widgets::arrow_nav(ui, sel, &order);
    if moved.is_some() {
        sel = moved;
    }
    widgets::set_selected(ui, KEY, sel);
    let len = doc.cp.quick_texts.messages.len();
    if let Some(act) = widgets::list_buttons(ui, sel, len, quick_text::MAX) {
        changed |= widgets::apply(ui, KEY, &mut doc.cp.quick_texts.messages, act, || {
            QuickText::new("New message")
        });
    }
    let sel = widgets::selected(ui, KEY);

    egui::Panel::bottom("quick_text_detail")
        .resizable(false)
        .show(ui, |ui| {
            ui.add_space(4.0);
            match sel {
                Some(i) => {
                    let m = &mut doc.cp.quick_texts.messages[i];
                    ui.strong(format!("Message {}", i + 1));
                    // Keep the typed text while editing: the stored text drops trailing spaces.
                    let buf = egui::Id::new(("quick_text_buf", i));
                    let mut text: String = ui.data(|d| d.get_temp(buf)).unwrap_or_else(|| m.text());
                    let resp = ui.add(
                        egui::TextEdit::multiline(&mut text)
                            .char_limit(QuickText::TEXT_LEN)
                            .desired_rows(3)
                            .desired_width(f32::INFINITY),
                    );
                    if resp.changed() {
                        // One line of printable ASCII: line breaks become spaces.
                        text = sanitize(&text.replace(['\r', '\n'], " "), QuickText::TEXT_LEN);
                        if text.trim_end() != m.text() {
                            m.set_text(&text);
                            changed = true;
                        }
                    }
                    if resp.has_focus() {
                        ui.data_mut(|d| d.insert_temp(buf, text));
                    } else {
                        ui.data_mut(|d| d.remove::<String>(buf));
                    }
                    ui.weak(format!(
                        "{} / {} characters (printable ASCII)",
                        m.text().len(),
                        QuickText::TEXT_LEN
                    ));
                }
                None => {
                    ui.label("Select a message to edit it.");
                }
            }
            ui.add_space(4.0);
        });

    let row_h = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
    let list = &doc.cp.quick_texts.messages;
    let mut clicked = None;
    super::talkgroups::table_area(ui, |ui| {
        widgets::scrolled(TableBuilder::new(ui), moved)
            .id_salt(KEY)
            .striped(true)
            .sense(egui::Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::exact(44.0))
            .column(Column::remainder().at_least(100.0))
            .header(row_h, |mut h| {
                for t in ["No.", "Data"] {
                    h.col(|ui| {
                        ui.strong(t);
                    });
                }
            })
            .body(|body| {
                body.rows(row_h, list.len(), |mut row| {
                    let i = row.index();
                    row.set_selected(sel == Some(i));
                    row.col(|ui| {
                        ui.label((i + 1).to_string());
                    });
                    row.col(|ui| {
                        ui.add(egui::Label::new(list[i].text()).truncate());
                    });
                    if row.response().clicked() {
                        clicked = Some(i);
                    }
                });
            });
    });
    if clicked.is_some() {
        widgets::release_focus(ui);
        widgets::set_selected(ui, KEY, clicked);
    }
    changed
}
