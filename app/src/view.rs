//! The data windows: text with bars, the 2d line, the 3d surface and everything that selects.
use crate::cmd::Cmd;
use crate::config::Config;
use crate::model::*;
use crate::theme::{c, pale, Theme};
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SelMode {
    #[default]
    Consecutive,
    Rectangle,
    Columns,
    Rows,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Sel {
    pub anchor: usize,
    pub head: usize,
}

impl Sel {
    pub fn new(anchor: usize, head: usize) -> Sel {
        Sel { anchor, head }
    }
    fn lo(&self) -> usize {
        self.anchor.min(self.head)
    }
    fn hi(&self) -> usize {
        self.anchor.max(self.head)
    }
    fn contains(&self, i: usize, cols: usize, mode: SelMode) -> bool {
        let (lo, hi) = (self.lo(), self.hi());
        match mode {
            SelMode::Consecutive => i >= lo && i <= hi,
            SelMode::Rows => i / cols >= lo / cols && i / cols <= hi / cols,
            SelMode::Columns => {
                let (a, b) = (self.anchor % cols, self.head % cols);
                let col = i % cols;
                col >= a.min(b) && col <= a.max(b)
            }
            SelMode::Rectangle => {
                let (a, b) = (self.anchor % cols, self.head % cols);
                let col = i % cols;
                let (r1, r2) = (self.anchor / cols, self.head / cols);
                let row = i / cols;
                col >= a.min(b) && col <= a.max(b) && row >= r1.min(r2) && row <= r1.max(r2)
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Grid {
    pub mode: ViewMode,
    pub base: usize,
    pub cols: usize,
    pub rows_fixed: Option<usize>,
    pub org: DataOrg,
    pub radix: u32,
    pub signed: bool,
    pub reciprocal: bool,
    pub factor: f64,
    pub offset: f64,
    pub precision: u32,
    pub no_factor: bool,
    pub delta: bool,
    pub percent: bool,
    pub show_orig: bool,
    pub right: RightSide,
    pub fix_right: bool,
    pub range_lo: f64,
    pub range_hi: f64,
    pub dynamic: bool,
    pub font: f32,
    pub cursor: usize,
    pub top_row: usize,
    pub sel: Option<Sel>,
    pub extra: Vec<Sel>,
    pub last_sel: Option<Sel>,
    pub sel_mode: SelMode,
    pub sel_begin: Option<usize>,
    pub follow: bool,
    pub zoom_x: f32,
    pub zoom_y: f32,
    pub dots: bool,
    pub row_markers: bool,
    pub rot: f32,
    pub persp: f32,
    pub zscale: f32,
    pub zoffset: f32,
    pub fill: bool,
    pub show_axis: bool,
    pub drag_kind: DragKind,
    pub visible_rows: usize,
    pub inplace: Option<String>,
    pub drag_total: f64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DragKind {
    #[default]
    None,
    Select,
    Columns,
    Rotate,
    Values,
    Scroll,
}

impl Default for Grid {
    fn default() -> Self {
        Grid {
            mode: ViewMode::Text,
            base: 0,
            cols: 16,
            rows_fixed: None,
            org: DataOrg::Byte,
            radix: 16,
            signed: false,
            reciprocal: false,
            factor: 1.0,
            offset: 0.0,
            precision: 0,
            no_factor: false,
            delta: false,
            percent: false,
            show_orig: false,
            right: RightSide::Bars,
            fix_right: false,
            range_lo: 0.0,
            range_hi: 255.0,
            dynamic: false,
            font: 12.0,
            cursor: 0,
            top_row: 0,
            sel: None,
            extra: vec![],
            last_sel: None,
            sel_mode: SelMode::Consecutive,
            sel_begin: None,
            follow: false,
            zoom_x: 2.0,
            zoom_y: 1.0,
            dots: true,
            row_markers: true,
            rot: 0.6,
            persp: 0.35,
            zscale: 1.0,
            zoffset: 0.0,
            fill: false,
            show_axis: true,
            drag_kind: DragKind::None,
            visible_rows: 30,
            inplace: None,
            drag_total: 0.0,
        }
    }
}

impl Grid {
    pub fn width(&self) -> usize {
        self.org.width()
    }
    pub fn addr(&self, i: usize) -> usize {
        self.base + i * self.width()
    }
    pub fn cell_of(&self, addr: usize) -> usize {
        addr.saturating_sub(self.base) / self.width()
    }
    /// How many cells the window can show given where the data ends.
    pub fn total(&self, end: usize) -> usize {
        if let Some(r) = self.rows_fixed {
            r * self.cols
        } else {
            end.saturating_sub(self.base) / self.width()
        }
    }
    pub fn is_selected(&self, i: usize) -> bool {
        self.sel.is_some_and(|s| s.contains(i, self.cols, self.sel_mode))
            || self.extra.iter().any(|s| s.contains(i, self.cols, self.sel_mode))
    }
    pub fn has_selection(&self) -> bool {
        self.sel.is_some()
    }
    /// Selected cells in order. Without a selection, the cursor cell.
    pub fn selected_cells(&self, total: usize) -> Vec<usize> {
        let mut out = vec![];
        let Some(s) = self.sel else {
            if self.cursor < total {
                out.push(self.cursor);
            }
            return out;
        };
        let (mut lo, mut hi) = (s.lo(), s.hi());
        for e in &self.extra {
            lo = lo.min(e.lo());
            hi = hi.max(e.hi());
        }
        if matches!(self.sel_mode, SelMode::Columns) {
            lo = 0;
            hi = total.saturating_sub(1);
        }
        if matches!(self.sel_mode, SelMode::Rows) {
            lo = lo / self.cols * self.cols;
            hi = (hi / self.cols + 1) * self.cols - 1;
        }
        for i in lo..=hi.min(total.saturating_sub(1)) {
            if self.is_selected(i) {
                out.push(i);
            }
        }
        out
    }
    /// The bounding rectangle of the selection, as (first cell, columns, rows).
    pub fn sel_rect(&self) -> Option<(usize, usize, usize)> {
        let s = self.sel?;
        let (a, b) = (s.anchor % self.cols, s.head % self.cols);
        let (r1, r2) = (s.anchor / self.cols, s.head / self.cols);
        let (c1, c2) = (a.min(b), a.max(b));
        let (r1, r2) = (r1.min(r2), r1.max(r2));
        match self.sel_mode {
            SelMode::Rectangle => Some((r1 * self.cols + c1, c2 - c1 + 1, r2 - r1 + 1)),
            SelMode::Rows => Some((r1 * self.cols, self.cols, r2 - r1 + 1)),
            SelMode::Consecutive if r1 == r2 => Some((s.lo(), s.hi() - s.lo() + 1, 1)),
            SelMode::Consecutive => Some((r1 * self.cols, self.cols, r2 - r1 + 1)),
            SelMode::Columns => None,
        }
    }
    pub fn clear_sel(&mut self) {
        if self.sel.is_some() {
            self.last_sel = self.sel;
        }
        self.sel = None;
        self.extra.clear();
    }
    /// Value as stored, from the data or the reference when "original values" is on.
    pub fn raw(&self, p: &Project, i: usize) -> Option<f64> {
        let a = self.addr(i);
        if self.show_orig {
            p.ref_value(self.org, a, self.signed)
        } else {
            self.org.read(p.data(), a, self.signed)
        }
    }
    pub fn has_factor(&self) -> bool {
        !self.no_factor && (self.factor != 1.0 || self.offset != 0.0 || self.reciprocal)
    }
    /// Stored number to what the person reads: reciprocal, then value*factor + offset.
    pub fn scaled(&self, raw: f64) -> f64 {
        let r = if self.reciprocal && raw != 0.0 && !self.no_factor { 1.0 / raw } else { raw };
        if self.no_factor {
            r
        } else {
            r * self.factor + self.offset
        }
    }
    /// What the person typed back to the stored number.
    pub fn unscaled(&self, shown: f64) -> f64 {
        if self.no_factor {
            return shown;
        }
        let r = if self.factor != 0.0 { (shown - self.offset) / self.factor } else { shown };
        if self.reciprocal && r != 0.0 {
            1.0 / r
        } else {
            r
        }
    }
    /// The number on screen for a cell, honouring percent, delta and the offset map.
    pub fn shown(&self, p: &Project, i: usize, var: Option<&MapDef>) -> Option<f64> {
        let raw = self.raw(p, i)?;
        if self.percent || self.delta {
            let o = p.ref_value(self.org, self.addr(i), self.signed).unwrap_or(raw);
            if self.percent {
                return Some(if o != 0.0 { (raw - o) / o * 100.0 } else { 0.0 });
            }
            return Some(self.scaled(raw) - self.scaled(o));
        }
        let mut v = self.scaled(raw);
        if let Some(m) = var {
            let w = m.org.width();
            if let Some(x) = m.org.read(p.data(), m.start + i * w, m.signed) {
                v += x * m.factor + m.offset;
            }
        }
        Some(v)
    }
    pub fn text(&self, p: &Project, i: usize, var: Option<&MapDef>) -> Option<String> {
        let v = self.shown(p, i, var)?;
        Some(self.format(v))
    }
    pub fn format(&self, v: f64) -> String {
        if self.percent {
            return format!("{v:.1}");
        }
        if self.delta && !self.has_factor() {
            if self.radix == 16 && !self.org.is_float() {
                return if v < 0.0 { format!("-{:X}", (-v) as u64) } else { format!("{:X}", v as u64) };
            }
            return format!("{v:.0}");
        }
        if self.org.is_float() {
            let digits = if self.precision > 0 { self.precision as usize } else { 3 };
            return format!("{v:.digits$}");
        }
        if self.radix == 16 && !self.has_factor() {
            let n = self.width() * 2;
            let raw = if v < 0.0 { (v as i64) as u64 & ((1u64 << (n * 4)) - 1) } else { v as u64 };
            return format!("{raw:0n$X}");
        }
        format!("{:.*}", self.precision as usize, v)
    }
    /// How many characters a cell needs on screen.
    pub fn cell_chars(&self) -> usize {
        if self.percent {
            return 7;
        }
        if self.org.is_float() {
            return 11;
        }
        if self.has_factor() || self.delta {
            let (lo, hi) = self.org.limits(self.signed);
            let m = self.scaled(lo).abs().max(self.scaled(hi).abs());
            let digits = if m >= 1.0 { m.log10().floor() as usize + 1 } else { 1 };
            return (digits + 2 + self.precision as usize).clamp(3, 14);
        }
        if self.radix == 16 {
            return self.width() * 2;
        }
        match (self.width(), self.signed) {
            (1, false) => 3,
            (1, true) => 4,
            (2, false) => 5,
            (2, true) => 6,
            (_, false) => 10,
            (_, true) => 11,
        }
    }
    pub fn apply_map(&mut self, m: &MapDef) {
        self.base = m.start;
        self.cols = m.cols.max(1);
        self.rows_fixed = Some(m.rows.max(1));
        self.org = m.org;
        self.radix = m.radix;
        self.signed = m.signed;
        self.reciprocal = m.reciprocal;
        self.factor = m.factor;
        self.offset = m.offset;
        self.precision = m.precision;
        self.right = m.right;
        self.range_lo = m.range_lo;
        self.range_hi = m.range_hi;
    }
}

#[derive(Clone, Debug)]
pub struct Child {
    pub id: u64,
    pub project: u64,
    pub map_uid: Option<u64>,
    pub rect: Rect,
    pub restore: Option<Rect>,
    pub minimized: bool,
    pub g: Grid,
    pub connected: Option<u64>,
    pub connect_offset: i64,
}

/// Everything the drawing needs to read.
pub struct Env<'a> {
    pub p: &'a Project,
    pub map: Option<&'a MapDef>,
    pub var: Option<&'a MapDef>,
    pub theme: &'a Theme,
    pub cfg: &'a Config,
    pub active: bool,
    pub end: usize,
}

/// One notch of the wheel at a time, whatever the system's smooth scrolling does.
fn wheel_of(i: &egui::InputState) -> f32 {
    i.events
        .iter()
        .filter_map(|e| match e {
            egui::Event::MouseWheel { delta, .. } => Some(delta.y),
            _ => None,
        })
        .sum()
}

const STRIP_W: f32 = 18.0;
const TABS_H: f32 = 18.0;

pub fn body(ui: &mut Ui, rect: Rect, child: &mut Child, env: &Env, out: &mut Vec<Cmd>) {
    let content = Rect::from_min_max(rect.min, Pos2::new(rect.right() - STRIP_W, rect.bottom() - TABS_H));
    let strip = Rect::from_min_max(Pos2::new(rect.right() - STRIP_W, rect.top()), Pos2::new(rect.right(), rect.bottom() - TABS_H));
    let tabs = Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - TABS_H), rect.max);
    ui.painter().rect_filled(content, 0.0, c(if env.map.is_some() { env.theme.map_bg } else { env.theme.bg }));
    match child.g.mode {
        ViewMode::Text => text_view(ui, content, child, env, out),
        ViewMode::TwoD => plot_2d(ui, content, child, env, out),
        ViewMode::ThreeD => plot_3d(ui, content, child, env, out),
    }
    scroll_strip(ui, strip, child, env);
    mode_tabs(ui, tabs, child, out);
}

fn scroll_strip(ui: &mut Ui, r: Rect, child: &mut Child, env: &Env) {
    let g = &mut child.g;
    let total_rows = g.total(env.end).div_ceil(g.cols.max(1)).max(1);
    let page = g.visible_rows.max(1);
    let painter = ui.painter_at(r);
    painter.rect_filled(r, 0.0, crate::shell::pick(Color32::from_rgb(0xEC, 0xE9, 0xD8), crate::shell::bar_bg()));
    const B: f32 = 16.0;
    let btns: [(f32, i64, &str); 3] = [(0.0, -(page as i64), "⏶⏶"), (B, -8, "⏶"), (2.0 * B, -1, "▴")];
    let btns_low: [(f32, i64, &str); 3] = [(3.0 * B, 1, "▾"), (2.0 * B, 8, "⏷"), (B, page as i64, "⏷⏷")];
    let top_btn = |ui: &mut Ui, y: f32, d: i64, g: &mut Grid, id: &str| {
        let br = Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(STRIP_W, B));
        let resp = ui.interact(br, ui.scope_id().with((id, y as i32)), Sense::click());
        let pa = ui.painter_at(br);
        let face = if resp.hovered() {
            crate::shell::pick(Color32::WHITE, Color32::from_gray(0x4A))
        } else {
            crate::shell::pick(Color32::from_rgb(0xF2, 0xF0, 0xE5), Color32::from_gray(0x3C))
        };
        pa.rect_filled(br.shrink(0.5), 1.0, face);
        pa.rect_stroke(br.shrink(0.5), 1.0, Stroke::new(1.0, crate::shell::gray(150)), egui::StrokeKind::Inside);
        let cx = br.center();
        let up = d < 0;
        let n = if d.abs() == 1 { 1 } else if d.abs() == 8 { 2 } else { 2 };
        for k in 0..n {
            let oy = (k as f32 - (n - 1) as f32 / 2.0) * 4.0;
            let s = if up { -1.0 } else { 1.0 };
            let pts = vec![
                Pos2::new(cx.x - 4.0, cx.y + oy - 2.0 * s),
                Pos2::new(cx.x + 4.0, cx.y + oy - 2.0 * s),
                Pos2::new(cx.x, cx.y + oy + 2.0 * s),
            ];
            pa.add(Shape::convex_polygon(pts, crate::shell::ink(), Stroke::NONE));
        }
        if d.abs() > 8 {
            let ly = if up { br.top() + 3.0 } else { br.bottom() - 3.0 };
            pa.line_segment([Pos2::new(cx.x - 4.0, ly), Pos2::new(cx.x + 4.0, ly)], Stroke::new(1.0, crate::shell::ink()));
        }
        if resp.clicked() {
            g.top_row = (g.top_row as i64 + d).max(0) as usize;
        }
    };
    for (o, d, id) in btns {
        top_btn(ui, r.top() + o, d, g, id);
    }
    for (o, d, id) in btns_low {
        top_btn(ui, r.bottom() - o, d, g, id);
    }
    let track = Rect::from_min_max(Pos2::new(r.left() + 3.0, r.top() + 3.0 * B), Pos2::new(r.right() - 3.0, r.bottom() - 3.0 * B));
    if track.height() < 8.0 {
        return;
    }
    let cols_bytes = (g.cols * g.width()).max(1);
    let start = g.base;
    let h = track.height().max(1.0) as usize;
    let bytes_total = total_rows * cols_bytes;
    let per_px = (bytes_total as f32 / h as f32).max(1.0);
    let diff = env.p.data();
    let reference = env.p.ref_data();
    let mut run: Option<(usize, Color32)> = None;
    let flush = |pa: &egui::Painter, run: &Option<(usize, Color32)>, y2: usize| {
        if let Some((y1, col)) = run {
            let rr = Rect::from_min_max(
                Pos2::new(track.left(), track.top() + *y1 as f32),
                Pos2::new(track.right(), track.top() + y2 as f32),
            );
            pa.rect_filled(rr, 0.0, *col);
        }
    };
    for y in 0..h {
        let a = start + (y as f32 * per_px) as usize;
        let e = (a + per_px as usize).min(diff.len());
        let kind = env.p.overview.as_ref().and_then(|o| o.kind_at(a));
        let changed = a < e && e <= reference.len() && diff[a..e] != reference[a..e];
        let col = match kind {
            Some(k) => {
                let rgb = k.rgb(changed);
                Color32::from_rgb(rgb[0], rgb[1], rgb[2])
            }
            None => {
                if changed {
                    Color32::from_gray(200)
                } else {
                    Color32::from_gray(64)
                }
            }
        };
        match run {
            Some((_, rc)) if rc == col => {}
            _ => {
                flush(&painter, &run, y);
                run = Some((y, col));
            }
        }
    }
    flush(&painter, &run, h);
    // The window's visible part
    let fy = g.top_row as f32 / total_rows as f32;
    let fh = (page as f32 / total_rows as f32).min(1.0);
    let t = track.top() + fy * track.height();
    let tb = (t + (fh * track.height()).max(6.0)).min(track.bottom());
    painter.rect_stroke(
        Rect::from_min_max(Pos2::new(track.left() - 2.0, t), Pos2::new(track.right() + 2.0, tb)),
        0.0,
        Stroke::new(1.5, crate::shell::gray(40)),
        egui::StrokeKind::Outside,
    );
    let resp = ui.interact(track, ui.scope_id().with("track"), Sense::click_and_drag());
    if resp.is_pointer_button_down_on() {
        if let Some(p) = resp.interact_pointer_pos() {
            let f = ((p.y - track.top()) / track.height()).clamp(0.0, 1.0);
            g.top_row = ((f * total_rows as f32) as usize).saturating_sub(page / 2);
        }
    }
    clamp_scroll(g, env.end);
}

fn clamp_scroll(g: &mut Grid, end: usize) {
    let total_rows = g.total(end).div_ceil(g.cols.max(1)).max(1);
    g.top_row = g.top_row.min(total_rows.saturating_sub(1));
}

fn mode_tabs(ui: &mut Ui, r: Rect, child: &mut Child, out: &mut Vec<Cmd>) {
    let painter = ui.painter_at(r);
    painter.rect_filled(r, 0.0, crate::shell::bar_bg());
    // The three ways to look at the data, as word buttons; the one in use is marked.
    let mut x = r.left() + 5.0;
    for (name, w, m) in [("Numbers", 58.0, ViewMode::Text), ("2D", 30.0, ViewMode::TwoD), ("3D", 30.0, ViewMode::ThreeD)] {
        let tr = Rect::from_min_size(Pos2::new(x, r.top() + 1.0), Vec2::new(w, r.height() - 3.0));
        let on = child.g.mode == m;
        let resp = ui.interact(tr, ui.scope_id().with(name), Sense::click());
        let (fill, edge) = if on {
            (crate::shell::pick(Color32::from_rgb(0xCC, 0xE4, 0xF7), Color32::from_rgb(0x26, 0x4F, 0x78)), crate::shell::pick(Color32::from_rgb(0x00, 0x78, 0xD7), Color32::from_rgb(0x4A, 0x9C, 0xE0)))
        } else if resp.hovered() {
            (crate::shell::pick(Color32::from_rgb(0xE5, 0xF1, 0xFB), Color32::from_gray(0x48)), crate::shell::pick(Color32::from_gray(0xA8), Color32::from_gray(0x70)))
        } else {
            (crate::shell::pick(Color32::from_gray(0xFA), Color32::from_gray(0x34)), crate::shell::pick(Color32::from_gray(0xD0), Color32::from_gray(0x48)))
        };
        painter.rect_filled(tr, 4.0, fill);
        painter.rect_stroke(tr, 4.0, Stroke::new(1.0, edge), egui::StrokeKind::Inside);
        painter.text(tr.center(), Align2::CENTER_CENTER, name, FontId::proportional(11.5), crate::shell::ink());
        if resp.clicked() {
            out.push(Cmd::Activate(child.id));
            out.push(Cmd::Mode(m));
        }
        x += w + 2.0;
    }
}

/// A region drawn over the hexdump.
struct Region<'a> {
    map: &'a MapDef,
    index: usize,
    potential: bool,
}

fn regions<'a>(env: &'a Env, g: &Grid, first: usize, last: usize) -> Vec<Region<'a>> {
    let (a, b) = (g.addr(first), g.addr(last + 1));
    let mut out = vec![];
    if env.map.is_some() {
        return out;
    }
    for (i, m) in env.p.maps.iter().enumerate() {
        if m.start < b && m.end() > a {
            out.push(Region { map: m, index: i, potential: false });
        }
    }
    if env.cfg.show_potential {
        for (i, m) in env.p.potential.iter().enumerate() {
            if m.start < b && m.end() > a && m.org.width() == g.width() {
                out.push(Region { map: m, index: i, potential: true });
            }
        }
    }
    out
}

