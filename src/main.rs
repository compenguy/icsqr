//! ICS QR desktop application.
//!
//! Wires the Slint UI to ICS parsing, QR generation, file dialogs, clipboard
//! export, and platform-specific file-drop handling.

mod ics;
mod qr;

use arboard::Clipboard;
use ics::{event_ics, load_calendar, LoadedCalendar};
use qr::generate_qr;
use slint::winit_030::{EventResult, WinitWindowAccessor, winit};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

slint::include_modules!();

/// Shared mutable state accessed from UI callbacks.
struct AppData {
    calendar: Option<LoadedCalendar>,
    /// PNG bytes of the current QR, used for copy/save without re-rendering.
    qr_png: Vec<u8>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = AppWindow::new()?;
    let app_weak = app.as_weak();
    let state = Rc::new(RefCell::new(AppData {
        calendar: None,
        qr_png: Vec::new(),
    }));

    {
        let app_weak = app_weak.clone();
        let state = state.clone();
        app.on_open_file(move || {
            open_ics_file(&app_weak, &state);
        });
    }

    {
        let app_weak = app_weak.clone();
        let state = state.clone();
        app.on_event_selected(move || {
            update_qr_for_selection(&app_weak, &state);
        });
    }

    {
        let app_weak = app_weak.clone();
        let state = state.clone();
        app.on_copy_qr(move || {
            copy_qr_to_clipboard(&app_weak, &state);
        });
    }

    {
        let app_weak = app_weak.clone();
        let state = state.clone();
        app.on_save_qr(move || {
            save_qr_to_file(&app_weak, &state);
        });
    }

    // File-drop hooks must be registered after the winit window exists (i.e. once
    // the event loop is running). spawn_local schedules this on the first tick.
    {
        let app_weak = app_weak.clone();
        let state = state.clone();
        slint::spawn_local(async move {
            setup_file_drop(&app_weak, &state);
        })?;
    }

    app.run()?;
    Ok(())
}

/// Listen for winit drag-and-drop events (supported on X11 and Windows, not Wayland).
fn setup_file_drop(app_weak: &slint::Weak<AppWindow>, state: &Rc<RefCell<AppData>>) {
    let Some(app) = app_weak.upgrade() else {
        return;
    };

    let app_weak = app_weak.clone();
    let state = state.clone();
    app.window().on_winit_window_event(move |_window, event| {
        match event {
            winit::event::WindowEvent::HoveredFile(_) => {
                if let Some(app) = app_weak.upgrade() {
                    app.set_drag_over(true);
                }
            }
            winit::event::WindowEvent::HoveredFileCancelled => {
                if let Some(app) = app_weak.upgrade() {
                    app.set_drag_over(false);
                }
            }
            winit::event::WindowEvent::DroppedFile(path) => {
                if let Some(app) = app_weak.upgrade() {
                    app.set_drag_over(false);
                }
                load_file(&app_weak, &state, path.as_path());
            }
            _ => {}
        }
        EventResult::Propagate
    });
}

/// Show a native file picker asynchronously so the Slint event loop stays responsive.
///
/// On Linux/Wayland, a synchronous dialog would block rendering and the compositor
/// may mark the app as unresponsive.
fn open_ics_file(app_weak: &slint::Weak<AppWindow>, state: &Rc<RefCell<AppData>>) {
    let app_weak = app_weak.clone();
    let state = state.clone();

    slint::spawn_local(async move {
        set_status(&app_weak, "Opening file picker…");

        let mut dialog = rfd::AsyncFileDialog::new()
            .set_title("Open iCalendar File")
            .add_filter("iCalendar", &["ics", "ical", "ifb", "icalendar"]);

        if let Some(app) = app_weak.upgrade() {
            let handle = app.window().window_handle();
            dialog = dialog.set_parent(&handle);
        }

        match dialog.pick_file().await {
            Some(file) => load_file(&app_weak, &state, file.path()),
            None => set_status(&app_weak, "File selection cancelled."),
        }
    })
    .expect("failed to schedule file picker");
}

