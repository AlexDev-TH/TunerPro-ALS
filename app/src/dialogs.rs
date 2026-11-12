//! Every dialog of the program, in the order of the menus.
use crate::a2l;
use crate::app::{edit_weights, parse_goto, relative, App, DiffMove, LastChange};
use crate::checksum;
use crate::cmd::Cmd;
use crate::config::{Client, Config};
use crate::formats::{self, Block, LineSwap};
use crate::model::*;
use crate::search;
use crate::shell::filter_matches;
use crate::theme::Theme;
use crate::view::{Grid, Sel, SelMode};
use egui::{ComboBox, RichText, TextEdit, Ui, Vec2};
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileFmt {
    Auto,
    Binary,
    TwoFiles,
    Intel,
    Motorola,
}

pub struct OpenEntry {
    pub path: PathBuf,
    pub props: Props,
    pub versions: usize,
    pub maps: usize,
}

pub struct ChangeItem {
    pub name: String,
    pub map: Option<MapDef>,
    pub src: usize,
    pub len: usize,
    pub org: DataOrg,
    pub target: Option<usize>,
    pub take_map: bool,
    pub take_data: bool,
}

pub enum Modal {
    NewProject,
    ImportOpts { fmt: FileFmt, swap: bool, overwrite: bool },
    ExportOpts { fmt: FileFmt, swap: bool, from: String, to: String, per_line: String, zip: bool, base: String },
    HexImport { blocks: Vec<Block>, size: String, offset: String, target: Option<u64>, more: bool },
    PackImport { maps: Vec<MapDef>, offset: String },
    A2l { text: String, offset: String, maps: Vec<(a2l::A2lMap, bool)>, warnings: Vec<String>, filter: String, error: String },
    ProjectProps(Box<Props>),
    VersionProps { idx: usize, name: String, comment: String, folder: String },
    Versions { sel: usize, rename: String },
    Elements { list: Vec<Element> },
    Resize { size: String, at_start: bool, fill: String },
    ProjectComment { text: String },
    Open { entries: Vec<OpenEntry>, filter: String, letter: Option<char>, sel: Option<usize>, as_filter: bool },
    ConfirmDelete(u64),
    SaveChanges { pid: u64, exit: bool },
    UndoMore,
    GoTo { text: String, binary_and: bool, error: String },
    ChangeAbs { shown: String, hex: String },
    ChangeRel { from_original: bool, percent: bool, w: String },
    ChangeEdit { from_original: bool, percent: bool, p: [f64; 5], scale: f64, realtime: bool, base: Vec<(usize, f64)>, dirty: bool },
    CreateSel { from: String, to: String, mode: SelMode },
    MapProps { map: Box<MapDef>, tab: usize, x: String, y: String },
    HexProps { cols: String, org: DataOrg, radix: u32, no_factor: bool, delta: bool, percent: bool, show_orig: bool, signed: bool, right: RightSide, lo: String, hi: String, dynamic: bool, factor: String, offset: String },
    Protect { run: Box<checksum::ProtectRun>, addr: usize },
    Checksums { sel: Option<usize> },
    ChecksumEdit { cs: Box<Checksum>, idx: Option<usize>, from: String, to: String, target: String, target_end: String, error: String },
    SyncEdit { a: String, b: String, len: String },
    AsText { addr: usize, text: String, list: Vec<(usize, String)>, filter: String },
    Comment { addr: usize, end: usize, text: String, color: [u8; 3], existing: bool },
    ChooseRef { version: usize, source: Option<u64>, min: String, offset: String },
    Connect { target: Option<u64>, offset: String, reference: bool, arrange: bool, status: String },
    ImportChanges { step: usize, source: Option<u64>, tolerance: f64, changed_only: bool, absolute: bool, items: Vec<ChangeItem> },
    ImportMaps { source: Option<u64> },
    Parallel { tolerance: f64, found: Vec<(usize, bool)>, names: bool, axes: bool, sync: bool, searched: bool },
    Replace { radix: search::Radix, find: String, with: String, in_original: bool, status: String },
    Config { tab: usize },
    SwapLines { addr: Vec<u8>, data: Vec<u8>, on: bool, error: String },
    AxisProfiles { name: String },
    Help(&'static str),
    Tip,
    About,
}

impl Modal {
    pub fn hex_import(blocks: Vec<Block>, size: usize, offset: u64, target: Option<u64>) -> Modal {
        Modal::HexImport { blocks, size: format!("{size:X}"), offset: format!("{offset:X}"), target, more: false }
    }
    pub fn pack_import(maps: Vec<MapDef>) -> Modal {
        Modal::PackImport { maps, offset: "0".into() }
    }
    pub fn a2l(text: String) -> Modal {
        Modal::A2l { text, offset: "0".into(), maps: vec![], warnings: vec![], filter: String::new(), error: String::new() }
    }
    pub fn import_opts() -> Modal {
        Modal::ImportOpts { fmt: FileFmt::Auto, swap: false, overwrite: false }
    }
    pub fn export_opts(p: Option<&Project>) -> Modal {
        let (from, to) = p.map(|p| p.element_range()).unwrap_or((0, 0));
        Modal::ExportOpts { fmt: FileFmt::Binary, swap: false, from: format!("{from:X}"), to: format!("{:X}", to.saturating_sub(1)), per_line: "32".into(), zip: false, base: "0".into() }
    }
    pub fn open_project(cfg: &Config) -> Modal {
        let mut entries = vec![];
        if let Ok(rd) = std::fs::read_dir(cfg.project_dir()) {
            for e in rd.flatten() {
                let path = e.path();
                if path.extension().is_some_and(|x| x == "alsproj") {
                    if let Ok((props, versions, maps)) = Project::peek(&path) {
                        entries.push(OpenEntry { path, props, versions, maps });
                    }
                }
            }
        }
        Modal::Open { entries, filter: String::new(), letter: None, sel: None, as_filter: true }
    }
    pub fn import_changes() -> Modal {
        Modal::ImportChanges { step: 0, source: None, tolerance: 0.0, changed_only: true, absolute: false, items: vec![] }
    }
    pub fn change_edit() -> Modal {
        Modal::ChangeEdit { from_original: false, percent: true, p: [0.0; 5], scale: 10.0, realtime: false, base: vec![], dirty: false }
    }
    pub fn swap_lines(s: Option<LineSwap>, size: usize) -> Modal {
        let lines = (usize::BITS - size.max(2).saturating_sub(1).leading_zeros()) as usize;
        let on = s.is_some();
        let s = s.unwrap_or_else(|| LineSwap::identity(lines));
        Modal::SwapLines { addr: s.addr, data: s.data, on, error: String::new() }
    }
    pub fn choose_ref(p: Option<&Project>) -> Modal {
        let (version, min, offset) = p.map(|p| (p.reference.version, p.reference.min_diff, p.reference.offset)).unwrap_or((0, 0.0, 0));
        Modal::ChooseRef { version, source: None, min: format!("{min}"), offset: format!("{offset}") }
    }
    pub fn connect() -> Modal {
        Modal::Connect { target: None, offset: "0".into(), reference: false, arrange: true, status: String::new() }
    }
    pub fn replace() -> Modal {
        Modal::Replace { radix: search::Radix::Hex, find: String::new(), with: String::new(), in_original: false, status: String::new() }
    }
    pub fn parallel() -> Modal {
        Modal::Parallel { tolerance: 10.0, found: vec![], names: true, axes: true, sync: false, searched: false }
    }
    pub fn map_props(m: MapDef) -> Modal {
        let (x, y) = (user_text(&m.x), user_text(&m.y));
        Modal::MapProps { map: Box::new(m), tab: 0, x, y }
    }
    pub fn hex_props(g: &Grid) -> Modal {
        Modal::HexProps {
            cols: g.cols.to_string(),
            org: g.org,
            radix: g.radix,
            no_factor: g.no_factor,
            delta: g.delta,
            percent: g.percent,
            show_orig: g.show_orig,
            signed: g.signed,
            right: g.right,
            lo: format!("{}", g.range_lo),
            hi: format!("{}", g.range_hi),
            dynamic: g.dynamic,
            factor: format!("{}", g.factor),
            offset: format!("{}", g.offset),
        }
    }
}

fn user_text(a: &Axis) -> String {
    a.user.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ")
}

fn hex(s: &str) -> Option<usize> {
    usize::from_str_radix(s.trim().trim_start_matches("0x").trim_start_matches("0X"), 16).ok()
}

fn num(s: &str) -> Option<f64> {
    s.trim().replace(',', ".").parse().ok()
}

fn field(ui: &mut Ui, label: &str, v: &mut String) {
    ui.label(label);
    ui.add(TextEdit::singleline(v).desired_width(150.0));
    ui.end_row();
}

fn org_combo(ui: &mut Ui, id: &str, org: &mut DataOrg) {
    ComboBox::from_id_salt(id).selected_text(org.label()).show_ui(ui, |ui| {
        for o in DataOrg::ALL {
            ui.selectable_value(org, o, o.label());
        }
    });
}

fn radix_combo(ui: &mut Ui, id: &str, r: &mut u32) {
    ComboBox::from_id_salt(id).selected_text(if *r == 16 { "Hexadecimal (Base 16)" } else { "Decimal (Base 10)" }).show_ui(ui, |ui| {
        ui.selectable_value(r, 10, "Decimal (Base 10)");
        ui.selectable_value(r, 16, "Hexadecimal (Base 16)");
    });
}

fn right_combo(ui: &mut Ui, id: &str, r: &mut RightSide) {
    let label = |x: RightSide| match x {
        RightSide::Nothing => "Nothing",
        RightSide::Bars => "Bar display",
        RightSide::Ascii => "ASCII",
        RightSide::Both => "Both",
    };
    ComboBox::from_id_salt(id).selected_text(label(*r)).show_ui(ui, |ui| {
        for x in [RightSide::Nothing, RightSide::Bars, RightSide::Ascii, RightSide::Both] {
            ui.selectable_value(r, x, label(x));
        }
    });
}

/// Which file an export writes, with the swap and range applied.
pub fn export_bytes(d: &[u8], from: usize, to: usize, swap: Option<&LineSwap>) -> Vec<u8> {
    let full;
    let src: &[u8] = match swap {
        Some(s) if !s.is_identity() => {
            full = s.unapply(d);
            &full
        }
        _ => d,
    };
    let end = (to + 1).min(src.len());
    if from < end {
        src[from..end].to_vec()
    } else {
        vec![]
    }
}

impl App {
    pub fn show_modal(&mut self, ctx: &egui::Context) {
        if let Some((title, text)) = self.message.clone() {
            let mut close = false;
            egui::Window::new(title).id(egui::Id::unique("message")).order(egui::Order::Foreground).collapsible(false).resizable(false).pivot(egui::Align2::CENTER_CENTER).default_pos(self.workspace.center()).show(ctx, |ui| {
                ui.set_max_width(420.0);
                ui.label(text);
                ui.add_space(6.0);
                if ui.button("   OK   ").clicked() {
                    close = true;
                }
            });
            if close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.message = None;
            }
        }
        let Some(mut m) = self.modal.take() else { return };
        let keep = self.modal_ui(ctx, &mut m);
        if keep && self.modal.is_none() {
            self.modal = Some(m);
        }
    }

