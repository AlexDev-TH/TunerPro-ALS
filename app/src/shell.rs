//! The main window: menus, toolbars, the map list, the status bar, the workspace and its windows.
use crate::analysis::{AreaKind, OVERVIEW_BLOCK};
use crate::app::{env_for, App};
use crate::cmd::Cmd;
use crate::model::*;
use crate::theme::c;
use crate::view::{self, SelMode};
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};

// ---------- the colours of the program's own frame ----------
// Menus, toolbars, the frame of the map list, window frames and the workspace come in a
// light and a dark set, so the program sits well next to TunerPro in either of its themes. The
// dark set is measured from TunerPro's dark theme. The data windows have themes of their own.

static DARK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn is_dark() -> bool {
    DARK.load(std::sync::atomic::Ordering::Relaxed)
}

/// One colour for the light frame, another for the dark one.
pub fn pick(light: Color32, dark: Color32) -> Color32 {
    if is_dark() {
        dark
    } else {
        light
    }
}

const fn flat(v: u8) -> Color32 {
    Color32::from_rgb(v, v, v)
}

pub fn menu_bg() -> Color32 {
    pick(flat(0xFF), flat(0x20))
}
pub fn bar_bg() -> Color32 {
    pick(flat(0xF0), flat(0x2B))
}
pub fn panel_bg() -> Color32 {
    pick(flat(0xD4), flat(0x2B))
}
pub fn work_bg() -> Color32 {
    pick(flat(0xAB), flat(0x1E))
}
/// Text, and the lines drawn on buttons.
pub fn ink() -> Color32 {
    pick(flat(0x00), flat(0xDC))
}
/// Fields, list heads and the light parts of buttons.
pub fn paper() -> Color32 {
    pick(flat(0xFF), flat(0x33))
}
/// Buttons and text that can't be used right now.
fn grey() -> Color32 {
    gray(165)
}
pub fn gray(v: u8) -> Color32 {
    tone(flat(v))
}
pub fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    tone(Color32::from_rgb(r, g, b))
}

fn from_hsl(h: f32, s: f32, l: f32) -> Color32 {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let u = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(u(r), u(g), u(b))
}

/// A colour that was chosen for the light frame, as it should look in the frame in use. In the
/// dark frame greys trade places (white fields become dark, black lines become light), pale
/// tints become deep ones and the other way round, and strong colours stay as they are.
pub fn tone(c: Color32) -> Color32 {
    if is_dark() {
        tone_dark(c)
    } else {
        c
    }
}

fn tone_dark(c: Color32) -> Color32 {
    let (r, g, b) = (c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    let s = if d == 0.0 { 0.0 } else { d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-6) };
    if s < 0.12 {
        let v = match l {
            l if l > 0.97 => 0x33, // fields
            l if l > 0.90 => 0x3C, // button faces
            l if l > 0.78 => 0x2B, // panels
            l if l > 0.55 => 0x5C, // rules and frames
            l if l > 0.29 => 0x8C, // dim lines and text
            _ => 0xDC,             // lines and text
        };
        return flat(v);
    }
    if (0.36..=0.66).contains(&l) && s > 0.35 {
        return c;
    }
    let h = if max == r {
        60.0 * (((g - b) / d) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    from_hsl(if h < 0.0 { h + 360.0 } else { h }, s, (1.0 - l).clamp(0.20, 0.78))
}

/// Switches the whole frame to the light or the dark set, the dialogs and menus included.
pub fn apply_colours(ctx: &egui::Context, dark: bool) {
    DARK.store(dark, std::sync::atomic::Ordering::Relaxed);
    let mut style = (*ctx.global_style()).clone();
    style.visuals = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    style.visuals.panel_fill = bar_bg();
    style.visuals.window_fill = bar_bg();
    style.visuals.window_corner_radius = 0.into();
    style.visuals.menu_corner_radius = 0.into();
    if dark {
        style.visuals.extreme_bg_color = paper();
        style.visuals.faint_bg_color = flat(0x30);
        style.visuals.window_stroke = Stroke::new(1.0, flat(0x55));
        style.visuals.selection.bg_fill = Color32::from_rgb(0x26, 0x4F, 0x78);
        style.visuals.selection.stroke = Stroke::new(1.0, flat(0xF0));
        style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, flat(0x55));
        style.visuals.widgets.inactive.weak_bg_fill = flat(0x3C);
        style.visuals.widgets.inactive.bg_fill = flat(0x3C);
        style.visuals.widgets.hovered.weak_bg_fill = flat(0x4A);
        style.visuals.widgets.hovered.bg_fill = flat(0x4A);
    }
    style.visuals.widgets.noninteractive.fg_stroke.color = ink();
    style.visuals.widgets.inactive.fg_stroke.color = ink();
    style.visuals.widgets.hovered.fg_stroke.color = ink();
    style.visuals.widgets.active.fg_stroke.color = ink();
    for w in [
        &mut style.visuals.widgets.noninteractive,
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        w.corner_radius = 0.into();
    }
    ctx.set_global_style(style);
}

pub fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let load = |path: &str| std::fs::read(path).ok();
    let root = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    let fdir = format!(r"{root}\Fonts\");
    if let Some(b) = load(&(fdir.clone() + "tahoma.ttf")) {
        fonts.font_data.insert("ui".into(), egui::FontData::from_owned(b).into());
        if let Some(fam) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            fam.insert(0, "ui".into());
        }
        // Thai and other scripts in names still show in the data windows' labels.
        if let Some(fam) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            fam.push("ui".into());
        }
    }
    if let Some(b) = load(&(fdir + "cour.ttf")) {
        fonts.font_data.insert("mono".into(), egui::FontData::from_owned(b).into());
        if let Some(fam) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            fam.insert(0, "mono".into());
        }
    }
    ctx.set_fonts(fonts);
    let mut style = (*ctx.global_style()).clone();
    style.spacing.item_spacing = Vec2::new(5.0, 3.0);
    style.spacing.button_padding = Vec2::new(6.0, 2.0);
    style.spacing.interact_size.y = 18.0;
    style.spacing.menu_margin = 2.into();
    use egui::TextStyle::*;
    style.text_styles.insert(Body, FontId::proportional(12.5));
    style.text_styles.insert(Button, FontId::proportional(12.5));
    style.text_styles.insert(Small, FontId::proportional(11.0));
    style.text_styles.insert(Monospace, FontId::monospace(12.5));
    style.text_styles.insert(Heading, FontId::proportional(15.0));
    ctx.set_global_style(style);
    apply_colours(ctx, is_dark());
}

/// A button of the bars: a word in a rounded frame. `active` marks a switch that is on.
fn chip(ui: &mut Ui, text: &str, on: bool, active: bool, tip: &str) -> egui::Response {
    let font = FontId::proportional(11.5);
    let words = ui.fonts_mut(|f| f.layout_no_wrap(text.to_string(), font, if on { ink() } else { grey() }));
    let size = Vec2::new((words.size().x + 12.0).max(22.0), 20.0);
    let (r, resp) = ui.allocate_exact_size(size, if on { Sense::click() } else { Sense::hover() });
    let (fill, edge) = if active {
        (pick(Color32::from_rgb(0xCC, 0xE4, 0xF7), Color32::from_rgb(0x26, 0x4F, 0x78)), pick(Color32::from_rgb(0x00, 0x78, 0xD7), Color32::from_rgb(0x4A, 0x9C, 0xE0)))
    } else if on && resp.hovered() {
        (pick(Color32::from_rgb(0xE5, 0xF1, 0xFB), flat(0x48)), pick(flat(0xA8), flat(0x70)))
    } else {
        (pick(flat(0xFA), flat(0x34)), pick(flat(0xD0), flat(0x48)))
    };
    let body = r.shrink(1.0);
    ui.painter().rect_filled(body, 4.0, fill);
    ui.painter().rect_stroke(body, 4.0, Stroke::new(1.0, edge), egui::StrokeKind::Inside);
    ui.painter().galley(r.center() - words.size() / 2.0, words, ink());
    if tip.is_empty() {
        resp
    } else {
        resp.on_hover_text(tip)
    }
}

/// One named group of the bars: a small caption with its buttons under it, then a rule.
fn group(ui: &mut Ui, caption: &str, add: impl FnOnce(&mut Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);
        ui.add(egui::Label::new(egui::RichText::new(caption).font(FontId::proportional(9.5)).color(gray(110))).selectable(false));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);
            add(ui);
        });
    });
    rule(ui, 32.0);
}

/// A thin upright line between groups of buttons.
fn rule(ui: &mut Ui, height: f32) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(9.0, height), Sense::hover());
    ui.painter().line_segment([Pos2::new(r.center().x, r.top() + 3.0), Pos2::new(r.center().x, r.bottom() - 2.0)], Stroke::new(1.0, gray(200)));
}

fn item(ui: &mut Ui, label: &str, key: &str, cmd: Cmd, on: bool, out: &mut Vec<Cmd>) {
    let mut b = egui::Button::new(label);
    if !key.is_empty() {
        b = b.shortcut_text(key);
    }
    if ui.add_enabled(on, b).clicked() {
        out.push(cmd);
        ui.close();
    }
}

fn check_item(ui: &mut Ui, label: &str, state: bool, cmd: Cmd, on: bool, out: &mut Vec<Cmd>) {
    let mark = if state { "✔ " } else { "    " };
    if ui.add_enabled(on, egui::Button::new(format!("{mark}{label}"))).clicked() {
        out.push(cmd);
        ui.close();
    }
}


/// How narrow and how wide the docked map list can be dragged.
pub const LIST_MIN: f32 = 180.0;
pub const LIST_MAX: f32 = 800.0;
/// The narrowest the list's address and size columns can be dragged.
pub const LIST_ADDR_MIN: f32 = 60.0;
pub const LIST_SIZE_MIN: f32 = 24.0;

