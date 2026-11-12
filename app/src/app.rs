//! The running program: open projects, open windows, and what every command does.
use crate::analysis::{self, SearchHandle, SearchMsg};
use crate::checksum;
use crate::cmd::Cmd;
use crate::config::Config;
use crate::dialogs::Modal;
use crate::formats::{self, Loaded};
use crate::model::*;
use crate::search;
use crate::view::{Child, Env, Grid, Sel, SelMode};
use egui::{Pos2, Rect, Vec2};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Receiver, Sender};

#[derive(Clone, Debug)]
pub struct Clip {
    pub cells: Vec<f64>,
    pub cols: usize,
    pub org: DataOrg,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LastChange {
    Absolute(f64),
    Relative { from_original: bool, percent: bool, w: f64 },
    Step(i32),
    Original,
}

/// The search window's state, kept while the window is closed.
pub struct FindState {
    pub open: bool,
    pub tab: search::Radix,
    pub text: String,
    pub realtime: bool,
    pub in_original: bool,
    pub match_case: bool,
    pub from: String,
    pub to: String,
    pub up: String,
    pub down: String,
    pub align: search::Align,
    pub skip_code: bool,
    pub results: Vec<usize>,
    pub show_list: bool,
    pub status: String,
    pub similar: Option<Vec<f64>>,
    pub tolerance: f64,
}

impl Default for FindState {
    fn default() -> Self {
        FindState {
            open: false,
            tab: search::Radix::Hex,
            text: String::new(),
            realtime: false,
            in_original: false,
            match_case: false,
            from: String::new(),
            to: String::new(),
            up: "0".into(),
            down: "0".into(),
            align: search::Align::Auto,
            skip_code: false,
            results: vec![],
            show_list: false,
            status: String::new(),
            similar: None,
            tolerance: 5.0,
        }
    }
}

pub struct ListState {
    pub filter: String,
    pub history: Vec<String>,
    pub collapsed: HashSet<String>,
    pub bits: u32,
    pub changed_only: bool,
    pub flagged_only: bool,
    pub show_hidden: bool,
    pub hide_potential: bool,
    pub visible: bool,
    pub floating: bool,
    pub selected: Option<(u64, u64)>,
    pub sort_by_name: bool,
}

pub struct App {
    pub cfg: Config,
    pub projects: Vec<Project>,
    pub children: Vec<Child>,
    pub active: Option<u64>,
    pub active_project: Option<u64>,
    pub next_id: u64,
    pub next_project: u64,
    pub pending: Vec<Cmd>,
    pub modal: Option<Modal>,
    pub message: Option<(String, String)>,
    pub searches: HashMap<u64, SearchHandle>,
    pub search_rx: Receiver<SearchMsg>,
    pub search_tx: Sender<SearchMsg>,
    pub search_status: String,
    pub clip: Option<Clip>,
    pub last_change: Option<LastChange>,
    pub find: FindState,
    pub show_diff: bool,
    pub show_overview: bool,
    pub show_preview: bool,
    pub floats_hidden: bool,
    pub diff_from: String,
    pub diff_max: String,
    pub list: ListState,
    pub workspace: Rect,
    pub screenshot: Option<PathBuf>,
    pub demo: Option<String>,
    pub frame: u32,
    pub ctx_repeat: (std::time::Instant, u32),
    pub status_hint: String,
    pub preview_cam: (f32, f32, f32),
    pub connect_panel: bool,
    pub quit_now: bool,
    pub fonts_ready: bool,
    pub shot_taken: bool,
    pub running_shot: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub swap: Option<formats::LineSwap>,
    pub start_files: Vec<String>,
    pub last_title: String,
    pub tunerpro: crate::tunerpro::Link,
    /// The colour set the frame was last switched to, so it is only switched on a change.
    pub applied_dark: Option<bool>,
}

/// What the title bar says, in front of the open project.
pub const TITLE: &str = "TunerPro Plugin ALS By AlexDev-TH";

impl App {
    pub fn new(screenshot: Option<PathBuf>, demo: Option<String>) -> App {
        let (tx, rx) = channel();
        let mut cfg = Config::load();
        if screenshot.is_some() {
            // A picture for checking always starts from the same, untouched settings.
            cfg = Config::default();
        }
        App {
            cfg,
            projects: vec![],
            children: vec![],
            active: None,
            active_project: None,
            next_id: 1,
            next_project: 1,
            pending: vec![],
            modal: None,
            message: None,
            searches: HashMap::new(),
            search_rx: rx,
            search_tx: tx,
            search_status: String::new(),
            clip: None,
            last_change: None,
            find: FindState::default(),
            show_diff: false,
            show_overview: false,
            show_preview: false,
            floats_hidden: false,
            diff_from: "0".into(),
            diff_max: "2000".into(),
            list: ListState {
                filter: String::new(),
                history: vec![],
                collapsed: HashSet::new(),
                bits: 0,
                changed_only: false,
                flagged_only: false,
                show_hidden: false,
                hide_potential: false,
                visible: true,
                floating: false,
                selected: None,
                sort_by_name: false,
            },
            workspace: Rect::from_min_size(Pos2::new(235.0, 100.0), Vec2::new(900.0, 650.0)),
            screenshot,
            demo,
            frame: 0,
            ctx_repeat: (std::time::Instant::now(), 0),
            status_hint: "Press F1 to receive help.".into(),
            preview_cam: (0.6, 0.35, 1.0),
            connect_panel: false,
            quit_now: false,
            fonts_ready: false,
            shot_taken: false,
            running_shot: Default::default(),
            swap: None,
            start_files: vec![],
            last_title: String::new(),
            tunerpro: Default::default(),
            applied_dark: None,
        }
    }

    /// Whether the frame should use its dark colours: the person's choice, or TunerPro's theme.
    pub fn wants_dark(&self) -> bool {
        match self.cfg.colours {
            crate::config::Colours::LikeTunerPro => self.tunerpro.dark.or(self.cfg.host_dark).unwrap_or(false),
            crate::config::Colours::Light => false,
            crate::config::Colours::Dark => true,
        }
    }