fn visible_range(g: &Grid, p: &Project, first: usize, last: usize) -> (f64, f64) {
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for i in first..=last {
        if let Some(v) = g.raw(p, i) {
            lo = lo.min(v);
            hi = hi.max(v);
        }
    }
    if lo > hi {
        (0.0, 1.0)
    } else if hi > lo {
        (lo, hi)
    } else {
        (lo, lo + 1.0)
    }
}

fn text_view(ui: &mut Ui, r: Rect, child: &mut Child, env: &Env, out: &mut Vec<Cmd>) {
    let id = child.id;
    let id_ui = ui.scope_id().with(("text", id));
    let g = &mut child.g;
    let t = env.theme;
    let map = env.map;
    let is_map = map.is_some();
    let font = FontId::monospace(g.font);
    let cw = ui.fonts_mut(|f| f.glyph_width(&font, '0')).max(1.0);
    let lh = (g.font * 1.24).round().max(8.0);
    let total = g.total(env.end);
    let cols = g.cols.max(1);
    let chars = g.cell_chars();
    let pitch = (chars as f32 + 1.0) * cw;
    let bg = c(if is_map { t.map_bg } else { t.bg });
    let txt = c(if is_map { t.map_text } else { t.text });
    let digits = format!("{:X}", env.end.saturating_sub(1).max(0xFFFF)).len().max(5);

    // Geometry
    let (mut y_axis_w, mut header_h) = (0.0, 0.0);
    let mut x_vals: Vec<f64> = vec![];
    let mut y_vals: Vec<f64> = vec![];
    if let Some(m) = map {
        let d = env.p.data();
        x_vals = m.x.values(d, m.cols);
        y_vals = m.y.values(d, m.rows);
        let w = y_vals.iter().map(|v| m.y.text(*v).len()).max().unwrap_or(1).max(4);
        y_axis_w = (w as f32 + 1.0) * cw;
        header_h = lh + 2.0;
    }
    let addr_w = if is_map { y_axis_w + 6.0 } else { (digits as f32 + 1.0) * cw + 6.0 };
    let grid_left = r.left() + addr_w;
    let grid_w = cols as f32 * pitch;
    let right_w = match g.right {
        RightSide::Nothing => 0.0,
        RightSide::Bars => cols as f32 * cw.max(7.0),
        RightSide::Ascii => (cols * g.width()) as f32 * cw,
        RightSide::Both => cols as f32 * cw.max(7.0) + (cols * g.width()) as f32 * cw + 6.0,
    };
    let right_left = if g.fix_right {
        (grid_left + grid_w + 4.0).min((r.right() - right_w - 4.0).max(grid_left + 40.0))
    } else {
        grid_left + grid_w + 4.0
    };
    let rows_vis = (((r.height() - header_h) / lh).floor() as usize).max(1);
    g.visible_rows = rows_vis;
    let total_rows = total.div_ceil(cols).max(1);
    if g.follow {
        let cr = g.cursor / cols;
        if cr < g.top_row {
            g.top_row = cr;
        } else if cr >= g.top_row + rows_vis {
            g.top_row = cr + 1 - rows_vis;
        }
        g.follow = false;
    }
    g.top_row = g.top_row.min(total_rows.saturating_sub(1));
    let first = g.top_row * cols;
    let last = ((g.top_row + rows_vis) * cols).min(total).saturating_sub(1);
    let dyn_range = if g.dynamic { Some(visible_range(g, env.p, first, last)) } else { None };

    // Input
    let resp = ui.interact(r, id_ui, Sense::click_and_drag());
    let data_rect = Rect::from_min_max(Pos2::new(grid_left, r.top() + header_h), Pos2::new((grid_left + grid_w).min(r.right()), r.bottom()));
    let single_x = (right_left - 2.0).min(r.right());
    let dot = Pos2::new(single_x, r.top() + header_h + 5.0);
    let hit = |g: &Grid, pos: Pos2| -> Option<usize> {
        if pos.y < r.top() + header_h {
            return None;
        }
        let row = g.top_row + ((pos.y - r.top() - header_h) / lh) as usize;
        let col = (((pos.x - grid_left) / pitch).floor().max(0.0) as usize).min(cols - 1);
        let i = row * cols + col;
        (i < total).then_some(i)
    };
    if let Some(pos) = resp.hover_pos() {
        if (pos.x - single_x).abs() < 3.0 {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        } else if !is_map && (pos.x - (grid_left - 4.0)).abs() < 3.0 {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        let (wheel, mods) = ui.input(|i| (wheel_of(i), i.modifiers));
        if wheel != 0.0 {
            let dir = if wheel > 0.0 { -1i64 } else { 1 };
            if mods.ctrl && mods.shift {
                out.push(if dir < 0 { Cmd::DiffPrev } else { Cmd::DiffNext });
            } else if mods.ctrl {
                g.font = (g.font - dir as f32).clamp(6.0, 28.0);
            } else {
                let step = if mods.shift { rows_vis as i64 } else { 3 };
                g.top_row = (g.top_row as i64 + dir * step).max(0) as usize;
            }
        }
    }
    // Everything starts where the button went down, not where the drag was first noticed.
    let pressed_here = ui.input(|i| i.pointer.primary_pressed()) && resp.contains_pointer();
    if pressed_here {
        if let Some(pos) = ui.input(|i| i.pointer.press_origin()) {
            if (pos.x - single_x).abs() < 3.0 {
                g.drag_kind = DragKind::Columns;
            } else if !is_map && (pos.x - (grid_left - 4.0)).abs() < 3.0 {
                g.drag_kind = DragKind::Scroll;
            } else if pos.x < grid_left && !is_map {
                g.sel_mode = SelMode::Rows;
                if let Some(i) = hit(g, Pos2::new(grid_left, pos.y)) {
                    g.cursor = i;
                    if let Some(s) = g.sel {
                        g.extra.push(s);
                    }
                    g.sel = Some(Sel::new(i, i));
                    g.sel_begin = Some(i);
                }
                g.drag_kind = DragKind::Select;
            } else if pos.x >= grid_left && pos.x < grid_left + grid_w {
                if let Some(i) = hit(g, pos) {
                    let shift = ui.input(|m| m.modifiers.shift);
                    if shift {
                        let anchor = g.sel.map(|s| s.anchor).unwrap_or(g.cursor);
                        g.sel = Some(Sel::new(anchor, i));
                        g.sel_begin = Some(anchor);
                    } else {
                        if env.cfg.delete_sel_on_move && !ui.input(|m| m.modifiers.ctrl) {
                            g.clear_sel();
                        }
                        g.sel_begin = Some(i);
                    }
                    g.cursor = i;
                    g.drag_kind = DragKind::Select;
                }
            } else {
                g.drag_kind = DragKind::None;
            }
        }
    }
    if resp.dragged() {
        if let Some(pos) = resp.interact_pointer_pos() {
            match g.drag_kind {
                DragKind::Select => {
                    if let Some(i) = hit(g, Pos2::new(pos.x.max(grid_left), pos.y.clamp(r.top() + header_h, r.bottom() - 1.0))) {
                        let anchor = g.sel_begin.unwrap_or(g.cursor);
                        // A new drag beside an older selection adds an area to it.
                        if let Some(old) = g.sel {
                            if old.anchor != anchor {
                                g.extra.push(old);
                                g.sel = None;
                            }
                        }
                        if i != anchor || g.sel.is_some() {
                            g.sel = Some(Sel::new(anchor, i));
                            g.cursor = i;
                        }
                    }
                    if pos.y < r.top() + header_h + 2.0 {
                        g.top_row = g.top_row.saturating_sub(1);
                    } else if pos.y > r.bottom() - 2.0 {
                        g.top_row += 1;
                    }
                }
                DragKind::Columns => {
                    let n = ((pos.x - grid_left) / pitch).round().max(1.0) as usize;
                    if n != g.cols && !is_map {
                        keep_top_addr(g, n);
                    }
                }
                DragKind::Scroll => {
                    let dy = resp.drag_delta().y;
                    if dy != 0.0 {
                        g.top_row = (g.top_row as f32 + dy * total_rows as f32 / r.height().max(1.0)).max(0.0) as usize;
                    }
                }
                _ => {}
            }
        }
    }
    if resp.drag_stopped() {
        g.drag_kind = DragKind::None;
        if env.cfg.support_selection && g.sel.is_some() && g.sel_mode == SelMode::Consecutive {
            out.push(Cmd::SupportSel);
        }
    }
    if resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            if pos.distance(dot) < 6.0 {
                g.fix_right = !g.fix_right;
            }
        }
    }
    if resp.double_clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            if pos.x < grid_left && !is_map {
                out.push(Cmd::GoTo);
            } else {
                out.push(Cmd::PropsWindow);
            }
        }
    }

    // Painting
    let painter = ui.painter_at(r);
    let col_line = c(t.rowmarker);
    let regs = regions(env, g, first, last.max(first));
    let d = env.p.data();
    let ov = env.p.overview.as_ref();
    if let Some(m) = map {
        // Axes
        let corner = Rect::from_min_size(r.min, Vec2::new(addr_w, header_h));
        painter.rect_filled(corner, 0.0, Color32::from_rgb(40, 40, 40));
        painter.text(corner.left_center() + Vec2::new(3.0, 0.0), Align2::LEFT_CENTER, &m.unit, FontId::monospace((g.font - 2.0).max(8.0)), Color32::WHITE);
        for (col, v) in x_vals.iter().enumerate().take(cols) {
            let x = grid_left + col as f32 * pitch;
            painter.text(Pos2::new(x + pitch - cw, r.top() + 1.0), Align2::RIGHT_TOP, m.x.text(*v), font.clone(), Color32::from_rgb(230, 230, 120));
        }
        painter.line_segment([Pos2::new(r.left(), r.top() + header_h - 1.0), Pos2::new(right_left, r.top() + header_h - 1.0)], Stroke::new(1.0, txt));
    }
    for row in 0..rows_vis {
        let rn = g.top_row + row;
        if rn >= total_rows {
            break;
        }
        let y = r.top() + header_h + row as f32 * lh;
        if let Some(m) = map {
            if let Some(v) = y_vals.get(rn) {
                painter.text(Pos2::new(grid_left - 4.0 - cw, y), Align2::RIGHT_TOP, m.y.text(*v), font.clone(), Color32::from_rgb(230, 230, 120));
            }
        } else {
            let a = g.addr(rn * cols);
            let display = a + env.p.active_element.and_then(|i| env.p.elements.get(i)).map(|e| e.display_offset).unwrap_or(0);
            painter.text(Pos2::new(r.left() + 3.0, y), Align2::LEFT_TOP, format!("{display:0digits$X}"), font.clone(), c(t.map_text));
        }
        for col in 0..cols {
            let i = rn * cols + col;
            if i >= total {
                break;
            }
            let x = grid_left + col as f32 * pitch;
            if x > r.right() {
                break;
            }
            let a = g.addr(i);
            let cell = Rect::from_min_size(Pos2::new(x, y), Vec2::new(pitch, lh));
            let selected = g.is_selected(i);
            let changed = env.p.is_changed(g.org, a, g.signed);
            let mut text_col = txt;
            let mut cell_bg = None;
            let reg = regs.iter().find(|q| q.map.contains(a));
            if let Some(q) = reg {
                if !q.potential {
                    cell_bg = Some(c(t.map_in_hex_bg));
                    text_col = c(t.map_in_hex_text);
                }
            }
            if let Some(o) = ov {
                if !is_map && env.cfg.show_pale && o.is_pale(a) && !changed {
                    text_col = pale(text_col, bg);
                }
            }
            // Height colours: in a map the cell takes the colour of its value; in the plain
            // hexdump the number itself does, so the page stays readable.
            if env.cfg.height_colors && !ov.is_some_and(|o| !is_map && o.is_pale(a)) {
                if let Some(raw) = g.raw(env.p, i) {
                    let (lo, hi) = match (dyn_range, reg) {
                        (_, Some(q)) if !is_map => (q.map.range_lo, q.map.range_hi),
                        (Some(v), _) => v,
                        _ => (g.range_lo, g.range_hi),
                    };
                    let hc = height_colour(((raw - lo) / (hi - lo).max(1e-9)) as f32);
                    if is_map || reg.is_some_and(|q| !q.potential) {
                        cell_bg = Some(hc);
                        text_col = ink_on(hc);
                    } else {
                        text_col = hc;
                    }
                }
            }
            if changed {
                let up = match (g.org.read(d, a, g.signed), env.p.ref_value(g.org, a, g.signed)) {
                    (Some(x), Some(y)) => x >= y,
                    _ => true,
                };
                text_col = c(t.changed);
                cell_bg = Some(c(if up { t.increased } else { t.decreased }));
                text_col = if t.name == "Black on white" { Color32::BLACK } else { text_col };
            }
            if let Some(cm) = env.p.comment_at(a) {
                cell_bg = Some(c(cm.color));
                text_col = c(t.comment_text);
            }
            if selected {
                cell_bg = Some(c(t.sel_bg));
                text_col = c(t.sel_text);
            }
            if let Some(b) = cell_bg {
                painter.rect_filled(cell, 0.0, b);
            }
            if let Some(s) = g.text(env.p, i, env.var) {
                painter.text(Pos2::new(x + pitch - cw * 0.5, y), Align2::RIGHT_TOP, s, font.clone(), text_col);
            }
            if env.p.markers.contains(&a) {
                painter.rect_filled(Rect::from_min_size(Pos2::new(x, y), Vec2::splat(4.0)), 0.0, Color32::from_rgb(255, 80, 80));
            }
            if env.active && i == g.cursor {
                painter.rect_stroke(cell.shrink(0.5), 0.0, Stroke::new(1.0, Color32::WHITE), egui::StrokeKind::Inside);
            }
            // Region borders where the neighbour isn't part of it
            for q in &regs {
                if q.map.contains(a) && q.potential {
                    let w = g.width();
                    let stroke = Stroke::new(1.0, c(t.potential));
                    let inside = |add: usize| add >= q.map.start && add < q.map.end();
                    let above = a.checked_sub(cols * w).is_some_and(|v| inside(v) && col < cols);
                    let below = inside(a + cols * w);
                    let left = col > 0 && a >= w && inside(a - w);
                    let right = col + 1 < cols && inside(a + w);
                    if !above {
                        painter.line_segment([cell.left_top(), cell.right_top()], stroke);
                    }
                    if !below {
                        painter.line_segment([cell.left_bottom(), cell.right_bottom()], stroke);
                    }
                    if !left {
                        painter.line_segment([cell.left_top(), cell.left_bottom()], stroke);
                    }
                    if !right {
                        painter.line_segment([cell.right_top(), cell.right_bottom()], stroke);
                    }
                }
            }
        }
        // Right side
        if matches!(g.right, RightSide::Bars | RightSide::Both) {
            let bw = cw.max(7.0);
            for col in 0..cols {
                let i = rn * cols + col;
                if i >= total {
                    break;
                }
                let a = g.addr(i);
                let x = right_left + col as f32 * bw;
                let Some(raw) = g.raw(env.p, i) else { continue };
                let (lo, hi) = match (dyn_range, regs.iter().find(|q| q.map.contains(a))) {
                    (_, Some(q)) if !is_map => (q.map.range_lo, q.map.range_hi),
                    (Some(v), _) => v,
                    _ => (g.range_lo, g.range_hi),
                };
                let f = ((raw - lo) / (hi - lo).max(1e-9)).clamp(0.0, 1.0) as f32;
                let h = (lh - 2.0) * f;
                // Program code and empty areas keep their plain look, as their numbers do.
                let dull = ov.is_some_and(|o| !is_map && o.is_pale(a));
                let mut col_bar = if env.cfg.height_colors && !dull { height_colour(f) } else { c(t.right) };
                if ov.is_some_and(|o| !is_map && env.cfg.show_pale && o.is_pale(a)) {
                    col_bar = pale(col_bar, bg);
                }
                if g.is_selected(i) {
                    col_bar = c(t.sel_text);
                } else if env.p.is_changed(g.org, a, g.signed) {
                    col_bar = c(t.changed);
                }
                if h > 0.0 {
                    let br = Rect::from_min_max(Pos2::new(x, y + lh - 1.0 - h), Pos2::new(x + bw, y + lh - 1.0));
                    painter.rect_filled(br, 0.0, col_bar);
                }
            }
        }
        if matches!(g.right, RightSide::Ascii | RightSide::Both) {
            let left = if g.right == RightSide::Both { right_left + cols as f32 * cw.max(7.0) + 6.0 } else { right_left };
            let n = cols * g.width();
            let a0 = g.addr(rn * cols);
            let s: String = (0..n)
                .map(|k| match d.get(a0 + k) {
                    Some(&b) if (0x20..0x7F).contains(&b) => b as char,
                    Some(_) => '.',
                    None => ' ',
                })
                .collect();
            painter.text(Pos2::new(left, y), Align2::LEFT_TOP, s, font.clone(), c(t.right));
        }
    }
    // Lines between the columns
    let line_top = r.top() + header_h;
    painter.line_segment([Pos2::new(grid_left - 4.0, line_top), Pos2::new(grid_left - 4.0, r.bottom())], Stroke::new(1.0, txt));
    painter.line_segment([Pos2::new(grid_left - 2.0, line_top), Pos2::new(grid_left - 2.0, r.bottom())], Stroke::new(1.0, txt));
    if !matches!(g.right, RightSide::Nothing) {
        painter.line_segment([Pos2::new(single_x, line_top), Pos2::new(single_x, r.bottom())], Stroke::new(1.0, txt));
        painter.circle_filled(dot, 2.5, if g.fix_right { Color32::from_rgb(0, 220, 0) } else { Color32::from_rgb(220, 0, 0) });
    }
    if !is_map && env.cfg.row_groups {
        for k in (4..cols).step_by(4) {
            let x = grid_left + k as f32 * pitch - 1.0;
            if x < single_x {
                painter.line_segment([Pos2::new(x, line_top), Pos2::new(x, r.bottom())], Stroke::new(0.5, col_line));
            }
        }
    }
    // Tags of potential maps
    let mut tag_hit: Option<(usize, bool)> = None;
    let mut seen = vec![];
    for q in regs.iter().filter(|q| q.potential) {
        if seen.contains(&q.index) {
            continue;
        }
        seen.push(q.index);
        let m = q.map;
        let cell = g.cell_of(m.start);
        if cell < first || cell > last {
            continue;
        }
        let row = (cell / cols) as f32 - g.top_row as f32;
        let x = grid_left + (cell % cols) as f32 * pitch;
        let y = r.top() + header_h + row * lh;
        let label = format!("{}: {}x{} ({} Bit)", m.potential_label.as_deref().unwrap_or("Map").replace("Map ", "").replace('"', ""), m.cols, m.rows, m.org.width() * 8);
        let f = FontId::proportional((g.font - 2.0).max(9.0));
        let gal = ui.fonts_mut(|fo| fo.layout_no_wrap(label, f, Color32::BLACK));
        let tag = Rect::from_min_size(Pos2::new(x, y - gal.size().y), gal.size() + Vec2::new(6.0, 0.0));
        painter.rect_filled(tag, 0.0, c(t.potential_tag));
        painter.galley(tag.min + Vec2::new(3.0, 0.0), gal, Color32::BLACK);
        let tr = ui.interact(tag, id_ui.with(("tag", q.index)), Sense::click());
        if tr.clicked() {
            tag_hit = Some((q.index, false));
        }
        if tr.double_clicked() {
            tag_hit = Some((q.index, true));
        }
        let index = q.index;
        tr.context_menu(|ui| {
            if ui.button("Show optimally").clicked() {
                out.push(Cmd::PotentialFit(index));
                ui.close();
            }
            if ui.button("Create a normal map").clicked() {
                out.push(Cmd::PotentialToMap(index));
                ui.close();
            }
            if ui.button("Drop this suggestion").clicked() {
                out.push(Cmd::PotentialRemove(index));
                ui.close();
            }
        });
    }
    if let Some((i, double)) = tag_hit {
        out.push(if double { Cmd::PotentialToMap(i) } else { Cmd::PotentialFit(i) });
    }
    // Comment tooltip
    if let Some(pos) = resp.hover_pos() {
        if data_rect.contains(pos) {
            if let Some(i) = hit(g, pos) {
                if let Some(cm) = env.p.comment_at(g.addr(i)) {
                    resp.clone().on_hover_text_at_pointer(cm.text.clone());
                }
            }
        }
    }
    // Inplace editor
    if g.inplace.is_some() && env.active {
        inplace_editor(ui, g, lh, pitch, grid_left, r.top() + header_h, cw, out);
    }
    let on_right = resp.hover_pos().is_some_and(|p| p.x > single_x);
    data_menu(&resp, g, env, on_right, out);
}