impl App {
    pub fn menu_bar(&mut self, ui: &mut Ui) {
        let mut out = vec![];
        let has_proj = self.current_project().is_some_and(|p| !p.versions.is_empty());
        let has_win = self.active_child().is_some();
        let (is_map, has_sel, sel_mode) = self.active_child().map(|c| (c.map_uid.is_some(), c.g.has_selection(), c.g.sel_mode)).unwrap_or((false, false, SelMode::Consecutive));
        let g_flags = self.active_child().map(|c| (c.g.percent, c.g.delta, c.g.no_factor, c.g.show_orig, c.g.right, c.g.fix_right, c.g.dots, c.g.row_markers, c.g.mode));
        let (can_undo, can_redo) = self.current_project().map(|p| (!p.undo.is_empty(), !p.redo.is_empty())).unwrap_or((false, false));
        let recent = self.cfg.recent.clone();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.add_space(3.0);
            ui.menu_button("File", |ui| {
                item(ui, "New project...", "Ctrl+N", Cmd::NewProject, true, &mut out);
                item(ui, "Open project...", "Ctrl+O", Cmd::OpenProject, true, &mut out);
                item(ui, "Versions...", "Ctrl+Shift+O", Cmd::OpenVersion, has_proj, &mut out);
                ui.separator();
                item(ui, "Save project", "Ctrl+S", Cmd::Save, has_proj, &mut out);
                item(ui, "Save all projects", "Ctrl+Shift+S", Cmd::SaveAll, has_proj, &mut out);
                if self.tunerpro.connected() {
                    item(ui, "Send maps to TunerPro (XDF)", "Ctrl+Shift+T", Cmd::SendTunerPro, has_proj, &mut out);
                }
                ui.separator();
                ui.menu_button("Import", |ui| {
                    item(ui, "Data file...", "Ctrl+I", Cmd::ImportFile, true, &mut out);
                    item(ui, "Every file in a folder...", "", Cmd::ImportDir, true, &mut out);
                    ui.separator();
                    item(ui, "TunerPro definition (XDF)...", "", Cmd::ImportXdf, has_proj, &mut out);
                    item(ui, "Map pack...", "", Cmd::ImportPack, has_proj, &mut out);
                    item(ui, "Description file (A2L)...", "Ctrl+D", Cmd::ImportA2l, has_proj, &mut out);
                    ui.separator();
                    item(ui, "Maps from another project...", "Ctrl+Shift+I", Cmd::ImportMaps, has_proj, &mut out);
                    item(ui, "Changes from another project...", "Ctrl+Alt+I", Cmd::ImportChanges, has_proj, &mut out);
                    item(ui, "Map names from a CSV list...", "", Cmd::UpdateCsv, has_proj, &mut out);
                });
                ui.menu_button("Export", |ui| {
                    item(ui, "Data file...", "Ctrl+E", Cmd::ExportFile, has_proj, &mut out);
                    ui.separator();
                    item(ui, "TunerPro definition (XDF)...", "", Cmd::ExportXdf, has_proj, &mut out);
                    item(ui, "Map pack...", "", Cmd::ExportPack, has_proj, &mut out);
                    item(ui, "Map list (CSV)...", "", Cmd::ExportCsv, has_proj, &mut out);
                    ui.separator();
                    item(ui, "Project by mail...", "Ctrl+M", Cmd::SendMail, has_proj, &mut out);
                });
                ui.separator();
                item(ui, "Project details...", "Ctrl+Alt+Enter", Cmd::PropsProject, has_proj, &mut out);
                item(ui, "Version details...", "Shift+Alt+Enter", Cmd::PropsVersion, has_proj, &mut out);
                ui.menu_button("Housekeeping", |ui| {
                    item(ui, "Find projects like this one...", "Ctrl+Alt+O", Cmd::SearchSimilar, has_proj, &mut out);
                    item(ui, "Find duplicate versions...", "", Cmd::SearchDouble, true, &mut out);
                    item(ui, "Refresh the details of all projects...", "", Cmd::UpdateAll, true, &mut out);
                    item(ui, "Recover a project from a backup...", "", Cmd::Rescue, true, &mut out);
                });
                ui.separator();
                item(ui, "Print...", "Ctrl+P", Cmd::Print, has_win, &mut out);
                ui.separator();
                for (i, r) in recent.iter().enumerate() {
                    let name = r.rsplit(['\\', '/']).next().unwrap_or(r);
                    item(ui, &format!("{} {name}", i + 1), "", Cmd::Recent(i), true, &mut out);
                }
                if !recent.is_empty() {
                    ui.separator();
                }
                item(ui, "Close project", "Ctrl+F4", Cmd::CloseProject, has_proj, &mut out);
                item(ui, "Close and delete project", "", Cmd::CloseDeleteProject, has_proj, &mut out);
                item(ui, "Quit", "", Cmd::Exit, true, &mut out);
            });
            if has_win {
                ui.menu_button("Edit", |ui| {
                    item(ui, "Undo", "Ctrl+Z", Cmd::Undo, can_undo, &mut out);
                    item(ui, "Redo", "Ctrl+Y", Cmd::Redo, can_redo, &mut out);
                    item(ui, "Undo several steps...", "", Cmd::UndoMore, can_undo, &mut out);
                    ui.separator();
                    item(ui, "Cut", "Ctrl+X", Cmd::Cut, true, &mut out);
                    item(ui, "Copy", "Ctrl+C", Cmd::Copy, true, &mut out);
                    item(ui, "Paste", "Ctrl+V", Cmd::Paste, true, &mut out);
                    ui.menu_button("Copy as", |ui| {
                        item(ui, "Address", "Ctrl+Shift+C", Cmd::CopyAddress, true, &mut out);
                        item(ui, "Plain text", "", Cmd::CopyText, true, &mut out);
                        item(ui, "C array", "", Cmd::CopyCArray, true, &mut out);
                        ui.separator();
                        item(ui, "Whole map", "", Cmd::CopyMap, is_map, &mut out);
                        item(ui, "Both axes", "", Cmd::CopyBothAxes(false), is_map, &mut out);
                        item(ui, "Both axes, text only", "", Cmd::CopyBothAxes(true), is_map, &mut out);
                        item(ui, "X axis", "", Cmd::CopyX(false), is_map, &mut out);
                        item(ui, "X axis, text only", "", Cmd::CopyX(true), is_map, &mut out);
                        item(ui, "Y axis", "", Cmd::CopyY(false), is_map, &mut out);
                        item(ui, "Y axis, text only", "", Cmd::CopyY(true), is_map, &mut out);
                    });
                    ui.separator();
                    item(ui, "Edit cell", "Enter", Cmd::Inplace, true, &mut out);
                    item(ui, "Edit the selection as text...", "", Cmd::AsText, true, &mut out);
                    item(ui, "Step up", "+ / ,", Cmd::Plus, true, &mut out);
                    item(ui, "Step down", "-", Cmd::Minus, true, &mut out);
                    item(ui, "Set to a value...", "=", Cmd::ChangeAbs, true, &mut out);
                    item(ui, "Add or scale...", "%", Cmd::ChangeRel, true, &mut out);
                    item(ui, "Blend across the selection...", "/", Cmd::ChangeEdit, has_sel, &mut out);
                    item(ui, "Repeat the last change", "F9", Cmd::Again, self.last_change.is_some(), &mut out);
                    item(ui, "Restore original values", "F11", Cmd::OriginalValue, true, &mut out);
                    ui.separator();
                    item(ui, "Checksums...", "F2 / c", Cmd::Checksums, true, &mut out);
                    item(ui, "Correct this checksum", "Alt+F2", Cmd::ApplyChecksum, true, &mut out);
                    item(ui, "Correct all checksums", "", Cmd::ApplyAllChecksums, true, &mut out);
                    ui.separator();
                    item(ui, "Window settings...", "Alt+Enter", Cmd::PropsWindow, true, &mut out);
                });
            }
            ui.menu_button("View", |ui| {
                check_item(ui, "View toolbar", self.cfg.toolbar_view, Cmd::ToggleToolbarView, true, &mut out);
                check_item(ui, "Status bar", self.cfg.status_bar, Cmd::ToggleStatus, true, &mut out);
                ui.menu_button("Window colours", |ui| {
                    use crate::config::Colours;
                    for (label, c) in [("Like TunerPro", Colours::LikeTunerPro), ("Light", Colours::Light), ("Dark", Colours::Dark)] {
                        check_item(ui, label, self.cfg.colours == c, Cmd::Colours(c), true, &mut out);
                    }
                });
                check_item(ui, "Height colours", self.cfg.height_colors, Cmd::HeightColors, true, &mut out);
                if let Some((pct, delta, nof, orig, right, fix, dots, rows, mode)) = g_flags {
                    ui.separator();
                    item(ui, "One more column", "m", Cmd::MoreCols, !is_map, &mut out);
                    item(ui, "One column fewer", "w", Cmd::LessCols, !is_map, &mut out);
                    item(ui, "Start one byte earlier", "Ctrl+Left", Cmd::OriginLeft, !is_map, &mut out);
                    item(ui, "Start one byte later", "Ctrl+Right", Cmd::OriginRight, !is_map, &mut out);
                    item(ui, "Reset view", "z", Cmd::DefaultView, true, &mut out);
                    ui.separator();
                    ui.menu_button("Show numbers as", |ui| {
                        check_item(ui, "Percent of the original", pct, Cmd::Percent, true, &mut out);
                        check_item(ui, "Difference to the original", delta, Cmd::Delta, true, &mut out);
                        check_item(ui, "Stored values, no factor", nof, Cmd::NoFactor, true, &mut out);
                        check_item(ui, "Original values", orig, Cmd::OrigValues, true, &mut out);
                    });
                    ui.menu_button("Beside the numbers", |ui| {
                        check_item(ui, "Nothing", right == RightSide::Nothing, Cmd::Right(RightSide::Nothing), true, &mut out);
                        check_item(ui, "Bars", right == RightSide::Bars, Cmd::Right(RightSide::Bars), true, &mut out);
                        check_item(ui, "Characters", right == RightSide::Ascii, Cmd::Right(RightSide::Ascii), true, &mut out);
                        check_item(ui, "Bars and characters", right == RightSide::Both, Cmd::Right(RightSide::Both), true, &mut out);
                        ui.separator();
                        check_item(ui, "Keep in view", fix, Cmd::FixRight, true, &mut out);
                    });
                    item(ui, "Zoom in", "Ctrl+Mousewheel", Cmd::Zoom, true, &mut out);
                    item(ui, "Zoom out", "Ctrl+Mousewheel", Cmd::Unzoom, true, &mut out);
                    item(ui, "Actual size", "", Cmd::Scale1, mode == ViewMode::TwoD, &mut out);
                    check_item(ui, if mode == ViewMode::ThreeD { "Solid surface" } else { "Join the points" }, dots, Cmd::ConnectDots, mode != ViewMode::Text, &mut out);
                    check_item(ui, "Row dividers", rows, Cmd::RowMarkers, mode == ViewMode::TwoD, &mut out);
                    item(ui, "Fit the value range", "Ctrl+B", Cmd::OptimizeRange, true, &mut out);
                    ui.separator();
                    item(ui, "Compare against...", "", Cmd::ChooseRef, true, &mut out);
                    item(ui, "List of differences", "u", Cmd::Differences, true, &mut out);
                    item(ui, "First difference", "e", Cmd::DiffFirst, true, &mut out);
                    item(ui, "Previous difference", "v", Cmd::DiffPrev, true, &mut out);
                    item(ui, "Next difference", "n", Cmd::DiffNext, true, &mut out);
                    item(ui, "Last difference", "l", Cmd::DiffLast, true, &mut out);
                }
            });
            if has_win {
                ui.menu_button("Select", |ui| {
                    item(ui, "This cell", "Insert", Cmd::SelCell, true, &mut out);
                    item(ui, "This row", "", Cmd::SelRow, true, &mut out);
                    item(ui, "This column", "", Cmd::SelCol, true, &mut out);
                    item(ui, "Everything on screen", "Ctrl+A", Cmd::SelScreen, true, &mut out);
                    item(ui, "Invert on screen", "", Cmd::SelInvert, true, &mut out);
                    item(ui, "By address...", "", Cmd::SelCreate, true, &mut out);
                    ui.separator();
                    item(ui, "Mark the start", "Ctrl+Shift+B", Cmd::SelBegin, true, &mut out);
                    item(ui, "Mark the end", "Ctrl+Shift+K", Cmd::SelEnd, true, &mut out);
                    ui.menu_button("Shape", |ui| {
                        check_item(ui, "Run of cells", sel_mode == SelMode::Consecutive, Cmd::SelModeSet(SelMode::Consecutive), true, &mut out);
                        check_item(ui, "Rectangle", sel_mode == SelMode::Rectangle, Cmd::SelModeSet(SelMode::Rectangle), true, &mut out);
                        check_item(ui, "Whole columns", sel_mode == SelMode::Columns, Cmd::SelModeSet(SelMode::Columns), true, &mut out);
                        check_item(ui, "Whole rows", sel_mode == SelMode::Rows, Cmd::SelModeSet(SelMode::Rows), true, &mut out);
                    });
                    ui.separator();
                    item(ui, "Clear the selection", "Delete", Cmd::SelNone, has_sel, &mut out);
                    item(ui, "Bring back the last selection", "", Cmd::SelRestore, true, &mut out);
                    ui.separator();
                    item(ui, "Load from a file...", "Ctrl+R", Cmd::SelLoad, true, &mut out);
                    item(ui, "Save to a file...", "Ctrl+W", Cmd::SelSave, has_sel, &mut out);
                });
                ui.menu_button("Maps", |ui| {
                    item(ui, "Make a map from the selection", "K", Cmd::SelToMap, !is_map && has_sel, &mut out);
                    item(ui, "Open the map under the cursor", "Shift+K", Cmd::OpenMapAtCursor, !is_map, &mut out);
                    item(ui, "Delete this map", "", Cmd::DeleteMap, true, &mut out);
                    ui.separator();
                    item(ui, "Find a map around the cursor", "f", Cmd::DetectMap(false), !is_map, &mut out);
                    check_item(ui, "Snap selections to the shape of a map", self.cfg.support_selection, Cmd::SupportSel, !is_map, &mut out);
                    ui.separator();
                    let running = self.active_project.is_some_and(|i| self.searches.contains_key(&i));
                    let done = self.current_project().is_some_and(|p| p.search_done);
                    let label = if running {
                        "Stop the map finder"
                    } else if done {
                        "Run the map finder again"
                    } else {
                        "Run the map finder"
                    };
                    item(ui, label, "", Cmd::MapSearchToggle, true, &mut out);
                    item(ui, "Map finder settings...", "", Cmd::MapSearchConfig, true, &mut out);
                    item(ui, "Axis presets...", "", Cmd::AxisProfiles, true, &mut out);
                    item(ui, "Maps like this one...", "", Cmd::ParallelMaps, is_map, &mut out);
                });
                ui.menu_button("Find", |ui| {
                    item(ui, "Bytes or text...", "Ctrl+F / s", Cmd::SearchBytes, true, &mut out);
                    item(ui, "The selected bytes...", "Ctrl+Shift+F", Cmd::SearchSel, has_sel, &mut out);
                    item(ui, "Find next", "F3", Cmd::SearchNext, true, &mut out);
                    item(ui, "Find previous", "Shift+F3", Cmd::SearchBack, true, &mut out);
                    item(ui, "Find from the start", "Alt+F3", Cmd::SearchStart, true, &mut out);
                    item(ui, "Replace...", "", Cmd::Replace, true, &mut out);
                    ui.separator();
                    item(ui, "Go to address...", "Ctrl+G / a", Cmd::GoTo, true, &mut out);
                    ui.separator();
                    item(ui, "Add or edit a comment...", "Shift+Enter", Cmd::CommentEdit, true, &mut out);
                    item(ui, "Remove the comment", "", Cmd::CommentDel, true, &mut out);
                    item(ui, "Previous comment", "Shift+V", Cmd::CommentPrev, true, &mut out);
                    item(ui, "Next comment", "Shift+N", Cmd::CommentNext, true, &mut out);
                    ui.separator();
                    item(ui, "Set or clear a bookmark", "Ctrl+F5", Cmd::MarkerToggle, true, &mut out);
                    item(ui, "Previous bookmark", "Shift+F5", Cmd::MarkerPrev, true, &mut out);
                    item(ui, "Next bookmark", "F5", Cmd::MarkerNext, true, &mut out);
                });
            }
            ui.menu_button("Tools", |ui| {
                item(ui, "Settings...", "F12", Cmd::Config, true, &mut out);
                ui.separator();
                item(ui, "Swap address or data lines...", "", Cmd::SwapLines, true, &mut out);
                item(ui, "Calculator", "", Cmd::Calc, true, &mut out);
            });
            ui.menu_button("Windows", |ui| {
                item(ui, "Map list", "Ctrl+K", Cmd::MapSelection, true, &mut out);
                item(ui, "Differences", "Ctrl+U", Cmd::Differences, has_win, &mut out);
                item(ui, "File overview", "o", Cmd::Overview, has_proj, &mut out);
                item(ui, "3D preview", "p", Cmd::Preview, has_proj, &mut out);
                item(ui, "Project notes...", "", Cmd::ProjectComment, has_proj, &mut out);
                item(ui, "Hide or show floating windows", "Tab", Cmd::ToggleFloat, true, &mut out);
                ui.separator();
                item(ui, "New hexdump window", "", Cmd::InsertHexdump, has_proj, &mut out);
                item(ui, "New map window", "", Cmd::InsertMap, has_win, &mut out);
                item(ui, "Another window like this", "", Cmd::NewWindow, has_proj, &mut out);
                item(ui, "Link windows...", "", Cmd::ConnectWindows, has_win, &mut out);
                ui.separator();
                item(ui, "Cascade", "", Cmd::Cascade, has_win, &mut out);
                item(ui, "Tile", "", Cmd::Tile, has_win, &mut out);
                item(ui, "Close this window", "", Cmd::DeleteWindow, has_win, &mut out);
                item(ui, "Close all map windows", "", Cmd::CloseAllMaps, has_win, &mut out);
                if !self.children.is_empty() {
                    ui.separator();
                }
                let titles: Vec<(u64, String)> = self.children.iter().map(|c| (c.id, self.child_title(c))).collect();
                for (k, (id, t)) in titles.iter().enumerate() {
                    check_item(ui, &format!("{} {t}", k + 1), self.active == Some(*id), Cmd::Activate(*id), true, &mut out);
                }
            });
            ui.menu_button("Help", |ui| {
                item(ui, "Getting started", "F1", Cmd::Help("start"), true, &mut out);
                item(ui, "Keys", "", Cmd::Help("keys"), true, &mut out);
                item(ui, "Mouse", "", Cmd::Help("mouse"), true, &mut out);
                item(ui, "Selecting", "", Cmd::Help("selections"), true, &mut out);
                item(ui, "Right-click menus", "", Cmd::Help("menus"), true, &mut out);
                item(ui, "Dropping files", "", Cmd::Help("drop"), true, &mut out);
                ui.separator();
                item(ui, "Tips...", "", Cmd::Tip, true, &mut out);
                item(ui, "About ALS...", "", Cmd::About, true, &mut out);
            });
        });
        self.pending.extend(out);
    }

    pub fn toolbars(&mut self, ui: &mut Ui) {
        let mut out = vec![];
        let has_proj = self.current_project().is_some_and(|p| !p.versions.is_empty());
        let has = self.active_child().is_some();
        let g = self.active_child().map(|c| c.g.clone());
        let is_map = self.active_child().is_some_and(|c| c.map_uid.is_some());
        let elements: Vec<String> = self.current_project().map(|p| p.elements.iter().map(|e| e.usage.clone()).collect()).unwrap_or_default();
        let element_label = self.current_project().map(|p| p.element_label());
        let (nav, overv, prev) = (self.list.visible, self.show_overview, self.show_preview);
        let (heights, snap, dim) = (self.cfg.height_colors, self.cfg.support_selection, self.cfg.show_pale);
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        ui.add_space(2.0);
        // Two rows of named groups; every button is a word.
        let row = |ui: &mut Ui, f: &mut dyn FnMut(&mut Ui)| {
            ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 35.0), egui::Layout::left_to_right(egui::Align::Min), |ui| {
                ui.add_space(6.0);
                f(ui);
            });
        };
        let mut part: Option<Option<usize>> = None;
        row(ui, &mut |ui| {
            let mut b = |ui: &mut Ui, text: &str, on: bool, active: bool, tip: &str, cmd: Cmd| {
                if chip(ui, text, on, active, tip).clicked() {
                    out.push(cmd);
                }
            };
            group(ui, "Project", |ui| {
                b(ui, "New", true, false, "New project (Ctrl+N)", Cmd::NewProject);
                b(ui, "Open", true, false, "Open a project (Ctrl+O)", Cmd::OpenProject);
                b(ui, "Print", has, false, "Print (Ctrl+P)", Cmd::Print);
            });
            group(ui, "Windows", |ui| {
                b(ui, "Hex", has_proj, false, "New hexdump window", Cmd::InsertHexdump);
                b(ui, "Map", has, false, "New map window", Cmd::InsertMap);
                b(ui, "Tile", has, false, "Tile the windows", Cmd::Tile);
                b(ui, "Link", has, false, "Link windows, so they scroll together", Cmd::ConnectWindows);
            });
            group(ui, "Panels", |ui| {
                b(ui, "List", true, nav, "Map list (Ctrl+K)", Cmd::MapSelection);
                b(ui, "Overview", has_proj, overv, "File overview (o)", Cmd::Overview);
                b(ui, "Preview", has_proj, prev, "3D preview (p)", Cmd::Preview);
            });
            group(ui, "Find", |ui| {
                b(ui, "Prev", has, false, "Find previous (Shift+F3)", Cmd::SearchBack);
                b(ui, "Find", has, false, "Find bytes or text (Ctrl+F)", Cmd::SearchBytes);
                b(ui, "Sel.", has, false, "Find the selected bytes (Ctrl+Shift+F)", Cmd::SearchSel);
                b(ui, "Next", has, false, "Find next (F3)", Cmd::SearchNext);
            });
            group(ui, "Differences", |ui| {
                b(ui, "First", has, false, "First difference (e)", Cmd::DiffFirst);
                b(ui, "Prev", has, false, "Previous difference (v)", Cmd::DiffPrev);
                b(ui, "List", has, false, "List of differences (u)", Cmd::Differences);
                b(ui, "Next", has, false, "Next difference (n)", Cmd::DiffNext);
                b(ui, "Last", has, false, "Last difference (l)", Cmd::DiffLast);
            });
            group(ui, "Details", |ui| {
                b(ui, "Project", has_proj, false, "Project details (Ctrl+Alt+Enter)", Cmd::PropsProject);
                b(ui, "Version", has_proj, false, "Version details (Shift+Alt+Enter)", Cmd::PropsVersion);
                b(ui, "Window", has_proj, false, "Window settings (Alt+Enter)", Cmd::PropsWindow);
            });
            group(ui, "Part", |ui| {
                ui.add_enabled_ui(has_proj, |ui| {
                    let label = element_label.clone().unwrap_or_else(|| "All".to_string());
                    ui.menu_button(label, |ui| {
                        for (i, e) in elements.iter().enumerate() {
                            if ui.button(e).clicked() {
                                part = Some(Some(i));
                                ui.close();
                            }
                        }
                        if ui.button("All parts").clicked() {
                            part = Some(None);
                            ui.close();
                        }
                    });
                });
            });
            group(ui, "Help", |ui| {
                b(ui, "Help", true, false, "Help (F1)", Cmd::Help("start"));
                b(ui, "Keys", true, false, "Keys", Cmd::Help("keys"));
            });
        });
        if let Some(p) = part {
            out.push(Cmd::SelectElement(p));
        }
        row(ui, &mut |ui| {
            let mut b = |ui: &mut Ui, text: &str, on: bool, active: bool, tip: &str, cmd: Cmd| {
                if chip(ui, text, on, active, tip).clicked() {
                    out.push(cmd);
                }
            };
            let mode = g.as_ref().map(|g| g.mode);
            group(ui, "View", |ui| {
                b(ui, "Numbers", has, mode == Some(ViewMode::Text), "Numbers (t steps through the views)", Cmd::Mode(ViewMode::Text));
                b(ui, "2D", has, mode == Some(ViewMode::TwoD), "2D graph", Cmd::Mode(ViewMode::TwoD));
                b(ui, "3D", has, mode == Some(ViewMode::ThreeD), "3D graph", Cmd::Mode(ViewMode::ThreeD));
            });
            group(ui, "Zoom", |ui| {
                b(ui, "A-", has, false, "Smaller (Ctrl+Mousewheel)", Cmd::Unzoom);
                let size = g.as_ref().map(|g| format!("{:.0} pt", g.font)).unwrap_or_else(|| "pt".to_string());
                let (r, _) = ui.allocate_exact_size(Vec2::new(34.0, 20.0), Sense::hover());
                ui.painter().text(r.center(), Align2::CENTER_CENTER, size, FontId::proportional(11.5), if has { ink() } else { grey() });
                b(ui, "A+", has, false, "Larger (Ctrl+Mousewheel)", Cmd::Zoom);
            });
            group(ui, "Columns", |ui| {
                b(ui, "-", has && !is_map, false, "One column fewer (w)", Cmd::LessCols);
                b(ui, "+", has && !is_map, false, "One more column (m)", Cmd::MoreCols);
                b(ui, "Reset", has, false, "Reset view (z)", Cmd::DefaultView);
            });
            let w = g.as_ref().map(|g| (g.width(), g.org.is_float()));
            group(ui, "Width", |ui| {
                b(ui, "8", has, w == Some((1, false)), "8 bit", Cmd::Bits(8));
                b(ui, "16", has, w == Some((2, false)), "16 bit", Cmd::Bits(16));
                b(ui, "32", has, w == Some((4, false)), "32 bit", Cmd::Bits(32));
                b(ui, "Float", has, w == Some((4, true)), "Floating point", Cmd::Bits(0));
            });
            let radix = g.as_ref().map(|g| g.radix);
            group(ui, "Base", |ui| {
                b(ui, "Dec", has, radix == Some(10), "Decimal", Cmd::Radix(10));
                b(ui, "Hex", has, radix == Some(16), "Hexadecimal", Cmd::Radix(16));
            });
            let be = g.as_ref().map(|g| g.org.big_endian()).unwrap_or(false);
            let multi = g.as_ref().is_some_and(|g| g.width() > 1);
            group(ui, "Bytes", |ui| {
                b(ui, if be { "High first" } else { "Low first" }, multi, false, "Byte order: click to swap low byte first and high byte first", Cmd::SwapEndian);
                b(ui, "Signed", has, g.as_ref().is_some_and(|g| g.signed), "Signed", Cmd::Signed);
            });
            group(ui, "Show", |ui| {
                b(ui, "Bars", has && !is_map, g.as_ref().is_some_and(|g| matches!(g.right, RightSide::Bars | RightSide::Both)), "Bars beside the numbers", Cmd::Right(RightSide::Bars));
                b(ui, "%", has, g.as_ref().is_some_and(|g| g.percent), "Percent of the original", Cmd::Percent);
                b(ui, "Diff", has, g.as_ref().is_some_and(|g| g.delta), "Difference to the original", Cmd::Delta);
                b(ui, "Raw", has, g.as_ref().is_some_and(|g| g.no_factor), "Stored values, no factor", Cmd::NoFactor);
                b(ui, "Original", has, g.as_ref().is_some_and(|g| g.show_orig), "Original values", Cmd::OrigValues);
                b(ui, "Compare", has, false, "Compare against a version", Cmd::ChooseRef);
            });
            group(ui, "Colours", |ui| {
                let r = chip(ui, "Heights", true, heights, "Height colours: each value takes a colour by how high it is");
                // A strip of the scale under the word.
                let strip = Rect::from_min_max(Pos2::new(r.rect.left() + 5.0, r.rect.bottom() - 4.0), Pos2::new(r.rect.right() - 5.0, r.rect.bottom() - 2.0));
                for k in 0..10 {
                    let (a, z) = (strip.left() + strip.width() * k as f32 / 10.0, strip.left() + strip.width() * (k + 1) as f32 / 10.0);
                    ui.painter().rect_filled(Rect::from_min_max(Pos2::new(a, strip.top()), Pos2::new(z, strip.bottom())), 0.0, view::height_colour(k as f32 / 9.0));
                }
                if r.clicked() {
                    out.push(Cmd::HeightColors);
                }
                if chip(ui, "Dim", true, dim, "Program code and empty areas are dimmed (set in Settings)").clicked() {
                    out.push(Cmd::Config);
                }
            });
            group(ui, "Maps", |ui| {
                if chip(ui, "Snap", true, snap, "Snap selections to the shape of a map").clicked() {
                    out.push(Cmd::SupportSel);
                }
                if chip(ui, "Join", has, g.as_ref().is_some_and(|g| g.dots), "Join the points of the graph").clicked() {
                    out.push(Cmd::ConnectDots);
                }
            });
        });
        self.pending.extend(out);
    }

    pub fn status_bar(&mut self, ui: &mut Ui) {
        let hint = self.status_hint.clone();
        let search = self.search_status.clone();
        let cs = self.current_project().map(crate::checksum::status_text).unwrap_or_default();
        let cursor = self.cursor_status();
        let has_proj = self.current_project().is_some();
        let w = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 20.0), Sense::hover());
        let painter = ui.painter_at(rect);
        let f = FontId::proportional(12.0);
        painter.text(rect.left_center() + Vec2::new(3.0, 0.0), Align2::LEFT_CENTER, if search.is_empty() { &hint } else { &search }, f.clone(), ink());
        let div = |x: f32| {
            painter.line_segment([Pos2::new(x, rect.top() + 2.0), Pos2::new(x, rect.bottom() - 2.0)], Stroke::new(1.0, gray(210)));
        };
        if rect.width() < 400.0 {
            return;
        }
        // Four areas: hints, checksums, the link to TunerPro, the cursor.
        let right = rect.right();
        let cursor_w = (rect.width() * 0.27).max(330.0);
        let hw_w = (rect.width() * 0.11).max(100.0);
        let cs_w = (rect.width() * 0.09).max(80.0);
        let x_cursor = right - cursor_w;
        let x_hw = x_cursor - hw_w;
        let x_cs = x_hw - cs_w;
        div(x_cs);
        div(x_hw);
        div(x_cursor);
        if has_proj {
            painter.text(Pos2::new(x_cs + 6.0, rect.center().y), Align2::LEFT_CENTER, &cs, f.clone(), ink());
            painter.text(Pos2::new(x_hw + 6.0, rect.center().y), Align2::LEFT_CENTER, if self.tunerpro.connected() { "TunerPro" } else { "" }, f.clone(), ink());
        }
        painter.text(Pos2::new(x_cursor + 6.0, rect.center().y), Align2::LEFT_CENTER, &cursor, f.clone(), ink());
        // Grip
        for k in 0..3 {
            let p0 = rect.right_bottom() - Vec2::new(3.0 + k as f32 * 4.0, 3.0);
            painter.circle_filled(p0, 0.9, gray(150));
        }
    }

    pub fn child_title(&self, c: &view::Child) -> String {
        let Some(p) = self.project(c.project) else { return String::new() };
        let name = p.map_title_name();
        let ver = p.version_name();
        let what = match c.map_uid.and_then(|u| p.maps.iter().find(|m| m.uid == u)) {
            Some(m) => m.name.clone(),
            None => "Hexdump".to_string(),
        };
        let soft = if p.props.sw_version.is_empty() { String::new() } else { format!(", {}", p.props.sw_version) };
        let dirty = if p.dirty_data || p.dirty_struct { " *" } else { "" };
        format!("{name} ({ver}){soft}, {what}{dirty}")
    }

    pub fn workspace_windows(&mut self, ctx: &egui::Context, ws: Rect) {
        // Windows keep their place in the workspace when its corner moves, as when the map list
        // is dragged wider or folded away. Left where they were, they would end up under the
        // list, unseen but still catching the mouse.
        let moved = ws.min - self.workspace.min;
        if moved != Vec2::ZERO {
            for c in &mut self.children {
                c.rect = c.rect.translate(moved);
                c.restore = c.restore.map(|r| r.translate(moved));
            }
        }
        self.workspace = ws;
        // Moves and sizes first, so that the order of windows stays stable.
        let ids: Vec<u64> = self.children.iter().map(|c| c.id).collect();
        let mut out: Vec<Cmd> = vec![];
        let mut close = vec![];
        for id in ids {
            let Some(i) = self.child_index(id) else { continue };
            let mut child = self.children[i].clone();
            let Some(pi) = self.project_index(child.project) else {
                close.push(id);
                continue;
            };
            let title = self.child_title(&child);
            let active = self.active == Some(id);
            let maximized = child.restore.is_some();
            let rect = if maximized { ws } else { child.rect };
            let rect = if child.minimized { Rect::from_min_size(rect.min, Vec2::new(240.0, 26.0)) } else { rect };
            let area_id = egui::Id::unique(("child", id));
            let mut activate = false;
            let mut closed = false;
            let mut drag_move = Vec2::ZERO;
            let mut drag_size = Vec2::ZERO;
            let mut toggle_max = false;
            let mut toggle_min = false;
            let resp = egui::Area::new(area_id).order(egui::Order::Middle).fixed_pos(rect.min).constrain(false).movable(false).show(ctx, |ui| {
                ui.set_clip_rect(ws);
                ui.set_min_size(rect.size());
                ui.set_max_size(rect.size());
                let painter = ui.painter();
                painter.rect_filled(rect, 0.0, bar_bg());
                painter.rect_stroke(rect, 0.0, Stroke::new(1.0, gray(if active { 80 } else { 150 })), egui::StrokeKind::Inside);
                let bar = Rect::from_min_size(rect.min + Vec2::splat(1.0), Vec2::new(rect.width() - 2.0, 24.0));
                let (l, r) = if active { (rgb(0x99, 0xB4, 0xD1), rgb(0xB9, 0xD1, 0xEA)) } else { (rgb(0xC6, 0xD4, 0xE2), rgb(0xD3, 0xE1, 0xEE)) };
                let mut mesh = egui::epaint::Mesh::default();
                mesh.colored_vertex(bar.left_top(), l);
                mesh.colored_vertex(bar.right_top(), r);
                mesh.colored_vertex(bar.right_bottom(), r);
                mesh.colored_vertex(bar.left_bottom(), l);
                mesh.add_triangle(0, 1, 2);
                mesh.add_triangle(0, 2, 3);
                painter.add(Shape::mesh(mesh));
                let ico = Rect::from_min_size(bar.min + Vec2::new(6.0, 4.0), Vec2::new(14.0, 14.0));
                painter.rect_filled(ico, 1.0, rgb(240, 210, 120));
                painter.rect_stroke(ico, 1.0, Stroke::new(1.0, gray(80)), egui::StrokeKind::Inside);
                let title_col = if active { ink() } else { gray(90) };
                let clip = ui.painter_at(bar.with_max_x(bar.right() - 84.0));
                clip.text(bar.left_center() + Vec2::new(26.0, 0.0), Align2::LEFT_CENTER, &title, FontId::proportional(12.5), title_col);
                let bar_resp = ui.interact(bar.with_max_x(bar.right() - 84.0), area_id.with("title"), Sense::click_and_drag());
                if bar_resp.dragged() && !maximized {
                    drag_move = bar_resp.drag_delta();
                }
                if bar_resp.double_clicked() {
                    toggle_max = true;
                }
                for (k, label) in ["─", "❐", "✕"].iter().enumerate() {
                    let br = Rect::from_min_size(Pos2::new(bar.right() - 84.0 + k as f32 * 28.0, bar.top() + 3.0), Vec2::new(26.0, 18.0));
                    let br_resp = ui.interact(br, area_id.with(("btn", k)), Sense::click());
                    let fill = if br_resp.hovered() { if k == 2 { rgb(232, 17, 35) } else { gray(230) } } else { gray(0xE8).gamma_multiply(0.0) };
                    let p2 = ui.painter();
                    p2.rect_filled(br, 2.0, fill);
                    p2.rect_stroke(br, 2.0, Stroke::new(1.0, gray(150)), egui::StrokeKind::Inside);
                    p2.text(br.center(), Align2::CENTER_CENTER, *label, FontId::proportional(11.0), ink());
                    if br_resp.clicked() {
                        match k {
                            0 => toggle_min = true,
                            1 => toggle_max = true,
                            _ => closed = true,
                        }
                    }
                }
                if child.minimized {
                    return;
                }
                // Inner toolbar
                let inner_bar = Rect::from_min_size(Pos2::new(rect.left() + 1.0, bar.bottom()), Vec2::new(rect.width() - 2.0, 25.0));
                ui.scope_builder(egui::UiBuilder::new().max_rect(inner_bar).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);
                    ui.add_space(5.0);
                    let groups: [&[(&str, &str, Cmd)]; 3] = [
                        &[("Cut", "Cut (Ctrl+X)", Cmd::Cut), ("Copy", "Copy (Ctrl+C)", Cmd::Copy), ("Paste", "Paste (Ctrl+V)", Cmd::Paste)],
                        &[
                            ("Make map", "Make a map from the selection (k)", Cmd::SelToMap),
                            ("Open map", "Open the map under the cursor (Shift+K)", Cmd::OpenMapAtCursor),
                            ("Delete map", "Delete the map under the cursor", Cmd::DeleteMap),
                        ],
                        &[("Checksums", "Checksums (F2)", Cmd::Checksums), ("Correct", "Correct this checksum (Alt+F2)", Cmd::ApplyChecksum)],
                    ];
                    for (k, cmds) in groups.iter().enumerate() {
                        if k > 0 {
                            rule(ui, 22.0);
                        }
                        for (text, tip, cmd) in cmds.iter() {
                            if chip(ui, text, true, false, tip).clicked() {
                                out.push(Cmd::Activate(id));
                                out.push(cmd.clone());
                            }
                        }
                    }
                });
                let body_rect = Rect::from_min_max(Pos2::new(rect.left() + 1.0, inner_bar.bottom()), rect.max - Vec2::splat(1.0));
                let env = env_for(self, &child, &self.projects[pi]);
                let mut local = vec![];
                let mut grid_child = child.clone();
                view::body(ui, body_rect, &mut grid_child, &env, &mut local);
                child.g = grid_child.g;
                if !local.is_empty() {
                    out.push(Cmd::Activate(id));
                    out.extend(local);
                }
                // Corner for resizing
                if !maximized {
                    let grip = Rect::from_min_size(rect.right_bottom() - Vec2::splat(14.0), Vec2::splat(14.0));
                    let g_resp = ui.interact(grip, area_id.with("grip"), Sense::drag());
                    if g_resp.hovered() || g_resp.dragged() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
                    }
                    if g_resp.dragged() {
                        drag_size = g_resp.drag_delta();
                    }
                }
            });
            // A press inside this window's own layer makes it the active window.
            let layer = resp.response.layer_id;
            let hit = ctx.input(|inp| inp.pointer.any_pressed()) && ctx.input(|inp| inp.pointer.interact_pos()).is_some_and(|pos| rect.contains(pos) && ctx.layer_id_at(pos) == Some(layer));
            if hit {
                activate = true;
            }
            let Some(i) = self.child_index(id) else { continue };
            let c = &mut self.children[i];
            c.g = child.g;
            c.rect = c.rect.translate(drag_move);
            if drag_size != Vec2::ZERO {
                c.rect.max += drag_size;
                c.rect.max.x = c.rect.max.x.max(c.rect.min.x + 220.0);
                c.rect.max.y = c.rect.max.y.max(c.rect.min.y + 120.0);
            }
            if toggle_max {
                if let Some(r) = c.restore.take() {
                    c.rect = r;
                } else {
                    c.restore = Some(c.rect);
                }
                c.minimized = false;
            }
            if toggle_min {
                c.minimized = !c.minimized;
                c.restore = None;
            }
            if closed {
                close.push(id);
            }
            if activate {
                self.active = Some(id);
                let pr = self.children[i].project;
                self.active_project = Some(pr);
            }
        }
        for id in close {
            self.close_child(id);
        }
        self.pending.extend(out);
    }

    // ---------- the map list ----------
    pub fn map_list(&mut self, ui: &mut Ui) {
        let mut out: Vec<Cmd> = vec![];
        let has_proj = self.current_project().is_some_and(|p| !p.versions.is_empty());
        let top = ui.max_rect();
        // Buttons above the list
        let row_r = Rect::from_min_size(top.min + Vec2::new(0.0, 4.0), Vec2::new(top.width(), 50.0));
        ui.painter().rect_filled(top, 0.0, panel_bg());
        // Two rows of word buttons for what is done to a whole project.
        let recent = self.cfg.recent.clone();
        let rows: [&[(&str, bool, &str, Option<Cmd>)]; 2] = [
            &[
                ("New", true, "New project (Ctrl+N)", Some(Cmd::NewProject)),
                ("Open", true, "Open a project (Ctrl+O)", Some(Cmd::OpenProject)),
                ("Recent", true, "Projects opened lately", None),
                ("Save", has_proj, "Save the project (Ctrl+S)", Some(Cmd::Save)),
            ],
            &[
                ("Versions", has_proj, "The versions of this project (Ctrl+Shift+O)", Some(Cmd::OpenVersion)),
                ("Import", true, "Import a data file (Ctrl+I)", Some(Cmd::ImportFile)),
                ("Export", has_proj, "Export a data file (Ctrl+E)", Some(Cmd::ExportFile)),
                ("Notes", has_proj, "Project notes", Some(Cmd::ProjectComment)),
            ],
        ];
        for (k, row) in rows.iter().enumerate() {
            let line = Rect::from_min_size(Pos2::new(row_r.left() + 5.0, row_r.top() + k as f32 * 24.0), Vec2::new(row_r.width() - 10.0, 22.0));
            ui.scope_builder(egui::UiBuilder::new().max_rect(line).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);
                for (text, on, tip, cmd) in row.iter() {
                    let resp = chip(ui, text, *on, false, tip);
                    match cmd {
                        Some(c) => {
                            if resp.clicked() {
                                out.push(c.clone());
                            }
                        }
                        None => {
                            egui::Popup::menu(&resp).show(|ui| {
                                if recent.is_empty() {
                                    ui.label("Nothing opened yet");
                                }
                                for (i, r) in recent.iter().enumerate() {
                                    let name = r.rsplit(['\\', '/']).next().unwrap_or(r);
                                    if ui.button(name).clicked() {
                                        out.push(Cmd::Recent(i));
                                        ui.close();
                                    }
                                }
                            });
                        }
                    }
                }
            });
        }
        // Search field
        let field = Rect::from_min_size(top.min + Vec2::new(5.0, 62.0), Vec2::new(top.width() - 10.0, 22.0));
        let te = egui::TextEdit::singleline(&mut self.list.filter).hint_text("Filter by name or address").frame(egui::Frame::NONE.fill(paper()).stroke(Stroke::new(1.0, gray(120))).inner_margin(egui::Margin::symmetric(3, 2))).desired_width(field.width() - 10.0);
        ui.put(field, te);
        // Filter row
        let fy = field.bottom() + 3.0;
        ui.painter().text(Pos2::new(top.left() + 5.0, fy + 10.0), Align2::LEFT_CENTER, "Filter:", FontId::proportional(12.0), ink());
        let filt = Rect::from_min_size(Pos2::new(top.left() + 40.0, fy), Vec2::new(top.width() - 45.0, 20.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(filt).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(3.0, 0.0);
            let label = match self.list.bits {
                8 => "8",
                16 => "16",
                32 => "32",
                _ => "∗",
            };
            ui.menu_button(label, |ui| {
                for (t, v) in [("All bit widths", 0), ("8 Bit", 8), ("16 Bit", 16), ("32 Bit", 32)] {
                    if ui.radio(self.list.bits == v, t).clicked() {
                        self.list.bits = v;
                        ui.close();
                    }
                }
            });
            if ui.selectable_label(self.list.changed_only, "Δ").on_hover_text("Only maps that differ from the original").clicked() {
                self.list.changed_only = !self.list.changed_only;
            }
            if ui.selectable_label(self.list.flagged_only, "⚑").on_hover_text("Only flagged maps").clicked() {
                self.list.flagged_only = !self.list.flagged_only;
            }
            if ui.selectable_label(self.list.show_hidden, "H").on_hover_text("Also show hidden maps").clicked() {
                self.list.show_hidden = !self.list.show_hidden;
            }
            if ui.selectable_label(self.list.hide_potential, "KK").on_hover_text("Hide suggested maps").clicked() {
                self.list.hide_potential = !self.list.hide_potential;
            }
            if ui.button("Off").on_hover_text("Turn every filter off").clicked() {
                self.list.bits = 0;
                self.list.changed_only = false;
                self.list.flagged_only = false;
                self.list.hide_potential = false;
                self.list.filter.clear();
            }
            ui.menu_button("▾", |ui| {
                item(ui, "Export the map list (CSV)...", "", Cmd::ExportCsv, has_proj, &mut out);
                item(ui, "Axis presets...", "", Cmd::AxisProfiles, has_proj, &mut out);
                ui.checkbox(&mut self.list.sort_by_name, "Sort by name");
            });
        });
        // Head of the list
        let head = Rect::from_min_size(Pos2::new(top.left() + 3.0, fy + 23.0), Vec2::new(top.width() - 10.0, 24.0));
        ui.painter().rect_filled(head, 0.0, paper());
        let hf = FontId::proportional(12.0);
        // Two column edges can be dragged: where the name begins and where the size begins.
        // The name always keeps some room between them.
        let room = (head.width() - 60.0).max(120.0);
        let addr_w = self.cfg.list_addr_w.clamp(LIST_ADDR_MIN, room - 30.0);
        let size_w = self.cfg.list_size_w.clamp(LIST_SIZE_MIN, room - addr_w);
        let (x_m, x_flag, x_addr) = (head.left() + 7.0, head.left() + 26.0, head.left() + 50.0);
        let (x_name, x_size) = (head.left() + addr_w, head.right() - size_w);
        let cy = head.center().y;
        let column = |from: f32, to: f32| ui.painter_at(Rect::from_min_max(Pos2::new(from, head.top()), Pos2::new(to.max(from), head.bottom())));
        column(head.left(), x_name - 2.0).text(Pos2::new(x_m, cy), Align2::LEFT_CENTER, "M", hf.clone(), ink());
        column(head.left(), x_name - 2.0).text(Pos2::new(x_flag, cy), Align2::LEFT_CENTER, "⁄", hf.clone(), ink());
        column(head.left(), x_name - 2.0).text(Pos2::new(x_addr, cy), Align2::LEFT_CENTER, "Address", hf.clone(), ink());
        column(x_name, x_size - 2.0).text(Pos2::new(x_name + 6.0, cy), Align2::LEFT_CENTER, "Name", hf.clone(), ink());
        column(x_size, head.right()).text(Pos2::new(x_size + 6.0, cy), Align2::LEFT_CENTER, "Size", hf.clone(), ink());
        for x in [x_m + 16.0, x_flag + 20.0, x_name, x_size] {
            ui.painter().line_segment([Pos2::new(x, head.top()), Pos2::new(x, head.bottom())], Stroke::new(1.0, gray(200)));
        }
        if ui.interact(head, ui.scope_id().with("list-head"), Sense::click()).on_hover_text("Double-click to dock the list or let it float. Drag a column edge to make it wider or narrower.").double_clicked() {
            self.list.floating = !self.list.floating;
        }
        for (key, x, name_edge) in [("col-name", x_name, true), ("col-size", x_size, false)] {
            let grip = Rect::from_min_max(Pos2::new(x - 3.0, head.top()), Pos2::new(x + 3.0, head.bottom()));
            let drag = ui.interact(grip, ui.scope_id().with(key), Sense::drag()).on_hover_cursor(egui::CursorIcon::ResizeColumn);
            if drag.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
                if let Some(p) = drag.interact_pointer_pos() {
                    if name_edge {
                        self.cfg.list_addr_w = (p.x - head.left()).clamp(LIST_ADDR_MIN, room - 30.0);
                    } else {
                        self.cfg.list_size_w = (head.right() - p.x).clamp(LIST_SIZE_MIN, room - addr_w);
                    }
                }
            }
        }
        let list_rect = Rect::from_min_max(Pos2::new(top.left() + 3.0, head.bottom()), Pos2::new(top.right() - 7.0, top.bottom() - 2.0));
        ui.painter().rect_filled(list_rect, 0.0, c(self.cfg.theme.list_bg));
        // Rows
        let rows = self.list_rows();
        let txt = c(self.cfg.theme.list_text);
        let mut list_ui = ui.new_child(egui::UiBuilder::new().max_rect(list_rect).layout(egui::Layout::top_down(egui::Align::Min)));
        let row_h = 15.0;
        egui::ScrollArea::vertical().id_salt("maplist").auto_shrink([false, false]).show_rows(&mut list_ui, row_h, rows.len(), |ui, range| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            for row in rows[range].iter() {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), row_h), Sense::click());
                let selected = row.key().is_some() && self.list.selected == row.key();
                let p = ui.painter();
                if selected {
                    p.rect_filled(r, 0.0, Color32::from_rgb(0, 80, 160));
                }
                let bold = FontId::new(12.0, egui::FontFamily::Proportional);
                match row {
                    ListRow::Project { pid, text } => {
                        p.text(r.left_center() + Vec2::new(2.0, 0.0), Align2::LEFT_CENTER, text, bold.clone(), txt);
                        p.text(r.left_center() + Vec2::new(2.3, 0.0), Align2::LEFT_CENTER, text, bold.clone(), txt);
                        if resp.clicked() {
                            self.active_project = Some(*pid);
                        }
                        let pid = *pid;
                        resp.context_menu(|ui| {
                            item(ui, "Properties: Project...", "", Cmd::PropsProject, true, &mut out);
                            item(ui, "Open: Version...", "", Cmd::OpenVersion, true, &mut out);
                            item(ui, "Save", "", Cmd::Save, true, &mut out);
                            item(ui, "Close", "", Cmd::CloseProject, true, &mut out);
                            let _ = pid;
                        });
                    }
                    ListRow::Folder { pid, name, label } => {
                        let fr = Rect::from_min_size(r.left_center() + Vec2::new(4.0, -5.0), Vec2::new(13.0, 10.0));
                        p.rect_filled(fr, 1.0, Color32::from_rgb(240, 210, 100));
                        p.text(r.left_center() + Vec2::new(20.0, 0.0), Align2::LEFT_CENTER, label, bold.clone(), txt);
                        p.text(r.left_center() + Vec2::new(20.3, 0.0), Align2::LEFT_CENTER, label, bold.clone(), txt);
                        if resp.clicked() {
                            let key = format!("{pid}/{name}");
                            if !self.list.collapsed.remove(&key) {
                                self.list.collapsed.insert(key);
                            }
                        }
                    }
                    ListRow::Item { pid, uid, addr, name, size, kind, flag, changed, hidden, open } => {
                        let col = if *hidden {
                            txt.gamma_multiply(0.5)
                        } else if *changed {
                            Color32::from_rgb(255, 110, 110)
                        } else {
                            txt
                        };
                        let f = FontId::proportional(12.0);
                        let fr = Rect::from_min_size(r.left_top() + Vec2::new(6.0, 3.0), Vec2::splat(9.0));
                        p.rect_stroke(fr, 0.0, Stroke::new(1.0, col), egui::StrokeKind::Inside);
                        if *flag {
                            p.rect_filled(fr.shrink(2.0), 0.0, Color32::from_rgb(255, 80, 80));
                        }
                        p.text(Pos2::new(r.left() + 20.0, r.center().y), Align2::LEFT_CENTER, addr, f.clone(), col);
                        let name_clip = ui.painter_at(Rect::from_min_max(Pos2::new(r.left() + addr_w + 1.0, r.top()), Pos2::new((r.right() - size_w).max(r.left() + addr_w + 1.0), r.bottom())));
                        name_clip.text(Pos2::new(r.left() + addr_w + 3.0, r.center().y), Align2::LEFT_CENTER, name, f.clone(), col);
                        if *open {
                            name_clip.text(Pos2::new(r.left() + addr_w + 3.3, r.center().y), Align2::LEFT_CENTER, name, f.clone(), col);
                        }
                        ui.painter().text(Pos2::new(r.right() - 2.0, r.center().y), Align2::RIGHT_CENTER, size, f, col);
                        let local_x = resp.interact_pointer_pos().map(|q| q.x - r.left());
                        let (pid, uid, kind) = (*pid, *uid, *kind);
                        if resp.clicked() {
                            self.list.selected = Some((pid, uid));
                            self.active_project = Some(pid);
                            if local_x.is_some_and(|x| x < 18.0) {
                                if let Some(pr) = self.project_mut(pid) {
                                    if let Some(m) = pr.maps.iter_mut().find(|m| m.uid == uid) {
                                        m.flag = !m.flag;
                                    }
                                    pr.dirty_struct = true;
                                }
                            } else {
                                out.push(Cmd::ListJump(pid, uid, kind));
                            }
                        }
                        if resp.double_clicked() {
                            out.push(Cmd::ListOpen(pid, uid, kind));
                        }
                        resp.context_menu(|ui| {
                            if ui.button("Open / close").clicked() {
                                out.push(Cmd::ListOpen(pid, uid, kind));
                                ui.close();
                            }
                            if kind == RowKind::Potential {
                                if ui.button("Keep it as a map").clicked() {
                                    out.push(Cmd::ListOpen(pid, uid, kind));
                                    ui.close();
                                }
                                if ui.button("Drop this suggestion").clicked() {
                                    out.push(Cmd::ListRemove(pid, uid, kind));
                                    ui.close();
                                }
                            } else if kind == RowKind::Map {
                                if ui.button("Duplicate").clicked() {
                                    out.push(Cmd::ListDuplicate(pid, uid));
                                    ui.close();
                                }
                                if ui.button("Hide / show").clicked() {
                                    out.push(Cmd::ListHide(pid, uid));
                                    ui.close();
                                }
                                if ui.button("Delete map").clicked() {
                                    out.push(Cmd::ListRemove(pid, uid, kind));
                                    ui.close();
                                }
                            }
                        });
                    }
                }
            }
        });
        if !self.list.floating {
            // The right edge drags to make the list wider or narrower. The fold tab below is
            // added after it, so a click on the tab still folds the list.
            let edge = Rect::from_min_max(Pos2::new(top.right() - 6.0, top.top()), top.right_bottom());
            let grip = ui.interact(edge, ui.scope_id().with("list-width"), Sense::drag()).on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
            if grip.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                if let Some(p) = grip.interact_pointer_pos() {
                    self.cfg.list_width = (p.x - top.left() + 3.0).clamp(LIST_MIN, LIST_MAX);
                }
            }
            let tab = Rect::from_min_size(Pos2::new(top.right() - 8.0, top.center().y - 25.0), Vec2::new(8.0, 50.0));
            let r = ui.interact(tab, ui.scope_id().with("fold"), Sense::click()).on_hover_text("Hide the map list");
            ui.painter().rect_filled(tab, 2.0, paper());
            let cx = tab.center();
            ui.painter().add(Shape::convex_polygon(vec![cx + Vec2::new(-3.0, 0.0), cx + Vec2::new(3.0, -6.0), cx + Vec2::new(3.0, 6.0)], rgb(0, 0, 128), Stroke::NONE));
            if r.clicked() {
                self.list.visible = false;
            }
        }
        self.pending.extend(out);
    }

    fn list_rows(&self) -> Vec<ListRow> {
        let mut rows = vec![];
        let (words, minus) = parse_filter(&self.list.filter);
        for p in &self.projects {
            let label = format!("{} ({})", p.map_title_name(), p.version_name());
            rows.push(ListRow::Project { pid: p.id, text: label });
            if p.versions.is_empty() {
                continue;
            }
            let hkey = format!("{}/Hexdump", p.id);
            rows.push(ListRow::Folder { pid: p.id, name: "Hexdump".into(), label: "Hexdump".into() });
            if !self.list.collapsed.contains(&hkey) {
                for ch in self.children.iter().filter(|c| c.project == p.id && c.map_uid.is_none()) {
                    rows.push(ListRow::Item { pid: p.id, uid: ch.id, addr: format!("{:05X}", ch.g.base), name: "Hexdump".into(), size: String::new(), kind: RowKind::Hexdump, flag: false, changed: false, hidden: false, open: true });
                }
            }
            let accept = |m: &MapDef| -> bool {
                let hay = format!("{} {} {} {:X} {}", m.name, m.id_name, m.folder, m.start, m.unit).to_lowercase();
                if !words.iter().all(|w| hay.contains(w)) || minus.iter().any(|w| hay.contains(w)) {
                    return false;
                }
                if self.list.bits != 0 && m.org.width() as u32 * 8 != self.list.bits {
                    return false;
                }
                true
            };
            // User folders, in order of first appearance
            let mut folders: Vec<String> = vec!["Maps".to_string()];
            for m in &p.maps {
                if !folders.contains(&m.folder) {
                    folders.push(m.folder.clone());
                }
            }
            let reference = p.ref_data();
            for fname in folders {
                let mut maps: Vec<&MapDef> = p.maps.iter().filter(|m| m.folder == fname && accept(m)).collect();
                if self.list.sort_by_name {
                    maps.sort_by(|a, b| a.name.cmp(&b.name));
                }
                let key = format!("{}/{fname}", p.id);
                rows.push(ListRow::Folder { pid: p.id, name: fname.clone(), label: fname.clone() });
                if self.list.collapsed.contains(&key) {
                    continue;
                }
                for m in maps {
                    let e = m.end().min(p.data().len());
                    let changed = m.start < e && e <= reference.len() && p.data()[m.start..e] != reference[m.start..e];
                    if m.hidden && !self.list.show_hidden {
                        continue;
                    }
                    if self.list.changed_only && !changed {
                        continue;
                    }
                    if self.list.flagged_only && !m.flag {
                        continue;
                    }
                    let open = self.children.iter().any(|c| c.project == p.id && c.map_uid == Some(m.uid));
                    rows.push(ListRow::Item {
                        pid: p.id,
                        uid: m.uid,
                        addr: format!("{:05X}", m.start),
                        name: m.name.clone(),
                        size: format!("{}x{}", m.cols, m.rows),
                        kind: RowKind::Map,
                        flag: m.flag,
                        changed,
                        hidden: m.hidden,
                        open,
                    });
                }
            }
            if !self.list.hide_potential && (p.search_done || !p.potential.is_empty()) {
                let pot: Vec<&MapDef> = p.potential.iter().filter(|m| accept(m)).collect();
                let key = format!("{}/Suggested maps", p.id);
                rows.push(ListRow::Folder { pid: p.id, name: "Suggested maps".into(), label: format!("Suggested maps ({})", pot.len()) });
                if !self.list.collapsed.contains(&key) {
                    for m in pot {
                        rows.push(ListRow::Item {
                            pid: p.id,
                            uid: m.uid,
                            addr: format!("{:05X}", m.start),
                            name: m.name.clone(),
                            size: format!("{}x{}", m.cols, m.rows),
                            kind: RowKind::Potential,
                            flag: false,
                            changed: false,
                            hidden: false,
                            open: false,
                        });
                    }
                }
            }
        }
        rows
    }

    // ---------- floating windows ----------
    pub fn floating(&mut self, ctx: &egui::Context) {
        if self.floats_hidden {
            return;
        }
        if self.show_overview {
            self.overview_window(ctx);
        }
        if self.show_preview {
            self.preview_window(ctx);
        }
        if self.show_diff {
            self.diff_window(ctx);
        }
        if self.find.open {
            self.find_window(ctx);
        }
        if self.connect_panel {
            self.connect_window(ctx);
        }
    }

    fn overview_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let mut jump = None;
        let mut rebuild = false;
        let Some(pid) = self.active_project else { return };
        egui::Window::new("File overview").order(egui::Order::Foreground).open(&mut open).default_size([290.0, 340.0]).default_pos(self.workspace.right_top() - Vec2::new(300.0, -8.0)).show(ctx, |ui| {
            let Some(p) = self.project(pid) else { return };
            let d = p.data();
            let r = p.ref_data();
            let avail = ui.available_size();
            ui.horizontal_top(|ui| {
                let h = avail.y.max(160.0);
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(110.0, h), Sense::click());
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 0.0, Color32::from_rgb(0, 0, 150));
                let strip = rect.shrink2(Vec2::new(10.0, 6.0));
                let size = d.len().max(1);
                let w = strip.width().max(1.0) as usize;
                let lines = strip.height().max(1.0) as usize;
                let per_line = (size as f32 / (w * lines) as f32).ceil().max(1.0) as usize;
                let per_px = per_line.max(1);
                let ov = p.overview.as_ref();
                let mut img = egui::ColorImage::filled([w, lines], Color32::from_rgb(0, 0, 150));
                for y in 0..lines {
                    for x in 0..w {
                        let a = (y * w + x) * per_px;
                        if a >= size {
                            break;
                        }
                        let e = (a + per_px).min(size);
                        let changed = e <= r.len() && d[a..e] != r[a..e];
                        let kind = ov.and_then(|o| o.kind_at(a)).unwrap_or(AreaKind::Unrecognized);
                        let rgb = kind.rgb(changed);
                        img.pixels[y * w + x] = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                    }
                }
                let tex = ui.ctx().load_texture("overview", img, egui::TextureOptions::NEAREST);
                painter.image(tex.id(), strip, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                if let Some(pos) = resp.interact_pointer_pos() {
                    if resp.clicked() {
                        let y = (pos.y - strip.top()).max(0.0) as usize;
                        let x = (pos.x - strip.left()).max(0.0) as usize;
                        jump = Some((y * w + x.min(w - 1)) * per_px);
                    }
                }
                if resp.double_clicked() {
                    rebuild = true;
                }
                ui.vertical(|ui| {
                    ui.label("Data:");
                    for k in AreaKind::ALL {
                        if k == AreaKind::Empty {
                            ui.add_space(6.0);
                            ui.label("More:");
                        }
                        ui.horizontal(|ui| {
                            let (sw, _) = ui.allocate_exact_size(Vec2::new(30.0, 16.0), Sense::hover());
                            let (dk, br) = (k.rgb(false), k.rgb(true));
                            ui.painter().rect_filled(Rect::from_min_size(sw.min, Vec2::new(15.0, 16.0)), 0.0, Color32::from_rgb(dk[0], dk[1], dk[2]));
                            ui.painter().rect_filled(Rect::from_min_size(sw.min + Vec2::new(15.0, 0.0), Vec2::new(15.0, 16.0)), 0.0, Color32::from_rgb(br[0], br[1], br[2]));
                            ui.label(k.label());
                        });
                    }
                    ui.add_space(8.0);
                    ui.label(format!("1 Pixel = {per_px} Byte(s)"));
                    ui.label(format!("Block size: {OVERVIEW_BLOCK} bytes"));
                });
            });
        });
        self.show_overview = open;
        if let Some(a) = jump {
            self.goto_addr(a);
        }
        if rebuild {
            if let Some(p) = self.project_mut(pid) {
                p.overview = Some(crate::analysis::build_overview(p.data(), &p.maps, &p.potential));
            }
        }
    }

    fn preview_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let source = self.preview_source();
        let theme = self.cfg.theme.clone();
        let cam = &mut self.preview_cam;
        egui::Window::new("3D preview").order(egui::Order::Foreground).open(&mut open).default_size([340.0, 240.0]).default_pos(self.workspace.right_bottom() - Vec2::new(360.0, 280.0)).show(ctx, |ui| {
            let size = ui.available_size().max(Vec2::new(160.0, 120.0));
            let (rect, resp) = ui.allocate_exact_size(size, Sense::drag());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, c(theme.bg));
            let d = resp.drag_delta();
            cam.0 += d.x * 0.01;
            cam.1 = (cam.1 + d.y * 0.005).clamp(0.0, 1.2);
            match &source {
                Some((vals, cols, rows)) => {
                    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
                    for v in vals {
                        lo = lo.min(*v);
                        hi = hi.max(*v);
                    }
                    let s = view::Surface { rot: cam.0, persp: cam.1, zscale: cam.2, zoffset: 0.0, fill: false, axis: false, line: c(theme.three_d), hi: c(theme.sel_text), floor: c(theme.three_d).gamma_multiply(0.7), height: self.cfg.height_colors };
                    let (cols, rows) = (*cols, *rows);
                    let z = move |i: usize, j: usize| ((vals.get(j * cols + i).copied().unwrap_or(lo) - lo) / (hi - lo).max(1e-9)) as f32;
                    view::surface(&painter, rect, cols, rows, &z, &|_, _| false, &s);
                }
                None => {
                    painter.text(rect.center(), Align2::CENTER_CENTER, "Select a rectangle or a map", FontId::proportional(12.0), c(theme.text));
                }
            }
        });
        self.show_preview = open;
    }

    /// Values for the preview: the selected rectangle, a map chosen in the list, or the open map.
    fn preview_source(&self) -> Option<(Vec<f64>, usize, usize)> {
        if let Some((pid, uid)) = self.list.selected {
            if let Some(p) = self.project(pid) {
                if let Some(m) = p.maps.iter().chain(p.potential.iter()).find(|m| m.uid == uid) {
                    let w = m.org.width();
                    let v: Vec<f64> = (0..m.cells()).map(|i| m.org.read(p.data(), m.start + i * w, m.signed).unwrap_or(0.0)).collect();
                    if m.rows > 1 || m.cols > 1 {
                        return Some((v, m.cols.min(512), m.rows.min(512)));
                    }
                }
            }
        }
        let c = self.active_child()?;
        let p = self.project(c.project)?;
        let g = &c.g;
        let (first, w, h) = g.sel_rect().or_else(|| c.map_uid.map(|_| (0, g.cols, g.rows_fixed.unwrap_or(1))))?;
        let mut v = Vec::with_capacity(w * h);
        for r in 0..h.min(512) {
            for k in 0..w.min(512) {
                v.push(g.raw(p, first + r * g.cols + k).unwrap_or(0.0));
            }
        }
        Some((v, w.min(512), h.min(512)))
    }

    fn diff_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let from = usize::from_str_radix(self.diff_from.trim(), 16).unwrap_or(0);
        let max = self.diff_max.trim().parse::<usize>().unwrap_or(2000).clamp(1, 2000);
        let rows = self.diff_list(from, max);
        let mut jump: Option<usize> = None;
        let mut act: Option<(f64, bool)> = None;
        let mut orig = false;
        let g = self.active_child().map(|c| c.g.clone());
        egui::Window::new("Differences").order(egui::Order::Foreground).open(&mut open).default_size([340.0, 300.0]).default_pos(self.workspace.left_top() + Vec2::new(40.0, 60.0)).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("From address:");
                ui.add(egui::TextEdit::singleline(&mut self.diff_from).desired_width(70.0));
                ui.label("Max. number:");
                ui.add(egui::TextEdit::singleline(&mut self.diff_max).desired_width(50.0));
            });
            ui.horizontal(|ui| {
                if ui.button("+ 100").clicked() {
                    act = Some((100.0, false));
                }
                if ui.button("- 100").clicked() {
                    act = Some((-100.0, false));
                }
                if ui.button("+ 1").clicked() {
                    act = Some((1.0, false));
                }
                if ui.button("- 1").clicked() {
                    act = Some((-1.0, false));
                }
                if ui.button("Original").clicked() {
                    orig = true;
                }
            });
            ui.separator();
            ui.label(format!("{} difference(s) shown", rows.len()));
            egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, 16.0, rows.len(), |ui, range| {
                egui::Grid::new("diffs").num_columns(4).striped(true).show(ui, |ui| {
                    for (a, v, o) in rows[range].iter() {
                        if ui.selectable_label(false, format!("{a:06X}")).clicked() {
                            jump = Some(*a);
                        }
                        if let Some(g) = &g {
                            ui.label(g.format(g.scaled(*o)));
                            ui.label(g.format(g.scaled(*v)));
                        }
                        ui.label(if *o != 0.0 { format!("{:+.1}%", (v - o) / o * 100.0) } else { "-".into() });
                        ui.end_row();
                    }
                });
            });
        });
        self.show_diff = open;
        if let Some(a) = jump {
            self.goto_addr(a);
        }
        if let Some((d, _)) = act {
            self.change_cells("Differences", None, &move |old, _, _, _| old + d);
        }
        if orig {
            self.restore_original();
        }
    }
}