    // ---------- lookups ----------
    pub fn child_index(&self, id: u64) -> Option<usize> {
        self.children.iter().position(|c| c.id == id)
    }
    pub fn active_child(&self) -> Option<&Child> {
        self.active.and_then(|id| self.child_index(id)).map(|i| &self.children[i])
    }
    pub fn active_child_mut(&mut self) -> Option<&mut Child> {
        let i = self.active.and_then(|id| self.child_index(id))?;
        Some(&mut self.children[i])
    }
    pub fn project_index(&self, id: u64) -> Option<usize> {
        self.projects.iter().position(|p| p.id == id)
    }
    pub fn project(&self, id: u64) -> Option<&Project> {
        self.projects.iter().find(|p| p.id == id)
    }
    pub fn project_mut(&mut self, id: u64) -> Option<&mut Project> {
        self.projects.iter_mut().find(|p| p.id == id)
    }
    pub fn current_project(&self) -> Option<&Project> {
        self.active_project.and_then(|id| self.project(id))
    }
    pub fn current_project_mut(&mut self) -> Option<&mut Project> {
        let id = self.active_project?;
        self.project_mut(id)
    }
    pub fn end_of(&self, c: &Child) -> usize {
        self.project(c.project).map(|p| p.element_range().1).unwrap_or(0)
    }
    pub fn tell(&mut self, title: &str, text: impl Into<String>) {
        self.message = Some((title.to_string(), text.into()));
    }

    // ---------- windows ----------
    fn next_rect(&self, w: f32, h: f32) -> Rect {
        let n = self.children.len() as f32 % 8.0;
        let o = self.workspace.min + Vec2::new(4.0 + n * 22.0, 4.0 + n * 22.0);
        let max = self.workspace.size() - Vec2::new(8.0, 8.0);
        Rect::from_min_size(o, Vec2::new(w.min(max.x.max(200.0)), h.min(max.y.max(120.0))))
    }

    pub fn new_grid(&self) -> Grid {
        Grid {
            mode: self.cfg.new_mode,
            radix: self.cfg.new_radix,
            right: self.cfg.new_right,
            ..Grid::default()
        }
    }

    pub fn open_hexdump(&mut self, project: u64) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let mut g = self.new_grid();
        if let Some(p) = self.project(project) {
            g.base = p.element_range().0;
        }
        let w = self.workspace.width() - 8.0;
        let h = (self.workspace.height() * 0.88).max(240.0);
        let rect = self.next_rect(w.min(1360.0), h.min(570.0));
        self.children.push(Child { id, project, map_uid: None, rect, restore: None, minimized: false, g, connected: None, connect_offset: 0 });
        self.active = Some(id);
        self.active_project = Some(project);
        id
    }

    pub fn open_map(&mut self, project: u64, uid: u64) {
        if let Some(c) = self.children.iter().find(|c| c.project == project && c.map_uid == Some(uid)) {
            let id = c.id;
            self.active = Some(id);
            return;
        }
        let Some(map) = self.project(project).and_then(|p| p.maps.iter().find(|m| m.uid == uid)).cloned() else { return };
        let id = self.next_id;
        self.next_id += 1;
        let mut g = self.new_grid();
        g.apply_map(&map);
        g.mode = map.view;
        let cw = g.cell_chars() as f32 + 1.0;
        let w = 130.0 + map.cols as f32 * cw * 7.3 + map.cols as f32 * 8.0;
        let h = 90.0 + map.rows as f32 * 15.0;
        let rect = self.next_rect(w.clamp(300.0, 1000.0), h.clamp(180.0, 620.0));
        self.children.push(Child { id, project, map_uid: Some(uid), rect, restore: None, minimized: false, g, connected: None, connect_offset: 0 });
        self.active = Some(id);
        self.active_project = Some(project);
    }

    pub fn close_child(&mut self, id: u64) {
        self.children.retain(|c| c.id != id);
        for c in &mut self.children {
            if c.connected == Some(id) {
                c.connected = None;
            }
        }
        if self.active == Some(id) {
            self.active = self.children.last().map(|c| c.id);
            if let Some(c) = self.children.last() {
                self.active_project = Some(c.project);
            }
        }
    }

    pub fn close_project_now(&mut self, id: u64) {
        let dead: Vec<u64> = self.children.iter().filter(|c| c.project == id).map(|c| c.id).collect();
        for d in dead {
            self.close_child(d);
        }
        if let Some(h) = self.searches.remove(&id) {
            h.cancel.store(true, Ordering::Relaxed);
        }
        self.projects.retain(|p| p.id != id);
        if self.active_project == Some(id) {
            self.active_project = self.projects.last().map(|p| p.id);
        }
    }

    pub fn tile(&mut self) {
        let vis: Vec<usize> = (0..self.children.len()).filter(|&i| !self.children[i].minimized).collect();
        if vis.is_empty() {
            return;
        }
        let n = vis.len();
        let cols = (n as f32).sqrt().ceil() as usize;
        let rows = n.div_ceil(cols);
        let s = self.workspace.size();
        let (w, h) = (s.x / cols as f32, s.y / rows as f32);
        for (k, &i) in vis.iter().enumerate() {
            let min = self.workspace.min + Vec2::new((k % cols) as f32 * w, (k / cols) as f32 * h);
            self.children[i].rect = Rect::from_min_size(min, Vec2::new(w, h));
            self.children[i].restore = None;
        }
    }

    pub fn cascade(&mut self) {
        let size = self.workspace.size() * 0.72;
        let min = self.workspace.min;
        for (k, c) in self.children.iter_mut().enumerate() {
            let o = (k % 12) as f32 * 24.0;
            c.rect = Rect::from_min_size(min + Vec2::splat(o), size);
            c.restore = None;
            c.minimized = false;
        }
    }

    // ---------- starting and ending projects ----------
    pub fn new_project_id(&mut self) -> u64 {
        let id = self.next_project;
        self.next_project += 1;
        id
    }

    pub fn adopt_project(&mut self, p: Project, ctx: &egui::Context, search_now: bool) {
        let id = p.id;
        self.projects.push(p);
        self.active_project = Some(id);
        self.open_hexdump(id);
        if search_now && self.cfg.search_potential {
            self.start_search(id, ctx);
        }
    }

    pub fn start_search(&mut self, id: u64, ctx: &egui::Context) {
        if self.searches.contains_key(&id) {
            return;
        }
        let Some(p) = self.project(id) else { return };
        if p.size() == 0 {
            return;
        }
        let h = analysis::spawn_search(id, p.data().to_vec(), p.maps.clone(), self.cfg.rules.clone(), self.search_tx.clone(), ctx.clone());
        self.searches.insert(id, h);
    }

