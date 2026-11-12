//! What each command does, the keyboard, dropped files and the frame loop.
use crate::analysis;
use crate::app::{App, DiffMove};
use crate::checksum;
use crate::cmd::Cmd;
use crate::dialogs::Modal;
use crate::formats;
use crate::model::*;
use crate::shell::{self, RowKind};
use crate::view::{Sel, SelMode};
use egui::{Key, Modifiers};
use std::sync::atomic::Ordering;

impl App {
    pub fn run_cmd(&mut self, cmd: Cmd, ctx: &egui::Context) {
        match cmd {
            Cmd::NewProject => self.modal = Some(Modal::NewProject),
            Cmd::SampleProject => {
                let p = sample_project(self.new_project_id());
                self.adopt_project(p, ctx, true);
            }
            Cmd::OpenProject => self.modal = Some(Modal::open_project(&self.cfg)),
            Cmd::OpenVersion => self.modal = Some(Modal::Versions { sel: self.current_project().map(|p| p.current).unwrap_or(0), rename: String::new() }),
            Cmd::CloseProject => {
                if let Some(id) = self.active_project {
                    self.request_close(id, false);
                }
            }
            Cmd::CloseDeleteProject => {
                if let Some(id) = self.active_project {
                    self.modal = Some(Modal::ConfirmDelete(id));
                }
            }
            Cmd::Save => {
                if let Some(id) = self.active_project {
                    self.save_project(id);
                }
            }
            Cmd::SaveAll => {
                let ids: Vec<u64> = self.projects.iter().filter(|p| p.dirty_data || p.dirty_struct).map(|p| p.id).collect();
                for id in ids {
                    self.save_project(id);
                }
            }
            Cmd::PropsProject => {
                if let Some(p) = self.current_project() {
                    self.modal = Some(Modal::ProjectProps(Box::new(p.props.clone())));
                }
            }
            Cmd::PropsVersion => {
                if let Some(p) = self.current_project() {
                    if let Some(v) = p.versions.get(p.current) {
                        self.modal = Some(Modal::VersionProps { idx: p.current, name: v.name.clone(), comment: v.comment.clone(), folder: v.folder.clone() });
                    }
                }
            }
            Cmd::SearchSimilar => self.search_similar_projects(),
            Cmd::SearchDouble => self.search_double_versions(),
            Cmd::UpdateAll => self.update_all_projects(),
            Cmd::ExportFile => self.modal = Some(Modal::export_opts(self.current_project())),
            Cmd::ImportFile => self.modal = Some(Modal::import_opts()),
            Cmd::SendMail => self.send_mail(),
            Cmd::ImportDir => self.import_dir(ctx),
            Cmd::ExportCsv => {
                if let (Some(p), Some(path)) = (self.current_project(), rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("maps.csv").save_file()) {
                    let text = formats::maps_csv(p);
                    self.write_text(&path, &text);
                }
            }
            Cmd::UpdateCsv => {
                if let Some(path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).pick_file() {
                    match std::fs::read_to_string(&path) {
                        Ok(t) => {
                            let n = self.current_project_mut().map(|p| formats::update_maps_from_csv(p, &t)).unwrap_or(0);
                            self.tell("Map names from CSV", format!("{n} map(s) updated."));
                        }
                        Err(e) => self.tell("Map names from CSV", e.to_string()),
                    }
                }
            }
            Cmd::ExportPack => {
                if let (Some(p), Some(path)) = (self.current_project(), rfd::FileDialog::new().add_filter("Map pack", &["alsmap"]).set_file_name("maps.alsmap").save_file()) {
                    match serde_json::to_vec_pretty(&p.maps) {
                        Ok(b) => {
                            if let Err(e) = std::fs::write(&path, b) {
                                self.tell("Export map pack", e.to_string());
                            }
                        }
                        Err(e) => self.tell("Export map pack", e.to_string()),
                    }
                }
            }
            Cmd::ImportPack => {
                if let Some(path) = rfd::FileDialog::new().add_filter("Map pack", &["alsmap"]).pick_file() {
                    self.import_path(&path, self.active_project, ctx);
                }
            }
            Cmd::ExportXdf => self.export_xdf(),
            Cmd::ImportXdf => {
                if let Some(path) = rfd::FileDialog::new().add_filter("TunerPro definition", &["xdf"]).pick_file() {
                    self.import_path(&path, self.active_project, ctx);
                }
            }
            Cmd::SendTunerPro => self.send_tunerpro(),
            Cmd::ImportMaps => self.modal = Some(Modal::ImportMaps { source: None }),
            Cmd::ImportChanges => self.modal = Some(Modal::import_changes()),
            Cmd::ImportA2l => {
                if let Some(path) = rfd::FileDialog::new().add_filter("ASAP2", &["a2l"]).pick_file() {
                    self.import_path(&path, self.active_project, ctx);
                }
            }
            Cmd::Print => self.print(),
            Cmd::Exit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Cmd::Recent(i) => {
                if let Some(r) = self.cfg.recent.get(i).cloned() {
                    self.import_path(std::path::Path::new(&r), None, ctx);
                }
            }
            Cmd::Undo => {
                if let Some(p) = self.current_project_mut() {
                    p.undo_one();
                }
            }
            Cmd::UndoMore => self.modal = Some(Modal::UndoMore),
            Cmd::Redo => {
                if let Some(p) = self.current_project_mut() {
                    p.redo_one();
                }
            }
            Cmd::Copy => {
                if let Some(clip) = self.clip_from_selection() {
                    ctx.copy_text(clip.text.clone());
                    self.clip = Some(clip);
                }
            }
            Cmd::CopyAddress => {
                if let Some(c) = self.active_child() {
                    ctx.copy_text(format!("{:X}", c.g.addr(c.g.cursor)));
                }
            }
            Cmd::CopyMap => self.copy_map_part(ctx, true, true, true),
            Cmd::CopyBothAxes(_) => self.copy_map_part(ctx, true, true, false),
            Cmd::CopyX(_) => self.copy_map_part(ctx, true, false, false),
            Cmd::CopyY(_) => self.copy_map_part(ctx, false, true, false),
            Cmd::CopyCArray => {
                if let Some(c) = self.clip_from_selection() {
                    let t = c_array(&c.cells, c.cols, c.org);
                    ctx.copy_text(t);
                }
            }
            Cmd::CopyText => {
                if let Some(c) = self.clip_from_selection() {
                    ctx.copy_text(c.text);
                }
            }
            Cmd::Cut => {
                if let Some(clip) = self.clip_from_selection() {
                    ctx.copy_text(clip.text.clone());
                    self.clip = Some(clip);
                    self.change_cells("Cut", None, &|_, _, _, _| 0.0);
                }
            }
            Cmd::Paste => self.paste(ctx),
            Cmd::InsertHexdump => {
                if let Some(id) = self.active_project {
                    self.open_hexdump(id);
                }
            }
            Cmd::InsertMap => self.insert_new_map(),
            Cmd::CloseAllMaps => {
                let dead: Vec<u64> = self.children.iter().filter(|c| c.map_uid.is_some()).map(|c| c.id).collect();
                for d in dead {
                    self.close_child(d);
                }
            }
            Cmd::DeleteWindow => {
                if let Some(id) = self.active {
                    self.close_child(id);
                }
            }
            Cmd::OpenMapAtCursor => {
                if let Some(c) = self.active_child() {
                    let (pid, a) = (c.project, c.g.addr(c.g.cursor));
                    if let Some(uid) = self.project(pid).and_then(|p| p.map_at(a)).map(|m| m.uid) {
                        self.open_map(pid, uid);
                    } else {
                        self.tell("Open map", "There is no map at the cursor.");
                    }
                }
            }
            Cmd::DeleteMap => self.delete_map_at_cursor(),
            Cmd::AsText => self.modal = Some(self.as_text_modal()),
            Cmd::Inplace => {
                if self.active_child().is_some_and(|c| c.g.mode == ViewMode::Text) {
                    self.inplace();
                } else {
                    self.pending.push(Cmd::ChangeAbs);
                }
            }
            Cmd::Plus => self.step_value(1),
            Cmd::Minus => self.step_value(-1),
            Cmd::ChangeAbs => {
                if let Some(c) = self.active_child() {
                    if let Some(p) = self.project(c.project) {
                        let v = c.g.raw(p, c.g.cursor).unwrap_or(0.0);
                        self.modal = Some(Modal::ChangeAbs { shown: format!("{}", c.g.scaled(v)), hex: format!("{:X}", v as i64) });
                    }
                }
            }
            Cmd::ChangeRel => {
                self.modal = Some(Modal::ChangeRel { from_original: false, percent: true, w: "0".into() });
            }
            Cmd::ChangeEdit => {
                if self.active_child().is_some_and(|c| c.g.has_selection()) {
                    self.modal = Some(Modal::change_edit());
                } else {
                    self.tell("Blend across the selection", "This command is only available when an area is selected.");
                }
            }
            Cmd::OriginalValue => self.restore_original(),
            Cmd::Again => {
                if let Some(ch) = self.last_change.clone() {
                    self.apply_change(&ch);
                }
            }
            Cmd::Checksums => self.modal = Some(Modal::Checksums { sel: None }),
            Cmd::ApplyChecksum => self.apply_checksum_at_cursor(),
            Cmd::ApplyAllChecksums => {
                let ok = self.current_project_mut().map(checksum::correct_all).unwrap_or(true);
                if !ok {
                    self.tell("Checksums", "Some checksums or mirrored blocks could not be corrected.");
                }
            }
            Cmd::PropsWindow => self.open_window_props(),
            Cmd::SwapLines => self.modal = Some(Modal::swap_lines(self.swap.clone(), self.current_project().map(|p| p.size()).unwrap_or(0x10000))),
            Cmd::OriginLeft | Cmd::OriginRight => {
                let start = self.current_project().map(|p| p.element_range().0).unwrap_or(0);
                if let Some(c) = self.active_child_mut() {
                    if c.map_uid.is_none() {
                        let addr = c.g.addr(c.g.cursor);
                        if cmd == Cmd::OriginRight {
                            c.g.base += 1;
                        } else if c.g.base > start {
                            c.g.base -= 1;
                        }
                        c.g.cursor = c.g.cell_of(addr);
                    }
                }
            }
            Cmd::MoreCols | Cmd::LessCols => {
                if let Some(c) = self.active_child_mut() {
                    if c.map_uid.is_none() {
                        let top = c.g.top_row * c.g.cols;
                        c.g.cols = if cmd == Cmd::MoreCols { (c.g.cols + 1).min(4096) } else { c.g.cols.saturating_sub(1).max(1) };
                        c.g.top_row = top / c.g.cols;
                    }
                }
            }
            Cmd::DefaultView => {
                let fresh = self.new_grid();
                if let Some(c) = self.active_child_mut() {
                    c.g.percent = false;
                    c.g.delta = false;
                    c.g.no_factor = false;
                    c.g.show_orig = false;
                    c.g.mode = fresh.mode;
                    c.g.font = fresh.font;
                    if c.map_uid.is_none() {
                        c.g.cols = fresh.cols;
                        c.g.org = fresh.org;
                        c.g.radix = fresh.radix;
                        c.g.right = fresh.right;
                    }
                }
            }
            Cmd::Percent => self.with_grid(|g| {
                g.percent = !g.percent;
                g.delta = false;
            }),
            Cmd::Delta => self.with_grid(|g| {
                g.delta = !g.delta;
                g.percent = false;
            }),
            Cmd::NoFactor => self.with_grid(|g| g.no_factor = !g.no_factor),
            Cmd::OrigValues => self.with_grid(|g| g.show_orig = !g.show_orig),
            Cmd::FixRight => self.with_grid(|g| g.fix_right = !g.fix_right),
            Cmd::Right(r) => self.with_grid(|g| g.right = if g.right == r { RightSide::Nothing } else { r }),
            Cmd::Zoom => self.with_grid(|g| match g.mode {
                ViewMode::Text => g.font = (g.font + 1.0).min(28.0),
                ViewMode::TwoD => g.zoom_x = (g.zoom_x * 1.25).min(64.0),
                ViewMode::ThreeD => g.zscale = (g.zscale * 1.1).min(8.0),
            }),
            Cmd::Unzoom => self.with_grid(|g| match g.mode {
                ViewMode::Text => g.font = (g.font - 1.0).max(6.0),
                ViewMode::TwoD => g.zoom_x = (g.zoom_x * 0.8).max(0.05),
                ViewMode::ThreeD => g.zscale = (g.zscale * 0.9).max(0.05),
            }),
            Cmd::Scale1 => self.with_grid(|g| {
                g.zoom_x = 1.0;
                g.zoom_y = 1.0;
            }),
            Cmd::ConnectDots => self.with_grid(|g| {
                if g.mode == ViewMode::ThreeD {
                    g.fill = !g.fill;
                } else {
                    g.dots = !g.dots;
                }
            }),
            Cmd::RowMarkers => self.with_grid(|g| g.row_markers = !g.row_markers),
            Cmd::Differences => self.show_diff = !self.show_diff,
            Cmd::DiffFirst => self.next_diff(DiffMove::First),
            Cmd::DiffPrev => self.next_diff(DiffMove::Prev),
            Cmd::DiffNext => self.next_diff(DiffMove::Next),
            Cmd::DiffLast => self.next_diff(DiffMove::Last),
            Cmd::ChooseRef => self.modal = Some(Modal::choose_ref(self.current_project())),
            Cmd::ConnectWindows => self.modal = Some(Modal::connect()),
            Cmd::OptimizeRange => self.optimize_range(),
            Cmd::DetectMap(back) => self.detect_map(back),
            Cmd::SupportSel => self.support_selection(),
            Cmd::Mode(m) => self.with_grid(|g| g.mode = m),
            Cmd::ModePrev => self.with_grid(|g| {
                g.mode = match g.mode {
                    ViewMode::ThreeD => ViewMode::TwoD,
                    _ => ViewMode::Text,
                }
            }),
            Cmd::ModeNext => self.with_grid(|g| {
                g.mode = match g.mode {
                    ViewMode::Text => ViewMode::TwoD,
                    _ => ViewMode::ThreeD,
                }
            }),
            Cmd::Radix(r) => self.with_grid(|g| g.radix = r),
            Cmd::Bits(b) => self.with_grid(|g| {
                let addr = g.addr(g.cursor);
                let top = g.addr(g.top_row * g.cols);
                let be = g.org.big_endian();
                g.org = match (b, be) {
                    (8, _) => DataOrg::Byte,
                    (16, false) => DataOrg::LoHi,
                    (16, true) => DataOrg::HiLo,
                    (32, false) => DataOrg::LoHiLoHi,
                    (32, true) => DataOrg::HiLoHiLo,
                    (_, false) => DataOrg::FloatLoHi,
                    (_, true) => DataOrg::FloatHiLo,
                };
                let (lo, hi) = g.org.limits(g.signed);
                g.range_lo = lo.max(0.0);
                g.range_hi = if g.org.is_float() { 1000.0 } else { hi };
                if g.rows_fixed.is_none() {
                    g.cursor = g.cell_of(addr);
                    g.top_row = g.cell_of(top) / g.cols.max(1);
                    g.sel = None;
                }
            }),
            Cmd::SwapEndian => self.with_grid(|g| g.org = g.org.swapped()),
            Cmd::Signed => self.with_grid(|g| g.signed = !g.signed),
            Cmd::ToggleToolbarView => self.cfg.toolbar_view = !self.cfg.toolbar_view,
            Cmd::ToggleStatus => self.cfg.status_bar = !self.cfg.status_bar,
            Cmd::Colours(c) => self.cfg.colours = c,
            Cmd::HeightColors => self.cfg.height_colors = !self.cfg.height_colors,
            Cmd::SelCell => self.with_grid(|g| {
                let c = g.cursor;
                if let Some(s) = g.sel {
                    g.extra.push(s);
                }
                g.sel = Some(Sel::new(c, c));
                g.cursor = c + 1;
                g.follow = true;
            }),
            Cmd::SelRow => self.with_grid(|g| {
                g.sel_mode = SelMode::Rows;
                g.sel = Some(Sel::new(g.cursor, g.cursor));
            }),
            Cmd::SelCol => self.with_grid(|g| {
                g.sel_mode = SelMode::Columns;
                g.sel = Some(Sel::new(g.cursor, g.cursor));
            }),
            Cmd::SelScreen => self.with_grid(|g| {
                let a = g.top_row * g.cols;
                let b = (g.top_row + g.visible_rows.max(1)) * g.cols - 1;
                g.sel_mode = SelMode::Consecutive;
                g.sel = Some(Sel::new(a, b));
            }),
            Cmd::SelInvert => self.with_grid(|g| {
                let a = g.top_row * g.cols;
                let b = (g.top_row + g.visible_rows.max(1)) * g.cols;
                let was: Vec<usize> = (a..b).filter(|&i| !g.is_selected(i)).collect();
                g.clear_sel();
                g.sel_mode = SelMode::Consecutive;
                let mut runs: Vec<Sel> = vec![];
                for i in was {
                    match runs.last_mut() {
                        Some(r) if r.head + 1 == i => r.head = i,
                        _ => runs.push(Sel::new(i, i)),
                    }
                }
                if let Some(first) = runs.pop() {
                    g.sel = Some(first);
                    g.extra = runs;
                }
            }),
            Cmd::SelCreate => self.modal = Some(Modal::CreateSel { from: String::new(), to: String::new(), mode: SelMode::Consecutive }),
            Cmd::SelNone => self.with_grid(|g| g.clear_sel()),
            Cmd::SelRestore => self.with_grid(|g| {
                if let Some(s) = g.last_sel {
                    g.sel = Some(s);
                }
            }),
            Cmd::SelToMap => {
                let shift = ctx.input(|i| i.modifiers.shift);
                self.selection_to_map(!shift);
            }
            Cmd::SelLoad => {
                if let Some(path) = rfd::FileDialog::new().add_filter("Selection", &["alssel"]).pick_file() {
                    self.import_path(&path, self.active_project, ctx);
                }
            }
            Cmd::SelSave => self.save_selection(),
            Cmd::SelModeSet(m) => self.with_grid(|g| g.sel_mode = m),
            Cmd::SelBegin => self.with_grid(|g| {
                g.sel_begin = Some(g.cursor);
                g.sel = None;
            }),
            Cmd::SelEnd => self.with_grid(|g| {
                if let Some(b) = g.sel_begin {
                    g.sel = Some(Sel::new(b, g.cursor));
                }
            }),
            Cmd::SearchBytes => {
                self.find.open = true;
                self.find.similar = None;
                self.floats_hidden = false;
            }
            Cmd::SearchSel => {
                if let Some(c) = self.clip_from_selection() {
                    self.find.similar = Some(c.cells);
                    self.find.open = true;
                    self.floats_hidden = false;
                    self.find.status = "Searching for areas like the selection.".into();
                }
            }
            Cmd::SearchStart => self.find_step(true, true),
            Cmd::SearchNext => self.find_step(true, false),
            Cmd::SearchBack => self.find_step(false, false),
            Cmd::Replace => self.modal = Some(Modal::replace()),
            Cmd::MapSearchToggle => {
                if let Some(id) = self.active_project {
                    if let Some(h) = self.searches.get(&id) {
                        h.cancel.store(true, Ordering::Relaxed);
                    } else {
                        self.start_search(id, ctx);
                    }
                }
            }
            Cmd::MapSearchConfig => self.modal = Some(Modal::Config { tab: 3 }),
            Cmd::AxisProfiles => self.modal = Some(Modal::AxisProfiles { name: String::new() }),
            Cmd::ParallelMaps => self.modal = Some(Modal::parallel()),
            Cmd::GoTo => {
                let cur = self.active_child().map(|c| format!("{:X}", c.g.addr(c.g.cursor))).unwrap_or_default();
                self.modal = Some(Modal::GoTo { text: cur, binary_and: false, error: String::new() });
            }
            Cmd::CommentPrev | Cmd::CommentNext => {
                let next = cmd == Cmd::CommentNext;
                let t = self.active_child().and_then(|c| {
                    let a = c.g.addr(c.g.cursor);
                    let p = self.project(c.project)?;
                    let mut v: Vec<usize> = p.comments.iter().map(|m| m.addr).collect();
                    v.sort_unstable();
                    if next {
                        v.into_iter().find(|&x| x > a)
                    } else {
                        v.into_iter().rev().find(|&x| x < a)
                    }
                });
                if let Some(a) = t {
                    self.goto_addr(a);
                }
            }
            Cmd::CommentEdit => self.modal = Some(self.comment_modal()),
            Cmd::CommentDel => {
                if let Some(a) = self.active_child().map(|c| c.g.addr(c.g.cursor)) {
                    if let Some(p) = self.current_project_mut() {
                        p.comments.retain(|m| !(a >= m.addr && a <= m.end.max(m.addr)));
                        p.dirty_struct = true;
                    }
                }
            }
            Cmd::MarkerPrev | Cmd::MarkerNext => {
                let next = cmd == Cmd::MarkerNext;
                let t = self.active_child().and_then(|c| {
                    let a = c.g.addr(c.g.cursor);
                    let p = self.project(c.project)?;
                    if next {
                        p.markers.iter().copied().find(|&x| x > a)
                    } else {
                        p.markers.iter().rev().copied().find(|&x| x < a)
                    }
                });
                if let Some(a) = t {
                    self.goto_addr(a);
                }
            }
            Cmd::MarkerToggle => {
                if let Some(a) = self.active_child().map(|c| c.g.addr(c.g.cursor)) {
                    if let Some(p) = self.current_project_mut() {
                        p.toggle_marker(a);
                    }
                }
            }
            Cmd::Rescue => {
                if let Some(path) = rfd::FileDialog::new().add_filter("Project backups", &["bak"]).set_directory(self.cfg.project_dir()).pick_file() {
                    let id = self.new_project_id();
                    match Project::load(&path, id) {
                        Ok(mut p) => {
                            p.path = None;
                            p.dirty_data = true;
                            self.adopt_project(p, ctx, false);
                        }
                        Err(e) => self.tell("Recover a project", e),
                    }
                }
            }
            Cmd::Config => self.modal = Some(Modal::Config { tab: 0 }),
            Cmd::Calc => {
                if let Err(e) = std::process::Command::new("calc.exe").spawn() {
                    self.tell("Calculator", e.to_string());
                }
            }
            Cmd::MapSelection => self.list.visible = !self.list.visible,
            Cmd::Overview => {
                self.show_overview = !self.show_overview;
                self.floats_hidden = false;
                if self.show_overview {
                    if let Some(p) = self.current_project_mut() {
                        if p.overview.is_none() {
                            p.overview = Some(analysis::build_overview(p.data(), &p.maps, &p.potential));
                        }
                    }
                }
            }
            Cmd::Preview => {
                self.show_preview = !self.show_preview;
                self.floats_hidden = false;
            }
            Cmd::ProjectComment => {
                if let Some(p) = self.current_project() {
                    self.modal = Some(Modal::ProjectComment { text: p.props.comment.clone() });
                }
            }
            Cmd::ToggleFloat => self.floats_hidden = !self.floats_hidden,
            Cmd::NewWindow => {
                if let Some(c) = self.active_child().cloned() {
                    let id = self.next_id;
                    self.next_id += 1;
                    let mut n = c;
                    n.id = id;
                    n.rect = n.rect.translate(egui::Vec2::splat(24.0));
                    n.connected = None;
                    self.children.push(n);
                    self.active = Some(id);
                } else if let Some(id) = self.active_project {
                    self.open_hexdump(id);
                }
            }
            Cmd::Cascade => self.cascade(),
            Cmd::Tile => self.tile(),
            Cmd::Activate(id) => {
                self.active = Some(id);
                if let Some(c) = self.children.iter_mut().find(|c| c.id == id) {
                    c.minimized = false;
                    let pr = c.project;
                    self.active_project = Some(pr);
                }
                let layer = egui::LayerId::new(egui::Order::Middle, egui::Id::unique(("child", id)));
                ctx.move_to_top(layer);
            }
            Cmd::Help(topic) => self.modal = Some(Modal::Help(topic)),
            Cmd::Tip => {
                self.cfg.tip_index += 1;
                self.modal = Some(Modal::Tip);
            }
            Cmd::About => self.modal = Some(Modal::About),
            Cmd::SelectElement(e) => {
                let mut start = 0;
                if let Some(p) = self.current_project_mut() {
                    p.active_element = e;
                    start = p.element_range().0;
                }
                let id = self.active_project;
                for c in self.children.iter_mut().filter(|c| Some(c.project) == id && c.map_uid.is_none()) {
                    c.g.base = start;
                    c.g.cursor = 0;
                    c.g.top_row = 0;
                    c.g.sel = None;
                }
            }
            Cmd::PotentialFit(i) => self.fit_potential(i),
            Cmd::PotentialToMap(i) => {
                self.potential_to_map(i);
            }
            Cmd::PotentialRemove(i) => {
                if let Some(p) = self.current_project_mut() {
                    if i < p.potential.len() {
                        p.potential.remove(i);
                    }
                }
            }
            Cmd::DragValues(d) => self.drag_values(d),
            Cmd::ListJump(pid, uid, kind) => self.list_jump(pid, uid, kind),
            Cmd::ListOpen(pid, uid, kind) => self.list_open(pid, uid, kind),
            Cmd::ListRemove(pid, uid, kind) => {
                if let Some(p) = self.project_mut(pid) {
                    match kind {
                        RowKind::Map => {
                            p.maps.retain(|m| m.uid != uid);
                            p.dirty_struct = true;
                        }
                        RowKind::Potential => p.potential.retain(|m| m.uid != uid),
                        RowKind::Hexdump => {}
                    }
                }
                let dead: Vec<u64> = self.children.iter().filter(|c| c.project == pid && c.map_uid == Some(uid)).map(|c| c.id).collect();
                for d in dead {
                    self.close_child(d);
                }
            }
            Cmd::ListDuplicate(pid, uid) => {
                if let Some(p) = self.project_mut(pid) {
                    if let Some(mut m) = p.maps.iter().find(|m| m.uid == uid).cloned() {
                        m.uid = p.uid();
                        m.name = format!("{} (copy)", m.name);
                        p.maps.push(m);
                        p.dirty_struct = true;
                    }
                }
            }
            Cmd::ListHide(pid, uid) => {
                if let Some(p) = self.project_mut(pid) {
                    if let Some(m) = p.maps.iter_mut().find(|m| m.uid == uid) {
                        m.hidden = !m.hidden;
                    }
                    p.dirty_struct = true;
                }
            }
        }
    }