fn keep_top_addr(g: &mut Grid, n: usize) {
    let top = g.top_row * g.cols.max(1);
    let cursor = g.cursor;
    g.cols = n.max(1);
    g.top_row = top / g.cols;
    g.cursor = cursor;
}

fn inplace_editor(ui: &mut Ui, g: &mut Grid, lh: f32, pitch: f32, grid_left: f32, top: f32, _cw: f32, out: &mut Vec<Cmd>) {
    let col = g.cursor % g.cols;
    let row = (g.cursor / g.cols) as f32 - g.top_row as f32;
    let cell = Rect::from_min_size(Pos2::new(grid_left + col as f32 * pitch, top + row * lh), Vec2::new((pitch * 1.2).max(46.0), lh + 2.0));
    let Some(buf) = g.inplace.as_mut() else { return };
    let te = egui::TextEdit::singleline(buf).font(FontId::monospace(g.font)).margin(Vec2::new(1.0, 0.0)).desired_width(cell.width());
    let resp = ui.put(cell, te);
    resp.request_focus();
    let (enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
    if esc {
        g.inplace = None;
    } else if enter {
        out.push(Cmd::Inplace);
    }
}

fn menu_item(ui: &mut Ui, label: &str, cmd: Cmd, on: bool, out: &mut Vec<Cmd>) {
    if ui.add_enabled(on, egui::Button::new(label)).clicked() {
        out.push(cmd);
        ui.close();
    }
}

fn data_menu(resp: &egui::Response, g: &mut Grid, env: &Env, on_right: bool, out: &mut Vec<Cmd>) {
    let has_sel = g.has_selection();
    let is_map = env.map.is_some();
    let sel_mode = g.sel_mode;
    let right = g.right;
    let fix = g.fix_right;
    resp.context_menu(|ui| {
        if on_right {
            for (label, v) in [("Nothing", RightSide::Nothing), ("Bars", RightSide::Bars), ("ASCII", RightSide::Ascii), ("Both", RightSide::Both)] {
                if ui.radio(right == v, label).clicked() {
                    out.push(Cmd::Right(v));
                    ui.close();
                }
            }
            ui.separator();
            if ui.checkbox(&mut { fix }, "Keep in view").clicked() {
                out.push(Cmd::FixRight);
                ui.close();
            }
            menu_item(ui, "Fit the value range", Cmd::OptimizeRange, true, out);
            return;
        }
        menu_item(ui, "Copy", Cmd::Copy, true, out);
        menu_item(ui, "Paste", Cmd::Paste, true, out);
        ui.separator();
        menu_item(ui, "Step up", Cmd::Plus, true, out);
        menu_item(ui, "Step down", Cmd::Minus, true, out);
        menu_item(ui, "Set to a value...", Cmd::ChangeAbs, true, out);
        menu_item(ui, "Add or scale...", Cmd::ChangeRel, true, out);
        menu_item(ui, "Blend across the selection...", Cmd::ChangeEdit, has_sel, out);
        menu_item(ui, "Restore original values", Cmd::OriginalValue, true, out);
        ui.separator();
        if has_sel {
            menu_item(ui, "Make a map from the selection", Cmd::SelToMap, !is_map, out);
            menu_item(ui, "Snap selections to the shape of a map", Cmd::SupportSel, !is_map, out);
            menu_item(ui, "Clear the selection", Cmd::SelNone, true, out);
            ui.separator();
        }
        ui.menu_button("Selection shape", |ui| {
            for (label, m) in [("Run of cells", SelMode::Consecutive), ("Rectangle", SelMode::Rectangle), ("Whole columns", SelMode::Columns), ("Whole rows", SelMode::Rows)] {
                if ui.radio(sel_mode == m, label).clicked() {
                    out.push(Cmd::SelModeSet(m));
                    ui.close();
                }
            }
        });
        menu_item(ui, "Go to address...", Cmd::GoTo, true, out);
        menu_item(ui, "Add or edit a comment...", Cmd::CommentEdit, true, out);
        menu_item(ui, "Set or clear a bookmark", Cmd::MarkerToggle, true, out);
        if is_map {
            ui.separator();
            menu_item(ui, "Maps like this one...", Cmd::ParallelMaps, true, out);
            menu_item(ui, "Delete this map", Cmd::DeleteMap, true, out);
        } else {
            menu_item(ui, "Open the map under the cursor", Cmd::OpenMapAtCursor, env.p.map_at(g.addr(g.cursor)).is_some(), out);
        }
        ui.separator();
        menu_item(ui, "Window settings...", Cmd::PropsWindow, true, out);
    });
}

fn nice_step(range: f64, want: f64) -> f64 {
    let raw = (range / want.max(1.0)).max(1e-9);
    let mag = 10f64.powf(raw.log10().floor());
    let n = raw / mag;
    let f = if n < 1.5 {
        1.0
    } else if n < 3.0 {
        2.0
    } else if n < 7.0 {
        5.0
    } else {
        10.0
    };
    f * mag
}

fn plot_2d(ui: &mut Ui, r: Rect, child: &mut Child, env: &Env, out: &mut Vec<Cmd>) {
    let id = child.id;
    let g = &mut child.g;
    let t = env.theme;
    let total = g.total(env.end);
    let cols = g.cols.max(1);
    const SCALE_B: f32 = 18.0;
    const SCALE_R: f32 = 48.0;
    let plot = Rect::from_min_max(r.min, Pos2::new(r.right() - SCALE_R, r.bottom() - SCALE_B));
    let cell_w = g.zoom_x.max(0.05);
    let vis_cells = (plot.width() / cell_w).ceil() as usize + 1;
    let rows_vis = vis_cells.div_ceil(cols).max(1);
    g.visible_rows = rows_vis;
    if g.follow {
        let cr = g.cursor / cols;
        if cr < g.top_row {
            g.top_row = cr;
        } else if cr >= g.top_row + rows_vis.saturating_sub(1).max(1) {
            g.top_row = cr.saturating_sub(rows_vis / 2);
        }
        g.follow = false;
    }
    let first = g.top_row * cols;
    if first >= total {
        g.top_row = 0;
    }
    let first = g.top_row * cols;
    let last = (first + vis_cells).min(total).saturating_sub(1);
    let (lo, hi) = if g.dynamic { visible_range(g, env.p, first, last) } else { (g.range_lo, g.range_hi) };
    let (slo, shi) = {
        let (a, b) = (g.scaled(lo), g.scaled(hi));
        (a.min(b), a.max(b))
    };
    let span = ((hi - lo).max(1e-9) / g.zoom_y.max(0.01) as f64).max(1e-9);
    let to_y = |raw: f64| plot.bottom() - 4.0 - ((raw - lo) / span) as f32 * (plot.height() - 8.0);
    let to_x = |i: usize| plot.left() + (i as f32 - first as f32 + 0.5) * cell_w;

    let resp = ui.interact(r, ui.scope_id().with(("2d", id)), Sense::click_and_drag());
    let painter = ui.painter_at(r);
    let grid_col = c(t.rowmarker);
    let txt = c(t.text);
    // Value scale
    let sf = FontId::monospace(10.0);
    let step = nice_step(shi - slo, 6.0);
    let mut v = (slo / step).ceil() * step;
    while v <= shi + step * 0.001 {
        let f = ((v - slo) / (shi - slo).max(1e-9)) as f32 / g.zoom_y.max(0.01);
        let y = plot.bottom() - 4.0 - f * (plot.height() - 8.0);
        if y >= plot.top() {
            painter.line_segment([Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)], Stroke::new(1.0, grid_col));
            painter.text(Pos2::new(plot.right() + 3.0, y), Align2::LEFT_CENTER, g.format(v), sf.clone(), txt);
        }
        v += step;
    }
    // Rows and the address scale
    let row_px = cols as f32 * cell_w;
    if g.row_markers && row_px >= 3.0 {
        let mut i = first;
        while i <= last + 1 {
            let x = plot.left() + (i - first) as f32 * cell_w;
            painter.line_segment([Pos2::new(x, plot.top()), Pos2::new(x, plot.bottom())], Stroke::new(1.0, grid_col));
            i += cols;
        }
    }
    let stepx = (nice_step(plot.width() as f64 / cell_w as f64, 8.0) as usize).max(1);
    let mut i = first.div_ceil(stepx) * stepx;
    while i <= last {
        let x = plot.left() + (i - first) as f32 * cell_w;
        painter.line_segment([Pos2::new(x, plot.bottom()), Pos2::new(x, plot.bottom() + 3.0)], Stroke::new(1.0, txt));
        painter.text(Pos2::new(x, plot.bottom() + 4.0), Align2::CENTER_TOP, format!("{:X}", g.addr(i)), sf.clone(), txt);
        i += stepx;
    }
    // Maps and selection under the line
    if env.map.is_none() {
        for m in env.p.maps.iter() {
            let (a, b) = (g.cell_of(m.start), g.cell_of(m.end()));
            if a <= last && b >= first {
                let rr = Rect::from_min_max(Pos2::new(to_x(a.max(first)), plot.top()), Pos2::new(to_x(b.min(last + 1)), plot.bottom()));
                painter.rect_filled(rr, 0.0, c(t.map_in_hex_bg));
            }
        }
    }
    let line_col = c(t.right);
    let sel_col = c(t.sel_text);
    let mut prev: Option<Pos2> = None;
    let mut path: Vec<Pos2> = vec![];
    let mut sel_path: Vec<Vec<Pos2>> = vec![];
    let mut chg: Vec<Pos2> = vec![];
    let compress = cell_w < 1.0;
    let mut i = first;
    let stride = if compress { (1.0 / cell_w).ceil() as usize } else { 1 };
    while i <= last {
        let Some(raw) = g.raw(env.p, i) else { break };
        let p = Pos2::new(to_x(i), to_y(raw));
        // With height colours each stretch of the curve takes the colour of its value.
        let point_col = if env.cfg.height_colors { height_colour(((raw - lo) / (hi - lo).max(1e-9)) as f32) } else { line_col };
        if g.dots {
            match prev {
                Some(q) if env.cfg.height_colors => {
                    painter.line_segment([q, p], Stroke::new(1.0, point_col));
                }
                _ => path.push(p),
            }
        }
        if g.is_selected(i) {
            match sel_path.last_mut() {
                Some(l) if prev.is_some_and(|q| l.last() == Some(&q)) => l.push(p),
                _ => sel_path.push(vec![p]),
            }
        }
        if env.p.is_changed(g.org, g.addr(i), g.signed) {
            chg.push(p);
        }
        if !g.dots {
            painter.circle_filled(p, 1.2, point_col);
        }
        prev = Some(p);
        i += stride;
    }
    if path.len() > 1 {
        painter.add(Shape::line(path, Stroke::new(1.0, line_col)));
    }
    for s in sel_path {
        if s.len() > 1 {
            painter.add(Shape::line(s, Stroke::new(2.0, sel_col)));
        } else if let Some(p) = s.first() {
            painter.circle_filled(*p, 2.0, sel_col);
        }
    }
    for p in chg {
        painter.circle_filled(p, 2.0, c(t.changed));
    }
    if let Some(raw) = g.raw(env.p, g.cursor) {
        if g.cursor >= first && g.cursor <= last {
            let p = Pos2::new(to_x(g.cursor), to_y(raw));
            painter.line_segment([Pos2::new(p.x, plot.top()), Pos2::new(p.x, plot.bottom())], Stroke::new(1.0, Color32::from_gray(120)));
            painter.circle_stroke(p, 3.5, Stroke::new(1.0, Color32::WHITE));
        }
    }
    // Input
    let cell_at = |x: f32| -> usize { first + ((x - plot.left()) / cell_w).max(0.0) as usize };
    if let Some(pos) = resp.hover_pos() {
        let (wheel, mods) = ui.input(|i| (wheel_of(i), i.modifiers));
        if wheel != 0.0 {
            if mods.ctrl {
                let f = if wheel > 0.0 { 1.25 } else { 0.8 };
                if mods.shift {
                    g.zoom_y = (g.zoom_y * f).clamp(0.05, 200.0);
                } else {
                    g.zoom_x = (g.zoom_x * f).clamp(0.05, 64.0);
                }
            } else {
                let dir = if wheel > 0.0 { -1i64 } else { 1 };
                let step = if mods.shift { rows_vis as i64 } else { (rows_vis as i64 / 8).max(1) };
                g.top_row = (g.top_row as i64 + dir * step).max(0) as usize;
            }
        }
        let i = cell_at(pos.x);
        if let Some(raw) = g.raw(env.p, i) {
            if plot.contains(pos) && (g.is_selected(i) || i == g.cursor) && (to_y(raw) - pos.y).abs() < 6.0 && env.cfg.drag_2d {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
            }
        }
    }
    if resp.drag_started() {
        if let Some(pos) = ui.input(|i| i.pointer.press_origin()) {
            let i = cell_at(pos.x);
            let near = g.raw(env.p, i).is_some_and(|raw| (to_y(raw) - pos.y).abs() < 6.0);
            if plot.contains(pos) && env.cfg.drag_2d && near && (g.is_selected(i) || i == g.cursor) {
                g.drag_kind = DragKind::Values;
                g.drag_total = 0.0;
            } else if pos.y > plot.bottom() || pos.x > plot.right() {
                g.drag_kind = DragKind::Scroll;
            } else {
                g.drag_kind = DragKind::Select;
                if env.cfg.delete_sel_on_move {
                    g.clear_sel();
                }
                g.cursor = i.min(total.saturating_sub(1));
                g.sel_begin = Some(g.cursor);
            }
        }
    }
    if resp.dragged() {
        if let Some(pos) = resp.interact_pointer_pos() {
            match g.drag_kind {
                DragKind::Select => {
                    let i = cell_at(pos.x.clamp(plot.left(), plot.right())).min(total.saturating_sub(1));
                    let a = g.sel_begin.unwrap_or(i);
                    if let Some(old) = g.sel {
                        if old.anchor != a {
                            g.extra.push(old);
                        }
                    }
                    g.sel = Some(Sel::new(a, i));
                    g.cursor = i;
                }
                DragKind::Values => {
                    let dy = -resp.drag_delta().y as f64;
                    let per_px = span / (plot.height() as f64 - 8.0);
                    let delta = dy * per_px;
                    g.drag_total += delta;
                    let whole = if g.org.is_float() { g.drag_total } else { g.drag_total.trunc() };
                    if whole != 0.0 {
                        g.drag_total -= whole;
                        out.push(Cmd::DragValues(whole));
                    }
                }
                DragKind::Scroll => {
                    let d = resp.drag_delta();
                    let delta_cells = (-d.x / cell_w) as i64;
                    let rows = delta_cells / cols as i64;
                    if rows != 0 {
                        g.top_row = (g.top_row as i64 + rows).max(0) as usize;
                    }
                    if d.y != 0.0 {
                        g.zoom_y = (g.zoom_y * (1.0 + d.y * 0.01)).clamp(0.05, 200.0);
                    }
                }
                _ => {}
            }
        }
    }
    if resp.drag_stopped() {
        g.drag_kind = DragKind::None;
    }
    if resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let i = cell_at(pos.x).min(total.saturating_sub(1));
            if env.cfg.delete_sel_on_move {
                g.clear_sel();
            }
            g.cursor = i;
        }
    }
    if resp.double_clicked() {
        out.push(Cmd::PropsWindow);
    }
    data_menu(&resp, g, env, false, out);
}