    pub fn poll_search(&mut self) {
        while let Ok(m) = self.search_rx.try_recv() {
            match m {
                SearchMsg::Progress { percent, found } => {
                    self.search_status = format!("Map finder running: {percent}%, {found} found");
                }
                SearchMsg::Done { project, maps, overview, cancelled, recognized } => {
                    self.searches.remove(&project);
                    let n = maps.len();
                    if let Some(p) = self.project_mut(project) {
                        if !cancelled {
                            let mut maps = maps;
                            for m in &mut maps {
                                m.uid = p.uid();
                            }
                            // Maps that arrived while the search ran are no longer proposals.
                            let known = &p.maps;
                            maps.retain(|c| !known.iter().any(|k| k.start < c.end() && c.start < k.end()));
                            p.potential = maps;
                            p.search_done = true;
                            p.overview = Some(overview);
                        }
                    }
                    self.search_status = if cancelled {
                        "Map finder stopped".to_string()
                    } else {
                        format!("Map finder done: {n} found, {recognized} matched an axis preset")
                    };
                }
            }
        }
    }

    /// Takes in the first data of a project or a further version.
    pub fn take_data(&mut self, data: Vec<u8>, name: &str, target: Option<u64>, overwrite: bool, ctx: &egui::Context) {
        if let Some(pid) = target {
            if let Some(p) = self.project_mut(pid) {
                if p.versions.is_empty() {
                    p.add_version("", data);
                    self.after_first_data(pid, ctx);
                    return;
                }
                if overwrite {
                    p.begin_edit("Import");
                    let size = p.size();
                    let n = data.len().min(size);
                    p.write(0, &data[..n]);
                    p.end_edit();
                } else {
                    let v = p.add_version(name, data);
                    p.current = v;
                    p.revision += 1;
                    p.overview = None;
                }
                return;
            }
        }
        let id = self.new_project_id();
        let mut p = Project::new(id);
        p.props.file_name = format!("{}", self.free_name());
        p.add_version("", data);
        self.projects.push(p);
        self.active_project = Some(id);
        self.after_first_data(id, ctx);
        self.open_hexdump(id);
    }

    fn after_first_data(&mut self, id: u64, ctx: &egui::Context) {
        let vehicle = self.cfg.vehicle_on_import;
        let props = self.cfg.show_props_on_import;
        if let Some(p) = self.project_mut(id) {
            if vehicle {
                fill_vehicle_facts(p);
            }
            if p.props.file_name.is_empty() {
                p.props.file_name = format!("Project {id}");
            }
        }
        if self.children.iter().any(|c| c.project == id) {
            for c in self.children.iter_mut().filter(|c| c.project == id) {
                c.g.base = 0;
            }
        }
        if self.cfg.search_potential {
            self.start_search(id, ctx);
        }
        if props {
            self.pending.push(Cmd::PropsProject);
        }
    }

    /// Adds maps to a project, leaving out ones it already has at the same place and size and
    /// ones that reach past its data. Returns how many went in and how many were left out.
    pub fn add_maps(&mut self, pid: u64, maps: Vec<MapDef>) -> (usize, usize) {
        let Some(p) = self.project_mut(pid) else { return (0, maps.len()) };
        let d = p.data().to_vec();
        let (mut added, mut left) = (0, 0);
        for mut m in maps {
            let twin = p.maps.iter().any(|k| k.start == m.start && k.byte_len() == m.byte_len());
            if twin || m.end() > d.len() {
                left += 1;
                continue;
            }
            m.uid = p.uid();
            m.auto_range(&d);
            p.maps.push(m);
            added += 1;
        }
        if added > 0 {
            // A search that ran before these arrived may have proposed the same places.
            let maps = p.maps.clone();
            p.potential.retain(|c| !maps.iter().any(|k| k.start < c.end() && c.start < k.end()));
            p.dirty_struct = true;
        }
        (added, left)
    }

    pub fn free_name(&self) -> String {
        let dir = self.cfg.project_dir();
        let used: HashSet<String> = self.projects.iter().map(|p| p.props.file_name.clone()).collect();
        for n in self.cfg.namespace_from..=self.cfg.namespace_to.max(self.cfg.namespace_from) {
            let name = format!("{n}.alsproj");
            if !used.contains(&name) && !dir.join(&name).exists() {
                return name;
            }
        }
        "project.alsproj".to_string()
    }

    pub fn import_path(&mut self, path: &Path, target: Option<u64>, ctx: &egui::Context) {
        let id = self.next_project;
        match formats::load_any(path, id) {
            Ok(Loaded::Raw(d)) => {
                let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                self.take_data(d, &name, target, false, ctx);
            }
            Ok(Loaded::Blocks(b)) => {
                let (size, offset) = formats::suggest_size(&b);
                self.modal = Some(Modal::hex_import(b, size, offset, target));
            }
            Ok(Loaded::Project(p)) => {
                self.next_project += 1;
                self.cfg.add_recent(&path.to_string_lossy());
                let mut p = *p;
                let need = !p.search_done;
                if !need {
                    p.overview = Some(analysis::build_overview(p.data(), &p.maps, &p.potential));
                }
                self.adopt_project(p, ctx, need);
            }
            Ok(Loaded::MapPack(maps)) => {
                if self.current_project().is_none() {
                    self.tell("Import maps", "Please open the project the maps belong to first.");
                } else {
                    self.modal = Some(Modal::pack_import(maps));
                }
            }
            Ok(Loaded::A2l(t)) => {
                if self.current_project().is_none() {
                    self.tell("Import", "A description file can only go into an open project.");
                } else {
                    self.modal = Some(Modal::a2l(t));
                }
            }
            Ok(Loaded::Selection(s)) => {
                self.clip = Some(Clip { cells: s.data.iter().map(|&b| b as f64).collect(), cols: 16, org: DataOrg::Byte, text: String::new() });
                self.tell("Selection", format!("{} byte(s) from {:X} are ready to paste.", s.data.len(), s.addr));
            }
            Err(e) => self.tell("Import", e),
        }
    }