/// Parse an ICS file, populate the event list, and generate a QR for the first event.
fn load_file(app_weak: &slint::Weak<AppWindow>, state: &Rc<RefCell<AppData>>, path: &Path) {
    let Some(app) = app_weak.upgrade() else {
        return;
    };

    match load_calendar(path) {
        Ok(loaded) => {
            let labels: Vec<SharedString> = loaded
                .labels
                .iter()
                .map(|label| SharedString::from(label.as_str()))
                .collect();

            app.set_event_labels(ModelRc::new(VecModel::from(labels)));
            app.set_selected_event_index(0);
            app.set_file_label(format!("Loaded: {}", path.display()).into());
            app.set_status_message(format!("Loaded {} event(s).", loaded.labels.len()).into());
            app.set_has_qr(false);
            app.set_qr_image(slint::Image::default());

            {
                let mut data = state.borrow_mut();
                data.calendar = Some(loaded);
                data.qr_png.clear();
            }

            update_qr_for_selection(app_weak, state);
        }
        Err(message) => {
            app.set_status_message(message.into());
            app.set_has_qr(false);
        }
    }
}

/// Rebuild the QR code from the currently selected event.
fn update_qr_for_selection(app_weak: &slint::Weak<AppWindow>, state: &Rc<RefCell<AppData>>) {
    let Some(app) = app_weak.upgrade() else {
        return;
    };

    let event_index = app.get_selected_event_index();
    if event_index < 0 {
        return;
    }

    let event_index = event_index as usize;
    let ics_text = {
        let data = state.borrow();
        let Some(calendar) = data.calendar.as_ref() else {
            return;
        };
        match event_ics(&calendar.calendar, event_index) {
            Ok(text) => text,
            Err(message) => {
                app.set_status_message(message.into());
                app.set_has_qr(false);
                return;
            }
        }
    };

    match generate_qr(&ics_text) {
        Ok(generated) => {
            app.set_qr_image(generated.image);
            app.set_has_qr(true);
            app.set_status_message(
                "QR code ready. Scan with your phone camera to add the event to your calendar."
                    .into(),
            );

            let mut data = state.borrow_mut();
            data.qr_png = generated.png_bytes;
        }
        Err(message) => {
            app.set_has_qr(false);
            app.set_status_message(message.into());
        }
    }
}

fn copy_qr_to_clipboard(app_weak: &slint::Weak<AppWindow>, state: &Rc<RefCell<AppData>>) {
    let data = state.borrow();
    if data.qr_png.is_empty() {
        return;
    }

    let image = match image::load_from_memory(&data.qr_png) {
        Ok(image) => image.to_rgba8(),
        Err(err) => {
            set_status(app_weak, format!("Failed to decode QR image: {err}"));
            return;
        }
    };

    let (width, height) = image.dimensions();
    let clipboard_image = arboard::ImageData {
        width: width as usize,
        height: height as usize,
        bytes: image.into_raw().into(),
    };

    match Clipboard::new().and_then(|mut clipboard| clipboard.set_image(clipboard_image)) {
        Ok(()) => set_status(app_weak, "QR code copied to clipboard."),
        Err(err) => set_status(app_weak, format!("Failed to copy QR code: {err}")),
    }
}

fn save_qr_to_file(app_weak: &slint::Weak<AppWindow>, state: &Rc<RefCell<AppData>>) {
    let png_bytes = state.borrow().qr_png.clone();
    if png_bytes.is_empty() {
        return;
    }

    let app_weak = app_weak.clone();
    slint::spawn_local(async move {
        set_status(&app_weak, "Opening save dialog…");

        let mut dialog = rfd::AsyncFileDialog::new()
            .set_title("Save QR Code")
            .set_file_name("event-qr.png")
            .add_filter("PNG Image", &["png"]);

        if let Some(app) = app_weak.upgrade() {
            let handle = app.window().window_handle();
            dialog = dialog.set_parent(&handle);
        }

        let Some(file) = dialog.save_file().await else {
            set_status(&app_weak, "Save cancelled.");
            return;
        };

        let path = file.path().to_path_buf();
        match std::fs::write(&path, &png_bytes) {
            Ok(()) => set_status(&app_weak, format!("Saved QR code to {}.", path.display())),
            Err(err) => set_status(&app_weak, format!("Failed to save QR code: {err}")),
        }
    })
    .expect("failed to schedule save dialog");
}

fn set_status(app_weak: &slint::Weak<AppWindow>, message: impl Into<SharedString>) {
    if let Some(app) = app_weak.upgrade() {
        app.set_status_message(message.into());
    }
}