/// Settings for drawing a surface.
#[derive(Clone, Copy, Debug)]
pub struct Surface {
    pub rot: f32,
    pub persp: f32,
    pub zscale: f32,
    pub zoffset: f32,
    pub fill: bool,
    pub axis: bool,
    pub line: Color32,
    pub hi: Color32,
    pub floor: Color32,
    /// Colour the surface by height, blue at the bottom to red at the top.
    pub height: bool,
}

fn project_point(x: f32, y: f32, z: f32, s: &Surface, r: Rect) -> (Pos2, f32) {
    let (sn, cs) = s.rot.sin_cos();
    let xr = x * cs - y * sn;
    let yr = x * sn + y * cs;
    let depth = 1.0 / (1.0 + yr * s.persp * 0.6).max(0.25);
    let scale = r.width().min(r.height() * 1.6) * 0.34;
    let px = xr * depth * scale;
    let py = (yr * 0.42 * depth - (z + s.zoffset) * s.zscale * 0.9 * depth) * scale;
    (r.center() + Vec2::new(px, py + r.height() * 0.17), yr)
}

/// Draws a surface from a value function. `highlight` marks cells; returns the picked screen points.
pub fn surface(painter: &egui::Painter, r: Rect, cols: usize, rows: usize, z: &dyn Fn(usize, usize) -> f32, highlight: &dyn Fn(usize, usize) -> bool, s: &Surface) -> Vec<(Pos2, usize, usize)> {
    let cols = cols.max(1);
    let rows = rows.max(1);
    let fx = |c: usize| if cols > 1 { c as f32 / (cols - 1) as f32 * 2.0 - 1.0 } else { 0.0 };
    let fy = |c: usize| if rows > 1 { c as f32 / (rows - 1) as f32 * 2.0 - 1.0 } else { 0.0 };
    let floor = Stroke::new(1.0, s.floor);
    let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    let fp: Vec<Pos2> = corners.iter().map(|&(x, y)| project_point(x, y, 0.0, s, r).0).collect();
    for i in 0..4 {
        painter.line_segment([fp[i], fp[(i + 1) % 4]], floor);
    }
    // Back walls
    let mut back: Vec<(usize, f32)> = corners.iter().enumerate().map(|(i, &(x, y))| (i, project_point(x, y, 0.0, s, r).1)).collect();
    back.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    for &(i, _) in back.iter().take(3) {
        let (x, y) = corners[i];
        painter.line_segment([project_point(x, y, 0.0, s, r).0, project_point(x, y, 1.0, s, r).0], floor);
    }
    let mut pts = Vec::with_capacity(cols * rows);
    let mut out = Vec::with_capacity(cols * rows);
    for j in 0..rows {
        for i in 0..cols {
            let (p, depth) = project_point(fx(i), fy(j), z(i, j), s, r);
            pts.push((p, depth));
            out.push((p, i, j));
        }
    }
    let idx = |i: usize, j: usize| j * cols + i;
    if s.fill && cols > 1 && rows > 1 {
        let mut quads: Vec<(f32, usize, usize)> = vec![];
        for j in 0..rows - 1 {
            for i in 0..cols - 1 {
                quads.push((pts[idx(i, j)].1 + pts[idx(i + 1, j + 1)].1, i, j));
            }
        }
        quads.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut mesh = egui::epaint::Mesh::default();
        for (_, i, j) in &quads {
            let h = (z(*i, *j) + z(i + 1, *j) + z(*i, j + 1) + z(i + 1, j + 1)) / 4.0;
            let col = if s.height { height_colour(h) } else { heat(h) };
            let b = mesh.vertices.len() as u32;
            for k in [idx(*i, *j), idx(i + 1, *j), idx(i + 1, j + 1), idx(*i, j + 1)] {
                mesh.colored_vertex(pts[k].0, col);
            }
            mesh.add_triangle(b, b + 1, b + 2);
            mesh.add_triangle(b, b + 2, b + 3);
        }
        painter.add(Shape::mesh(mesh));
    }
    let line = Stroke::new(1.0, if s.fill { Color32::from_rgba_unmultiplied(0, 0, 0, 120) } else { s.line });
    // A wire frame coloured by height: each wire takes the colour of the middle of its ends.
    let wires = s.height && !s.fill;
    let wire = |a: (usize, usize), b: (usize, usize)| if wires { Stroke::new(1.0, height_colour((z(a.0, a.1) + z(b.0, b.1)) / 2.0)) } else { line };
    for j in 0..rows {
        for i in 0..cols {
            if i + 1 < cols {
                let hi = highlight(i, j) && highlight(i + 1, j);
                let st = if hi { Stroke::new(1.6, s.hi) } else { wire((i, j), (i + 1, j)) };
                painter.line_segment([pts[idx(i, j)].0, pts[idx(i + 1, j)].0], st);
            }
            if j + 1 < rows {
                let hi = highlight(i, j) && highlight(i, j + 1);
                let st = if hi { Stroke::new(1.6, s.hi) } else { wire((i, j), (i, j + 1)) };
                painter.line_segment([pts[idx(i, j)].0, pts[idx(i, j + 1)].0], st);
            }
        }
    }
    if !s.fill && cols * rows < 2500 {
        for (k, p) in pts.iter().enumerate() {
            let col = if wires { height_colour(z(k % cols, k / cols)) } else { s.line };
            painter.circle_filled(p.0, 1.3, col);
        }
    }
    out
}