    pub fn save_project(&mut self, id: u64) -> bool {
        let dir = self.cfg.project_dir();
        let name = self.free_name();
        let auto = self.cfg.auto_names;
        let path = match self.project(id).and_then(|p| p.path.clone()) {
            Some(p) => Some(p),
            None if auto => {
                let _ = std::fs::create_dir_all(&dir);
                let fname = self.project(id).map(|p| p.props.file_name.clone()).filter(|n| n.ends_with(".alsproj") && !dir.join(n).exists()).unwrap_or(name);
                Some(dir.join(fname))
            }
            None => rfd::FileDialog::new().add_filter("Project", &["alsproj"]).set_directory(&dir).save_file(),
        };
        let Some(path) = path else { return false };
        let Some(p) = self.project_mut(id) else { return false };
        if path.exists() {
            let _ = std::fs::copy(&path, path.with_extension("alsproj.bak"));
        }
        let r = p.save(&path);
        match r {
            Ok(()) => {
                let s = path.to_string_lossy().to_string();
                self.cfg.add_recent(&s);
                true
            }
            Err(e) => {
                self.tell("Save", format!("The project couldn't be saved: {e}"));
                false
            }
        }
    }

    // ---------- values ----------
    /// Changes cells of the active window. `f` gets the stored value and the cell number.
    pub fn change_cells(&mut self, label: &str, cells: Option<Vec<usize>>, f: &dyn Fn(f64, usize, &Grid, &Project) -> f64) {
        let Some(ci) = self.active.and_then(|id| self.child_index(id)) else { return };
        let end = self.end_of(&self.children[ci]);
        let g = self.children[ci].g.clone();
        let Some(pi) = self.project_index(self.children[ci].project) else { return };
        let total = g.total(end);
        let mut cells = cells.unwrap_or_else(|| g.selected_cells(total));
        if self.cfg.visible_columns_only && g.sel_mode == SelMode::Columns && g.has_selection() {
            let (a, b) = (g.top_row * g.cols, (g.top_row + g.visible_rows.max(1)) * g.cols);
            cells.retain(|&i| i >= a && i < b);
        }
        let p = &self.projects[pi];
        // Work out every write first: protected memory may ask about each one.
        let mut writes = Vec::with_capacity(cells.len());
        for i in cells {
            let a = g.addr(i);
            let Some(old) = g.org.read(p.data(), a, g.signed) else { continue };
            let new = f(old, i, &g, p);
            let bytes = g.org.encode(new, g.signed);
            // Unchanged cells never count as changing protected memory.
            if p.data().get(a..a + bytes.len()) != Some(bytes.as_slice()) {
                writes.push((a, bytes, checksum::covered(p, a)));
            }
        }
        if p.protect_outside && writes.iter().any(|w| !w.2) {
            let mut run = checksum::ProtectRun { label: label.to_string(), writes, ..Default::default() };
            match run.next_question() {
                Some(addr) => self.modal = Some(Modal::Protect { run: Box::new(run), addr }),
                None => self.commit_protected(pi, &run),
            }
            return;
        }
        let p = &mut self.projects[pi];
        p.begin_edit(label);
        for (a, bytes, _) in &writes {
            p.write(*a, bytes);
        }
        p.end_edit();
    }

    pub fn commit_protected(&mut self, pi: usize, run: &checksum::ProtectRun) {
        let Some(p) = self.projects.get_mut(pi) else { return };
        if run.turn_off {
            p.protect_outside = false;
        }
        p.begin_edit(&run.label);
        for (a, bytes) in &run.accepted {
            p.write(*a, bytes);
        }
        p.end_edit();
    }

    pub fn step_value(&mut self, dir: i32) {
        let Some(c) = self.active_child() else { return };
        let g = &c.g;
        let step = match g.width() {
            1 => self.cfg.step8,
            2 => self.cfg.step16,
            _ => self.cfg.step32,
        } as f64;
        // Holding the key makes it go faster and faster.
        let now = std::time::Instant::now();
        let held = now.duration_since(self.ctx_repeat.0).as_millis() < 150;
        self.ctx_repeat = (now, if held { self.ctx_repeat.1 + 1 } else { 0 });
        let accel = match self.ctx_repeat.1 {
            0..=19 => 1.0,
            20..=59 => 10.0,
            _ => 100.0,
        };
        let shown = self.cfg.edit_shown_values;
        let delta = step * accel * dir as f64;
        self.change_cells("Value +/-", None, &move |old, _, g, _| {
            if shown && g.has_factor() {
                let unit = 10f64.powi(-(g.precision as i32));
                let v = g.scaled(old) + unit * delta;
                let raw = g.unscaled(v);
                // A change too small for the stored number still moves it by one.
                if raw.round() == old.round() {
                    old + delta
                } else {
                    raw
                }
            } else {
                old + delta
            }
        });
        self.last_change = Some(LastChange::Step(dir));
    }

    pub fn apply_change(&mut self, ch: &LastChange) {
        match ch.clone() {
            LastChange::Step(d) => self.step_value(d),
            LastChange::Original => self.restore_original(),
            LastChange::Absolute(v) => {
                self.change_cells("Change absolute", None, &move |_, _, g, _| g.unscaled(v));
            }
            LastChange::Relative { from_original, percent, w } => {
                let shown = self.cfg.edit_shown_values;
                self.change_cells("Change relative", None, &move |old, i, g, p| {
                    let x = if from_original { p.ref_value(g.org, g.addr(i), g.signed).unwrap_or(old) } else { old };
                    relative(x, w, percent, shown, g)
                });
            }
        }
        self.last_change = Some(ch.clone());
    }

    pub fn restore_original(&mut self) {
        self.change_cells("Original value", None, &|old, i, g, p| p.ref_value(g.org, g.addr(i), g.signed).unwrap_or(old));
        self.last_change = Some(LastChange::Original);
    }

    /// Moves every selected value by a number on the shown scale, for dragging in 2d and 3d.
    pub fn drag_values(&mut self, stored_delta: f64) {
        self.change_cells("Drag", None, &move |old, _, _, _| old + stored_delta);
    }

    /// Text for the status bar: where the cursor is, its value, the original value and the change.
    pub fn cursor_status(&self) -> String {
        let Some(c) = self.active_child() else { return String::new() };
        let Some(p) = self.project(c.project) else { return String::new() };
        let g = &c.g;
        let a = g.addr(g.cursor);
        let v = g.org.read(p.data(), a, g.signed);
        let o = p.ref_value(g.org, a, g.signed);
        match (v, o) {
            (Some(v), Some(o)) => {
                let d = v - o;
                let pct = if o != 0.0 { d / o * 100.0 } else { 0.0 };
                format!("At {a:05X}: {} (was {}), change {} ({pct:.2}%), {} per row", g.format(g.scaled(v)), g.format(g.scaled(o)), fmt_delta(d), g.cols)
            }
            _ => format!("At {a:05X}"),
        }
    }