    fn win(&self, title: &str, width: f32) -> egui::Window<'static> {
        egui::Window::new(title.to_string())
            .id(egui::Id::unique(("dlg", title.to_string())))
            .order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .default_width(width)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(self.workspace.center())
    }

    fn modal_ui(&mut self, ctx: &egui::Context, m: &mut Modal) -> bool {
        let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape)) && self.message.is_none();
        let mut keep = !esc;
        match m {
            Modal::NewProject => {
                self.win("New project", 340.0).show(ctx, |ui| {
                    ui.label("Where does the data of the new project come from?");
                    ui.add_space(6.0);
                    if ui.button("From a file...").clicked() {
                        self.pending.push(Cmd::ImportFile);
                        keep = false;
                    }
                    if ui.button("Try it with made-up sample data").clicked() {
                        self.pending.push(Cmd::SampleProject);
                        keep = false;
                    }
                    ui.add_space(6.0);
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
            }
            Modal::ImportOpts { fmt, swap, overwrite } => {
                let has = self.current_project().is_some_and(|p| !p.versions.is_empty());
                self.win("How the file is read", 340.0).show(ctx, |ui| {
                    ui.label("File format:");
                    for (f, l) in [(FileFmt::Auto, "Recognise automatically"), (FileFmt::Binary, "Binary"), (FileFmt::TwoFiles, "Binary, 2 files (16 bit)"), (FileFmt::Intel, "Intel Hex"), (FileFmt::Motorola, "Motorola Hex")] {
                        ui.radio_value(fmt, f, l);
                    }
                    ui.add_enabled(self.swap.is_some(), egui::Checkbox::new(swap, "Swap lines as set under Tools"));
                    ui.add_enabled(has, egui::Checkbox::new(overwrite, "Overwrite the current version"));
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            keep = false;
                            self.run_import(ctx, *fmt, *swap, *overwrite);
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            }
            Modal::ExportOpts { fmt, swap, from, to, per_line, zip, base } => {
                self.win("How the file is written", 360.0).show(ctx, |ui| {
                    ui.label("File format:");
                    for (f, l) in [(FileFmt::Binary, "Binary"), (FileFmt::TwoFiles, "Binary, 2 files (16 bit)"), (FileFmt::Intel, "Intel Hex"), (FileFmt::Motorola, "Motorola Hex")] {
                        ui.radio_value(fmt, f, l);
                    }
                    ui.add_enabled(self.swap.is_some(), egui::Checkbox::new(swap, "Swap lines as set under Tools"));
                    egui::Grid::new("exp").show(ui, |ui| {
                        field(ui, "From (hex):", from);
                        field(ui, "To (hex):", to);
                        if matches!(*fmt, FileFmt::Intel | FileFmt::Motorola) {
                            field(ui, "Bytes per line:", per_line);
                            field(ui, "Address in the file (hex):", base);
                        }
                    });
                    ui.add_enabled(*fmt != FileFmt::TwoFiles, egui::Checkbox::new(zip, "Pack as zip"));
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            if let Err(e) = self.run_export(*fmt, *swap, from, to, per_line, *zip, base) {
                                self.tell("Export", e);
                            } else {
                                keep = false;
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            }
            Modal::HexImport { blocks, size, offset, target, more } => {
                self.win("Hex file import", 400.0).show(ctx, |ui| {
                    egui::Grid::new("hexi").show(ui, |ui| {
                        field(ui, "Project size (hex):", size);
                        field(ui, "Offset (hex):", offset);
                    });
                    let (sz, off) = (hex(size).unwrap_or(0), hex(offset).unwrap_or(0));
                    let valid = sz > 0 && sz <= 0x4000_0000;
                    if valid {
                        let (_, over) = formats::map_blocks(blocks, sz, off as u64);
                        if over {
                            ui.colored_label(egui::Color32::RED, "Overlapping areas found! The project size may be too small.");
                        }
                    } else {
                        ui.colored_label(egui::Color32::RED, "Please enter a valid size.");
                    }
                    if ui.button("More information").clicked() {
                        *more = !*more;
                    }
                    if *more {
                        egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                            for b in blocks.iter() {
                                ui.monospace(format!("{:08X} - {:08X}  ({} bytes)", b.addr, b.addr + b.data.len() as u64 - 1, b.data.len()));
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        if ui.add_enabled(valid, egui::Button::new("OK")).clicked() {
                            let (d, _) = formats::map_blocks(blocks, sz, off as u64);
                            let t = *target;
                            self.take_data(d, "", t, false, ctx);
                            keep = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            }
            Modal::PackImport { maps, offset } => {
                self.win("Maps from a file", 380.0).show(ctx, |ui| {
                    ui.label(format!("{} map(s) in this pack.", maps.len()));
                    ui.horizontal(|ui| {
                        ui.label("Offset (hex, may be negative):");
                        ui.add(TextEdit::singleline(offset).desired_width(90.0));
                    });
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            let t = offset.trim();
                            let (neg, t) = t.strip_prefix('-').map(|r| (true, r)).unwrap_or((false, t));
                            let off = hex(t).unwrap_or(0) as i64 * if neg { -1 } else { 1 };
                            let mut added = 0;
                            let mut skipped = 0;
                            if let Some(p) = self.current_project_mut() {
                                let size = p.size();
                                let d = p.data().to_vec();
                                for src in maps.iter() {
                                    let mut mm = src.clone();
                                    let s = mm.start as i64 + off;
                                    if s < 0 || (s as usize) + mm.byte_len() > size {
                                        skipped += 1;
                                        continue;
                                    }
                                    mm.start = s as usize;
                                    if mm.x.src != AxisSrc::Enumerate {
                                        mm.x.addr = (mm.x.addr as i64 + off).max(0) as usize;
                                    }
                                    if mm.y.src != AxisSrc::Enumerate {
                                        mm.y.addr = (mm.y.addr as i64 + off).max(0) as usize;
                                    }
                                    mm.uid = p.uid();
                                    mm.auto_range(&d);
                                    p.maps.push(mm);
                                    added += 1;
                                }
                                p.dirty_struct = true;
                            }
                            self.tell("Import maps", format!("{added} map(s) imported, {skipped} outside this project."));
                            keep = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            }
            Modal::A2l { text, offset, maps, warnings, filter, error } => {
                self.win("Import description file (A2L)", 560.0).show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Address offset (hex):");
                        ui.add(TextEdit::singleline(offset).desired_width(90.0));
                        if ui.button("Analyse the file").clicked() {
                            let size = self.current_project().map(|p| p.size()).unwrap_or(0);
                            match a2l::parse(text, hex(offset).unwrap_or(0) as i64, size) {
                                Ok((v, w)) => {
                                    *maps = v.into_iter().map(|x| (x, true)).collect();
                                    *warnings = w;
                                    error.clear();
                                }
                                Err(e) => *error = e,
                            }
                        }
                    });
                    if !error.is_empty() {
                        ui.colored_label(egui::Color32::RED, error.as_str());
                    }
                    if !maps.is_empty() {
                        ui.horizontal(|ui| {
                            ui.label("Search:");
                            ui.text_edit_singleline(filter);
                            if ui.button("All").clicked() {
                                maps.iter_mut().for_each(|x| x.1 = true);
                            }
                            if ui.button("None").clicked() {
                                maps.iter_mut().for_each(|x| x.1 = false);
                            }
                        });
                        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                            for (x, on) in maps.iter_mut() {
                                if filter_matches(&format!("{} {}", x.map.name, x.map.desc), filter) {
                                    ui.checkbox(on, format!("{:06X}  {}  {}x{}  {}", x.map.start, x.map.name, x.map.cols, x.map.rows, x.map.desc));
                                }
                            }
                        });
                    }
                    if !warnings.is_empty() {
                        ui.collapsing(format!("{} warning(s)", warnings.len()), |ui| {
                            egui::ScrollArea::vertical().max_height(100.0).show(ui, |ui| {
                                for w in warnings.iter() {
                                    ui.label(w);
                                }
                            });
                        });
                    }
                    ui.horizontal(|ui| {
                        if ui.add_enabled(!maps.is_empty(), egui::Button::new("Import")).clicked() {
                            let mut n = 0;
                            if let Some(p) = self.current_project_mut() {
                                let d = p.data().to_vec();
                                for (x, on) in maps.iter() {
                                    if *on {
                                        let mut mm = x.map.clone();
                                        mm.uid = p.uid();
                                        mm.auto_range(&d);
                                        p.maps.push(mm);
                                        n += 1;
                                    }
                                }
                                p.dirty_struct = true;
                            }
                            self.tell("Import", format!("{n} map(s) imported."));
                            keep = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            }
            Modal::ProjectProps(pr) => self.project_props(ctx, pr, &mut keep),
            Modal::VersionProps { idx, name, comment, folder } => {
                let sums = self.current_project().map(|p| (p.versions.first().map(|v| sum8(&v.data)).unwrap_or(0), p.versions.get(*idx).map(|v| sum8(&v.data)).unwrap_or(0)));
                self.win("Version details", 380.0).show(ctx, |ui| {
                    egui::Grid::new("verp").show(ui, |ui| {
                        field(ui, "Name:", name);
                        field(ui, "Folder:", folder);
                    });
                    ui.label("Comment:");
                    ui.add(TextEdit::multiline(comment).desired_rows(4).desired_width(340.0));
                    if let Some((a, b)) = sums {
                        ui.label(format!("8 bit sum: original {a:04X}, this version {b:04X}"));
                    }
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            let i = *idx;
                            let (n, c, f) = (name.clone(), comment.clone(), folder.clone());
                            if let Some(p) = self.current_project_mut() {
                                if let Some(v) = p.versions.get_mut(i) {
                                    v.name = n;
                                    v.comment = c;
                                    v.folder = f;
                                }
                                p.dirty_struct = true;
                            }
                            keep = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            }
            Modal::Versions { sel, rename } => self.versions_dialog(ctx, sel, rename, &mut keep),
            Modal::Elements { list } => {
                self.win("Parts of the data", 520.0).show(ctx, |ui| {
                    let mut del = None;
                    egui::Grid::new("elems").show(ui, |ui| {
                        ui.label("Use");
                        ui.label("Start (hex)");
                        ui.label("End (hex)");
                        ui.label("Display offset (hex)");
                        ui.end_row();
                        for (i, e) in list.iter_mut().enumerate() {
                            ui.add(TextEdit::singleline(&mut e.usage).desired_width(100.0));
                            hex_edit(ui, &mut e.start);
                            hex_edit(ui, &mut e.end);
                            hex_edit(ui, &mut e.display_offset);
                            if ui.button("Delete").clicked() {
                                del = Some(i);
                            }
                            ui.end_row();
                        }
                    });
                    if let Some(i) = del {
                        if list.len() > 1 {
                            list.remove(i);
                        }
                    }
                    ui.horizontal(|ui| {
                        if ui.add_enabled(list.len() < MAX_ELEMENTS, egui::Button::new("Add")).clicked() {
                            let e = list.last().map(|l| l.end + 1).unwrap_or(0);
                            list.push(Element { usage: "Extra data".into(), start: e, end: e, display_offset: 0 });
                        }
                        if ui.button("OK").clicked() {
                            let l = list.clone();
                            if let Some(p) = self.current_project_mut() {
                                p.elements = l;
                                p.active_element = Some(0);
                                p.dirty_struct = true;
                            }
                            keep = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            }
            Modal::Resize { size, at_start, fill } => {
                self.win("Resize the data", 380.0).show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("New size (hex):");
                        ui.add(TextEdit::singleline(size).desired_width(100.0));
                    });
                    ui.radio_value(at_start, false, "Add / remove bytes at the end");
                    ui.radio_value(at_start, true, "Add / remove bytes at the beginning");
                    ui.horizontal(|ui| {
                        ui.label("Fill new bytes with (hex):");
                        ui.add(TextEdit::singleline(fill).desired_width(40.0));
                    });
                    ui.colored_label(crate::shell::rgb(170, 0, 0), "This applies to all versions and cannot be undone. Bytes cut off are gone, and checksums and sync blocks are removed.");
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            if let Some(n) = hex(size).filter(|&n| n > 0) {
                                let f = hex(fill).unwrap_or(0xFF) as u8;
                                let s = *at_start;
                                if let Some(p) = self.current_project_mut() {
                                    p.resize(n, s, f);
                                }
                                keep = false;
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            }
            Modal::ProjectComment { text } => {
                self.win("Project notes", 420.0).show(ctx, |ui| {
                    ui.add(TextEdit::multiline(text).desired_rows(8).desired_width(400.0));
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            let t = text.clone();
                            if let Some(p) = self.current_project_mut() {
                                p.props.comment = t;
                                p.dirty_struct = true;
                            }
                            keep = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            }
            Modal::Open { entries, filter, letter, sel, as_filter } => self.open_dialog(ctx, entries, filter, letter, sel, as_filter, &mut keep),
            Modal::ConfirmDelete(id) => {
                let id = *id;
                let name = self.project(id).map(|p| p.map_title_name()).unwrap_or_default();
                self.win("Close and delete project", 360.0).show(ctx, |ui| {
                    ui.label(format!("Really delete \"{name}\" from the disk? This cannot be undone."));
                    ui.horizontal(|ui| {
                        if ui.button("Delete").clicked() {
                            let path = self.project(id).and_then(|p| p.path.clone());
                            self.close_project_now(id);
                            if let Some(path) = path {
                                if let Err(e) = std::fs::remove_file(&path) {
                                    self.tell("Delete project", e.to_string());
                                }
                            }
                            keep = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            }
            Modal::SaveChanges { pid, exit } => {
                let (id, ex) = (*pid, *exit);
                let name = self.project(id).map(|p| p.map_title_name()).unwrap_or_default();
                self.win("Save changes", 360.0).show(ctx, |ui| {
                    ui.label(format!("\"{name}\" has unsaved changes."));
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            if self.save_project(id) {
                                self.finish_close(id, ex);
                                keep = false;
                            }
                        }
                        if ui.button("Close WITHOUT saving").clicked() {
                            self.finish_close(id, ex);
                            keep = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            }
            Modal::UndoMore => {
                let labels: Vec<String> = self.current_project().map(|p| p.undo.iter().rev().map(|g| format!("{} ({} change(s))", g.label, g.edits.len())).collect()).unwrap_or_default();
                self.win("Undo several steps", 340.0).show(ctx, |ui| {
                    let mut pick = None;
                    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        for (i, l) in labels.iter().enumerate() {
                            if ui.selectable_label(false, l).clicked() {
                                pick = Some(i);
                            }
                        }
                        if labels.is_empty() {
                            ui.label("Nothing to undo.");
                        }
                    });
                    if let Some(i) = pick {
                        if let Some(p) = self.current_project_mut() {
                            for _ in 0..=i {
                                p.undo_one();
                            }
                        }
                        keep = false;
                    }
                    if ui.button("Close").clicked() {
                        keep = false;
                    }
                });
            }
            Modal::GoTo { text, binary_and, error } => {
                let (comments, maps): (Vec<(usize, String)>, Vec<(usize, String)>) = self
                    .current_project()
                    .map(|p| (p.comments.iter().map(|c| (c.addr, c.text.clone())).collect(), p.maps.iter().map(|m| (m.start, m.name.clone())).collect()))
                    .unwrap_or_default();
                self.win("Go to address", 380.0).show(ctx, |ui| {
                    let r = ui.add(TextEdit::singleline(text).desired_width(340.0).hint_text("Address in hex, 0d100, or a map or comment name"));
                    r.request_focus();
                    ui.checkbox(binary_and, "Address binary-and file size - 1");
                    if !error.is_empty() {
                        ui.colored_label(egui::Color32::RED, error.as_str());
                    }
                    if !comments.is_empty() || !maps.is_empty() || !self.list.history.is_empty() {
                        egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                            for h in self.list.history.iter() {
                                if ui.selectable_label(false, format!("{h:>6}  (last position)")).clicked() {
                                    *text = h.clone();
                                }
                            }
                            for (a, t) in comments.iter().chain(maps.iter()) {
                                if ui.selectable_label(false, format!("{a:06X}  {t}")).clicked() {
                                    *text = format!("{a:X}");
                                }
                            }
                        });
                    }
                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() || enter {
                            let r = self.current_project().map(|p| parse_goto(text, p, *binary_and)).unwrap_or(Err("No project".into()));
                            match r {
                                Ok(a) => {
                                    if let Some(c) = self.active_child() {
                                        let here = c.g.addr(c.g.cursor);
                                        self.remember_position(here);
                                    }
                                    self.goto_addr(a);
                                    keep = false;
                                }
                                Err(e) => *error = e,
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            }
            Modal::ChangeAbs { shown, hex: hx } => {
                let g = self.active_child().map(|c| c.g.clone());
                self.win("Set to a value", 260.0).show(ctx, |ui| {
                    let Some(g) = g else { return };
                    ui.label("Value as shown:");
                    if ui.add(TextEdit::singleline(shown).desired_width(200.0)).changed() {
                        if let Some(v) = num(shown) {
                            *hx = format!("{:X}", g.unscaled(v).round() as i64);
                        }
                    }
                    ui.label("Stored value (hex):");
                    if ui.add(TextEdit::singleline(hx).desired_width(200.0)).changed() {
                        if let Some(v) = hex(hx) {
                            *shown = format!("{}", g.scaled(v as f64));
                        }
                    }
                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() || enter {
                            if let Some(v) = num(shown) {
                                self.apply_change(&LastChange::Absolute(v));
                                keep = false;
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            }
            Modal::ChangeRel { from_original, percent, w } => {
                let preview = self.active_child().and_then(|c| {
                    let p = self.project(c.project)?;
                    let v = c.g.raw(p, c.g.cursor)?;
                    let o = p.ref_value(c.g.org, c.g.addr(c.g.cursor), c.g.signed).unwrap_or(v);
                    let x = if *from_original { o } else { v };
                    let wv = num(w)?;
                    let n = relative(x, wv, *percent, self.cfg.edit_shown_values, &c.g);
                    Some(format!("Cursor cell: {} → {}", c.g.format(c.g.scaled(v)), c.g.format(c.g.scaled(n))))
                });
                self.win("Add or scale", 320.0).show(ctx, |ui| {
                    ui.group(|ui| {
                        ui.label("Data source (x):");
                        ui.radio_value(from_original, false, "Current data");
                        ui.radio_value(from_original, true, "The original data (earlier changes are overwritten)");
                    });
                    ui.group(|ui| {
                        ui.label("What to do:");
                        ui.radio_value(percent, false, "Add the amount to each value");
                        ui.radio_value(percent, true, "Add the amount as a percentage of each value");
                    });
                    ui.horizontal(|ui| {
                        ui.label("Change all values by (w):");
                        ui.add(TextEdit::singleline(w).desired_width(90.0));
                        ui.label(if *percent { "%" } else { "" });
                    });
                    if let Some(t) = preview {
                        ui.label(t);
                    }
                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() || enter {
                            if let Some(v) = num(w) {
                                self.apply_change(&LastChange::Relative { from_original: *from_original, percent: *percent, w: v });
                                keep = false;
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            }
            Modal::ChangeEdit { from_original, percent, p: params, scale, realtime, base, dirty } => {
                if base.is_empty() {
                    if let Some(c) = self.active_child() {
                        if let Some(pj) = self.project(c.project) {
                            let total = c.g.total(self.end_of(c));
                            *base = c.g.selected_cells(total).into_iter().filter_map(|i| c.g.org.read(pj.data(), c.g.addr(i), c.g.signed).map(|v| (i, v))).collect();
                        }
                    }
                }
                let mut changed = false;
                let mut ok = false;
                let mut cancel = false;
                self.win("Blend across the selection", 420.0).show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.radio_value(from_original, false, "Current data");
                        ui.radio_value(from_original, true, "Original");
                        ui.separator();
                        ui.radio_value(percent, false, "Addition");
                        ui.radio_value(percent, true, "Percentage");
                    });
                    ui.add(egui::Slider::new(scale, 1.0..=1000.0).logarithmic(true).text("Scale"));
                    ui.add_space(4.0);
                    let s = *scale;
                    let mut slide = |ui: &mut Ui, i: usize, label: &str| {
                        ui.vertical(|ui| {
                            ui.label(label);
                            if ui.add(egui::Slider::new(&mut params[i], -s..=s)).changed() {
                                changed = true;
                            }
                        });
                    };
                    ui.horizontal(|ui| {
                        slide(ui, 0, "Upper left");
                        slide(ui, 1, "Upper right");
                    });
                    slide(ui, 4, "All cells");
                    ui.horizontal(|ui| {
                        slide(ui, 2, "Lower left");
                        slide(ui, 3, "Lower right");
                    });
                    ui.checkbox(realtime, "Changes in real-time (use with care)");
                    ui.horizontal(|ui| {
                        ok = ui.button("OK").clicked();
                        cancel = ui.button("Cancel").clicked();
                    });
                });
                if (changed && *realtime) || ok {
                    self.apply_edit(base, *from_original, *percent, *params);
                    *dirty = true;
                }
                if ok {
                    keep = false;
                }
                if cancel || esc {
                    if *dirty {
                        let b = base.clone();
                        let map: std::collections::HashMap<usize, f64> = b.iter().copied().collect();
                        let cells: Vec<usize> = map.keys().copied().collect();
                        self.change_cells("Change by edit undone", Some(cells), &move |old, i, _, _| map.get(&i).copied().unwrap_or(old));
                    }
                    keep = false;
                }
            }
            Modal::CreateSel { from, to, mode } => {
                self.win("Select by address", 300.0).show(ctx, |ui| {
                    egui::Grid::new("csel").show(ui, |ui| {
                        field(ui, "Start address (hex):", from);
                        field(ui, "End address (hex):", to);
                    });
                    for (l, v) in [("Consecutive", SelMode::Consecutive), ("Rectangle", SelMode::Rectangle), ("Columns", SelMode::Columns), ("Rows", SelMode::Rows)] {
                        ui.radio_value(mode, v, l);
                    }
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            if let (Some(a), Some(b)) = (hex(from), hex(to)) {
                                let md = *mode;
                                self.with_grid(|g| {
                                    let (x, y) = (g.cell_of(a), g.cell_of(b));
                                    g.sel_mode = md;
                                    g.sel = Some(Sel::new(x, y));
                                    g.cursor = x;
                                    g.follow = true;
                                });
                                keep = false;
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            }
            Modal::MapProps { map, tab, x, y } => self.map_props_dialog(ctx, map, tab, x, y, &mut keep),
            Modal::HexProps { cols, org, radix, no_factor, delta, percent, show_orig, signed, right, lo, hi, dynamic, factor, offset } => {
                let cursor_range = self.active_child().and_then(|c| self.project(c.project).map(|p| (c.g.clone(), self.end_of(c), p.data().len())));
                self.win("Window settings", 360.0).show(ctx, |ui| {
                    egui::Grid::new("hexp").num_columns(2).show(ui, |ui| {
                        field(ui, "Columns:", cols);
                        ui.label("Values:");
                        org_combo(ui, "hexorg", org);
                        ui.end_row();
                        ui.label("Number format:");
                        radix_combo(ui, "hexrad", radix);
                        ui.end_row();
                        ui.label("Right side:");
                        right_combo(ui, "hexright", right);
                        ui.end_row();
                        field(ui, "Value range from:", lo);
                        field(ui, "Value range to:", hi);
                        field(ui, "Factor:", factor);
                        field(ui, "Offset:", offset);
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(no_factor, "No factor / off.");
                        ui.checkbox(delta, "Difference");
                        ui.checkbox(percent, "Percent");
                        ui.checkbox(show_orig, "Original");
                        ui.checkbox(signed, "Sign");
                        ui.checkbox(dynamic, "Dynamic");
                    });
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            let n = cols.trim().parse::<usize>().unwrap_or(16).clamp(1, 4096);
                            let (o, rd, nf, de, pe, so, si, ri, dy) = (*org, *radix, *no_factor, *delta, *percent, *show_orig, *signed, *right, *dynamic);
                            let (l, h, f, of) = (num(lo).unwrap_or(0.0), num(hi).unwrap_or(255.0), num(factor).unwrap_or(1.0), num(offset).unwrap_or(0.0));
                            self.with_grid(|g| {
                                let a = g.addr(g.cursor);
                                g.cols = n;
                                g.org = o;
                                g.radix = rd;
                                g.no_factor = nf;
                                g.delta = de;
                                g.percent = pe;
                                g.show_orig = so;
                                g.signed = si;
                                g.right = ri;
                                g.dynamic = dy;
                                g.range_lo = l;
                                g.range_hi = h;
                                g.factor = if f == 0.0 { 1.0 } else { f };
                                g.offset = of;
                                g.cursor = g.cell_of(a);
                                g.follow = true;
                            });
                            keep = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
                let _ = cursor_range;
            }
            Modal::Protect { run, addr } => {
                use checksum::ProtectAnswer as A;
                let mut pick = None;
                self.win("This change is outside every checksum area", 480.0).show(ctx, |ui| {
                    ui.label(format!("Address {addr:X} is about to change, but no checksum area covers it. What should happen?"));
                    ui.add_space(4.0);
                    for (l, a) in [
                        ("Change this one", A::ThisOne),
                        ("Change all of them in this step", A::AllOfOperation),
                        ("Always change, and stop asking (turns the protection off)", A::Always),
                        ("Leave this one as it is", A::IgnoreThis),
                        ("Leave all of them as they are in this step", A::IgnoreAllProtected),
                        ("Cancel the whole step", A::CancelOperation),
                    ] {
                        if ui.button(l).clicked() {
                            pick = Some(a);
                        }
                    }
                });
                if let Some(a) = pick {
                    run.answer(a);
                    match run.next_question() {
                        Some(n) => *addr = n,
                        None => {
                            if let Some(pi) = self.active_project.and_then(|id| self.project_index(id)) {
                                self.commit_protected(pi, run);
                            }
                            keep = false;
                        }
                    }
                }
            }
            Modal::Checksums { sel } => self.checksums_dialog(ctx, sel, &mut keep),
            Modal::ChecksumEdit { cs, idx, from, to, target, target_end, error } => {
                self.win("Checksum settings", 340.0).show(ctx, |ui| {
                    ui.label("Algorithm:");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut cs.algo, CsAlgo::Additive, "Manual default (sum)");
                        ui.radio_value(&mut cs.algo, CsAlgo::Copy, "Copy");
                    });
                    egui::Grid::new("cse").num_columns(2).show(ui, |ui| {
                        field(ui, "Name:", &mut cs.name);
                        field(ui, "From (hex):", from);
                        field(ui, "To (hex):", to);
                        ui.label("Data:");
                        org_combo(ui, "csorg", &mut cs.org);
                        ui.end_row();
                        field(ui, "Correct to (hex):", target);
                        if cs.fullbyte {
                            field(ui, "Correct to, end (hex):", target_end);
                        }
                    });
                    ui.checkbox(&mut cs.auto, "Automatic correction");
                    ui.checkbox(&mut cs.fullbyte, "Fullbyte checksum");
                    ui.checkbox(&mut cs.display_only, "Only display the sum");
                    if !error.is_empty() {
                        ui.colored_label(egui::Color32::RED, error.as_str());
                    }
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            let parsed = (hex(from), hex(to), hex(target).or(if cs.display_only { Some(0) } else { None }));
                            match parsed {
                                (Some(a), Some(b), Some(t)) if a <= b => {
                                    cs.from = a;
                                    cs.to = b;
                                    cs.target = t;
                                    cs.target_end = if cs.fullbyte { hex(target_end).unwrap_or(t) } else { t };
                                    if checksum::overlaps_target(cs) {
                                        *error = "The range that is checked overlaps with the range that is changed by the checksum. It wouldn't work this way.".into();
                                    } else {
                                        let mut c = (**cs).clone();
                                        let i = *idx;
                                        if let Some(p) = self.current_project_mut() {
                                            let r = p.ref_data().to_vec();
                                            checksum::init_expected(&mut c, &r);
                                            match i {
                                                Some(i) => p.checksums[i] = c,
                                                None => p.checksums.push(c),
                                            }
                                            p.dirty_struct = true;
                                        }
                                        self.modal = Some(Modal::Checksums { sel: None });
                                        keep = false;
                                    }
                                }
                                _ => *error = "Please enter valid addresses.".into(),
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.modal = Some(Modal::Checksums { sel: None });
                            keep = false;
                        }
                    });
                });
            }
            Modal::SyncEdit { a, b, len } => {
                self.win("Mirrored block", 300.0).show(ctx, |ui| {
                    egui::Grid::new("sy").show(ui, |ui| {
                        field(ui, "First block (hex):", a);
                        field(ui, "Second block (hex):", b);
                        field(ui, "Length (hex):", len);
                    });
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            if let (Some(x), Some(y), Some(n)) = (hex(a), hex(b), hex(len)) {
                                if n > 0 && x + n <= y.max(x) || y + n <= x {
                                    if let Some(p) = self.current_project_mut() {
                                        p.syncs.push(SyncBlock { a: x, b: y, len: n });
                                        p.dirty_struct = true;
                                    }
                                    self.modal = Some(Modal::Checksums { sel: None });
                                    keep = false;
                                }
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.modal = Some(Modal::Checksums { sel: None });
                            keep = false;
                        }
                    });
                });
            }
            Modal::AsText { addr, text, list, filter } => {
                let mut defaults = self.cfg.default_texts.clone();
                self.win("Edit as text", 480.0).show(ctx, |ui| {
                    ui.label(format!("Text at {addr:X}:"));
                    ui.add(TextEdit::singleline(text).desired_width(440.0));
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            let (a, b) = (*addr, text.clone().into_bytes());
                            if let Some(p) = self.current_project_mut() {
                                p.begin_edit("As text");
                                p.write(a, &b);
                                p.end_edit();
                            }
                            keep = false;
                        }
                        if ui.button("Add to default texts").clicked() {
                            defaults.push(text.clone());
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Texts in this project:");
                        ui.text_edit_singleline(filter);
                    });
                    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                        for t in defaults.iter() {
                            if ui.selectable_label(false, format!("(default)  {t}")).clicked() {
                                *text = t.clone();
                            }
                        }
                        for (a, t) in list.iter().filter(|e| filter_matches(&e.1, filter)).take(1000) {
                            if ui.selectable_label(false, format!("{a:06X}  {t}")).clicked() {
                                *addr = *a;
                                *text = t.clone();
                            }
                        }
                    });
                });
                self.cfg.default_texts = defaults;
            }
            Modal::Comment { addr, end, text, color, existing } => {
                self.win("Comment", 360.0).show(ctx, |ui| {
                    ui.label(if *end > *addr { format!("Range {addr:X} - {end:X}") } else { format!("Address {addr:X}") });
                    ui.add(TextEdit::multiline(text).desired_rows(4).desired_width(330.0));
                    ui.horizontal(|ui| {
                        ui.label("Marking colour:");
                        ui.color_edit_button_srgb(color);
                    });
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            let c = Comment { addr: *addr, end: *end, text: text.clone(), color: *color };
                            let ex = *existing;
                            if let Some(p) = self.current_project_mut() {
                                if ex {
                                    p.comments.retain(|m| m.addr != c.addr);
                                }
                                p.comments.push(c);
                                p.dirty_struct = true;
                            }
                            keep = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            }
            Modal::ChooseRef { version, source, min, offset } => {
                let others: Vec<(u64, String)> = self.projects.iter().filter(|p| Some(p.id) != self.active_project).map(|p| (p.id, p.map_title_name())).collect();
                let names: Vec<String> = self.current_project().map(|p| p.versions.iter().map(|v| v.name.clone()).collect()).unwrap_or_default();
                self.win("Compare against", 380.0).show(ctx, |ui| {
                    ui.radio_value(source, None, "A version of this project:");
                    for (i, n) in names.iter().enumerate() {
                        ui.radio_value(version, i, format!("    {n}"));
                    }
                    for (id, n) in &others {
                        ui.radio_value(source, Some(*id), format!("Another project: {n}"));
                    }
                    egui::Grid::new("rf").show(ui, |ui| {
                        field(ui, "Minimum difference:", min);
                        field(ui, "Address offset:", offset);
                    });
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            let ext = source.and_then(|id| self.project(id).map(|o| (o.map_title_name(), std::sync::Arc::new(o.data().to_vec()))));
                            let (v, mn, off) = (*version, num(min).unwrap_or(0.0), offset.trim().parse::<i64>().unwrap_or(0));
                            if let Some(p) = self.current_project_mut() {
                                p.reference.version = v;
                                p.reference.min_diff = mn;
                                p.reference.offset = off;
                                p.reference.external_name = ext.as_ref().map(|e| e.0.clone());
                                p.reference.external = ext.map(|e| e.1);
                                p.revision += 1;
                            }
                            keep = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            }
            Modal::Connect { target, offset, reference, arrange, status } => {
                let me = self.active;
                let wins: Vec<(u64, String)> = self.children.iter().filter(|c| Some(c.id) != me).map(|c| (c.id, self.child_title(c))).collect();
                self.win("Link with a window", 420.0).show(ctx, |ui| {
                    if wins.is_empty() {
                        ui.label("Open a second window first, for example the same place in another project.");
                    }
                    for (id, t) in &wins {
                        ui.radio_value(target, Some(*id), t);
                    }
                    ui.horizontal(|ui| {
                        ui.label("Address offset (signed hex):");
                        ui.add(TextEdit::singleline(offset).desired_width(90.0));
                        if ui.add_enabled(target.is_some(), egui::Button::new("Automatically")).clicked() {
                            match self.connect_offset_auto(target.unwrap_or(0)) {
                                Some(o) => {
                                    *offset = fmt_signed(o);
                                    status.clear();
                                }
                                None => *status = "No similar area was found around the cursor.".into(),
                            }
                        }
                    });
                    if !status.is_empty() {
                        ui.label(status.as_str());
                    }
                    ui.checkbox(reference, "Also use as reference window");
                    ui.checkbox(arrange, "Arrange windows");
                    ui.horizontal(|ui| {
                        if ui.add_enabled(target.is_some(), egui::Button::new("OK")).clicked() {
                            let t = target.unwrap_or(0);
                            self.connect_windows(t, parse_signed(offset), *reference, *arrange);
                            keep = false;
                        }
                        if ui.button("Remove the connection").clicked() {
                            if let Some(c) = self.active_child_mut() {
                                c.connected = None;
                            }
                            self.connect_panel = false;
                            keep = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            }
            Modal::ImportChanges { step, source, tolerance, changed_only, absolute, items } => self.import_changes_dialog(ctx, step, source, tolerance, changed_only, absolute, items, &mut keep),
            Modal::ImportMaps { source } => {
                let others: Vec<(u64, String)> = self.projects.iter().filter(|p| Some(p.id) != self.active_project).map(|p| (p.id, p.map_title_name())).collect();
                self.win("Maps from another project", 360.0).show(ctx, |ui| {
                    ui.label("Take the map definitions from this project. The data doesn't change.");
                    for (id, n) in &others {
                        ui.radio_value(source, Some(*id), n);
                    }
                    if others.is_empty() {
                        ui.label("(Open the other project first.)");
                    }
                    ui.horizontal(|ui| {
                        if ui.add_enabled(source.is_some(), egui::Button::new("OK")).clicked() {
                            let maps = source.and_then(|s| self.project(s)).map(|p| p.maps.clone()).unwrap_or_default();
                            let n = maps.len();
                            if let Some(p) = self.current_project_mut() {
                                let d = p.data().to_vec();
                                for mut mm in maps {
                                    mm.uid = p.uid();
                                    mm.auto_range(&d);
                                    p.maps.push(mm);
                                }
                                p.dirty_struct = true;
                            }
                            self.tell("Import maps", format!("{n} map(s) imported."));
                            keep = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            }
            Modal::Parallel { tolerance, found, names, axes, sync, searched } => self.parallel_dialog(ctx, tolerance, found, names, axes, sync, searched, &mut keep),
            Modal::Replace { radix, find, with, in_original, status } => {
                self.win("Find and replace", 420.0).show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.selectable_value(radix, search::Radix::Dec, "Decimal");
                        ui.selectable_value(radix, search::Radix::Hex, "Hexadecimal");
                        ui.selectable_value(radix, search::Radix::Text, "Text");
                    });
                    egui::Grid::new("rep").show(ui, |ui| {
                        field(ui, "Search for:", find);
                        field(ui, "Replace with:", with);
                    });
                    ui.checkbox(in_original, "Search in the original");
                    if !status.is_empty() {
                        ui.label(status.as_str());
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Replace next").clicked() {
                            *status = self.replace_run(*radix, find, with, *in_original, false);
                        }
                        if ui.button("Replace all").clicked() {
                            *status = self.replace_run(*radix, find, with, *in_original, true);
                        }
                        if ui.button("Close").clicked() {
                            keep = false;
                        }
                    });
                });
            }
            Modal::Config { tab } => self.config_dialog(ctx, tab, &mut keep),
            Modal::SwapLines { addr, data, on, error } => {
                self.win("Swap address or data lines", 420.0).show(ctx, |ui| {
                    ui.checkbox(on, "Swap lines when importing and exporting");
                    ui.add_enabled_ui(*on, |ui| {
                        ui.label("For each line, the line of the file it is wired to:");
                        ui.label("Data lines:");
                        ui.horizontal_wrapped(|ui| {
                            for v in data.iter_mut() {
                                ui.add(egui::DragValue::new(v).range(0..=7));
                            }
                        });
                        ui.label("Address lines:");
                        ui.horizontal_wrapped(|ui| {
                            let n = addr.len().max(1) as u8 - 1;
                            for v in addr.iter_mut() {
                                ui.add(egui::DragValue::new(v).range(0..=n));
                            }
                        });
                    });
                    if !error.is_empty() {
                        ui.colored_label(egui::Color32::RED, error.as_str());
                    }
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            let s = LineSwap { addr: addr.clone(), data: data.clone() };
                            if !*on {
                                self.swap = None;
                                keep = false;
                            } else if s.valid() {
                                self.swap = Some(s);
                                keep = false;
                            } else {
                                *error = "Each line may be used only once.".into();
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            }
            Modal::AxisProfiles { name } => self.axis_profiles_dialog(ctx, name, &mut keep),
            Modal::Help(topic) => {
                let (title, text) = help_text(topic);
                self.win(title, 560.0).show(ctx, |ui| {
                    egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                        ui.label(text);
                    });
                    ui.add_space(6.0);
                    if ui.button("Close").clicked() {
                        keep = false;
                    }
                });
            }
            Modal::Tip => {
                let t = TIPS[self.cfg.tip_index % TIPS.len()];
                self.win("Tips", 420.0).show(ctx, |ui| {
                    ui.label(t);
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button("Next tip").clicked() {
                            self.cfg.tip_index += 1;
                        }
                        if ui.button("Close").clicked() {
                            keep = false;
                        }
                    });
                });
            }
            Modal::About => {
                self.win("About ALS", 380.0).show(ctx, |ui| {
                    ui.heading("ALS");
                    ui.label(crate::app::TITLE);
                    ui.label(format!("Language: {LANGUAGE_NAME}"));
                    ui.label("An editor for calibration data: projects, versions, maps, comparing and general checksums.");
                    ui.label("It does not read from or write to an ECU.");
                    ui.add_space(6.0);
                    if ui.button("Close").clicked() {
                        keep = false;
                    }
                });
            }
        }
        keep
    }

    fn finish_close(&mut self, id: u64, exit: bool) {
        self.close_project_now(id);
        if exit {
            let next = self.projects.iter().find(|p| p.dirty_data || p.dirty_struct).map(|p| p.id);
            match next {
                Some(n) => self.modal = Some(Modal::SaveChanges { pid: n, exit: true }),
                None => self.quit_now = true,
            }
        }
    }

    pub fn as_text_modal(&self) -> Modal {
        let (mut addr, mut text) = (0, String::new());
        let mut list = vec![];
        if let Some(c) = self.active_child() {
            if let Some(p) = self.project(c.project) {
                addr = c.g.addr(c.g.cursor);
                list = search::text_runs(p.data(), 6);
                let d = p.data();
                let mut e = addr;
                while e < d.len() && (0x20..0x7F).contains(&d[e]) {
                    e += 1;
                }
                text = String::from_utf8_lossy(&d[addr.min(d.len())..e]).to_string();
            }
        }
        Modal::AsText { addr, text, list, filter: String::new() }
    }

    pub fn comment_modal(&self) -> Modal {
        let Some(c) = self.active_child() else { return Modal::About };
        let a = c.g.addr(c.g.cursor);
        let total = c.g.total(self.end_of(c));
        if let Some(cm) = self.project(c.project).and_then(|p| p.comment_at(a)) {
            return Modal::Comment { addr: cm.addr, end: cm.end, text: cm.text.clone(), color: cm.color, existing: true };
        }
        let (mut s, mut e) = (a, a);
        if c.g.has_selection() {
            let cells = c.g.selected_cells(total);
            if let (Some(f), Some(l)) = (cells.first(), cells.last()) {
                s = c.g.addr(*f);
                e = c.g.addr(*l) + c.g.width() - 1;
            }
        }
        Modal::Comment { addr: s, end: e, text: String::new(), color: [255, 255, 0], existing: false }
    }

    fn run_import(&mut self, ctx: &egui::Context, fmt: FileFmt, swap: bool, overwrite: bool) {
        let target = self.active_project;
        let line = if swap { self.swap.clone() } else { None };
        let fix = |d: Vec<u8>| match &line {
            Some(s) if !s.is_identity() => s.apply(&d),
            _ => d,
        };
        match fmt {
            FileFmt::TwoFiles => {
                let Some(fs) = rfd::FileDialog::new().set_title("Choose the two files").pick_files() else { return };
                if fs.len() != 2 {
                    self.tell("Import", "Please choose exactly two files.");
                    return;
                }
                match (std::fs::read(&fs[0]), std::fs::read(&fs[1])) {
                    (Ok(a), Ok(b)) => {
                        let d = if formats::guess_high_first(&a, &b) { formats::merge_two(&a, &b) } else { formats::merge_two(&b, &a) };
                        self.take_data(fix(d), "", target, overwrite, ctx);
                    }
                    (Err(e), _) | (_, Err(e)) => self.tell("Import", e.to_string()),
                }
            }
            FileFmt::Binary => {
                let Some(f) = rfd::FileDialog::new().pick_file() else { return };
                match std::fs::read(&f) {
                    Ok(d) => {
                        let n = f.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                        self.take_data(fix(d), &n, target, overwrite, ctx);
                    }
                    Err(e) => self.tell("Import", e.to_string()),
                }
            }
            FileFmt::Intel | FileFmt::Motorola => {
                let Some(f) = rfd::FileDialog::new().pick_file() else { return };
                let r = std::fs::read_to_string(&f).map_err(|e| e.to_string()).and_then(|t| if fmt == FileFmt::Intel { formats::parse_ihex(&t) } else { formats::parse_srec(&t) });
                match r {
                    Ok(b) => {
                        let (s, o) = formats::suggest_size(&b);
                        self.modal = Some(Modal::hex_import(b, s, o, target));
                    }
                    Err(e) => self.tell("Import", e),
                }
            }
            FileFmt::Auto => {
                if let Some(f) = rfd::FileDialog::new().pick_file() {
                    self.import_path(&f, target, ctx);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn run_export(&mut self, fmt: FileFmt, swap: bool, from: &str, to: &str, per_line: &str, zip: bool, base: &str) -> Result<(), String> {
        let p = self.current_project().ok_or("No project is open.")?;
        let (a, b) = (hex(from).ok_or("Bad start address")?, hex(to).ok_or("Bad end address")?);
        let s = if swap { self.swap.clone() } else { None };
        let bytes = export_bytes(p.data(), a, b, s.as_ref());
        if bytes.is_empty() {
            return Err("The range is empty.".into());
        }
        let stem = p.map_title_name().trim_end_matches(".alsproj").to_string();
        let per = per_line.trim().parse().unwrap_or(32);
        let addr = hex(base).unwrap_or(0) as u64 + a as u64;
        match fmt {
            FileFmt::TwoFiles => {
                let Some(dir) = rfd::FileDialog::new().set_title("Folder for the two files").pick_folder() else { return Ok(()) };
                let (hi, lo) = formats::split_two(&bytes);
                std::fs::write(dir.join(format!("{stem}_1.bin")), hi).map_err(|e| e.to_string())?;
                std::fs::write(dir.join(format!("{stem}_2.bin")), lo).map_err(|e| e.to_string())?;
            }
            _ => {
                let (ext, content): (&str, Vec<u8>) = match fmt {
                    FileFmt::Intel => ("hex", formats::write_ihex(&bytes, addr, per).into_bytes()),
                    FileFmt::Motorola => ("s19", formats::write_srec(&bytes, addr, per).into_bytes()),
                    _ => ("bin", bytes),
                };
                let name = format!("{stem}.{ext}");
                let dlg = rfd::FileDialog::new().set_file_name(if zip { format!("{stem}.zip") } else { name.clone() });
                let Some(path) = dlg.save_file() else { return Ok(()) };
                if zip {
                    formats::write_zip(&path, &name, &content)?;
                } else {
                    std::fs::write(path, content).map_err(|e| e.to_string())?;
                }
            }
        }
        Ok(())
    }

    fn apply_edit(&mut self, base: &[(usize, f64)], from_original: bool, percent: bool, params: [f64; 5]) {
        let Some(c) = self.active_child() else { return };
        let g = c.g.clone();
        let (mut minc, mut maxc, mut minr, mut maxr) = (usize::MAX, 0, usize::MAX, 0);
        for (i, _) in base {
            minc = minc.min(i % g.cols);
            maxc = maxc.max(i % g.cols);
            minr = minr.min(i / g.cols);
            maxr = maxr.max(i / g.cols);
        }
        let (cols, rows) = ((maxc + 1 - minc.min(maxc)) as f64, (maxr + 1 - minr.min(maxr)) as f64);
        let map: std::collections::HashMap<usize, f64> = base.iter().copied().collect();
        let cells: Vec<usize> = map.keys().copied().collect();
        let shown = self.cfg.edit_shown_values;
        self.change_cells("Change by edit", Some(cells), &move |old, i, g, p| {
            let b = map.get(&i).copied().unwrap_or(old);
            let x = if from_original { p.ref_value(g.org, g.addr(i), g.signed).unwrap_or(b) } else { b };
            let w = edit_weights(((i % g.cols) - minc) as f64, ((i / g.cols) - minr) as f64, cols, rows, params);
            relative(x, w, percent, shown, g)
        });
    }

    fn project_props(&mut self, ctx: &egui::Context, pr: &mut Props, keep: &mut bool) {
        let info = self.current_project().map(|p| (p.map_title_name(), p.path.clone(), p.size(), p.versions.first().map(|v| sum8(&v.data)).unwrap_or(0)));
        self.win("Project details", 720.0).show(ctx, |ui| {
            ui.columns(3, |col| {
                col[0].group(|ui| {
                    ui.label(RichText::new("Customer:").strong());
                    egui::Grid::new("pc").num_columns(2).show(ui, |ui| {
                        field(ui, "Name:", &mut pr.client_name);
                        field(ui, "Licence plate:", &mut pr.licence);
                        field(ui, "Project state:", &mut pr.state);
                    });
                });
                col[0].group(|ui| {
                    ui.label(RichText::new("Vehicle:").strong());
                    egui::Grid::new("pv").num_columns(2).show(ui, |ui| {
                        field(ui, "Type:", &mut pr.vehicle_type);
                        field(ui, "Producer:", &mut pr.producer);
                        field(ui, "Chassis:", &mut pr.chassis);
                        field(ui, "Model:", &mut pr.model);
                        field(ui, "Modelyear:", &mut pr.model_year);
                        field(ui, "VIN:", &mut pr.vin);
                    });
                });
                col[1].group(|ui| {
                    ui.label(RichText::new("ECU:").strong());
                    egui::Grid::new("pe").num_columns(2).show(ui, |ui| {
                        field(ui, "Usage:", &mut pr.usage);
                        field(ui, "Producer:", &mut pr.ecu_producer);
                        field(ui, "Build:", &mut pr.ecu_build);
                        field(ui, "ECU Prod.:", &mut pr.ecu_prod);
                        field(ui, "ECU Stg.:", &mut pr.ecu_stg);
                        field(ui, "Softwareversion:", &mut pr.sw_version);
                        field(ui, "Softwaresize:", &mut pr.sw_size);
                        field(ui, "Processor:", &mut pr.processor);
                    });
                });
                col[1].group(|ui| {
                    ui.label(RichText::new("Engine:").strong());
                    egui::Grid::new("pg").num_columns(2).show(ui, |ui| {
                        field(ui, "Name:", &mut pr.engine_name);
                        field(ui, "Type:", &mut pr.engine_type);
                        field(ui, "Displacement:", &mut pr.displacement);
                        field(ui, "Output (PS):", &mut pr.ps);
                        field(ui, "Output (KW):", &mut pr.kw);
                        field(ui, "Transmission:", &mut pr.transmission);
                        field(ui, "Emission std.:", &mut pr.emission);
                    });
                });
                col[2].group(|ui| {
                    ui.label(RichText::new("File:").strong());
                    if let Some((n, path, size, sum)) = &info {
                        ui.label(format!("File: {n}"));
                        ui.label(format!("Folder: {}", path.as_ref().and_then(|x| x.parent()).map(|x| x.display().to_string()).unwrap_or_else(|| "-".into())));
                        ui.label(format!("Size: {size:X}"));
                        ui.label(format!("8-Bit sum: {sum:04X}"));
                    }
                    ui.label(format!("Created: {}", pr.created));
                    ui.label(format!("Changed: {}", pr.changed));
                });
                col[2].group(|ui| {
                    ui.label(RichText::new("User defined:").strong());
                    egui::Grid::new("pu").num_columns(2).show(ui, |ui| {
                        for i in 0..5 {
                            let label = format!("User {}:", i + 1);
                            field(ui, &label, &mut pr.user[i]);
                        }
                    });
                });
                if col[2].button("Comment...").clicked() {
                    self.pending.push(Cmd::ProjectComment);
                }
                if col[2].button("Elements...").clicked() {
                    let l = self.current_project().map(|p| p.elements.clone()).unwrap_or_default();
                    self.modal = Some(Modal::Elements { list: l });
                }
                if col[2].button("Change project size...").clicked() {
                    let s = self.current_project().map(|p| p.size()).unwrap_or(0);
                    self.modal = Some(Modal::Resize { size: format!("{s:X}"), at_start: false, fill: "FF".into() });
                }
                if col[2].button("Search vehicle data").clicked() {
                    if let Some(p) = self.current_project() {
                        let f = crate::analysis::search_vehicle_data(p.data());
                        let cpu = crate::analysis::guess_processor(p.data());
                        if let Some(v) = f.ecu_producer {
                            pr.ecu_producer = v;
                        }
                        if let Some(v) = f.ecu_prod {
                            pr.ecu_prod = v;
                        }
                        if let Some(v) = f.sw_version {
                            pr.sw_version = v;
                        }
                        if let Some(v) = f.ecu_build {
                            pr.ecu_build = v;
                        }
                        if let Some(v) = cpu {
                            pr.processor = v.into();
                        }
                    }
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("   OK   ").clicked() {
                    let v = pr.clone();
                    if let Some(p) = self.current_project_mut() {
                        p.props = v;
                        p.dirty_struct = true;
                    }
                    *keep = false;
                }
                if ui.button("Cancel").clicked() {
                    *keep = false;
                }
            });
        });
    }

    fn versions_dialog(&mut self, ctx: &egui::Context, sel: &mut usize, rename: &mut String, keep: &mut bool) {
        let Some(pid) = self.active_project else {
            *keep = false;
            return;
        };
        self.win("Versions", 620.0).show(ctx, |ui| {
            let Some(p) = self.project(pid) else { return };
            let names: Vec<(String, String)> = p.versions.iter().map(|v| (v.name.clone(), v.folder.clone())).collect();
            let changed = p.changed_maps(*sel);
            let comment = p.versions.get(*sel).map(|v| v.comment.clone()).unwrap_or_default();
            let current = p.current;
            let mut act = 0;
            ui.columns(2, |col| {
                egui::ScrollArea::vertical().id_salt("vl").max_height(230.0).show(&mut col[0], |ui| {
                    for (i, (n, f)) in names.iter().enumerate() {
                        let label = if f.is_empty() { n.clone() } else { format!("{f} / {n}") };
                        let t = if i == current { format!("▶ {label}") } else { label };
                        let t = if i == 0 { format!("{t}  (original)") } else { t };
                        if ui.selectable_label(*sel == i, t).clicked() {
                            *sel = i;
                            *rename = n.clone();
                        }
                    }
                });
                col[1].label("Comment:");
                col[1].label(if comment.is_empty() { "-" } else { comment.as_str() });
                col[1].separator();
                col[1].label(format!("{} map(s) changed from the original:", changed.len()));
                egui::ScrollArea::vertical().id_salt("vc").max_height(150.0).show(&mut col[1], |ui| {
                    for n in &changed {
                        ui.label(n);
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.add(TextEdit::singleline(rename).desired_width(180.0));
                if ui.button("Rename").clicked() {
                    act = 1;
                }
                if ui.button("Duplicate").clicked() {
                    act = 2;
                }
                if ui.add_enabled(names.len() > 1, egui::Button::new("Delete")).clicked() {
                    act = 3;
                }
                if ui.add_enabled(*sel != 0, egui::Button::new("As original")).clicked() {
                    act = 4;
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Open").clicked() {
                    act = 5;
                }
                if ui.button("Cancel").clicked() {
                    *keep = false;
                }
            });
            let s = *sel;
            let new_name = rename.clone();
            let Some(p) = self.project_mut(pid) else { return };
            match act {
                1 => {
                    if let Some(v) = p.versions.get_mut(s) {
                        v.name = new_name;
                    }
                    p.dirty_struct = true;
                }
                2 => {
                    if let Some(d) = p.versions.get(s).map(|v| v.data.clone()) {
                        p.add_version("", d);
                    }
                }
                3 => {
                    p.delete_version(s);
                    *sel = 0;
                }
                4 => {
                    p.make_original(s);
                    *sel = 0;
                }
                5 => {
                    p.current = s;
                    p.revision += 1;
                    p.undo.clear();
                    p.redo.clear();
                    *keep = false;
                }
                _ => {}
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn open_dialog(&mut self, ctx: &egui::Context, entries: &mut Vec<OpenEntry>, filter: &mut String, letter: &mut Option<char>, sel: &mut Option<usize>, as_filter: &mut bool, keep: &mut bool) {
        let dir = self.cfg.project_dir();
        let client = self.cfg.clients.get(self.cfg.client).map(|c| c.name.clone()).unwrap_or_default();
        let clients: Vec<String> = self.cfg.clients.iter().map(|c| c.name.clone()).collect();
        let mut switch = None;
        let mut open_path: Option<PathBuf> = None;
        self.win(&format!("Open a project in {client}"), 760.0).show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.selectable_label(letter.is_none(), "All ECUs").clicked() {
                    *letter = None;
                }
                for ch in 'A'..='Z' {
                    if ui.selectable_label(*letter == Some(ch), ch.to_string()).clicked() {
                        *letter = Some(ch);
                    }
                }
            });
            let today = today();
            let hay = |e: &OpenEntry| {
                let pr = &e.props;
                format!("{} {} {} {} {} {} {} {} {}", pr.vehicle_type, pr.producer, pr.chassis, pr.model, pr.usage, pr.ecu_producer, pr.ecu_prod, pr.sw_version, pr.client_name)
            };
            let rows: Vec<usize> = (0..entries.len())
                .filter(|&i| {
                    let e = &entries[i];
                    let first = e.props.producer.chars().next().map(|c| c.to_ascii_uppercase());
                    letter.is_none_or(|l| first == Some(l)) && (!*as_filter || filter_matches(&hay(e), filter))
                })
                .collect();
            let _ = today;
            egui::ScrollArea::both().max_height(280.0).auto_shrink([false, false]).show(ui, |ui| {
                egui::Grid::new("opn").striped(true).num_columns(8).show(ui, |ui| {
                    for h in ["Type", "Producer", "Chassis", "Model", "Use", "ECU producer", "Software", "File"] {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                    for &i in &rows {
                        let e = &entries[i];
                        let pr = &e.props;
                        let name = e.path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                        let r = ui.selectable_label(*sel == Some(i), &pr.vehicle_type);
                        if r.clicked() {
                            *sel = Some(i);
                        }
                        if r.double_clicked() {
                            open_path = Some(e.path.clone());
                        }
                        ui.label(&pr.producer);
                        ui.label(&pr.chassis);
                        ui.label(&pr.model);
                        ui.label(&pr.usage);
                        ui.label(&pr.ecu_producer);
                        ui.label(&pr.sw_version);
                        ui.label(name);
                        ui.end_row();
                    }
                });
            });
            if let Some(e) = sel.and_then(|i| entries.get(i)) {
                ui.label(format!("Created: {}   Changed: {}   Versions: {}   Maps: {}", e.props.created, e.props.changed, e.versions, e.maps));
            }
            ui.horizontal(|ui| {
                ui.label("Search for:");
                ui.add(TextEdit::singleline(filter).desired_width(380.0).hint_text("e.g. BMW -Siemens"));
                ui.checkbox(as_filter, "Filter instead of search");
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(sel.is_some(), egui::Button::new("OK")).clicked() {
                    open_path = sel.and_then(|i| entries.get(i)).map(|e| e.path.clone());
                }
                if ui.button("Other file...").clicked() {
                    open_path = rfd::FileDialog::new().add_filter("Projects", &["alsproj"]).set_directory(&dir).pick_file();
                }
                if ui.button("Cancel").clicked() {
                    *keep = false;
                }
                ui.separator();
                ComboBox::from_id_salt("client").selected_text(format!("Folder: {client}")).show_ui(ui, |ui| {
                    for (i, c) in clients.iter().enumerate() {
                        if ui.selectable_label(i == self.cfg.client, c).clicked() {
                            switch = Some(i);
                        }
                    }
                });
            });
            ui.label(format!("Folder: {}", dir.display()));
        });
        if let Some(i) = switch {
            self.cfg.client = i;
            self.modal = Some(Modal::open_project(&self.cfg));
            *keep = false;
        }
        if let Some(p) = open_path {
            self.import_path(&p, None, ctx);
            *keep = false;
        }
    }

    fn map_props_dialog(&mut self, ctx: &egui::Context, m: &mut MapDef, tab: &mut usize, xu: &mut String, yu: &mut String, keep: &mut bool) {
        let presets = self.cfg.unit_presets.clone();
        let others: Vec<(u64, String)> = self.current_project().map(|p| p.maps.iter().filter(|o| o.uid != m.uid && o.x.addr == m.x.addr && o.y.addr == m.y.addr).map(|o| (o.uid, o.name.clone())).collect()).unwrap_or_default();
        let cursor = self.active_child().map(|c| c.g.addr(c.g.cursor));
        let hex_cursor = self.children.iter().find(|c| c.map_uid.is_none() && Some(c.project) == self.active_project).map(|c| c.g.addr(c.g.cursor)).or(cursor);
        self.win("Map settings", 430.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(tab, 0, "Map properties");
                ui.selectable_value(tab, 1, "X-Axis");
                ui.selectable_value(tab, 2, "Y-Axis");
                ui.selectable_value(tab, 3, "3d");
            });
            ui.separator();
            match *tab {
                0 => {
                    egui::Grid::new("mp").num_columns(2).show(ui, |ui| {
                        field(ui, "Description:", &mut m.desc);
                        field(ui, "Unit:", &mut m.unit);
                        field(ui, "Id:", &mut m.id_name);
                        field(ui, "Name:", &mut m.name);
                        field(ui, "Folder:", &mut m.folder);
                        ui.label("Start address:");
                        hex_edit(ui, &mut m.start);
                        ui.end_row();
                        ui.label("Column x rows:");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut m.cols).range(1..=4096));
                            ui.label("x");
                            ui.add(egui::DragValue::new(&mut m.rows).range(1..=4096));
                        });
                        ui.end_row();
                        ui.label("Values:");
                        org_combo(ui, "morg", &mut m.org);
                        ui.end_row();
                        ui.label("Number format:");
                        radix_combo(ui, "mrad", &mut m.radix);
                        ui.end_row();
                        ui.label("Organization:");
                        ComboBox::from_id_salt("mkind").selected_text(m.kind.label()).show_ui(ui, |ui| {
                            for k in [MapKind::Single, MapKind::OneDim, MapKind::TwoDim, MapKind::TwoDimInv] {
                                ui.selectable_value(&mut m.kind, k, k.label());
                            }
                        });
                        ui.end_row();
                        ui.label("Right side:");
                        right_combo(ui, "mright", &mut m.right);
                        ui.end_row();
                        ui.label("Value range:");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut m.range_lo));
                            ui.label("-");
                            ui.add(egui::DragValue::new(&mut m.range_hi));
                        });
                        ui.end_row();
                        ui.label("Factor & Offset:");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut m.factor).speed(0.001).max_decimals(8));
                            ui.add(egui::DragValue::new(&mut m.offset).speed(0.1).max_decimals(8));
                        });
                        ui.end_row();
                        ui.label("Precision:");
                        ui.add(egui::DragValue::new(&mut m.precision).range(0..=8));
                        ui.end_row();
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut m.reciprocal, "Reciprocal");
                        ui.checkbox(&mut m.signed, "Sign");
                        if ui.button("Auto").on_hover_text("Fit the value range to the data").clicked() {
                            if let Some(p) = self.current_project() {
                                let d = p.data().to_vec();
                                m.auto_range(&d);
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Units:");
                        for u in &presets {
                            if ui.small_button(&u.name).clicked() {
                                m.unit = u.unit.clone();
                                m.factor = u.factor;
                                m.offset = u.offset;
                                m.precision = u.precision;
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Variable offset:");
                        let cur = m.var_offset.and_then(|u| others.iter().find(|o| o.0 == u)).map(|o| o.1.clone()).unwrap_or_else(|| "(none)".into());
                        ComboBox::from_id_salt("mvar").selected_text(cur).show_ui(ui, |ui| {
                            ui.selectable_value(&mut m.var_offset, None, "(none)");
                            for (u, n) in &others {
                                ui.selectable_value(&mut m.var_offset, Some(*u), n);
                            }
                        });
                    });
                }
                1 | 2 => {
                    let is_x = *tab == 1;
                    let (ax, user) = if is_x { (&mut m.x, &mut *xu) } else { (&mut m.y, &mut *yu) };
                    egui::Grid::new("ax").num_columns(2).show(ui, |ui| {
                        field(ui, "Description:", &mut ax.desc);
                        field(ui, "Unit:", &mut ax.unit);
                        ui.label("Data source:");
                        ComboBox::from_id_salt("axs").selected_text(ax.src.label()).show_ui(ui, |ui| {
                            for s in AxisSrc::ALL {
                                ui.selectable_value(&mut ax.src, s, s.label());
                            }
                        });
                        ui.end_row();
                        ui.label("Start address:");
                        ui.horizontal(|ui| {
                            hex_edit(ui, &mut ax.addr);
                            if ui.button("From hexdump cursor").clicked() {
                                if let Some(a) = hex_cursor {
                                    ax.addr = a;
                                }
                            }
                        });
                        ui.end_row();
                        ui.label("Values:");
                        org_combo(ui, "axorg", &mut ax.org);
                        ui.end_row();
                        ui.label("Skip bytes:");
                        ui.add(egui::DragValue::new(&mut ax.skip).range(0..=64));
                        ui.end_row();
                        ui.label("Number format:");
                        radix_combo(ui, "axrad", &mut ax.radix);
                        ui.end_row();
                        ui.label("Factor & Offset:");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut ax.factor).speed(0.001).max_decimals(8));
                            ui.add(egui::DragValue::new(&mut ax.offset).speed(0.1).max_decimals(8));
                        });
                        ui.end_row();
                        ui.label("Precision:");
                        ui.add(egui::DragValue::new(&mut ax.precision).range(0..=8));
                        ui.end_row();
                        if ax.src == AxisSrc::User {
                            field(ui, "Values (with spaces):", user);
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut ax.reciprocal, "Reciprocal");
                        ui.checkbox(&mut ax.signed, "Sign");
                        ui.checkbox(&mut m.mirror, "Mirror map");
                    });
                }
                _ => {
                    let id = self.active.unwrap_or(0);
                    if let Some(i) = self.child_index(id) {
                        let g = &mut self.children[i].g;
                        ui.add(egui::Slider::new(&mut g.rot, -3.2..=3.2).text("Rotation"));
                        ui.add(egui::Slider::new(&mut g.persp, 0.0..=1.2).text("Perspective"));
                        ui.add(egui::Slider::new(&mut g.zscale, 0.05..=8.0).text("Z-Scaling"));
                        ui.add(egui::Slider::new(&mut g.zoffset, -1.0..=1.0).text("Z-Offset"));
                        ui.checkbox(&mut g.show_axis, "Show axis description");
                    }
                }
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("   OK   ").clicked() {
                    m.x.user = xu_values(xu);
                    m.y.user = xu_values(yu);
                    if m.factor == 0.0 {
                        m.factor = 1.0;
                    }
                    let v = m.clone();
                    let uid = v.uid;
                    if let Some(p) = self.current_project_mut() {
                        if let Some(t) = p.maps.iter_mut().find(|t| t.uid == uid) {
                            *t = v.clone();
                        }
                        p.dirty_struct = true;
                    }
                    for c in self.children.iter_mut().filter(|c| c.map_uid == Some(uid)) {
                        let (mode, font, cur) = (c.g.mode, c.g.font, c.g.cursor);
                        c.g.apply_map(&v);
                        c.g.mode = mode;
                        c.g.font = font;
                        c.g.cursor = cur.min(v.cells().saturating_sub(1));
                    }
                    *keep = false;
                }
                if ui.button("Cancel").clicked() {
                    *keep = false;
                }
            });
        });
    }

    fn checksums_dialog(&mut self, ctx: &egui::Context, sel: &mut Option<usize>, keep: &mut bool) {
        let Some(pid) = self.active_project else {
            *keep = false;
            return;
        };
        self.win("Checksums", 620.0).show(ctx, |ui| {
            let Some(p) = self.project(pid) else { return };
            let rows: Vec<(String, usize, usize, usize, bool, bool)> = p.checksums.iter().map(|c| (if c.name.is_empty() { "Checksum".to_string() } else { c.name.clone() }, c.from, c.to, c.target, c.auto, checksum::is_ok(p.data(), c))).collect();
            let syncs = p.syncs.clone();
            let mut protect = p.protect_outside;
            ui.label("The following checksums exist:");
            egui::ScrollArea::vertical().max_height(170.0).auto_shrink([false, false]).show(ui, |ui| {
                egui::Grid::new("csl").striped(true).num_columns(6).show(ui, |ui| {
                    for h in ["Algorithm", "From", "To", "Correct to", "AutoCorrect", "State"] {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                    for (i, (n, a, b, t, au, ok)) in rows.iter().enumerate() {
                        if ui.selectable_label(*sel == Some(i), n).clicked() {
                            *sel = Some(i);
                        }
                        ui.label(format!("{a:X}"));
                        ui.label(format!("{b:X}"));
                        ui.label(format!("{t:X}"));
                        ui.label(if *au { "yes" } else { "no" });
                        ui.colored_label(if *ok { crate::shell::rgb(0, 130, 0) } else { egui::Color32::RED }, if *ok { "okay" } else { "wrong" });
                        ui.end_row();
                    }
                    for s in &syncs {
                        ui.label("Sync block");
                        ui.label(format!("{:X}", s.a));
                        ui.label(format!("{:X}", s.a + s.len - 1));
                        ui.label(format!("{:X}", s.b));
                        ui.label("yes");
                        ui.label("-");
                        ui.end_row();
                    }
                });
            });
            let mut act = 0;
            ui.horizontal(|ui| {
                if ui.button("Search").on_hover_text("Finds identical blocks in the file. Car-specific checksum modules aren't part of this version.").clicked() {
                    act = 1;
                }
                if ui.button("Add...").clicked() {
                    act = 2;
                }
                if ui.button("Add mirrored block...").clicked() {
                    act = 3;
                }
                if ui.add_enabled(sel.is_some(), egui::Button::new("Properties...")).clicked() {
                    act = 4;
                }
                if ui.add_enabled(sel.is_some(), egui::Button::new("Delete")).clicked() {
                    act = 5;
                }
                if ui.add_enabled(sel.is_some(), egui::Button::new("Apply")).clicked() {
                    act = 6;
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Correct all checksums").clicked() {
                    act = 7;
                }
                if ui.button("Remove all, keep the values as they are").clicked() {
                    act = 8;
                }
                if ui.button("Remove all, put the original values back").clicked() {
                    act = 9;
                }
            });
            ui.checkbox(&mut protect, "Ask before changing data outside the checksum areas");
            if ui.button("   OK   ").clicked() {
                *keep = false;
            }
            let s = *sel;
            let mut note = None;
            if let Some(p) = self.project_mut(pid) {
                p.protect_outside = protect;
                match act {
                    1 => {
                        let found = checksum::search_blocks(p.data());
                        let n = found.len();
                        for (a, b, len) in found {
                            if !p.syncs.iter().any(|x| x.a == a && x.b == b) {
                                p.syncs.push(SyncBlock { a, b, len });
                            }
                        }
                        p.dirty_struct = n > 0 || p.dirty_struct;
                        note = Some(if n == 0 { "No mirrored blocks were found. You can add a checksum by hand.".to_string() } else { format!("{n} identical block pair(s) were found and are now kept in sync.") });
                    }
                    5 => {
                        if let Some(i) = s {
                            if i < p.checksums.len() {
                                p.checksums.remove(i);
                            }
                            p.dirty_struct = true;
                        }
                        *sel = None;
                    }
                    7 => {
                        if !checksum::correct_all(p) {
                            note = Some("Some checksums or mirrored blocks could not be corrected.".to_string());
                        }
                    }
                    8 | 9 => {
                        checksum::delete_all(p, act == 9);
                        *sel = None;
                    }
                    6 => {
                        if let Some(i) = s {
                            p.begin_edit("Checksum");
                            if let Err(e) = checksum::correct(p, i) {
                                note = Some(e);
                            }
                            p.end_edit();
                        }
                    }
                    _ => {}
                }
            }
            if let Some(n) = note {
                self.tell("Checksums", n);
            }
            match act {
                2 => {
                    self.modal = Some(Modal::ChecksumEdit { cs: Box::new(Checksum { name: "Checksum".into(), auto: true, ..Checksum::default() }), idx: None, from: String::new(), to: String::new(), target: String::new(), target_end: String::new(), error: String::new() });
                    *keep = false;
                }
                3 => {
                    self.modal = Some(Modal::SyncEdit { a: String::new(), b: String::new(), len: String::new() });
                    *keep = false;
                }
                4 => {
                    if let Some(c) = s.and_then(|i| self.project(pid).and_then(|p| p.checksums.get(i))).cloned() {
                        self.modal = Some(Modal::ChecksumEdit { from: format!("{:X}", c.from), to: format!("{:X}", c.to), target: format!("{:X}", c.target), target_end: format!("{:X}", c.target_end), cs: Box::new(c), idx: s, error: String::new() });
                        *keep = false;
                    }
                }
                _ => {}
            }
        });
    }

    fn connect_offset_auto(&self, target: u64) -> Option<i64> {
        let c = self.active_child()?;
        let other = self.children.iter().find(|x| x.id == target)?;
        let p = self.project(c.project)?;
        let q = self.project(other.project)?;
        let a = c.g.addr(c.g.cursor);
        let w = c.g.width();
        let n = 32usize;
        let start = a.saturating_sub(n / 2 * w);
        let block: Vec<f64> = (0..n).filter_map(|i| c.g.org.read(p.data(), start + i * w, false)).collect();
        let same = if c.project == other.project { Some((start, start + n * w)) } else { None };
        let hits = search::find_similar(q.data(), &block, c.g.org, 2.0, 200, same);
        hits.first().map(|h| h.0 as i64 - start as i64)
    }

    fn connect_windows(&mut self, target: u64, offset: i64, reference: bool, arrange: bool) {
        let Some(me) = self.active else { return };
        if let Some(c) = self.active_child_mut() {
            c.connected = Some(target);
            c.connect_offset = offset;
        }
        self.connect_panel = true;
        if reference {
            let other = self.children.iter().find(|x| x.id == target).map(|x| x.project);
            let data = other.and_then(|o| self.project(o)).map(|o| (o.map_title_name(), std::sync::Arc::new(o.data().to_vec())));
            if let Some(p) = self.current_project_mut() {
                if let Some((n, d)) = data {
                    p.reference.external_name = Some(n);
                    p.reference.external = Some(d);
                    p.reference.offset = offset;
                    p.revision += 1;
                }
            }
        }
        if arrange {
            let ws = self.workspace;
            let half = ws.width() / 2.0;
            for (id, x) in [(me, 0.0), (target, half)] {
                if let Some(c) = self.children.iter_mut().find(|c| c.id == id) {
                    c.rect = egui::Rect::from_min_size(ws.min + Vec2::new(x, 0.0), Vec2::new(half, ws.height()));
                    c.restore = None;
                    c.minimized = false;
                }
            }
        }
    }

    pub fn connect_window(&mut self, ctx: &egui::Context) {
        let Some(me) = self.children.iter().find(|c| c.connected.is_some()).map(|c| c.id) else {
            self.connect_panel = false;
            return;
        };
        let (other, off) = self.children.iter().find(|c| c.id == me).map(|c| (c.connected.unwrap_or(0), c.connect_offset)).unwrap_or((0, 0));
        // Both windows scroll together, whichever one was moved.
        let a = self.children.iter().find(|c| c.id == me).map(|c| (c.g.addr(c.g.top_row * c.g.cols), c.g.addr(c.g.cursor), self.active == Some(c.id)));
        let b = self.children.iter().find(|c| c.id == other).map(|c| (c.g.addr(c.g.top_row * c.g.cols), c.g.addr(c.g.cursor)));
        if let (Some((ta, ca, a_active)), Some((tb, cb))) = (a, b) {
            if a_active {
                if let Some(c) = self.children.iter_mut().find(|c| c.id == other) {
                    let tt = (ta as i64 + off).max(0) as usize;
                    c.g.top_row = c.g.cell_of(tt) / c.g.cols.max(1);
                    c.g.cursor = c.g.cell_of((ca as i64 + off).max(0) as usize);
                }
            } else if let Some(c) = self.children.iter_mut().find(|c| c.id == me) {
                let tt = (tb as i64 - off).max(0) as usize;
                c.g.top_row = c.g.cell_of(tt) / c.g.cols.max(1);
                c.g.cursor = c.g.cell_of((cb as i64 - off).max(0) as usize);
            }
        }
        let mut open = true;
        let mut new_off = off;
        let mut act = 0;
        let w = self.children.iter().find(|c| c.id == me).map(|c| c.g.width() as i64).unwrap_or(1);
        egui::Window::new("Linked windows").order(egui::Order::Foreground).open(&mut open).default_width(240.0).default_pos(self.workspace.center_top() + Vec2::new(-120.0, 8.0)).show(ctx, |ui| {
            ui.label(format!("Offset: {}", fmt_signed(off)));
            ui.horizontal(|ui| {
                if ui.button("-").clicked() {
                    new_off -= w;
                }
                if ui.button("+").clicked() {
                    new_off += w;
                }
                if ui.button("At cursor").clicked() {
                    act = 1;
                }
                if ui.button("Search").clicked() {
                    act = 2;
                }
            });
            ui.separator();
            ui.label("Differences:");
            ui.horizontal(|ui| {
                if ui.button("|<").clicked() {
                    self.next_diff(DiffMove::First);
                }
                if ui.button("<").clicked() {
                    self.next_diff(DiffMove::Prev);
                }
                if ui.button(">").clicked() {
                    self.next_diff(DiffMove::Next);
                }
                if ui.button(">|").clicked() {
                    self.next_diff(DiffMove::Last);
                }
            });
            ui.separator();
            if ui.button("Copy the selection to the other window").clicked() {
                act = 3;
            }
        });
        match act {
            1 => {
                if let (Some((_, ca, _)), Some((_, cb))) = (a, b) {
                    new_off = cb as i64 - ca as i64;
                }
            }
            2 => {
                self.active = Some(me);
                if let Some(o) = self.connect_offset_auto(other) {
                    new_off = o;
                } else {
                    self.status_hint = "No similar area was found around the cursor.".into();
                }
            }
            3 => self.copy_to_connected(me, other, new_off),
            _ => {}
        }
        if let Some(c) = self.children.iter_mut().find(|c| c.id == me) {
            c.connect_offset = new_off;
            if !open {
                c.connected = None;
            }
        }
        self.connect_panel = open;
    }

    fn copy_to_connected(&mut self, me: u64, other: u64, off: i64) {
        let Some(c) = self.children.iter().find(|c| c.id == me).cloned() else { return };
        let Some(o) = self.children.iter().find(|c| c.id == other).map(|x| x.project) else { return };
        let Some(p) = self.project(c.project) else { return };
        let total = c.g.total(self.end_of(&c));
        let writes: Vec<(usize, Vec<u8>)> = c.g.selected_cells(total).into_iter().filter_map(|i| {
            let a = c.g.addr(i);
            let b = p.data().get(a..a + c.g.width())?.to_vec();
            let t = a as i64 + off;
            (t >= 0).then_some((t as usize, b))
        }).collect();
        if let Some(q) = self.project_mut(o) {
            q.begin_edit("Transfer values");
            for (a, b) in writes {
                q.write(a, &b);
            }
            q.end_edit();
        }
    }

    pub fn find_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let mut run: Option<(bool, bool)> = None;
        let mut list = false;
        let mut jump = None;
        let similar = self.find.similar.is_some();
        let before = self.find.text.clone();
        let title = if similar { "Search for the selection" } else { "Search for byte sequence" };
        egui::Window::new(title).id(egui::Id::unique("find")).order(egui::Order::Foreground).open(&mut open).default_width(400.0).default_pos(self.workspace.left_top() + Vec2::new(60.0, 40.0)).show(ctx, |ui| {
            let f = &mut self.find;
            if similar {
                ui.label(format!("Looking for {} values like the selected ones.", f.similar.as_ref().map(|v| v.len()).unwrap_or(0)));
                ui.add(egui::Slider::new(&mut f.tolerance, 0.0..=50.0).text("Tolerance %"));
                if ui.button("Search for text or bytes instead").clicked() {
                    f.similar = None;
                }
            } else {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut f.tab, search::Radix::Dec, "Decimal");
                    ui.selectable_value(&mut f.tab, search::Radix::Hex, "Hexadecimal");
                    ui.selectable_value(&mut f.tab, search::Radix::Text, "Text");
                });
                ui.add(TextEdit::singleline(&mut f.text).desired_width(370.0).hint_text("ff ff aa ?? ab"));
                ui.columns(2, |col| {
                    col[0].checkbox(&mut f.realtime, "Search in Realtime");
                    col[0].checkbox(&mut f.in_original, "In original");
                    col[0].add_enabled(f.tab == search::Radix::Text, egui::Checkbox::new(&mut f.match_case, "Match case"));
                    col[1].horizontal(|ui| {
                        ui.label("Range (hex)");
                        ui.add(TextEdit::singleline(&mut f.from).desired_width(52.0));
                        ui.add(TextEdit::singleline(&mut f.to).desired_width(52.0));
                    });
                    col[1].horizontal(|ui| {
                        ui.label("Max. diff u/d");
                        ui.add(TextEdit::singleline(&mut f.up).desired_width(40.0));
                        ui.add(TextEdit::singleline(&mut f.down).desired_width(40.0));
                    });
                });
                ui.horizontal(|ui| {
                    ui.label("Addresses:");
                    ui.selectable_value(&mut f.align, search::Align::Auto, "automatic");
                    ui.selectable_value(&mut f.align, search::Align::All, "all");
                    ui.selectable_value(&mut f.align, search::Align::Even, "even");
                    ui.selectable_value(&mut f.align, search::Align::Four, "by 4");
                    ui.checkbox(&mut f.skip_code, "Not in program code");
                });
            }
            ui.horizontal(|ui| {
                if ui.button("Start").clicked() {
                    list = true;
                }
                if ui.button("◀").clicked() {
                    run = Some((false, false));
                }
                if ui.button("▶").clicked() {
                    run = Some((true, false));
                }
                ui.checkbox(&mut f.show_list, "List");
            });
            if f.show_list {
                egui::ScrollArea::vertical().max_height(180.0).auto_shrink([false, true]).show(ui, |ui| {
                    for (i, a) in f.results.iter().enumerate() {
                        if ui.selectable_label(false, format!("{:>3}   {a:06X}", i + 1)).clicked() {
                            jump = Some(*a);
                        }
                    }
                });
            }
            ui.label(f.status.as_str());
        });
        self.find.open = open;
        if self.find.realtime && self.find.text != before && !self.find.text.is_empty() {
            run = Some((true, true));
        }
        if list {
            if similar {
                self.similar_step(true);
            } else {
                self.find_list();
            }
        }
        if let Some((forward, restart)) = run {
            self.find_step(forward, restart);
        }
        if let Some(a) = jump {
            self.goto_addr(a);
        }
    }

    fn replace_run(&mut self, radix: search::Radix, find: &str, with: &str, in_original: bool, all: bool) -> String {
        let Some(c) = self.active_child() else { return "No window is open.".into() };
        let g = c.g.clone();
        let pid = c.project;
        let start = g.addr(g.cursor);
        let Some(p) = self.project_mut(pid) else { return String::new() };
        let (range0, range1) = p.element_range();
        let mut q = search::Query { org: g.org, from: range0, to: range1.saturating_sub(1), ..Default::default() };
        let parsed_with: Vec<Option<f64>>;
        if radix == search::Radix::Text {
            q.org = DataOrg::Byte;
            let t = search::parse_text(find);
            parsed_with = search::parse_text(with).iter().map(|b| b.map(|x| x as f64)).collect();
            if t.len() != parsed_with.len() {
                return "Both texts must have the same length. You may append question marks to the shorter one.".into();
            }
            q.text = Some(t);
        } else {
            let (a, b) = match (search::parse_pattern(find, radix), search::parse_pattern(with, radix)) {
                (Ok(a), Ok(b)) => (a, b),
                (Err(e), _) | (_, Err(e)) => return e,
            };
            if a.len() != b.len() {
                return "Both texts must have the same length. You may append question marks to the shorter one.".into();
            }
            q.cells = a;
            parsed_with = b;
        }
        let w = q.org.width();
        let hits: Vec<usize> = {
            let d = if in_original { p.ref_data() } else { p.data() };
            if all {
                search::find_all(d, &q, usize::MAX, None)
            } else {
                search::find_next(d, &q, start, true).or_else(|| search::find_next(d, &q, range0, true)).into_iter().collect()
            }
        };
        if hits.is_empty() {
            return "no search results".into();
        }
        p.begin_edit("Replace");
        for &a in &hits {
            for (i, v) in parsed_with.iter().enumerate() {
                if let Some(v) = v {
                    let bytes = q.org.encode(*v, false);
                    p.write(a + i * w, &bytes);
                }
            }
        }
        p.end_edit();
        let n = hits.len();
        if !all {
            let a = hits[0];
            let width = parsed_with.len() * w;
            self.goto_addr(a + width);
        }
        format!("{n} occurrence(s) replaced.")
    }

    #[allow(clippy::too_many_arguments)]
    fn import_changes_dialog(&mut self, ctx: &egui::Context, step: &mut usize, source: &mut Option<u64>, tolerance: &mut f64, changed_only: &mut bool, absolute: &mut bool, items: &mut Vec<ChangeItem>, keep: &mut bool) {
        let others: Vec<(u64, String)> = self.projects.iter().filter(|p| Some(p.id) != self.active_project).map(|p| (p.id, p.map_title_name())).collect();
        let title = if *step == 0 { "Import changes (1/2)" } else { "Import changes (2/2)" };
        self.win(title, 560.0).show(ctx, |ui| {
            if *step == 0 {
                ui.label("From which open project do you want to take maps and changes?");
                for (id, n) in &others {
                    ui.radio_value(source, Some(*id), n);
                }
                if others.is_empty() {
                    ui.colored_label(crate::shell::rgb(170, 0, 0), "Only opened projects can be chosen. Please open the other project first.");
                }
                ui.checkbox(changed_only, "Only show maps that were changed in that project");
                ui.add(egui::Slider::new(tolerance, 0.0..=30.0).text("Tolerance for recognising maps (%)"));
                ui.horizontal(|ui| {
                    if ui.add_enabled(source.is_some(), egui::Button::new("Next >")).clicked() {
                        *items = self.collect_changes(source.unwrap_or(0), *changed_only, *tolerance);
                        *step = 1;
                    }
                    if ui.button("Cancel").clicked() {
                        *keep = false;
                    }
                });
            } else {
                ui.label(format!("{} item(s). Those without a place weren't recognised in this project.", items.len()));
                egui::ScrollArea::vertical().max_height(300.0).auto_shrink([false, true]).show(ui, |ui| {
                    egui::Grid::new("chg").striped(true).num_columns(5).show(ui, |ui| {
                        for h in ["Name", "From", "Found at", "Map", "Data"] {
                            ui.label(RichText::new(h).strong());
                        }
                        ui.end_row();
                        for it in items.iter_mut() {
                            ui.label(&it.name);
                            ui.label(format!("{:06X}", it.src));
                            ui.label(it.target.map(|t| format!("{t:06X}")).unwrap_or_else(|| "-".into()));
                            ui.add_enabled(it.target.is_some() && it.map.is_some(), egui::Checkbox::without_text(&mut it.take_map));
                            ui.add_enabled(it.target.is_some(), egui::Checkbox::without_text(&mut it.take_data));
                            ui.end_row();
                        }
                    });
                });
                ui.checkbox(absolute, "Transfer values absolute instead of as difference");
                ui.horizontal(|ui| {
                    if ui.button("< Back").clicked() {
                        *step = 0;
                    }
                    if ui.button("Finish").clicked() {
                        let t = self.apply_changes(source.unwrap_or(0), items, *absolute);
                        self.tell("Import changes", t);
                        *keep = false;
                    }
                    if ui.button("Cancel").clicked() {
                        *keep = false;
                    }
                });
            }
        });
    }

    fn collect_changes(&self, source: u64, changed_only: bool, tol: f64) -> Vec<ChangeItem> {
        let (Some(src), Some(dst)) = (self.project(source), self.current_project()) else { return vec![] };
        let mut out = vec![];
        let (d, r) = (src.data(), src.ref_data());
        let mut covered: Vec<(usize, usize)> = vec![];
        for m in &src.maps {
            let e = m.end().min(d.len());
            let changed = m.start < e && e <= r.len() && d[m.start..e] != r[m.start..e];
            covered.push((m.start, m.end()));
            if changed_only && !changed {
                continue;
            }
            let block: Vec<f64> = (0..m.cells()).filter_map(|i| m.org.read(r, m.start + i * m.org.width(), false)).collect();
            let target = search::find_similar(dst.data(), &block, m.org, tol, 1, None).first().map(|h| h.0);
            out.push(ChangeItem { name: m.name.clone(), map: Some(m.clone()), src: m.start, len: m.byte_len(), org: m.org, target, take_map: target.is_some(), take_data: target.is_some() && changed });
        }
        // Changed bytes outside any map, with some context to find them by.
        const CONTEXT: usize = 16;
        for (a, b) in src.changed_ranges() {
            if covered.iter().any(|&(s, e)| a < e && b > s) {
                continue;
            }
            let s = a.saturating_sub(CONTEXT);
            let e = (b + CONTEXT).min(r.len());
            let block: Vec<f64> = r[s..e].iter().map(|&x| x as f64).collect();
            let target = search::find_similar(dst.data(), &block, DataOrg::Byte, tol, 1, None).first().map(|h| h.0 + (a - s));
            out.push(ChangeItem { name: format!("Data area ({} bytes)", b - a), map: None, src: a, len: b - a, org: DataOrg::Byte, target, take_map: false, take_data: target.is_some() });
        }
        out
    }

    fn apply_changes(&mut self, source: u64, items: &[ChangeItem], absolute: bool) -> String {
        let Some(src) = self.project(source) else { return "The other project was closed.".into() };
        let (sd, sr) = (src.data().to_vec(), src.ref_data().to_vec());
        let Some(p) = self.current_project_mut() else { return String::new() };
        let (mut maps, mut cells) = (0, 0);
        p.begin_edit("Import changes");
        for it in items {
            let Some(t) = it.target else { continue };
            if it.take_map {
                if let Some(m) = &it.map {
                    let mut n = m.clone();
                    let delta = t as i64 - m.start as i64;
                    n.uid = p.uid();
                    n.start = t;
                    if n.x.byte_len(n.cols) > 0 {
                        n.x.addr = (n.x.addr as i64 + delta).max(0) as usize;
                    }
                    if n.y.byte_len(n.rows) > 0 {
                        n.y.addr = (n.y.addr as i64 + delta).max(0) as usize;
                    }
                    p.maps.push(n);
                    p.dirty_struct = true;
                    maps += 1;
                }
            }
            if it.take_data {
                let w = it.org.width();
                for i in 0..it.len / w {
                    let (a, b) = (it.src + i * w, t + i * w);
                    let (Some(v), Some(o)) = (it.org.read(&sd, a, false), it.org.read(&sr, a, false)) else { continue };
                    if v == o {
                        continue;
                    }
                    let new = if absolute { v } else { it.org.read(p.data(), b, false).unwrap_or(o) + (v - o) };
                    let bytes = it.org.encode(new, false);
                    p.write(b, &bytes);
                    cells += 1;
                }
            }
        }
        p.end_edit();
        format!("{maps} map(s) and {cells} changed value(s) were imported.")
    }

    #[allow(clippy::too_many_arguments)]
    fn parallel_dialog(&mut self, ctx: &egui::Context, tol: &mut f64, found: &mut Vec<(usize, bool)>, names: &mut bool, axes: &mut bool, sync: &mut bool, searched: &mut bool, keep: &mut bool) {
        let cur = self.active_child().and_then(|c| c.map_uid);
        let m = cur.and_then(|u| self.current_project().and_then(|p| p.maps.iter().find(|m| m.uid == u))).cloned();
        let Some(m) = m else {
            self.tell("Maps like this one", "Please open a map first.");
            *keep = false;
            return;
        };
        self.win("Maps like this one", 420.0).show(ctx, |ui| {
            ui.label(format!("Maps like \"{}\" ({}x{}):", m.name, m.cols, m.rows));
            let ch = ui.add(egui::Slider::new(tol, 0.0..=100.0).text("Tolerance %")).changed();
            if ch || !*searched {
                *searched = true;
                if let Some(p) = self.current_project() {
                    let w = m.org.width();
                    let block: Vec<f64> = (0..m.cells()).filter_map(|i| m.org.read(p.data(), m.start + i * w, false)).collect();
                    let mut hits = search::find_similar(p.data(), &block, m.org, *tol, 500, Some((m.start, m.end())));
                    hits.sort_by_key(|h| h.0);
                    // Copies that overlap each other are one place.
                    let mut kept: Vec<(usize, bool)> = vec![];
                    for (a, _) in hits {
                        if kept.last().is_none_or(|l| a >= l.0 + m.byte_len()) {
                            kept.push((a, true));
                        }
                    }
                    kept.truncate(100);
                    *found = kept;
                }
            }
            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                if found.is_empty() {
                    ui.label("Nothing was found with this tolerance.");
                }
                for (a, on) in found.iter_mut() {
                    ui.checkbox(on, format!("{a:06X}"));
                }
            });
            ui.checkbox(names, "Transfer map names");
            ui.checkbox(axes, "Transfer axis descriptions");
            ui.checkbox(sync, "Keep the values synchronised (sync blocks)");
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    let (n, a, s) = (*names, *axes, *sync);
                    let list: Vec<usize> = found.iter().filter(|x| x.1).map(|x| x.0).collect();
                    let mut src = m.clone();
                    let mut count = 0;
                    if let Some(p) = self.current_project_mut() {
                        let group = src.parallel_group.unwrap_or_else(|| p.uid());
                        src.parallel_group = Some(group);
                        if let Some(t) = p.maps.iter_mut().find(|t| t.uid == src.uid) {
                            t.parallel_group = Some(group);
                        }
                        for at in list {
                            let mut c = src.clone();
                            let delta = at as i64 - src.start as i64;
                            c.uid = p.uid();
                            c.start = at;
                            if !n {
                                c.name = format!("Map ({at:05X})");
                            }
                            if !a {
                                c.x.desc = "-".into();
                                c.x.unit = "-".into();
                                c.y.desc = "-".into();
                                c.y.unit = "-".into();
                            }
                            if c.x.byte_len(c.cols) > 0 {
                                c.x.addr = (c.x.addr as i64 + delta).max(0) as usize;
                            }
                            if c.y.byte_len(c.rows) > 0 {
                                c.y.addr = (c.y.addr as i64 + delta).max(0) as usize;
                            }
                            if s {
                                p.syncs.push(SyncBlock { a: src.start, b: at, len: src.byte_len() });
                            }
                            p.maps.push(c);
                            count += 1;
                        }
                        p.dirty_struct = true;
                    }
                    self.tell("Maps like this one", format!("{count} parallel map(s) created. Press f in the map to jump through them."));
                    *keep = false;
                }
                if ui.button("Cancel").clicked() {
                    *keep = false;
                }
            });
        });
    }

    fn config_dialog(&mut self, ctx: &egui::Context, tab: &mut usize, keep: &mut bool) {
        self.win("Settings", 560.0).show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (i, t) in ["Colours", "View", "Edit", "Map finder", "Folders", "Automatically", "Messages"].iter().enumerate() {
                    ui.selectable_value(tab, i, *t);
                }
            });
            ui.separator();
            let cfg = &mut self.cfg;
            match *tab {
                0 => {
                    ui.horizontal(|ui| {
                        ui.label("Colour theme:");
                        let mut pick: Option<Theme> = None;
                        ComboBox::from_id_salt("theme").selected_text(cfg.theme.name.clone()).show_ui(ui, |ui| {
                            for t in Theme::presets() {
                                if ui.selectable_label(cfg.theme.name == t.name, &t.name).clicked() {
                                    pick = Some(t);
                                }
                            }
                            if ui.selectable_label(cfg.theme.name == "User defined", "User defined").clicked() {
                                pick = Some(cfg.user_theme.clone());
                            }
                        });
                        if let Some(t) = pick {
                            cfg.theme = t;
                        }
                        if ui.button("Reset changes").clicked() {
                            if let Some(t) = Theme::presets().into_iter().find(|t| t.name == cfg.theme.name) {
                                cfg.theme = t;
                            }
                        }
                    });
                    let t = &mut cfg.theme;
                    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                        egui::Grid::new("cols").num_columns(4).show(ui, |ui| {
                            let row = |ui: &mut Ui, n: &str, a: &mut [u8; 3], b: Option<&mut [u8; 3]>| {
                                ui.label(n);
                                ui.color_edit_button_srgb(a);
                                if let Some(b) = b {
                                    ui.color_edit_button_srgb(b);
                                }
                                ui.end_row();
                            };
                            row(ui, "Hexdump (text, background)", &mut t.text, Some(&mut t.bg));
                            row(ui, "Map (text, background)", &mut t.map_text, Some(&mut t.map_bg));
                            row(ui, "Selection (text, background)", &mut t.sel_text, Some(&mut t.sel_bg));
                            row(ui, "... right & 2d", &mut t.right, None);
                            row(ui, "... 3d", &mut t.three_d, None);
                            row(ui, "Changed data", &mut t.changed, None);
                            row(ui, "... increased", &mut t.increased, None);
                            row(ui, "... decreased", &mut t.decreased, None);
                            row(ui, "Map in hexdump (text, background)", &mut t.map_in_hex_text, Some(&mut t.map_in_hex_bg));
                            row(ui, "Rowmarker", &mut t.rowmarker, None);
                            row(ui, "Suggested maps (border, tag)", &mut t.potential, Some(&mut t.potential_tag));
                            row(ui, "Comments (text, background)", &mut t.comment_text, Some(&mut t.comment_bg));
                            row(ui, "Map list (text, background)", &mut t.list_text, Some(&mut t.list_bg));
                        });
                    });
                    if cfg.theme.name == "User defined" {
                        cfg.user_theme = cfg.theme.clone();
                    }
                }
                1 => {
                    ui.label("New windows:");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut cfg.new_mode, ViewMode::Text, "Text");
                        ui.radio_value(&mut cfg.new_mode, ViewMode::TwoD, "2d");
                        ui.radio_value(&mut cfg.new_mode, ViewMode::ThreeD, "3d");
                    });
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut cfg.new_radix, 10, "10");
                        ui.radio_value(&mut cfg.new_radix, 16, "16");
                        ui.separator();
                        right_combo(ui, "cfgright", &mut cfg.new_right);
                    });
                    ui.separator();
                    ui.checkbox(&mut cfg.show_potential, "Show suggested maps in hexdumps");
                    ui.checkbox(&mut cfg.show_pale, "Show unimportant data (program code, empty areas) pale");
                    ui.checkbox(&mut cfg.row_groups, "Lines every 4 columns");
                    ui.checkbox(&mut cfg.drag_2d, "2d: dragging a selected value changes it");
                    ui.checkbox(&mut cfg.drag_3d, "3d: dragging a selected value changes it");
                }
                2 => {
                    ui.checkbox(&mut cfg.visible_columns_only, "Select only visible parts of columns");
                    ui.checkbox(&mut cfg.delete_sel_on_move, "Delete selection when moving the cursor");
                    ui.separator();
                    ui.label("Strength of the function value +/- 1:");
                    ui.horizontal(|ui| {
                        ui.label("8 Bit:");
                        ui.add(egui::DragValue::new(&mut cfg.step8).range(1..=255));
                        ui.label("16 Bit:");
                        ui.add(egui::DragValue::new(&mut cfg.step16).range(1..=65535));
                        ui.label("32 Bit:");
                        ui.add(egui::DragValue::new(&mut cfg.step32).range(1..=1000000));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Strength of Ctrl + PageUp / PageDown (bytes, hex):");
                        hex_edit(ui, &mut cfg.page_jump);
                    });
                    ui.separator();
                    ui.checkbox(&mut cfg.edit_shown_values, "Editing operations work with shown values (Text+2d)");
                    ui.checkbox(&mut cfg.number_keys_edit, "Number keys start in text mode the 'Inplace edit' mode");
                }
                3 => {
                    ui.label("Certain rules decide which maps are found and which are ignored.");
                    ui.label("Ignore maps that...");
                    ui.checkbox(&mut cfg.rules.ignore_constant, "consist of one constant value");
                    ui.checkbox(&mut cfg.rules.ignore_mostly_constant, "mostly consist of one constant value");
                    ui.checkbox(&mut cfg.rules.ignore_alternating, "consist of 2 alternating values");
                    ui.separator();
                    ui.checkbox(&mut cfg.rules.axis_maps, "Recognise maps by the axes that are stored with them");
                    ui.checkbox(&mut cfg.rules.statistical, "Use statistical map recognition (by the look, takes more time)");
                }
                4 => {
                    ui.label("Project folders, each with a name of its own.");
                    let mut del = None;
                    egui::Grid::new("cl").num_columns(3).show(ui, |ui| {
                        for (i, c) in cfg.clients.iter_mut().enumerate() {
                            ui.add(TextEdit::singleline(&mut c.name).desired_width(120.0));
                            ui.add(TextEdit::singleline(&mut c.path).desired_width(320.0));
                            if ui.small_button("✕").clicked() {
                                del = Some(i);
                            }
                            ui.end_row();
                        }
                    });
                    if let Some(i) = del {
                        if cfg.clients.len() > 1 {
                            cfg.clients.remove(i);
                            cfg.client = 0;
                        }
                    }
                    if ui.button("Add folder").clicked() {
                        let path = cfg.project_dir().to_string_lossy().to_string();
                        cfg.clients.push(Client { name: format!("Folder {}", cfg.clients.len() + 1), path });
                    }
                }
                5 => {
                    ui.checkbox(&mut cfg.search_potential, "Look for maps in the background");
                    ui.checkbox(&mut cfg.generate_overview, "Generate overview information");
                    ui.checkbox(&mut cfg.auto_names, "Automatically create internal filenames for project files");
                    ui.horizontal(|ui| {
                        ui.label("Namespace:");
                        ui.add(egui::DragValue::new(&mut cfg.namespace_from));
                        ui.label("-");
                        ui.add(egui::DragValue::new(&mut cfg.namespace_to));
                    });
                    ui.separator();
                    ui.label("When importing:");
                    ui.checkbox(&mut cfg.show_props_on_import, "Show project properties");
                    ui.checkbox(&mut cfg.vehicle_on_import, "Search vehicle data");
                }
                _ => {
                    ui.checkbox(&mut cfg.warn_big_selection, "Warn about very large selections");
                    ui.checkbox(&mut cfg.show_cs_status, "Show the checksum state in the status bar");
                    ui.checkbox(&mut cfg.tips, "Show tips");
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("   OK   ").clicked() {
                    self.cfg.save();
                    *keep = false;
                }
            });
        });
    }

    fn axis_profiles_dialog(&mut self, ctx: &egui::Context, name: &mut String, keep: &mut bool) {
        let cur = self.active_child().and_then(|c| c.map_uid);
        self.win("Axis presets", 520.0).show(ctx, |ui| {
            ui.label("Collections of axis descriptions (name, unit, factor, offset). Collect them from a map and apply them to others.");
            ui.horizontal(|ui| {
                ui.label("Profile:");
                ui.add(TextEdit::singleline(name).desired_width(180.0));
                if ui.add_enabled(cur.is_some(), egui::Button::new("Collect from the open map")).clicked() {
                    let n = if name.is_empty() { "Default".to_string() } else { name.clone() };
                    if let Some(p) = self.current_project_mut() {
                        if let Some(m) = cur.and_then(|u| p.maps.iter().find(|m| m.uid == u)).cloned() {
                            for ax in [m.x, m.y] {
                                p.profiles.push(AxisProfile { profile: n.clone(), desc: ax.desc, unit: ax.unit, factor: ax.factor, offset: ax.offset, precision: ax.precision, signed: ax.signed, signature: ax.signature });
                            }
                            p.dirty_struct = true;
                        }
                    }
                }
            });
            let list: Vec<AxisProfile> = self.current_project().map(|p| p.profiles.clone()).unwrap_or_default();
            let mut del = None;
            let mut apply: Option<(AxisProfile, bool)> = None;
            egui::ScrollArea::vertical().max_height(240.0).auto_shrink([false, true]).show(ui, |ui| {
                egui::Grid::new("ap").striped(true).num_columns(7).show(ui, |ui| {
                    for (i, a) in list.iter().enumerate() {
                        ui.label(&a.profile);
                        ui.label(&a.desc);
                        ui.label(&a.unit);
                        ui.label(format!("{} / {}", a.factor, a.offset));
                        if ui.add_enabled(cur.is_some(), egui::Button::new("To X")).clicked() {
                            apply = Some((a.clone(), true));
                        }
                        if ui.add_enabled(cur.is_some(), egui::Button::new("To Y")).clicked() {
                            apply = Some((a.clone(), false));
                        }
                        if ui.small_button("✕").clicked() {
                            del = Some(i);
                        }
                        ui.end_row();
                    }
                });
            });
            if let Some(i) = del {
                if let Some(p) = self.current_project_mut() {
                    p.profiles.remove(i);
                    p.dirty_struct = true;
                }
            }
            if let Some((a, is_x)) = apply {
                if let Some(p) = self.current_project_mut() {
                    if let Some(m) = cur.and_then(|u| p.maps.iter_mut().find(|m| m.uid == u)) {
                        let t = if is_x { &mut m.x } else { &mut m.y };
                        t.desc = a.desc;
                        t.unit = a.unit;
                        t.factor = a.factor;
                        t.offset = a.offset;
                        t.precision = a.precision;
                        t.signed = a.signed;
                    }
                    p.dirty_struct = true;
                }
            }
            if ui.button("Close").clicked() {
                *keep = false;
            }
        });
    }
}

fn hex_edit(ui: &mut Ui, v: &mut usize) {
    let mut t = format!("{v:X}");
    if ui.add(TextEdit::singleline(&mut t).desired_width(90.0)).changed() {
        *v = usize::from_str_radix(t.trim(), 16).unwrap_or(0);
    }
}

fn xu_values(s: &str) -> Vec<f64> {
    s.split_whitespace().filter_map(|t| t.replace(',', ".").parse().ok()).collect()
}

fn fmt_signed(v: i64) -> String {
    if v < 0 {
        format!("-{:X}", -v)
    } else {
        format!("{v:X}")
    }
}

fn parse_signed(s: &str) -> i64 {
    let t = s.trim();
    let (neg, t) = t.strip_prefix('-').map(|r| (true, r)).unwrap_or((false, t));
    let v = i64::from_str_radix(t, 16).unwrap_or(0);
    if neg {
        -v
    } else {
        v
    }
}

/// The language of the program's texts.
const LANGUAGE_NAME: &str = "English (UK)";

const TIPS: [&str; 8] = [
    "Press f to jump to the next suggested map, and Shift+F to go back.",
    "t and Shift+T step through numbers, the 2D graph and the 3D graph.",
    "m and w change how many columns a row has. A table shows itself when its rows line up.",
    "Ctrl+B fits the bars and graphs to the selected values.",
    "Tab hides and shows every floating window at once.",
    "Drag a selected value in the 2D or 3D graph to change it.",
    "Press p for a 3D preview of a rectangular selection, or of a map chosen in the list.",
    "Drop a second file on a project window to add it as a new version.",
];

fn help_text(topic: &str) -> (&'static str, &'static str) {
    match topic {
        "keys" => ("Keys", "Moving\n  Arrows and Page keys move the cursor. With Shift they select on the way.\n  Ctrl+Up/Down scrolls. Ctrl+Left/Right moves the start address.\n  Ctrl+Home/End goes to the start and the end. Ctrl+PageUp/Down jumps a set distance.\n\nViews\n  t and Shift+T: previous and next view.  m and w: more and fewer columns.\n  # : next bit width.  z: reset the view.  o: file overview.  p: 3D preview.  Tab: floating windows.\n\nValues\n  + and - : one step.  = : set a value.  % : add or scale.  / : blend.  F11: original value.  F9: repeat.\n  Enter or a number key: type a new value.\n\nMaps and differences\n  f and Shift+F: next and previous suggested map.  k: make a map from the selection.  Shift+K: open the map under the cursor.\n  u: list of differences.  e, v, n, l: first, previous, next and last difference.\n\nFinding\n  s or Ctrl+F: find.  F3: next.  Shift+F3: previous.  a or Ctrl+G: go to an address.\n  F5: next bookmark.  Ctrl+F5: set or clear a bookmark.  Shift+Enter: comment."),
        "mouse" => ("Mouse", "Numbers: drag the double line beside the addresses to scroll, and the single line before the bars to change the number of columns. The small dot keeps the bars in view.\n\n2D graph: drag the scales to scroll and stretch. Drag a selected value to change it.\n\n3D graph: drag to turn it and change the perspective, with Ctrl to stretch the height. The wheel stretches the height too. Drag a selected value to change it."),
        "menus" => ("Right-click menus", "The right mouse button opens a menu wherever you are: in the data for editing and moving about, in the bars for what is shown beside the numbers, on the tag of a suggested map to keep or drop it, and in the map list to open, copy, hide or delete maps."),
        "drop" => ("Dropping files", "Drop a file on the program to start a new project, or on a project window to add a new version. Two files dropped together are taken as the two halves of 16-bit data.\n\nThese are understood: binary files, Intel hex and Motorola S-records, zip archives, ALS projects, map packs and selections, TunerPro definitions (XDF) and A2L description files."),
        "selections" => ("Selecting", "A selection can be a run of cells, a rectangle, whole columns or whole rows, and the shape can be changed afterwards. Ctrl adds another area, Delete clears the selection, and a click on an address selects its row.\n\nSelections can be saved and loaded, searched for, and turned into a map with k."),
        _ => ("Getting started", "Open a file with File, New project, or try the sample data. The map finder looks for tables in the background: press f to step through what it suggests, or look in the list on the left.\n\nEvery version is compared with the first one. Changed values are marked in colour, in the bars and in the file overview. The tabs at the bottom of a window switch between numbers, a 2D graph and a 3D graph.\n\nDouble-click the tag of a suggested map to keep it. Select cells to change many at once. The other help pages list the keys and what the mouse does.\n\nTest changed data with care: wrong values can damage an engine, and changes to emission control may be against the law where you live."),
    }
}