    pub fn with_grid(&mut self, f: impl FnOnce(&mut crate::view::Grid)) {
        if let Some(c) = self.active_child_mut() {
            f(&mut c.g);
        }
    }

    pub fn request_close(&mut self, id: u64, exit: bool) {
        let dirty = self.project(id).is_some_and(|p| p.dirty_data || p.dirty_struct);
        if dirty {
            self.modal = Some(Modal::SaveChanges { pid: id, exit });
        } else {
            self.close_project_now(id);
        }
    }

    fn write_text(&mut self, path: &std::path::Path, text: &str) {
        if let Err(e) = std::fs::write(path, text) {
            self.tell("Export", e.to_string());
        }
    }

    /// Asks where a definition should go. It starts next to the definition TunerPro has open
    /// (under a name of its own, so that file stays as it is), or else next to the bin.
    fn pick_xdf_target(&self) -> Option<std::path::PathBuf> {
        let stem = |p: &std::path::Path| p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "maps".into());
        let link = &self.tunerpro;
        let (near, name) = match (&link.xdf, &link.bin) {
            (Some(x), _) if stem(x).ends_with("-Advanced") => (Some(x), format!("{}.xdf", stem(x))),
            (Some(x), _) => (Some(x), format!("{}-Advanced.xdf", stem(x))),
            (None, Some(b)) => (Some(b), format!("{}.xdf", stem(b))),
            (None, None) => (None, "maps.xdf".to_string()),
        };
        let mut dialog = rfd::FileDialog::new().add_filter("TunerPro definition", &["xdf"]).set_file_name(&name);
        if let Some(dir) = near.and_then(|p| p.parent()).filter(|d| d.is_dir()) {
            dialog = dialog.set_directory(dir);
        }
        dialog.save_file()
    }

    fn export_xdf(&mut self) {
        if self.current_project().is_none_or(|p| p.maps.is_empty()) {
            self.tell("Export XDF", "This project has no maps yet. Select the values of a table and press k to make a map of them first.");
            return;
        }
        let Some(path) = self.pick_xdf_target() else { return };
        let Some(p) = self.current_project() else { return };
        let (text, n) = (crate::xdf::write_xdf(&p.maps, p.data()), p.maps.len());
        match std::fs::write(&path, text) {
            Ok(()) => self.tell("Export XDF", format!("{n} map(s) written to {}.", path.display())),
            Err(e) => self.tell("Export XDF", format!("The definition couldn't be written: {e}")),
        }
    }

    /// Writes the maps as a definition and has TunerPro open it. When TunerPro already had a
    /// definition open, the new maps go into a copy of it, so nothing in it is lost.
    fn send_tunerpro(&mut self) {
        let title = "Send to TunerPro";
        if self.current_project().is_none_or(|p| p.maps.is_empty()) {
            self.tell(title, "There are no maps to send yet. Select the values of a table and press k to make a map of them first.");
            return;
        }
        let Some(path) = self.pick_xdf_target() else { return };
        let base = self.tunerpro.xdf.clone().filter(|x| x.is_file());
        let Some(p) = self.current_project() else { return };
        let fresh = || (crate::xdf::write_xdf(&p.maps, p.data()).into_bytes(), format!("{} map(s) written as a new definition.", p.maps.len()));
        let merged = base.as_ref().map(|b| std::fs::read(b).map_err(|e| e.to_string()).and_then(|old| crate::xdf::merge_xdf(&old, &p.maps, p.data())));
        let (bytes, mut note) = match merged {
            Some(Ok(m)) => {
                let from = base.as_ref().and_then(|b| b.file_name()).map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                let mut note = format!("{} new map(s) added to a copy of {from}; {} were already in it.", m.added, m.kept);
                if m.outside > 0 {
                    note.push_str(&format!(" {} lie outside its address range and were left out.", m.outside));
                }
                (m.bytes, note)
            }
            Some(Err(e)) => {
                let (bytes, note) = fresh();
                (bytes, format!("{note} The definition TunerPro has open couldn't be extended: {e}"))
            }
            None => fresh(),
        };
        if let Err(e) = std::fs::write(&path, bytes) {
            self.tell(title, format!("The definition couldn't be written: {e}"));
            return;
        }
        match self.tunerpro.load_xdf(&path) {
            Ok(()) => {
                note.push_str(" TunerPro is opening it now.");
                // The next send builds on this file.
                self.tunerpro.xdf = Some(path);
            }
            Err(e) => note.push_str(&format!(" {e} The file is at {}.", path.display())),
        }
        self.tell(title, note);
    }

    /// Started from TunerPro with a definition open: its tables become the project's maps.
    fn take_tunerpro_xdf(&mut self) {
        let (Some(x), Some(pid)) = (self.tunerpro.xdf.clone(), self.active_project) else { return };
        let name = x.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        match std::fs::read(&x).map_err(|e| e.to_string()).and_then(|b| crate::xdf::read_xdf(&b)) {
            Ok(maps) => {
                let (added, left) = self.add_maps(pid, maps);
                self.status_hint = if left > 0 {
                    format!("{added} map(s) taken from {name}; {left} don't fit this file.")
                } else {
                    format!("{added} map(s) taken from {name}.")
                };
            }
            Err(e) => self.tell("TunerPro definition", format!("{name}: {e}")),
        }
    }

    fn copy_map_part(&mut self, ctx: &egui::Context, x: bool, y: bool, values: bool) {
        let Some(c) = self.active_child() else { return };
        let Some(p) = self.project(c.project) else { return };
        let Some(m) = c.map_uid.and_then(|u| p.maps.iter().find(|m| m.uid == u)) else { return };
        ctx.copy_text(self.map_text(c, p, m, x, y, values));
    }

    fn insert_new_map(&mut self) {
        let Some(c) = self.active_child() else { return };
        let (pid, a, org, radix) = (c.project, c.g.addr(c.g.cursor), c.g.org, c.g.radix);
        let Some(p) = self.project_mut(pid) else { return };
        let uid = p.uid();
        let mut m = MapDef { uid, name: format!("Map ({a:05X})"), start: a, org, radix, ..MapDef::default() };
        let d = p.data().to_vec();
        m.auto_range(&d);
        p.maps.push(m.clone());
        p.dirty_struct = true;
        self.open_map(pid, uid);
        self.modal = Some(Modal::map_props(m));
    }

    fn delete_map_at_cursor(&mut self) {
        let Some(c) = self.active_child() else { return };
        let (pid, cid) = (c.project, c.id);
        let uid = c.map_uid.or_else(|| self.project(pid).and_then(|p| p.map_at(c.g.addr(c.g.cursor))).map(|m| m.uid));
        let Some(uid) = uid else {
            self.tell("Delete map", "There is no map at the cursor.");
            return;
        };
        if let Some(p) = self.project_mut(pid) {
            p.maps.retain(|m| m.uid != uid);
            p.dirty_struct = true;
        }
        let dead: Vec<u64> = self.children.iter().filter(|x| x.project == pid && x.map_uid == Some(uid)).map(|x| x.id).collect();
        for d in dead {
            self.close_child(d);
        }
        let _ = cid;
    }

    fn inplace(&mut self) {
        let Some(c) = self.active_child() else { return };
        let (Some(buf), g) = (c.g.inplace.clone(), c.g.clone()) else {
            let p = self.project(c.project);
            let text = p.and_then(|p| c.g.raw(p, c.g.cursor).map(|v| c.g.format(c.g.scaled(v)))).unwrap_or_default();
            self.with_grid(|g| {
                g.inplace = Some(text);
                g.follow = true;
            });
            return;
        };
        let v = if g.radix == 16 && !g.has_factor() {
            u64::from_str_radix(buf.trim(), 16).ok().map(|x| x as f64)
        } else {
            buf.trim().replace(',', ".").parse::<f64>().ok()
        };
        self.with_grid(|g| g.inplace = None);
        match v {
            Some(v) => {
                let cell = g.cursor;
                self.change_cells("Inplace edit", Some(vec![cell]), &move |_, _, g, _| g.unscaled(v));
                self.with_grid(|g| {
                    g.cursor += 1;
                    g.follow = true;
                });
            }
            None => self.tell("Edit cell", format!("\"{}\" isn't a number.", buf.trim())),
        }
    }

    fn apply_checksum_at_cursor(&mut self) {
        let Some(c) = self.active_child() else { return };
        let a = c.g.addr(c.g.cursor);
        let Some(p) = self.current_project_mut() else { return };
        let Some(i) = p.checksums.iter().position(|k| a >= k.from && a <= k.to) else {
            self.tell("Correct checksum", "The cursor isn't inside a checksum block.");
            return;
        };
        p.begin_edit("Checksum");
        let r = checksum::correct(p, i);
        p.end_edit();
        if let Err(e) = r {
            self.tell("Correct checksum", e);
        }
    }

    fn open_window_props(&mut self) {
        let Some(c) = self.active_child() else { return };
        if let Some(uid) = c.map_uid {
            if let Some(m) = self.project(c.project).and_then(|p| p.maps.iter().find(|m| m.uid == uid)).cloned() {
                self.modal = Some(Modal::map_props(m));
            }
        } else {
            self.modal = Some(Modal::hex_props(&c.g));
        }
    }

    fn optimize_range(&mut self) {
        let Some(c) = self.active_child() else { return };
        let Some(p) = self.project(c.project) else { return };
        let g = c.g.clone();
        let total = g.total(self.end_of(c));
        let cells = if g.has_selection() {
            g.selected_cells(total)
        } else {
            let a = g.top_row * g.cols;
            let b = ((g.top_row + g.visible_rows) * g.cols).min(total);
            (a..b).collect()
        };
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for i in cells {
            if let Some(v) = g.raw(p, i) {
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
        if lo <= hi {
            let uid = c.map_uid;
            let pid = c.project;
            let hi = if hi > lo { hi } else { lo + 1.0 };
            self.with_grid(|g| {
                g.range_lo = lo;
                g.range_hi = hi;
            });
            if let (Some(u), Some(p)) = (uid, self.project_mut(pid)) {
                if let Some(m) = p.maps.iter_mut().find(|m| m.uid == u) {
                    m.range_lo = lo;
                    m.range_hi = hi;
                }
            }
        }
    }

    /// F: selects the next potential map, or jumps through parallel maps in a map window.
    fn detect_map(&mut self, back: bool) {
        let Some(c) = self.active_child() else { return };
        let Some(p) = self.project(c.project) else { return };
        if let Some(uid) = c.map_uid {
            let Some(group) = p.maps.iter().find(|m| m.uid == uid).and_then(|m| m.parallel_group) else { return };
            let mut same: Vec<u64> = p.maps.iter().filter(|m| m.parallel_group == Some(group)).map(|m| m.uid).collect();
            same.sort_unstable();
            let Some(i) = same.iter().position(|&u| u == uid) else { return };
            let n = same.len();
            let t = same[if back { (i + n - 1) % n } else { (i + 1) % n }];
            let pid = c.project;
            self.open_map(pid, t);
            return;
        }
        if !p.search_done {
            self.tell("Find a map", "This works once the map finder has finished. The status bar shows how far it is.");
            return;
        }
        let a = c.g.addr(c.g.cursor);
        let bits = self.list.bits;
        let ok = |m: &&MapDef| bits == 0 || m.org.width() as u32 * 8 == bits;
        let hit = if back {
            p.potential.iter().rev().filter(ok).find(|m| m.start < a).cloned()
        } else {
            p.potential.iter().filter(ok).find(|m| m.start > a).cloned()
        };
        match hit {
            Some(m) => self.fit_to(&m),
            None => self.status_hint = "No more suggested maps.".into(),
        }
    }

    fn support_selection(&mut self) {
        let has = self.active_child().is_some_and(|c| c.g.has_selection() && c.map_uid.is_none());
        if !has {
            self.cfg.support_selection = !self.cfg.support_selection;
            return;
        }
        let Some(c) = self.active_child() else { return };
        let Some(p) = self.project(c.project) else { return };
        let g = c.g.clone();
        let Some(s) = g.sel else { return };
        let (a, b) = (s.anchor.min(s.head), s.anchor.max(s.head));
        let r = analysis::support_selection(p.data(), g.addr(a), g.addr(b + 1) - 1, g.org);
        match r {
            Some((start, cols, rows)) => {
                let w = g.org.width();
                let range = p.element_range().0;
                self.with_grid(|g| {
                    g.cols = cols;
                    let row_bytes = cols * w;
                    g.base = range + (start - range.min(start)) % row_bytes;
                    let f = g.cell_of(start);
                    g.sel_mode = SelMode::Rectangle;
                    g.sel = Some(Sel::new(f, f + cols * rows - 1));
                    g.cursor = f;
                    g.follow = true;
                });
            }
            None => self.status_hint = "No map shape could be found in this selection.".into(),
        }
    }

    fn save_selection(&mut self) {
        let Some(c) = self.active_child() else { return };
        let Some(p) = self.project(c.project) else { return };
        let g = &c.g;
        let total = g.total(self.end_of(c));
        let cells = g.selected_cells(total);
        let (Some(&f), Some(&l)) = (cells.first(), cells.last()) else { return };
        let (a, b) = (g.addr(f), g.addr(l + 1).min(p.size()));
        let s = formats::SelectionFile { addr: a, data: p.data()[a..b].to_vec() };
        if let Some(path) = rfd::FileDialog::new().add_filter("Selection", &["alssel"]).set_file_name("selection.alssel").save_file() {
            if let Err(e) = std::fs::write(path, formats::write_selection(&s)) {
                self.tell("Save selection", e.to_string());
            }
        }
    }

    fn list_jump(&mut self, pid: u64, uid: u64, kind: RowKind) {
        match kind {
            RowKind::Hexdump => self.pending.push(Cmd::Activate(uid)),
            RowKind::Map | RowKind::Potential => {
                let m = self.project(pid).and_then(|p| if kind == RowKind::Map { p.maps.iter().find(|m| m.uid == uid) } else { p.potential.iter().find(|m| m.uid == uid) }).cloned();
                let Some(m) = m else { return };
                // Show it in this project's hexdump.
                let hex = self.children.iter().find(|c| c.project == pid && c.map_uid.is_none()).map(|c| c.id);
                if let Some(h) = hex {
                    self.active = Some(h);
                    self.active_project = Some(pid);
                    if kind == RowKind::Potential {
                        self.fit_to(&m);
                    } else {
                        self.goto_addr(m.start);
                    }
                }
            }
        }
    }

    fn list_open(&mut self, pid: u64, uid: u64, kind: RowKind) {
        match kind {
            RowKind::Hexdump => self.pending.push(Cmd::Activate(uid)),
            RowKind::Map => {
                let open = self.children.iter().find(|c| c.project == pid && c.map_uid == Some(uid)).map(|c| c.id);
                match open {
                    Some(id) => self.close_child(id),
                    None => self.open_map(pid, uid),
                }
            }
            RowKind::Potential => {
                self.active_project = Some(pid);
                let i = self.project(pid).and_then(|p| p.potential.iter().position(|m| m.uid == uid));
                if let Some(i) = i {
                    self.potential_to_map(i);
                }
            }
        }
    }

    // ---------- project folder functions ----------
    fn project_files(&self) -> Vec<std::path::PathBuf> {
        let mut out = vec![];
        if let Ok(rd) = std::fs::read_dir(self.cfg.project_dir()) {
            for e in rd.flatten() {
                if e.path().extension().is_some_and(|x| x == "alsproj") {
                    out.push(e.path());
                }
            }
        }
        out.sort();
        out
    }

    fn search_similar_projects(&mut self) {
        let Some(me) = self.current_project() else { return };
        let mine = me.data().to_vec();
        let own = me.path.clone();
        let mut lines = vec![];
        for f in self.project_files() {
            if Some(&f) == own.as_ref() {
                continue;
            }
            if let Ok(p) = Project::load(&f, 0) {
                let d = p.versions.first().map(|v| v.data.as_slice()).unwrap_or(&[]);
                let n = d.len().min(mine.len());
                if n == 0 {
                    continue;
                }
                let same = d[..n].iter().zip(&mine[..n]).filter(|(a, b)| a == b).count();
                let pct = same as f64 / d.len().max(mine.len()) as f64 * 100.0;
                if pct >= 50.0 {
                    lines.push((pct, f.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default()));
                }
            }
        }
        lines.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let text = if lines.is_empty() {
            "No similar project was found in the project folder.".to_string()
        } else {
            lines.iter().map(|(p, n)| format!("{p:.1}%  {n}")).collect::<Vec<_>>().join("\n")
        };
        self.tell("Projects like this one", text);
    }

    fn search_double_versions(&mut self) {
        let mut lines = vec![];
        for f in self.project_files() {
            if let Ok(p) = Project::load(&f, 0) {
                for i in 0..p.versions.len() {
                    for j in i + 1..p.versions.len() {
                        if p.versions[i].data == p.versions[j].data {
                            lines.push(format!("{}: \"{}\" = \"{}\"", f.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default(), p.versions[i].name, p.versions[j].name));
                        }
                    }
                }
            }
        }
        let t = if lines.is_empty() { "No project contains the same version twice.".to_string() } else { format!("{}\n\nNothing was deleted. Open a project and remove a version in its version list.", lines.join("\n")) };
        self.tell("Duplicate versions", t);
    }

    fn update_all_projects(&mut self) {
        let open: Vec<_> = self.projects.iter().filter_map(|p| p.path.clone()).collect();
        let (mut n, mut failed) = (0, 0);
        for f in self.project_files() {
            if open.contains(&f) {
                continue;
            }
            match Project::load(&f, 0) {
                Ok(mut p) => {
                    let before = serde_json::to_string(&p.props).unwrap_or_default();
                    crate::app::fill_vehicle_facts(&mut p);
                    if before != serde_json::to_string(&p.props).unwrap_or_default() {
                        if p.save(&f).is_ok() {
                            n += 1;
                        } else {
                            failed += 1;
                        }
                    }
                }
                Err(_) => failed += 1,
            }
        }
        self.tell("Refresh project details", format!("{n} project(s) updated, {failed} couldn't be processed. Open projects are skipped."));
    }

    fn import_dir(&mut self, ctx: &egui::Context) {
        let Some(dir) = rfd::FileDialog::new().pick_folder() else { return };
        let mut n = 0;
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let path = e.path();
                if path.is_file() {
                    self.import_path(&path, None, ctx);
                    n += 1;
                }
            }
        }
        self.tell("Import a folder", format!("{n} file(s) imported."));
    }

    /// Exports the project next to the mail request, then opens a new mail. Nothing is sent by itself.
    fn send_mail(&mut self) {
        let Some(id) = self.active_project else { return };
        if !self.save_project(id) {
            return;
        }
        let Some(p) = self.project(id) else { return };
        let path = p.path.clone().unwrap_or_default();
        let name = p.map_title_name();
        let subject = format!("Project {name}").replace(' ', "%20");
        let _ = std::process::Command::new("cmd").args(["/C", "start", "", &format!("mailto:?subject={subject}")]).spawn();
        self.tell("Project by mail", format!("A new mail was opened. Please attach this file yourself:\n{}", path.display()));
    }

    /// Writes a printable page and opens it, where the browser's print function takes over.
    fn print(&mut self) {
        let Some(c) = self.active_child() else { return };
        let Some(p) = self.project(c.project) else { return };
        let g = &c.g;
        let mut h = String::from("<html><head><meta charset=utf-8><style>body{font:12px monospace}td{padding:0 6px;text-align:right}</style></head><body>");
        h.push_str(&format!("<h3>{}</h3><table>", html(&self.child_title(c))));
        let total = g.total(self.end_of(c));
        let (first, rows) = if c.map_uid.is_some() { (0, total / g.cols.max(1)) } else { (g.top_row * g.cols, g.visible_rows) };
        for r in 0..rows {
            h.push_str(&format!("<tr><th>{:05X}</th>", g.addr(first + r * g.cols)));
            for k in 0..g.cols {
                let i = first + r * g.cols + k;
                if i < total {
                    h.push_str(&format!("<td>{}</td>", html(&g.text(p, i, None).unwrap_or_default())));
                }
            }
            h.push_str("</tr>");
        }
        h.push_str("</table></body></html>");
        let path = std::env::temp_dir().join("als_print.html");
        match std::fs::write(&path, h) {
            Ok(()) => {
                let _ = std::process::Command::new("cmd").args(["/C", "start", "", &path.to_string_lossy()]).spawn();
            }
            Err(e) => self.tell("Print", e.to_string()),
        }
    }

    // ---------- keyboard ----------
    pub fn keyboard(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || self.modal.is_some() {
            return;
        }
        let events: Vec<egui::Event> = ctx.input(|i| i.events.clone());
        for e in events {
            match e {
                egui::Event::Copy => self.pending.push(Cmd::Copy),
                egui::Event::Cut => self.pending.push(Cmd::Cut),
                egui::Event::Paste(t) => {
                    self.paste_text(&t);
                }
                egui::Event::Key { key, pressed: true, modifiers, .. } => self.key(key, modifiers),
                egui::Event::Text(t) => self.typed(&t),
                _ => {}
            }
        }
    }

    fn key(&mut self, k: Key, m: Modifiers) {
        let (c, s, a) = (m.ctrl, m.shift, m.alt);
        let has = self.active_child().is_some();
        let cmd = match k {
            Key::N if c => Some(Cmd::NewProject),
            Key::O if c && a => Some(Cmd::SearchSimilar),
            Key::O if c && s => Some(Cmd::OpenVersion),
            Key::O if c => Some(Cmd::OpenProject),
            Key::S if c && s => Some(Cmd::SaveAll),
            Key::S if c => Some(Cmd::Save),
            Key::E if c => Some(Cmd::ExportFile),
            Key::I if c && a => Some(Cmd::ImportChanges),
            Key::I if c && s => Some(Cmd::ImportMaps),
            Key::I if c => Some(Cmd::ImportFile),
            Key::M if c => Some(Cmd::SendMail),
            Key::T if c && s && self.tunerpro.connected() => Some(Cmd::SendTunerPro),
            Key::D if c => Some(Cmd::ImportA2l),
            Key::P if c => Some(Cmd::Print),
            Key::Z if c => Some(Cmd::Undo),
            Key::Y if c => Some(Cmd::Redo),
            Key::K if c && s => Some(Cmd::SelEnd),
            Key::K if c => Some(Cmd::MapSelection),
            Key::U if c => Some(Cmd::Differences),
            Key::F if c && s => Some(Cmd::SearchSel),
            Key::F if c => Some(Cmd::SearchBytes),
            Key::G if c => Some(Cmd::GoTo),
            Key::B if c && s => Some(Cmd::SelBegin),
            Key::B if c => Some(Cmd::OptimizeRange),
            Key::A if c => Some(Cmd::SelScreen),
            Key::R if c => Some(Cmd::SelLoad),
            Key::W if c => Some(Cmd::SelSave),
            Key::F1 => Some(Cmd::Help("start")),
            Key::F2 if a => Some(Cmd::ApplyChecksum),
            Key::F2 => Some(Cmd::Checksums),
            Key::F3 if a => Some(Cmd::SearchStart),
            Key::F3 if s => Some(Cmd::SearchBack),
            Key::F3 => Some(Cmd::SearchNext),
            Key::F4 if c => Some(Cmd::CloseProject),
            Key::F5 if c => Some(Cmd::MarkerToggle),
            Key::F5 if s => Some(Cmd::MarkerPrev),
            Key::F5 => Some(Cmd::MarkerNext),
            Key::F9 => Some(Cmd::Again),
            Key::F11 => Some(Cmd::OriginalValue),
            Key::F12 => Some(Cmd::Config),
            Key::Enter if c && a => Some(Cmd::PropsProject),
            Key::Enter if s && a => Some(Cmd::PropsVersion),
            Key::Enter if a => Some(Cmd::PropsWindow),
            Key::Enter if s => Some(Cmd::CommentEdit),
            Key::Enter if has => Some(Cmd::Inplace),
            Key::Insert if has => Some(Cmd::SelCell),
            Key::Delete if has => Some(Cmd::SelNone),
            _ => None,
        };
        if let Some(x) = cmd {
            self.pending.push(x);
            return;
        }
        if !has {
            return;
        }
        // Moving the cursor
        if let (Key::ArrowLeft | Key::ArrowRight, true, true) = (k, c, s) {
            let d = if k == Key::ArrowLeft { -0.05 } else { 0.05 };
            self.with_grid(|g| g.rot += d);
            return;
        }
        if c && matches!(k, Key::ArrowLeft | Key::ArrowRight) {
            self.pending.push(if k == Key::ArrowLeft { Cmd::OriginLeft } else { Cmd::OriginRight });
            return;
        }
        if c && matches!(k, Key::ArrowUp | Key::ArrowDown) {
            let d: i64 = if k == Key::ArrowUp { -1 } else { 1 };
            self.with_grid(|g| g.top_row = (g.top_row as i64 + d).max(0) as usize);
            return;
        }
        let jump = self.cfg.page_jump;
        let end = self.active_child().map(|ch| self.end_of(ch)).unwrap_or(0);
        let clear = self.cfg.delete_sel_on_move;
        self.with_grid(|g| {
            let total = g.total(end).max(1);
            let cols = g.cols.max(1);
            let page = g.visible_rows.max(1) * cols;
            let cur = g.cursor;
            let next: Option<usize> = match k {
                Key::ArrowLeft => Some(cur.saturating_sub(1)),
                Key::ArrowRight => Some(cur + 1),
                Key::ArrowUp => Some(cur.saturating_sub(cols)),
                Key::ArrowDown => Some(cur + cols),
                Key::PageUp if c => Some(cur.saturating_sub(jump / g.org.width())),
                Key::PageDown if c => Some(cur + jump / g.org.width()),
                Key::PageUp => Some(cur.saturating_sub(page)),
                Key::PageDown => Some(cur + page),
                Key::Home if c => Some(0),
                Key::End if c => Some(total - 1),
                Key::Home => Some(cur - cur % cols),
                Key::End => Some(cur - cur % cols + cols - 1),
                _ => None,
            };
            if let Some(n) = next {
                let n = n.min(total - 1);
                if s {
                    let anchor = g.sel.map(|x| x.anchor).unwrap_or(cur);
                    g.sel = Some(Sel::new(anchor, n));
                } else if clear {
                    g.clear_sel();
                }
                g.cursor = n;
                g.follow = true;
            }
        });
    }

    /// Single-letter hotkeys. Text events carry the shifted symbols such as % and =.
    fn typed(&mut self, t: &str) {
        let has = self.active_child().is_some();
        let has_proj = self.current_project().is_some();
        let mut chars = t.chars();
        let (Some(ch), None) = (chars.next(), chars.next()) else { return };
        if !has {
            return;
        }
        let number_keys = self.cfg.number_keys_edit;
        let cmd = match ch {
            '+' | ',' => Some(Cmd::Plus),
            '-' => Some(Cmd::Minus),
            '=' => Some(Cmd::ChangeAbs),
            '%' => Some(Cmd::ChangeRel),
            '/' => Some(Cmd::ChangeEdit),
            'm' => Some(Cmd::MoreCols),
            'w' => Some(Cmd::LessCols),
            'z' => Some(Cmd::DefaultView),
            'u' => Some(Cmd::Differences),
            'e' => Some(Cmd::DiffFirst),
            'v' => Some(Cmd::DiffPrev),
            'n' => Some(Cmd::DiffNext),
            'l' => Some(Cmd::DiffLast),
            'V' => Some(Cmd::CommentPrev),
            'N' => Some(Cmd::CommentNext),
            'f' => Some(Cmd::DetectMap(false)),
            'F' => Some(Cmd::DetectMap(true)),
            'k' => Some(Cmd::SelToMap),
            'K' => Some(Cmd::OpenMapAtCursor),
            't' => Some(Cmd::ModePrev),
            'T' => Some(Cmd::ModeNext),
            's' => Some(Cmd::SearchBytes),
            'a' => Some(Cmd::GoTo),
            'c' => Some(Cmd::Checksums),
            'o' if has_proj => Some(Cmd::Overview),
            'p' if has_proj => Some(Cmd::Preview),
            '.' => Some(Cmd::ConnectDots),
            '#' => Some(Cmd::Bits(self.next_bits())),
            d if d.is_ascii_digit() && number_keys && self.active_child().is_some_and(|c| c.g.mode == ViewMode::Text) => {
                self.with_grid(|g| g.inplace = Some(d.to_string()));
                None
            }
            _ => None,
        };
        if let Some(x) = cmd {
            self.pending.push(x);
        }
    }

    fn next_bits(&self) -> u32 {
        match self.active_child().map(|c| c.g.width()) {
            Some(1) => 16,
            Some(2) => 32,
            _ => 8,
        }
    }

    fn dropped(&mut self, ctx: &egui::Context) {
        let files: Vec<std::path::PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        if files.is_empty() {
            return;
        }
        // Over a project window it becomes a new version, anywhere else a new project.
        let pos = ctx.input(|i| i.pointer.latest_pos());
        let target = pos.and_then(|p| {
            self.children.iter().rev().find(|c| c.rect.contains(p) && self.workspace.contains(p)).map(|c| c.project)
        });
        if files.len() == 2 {
            if let (Ok(a), Ok(b)) = (std::fs::read(&files[0]), std::fs::read(&files[1])) {
                let d = if formats::guess_high_first(&a, &b) { formats::merge_two(&a, &b) } else { formats::merge_two(&b, &a) };
                self.take_data(d, "", target, false, ctx);
                return;
            }
        }
        for f in files {
            self.import_path(&f, target, ctx);
        }
    }

    /// Prepares what a check or a demonstration looks at.
    fn run_demo(&mut self, ctx: &egui::Context, what: &str) {
        let id = self.new_project_id();
        let mut p = sample_project(id);
        let rules = self.cfg.rules.clone();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let mut found = analysis::run_search(p.data(), &rules, &cancel, &|_, _| {});
        found.retain(|m| !p.maps.iter().any(|k| k.start < m.end() && m.start < k.end()));
        for m in &mut found {
            m.uid = p.uid();
        }
        p.potential = found;
        p.search_done = true;
        p.overview = Some(analysis::build_overview(p.data(), &p.maps, &p.potential));
        let first_pot = p.potential.first().cloned();
        let map_uid = p.maps.first().map(|m| m.uid);
        self.adopt_project(p, ctx, false);
        match what {
            "2d" => self.with_grid(|g| g.mode = ViewMode::TwoD),
            "3d" | "map3d" | "map2d" | "map" => {
                if let Some(u) = map_uid {
                    self.open_map(id, u);
                    match what {
                        "3d" | "map3d" => self.with_grid(|g| g.mode = ViewMode::ThreeD),
                        "map2d" => self.with_grid(|g| g.mode = ViewMode::TwoD),
                        _ => {}
                    }
                }
            }
            "fit" => {
                if let Some(m) = first_pot {
                    self.fit_to(&m);
                }
            }
            "overview" => {
                self.show_overview = true;
                self.show_preview = true;
                self.show_diff = true;
            }
            "search" => {
                self.find.open = true;
                self.find.text = "12 34".into();
                self.find_list();
            }
            "props" => self.pending.push(Cmd::PropsProject),
            "checksums" => self.pending.push(Cmd::Checksums),
            "config" => self.pending.push(Cmd::Config),
            "open" => self.pending.push(Cmd::OpenProject),
            "change" => self.pending.push(Cmd::ChangeRel),
            _ => {}
        }
    }
}