    // ---------- clipboard ----------
    pub fn clip_from_selection(&mut self) -> Option<Clip> {
        let c = self.active_child()?;
        let p = self.project(c.project)?;
        let g = &c.g;
        let total = g.total(self.end_of(c));
        let cells = g.selected_cells(total);
        let (cols, values): (usize, Vec<f64>) = match g.sel_rect() {
            Some((f, w, h)) if g.has_selection() && w * h == cells.len() => {
                (w, (0..h).flat_map(|r| (0..w).map(move |k| f + r * g.cols + k)).filter_map(|i| g.raw(p, i)).collect())
            }
            _ => (g.cols, cells.iter().filter_map(|&i| g.raw(p, i)).collect()),
        };
        let mut text = String::new();
        for (k, v) in values.iter().enumerate() {
            if k > 0 {
                text.push(if k % cols.max(1) == 0 { '\n' } else { '\t' });
            }
            text.push_str(&g.format(g.scaled(*v)));
        }
        Some(Clip { cells: values, cols, org: g.org, text })
    }

    pub fn paste(&mut self, ctx: &egui::Context) {
        let Some(clip) = self.clip.clone() else {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            return;
        };
        let Some(c) = self.active_child() else { return };
        let g = c.g.clone();
        let total = g.total(self.end_of(c));
        let start = g.cursor;
        let cols = clip.cols.max(1);
        let mut cells = vec![];
        for (k, _) in clip.cells.iter().enumerate() {
            let i = start + (k / cols) * g.cols + k % cols;
            if i < total {
                cells.push(i);
            }
        }
        let map: HashMap<usize, f64> = cells.iter().copied().zip(clip.cells.iter().copied()).collect();
        self.change_cells("Paste", Some(cells), &move |old, i, _, _| map.get(&i).copied().unwrap_or(old));
    }

    pub fn paste_text(&mut self, text: &str) {
        let Some(c) = self.active_child() else { return };
        let g = c.g.clone();
        if let Some(clip) = &self.clip {
            if clip.text.trim() == text.trim() {
                let ctx_clip = clip.clone();
                let start = g.cursor;
                let total = g.total(self.end_of(self.active_child().expect("active")));
                let cols = ctx_clip.cols.max(1);
                let mut cells = vec![];
                for (k, _) in ctx_clip.cells.iter().enumerate() {
                    let i = start + (k / cols) * g.cols + k % cols;
                    if i < total {
                        cells.push(i);
                    }
                }
                let map: HashMap<usize, f64> = cells.iter().copied().zip(ctx_clip.cells.iter().copied()).collect();
                self.change_cells("Paste", Some(cells), &move |old, i, _, _| map.get(&i).copied().unwrap_or(old));
                return;
            }
        }
        // Numbers from another program, in rows and columns.
        let mut rows: Vec<Vec<f64>> = vec![];
        for line in text.lines() {
            let mut r = vec![];
            for t in line.split(|c: char| c == '\t' || c == ';' || c == ' ' || c == ',').filter(|t| !t.is_empty()) {
                let v = if g.radix == 16 && !g.has_factor() {
                    u64::from_str_radix(t.trim_start_matches("0x"), 16).ok().map(|x| x as f64)
                } else {
                    t.parse::<f64>().ok()
                };
                let Some(v) = v else {
                    self.tell("Paste", format!("\"{t}\" isn't a number."));
                    return;
                };
                r.push(g.unscaled(v));
            }
            if !r.is_empty() {
                rows.push(r);
            }
        }
        let total = g.total(self.end_of(self.active_child().expect("active")));
        let mut map = HashMap::new();
        for (rn, r) in rows.iter().enumerate() {
            for (cn, v) in r.iter().enumerate() {
                let i = g.cursor + rn * g.cols + cn;
                if i < total {
                    map.insert(i, *v);
                }
            }
        }
        let cells: Vec<usize> = map.keys().copied().collect();
        self.change_cells("Paste", Some(cells), &move |old, i, _, _| map.get(&i).copied().unwrap_or(old));
    }

    pub fn map_text(&self, c: &Child, p: &Project, m: &MapDef, x: bool, y: bool, values: bool) -> String {
        let d = p.data();
        let mut s = String::new();
        if values && (x || y) {
            s.push_str(&format!("{}\r\n", m.name));
        }
        let xs = m.x.values(d, m.cols);
        let ys = m.y.values(d, m.rows);
        if x {
            if y && values {
                s.push('\t');
            }
            let line: Vec<String> = xs.iter().map(|v| m.x.text(*v)).collect();
            s.push_str(&line.join("\t"));
            s.push_str("\r\n");
        }
        if values {
            for r in 0..m.rows {
                if y {
                    s.push_str(&m.y.text(ys[r]));
                    s.push('\t');
                }
                let line: Vec<String> = (0..m.cols).map(|k| c.g.format(c.g.shown(p, r * m.cols + k, None).unwrap_or(0.0))).collect();
                s.push_str(&line.join("\t"));
                s.push_str("\r\n");
            }
        } else if y {
            let line: Vec<String> = ys.iter().map(|v| m.y.text(*v)).collect();
            s.push_str(&line.join("\r\n"));
            s.push_str("\r\n");
        }
        s
    }

    // ---------- moving around ----------
    pub fn remember_position(&mut self, addr: usize) {
        let h = &mut self.list.history;
        let t = format!("{addr:X}");
        h.retain(|x| *x != t);
        h.insert(0, t);
        h.truncate(16);
    }

    pub fn goto_addr(&mut self, addr: usize) {
        let Some(c) = self.active_child_mut() else { return };
        c.g.cursor = c.g.cell_of(addr);
        c.g.follow = true;
    }

    pub fn fit_potential(&mut self, i: usize) {
        let Some(pid) = self.active_project else { return };
        let Some(m) = self.project(pid).and_then(|p| p.potential.get(i)).cloned() else { return };
        self.fit_to(&m);
    }

