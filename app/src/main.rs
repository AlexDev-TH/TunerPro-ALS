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
mod search;
mod shell;
mod theme;
mod tunerpro;
mod view;
mod xdf;

fn main() -> eframe::Result<()> {
    let mut args = std::env::args().skip(1);
    let (mut screenshot, mut demo, mut files) = (None, None, vec![]);
    let mut link = tunerpro::Link::default();
    let mut send = None;
    let mut colours = None;
    let mut heights = false;
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
            _ => files.push(a),
        }
    }
    if let Some(x) = send {
        std::process::exit(if link.load_xdf(&x).is_ok() { 0 } else { 1 });
    }
    link.bin = files.first().map(std::path::PathBuf::from);
    link.dark = colours.or_else(|| tunerpro::host_is_dark(&link));
    let mut viewport = egui::ViewportBuilder::default().with_title(app::TITLE).with_inner_size([1241.0, 779.0]).with_min_inner_size([640.0, 400.0]);
    if let Some(icon) = tunerpro::host_icon(&link) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(
        "ALS",
        options,
        Box::new(move |_cc| {
            let mut a = app::App::new(screenshot, demo);
            a.start_files = files;
            a.tunerpro = link;
            a.cfg.height_colors |= heights;
            Ok(Box::new(a))
        }),
    )
}
