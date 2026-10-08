//! `dm32cps` — graphical programming software for the Baofeng DM-32UV.

mod app;
mod files;
mod job;
mod views;

fn main() -> eframe::Result {
    let file = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("DM-32UV CPS")
            .with_app_id("dm32cps")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };
    eframe::run_native(
        "dm32cps",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, file)))),
    )
}
