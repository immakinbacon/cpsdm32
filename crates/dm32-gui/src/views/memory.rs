//! Raw view of every block, for troubleshooting and for fields not yet supported.

use dm32_codeplug::block;
use eframe::egui;

use super::Doc;

pub fn show(ui: &mut egui::Ui, doc: &mut Doc) {
    let id = egui::Id::new("memory_block");
    let mut selected: u8 = ui.data(|d| d.get_temp(id)).unwrap_or(block::SETTINGS);
    ui.horizontal(|ui| {
        ui.label("Block");
        egui::ComboBox::from_id_salt("block")
            .selected_text(format!("{selected:#04x} {}", block::name(selected)))
            .height(400.0)
            .show_ui(ui, |ui| {
                for &b in doc.image.blocks.keys() {
                    ui.selectable_value(&mut selected, b, format!("{b:#04x} {}", block::name(b)));
                }
            });
    });
    ui.data_mut(|d| d.insert_temp(id, selected));
    let Some(page) = doc.image.blocks.get(&selected) else {
        ui.label("Not present in this codeplug.");
        return;
    };
    ui.separator();
    let row_h = ui.text_style_height(&egui::TextStyle::Monospace);
    egui::ScrollArea::vertical().auto_shrink(false).show_rows(
        ui,
        row_h,
        page.len() / 16,
        |ui, rows| {
            for r in rows {
                let chunk = &page[r * 16..r * 16 + 16];
                let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
                let ascii: String = chunk
                    .iter()
                    .map(|&b| {
                        if (0x20..0x7f).contains(&b) {
                            b as char
                        } else {
                            '.'
                        }
                    })
                    .collect();
                ui.monospace(format!("{:03x}  {}  {ascii}", r * 16, hex.join(" ")));
            }
        },
    );
}