/// The colour of a height between 0 (lowest) and 1 (highest): blue, through cyan, green and
/// yellow, to red. The blue end is kept light enough to read as text on a dark page.
pub fn height_colour(f: f32) -> Color32 {
    const STOPS: [[f32; 3]; 5] = [[60.0, 110.0, 255.0], [0.0, 215.0, 230.0], [40.0, 215.0, 60.0], [240.0, 220.0, 30.0], [240.0, 50.0, 40.0]];
    let x = if f.is_finite() { f.clamp(0.0, 1.0) * 4.0 } else { 0.0 };
    let i = (x as usize).min(3);
    let t = x - i as f32;
    let mix = |k: usize| (STOPS[i][k] + (STOPS[i + 1][k] - STOPS[i][k]) * t) as u8;
    Color32::from_rgb(mix(0), mix(1), mix(2))
}

/// Black or white, whichever reads better on this colour.
fn ink_on(bg: Color32) -> Color32 {
    let light = 0.299 * bg.r() as f32 + 0.587 * bg.g() as f32 + 0.114 * bg.b() as f32;
    if light > 140.0 {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

fn heat(h: f32) -> Color32 {
    let h = h.clamp(0.0, 1.0);
    let (r, g, b) = if h < 0.5 {
        let t = h * 2.0;
        (40.0 * t, 90.0 + 130.0 * t, 200.0 - 120.0 * t)
    } else {
        let t = (h - 0.5) * 2.0;
        (40.0 + 200.0 * t, 220.0 - 130.0 * t, 80.0 - 50.0 * t)
    };
    Color32::from_rgb(r as u8, g as u8, b as u8)
}

fn plot_3d(ui: &mut Ui, r: Rect, child: &mut Child, env: &Env, out: &mut Vec<Cmd>) {
    let id = child.id;
    let g = &mut child.g;
    let t = env.theme;
    let total = g.total(env.end);
    let cols = g.cols.max(1);
    let draw_cols = cols.min(128);
    let rows_all = total.div_ceil(cols).max(1);
    let draw_rows = if g.rows_fixed.is_some() { rows_all.min(128) } else { 48.min(rows_all) };
    g.visible_rows = draw_rows;
    if g.follow {
        let cr = g.cursor / cols;
        if cr < g.top_row || cr >= g.top_row + draw_rows {
            g.top_row = cr.saturating_sub(draw_rows / 2);
        }
        g.follow = false;
    }
    let top = g.top_row.min(rows_all.saturating_sub(draw_rows));
    let first = top * cols;
    let last = (first + draw_rows * cols).min(total).saturating_sub(1);
    let (lo, hi) = if env.map.is_some() || !g.dynamic {
        if env.map.is_some() {
            (g.range_lo, g.range_hi)
        } else {
            visible_range(g, env.p, first, last)
        }
    } else {
        visible_range(g, env.p, first, last)
    };
    let resp = ui.interact(r, ui.scope_id().with(("3d", id)), Sense::click_and_drag());
    let painter = ui.painter_at(r);
    let s = Surface {
        rot: g.rot,
        persp: g.persp,
        zscale: g.zscale,
        zoffset: g.zoffset,
        fill: g.fill,
        axis: g.show_axis,
        line: c(t.three_d),
        hi: c(t.sel_text),
        floor: c(t.rowmarker).gamma_multiply(1.6),
        height: env.cfg.height_colors,
    };
    let grid_ref: &Grid = g;
    let zf = |i: usize, j: usize| -> f32 {
        let idx = (top + j) * cols + i;
        let v = grid_ref.raw(env.p, idx).unwrap_or(lo);
        ((v - lo) / (hi - lo).max(1e-9)) as f32
    };
    let hl = |i: usize, j: usize| grid_ref.is_selected((top + j) * cols + i);
    let points = surface(&painter, r, draw_cols, draw_rows, &zf, &hl, &s);
    if s.axis {
        let f = FontId::monospace(10.0);
        painter.text(r.left_top() + Vec2::new(4.0, 4.0), Align2::LEFT_TOP, format!("{}\n{}", grid_ref.format(grid_ref.scaled(hi)), grid_ref.format(grid_ref.scaled(lo))), f.clone(), c(t.text));
        if let Some(m) = env.map {
            let (a, b) = (project_point(-1.0, 1.0, 0.0, &s, r).0, project_point(1.0, 1.0, 0.0, &s, r).0);
            painter.text(a.lerp(b, 0.5) + Vec2::new(0.0, 10.0), Align2::CENTER_TOP, &m.x.desc, f.clone(), c(t.text));
            let (a2, b2) = (project_point(1.0, -1.0, 0.0, &s, r).0, project_point(1.0, 1.0, 0.0, &s, r).0);
            painter.text(a2.lerp(b2, 0.5) + Vec2::new(10.0, 0.0), Align2::LEFT_CENTER, &m.y.desc, f, c(t.text));
        }
    }
    let cur = g.cursor;
    if cur >= first && cur <= last {
        let ci = cur % cols;
        let cj = cur / cols - top;
        if let Some(p) = points.iter().find(|p| p.1 == ci && p.2 == cj) {
            painter.circle_stroke(p.0, 4.0, Stroke::new(1.2, Color32::WHITE));
        }
    }
    let nearest = |pos: Pos2| -> Option<(f32, usize, usize)> {
        points
            .iter()
            .map(|p| (p.0.distance(pos), p.1, p.2))
            .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
    };
    let cell_of = |(i, j): (usize, usize)| (top + j) * cols + i;
    if let Some(pos) = resp.hover_pos() {
        if let Some((dist, i, j)) = nearest(pos) {
            let k = cell_of((i, j));
            if dist < 6.0 && (g.is_selected(k) || k == g.cursor) && env.cfg.drag_3d {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
            }
        }
        let wheel = ui.input(wheel_of);
        if wheel != 0.0 {
            g.zscale = (g.zscale * if wheel > 0.0 { 1.1 } else { 0.9 }).clamp(0.05, 8.0);
        }
    }
    if resp.drag_started() {
        if let Some(pos) = ui.input(|i| i.pointer.press_origin()) {
            g.drag_kind = DragKind::Rotate;
            if let Some((dist, i, j)) = nearest(pos) {
                let k = cell_of((i, j));
                if dist < 6.0 && (g.is_selected(k) || k == g.cursor) && env.cfg.drag_3d {
                    g.drag_kind = DragKind::Values;
                    g.drag_total = 0.0;
                }
            }
        }
    }
    if resp.dragged() {
        let d = resp.drag_delta();
        match g.drag_kind {
            DragKind::Rotate => {
                g.rot += d.x * 0.01;
                if ui.input(|i| i.modifiers.ctrl) {
                    g.zscale = (g.zscale - d.y * 0.01).clamp(0.05, 8.0);
                } else {
                    g.persp = (g.persp + d.y * 0.005).clamp(0.0, 1.2);
                }
            }
            DragKind::Values => {
                let span = (hi - lo).max(1e-9);
                let per_px = span / (r.height() as f64 * 0.3 * g.zscale as f64).max(1.0);
                g.drag_total += -d.y as f64 * per_px;
                let whole = if g.org.is_float() { g.drag_total } else { g.drag_total.trunc() };
                if whole != 0.0 {
                    g.drag_total -= whole;
                    out.push(Cmd::DragValues(whole));
                }
            }
            _ => {}
        }
    }
    if resp.drag_stopped() {
        g.drag_kind = DragKind::None;
    }
    if resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            if let Some((dist, i, j)) = nearest(pos) {
                if dist < 12.0 {
                    if env.cfg.delete_sel_on_move {
                        g.clear_sel();
                    }
                    g.cursor = cell_of((i, j));
                }
            }
        }
    }
    if resp.double_clicked() {
        out.push(Cmd::PropsWindow);
    }
    data_menu(&resp, g, env, false, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(cols: usize) -> Grid {
        Grid { cols, ..Grid::default() }
    }

    #[test]
    fn four_selection_modes() {
        let mut v = g(4);
        v.sel = Some(Sel::new(5, 10)); // row 1 col 1 .. row 2 col 2
        v.sel_mode = SelMode::Consecutive;
        assert_eq!(v.selected_cells(16), vec![5, 6, 7, 8, 9, 10]);
        v.sel_mode = SelMode::Rectangle;
        assert_eq!(v.selected_cells(16), vec![5, 6, 9, 10]);
        v.sel_mode = SelMode::Rows;
        assert_eq!(v.selected_cells(16), vec![4, 5, 6, 7, 8, 9, 10, 11]);
        v.sel_mode = SelMode::Columns;
        assert_eq!(v.selected_cells(16), vec![1, 2, 5, 6, 9, 10, 13, 14]);
    }

    #[test]
    fn no_selection_means_the_cursor() {
        let mut v = g(4);
        v.cursor = 3;
        assert_eq!(v.selected_cells(16), vec![3]);
        assert!(g(4).selected_cells(0).is_empty());
    }

    #[test]
    fn extra_selections_add_up() {
        let mut v = g(4);
        v.sel = Some(Sel::new(0, 1));
        v.extra.push(Sel::new(8, 9));
        assert_eq!(v.selected_cells(16), vec![0, 1, 8, 9]);
    }

    #[test]
    fn rectangle_is_the_map_shape() {
        let mut v = g(4);
        v.sel = Some(Sel::new(5, 10));
        v.sel_mode = SelMode::Rectangle;
        assert_eq!(v.sel_rect(), Some((5, 2, 2)));
    }

    #[test]
    fn factor_offset_and_reciprocal() {
        let mut v = g(4);
        v.factor = 0.5;
        v.offset = 10.0;
        assert_eq!(v.scaled(100.0), 60.0);
        assert_eq!(v.unscaled(60.0), 100.0);
        v.reciprocal = true;
        assert_eq!(v.scaled(2.0), 0.5 * 0.5 + 10.0);
        assert!((v.unscaled(v.scaled(4.0)) - 4.0).abs() < 1e-9);
        v.no_factor = true;
        assert_eq!(v.scaled(7.0), 7.0);
    }

    #[test]
    fn shown_percent_and_delta_against_the_reference() {
        let mut p = Project::new(1);
        p.add_version("", vec![100, 200]);
        let n = p.add_version("", vec![110, 200]);
        p.current = n;
        let mut v = g(2);
        v.delta = true;
        assert_eq!(v.shown(&p, 0, None), Some(10.0));
        v.delta = false;
        v.percent = true;
        assert_eq!(v.shown(&p, 0, None), Some(10.0));
        assert_eq!(v.shown(&p, 1, None), Some(0.0));
        v.percent = false;
        v.show_orig = true;
        assert_eq!(v.shown(&p, 0, None), Some(100.0));
    }

    #[test]
    fn hex_text_pads_to_the_cell_width() {
        let mut v = g(4);
        assert_eq!(v.format(15.0), "0F");
        v.org = DataOrg::HiLo;
        assert_eq!(v.format(15.0), "000F");
        v.radix = 10;
        assert_eq!(v.format(15.0), "15");
    }
}

#[cfg(test)]
mod colour_tests {
    use super::*;

    #[test]
    fn height_colours_run_from_blue_to_red() {
        let (lo, mid, hi) = (height_colour(0.0), height_colour(0.5), height_colour(1.0));
        assert!(lo.b() > lo.r() && lo.b() > 200, "{lo:?}");
        assert!(mid.g() > mid.r() && mid.g() > mid.b(), "{mid:?}");
        assert!(hi.r() > hi.g() && hi.r() > hi.b(), "{hi:?}");
        assert_eq!(height_colour(-3.0), lo);
        assert_eq!(height_colour(9.0), hi);
        assert_eq!(height_colour(f32::NAN), lo);
        assert_eq!(ink_on(height_colour(0.75)), Color32::BLACK, "black reads best on yellow");
        assert_eq!(ink_on(lo), Color32::WHITE, "white reads best on blue");
    }
}
