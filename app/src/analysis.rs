//! Background map search, the coloured overview and the vehicle data search.
//! The rules here are this program's own.
use crate::model::*;
use regex::bytes::Regex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AreaKind {
    Empty,
    Code,
    Unrecognized,
    Map8,
    Map16,
    Map32,
    Possible,
    Recognized,
}

impl AreaKind {
    pub fn label(self) -> &'static str {
        match self {
            AreaKind::Map8 => "8-bit tables",
            AreaKind::Map16 => "16-bit tables",
            AreaKind::Map32 => "32-bit tables",
            AreaKind::Possible => "Likely tables",
            AreaKind::Recognized => "Your maps",
            AreaKind::Empty => "Empty",
            AreaKind::Code => "Code",
            AreaKind::Unrecognized => "Other data",
        }
    }
    pub const ALL: [AreaKind; 8] = [
        AreaKind::Map8,
        AreaKind::Map16,
        AreaKind::Map32,
        AreaKind::Possible,
        AreaKind::Recognized,
        AreaKind::Empty,
        AreaKind::Code,
        AreaKind::Unrecognized,
    ];
    pub fn rgb(self, bright: bool) -> [u8; 3] {
        let (d, b) = match self {
            AreaKind::Map8 => ([0xC0, 0x00, 0x00], [0xFF, 0x80, 0x80]),
            AreaKind::Map16 => ([0xC0, 0x00, 0xC0], [0xFF, 0x80, 0xFF]),
            AreaKind::Map32 => ([0x99, 0x4C, 0x00], [0xFF, 0x99, 0x40]),
            AreaKind::Possible => ([0x00, 0x80, 0x00], [0x60, 0xFF, 0x60]),
            AreaKind::Recognized => ([0x88, 0x88, 0xF0], [0xC8, 0xC8, 0xFF]),
            AreaKind::Empty => ([0x40, 0x40, 0x40], [0xC0, 0xC0, 0xC0]),
            AreaKind::Code => ([0x00, 0x80, 0x80], [0x40, 0xFF, 0xFF]),
            AreaKind::Unrecognized => ([0x80, 0x80, 0x00], [0xFF, 0xFF, 0x40]),
        };
        if bright {
            b
        } else {
            d
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Overview {
    pub bytes_per_block: usize,
    pub blocks: Vec<AreaKind>,
}

impl Overview {
    pub fn kind_at(&self, addr: usize) -> Option<AreaKind> {
        self.blocks.get(addr / self.bytes_per_block.max(1)).copied()
    }
    pub fn is_pale(&self, addr: usize) -> bool {
        matches!(self.kind_at(addr), Some(AreaKind::Empty | AreaKind::Code))
    }
}

pub const OVERVIEW_BLOCK: usize = 16;

fn block_kind(b: &[u8]) -> AreaKind {
    let first = b[0];
    if b.iter().all(|&x| x == first) && (first == 0 || first == 0xFF) {
        return AreaKind::Empty;
    }
    let mut seen = [false; 256];
    let mut distinct = 0usize;
    for &x in b {
        if !seen[x as usize] {
            seen[x as usize] = true;
            distinct += 1;
        }
    }
    // Program code rarely repeats a byte in a short block, and its neighbours jump a lot.
    let jumps = b.windows(2).filter(|w| (w[0] as i32 - w[1] as i32).abs() > 48).count();
    if distinct * 8 >= b.len() * 5 && jumps * 5 >= b.len() * 2 {
        AreaKind::Code
    } else {
        AreaKind::Unrecognized
    }
}

pub fn build_overview(d: &[u8], maps: &[MapDef], potential: &[MapDef]) -> Overview {
    let bb = OVERVIEW_BLOCK;
    let mut blocks: Vec<AreaKind> = d.chunks(bb).map(block_kind).collect();
    let mut paint = |m: &MapDef, k: AreaKind| {
        let from = m.start / bb;
        let to = m.end().div_ceil(bb).min(blocks.len());
        if from < to {
            for b in &mut blocks[from..to] {
                *b = k;
            }
        }
    };
    for m in potential {
        let k = if m.potential_label.as_deref().is_some_and(|s| s.contains("Statistical")) {
            AreaKind::Possible
        } else {
            match m.org.width() {
                1 => AreaKind::Map8,
                2 => AreaKind::Map16,
                _ => AreaKind::Map32,
            }
        };
        paint(m, k);
    }
    for m in maps {
        paint(m, AreaKind::Recognized);
    }
    Overview { bytes_per_block: bb, blocks }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SearchRules {
    pub ignore_constant: bool,
    pub ignore_mostly_constant: bool,
    pub ignore_alternating: bool,
    pub axis_maps: bool,
    pub statistical: bool,
}

impl Default for SearchRules {
    fn default() -> Self {
        SearchRules {
            ignore_constant: true,
            ignore_mostly_constant: true,
            ignore_alternating: true,
            axis_maps: true,
            statistical: true,
        }
    }
}

pub const MIN_AXIS: usize = 2;
pub const MAX_AXIS: usize = 64;
pub const BYTE_MIN_CELLS: usize = 25;
pub const BYTE_MIN_LINE: usize = 10;
pub const WORD_MIN_CELLS: usize = 4;
pub const WORD_MIN_LINE: usize = 4;
pub const STAT_MIN_CELLS: usize = 24;
pub const STAT_MIN_COLS: usize = 4;
pub const STAT_MAX_COLS: usize = 48;
pub const STAT_MIN_ROWS: usize = 3;
pub const STAT_WINDOW_CELLS: usize = 16;
pub const STAT_SMOOTH_LIMIT: f64 = 0.25;
pub const STAT_MIN_RUN: usize = 32;
pub const STAT_ROW_MATCH: f64 = 0.5;
pub const STAT_TRIM: usize = 16;
pub const STAT_TRIM_MAX_RUN: usize = 4096;
pub const MOSTLY_CONSTANT: f64 = 0.8;

fn rising(d: &[u8], at: usize, n: usize, org: DataOrg) -> bool {
    let w = org.width();
    let mut prev = None;
    for i in 0..n {
        let Some(v) = org.read(d, at + i * w, false) else { return false };
        if let Some(p) = prev {
            if v <= p {
                return false;
            }
        }
        prev = Some(v);
    }
    true
}

/// Whether the values are too dull to be a map, under the person's rules.
pub fn boring(d: &[u8], start: usize, cells: usize, org: DataOrg, r: &SearchRules) -> bool {
    let w = org.width();
    let mut counts: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
    for i in 0..cells {
        let Some(v) = org.read(d, start + i * w, false) else { return true };
        *counts.entry(v.to_bits()).or_insert(0) += 1;
    }
    if r.ignore_constant && counts.len() <= 1 {
        return true;
    }
    if r.ignore_mostly_constant {
        let top = counts.values().copied().max().unwrap_or(0);
        if top as f64 >= cells as f64 * MOSTLY_CONSTANT {
            return true;
        }
    }
    if r.ignore_alternating && counts.len() == 2 {
        let alt = (2..cells).all(|i| {
            org.read(d, start + i * w, false) == org.read(d, start + (i - 2) * w, false)
        });
        if alt {
            return true;
        }
    }
    false
}

fn make_map(p: (usize, usize, usize), org: DataOrg, label: String, x: Option<usize>, y: Option<usize>, d: &[u8]) -> MapDef {
    let (start, cols, rows) = p;
    let mut m = MapDef {
        name: label.clone(),
        potential_label: Some(label),
        folder: "Suggested maps".into(),
        start,
        cols,
        rows,
        org,
        kind: if rows > 1 { MapKind::TwoDim } else { MapKind::OneDim },
        ..MapDef::default()
    };
    if let Some(a) = x {
        m.x = Axis { src: AxisSrc::Rom, addr: a, org, ..Axis::default() };
    }
    if let Some(a) = y {
        m.y = Axis { src: AxisSrc::Rom, addr: a, org, ..Axis::default() };
    }
    m.auto_range(d);
    m
}

fn bits(org: DataOrg) -> u32 {
    org.width() as u32 * 8
}

/// Maps that carry their own axes:
///  A: [nx][ny][x axis][y axis][values]   B: [nx][x axis][ny][y axis][values]   C: [n][axis][values]
pub fn axis_maps(d: &[u8], org: DataOrg, r: &SearchRules, progress: &dyn Fn(usize) -> bool) -> Vec<MapDef> {
    let w = org.width();
    let mut out = vec![];
    let mut a = 0usize;
    // Single bytes fit an axis by chance far more often, so they have to prove more.
    let (min_cells, min_line) = if w == 1 { (BYTE_MIN_CELLS, BYTE_MIN_LINE) } else { (WORD_MIN_CELLS, WORD_MIN_LINE) };
    let count = |at: usize| -> Option<usize> {
        let v = org.read(d, at, false)? as usize;
        (MIN_AXIS..=MAX_AXIS).contains(&v).then_some(v)
    };
    while a + 4 * w < d.len() {
        if a % 65536 == 0 && !progress(a) {
            return out;
        }
        if let Some(nx) = count(a) {
            // Layout A
            if let Some(ny) = count(a + w) {
                let x = a + 2 * w;
                let y = x + nx * w;
                let v = y + ny * w;
                if nx * ny >= min_cells
                    && v + nx * ny * w <= d.len()
                    && rising(d, x, nx, org)
                    && rising(d, y, ny, org)
                    && !boring(d, v, nx * ny, org, r)
                {
                    out.push(make_map((v, nx, ny), org, format!("Map \"Axes A {}\"", bits(org)), Some(x), Some(y), d));
                    a = v + nx * ny * w;
                    continue;
                }
            }
            // Layout B
            let x = a + w;
            if rising(d, x, nx, org) {
                if let Some(ny) = count(x + nx * w) {
                    let y = x + nx * w + w;
                    let v = y + ny * w;
                    if nx * ny >= min_cells
                        && v + nx * ny * w <= d.len()
                        && rising(d, y, ny, org)
                        && !boring(d, v, nx * ny, org, r)
                    {
                        out.push(make_map((v, nx, ny), org, format!("Map \"Axes B {}\"", bits(org)), Some(x), Some(y), d));
                        a = v + nx * ny * w;
                        continue;
                    }
                }
                // Layout C
                let v = x + nx * w;
                if nx >= min_line && v + nx * w <= d.len() && !boring(d, v, nx, org, r) {
                    out.push(make_map((v, nx, 1), org, format!("Map \"Axes C {}\"", bits(org)), Some(x), None, d));
                    a = v + nx * w;
                    continue;
                }
            }
        }
        a += w;
    }
    out
}

/// Maps with no axes, recognised by their look: long smooth stretches with a repeating row width.
pub fn statistical_maps(d: &[u8], org: DataOrg, r: &SearchRules, taken: &[(usize, usize)], progress: &dyn Fn(usize) -> bool) -> Vec<MapDef> {
    let w = org.width();
    let n = d.len() / w;
    let mut out = vec![];
    if n < STAT_MIN_CELLS {
        return out;
    }
    let vals: Vec<f64> = (0..n).map(|i| org.read(d, i * w, false).unwrap_or(0.0)).collect();
    let full = org.limits(false).1;
    let mut i = 0;
    while i + STAT_WINDOW_CELLS <= n {
        if i % 8192 == 0 && !progress(i * w) {
            return out;
        }
        if !window_smooth(&vals[i..i + STAT_WINDOW_CELLS], full) {
            i += STAT_WINDOW_CELLS / 2;
            continue;
        }
        let mut e = i + STAT_WINDOW_CELLS;
        while e + STAT_WINDOW_CELLS / 2 <= n && window_smooth(&vals[e - STAT_WINDOW_CELLS / 2..e + STAT_WINDOW_CELLS / 2], full) {
            e += STAT_WINDOW_CELLS / 2;
        }
        let addr = i * w;
        let overlap = taken.iter().any(|&(s, t)| addr < t && addr + (e - i) * w > s);
        if e - i >= STAT_MIN_RUN && !overlap {
            if let Some((skip, cols)) = trimmed_cols(&vals[i..e]) {
                let at = addr + skip * w;
                let rows = (e - i - skip) / cols;
                if rows >= STAT_MIN_ROWS && cols * rows >= STAT_MIN_CELLS && !boring(d, at, cols * rows, org, r) {
                    out.push(make_map((at, cols, rows), org, format!("Map \"Statistical {}\"", bits(org)), None, None, d));
                }
            }
        }
        i = e;
    }
    out
}

fn window_smooth(v: &[f64], full: f64) -> bool {
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for &x in v {
        lo = lo.min(x);
        hi = hi.max(x);
    }
    let range = hi - lo;
    // A flat window or one spanning most of the number range isn't a table.
    if range < 2.0 || range > full * 0.7 {
        return false;
    }
    let step: f64 = v.windows(2).map(|p| (p[0] - p[1]).abs()).sum::<f64>() / (v.len() - 1) as f64;
    step <= range * STAT_SMOOTH_LIMIT
}

/// The row width at which each row is most nearly the row above it plus a constant.
fn best_cols(v: &[f64]) -> Option<usize> {
    let step: f64 = v.windows(2).map(|p| (p[0] - p[1]).abs()).sum::<f64>() / (v.len() - 1) as f64;
    let mut best: Option<(usize, f64)> = None;
    for c in STAT_MIN_COLS..=STAT_MAX_COLS.min(v.len() / 2) {
        let n = (v.len() - c) as f64;
        let mean: f64 = (c..v.len()).map(|i| v[i] - v[i - c]).sum::<f64>() / n;
        let spread: f64 = (c..v.len()).map(|i| (v[i] - v[i - c] - mean).abs()).sum::<f64>() / n;
        // Small penalty per column so that a multiple of the real width doesn't win.
        let score = spread * (1.0 + c as f64 * 0.002) + c as f64 * 1e-9;
        if best.is_none_or(|b| score < b.1) {
            best = Some((c, score));
        }
    }
    let (c, e) = best?;
    (e < step.max(1.0) * STAT_ROW_MATCH).then_some(c)
}

/// A run can pick up a few cells of its neighbours on either side, which spoil the row test.
/// Returns how many cells to skip at the start, and the row width.
fn trimmed_cols(run: &[f64]) -> Option<(usize, usize)> {
    if let Some(c) = best_cols(run) {
        return Some((0, c));
    }
    if run.len() > STAT_TRIM_MAX_RUN {
        return None;
    }
    for skip in 0..=STAT_TRIM {
        for cut in 0..=STAT_TRIM {
            if skip == 0 && cut == 0 {
                continue;
            }
            if run.len() < skip + cut + STAT_MIN_RUN {
                continue;
            }
            if let Some(c) = best_cols(&run[skip..run.len() - cut]) {
                return Some((skip, c));
            }
        }
    }
    None
}

pub enum SearchMsg {
    Progress { percent: u32, found: usize },
    Done { project: u64, maps: Vec<MapDef>, overview: Overview, cancelled: bool, recognized: usize },
}

pub struct SearchHandle {
    pub cancel: Arc<AtomicBool>,
}

/// Everything the background search does, from bytes to a de-duplicated map list.
pub fn run_search(d: &[u8], rules: &SearchRules, cancel: &AtomicBool, report: &dyn Fn(u32, usize)) -> Vec<MapDef> {
    let mut all: Vec<MapDef> = vec![];
    let orgs = [DataOrg::HiLo, DataOrg::LoHi, DataOrg::Byte];
    let total = orgs.len() * 2;
    let size = d.len().max(1);
    if rules.axis_maps {
        for (k, org) in orgs.iter().enumerate() {
            let found = all.len();
            let f = |a: usize| {
                report(((k as f64 + a as f64 / size as f64) * 100.0 / total as f64) as u32, found);
                !cancel.load(Ordering::Relaxed)
            };
            all.extend(axis_maps(d, *org, rules, &f));
        }
    }
    // When both byte orders give the same place, the one with the calmer data is the real one.
    all = remove_overlaps(all, d);
    if rules.statistical {
        for (k, org) in orgs.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let taken: Vec<(usize, usize)> = all.iter().map(|m| (m.start, m.end())).collect();
            let found = all.len();
            let f = |a: usize| {
                report((((orgs.len() + k) as f64 + a as f64 / size as f64) * 100.0 / total as f64) as u32, found);
                !cancel.load(Ordering::Relaxed)
            };
            let mut found_maps = statistical_maps(d, *org, rules, &taken, &f);
            all.append(&mut found_maps);
            all = remove_overlaps(all, d);
        }
    }
    all.sort_by_key(|m| m.start);
    all
}

fn calmness(m: &MapDef, d: &[u8]) -> f64 {
    let w = m.org.width();
    let mut s = 0.0;
    for i in 1..m.cells() {
        let (a, b) = (m.org.read(d, m.start + i * w, false), m.org.read(d, m.start + (i - 1) * w, false));
        if let (Some(a), Some(b)) = (a, b) {
            s += (a - b).abs();
        }
    }
    let span = m.org.limits(false).1.max(1.0);
    s / m.cells().max(1) as f64 / span
}

fn remove_overlaps(mut all: Vec<MapDef>, d: &[u8]) -> Vec<MapDef> {
    // Axis-carrying maps first, then the calmer data of two readings of the same bytes.
    let rank = |m: &MapDef| -> u8 {
        if m.potential_label.as_deref().is_some_and(|s| s.contains("Statistical")) {
            1
        } else {
            0
        }
    };
    all.sort_by(|a, b| {
        rank(a).cmp(&rank(b)).then(calmness(a, d).partial_cmp(&calmness(b, d)).unwrap_or(std::cmp::Ordering::Equal))
    });
    let mut kept: Vec<MapDef> = vec![];
    for m in all {
        if !kept.iter().any(|k| m.start < k.end() && k.start < m.end()) {
            kept.push(m);
        }
    }
    kept.sort_by_key(|m| m.start);
    kept
}

pub fn spawn_search(project: u64, data: Vec<u8>, maps: Vec<MapDef>, rules: SearchRules, tx: Sender<SearchMsg>, wake: egui::Context) -> SearchHandle {
    let cancel = Arc::new(AtomicBool::new(false));
    let c2 = cancel.clone();
    std::thread::spawn(move || {
        let tx2 = tx.clone();
        let wake2 = wake.clone();
        let last = AtomicU32::new(0);
        let report = move |pct: u32, found: usize| {
            if last.swap(pct, Ordering::Relaxed) != pct {
                let _ = tx2.send(SearchMsg::Progress { percent: pct, found });
                wake2.request_repaint();
            }
        };
        let mut found = run_search(&data, &rules, &c2, &report);
        // Anything the person already registered isn't a potential map any more.
        found.retain(|m| !maps.iter().any(|k| k.start < m.end() && m.start < k.end()));
        let overview = build_overview(&data, &maps, &found);
        let cancelled = c2.load(Ordering::Relaxed);
        let _ = tx.send(SearchMsg::Done { project, maps: found, overview, cancelled, recognized: 0 });
        wake.request_repaint();
    });
    SearchHandle { cancel }
}

/// Tidies a rough selection: finds the row width and the start of the first full row.
/// Returns (start, cols, rows) in bytes and cells.
pub fn support_selection(d: &[u8], from: usize, to: usize, org: DataOrg) -> Option<(usize, usize, usize)> {
    let w = org.width();
    let cells = (to + 1).saturating_sub(from) / w;
    if cells < STAT_MIN_COLS * 2 {
        return None;
    }
    let v: Vec<f64> = (0..cells).map(|i| org.read(d, from + i * w, false).unwrap_or(0.0)).collect();
    let cols = best_cols(&v)?;
    // The row start sits right after the biggest recurring jump.
    let mut jump = vec![0.0; cols];
    for i in 1..v.len() {
        jump[i % cols] += (v[i] - v[i - 1]).abs();
    }
    let shift = jump
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|p| p.0)
        .unwrap_or(0);
    let start_cell = if shift == 0 { 0 } else { shift };
    let rows = (cells - start_cell) / cols;
    if rows == 0 {
        return None;
    }
    Some((from + start_cell * w, cols, rows))
}

/// Finds ECU facts in the bytes: the text `0281010000 1037360000` gives the
/// producer Bosch, the hardware number 0281010000 and the software number 360000.
#[derive(Default, Clone, Debug, PartialEq)]
pub struct VehicleFacts {
    pub ecu_producer: Option<String>,
    pub ecu_prod: Option<String>,
    pub sw_version: Option<String>,
    pub processor: Option<String>,
    pub ecu_build: Option<String>,
}

pub fn search_vehicle_data(d: &[u8]) -> VehicleFacts {
    let mut f = VehicleFacts::default();
    let digits = |s: &[u8]| String::from_utf8_lossy(s).to_string();
    if let Ok(re) = Regex::new(r"(?-u)\b(0[23]\d1\d{6})\b") {
        if let Some(m) = re.find(d) {
            f.ecu_prod = Some(digits(m.as_bytes()));
            f.ecu_producer = Some("Bosch".into());
        }
    }
    if let Ok(re) = Regex::new(r"(?-u)\b(103[0-9]\d{6})\b") {
        if let Some(m) = re.find(d) {
            let s = digits(m.as_bytes());
            f.sw_version = Some(s[s.len() - 6..].to_string());
            f.ecu_producer.get_or_insert("Bosch".into());
        }
    }
    if let Ok(re) = Regex::new(r"(?-u)\b(M[EDS][DGV]?\s?\d{1,2}(?:\.\d{1,2})*|EDC\s?1[5-7][A-Z]{0,3}\d{0,2}\+?|SIMOS\s?\d(?:\.\d)?|SID\s?\d{3})\b") {
        if let Some(m) = re.find(d) {
            f.ecu_build = Some(digits(m.as_bytes()));
        }
    }
    f
}

pub fn guess_processor(d: &[u8]) -> Option<&'static str> {
    // Two-byte values in maps show the byte order. With no maps this stays unknown.
    let sample = axis_maps(d, DataOrg::HiLo, &SearchRules::default(), &|_| true).len();
    let le = axis_maps(d, DataOrg::LoHi, &SearchRules::default(), &|_| true).len();
    if sample == 0 && le == 0 {
        None
    } else if sample >= le {
        Some("Motorola (HiLo)")
    } else {
        Some("Intel (LoHi)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map_a(d: &mut [u8], at: usize, nx: usize, ny: usize, be: bool) -> usize {
        let org = if be { DataOrg::HiLo } else { DataOrg::LoHi };
        let mut p = at;
        let mut put = |p: &mut usize, v: u32| {
            d[*p..*p + 2].copy_from_slice(&org.encode(v as f64, false));
            *p += 2;
        };
        put(&mut p, nx as u32);
        put(&mut p, ny as u32);
        for i in 0..nx {
            put(&mut p, 1000 + i as u32 * 400);
        }
        for j in 0..ny {
            put(&mut p, 100 + j as u32 * 150);
        }
        let v = p;
        for j in 0..ny {
            for i in 0..nx {
                put(&mut p, 2000 + (i * j * 18) as u32 + i as u32 * 7);
            }
        }
        v
    }

    fn noise(n: usize) -> Vec<u8> {
        let mut x = 0x2545F491u32;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                x as u8
            })
            .collect()
    }

    #[test]
    fn finds_axis_map_in_both_byte_orders() {
        for be in [true, false] {
            let mut d = noise(0x4000);
            let v = map_a(&mut d, 0x1000, 16, 12, be);
            let found = run_search(&d, &SearchRules::default(), &AtomicBool::new(false), &|_, _| {});
            let m = found.iter().find(|m| m.start == v).unwrap_or_else(|| panic!("map not found be={be}: {found:?}"));
            assert_eq!((m.cols, m.rows), (16, 12));
            assert_eq!(m.org, if be { DataOrg::HiLo } else { DataOrg::LoHi });
            assert_eq!(m.x.src, AxisSrc::Rom);
            assert_eq!(m.x.addr, 0x1004);
            assert_eq!(m.y.addr, 0x1004 + 32);
        }
    }

    #[test]
    fn plain_table_found_by_look_with_right_width() {
        let mut d = noise(0x4000);
        let org = DataOrg::HiLo;
        let at = 0x2000;
        for j in 0..10 {
            for i in 0..14 {
                let v = 3000.0 + i as f64 * 120.0 + j as f64 * 40.0;
                d[at + (j * 14 + i) * 2..at + (j * 14 + i) * 2 + 2].copy_from_slice(&org.encode(v, false));
            }
        }
        let found = run_search(&d, &SearchRules::default(), &AtomicBool::new(false), &|_, _| {});
        let m = found.iter().find(|m| m.start.abs_diff(at) < 40).expect("table not found");
        assert_eq!(m.cols, 14);
        assert_eq!(m.org, org);
    }

    #[test]
    fn table_with_noise_on_both_sides_keeps_its_exact_place() {
        let mut x = 7u32;
        let mut v: Vec<f64> = (0..5)
            .map(|_| {
                x = x.wrapping_mul(2654435761).wrapping_add(11);
                (x >> 16) as f64
            })
            .collect();
        for j in 0..9 {
            for i in 0..14 {
                v.push(3000.0 + i as f64 * 120.0 + j as f64 * 40.0 + 50.0 * (i as f64).sin());
            }
        }
        for _ in 0..4 {
            x = x.wrapping_mul(2654435761).wrapping_add(11);
            v.push((x >> 16) as f64);
        }
        assert_eq!(trimmed_cols(&v), Some((5, 14)));
    }

    #[test]
    fn noise_and_flat_data_give_nothing_much() {
        let d = noise(0x8000);
        let found = run_search(&d, &SearchRules::default(), &AtomicBool::new(false), &|_, _| {});
        assert!(found.len() < 4, "too many false maps in noise: {}", found.len());
        let flat = vec![0xFFu8; 0x8000];
        assert!(run_search(&flat, &SearchRules::default(), &AtomicBool::new(false), &|_, _| {}).is_empty());
    }

    #[test]
    fn rules_ignore_dull_values() {
        let r = SearchRules::default();
        assert!(boring(&[5; 16], 0, 16, DataOrg::Byte, &r));
        let mut d = vec![5u8; 16];
        d[3] = 6;
        assert!(boring(&d, 0, 16, DataOrg::Byte, &r), "mostly constant");
        let alt: Vec<u8> = (0..16).map(|i| if i % 2 == 0 { 1 } else { 2 }).collect();
        assert!(boring(&alt, 0, 16, DataOrg::Byte, &r), "alternating");
        let good: Vec<u8> = (0..16).collect();
        assert!(!boring(&good, 0, 16, DataOrg::Byte, &r));
    }

    #[test]
    fn overview_classes() {
        let mut d = vec![0xFFu8; 64];
        d.extend(noise(64));
        d.extend((0..64).map(|i| (i / 8) as u8 + 10));
        let o = build_overview(&d, &[], &[]);
        assert_eq!(o.kind_at(0), Some(AreaKind::Empty));
        let code = (64..128).step_by(16).filter(|&a| o.kind_at(a) == Some(AreaKind::Code)).count();
        assert!(code >= 3, "noise should read as program code, got {code} of 4 blocks");
        assert_eq!(o.kind_at(130), Some(AreaKind::Unrecognized));
        assert!(o.is_pale(0) && !o.is_pale(130));
    }

    #[test]
    fn vehicle_data_as_seen_in_the_original() {
        let f = search_vehicle_data(b"xx 0281010000 1037360000 DEMO");
        assert_eq!(f.ecu_prod.as_deref(), Some("0281010000"));
        assert_eq!(f.sw_version.as_deref(), Some("360000"));
        assert_eq!(f.ecu_producer.as_deref(), Some("Bosch"));
    }

    #[test]
    fn selection_support_finds_the_columns() {
        let org = DataOrg::Byte;
        let mut d = vec![0u8; 400];
        for j in 0..12 {
            for i in 0..10 {
                d[20 + j * 10 + i] = (20 + i * 15 + j) as u8;
            }
        }
        let (start, cols, rows) = support_selection(&d, 20, 139, org).unwrap();
        assert_eq!(cols, 10);
        assert_eq!(start, 20);
        assert_eq!(rows, 12);
    }
}