fn html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

pub fn c_array(cells: &[f64], cols: usize, org: DataOrg) -> String {
    let ty = match (org.width(), org.is_float()) {
        (1, _) => "unsigned char",
        (2, _) => "unsigned short",
        (_, true) => "float",
        _ => "unsigned long",
    };
    let mut s = format!("{ty} data[{}] = {{\r\n", cells.len());
    for (i, row) in cells.chunks(cols.max(1)).enumerate() {
        let line: Vec<String> = row.iter().map(|v| if org.is_float() { format!("{v}") } else { format!("{}", *v as i64) }).collect();
        let last = (i + 1) * cols.max(1) >= cells.len();
        s.push_str(&format!("    {}{}\r\n", line.join(", "), if last { "" } else { "," }));
    }
    s.push_str("};\r\n");
    s
}



/// A made-up data file with a few tables, so that everything can be tried at once.
pub fn sample_project(id: u64) -> Project {
    let mut p = Project::new(id);
    let size = 0x40000;
    let mut x = 0x2545F491u32;
    let mut d: Vec<u8> = (0..size)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect();
    d[..0x2000].fill(0xFF);
    d[0x38000..].fill(0xFF);
    d[0x20000..0x24000].fill(0);
    let t = b"SAMPLE DATA - NOT FROM A REAL CAR";
    d[0x2100..0x2100 + t.len()].copy_from_slice(t);
    let org = DataOrg::HiLo;
    let put = |d: &mut Vec<u8>, a: &mut usize, v: f64| {
        d[*a..*a + 2].copy_from_slice(&org.encode(v, false));
        *a += 2;
    };
    let table = |d: &mut Vec<u8>, at: usize, nx: usize, ny: usize, f: &dyn Fn(usize, usize) -> f64| -> (usize, usize, usize) {
        let mut a = at;
        put(d, &mut a, nx as f64);
        put(d, &mut a, ny as f64);
        let xa = a;
        for i in 0..nx {
            put(d, &mut a, 1000.0 + i as f64 * 400.0);
        }
        let ya = a;
        for j in 0..ny {
            put(d, &mut a, 100.0 + j as f64 * 150.0);
        }
        let v = a;
        for j in 0..ny {
            for i in 0..nx {
                put(d, &mut a, f(i, j));
            }
        }
        (v, xa, ya)
    };
    let a = table(&mut d, 0x8000, 16, 12, &|i, j| 2000.0 + (i * j) as f64 * 18.0 + 200.0 * (i as f64 / 3.0).sin());
    table(&mut d, 0x8400, 10, 8, &|i, j| 5000.0 + i as f64 * 90.0 - j as f64 * 120.0);
    table(&mut d, 0x9000, 12, 16, &|i, j| 800.0 + i as f64 * 50.0 + (j * j) as f64 * 7.0);
    table(&mut d, 0xA000, 8, 8, &|i, j| 300.0 + (i * i) as f64 * 3.0 + j as f64 * 11.0);
    for j in 0..10 {
        for i in 0..14 {
            let v = 3000.0 + i as f64 * 120.0 + j as f64 * 40.0 + 50.0 * (i as f64).sin();
            let at = 0xB000 + (j * 14 + i) * 2;
            d[at..at + 2].copy_from_slice(&org.encode(v, false));
        }
    }
    p.props.file_name = "Sample.alsproj".into();
    p.props.producer = "Sample".into();
    p.add_version("", d.clone());
    let uid = p.uid();
    let mut m = MapDef {
        uid,
        name: "Sample torque limit".into(),
        start: a.0,
        cols: 16,
        rows: 12,
        org,
        unit: "Nm".into(),
        x: Axis { src: AxisSrc::Rom, addr: a.1, org, desc: "Engine speed".into(), unit: "rpm".into(), ..Axis::default() },
        y: Axis { src: AxisSrc::Rom, addr: a.2, org, desc: "Load".into(), unit: "%".into(), ..Axis::default() },
        ..MapDef::default()
    };
    m.auto_range(&d);
    p.maps.push(m);
    let v = p.add_version("Version 1", d);
    p.current = v;
    p.begin_edit("Sample change");
    for k in 0..12 {
        let at = a.0 + (k * 16 + 8) * 2;
        let old = org.read(p.data(), at, false).unwrap_or(0.0);
        p.write(at, &org.encode(old * 1.08, false));
    }
    p.end_edit();
    p.undo.clear();
    p.dirty_data = false;
    p.dirty_struct = false;
    p
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frame_ui(ui);
        crate::report::frame_drawn();
    }
}