    /// Sets the hexdump so that a map sits on its rows, and selects it.
    pub fn fit_to(&mut self, m: &MapDef) {
        let range = self.current_project().map(|p| p.element_range()).unwrap_or((0, 0));
        let Some(c) = self.active_child_mut() else { return };
        if c.map_uid.is_some() {
            return;
        }
        let w = m.org.width();
        c.g.org = m.org;
        c.g.cols = m.cols.max(1);
        let row_bytes = m.cols * w;
        c.g.base = range.0 + (m.start - range.0.min(m.start)) % row_bytes.max(1);
        let cell = c.g.cell_of(m.start);
        c.g.sel_mode = SelMode::Rectangle;
        c.g.sel = Some(Sel::new(cell, cell + m.cells().saturating_sub(1)));
        c.g.cursor = cell;
        c.g.range_lo = m.range_lo;
        c.g.range_hi = m.range_hi;
        c.g.follow = true;
        c.g.top_row = (cell / m.cols.max(1)).saturating_sub(2);
        c.g.radix = if m.org.width() == 1 { c.g.radix } else { 10 };
    }

    pub fn potential_to_map(&mut self, i: usize) -> Option<u64> {
        let pid = self.active_project?;
        let p = self.project_mut(pid)?;
        if i >= p.potential.len() {
            return None;
        }
        let mut m = p.potential.remove(i);
        m.uid = p.uid();
        m.folder = "Maps".into();
        m.name = m.potential_label.take().unwrap_or_else(|| "Map".into());
        let uid = m.uid;
        p.maps.push(m);
        p.dirty_struct = true;
        self.open_map(pid, uid);
        Some(uid)
    }

    pub fn next_diff(&mut self, how: DiffMove) {
        let Some(ci) = self.active.and_then(|id| self.child_index(id)) else { return };
        let end = self.end_of(&self.children[ci]);
        let Some(pi) = self.project_index(self.children[ci].project) else { return };
        let p = &self.projects[pi];
        let c = &self.children[ci];
        let g = &c.g;
        let total = g.total(end);
        let is = |i: usize| p.is_changed(g.org, g.addr(i), g.signed);
        let found = match how {
            DiffMove::First => (0..total).find(|&i| is(i)),
            DiffMove::Last => (0..total).rev().find(|&i| is(i)),
            DiffMove::Next => (g.cursor + 1..total).find(|&i| is(i)),
            DiffMove::Prev => (0..g.cursor).rev().find(|&i| is(i)),
        };
        if let Some(i) = found {
            let c = &mut self.children[ci];
            c.g.cursor = i;
            c.g.follow = true;
        }
    }

    /// Cells of the active window that differ, for the differences window.
    pub fn diff_list(&self, from: usize, max: usize) -> Vec<(usize, f64, f64)> {
        let mut out = vec![];
        let Some(c) = self.active_child() else { return out };
        let Some(p) = self.project(c.project) else { return out };
        let g = &c.g;
        let total = g.total(self.end_of(c));
        let first = g.cell_of(from);
        for i in first..total {
            let a = g.addr(i);
            if p.is_changed(g.org, a, g.signed) {
                let v = g.org.read(p.data(), a, g.signed).unwrap_or(0.0);
                let o = p.ref_value(g.org, a, g.signed).unwrap_or(0.0);
                out.push((a, v, o));
                if out.len() >= max {
                    break;
                }
            }
        }
        out
    }

    // ---------- search ----------
    pub fn build_query(&self) -> Result<search::Query, String> {
        let f = &self.find;
        let c = self.active_child().ok_or("No window is open.")?;
        let p = self.project(c.project).ok_or("No project is open.")?;
        let (start, end) = p.element_range();
        let parse_addr = |s: &str, d: usize| -> Result<usize, String> {
            let t = s.trim();
            if t.is_empty() {
                Ok(d)
            } else {
                usize::from_str_radix(t.trim_start_matches("0x"), 16).map_err(|_| format!("\"{t}\" isn't an address."))
            }
        };
        let num = |s: &str| s.trim().parse::<f64>().unwrap_or(0.0);
        let mut q = search::Query {
            org: c.g.org,
            from: parse_addr(&f.from, start)?,
            to: parse_addr(&f.to, end.saturating_sub(1))?,
            up: num(&f.up),
            down: num(&f.down),
            align: f.align,
            match_case: f.match_case,
            skip_code: f.skip_code,
            ..Default::default()
        };
        if f.tab == search::Radix::Text {
            q.text = Some(search::parse_text(&f.text));
            q.org = DataOrg::Byte;
        } else {
            q.cells = search::parse_pattern(&f.text, f.tab)?;
        }
        Ok(q)
    }

    pub fn search_data<'a>(&'a self, p: &'a Project) -> &'a [u8] {
        if self.find.in_original {
            p.ref_data()
        } else {
            p.data()
        }
    }

    pub fn find_step(&mut self, forward: bool, restart: bool) {
        if self.find.similar.is_some() {
            self.similar_step(forward);
            return;
        }
        let q = match self.build_query() {
            Ok(q) => q,
            Err(e) => {
                self.find.status = e;
                return;
            }
        };
        let Some(c) = self.active_child() else { return };
        let Some(p) = self.project(c.project) else { return };
        let cursor = c.g.addr(c.g.cursor);
        let d = self.search_data(p);
        let from = if restart {
            q.from
        } else if forward {
            cursor + 1
        } else {
            cursor
        };
        let found = search::find_next(d, &q, from, forward);
        match found {
            Some(a) => {
                let n = q.text.as_ref().map(|t| t.len()).unwrap_or(q.cells.len() * q.org.width());
                self.find.status = format!("Found at {a:X}.");
                if let Some(c) = self.active_child_mut() {
                    let w = c.g.width();
                    c.g.cursor = c.g.cell_of(a);
                    c.g.follow = true;
                    c.g.sel_mode = SelMode::Consecutive;
                    let last = c.g.cell_of(a + n.max(1) - 1);
                    let _ = w;
                    c.g.sel = Some(Sel::new(c.g.cursor, last));
                }
            }
            None => self.find.status = "no search results".into(),
        }
    }

    pub fn find_list(&mut self) {
        const MAX: usize = 200;
        let q = match self.build_query() {
            Ok(q) => q,
            Err(e) => {
                self.find.status = e;
                return;
            }
        };
        let Some(c) = self.active_child() else { return };
        let Some(p) = self.project(c.project) else { return };
        let ov = p.overview.clone();
        let is_code = move |a: usize| ov.as_ref().is_some_and(|o| o.kind_at(a) == Some(analysis::AreaKind::Code));
        let r = search::find_all(self.search_data(p), &q, MAX, Some(&is_code));
        self.find.status = if r.is_empty() {
            "no search results".to_string()
        } else if r.len() >= MAX {
            format!("Search stopped after {MAX} occurrences.")
        } else {
            format!("Search finished. {} occurrences found.", r.len())
        };
        self.find.results = r;
        self.find.show_list = true;
    }