fn parse_filter(s: &str) -> (Vec<String>, Vec<String>) {
    let mut plus = vec![];
    let mut minus = vec![];
    let mut cur = String::new();
    let mut q = false;
    let mut neg = false;
    let mut push = |cur: &mut String, neg: &mut bool| {
        if !cur.is_empty() {
            if *neg {
                minus.push(cur.to_lowercase());
            } else {
                plus.push(cur.to_lowercase());
            }
            cur.clear();
        }
        *neg = false;
    };
    for ch in s.chars() {
        match ch {
            '"' => {
                if q {
                    push(&mut cur, &mut neg);
                }
                q = !q;
            }
            '-' if !q && cur.is_empty() => neg = true,
            c if c.is_whitespace() && !q => push(&mut cur, &mut neg),
            c => cur.push(c),
        }
    }
    push(&mut cur, &mut neg);
    (plus, minus)
}

pub fn filter_matches(hay: &str, filter: &str) -> bool {
    let (p, m) = parse_filter(filter);
    let h = hay.to_lowercase();
    p.iter().all(|w| h.contains(w)) && !m.iter().any(|w| h.contains(w))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RowKind {
    Hexdump,
    Map,
    Potential,
}

enum ListRow {
    Project { pid: u64, text: String },
    Folder { pid: u64, name: String, label: String },
    Item { pid: u64, uid: u64, addr: String, name: String, size: String, kind: RowKind, flag: bool, changed: bool, hidden: bool, open: bool },
}

impl ListRow {
    fn key(&self) -> Option<(u64, u64)> {
        match self {
            ListRow::Item { pid, uid, .. } => Some((*pid, *uid)),
            _ => None,
        }
    }
}

impl Project {
    pub fn map_title_name(&self) -> String {
        match &self.path {
            Some(p) => p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            None => {
                if self.props.file_name.is_empty() {
                    format!("Project {}", self.id)
                } else {
                    self.props.file_name.clone()
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_words_quotes_and_minus() {
        assert!(filter_matches("BMW 530 Diesel", "bmw diesel"));
        assert!(!filter_matches("BMW 530 Siemens", "BMW -Siemens"));
        assert!(filter_matches("Audi A6 2.7 TDI", "\"A6 2.7\""));
        assert!(!filter_matches("Audi A6 3.0 2.7", "\"A6 2.7\""));
        assert!(filter_matches("anything", ""));
    }
}

#[cfg(test)]
mod colour_tests {
    use super::*;

    #[test]
    fn the_dark_frame_trades_greys_and_keeps_strong_colours() {
        assert_eq!(tone_dark(Color32::WHITE), flat(0x33), "fields");
        assert_eq!(tone_dark(flat(0xF4)), flat(0x3C), "button faces");
        assert_eq!(tone_dark(flat(0xD4)), flat(0x2B), "panels");
        assert_eq!(tone_dark(Color32::BLACK), flat(0xDC), "lines and text");
        assert_eq!(tone_dark(flat(165)), flat(0x5C), "what can't be used stays dim");
        let close = Color32::from_rgb(232, 17, 35);
        assert_eq!(tone_dark(close), close, "a strong red stays red");
        let navy = tone_dark(Color32::from_rgb(0, 0, 128));
        assert!(navy.b() > 200 && navy.r() > 90, "a deep blue becomes a light one: {navy:?}");
        let hover = tone_dark(Color32::from_rgb(0xE5, 0xF1, 0xFB));
        assert!(hover.b() > hover.r() && hover.b() < 130, "a pale tint becomes a deep one: {hover:?}");
    }
}
