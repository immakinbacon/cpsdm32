use dm32_codeplug::block;
use eframe::egui;

use super::Doc;

pub fn show(ui: &mut egui::Ui, doc: &mut Doc) -> bool {
    let info = &doc.image.info;
    ui.heading("Radio");
    egui::Grid::new("overview")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            for (k, v) in [
                ("Model", info.model.as_str()),
                ("Firmware", info.firmware.as_str()),
                ("Build date", info.build_date.as_str()),
                ("Codeplug version", info.codeplug_version.as_str()),
            ] {
                ui.label(k);
                ui.label(if v.is_empty() { "—" } else { v });
                ui.end_row();
            }
            ui.label("Codeplug blocks");
            ui.label(doc.image.blocks.len().to_string());
            ui.end_row();
            ui.label("Contact database pages");
            ui.label(doc.image.contacts.len().to_string());
            ui.end_row();
        });
    if !doc.image.blocks.contains_key(&block::SETTINGS) {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            "This codeplug has no radio settings block.",
        );
    }
    ui.add_space(12.0);
    ui.heading("Checks");
    let problems = doc.cp.problems();
    if problems.is_empty() {
        ui.label("No problems found.");
    }
    for p in problems {
        ui.colored_label(ui.visuals().warn_fg_color, p);
    }
    false
}