    /// Searches an area that resembles the selected values.
    pub fn similar_step(&mut self, forward: bool) {
        let Some(block) = self.find.similar.clone() else { return };
        let Some(c) = self.active_child() else { return };
        let Some(p) = self.project(c.project) else { return };
        let g = &c.g;
        let cursor = g.addr(g.cursor);
        let w = g.width();
        let all = search::find_similar(p.data(), &block, g.org, self.find.tolerance, 200, None);
        let mut addrs: Vec<usize> = all.iter().map(|h| h.0).collect();
        addrs.sort_unstable();
        let hit = if forward {
            addrs.iter().find(|&&a| a > cursor).copied()
        } else {
            addrs.iter().rev().find(|&&a| a < cursor).copied()
        };
        self.find.results = addrs;
        self.find.show_list = true;
        match hit {
            Some(a) => {
                let n = block.len();
                self.find.status = format!("Found at {a:X} ({} similar places).", self.find.results.len());
                if let Some(c) = self.active_child_mut() {
                    c.g.cursor = c.g.cell_of(a);
                    c.g.follow = true;
                    c.g.sel_mode = SelMode::Consecutive;
                    c.g.sel = Some(Sel::new(c.g.cursor, c.g.cursor + n - 1));
                }
                let _ = w;
            }
            None => self.find.status = "no search results".into(),
        }
    }

