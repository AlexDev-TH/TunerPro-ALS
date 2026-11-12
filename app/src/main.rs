#![cfg_attr(not(test), windows_subsystem = "windows")]
mod a2l;
mod analysis;
mod app;
mod checksum;
mod cmd;
mod commands;
mod config;
mod dialogs;
#[cfg(test)]
mod drive;
mod formats;
mod model;
mod report;
mod search;
mod shell;
mod theme;
mod tunerpro;
mod view;
mod xdf;

use report::Drawing;

fn main() {
    report::install_panic_hook();
    let mut args = std::env::args().skip(1);
    let (mut screenshot, mut demo, mut files) = (None, None, vec![]);
    let mut link = tunerpro::Link::default();
    let mut send = None;
    let mut colours = None;
    let mut heights = false;
    let mut asked = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--screenshot" => screenshot = args.next().map(std::path::PathBuf::from),
            "--demo" => demo = args.next(),
            // Set by the TunerPro plug-in: its main window, and the definition it has open.
            "--tunerpro" => link.window = args.next().and_then(|s| s.parse().ok()),
            "--xdf" => link.xdf = args.next().map(std::path::PathBuf::from),
            // Hands a definition that already exists to TunerPro and leaves, without a window:
            // `--tunerpro <window> --send-xdf <file>`. The exit code says whether it was taken.
            "--send-xdf" => send = args.next().map(std::path::PathBuf::from),
            // Says which colours TunerPro is showing, instead of looking at its window.
            "--colours" => colours = args.next().map(|s| s == "dark"),
            // Starts with the height colours switched on.
            "--height-colours" => heights = true,
            // Picks the way of drawing instead of trying them in turn: `opengl` or `modern`.
            "--drawing" => asked = args.next().map(|s| if s.eq_ignore_ascii_case("opengl") { Drawing::OpenGl } else { Drawing::Modern }),
            _ => files.push(a),
        }
    }
    if let Some(x) = send {
        std::process::exit(if link.load_xdf(&x).is_ok() { 0 } else { 1 });
    }
    // A picture for checking runs unattended: nothing may wait for a click.
    report::set_quiet(screenshot.is_some());
    link.bin = files.first().map(std::path::PathBuf::from);
    link.dark = colours.or_else(|| tunerpro::host_is_dark(&link));
    let icon = tunerpro::host_icon(&link);

    // Not every computer can draw the usual way (old graphics chips, missing drivers, remote
    // sessions), so when that fails at the start the window is opened again with OpenGL.
    let mut tried: Vec<(Drawing, String)> = vec![];
    for drawing in report::drawing_order(asked) {
        let mut viewport = egui::ViewportBuilder::default().with_title(app::TITLE).with_inner_size([1241.0, 779.0]).with_min_inner_size([640.0, 400.0]);
        if let Some(icon) = icon.clone() {
            viewport = viewport.with_icon(icon);
        }
        let renderer = match drawing {
            Drawing::Modern => eframe::Renderer::Wgpu,
            Drawing::OpenGl => eframe::Renderer::Glow,
        };
        let options = eframe::NativeOptions { viewport, renderer, ..Default::default() };
        let (screenshot, demo, files, link) = (screenshot.clone(), demo.clone(), files.clone(), link.clone());
        report::starting(drawing);
        let result = eframe::run_native(
            "ALS",
            options,
            Box::new(move |_cc| {
                let mut a = app::App::new(screenshot, demo);
                a.start_files = files;
                a.tunerpro = link;
                a.cfg.height_colors |= heights;
                Ok(Box::new(a))
            }),
        );
        report::ended();
        let Err(error) = result else { return };
        let text = error.to_string();
        report::note(&format!("{} failed: {text}", drawing.name()));
        if report::drew_frames() {
            // It broke while in use, not at the start: opening a second, empty window would not help.
            report::alert(&format!(
                "ALS หยุดทำงานเพราะระบบกราฟิกมีปัญหา\nALS stopped because drawing failed.\n\n{}: {text}\n\nบันทึก / Log: {}",
                drawing.name(),
                report::log_path().display()
            ));
            std::process::exit(1);
        }
        tried.push((drawing, text));
    }
    report::no_window(&tried);
    std::process::exit(1);
}