impl App {
    /// One whole frame of the program; the tests call this directly.
    pub fn frame_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let had_focus = ctx.memory(|m| m.focused().is_some());
        if !self.fonts_ready {
            shell::setup_fonts(&ctx);
            self.fonts_ready = true;
        }
        if self.tunerpro.dark.is_some() {
            self.cfg.host_dark = self.tunerpro.dark;
        }
        let dark = self.wants_dark();
        if self.applied_dark != Some(dark) {
            shell::apply_colours(&ctx, dark);
            self.applied_dark = Some(dark);
        }
        self.frame += 1;
        if self.frame == 2 {
            if let Some(d) = self.demo.clone() {
                self.run_demo(&ctx, &d);
            }
        }
        if self.frame == 2 {
            for f in std::mem::take(&mut self.start_files) {
                self.import_path(std::path::Path::new(&f), None, &ctx);
            }
            self.take_tunerpro_xdf();
        }
        self.poll_search();
        if !self.searches.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        // Children whose project or map went away close themselves.
        let dead: Vec<u64> = self
            .children
            .iter()
            .filter(|c| {
                let p = self.project(c.project);
                p.is_none() || c.map_uid.is_some_and(|u| !p.is_some_and(|p| p.maps.iter().any(|m| m.uid == u)))
            })
            .map(|c| c.id)
            .collect();
        for d in dead {
            self.close_child(d);
        }
        // Map windows follow the map's settings.
        for i in 0..self.children.len() {
            if let Some(u) = self.children[i].map_uid {
                let pid = self.children[i].project;
                if let Some(m) = self.project(pid).and_then(|p| p.maps.iter().find(|m| m.uid == u)).cloned() {
                    let g = &mut self.children[i].g;
                    g.base = m.start;
                    g.cols = m.cols.max(1);
                    g.rows_fixed = Some(m.rows.max(1));
                    g.org = m.org;
                }
            }
        }
        let title = match self.current_project() {
            Some(p) => format!("{} - {} ({}) as {}", crate::app::TITLE, p.map_title_name(), p.version_name(), p.element_label()),
            None => crate::app::TITLE.to_string(),
        };
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
        egui::Panel::top("menu").exact_size(20.0).frame(egui::Frame::NONE.fill(shell::menu_bg())).show(ui, |ui| self.menu_bar(ui));
        egui::Panel::top("bars").exact_size(73.0).frame(egui::Frame::NONE.fill(shell::bar_bg())).show(ui, |ui| self.toolbars(ui));
        if self.cfg.status_bar {
            egui::Panel::bottom("status").exact_size(20.0).frame(egui::Frame::NONE.fill(shell::bar_bg())).show(ui, |ui| self.status_bar(ui));
        }
        if self.list.visible && !self.list.floating {
            // The list keeps the width the person dragged it to; see the grip in `map_list`.
            let width = self.cfg.list_width.clamp(shell::LIST_MIN, shell::LIST_MAX);
            egui::Panel::left("maps").exact_size(width).resizable(false).frame(egui::Frame::NONE.fill(shell::panel_bg())).show(ui, |ui| self.map_list(ui));
        }
        egui::CentralPanel::no_frame().frame(egui::Frame::NONE.fill(shell::work_bg())).show(ui, |ui| {
            let ws = ui.max_rect();
            // When the map list is hidden, this little arrow brings it back.
            if !self.list.visible {
                let tab = egui::Rect::from_min_size(ws.left_center() - egui::Vec2::new(0.0, 25.0), egui::Vec2::new(8.0, 50.0));
                let r = ui.interact(tab, ui.scope_id().with("unfold"), egui::Sense::click());
                ui.painter().rect_filled(tab, 2.0, shell::paper());
                let cx = tab.center();
                ui.painter().add(egui::Shape::convex_polygon(
                    vec![cx + egui::vec2(3.0, 0.0), cx + egui::vec2(-3.0, -6.0), cx + egui::vec2(-3.0, 6.0)],
                    shell::rgb(0, 0, 128),
                    egui::Stroke::NONE,
                ));
                if r.clicked() {
                    self.list.visible = true;
                }
            }
            self.workspace_windows(&ctx, ws);
        });
        if self.list.visible && self.list.floating && !self.floats_hidden {
            let mut open = true;
            egui::Window::new("Map list").order(egui::Order::Foreground).open(&mut open).fixed_size([240.0, 520.0]).default_pos(self.workspace.left_top() + egui::vec2(10.0, 10.0)).show(&ctx, |ui| {
                ui.set_min_size(egui::vec2(240.0, 520.0));
                self.map_list(ui);
            });
            self.list.visible = open;
        }
        self.floating(&ctx);
        let dialog_open = self.modal.is_some() || self.message.is_some();
        self.show_modal(&ctx);
        if !dialog_open {
            self.keyboard(&ctx);
        }
        if !dialog_open && !had_focus && ctx.input(|i| i.key_pressed(Key::Tab) && i.modifiers.is_none()) {
            // The toolkit would move the focus to a widget; here Tab toggles the floating windows.
            if let Some(id) = ctx.memory(|m| m.focused()) {
                ctx.memory_mut(|m| m.surrender_focus(id));
            }
            self.pending.push(Cmd::ToggleFloat);
        }
        self.dropped(&ctx);
        let cmds = std::mem::take(&mut self.pending);
        for c in cmds {
            self.run_cmd(c, &ctx);
        }
        // Closing the window asks about unsaved work first.
        if ctx.input(|i| i.viewport().close_requested()) && self.screenshot.is_none() {
            let dirty = self.projects.iter().find(|p| p.dirty_data || p.dirty_struct).map(|p| p.id);
            if let Some(id) = dirty {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.modal = Some(Modal::SaveChanges { pid: id, exit: true });
            } else {
                self.cfg.save();
            }
        }
        if self.quit_now {
            self.quit_now = false;
            self.cfg.save();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if let Some(path) = self.screenshot.clone() {
            ctx.request_repaint();
            if self.frame >= 120 && !self.shot_taken {
                self.shot_taken = true;
                let done = self.running_shot.clone();
                let c2 = ctx.clone();
                ctx.request_screenshot(move |img| {
                    let (w, h) = (img.size[0] as u32, img.size[1] as u32);
                    let mut rgba = Vec::with_capacity(img.pixels.len() * 4);
                    for p in &img.pixels {
                        rgba.extend_from_slice(&[p.r(), p.g(), p.b(), 255]);
                    }
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if let Err(e) = image::save_buffer(&path, &rgba, w, h, image::ColorType::Rgba8) {
                        eprintln!("The picture couldn't be saved: {e}");
                    }
                    done.store(true, Ordering::Relaxed);
                    c2.request_repaint();
                });
            }
            if self.running_shot.load(Ordering::Relaxed) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{parse_goto, LastChange};

    #[test]
    fn sample_project_is_searchable_and_changed() {
        let p = sample_project(1);
        assert_eq!(p.versions.len(), 2);
        assert_eq!(p.maps.len(), 1);
        assert!(!p.changed_ranges().is_empty());
        let found = analysis::run_search(p.data(), &analysis::SearchRules::default(), &std::sync::atomic::AtomicBool::new(false), &|_, _| {});
        assert!(found.len() >= 4, "found {}", found.len());
    }

    #[test]
    fn c_array_shape() {
        let t = c_array(&[1.0, 2.0, 3.0, 4.0], 2, DataOrg::Byte);
        assert!(t.starts_with("unsigned char data[4] = {"));
        assert!(t.contains("1, 2,"));
        assert!(t.contains("3, 4\r\n"));
    }

    #[test]
    fn goto_is_reachable_from_here() {
        let p = sample_project(1);
        assert_eq!(parse_goto("sample torque", &p, false), Ok(p.maps[0].start));
    }

    #[test]
    fn last_change_can_repeat() {
        assert_eq!(LastChange::Step(1), LastChange::Step(1));
    }
}