    // ---------- building maps ----------
    /// Makes a map from a rectangle in the active hexdump. Unless shift is held, axes that sit
    /// directly before the values are recognised.
    pub fn selection_to_map(&mut self, detect: bool) {
        let Some(ci) = self.active.and_then(|id| self.child_index(id)) else { return };
        if self.children[ci].map_uid.is_some() {
            return;
        }
        let g = self.children[ci].g.clone();
        let Some((first, w_cells, h_cells)) = g.sel_rect() else {
            self.tell("Make a map", "Please select a rectangle of cells first.");
            return;
        };
        let pid = self.children[ci].project;
        let Some(p) = self.project_mut(pid) else { return };
        let uid = p.uid();
        let start = g.addr(first);
        // A rectangle whose rows don't touch can't become a map with these columns.
        let same_columns = w_cells == g.cols || h_cells == 1;
        let (cols, rows) = if same_columns { (w_cells, h_cells) } else { (w_cells * h_cells, 1) };
        let mut m = MapDef {
            uid,
            name: format!("Map ({start:05X})"),
            start,
            cols,
            rows,
            org: g.org,
            radix: g.radix,
            signed: g.signed,
            right: g.right,
            kind: if rows > 1 { MapKind::TwoDim } else { MapKind::OneDim },
            ..MapDef::default()
        };
        let d = p.data().to_vec();
        m.auto_range(&d);
        if detect {
            // A selected potential map already knows its axes, and stops being only potential.
            if let Some(i) = p.potential.iter().position(|q| q.start == m.start && q.cols == m.cols && q.rows == m.rows && q.org == m.org) {
                let q = p.potential.remove(i);
                m.x = q.x;
                m.y = q.y;
                m.kind = q.kind;
            } else {
                detect_axes(&mut m, &d);
            }
        }
        p.maps.push(m);
        p.dirty_struct = true;
        self.open_map(pid, uid);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DiffMove {
    First,
    Prev,
    Next,
    Last,
}

pub fn relative(x: f64, w: f64, percent: bool, shown: bool, g: &Grid) -> f64 {
    if shown && g.has_factor() {
        let s = g.scaled(x);
        let v = if percent { s + s * w / 100.0 } else { s + w };
        g.unscaled(v)
    } else if percent {
        x + x * w / 100.0
    } else {
        x + w
    }
}

/// Bilinear share of the four corner parameters for a cell, plus the middle one for all cells.
pub fn edit_weights(col: f64, row: f64, cols: f64, rows: f64, p: [f64; 5]) -> f64 {
    let u = if cols > 1.0 { col / (cols - 1.0) } else { 0.5 };
    let v = if rows > 1.0 { row / (rows - 1.0) } else { 0.5 };
    let corners = p[0] * (1.0 - u) * (1.0 - v) + p[1] * u * (1.0 - v) + p[2] * (1.0 - u) * v + p[3] * u * v;
    p[4] + corners
}

pub fn fmt_delta(d: f64) -> String {
    if d.fract() == 0.0 {
        format!("{}", d as i64)
    } else {
        format!("{d:.3}")
    }
}

pub fn fill_vehicle_facts(p: &mut Project) {
    let f = analysis::search_vehicle_data(p.data());
    let set = |slot: &mut String, v: Option<String>| {
        if slot.is_empty() {
            if let Some(v) = v {
                *slot = v;
            }
        }
    };
    set(&mut p.props.ecu_producer, f.ecu_producer);
    set(&mut p.props.ecu_prod, f.ecu_prod);
    set(&mut p.props.sw_version, f.sw_version);
    set(&mut p.props.ecu_build, f.ecu_build);
    if p.props.processor.is_empty() {
        if let Some(g) = analysis::guess_processor(p.data()) {
            p.props.processor = g.to_string();
        }
    }
}

/// Looks right before the values for ascending axes: [y axis][x axis][values].
pub fn detect_axes(m: &mut MapDef, d: &[u8]) {
    let w = m.org.width();
    let rising = |at: usize, n: usize| -> bool {
        n >= 2
            && (0..n).all(|i| m.org.read(d, at + i * w, false).is_some())
            && (1..n).all(|i| m.org.read(d, at + i * w, false) > m.org.read(d, at + (i - 1) * w, false))
    };
    let org = m.org;
    let axis = |addr: usize| Axis { src: AxisSrc::Rom, addr, org, ..Axis::default() };
    // [y axis][x axis][values], the order seen in real map packs; a count may sit between them.
    if let Some(x) = m.start.checked_sub(m.cols * w) {
        if rising(x, m.cols) {
            m.x = axis(x);
            for gap in [0, 1] {
                if let Some(y) = x.checked_sub((m.rows + gap) * w) {
                    if rising(y, m.rows) {
                        m.y = axis(y);
                        break;
                    }
                }
            }
            return;
        }
    }
    // [x axis][y axis][values]
    if let Some(y) = m.start.checked_sub(m.rows * w) {
        if rising(y, m.rows) {
            if let Some(x) = y.checked_sub(m.cols * w) {
                if rising(x, m.cols) {
                    m.x = axis(x);
                    m.y = axis(y);
                }
            }
        }
    }
}

/// Sets and looks at the address rule: a leading 0d is decimal, a name finds a map or
/// comment, and binary-and folds addresses outside the project into it.
pub fn parse_goto(text: &str, p: &Project, binary_and: bool) -> Result<usize, String> {
    let t = text.trim();
    let low = t.to_ascii_lowercase();
    let size = p.size().max(1);
    let v = if let Some(d) = low.strip_prefix("0d") {
        d.parse::<usize>().ok()
    } else {
        usize::from_str_radix(low.trim_start_matches("0x"), 16).ok()
    };
    if let Some(v) = v {
        let a = if binary_and { v & (size.next_power_of_two() - 1) } else { v };
        return Ok(a);
    }
    if let Some(m) = p.maps.iter().find(|m| m.name.to_lowercase().contains(&low)) {
        return Ok(m.start);
    }
    if let Some(c) = p.comments.iter().find(|c| c.text.to_lowercase().contains(&low)) {
        return Ok(c.addr);
    }
    Err(format!("\"{t}\" is neither an address nor the name of a map or comment."))
}

pub fn env_for<'a>(app: &'a App, c: &Child, p: &'a Project) -> Env<'a> {
    let map = c.map_uid.and_then(|u| p.maps.iter().find(|m| m.uid == u));
    let var = map.and_then(|m| m.var_offset).and_then(|u| p.maps.iter().find(|m| m.uid == u));
    Env { p, map, var, theme: &app.cfg.theme, cfg: &app.cfg, active: app.active == Some(c.id), end: p.element_range().1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(data: Vec<u8>) -> Project {
        let mut p = Project::new(1);
        p.add_version("", data);
        p
    }

    fn g16() -> Grid {
        Grid { org: DataOrg::Byte, ..Grid::default() }
    }

    #[test]
    fn relative_operations_as_the_manual_gives_them() {
        let g = g16();
        assert_eq!(relative(1000.0, 1000.0, false, false, &g), 2000.0);
        assert_eq!(relative(1000.0, -10.0, true, false, &g), 900.0);
        let mut f = g16();
        f.factor = 0.5;
        // On shown values: 1000 shows 500, plus 100 is 600, stored as 1200.
        assert_eq!(relative(1000.0, 100.0, false, true, &f), 1200.0);
    }

    #[test]
    fn corner_parameters_weigh_by_position() {
        let p = [10.0, 0.0, 0.0, 0.0, 1.0];
        assert_eq!(edit_weights(0.0, 0.0, 5.0, 5.0, p), 11.0, "upper left gets it all");
        assert_eq!(edit_weights(4.0, 4.0, 5.0, 5.0, p), 1.0, "lower right only the middle value");
        assert_eq!(edit_weights(2.0, 2.0, 5.0, 5.0, p), 1.0 + 10.0 * 0.25);
    }

    #[test]
    fn goto_rules() {
        let mut p = project(vec![0; 0x80000]);
        let uid = p.uid();
        p.maps.push(MapDef { uid, name: "Boost limit".into(), start: 0x1234, ..MapDef::default() });
        p.comments.push(Comment { addr: 0x55, end: 0x55, text: "checksum area".into(), color: [0; 3] });
        assert_eq!(parse_goto("1000", &p, false), Ok(0x1000));
        assert_eq!(parse_goto("0d100", &p, false), Ok(100));
        assert_eq!(parse_goto("0x10", &p, false), Ok(0x10));
        assert_eq!(parse_goto("boost", &p, false), Ok(0x1234));
        assert_eq!(parse_goto("checksum", &p, false), Ok(0x55));
        // For example: 1E02E8 in a 7FFFF file is 602E8.
        assert_eq!(parse_goto("1E02E8", &p, true), Ok(0x602E8));
        assert!(parse_goto("nowhere", &p, false).is_err());
    }

    #[test]
    fn axes_found_before_the_values() {
        let org = DataOrg::Byte;
        let mut d = vec![0u8; 64];
        d[10..13].copy_from_slice(&[5, 6, 7]); // y
        d[13..17].copy_from_slice(&[1, 2, 3, 4]); // x
        d[17..29].copy_from_slice(&[9, 9, 9, 9, 8, 8, 8, 8, 7, 7, 7, 7]);
        let mut m = MapDef { start: 17, cols: 4, rows: 3, org, ..MapDef::default() };
        detect_axes(&mut m, &d);
        assert_eq!((m.x.src, m.x.addr), (AxisSrc::Rom, 13));
        assert_eq!((m.y.src, m.y.addr), (AxisSrc::Rom, 10));
    }

    #[test]
    fn axes_found_in_the_other_order_too() {
        let mut d = vec![0u8; 64];
        d[10..14].copy_from_slice(&[5, 6, 7, 8]); // x
        d[14..17].copy_from_slice(&[1, 2, 3]); // y
        d[17..29].copy_from_slice(&[9, 9, 9, 9, 8, 8, 8, 8, 7, 7, 7, 7]);
        let mut m = MapDef { start: 17, cols: 4, rows: 3, org: DataOrg::Byte, ..MapDef::default() };
        detect_axes(&mut m, &d);
        assert_eq!((m.x.src, m.x.addr), (AxisSrc::Rom, 10));
        assert_eq!((m.y.src, m.y.addr), (AxisSrc::Rom, 14));
    }

    #[test]
    fn no_axes_when_nothing_rises() {
        let d = vec![3u8; 64];
        let mut m = MapDef { start: 32, cols: 4, rows: 3, org: DataOrg::Byte, ..MapDef::default() };
        detect_axes(&mut m, &d);
        assert_eq!(m.x.src, AxisSrc::Enumerate);
    }

    #[test]
    fn vehicle_facts_never_overwrite_what_was_typed() {
        let mut p = project(b"0281010000 1037360000".to_vec());
        p.props.ecu_producer = "My own".into();
        fill_vehicle_facts(&mut p);
        assert_eq!(p.props.ecu_producer, "My own");
        assert_eq!(p.props.sw_version, "360000");
    }
}
