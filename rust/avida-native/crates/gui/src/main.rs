//! `avida-gui [config-dir] [avida args...]`: the desktop viewer.
//!
//! Before opening the window it checks the login session: on macOS a
//! shell outside the desktop session (inside tmux, or over ssh) gets a
//! window that cannot take the focus, so it prints why and what to do
//! instead; elsewhere a missing display stops it. See
//! [`avida_gui::launch`]. Logs (eframe, wgpu, winit) go to stderr at the
//! level `RUST_LOG` names, warnings by default.

use std::process::ExitCode;

use avida_gui::cli::{parse, Parsed, USAGE};
use avida_gui::launch;
use avida_gui::AvidaApp;

fn main() -> ExitCode {
    let startup = match parse(std::env::args().skip(1)) {
        Parsed::Help => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Parsed::Run(s) => s,
    };
    launch::install_logger();
    if let Some(problem) = launch::display_problem() {
        eprintln!("{}", launch::display_problem_message(&problem));
        // A macOS window still opens (behind the others); without a
        // display there is nothing to open it on.
        if !cfg!(target_os = "macos") {
            return ExitCode::from(2);
        }
    }
    // The window's size and position, and egui's memory (the widths of
    // the side and log panels among it), are stored with the settings and
    // restored at the next start. eframe drops a restored position that
    // lies on no monitor and clamps the size to the largest one; the first
    // frames fit the window to its current monitor and bring it forward
    // (see `launch::WindowStartup`).
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Avida")
            .with_app_id("avida-gui")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1000.0, 650.0])
            .with_drag_and_drop(true)
            // A bare binary started from a terminal is not brought to the
            // front by macOS unless it asks.
            .with_active(true),
        persist_window: true,
        ..Default::default()
    };
    let shown = launch::watch_for_first_frame(launch::FIRST_FRAME_TIMEOUT);
    let result = eframe::run_native(
        "Avida",
        options,
        Box::new(move |cc| {
            Ok(Box::new(
                AvidaApp::new(cc, startup).with_window_startup(shown),
            ))
        }),
    );
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!(
                "avida-gui: could not open the window: {e}\n\n\
                 The graphics driver or the display refused it. RUST_LOG=info shows \
                 what eframe and wgpu tried."
            );
            ExitCode::FAILURE
        }
    }
}
