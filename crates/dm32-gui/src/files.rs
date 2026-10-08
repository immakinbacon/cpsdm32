//! File dialogs drawn inside the application with egui.
//!
//! Native dialogs on Linux go through xdg-desktop-portal (or GTK); where neither is available
//! they fail silently and the button appears to do nothing. Drawing the dialog ourselves works
//! on every desktop.
//!
//! Usage: [`request`] opens a dialog for a [`Purpose`]; the app calls [`show`] once per frame;
//! the code that asked collects the result with [`take`].

use std::cell::RefCell;
use std::path::PathBuf;

use eframe::egui;
use egui_file_dialog::FileDialog;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Open,
    SaveAs,
    ExportData,
    ImportCsv,
    ExportCsv,
}

impl Purpose {
    fn spec(
        self,
    ) -> (
        &'static str,
        &'static str,
        Vec<&'static str>,
        Option<&'static str>,
    ) {
        // (title, filter name, extensions, default save name)
        match self {
            Purpose::Open => (
                "Open codeplug",
                "Codeplug (.dm32, .data)",
                vec!["dm32", "data"],
                None,
            ),
            Purpose::SaveAs => (
                "Save codeplug",
                "DM-32 codeplug (.dm32)",
                vec!["dm32"],
                Some("codeplug.dm32"),
            ),
            Purpose::ExportData => (
                "Export official CPS file",
                "Official CPS file (.data)",
                vec!["data"],
                Some("codeplug.data"),
            ),
            Purpose::ImportCsv => ("Import contacts", "CSV (.csv)", vec!["csv"], None),
            Purpose::ExportCsv => (
                "Export contacts",
                "CSV (.csv)",
                vec!["csv"],
                Some("contacts.csv"),
            ),
        }
    }

    fn saves(self) -> bool {
        matches!(
            self,
            Purpose::SaveAs | Purpose::ExportData | Purpose::ExportCsv
        )
    }
}

#[derive(Default)]
struct State {
    dialog: Option<(Purpose, FileDialog)>,
    picked: Option<(Purpose, PathBuf)>,
    last_dir: Option<PathBuf>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Open a dialog. Replaces any dialog that is already open.
pub fn request(purpose: Purpose) {
    STATE.with_borrow_mut(|st| {
        let (title, filter, exts, default_name) = purpose.spec();
        let ext = exts[0];
        let mut d = FileDialog::new()
            .title(title)
            .add_file_filter_extensions(filter, exts)
            .default_file_filter(filter);
        if purpose.saves() {
            d = d
                .add_save_extension(filter, ext)
                .default_save_extension(filter);
        }
        if let Some(name) = default_name {
            d = d.default_file_name(name);
        }
        if let Some(dir) = &st.last_dir {
            d = d.initial_directory(dir.clone());
        }
        if purpose.saves() {
            d.save_file();
        } else {
            d.pick_file();
        }
        st.dialog = Some((purpose, d));
        st.picked = None;
    });
}

/// Draw the open dialog, if any. Call once per frame.
pub fn show(ctx: &egui::Context) {
    STATE.with_borrow_mut(|st| {
        let Some((purpose, d)) = st.dialog.as_mut() else {
            return;
        };
        d.update(ctx);
        let purpose = *purpose;
        if let Some(mut path) = d.take_picked() {
            let (_, _, exts, _) = purpose.spec();
            if purpose.saves() && path.extension().is_none() {
                path.set_extension(exts[0]);
            }
            st.last_dir = path.parent().map(PathBuf::from);
            st.picked = Some((purpose, path));
            st.dialog = None;
        } else if matches!(
            d.state(),
            egui_file_dialog::DialogState::Closed | egui_file_dialog::DialogState::Cancelled
        ) {
            st.dialog = None;
        }
    });
}

/// The path chosen for `purpose`, once (consumes it).
pub fn take(purpose: Purpose) -> Option<PathBuf> {
    STATE.with_borrow_mut(|st| match &st.picked {
        Some((p, _)) if *p == purpose => st.picked.take().map(|(_, path)| path),
        _ => None,
    })
}
