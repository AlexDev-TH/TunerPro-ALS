//! Projects, versions, maps and the undo history. Nothing here knows about the screen.
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default, Hash)]
pub enum DataOrg {
    #[default]
    Byte,
    LoHi,
    HiLo,
    LoHiLoHi,
    HiLoHiLo,
    FloatLoHi,
    FloatHiLo,
}

impl DataOrg {
    pub fn width(self) -> usize {
        match self {
            DataOrg::Byte => 1,
            DataOrg::LoHi | DataOrg::HiLo => 2,
            _ => 4,
        }
    }
    pub fn is_float(self) -> bool {
        matches!(self, DataOrg::FloatLoHi | DataOrg::FloatHiLo)
    }
    pub fn big_endian(self) -> bool {
        matches!(self, DataOrg::HiLo | DataOrg::HiLoHiLo | DataOrg::FloatHiLo)
    }
    pub fn label(self) -> &'static str {
        match self {
            DataOrg::Byte => "8 Bit",
            DataOrg::LoHi => "16 Bit (LoHi)",
            DataOrg::HiLo => "16 Bit (HiLo)",
            DataOrg::LoHiLoHi => "32 Bit (LoHi)",
            DataOrg::HiLoHiLo => "32 Bit (HiLo)",
            DataOrg::FloatLoHi => "Float (LoHi)",
            DataOrg::FloatHiLo => "Float (HiLo)",
        }
    }
    pub const ALL: [DataOrg; 7] = [
        DataOrg::Byte,
        DataOrg::LoHi,
        DataOrg::HiLo,
        DataOrg::LoHiLoHi,
        DataOrg::HiLoHiLo,
        DataOrg::FloatLoHi,
        DataOrg::FloatHiLo,
    ];
    /// Same bit width, other byte order.
    pub fn swapped(self) -> DataOrg {
        match self {
            DataOrg::LoHi => DataOrg::HiLo,
            DataOrg::HiLo => DataOrg::LoHi,
            DataOrg::LoHiLoHi => DataOrg::HiLoHiLo,
            DataOrg::HiLoHiLo => DataOrg::LoHiLoHi,
            DataOrg::FloatLoHi => DataOrg::FloatHiLo,
            DataOrg::FloatHiLo => DataOrg::FloatLoHi,
            DataOrg::Byte => DataOrg::Byte,
        }
    }
    /// The stored number at `addr`, or `None` when it runs past the end of the data.
    pub fn read(self, d: &[u8], addr: usize, signed: bool) -> Option<f64> {
        let w = self.width();
        let b = d.get(addr..addr.checked_add(w)?)?;
        Some(match self {
            DataOrg::Byte => {
                if signed {
                    b[0] as i8 as f64
                } else {
                    b[0] as f64
                }
            }
            DataOrg::LoHi | DataOrg::HiLo => {
                let v = if self.big_endian() {
                    u16::from_be_bytes([b[0], b[1]])
                } else {
                    u16::from_le_bytes([b[0], b[1]])
                };
                if signed {
                    v as i16 as f64
                } else {
                    v as f64
                }
            }
            DataOrg::LoHiLoHi | DataOrg::HiLoHiLo => {
                let a = [b[0], b[1], b[2], b[3]];
                let v = if self.big_endian() {
                    u32::from_be_bytes(a)
                } else {
                    u32::from_le_bytes(a)
                };
                if signed {
                    v as i32 as f64
                } else {
                    v as f64
                }
            }
            DataOrg::FloatLoHi | DataOrg::FloatHiLo => {
                let a = [b[0], b[1], b[2], b[3]];
                (if self.big_endian() {
                    f32::from_be_bytes(a)
                } else {
                    f32::from_le_bytes(a)
                }) as f64
            }
        })
    }
    /// The bytes that store `v`, clamped into what the cell can hold.
    pub fn encode(self, v: f64, signed: bool) -> Vec<u8> {
        let v = if v.is_nan() { 0.0 } else { v };
        let (lo, hi) = self.limits(signed);
        let c = v.max(lo).min(hi);
        let r = c.round();
        match self {
            DataOrg::Byte => vec![if signed { (r as i64 as i8) as u8 } else { r as u8 }],
            DataOrg::LoHi | DataOrg::HiLo => {
                let u = if signed { (r as i64 as i16) as u16 } else { r as u16 };
                if self.big_endian() {
                    u.to_be_bytes().to_vec()
                } else {
                    u.to_le_bytes().to_vec()
                }
            }
            DataOrg::LoHiLoHi | DataOrg::HiLoHiLo => {
                let u = if signed { (r as i64 as i32) as u32 } else { r as u32 };
                if self.big_endian() {
                    u.to_be_bytes().to_vec()
                } else {
                    u.to_le_bytes().to_vec()
                }
            }
            DataOrg::FloatLoHi | DataOrg::FloatHiLo => {
                let f = c as f32;
                if self.big_endian() {
                    f.to_be_bytes().to_vec()
                } else {
                    f.to_le_bytes().to_vec()
                }
            }
        }
    }
    pub fn limits(self, signed: bool) -> (f64, f64) {
        match (self.width(), self.is_float(), signed) {
            (_, true, _) => (f32::MIN as f64, f32::MAX as f64),
            (1, _, false) => (0.0, 255.0),
            (1, _, true) => (-128.0, 127.0),
            (2, _, false) => (0.0, 65535.0),
            (2, _, true) => (-32768.0, 32767.0),
            (_, _, false) => (0.0, 4294967295.0),
            (_, _, true) => (-2147483648.0, 2147483647.0),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub enum AxisSrc {
    None,
    #[default]
    Enumerate,
    Rom,
    RomAdd,
    RomSub,
    User,
    RomBackwards,
}

impl AxisSrc {
    pub fn label(self) -> &'static str {
        match self {
            AxisSrc::None => "No axis",
            AxisSrc::Enumerate => "1, 2, 3, ...",
            AxisSrc::Rom => "Values in the file",
            AxisSrc::RomAdd => "Values in the file, adding up",
            AxisSrc::RomSub => "Values in the file, counting down",
            AxisSrc::User => "Free editable",
            AxisSrc::RomBackwards => "Values in the file, backwards",
        }
    }
    pub const ALL: [AxisSrc; 7] = [
        AxisSrc::None,
        AxisSrc::Enumerate,
        AxisSrc::Rom,
        AxisSrc::RomAdd,
        AxisSrc::RomSub,
        AxisSrc::User,
        AxisSrc::RomBackwards,
    ];
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct Axis {
    pub desc: String,
    pub unit: String,
    pub src: AxisSrc,
    pub addr: usize,
    pub org: DataOrg,
    pub radix: u32,
    pub signed: bool,
    pub reciprocal: bool,
    pub factor: f64,
    pub offset: f64,
    pub precision: u32,
    pub skip: usize,
    pub signature: Option<u32>,
    pub user: Vec<f64>,
}

impl Default for Axis {
    fn default() -> Self {
        Axis {
            desc: "-".into(),
            unit: "-".into(),
            src: AxisSrc::Enumerate,
            addr: 0,
            org: DataOrg::Byte,
            radix: 10,
            signed: false,
            reciprocal: false,
            factor: 1.0,
            offset: 0.0,
            precision: 0,
            skip: 0,
            signature: None,
            user: vec![],
        }
    }
}

impl Axis {
    pub fn value(&self, d: &[u8], i: usize) -> f64 {
        let step = self.org.width() + self.skip;
        let raw_at = |k: usize| self.org.read(d, self.addr + k * step, self.signed).unwrap_or(0.0);
        let raw = match self.src {
            AxisSrc::None | AxisSrc::Enumerate => return (i + 1) as f64,
            AxisSrc::User => return self.user.get(i).copied().unwrap_or((i + 1) as f64),
            AxisSrc::Rom => raw_at(i),
            AxisSrc::RomBackwards => raw_at(i),
            AxisSrc::RomAdd => (0..=i).map(raw_at).sum(),
            AxisSrc::RomSub => {
                let first = raw_at(0);
                first - (1..=i).map(raw_at).sum::<f64>()
            }
        };
        let r = if self.reciprocal && raw != 0.0 { 1.0 / raw } else { raw };
        r * self.factor + self.offset
    }
    /// The axis values for `n` cells, reversed when the source is backwards.
    pub fn values(&self, d: &[u8], n: usize) -> Vec<f64> {
        let mut v: Vec<f64> = (0..n).map(|i| self.value(d, i)).collect();
        if self.src == AxisSrc::RomBackwards {
            v.reverse();
        }
        v
    }
    pub fn text(&self, v: f64) -> String {
        if self.radix == 16 && self.factor == 1.0 && self.offset == 0.0 {
            format!("{:X}", v as i64)
        } else {
            format!("{:.*}", self.precision as usize, v)
        }
    }
    pub fn byte_len(&self, n: usize) -> usize {
        match self.src {
            AxisSrc::None | AxisSrc::Enumerate | AxisSrc::User => 0,
            _ => n * (self.org.width() + self.skip),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub enum MapKind {
    Single,
    OneDim,
    #[default]
    TwoDim,
    TwoDimInv,
}

impl MapKind {
    pub fn label(self) -> &'static str {
        match self {
            MapKind::Single => "Single value",
            MapKind::OneDim => "Onedimensional",
            MapKind::TwoDim => "Twodimensional",
            MapKind::TwoDimInv => "Twodimensional, inverted",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub enum RightSide {
    Nothing,
    #[default]
    Bars,
    Ascii,
    Both,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub enum ViewMode {
    #[default]
    Text,
    TwoD,
    ThreeD,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct MapDef {
    pub uid: u64,
    pub name: String,
    pub id_name: String,
    pub folder: String,
    pub desc: String,
    pub unit: String,
    pub comment: String,
    pub kind: MapKind,
    pub start: usize,
    pub cols: usize,
    pub rows: usize,
    pub org: DataOrg,
    pub radix: u32,
    pub signed: bool,
    pub reciprocal: bool,
    pub mirror: bool,
    pub factor: f64,
    pub offset: f64,
    pub precision: u32,
    pub range_lo: f64,
    pub range_hi: f64,
    pub var_offset: Option<u64>,
    pub right: RightSide,
    pub view: ViewMode,
    pub x: Axis,
    pub y: Axis,
    pub hidden: bool,
    pub flag: bool,
    pub potential_label: Option<String>,
    /// Maps that were found from a pack or a script keep which one they came from.
    pub parallel_group: Option<u64>,
}

impl Default for MapDef {
    fn default() -> Self {
        MapDef {
            uid: 0,
            name: "Map".into(),
            id_name: String::new(),
            folder: "Maps".into(),
            desc: "-".into(),
            unit: "-".into(),
            comment: String::new(),
            kind: MapKind::TwoDim,
            start: 0,
            cols: 8,
            rows: 8,
            org: DataOrg::Byte,
            radix: 10,
            signed: false,
            reciprocal: false,
            mirror: false,
            factor: 1.0,
            offset: 0.0,
            precision: 0,
            range_lo: 0.0,
            range_hi: 255.0,
            var_offset: None,
            right: RightSide::Bars,
            view: ViewMode::Text,
            x: Axis::default(),
            y: Axis::default(),
            hidden: false,
            flag: false,
            potential_label: None,
            parallel_group: None,
        }
    }
}

impl MapDef {
    pub fn cells(&self) -> usize {
        self.cols * self.rows
    }
    pub fn byte_len(&self) -> usize {
        self.cells() * self.org.width()
    }
    pub fn end(&self) -> usize {
        self.start + self.byte_len()
    }
    pub fn contains(&self, addr: usize) -> bool {
        addr >= self.start && addr < self.end()
    }
    /// Sets the display range from the data under the map.
    pub fn auto_range(&mut self, d: &[u8]) {
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for i in 0..self.cells() {
            if let Some(v) = self.org.read(d, self.start + i * self.org.width(), self.signed) {
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
        if lo <= hi {
            self.range_lo = lo;
            self.range_hi = if hi > lo { hi } else { lo + 1.0 };
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[serde(default)]
pub struct Version {
    pub name: String,
    pub comment: String,
    pub folder: String,
    pub linked: Vec<String>,
    pub created: String,
    #[serde(skip)]
    pub data: Vec<u8>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[serde(default)]
pub struct Element {
    pub usage: String,
    pub start: usize,
    pub end: usize,
    pub display_offset: usize,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[serde(default)]
pub struct Comment {
    pub addr: usize,
    pub end: usize,
    pub text: String,
    pub color: [u8; 3],
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub enum CsAlgo {
    /// Sum of all values stays the same.
    #[default]
    Additive,
    /// Same data at another place.
    Copy,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[serde(default)]
pub struct Checksum {
    pub name: String,
    pub algo: CsAlgo,
    pub from: usize,
    pub to: usize, // inclusive, as the checksum dialog shows it
    pub org: DataOrg,
    pub target: usize,
    pub target_end: usize,
    pub auto: bool,
    pub fullbyte: bool,
    pub display_only: bool,
    pub expected: u64,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[serde(default)]
pub struct SyncBlock {
    pub a: usize,
    pub b: usize,
    pub len: usize,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct Props {
    pub client_name: String,
    pub licence: String,
    pub state: String,
    pub vehicle_type: String,
    pub producer: String,
    pub chassis: String,
    pub model: String,
    pub model_year: String,
    pub vin: String,
    pub user: [String; 5],
    pub usage: String,
    pub ecu_producer: String,
    pub ecu_build: String,
    pub ecu_prod: String,
    pub ecu_stg: String,
    pub sw_version: String,
    pub sw_size: String,
    pub processor: String,
    pub engine_name: String,
    pub engine_type: String,
    pub displacement: String,
    pub ps: String,
    pub kw: String,
    pub transmission: String,
    pub emission: String,
    pub comment: String,
    pub linked: Vec<String>,
    pub created: String,
    pub changed: String,
    pub created_by: String,
    pub changed_by: String,
    pub project_type: String,
    pub file_name: String,
}

impl Default for Props {
    fn default() -> Self {
        Props {
            client_name: String::new(),
            licence: String::new(),
            state: "in development".into(),
            vehicle_type: "Passenger car".into(),
            producer: String::new(),
            chassis: String::new(),
            model: String::new(),
            model_year: String::new(),
            vin: String::new(),
            user: Default::default(),
            usage: "Engine".into(),
            ecu_producer: String::new(),
            ecu_build: String::new(),
            ecu_prod: String::new(),
            ecu_stg: String::new(),
            sw_version: String::new(),
            sw_size: String::new(),
            processor: String::new(),
            engine_name: String::new(),
            engine_type: String::new(),
            displacement: String::new(),
            ps: String::new(),
            kw: String::new(),
            transmission: String::new(),
            emission: String::new(),
            comment: String::new(),
            linked: vec![],
            created: String::new(),
            changed: String::new(),
            created_by: String::new(),
            changed_by: String::new(),
            project_type: "Binary data".into(),
            file_name: String::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[serde(default)]
pub struct Reference {
    pub version: usize,
    pub min_diff: f64,
    pub offset: i64,
    pub external_name: Option<String>,
    #[serde(skip)]
    pub external: Option<std::sync::Arc<Vec<u8>>>,
}

#[derive(Clone, Debug)]
pub struct Edit {
    pub version: usize,
    pub addr: usize,
    pub old: Vec<u8>,
    pub new: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct EditGroup {
    pub label: String,
    pub edits: Vec<Edit>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ProjectFile {
    format: u32,
    props: Props,
    versions: Vec<Version>,
    current: usize,
    reference: Reference,
    elements: Vec<Element>,
    active_element: Option<usize>,
    maps: Vec<MapDef>,
    potential: Vec<MapDef>,
    search_done: bool,
    comments: Vec<Comment>,
    markers: Vec<usize>,
    checksums: Vec<Checksum>,
    syncs: Vec<SyncBlock>,
    protect_outside: bool,
    next_uid: u64,
    profiles: Vec<AxisProfile>,
}

/// One remembered axis description, grouped by a named profile.
#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[serde(default)]
pub struct AxisProfile {
    pub profile: String,
    pub desc: String,
    pub unit: String,
    pub factor: f64,
    pub offset: f64,
    pub precision: u32,
    pub signed: bool,
    pub signature: Option<u32>,
}

pub struct Project {
    pub id: u64,
    pub path: Option<PathBuf>,
    pub props: Props,
    pub versions: Vec<Version>,
    pub current: usize,
    pub reference: Reference,
    pub elements: Vec<Element>,
    pub active_element: Option<usize>,
    pub maps: Vec<MapDef>,
    pub potential: Vec<MapDef>,
    pub search_done: bool,
    pub comments: Vec<Comment>,
    pub markers: Vec<usize>,
    pub checksums: Vec<Checksum>,
    pub syncs: Vec<SyncBlock>,
    pub protect_outside: bool,
    pub next_uid: u64,
    pub profiles: Vec<AxisProfile>,
    pub undo: Vec<EditGroup>,
    pub redo: Vec<EditGroup>,
    pub open_group: Option<EditGroup>,
    pub dirty_data: bool,
    pub dirty_struct: bool,
    pub overview: Option<crate::analysis::Overview>,
    pub revision: u64,
    pub in_sync: bool,
}

pub const MAX_ELEMENTS: usize = 8;

/// Today's date from the system clock, in day.month.year form.
pub fn today() -> String {
    let s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, m, d) = civil(s / 86400);
    format!("{d}.{m}.{y}")
}

fn civil(days: u64) -> (i64, u32, u32) {
    let z = days as i64 + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

impl Project {
    pub fn new(id: u64) -> Project {
        Project {
            id,
            path: None,
            props: Props { created: today(), changed: today(), ..Props::default() },
            versions: vec![],
            current: 0,
            reference: Reference::default(),
            elements: vec![],
            active_element: None,
            maps: vec![],
            potential: vec![],
            search_done: false,
            comments: vec![],
            markers: vec![],
            checksums: vec![],
            syncs: vec![],
            protect_outside: false,
            next_uid: 1,
            profiles: vec![],
            undo: vec![],
            redo: vec![],
            open_group: None,
            dirty_data: false,
            dirty_struct: false,
            overview: None,
            revision: 1,
            in_sync: false,
        }
    }

    pub fn uid(&mut self) -> u64 {
        let u = self.next_uid;
        self.next_uid += 1;
        u
    }

    pub fn size(&self) -> usize {
        self.versions.first().map(|v| v.data.len()).unwrap_or(0)
    }

    pub fn version_name(&self) -> String {
        self.versions.get(self.current).map(|v| v.name.clone()).unwrap_or_default()
    }

    pub fn data(&self) -> &[u8] {
        self.versions.get(self.current).map(|v| v.data.as_slice()).unwrap_or(&[])
    }

    /// The reference data: normally the first version, or what the person picked.
    pub fn ref_data(&self) -> &[u8] {
        if let Some(e) = &self.reference.external {
            return e.as_slice();
        }
        self.versions
            .get(self.reference.version)
            .or_else(|| self.versions.first())
            .map(|v| v.data.as_slice())
            .unwrap_or(&[])
    }

    /// Address in the reference data for an address in this project.
    pub fn ref_addr(&self, addr: usize) -> Option<usize> {
        let a = addr as i64 + self.reference.offset;
        (a >= 0).then_some(a as usize)
    }

    pub fn ref_value(&self, org: DataOrg, addr: usize, signed: bool) -> Option<f64> {
        org.read(self.ref_data(), self.ref_addr(addr)?, signed)
    }

    /// Whether a cell counts as changed from the reference, honouring the minimum difference.
    pub fn is_changed(&self, org: DataOrg, addr: usize, signed: bool) -> bool {
        let w = org.width();
        let (Some(a), Some(r)) = (self.data().get(addr..addr + w), self.ref_addr(addr)) else {
            return false;
        };
        let Some(b) = self.ref_data().get(r..r + w) else { return false };
        if a == b {
            return false;
        }
        if self.reference.min_diff > 0.0 {
            let (Some(x), Some(y)) = (org.read(self.data(), addr, signed), org.read(self.ref_data(), r, signed)) else {
                return false;
            };
            return (x - y).abs() >= self.reference.min_diff;
        }
        true
    }

    /// The address range the active element covers (the whole data for `<All elements>`).
    pub fn element_range(&self) -> (usize, usize) {
        match self.active_element.and_then(|i| self.elements.get(i)) {
            Some(e) => (e.start, (e.end + 1).min(self.size())),
            None => (0, self.size()),
        }
    }

    pub fn element_label(&self) -> String {
        match self.active_element.and_then(|i| self.elements.get(i)) {
            Some(e) => e.usage.clone(),
            None => "<All elements>".into(),
        }
    }

    pub fn map_at(&self, addr: usize) -> Option<&MapDef> {
        self.maps.iter().find(|m| m.contains(addr))
    }

    // ---------- editing ----------
    pub fn begin_edit(&mut self, label: &str) {
        if self.open_group.is_none() {
            self.open_group = Some(EditGroup { label: label.into(), edits: vec![] });
        }
    }

    /// Writes into the current version, remembers it for undo and keeps sync blocks identical.
    pub fn write(&mut self, addr: usize, bytes: &[u8]) {
        let ver = self.current;
        self.write_raw(ver, addr, bytes);
        if !self.in_sync {
            self.in_sync = true;
            let syncs = self.syncs.clone();
            for s in syncs {
                for (from, to) in [(s.a, s.b), (s.b, s.a)] {
                    let lo = addr.max(from);
                    let hi = (addr + bytes.len()).min(from + s.len);
                    if lo < hi {
                        let chunk = bytes[lo - addr..hi - addr].to_vec();
                        self.write_raw(ver, to + (lo - from), &chunk);
                    }
                }
            }
            self.in_sync = false;
        }
    }

    fn write_raw(&mut self, ver: usize, addr: usize, bytes: &[u8]) {
        let Some(v) = self.versions.get_mut(ver) else { return };
        if addr >= v.data.len() {
            return;
        }
        let end = (addr + bytes.len()).min(v.data.len());
        let n = end - addr;
        if v.data[addr..end] == bytes[..n] {
            return;
        }
        let old = v.data[addr..end].to_vec();
        v.data[addr..end].copy_from_slice(&bytes[..n]);
        let edit = Edit { version: ver, addr, old, new: bytes[..n].to_vec() };
        match &mut self.open_group {
            Some(g) => g.edits.push(edit),
            None => {
                self.undo.push(EditGroup { label: "Change".into(), edits: vec![edit] });
                self.redo.clear();
            }
        }
        self.dirty_data = true;
        self.revision += 1;
    }

    pub fn end_edit(&mut self) {
        crate::checksum::auto_correct(self);
        if let Some(g) = self.open_group.take() {
            if !g.edits.is_empty() {
                self.undo.push(g);
                self.redo.clear();
                if self.undo.len() > 2000 {
                    self.undo.remove(0);
                }
            }
        }
    }

    pub fn undo_one(&mut self) -> bool {
        let Some(g) = self.undo.pop() else { return false };
        for e in g.edits.iter().rev() {
            if let Some(v) = self.versions.get_mut(e.version) {
                v.data[e.addr..e.addr + e.old.len()].copy_from_slice(&e.old);
            }
        }
        self.redo.push(g);
        self.revision += 1;
        self.dirty_data = true;
        true
    }

    pub fn redo_one(&mut self) -> bool {
        let Some(g) = self.redo.pop() else { return false };
        for e in &g.edits {
            if let Some(v) = self.versions.get_mut(e.version) {
                v.data[e.addr..e.addr + e.new.len()].copy_from_slice(&e.new);
            }
        }
        self.undo.push(g);
        self.revision += 1;
        self.dirty_data = true;
        true
    }

    // ---------- versions ----------
    pub fn add_version(&mut self, name: &str, data: Vec<u8>) -> usize {
        let name = if name.is_empty() {
            if self.versions.is_empty() {
                "Original".to_string()
            } else {
                format!("Version {}", self.versions.len())
            }
        } else {
            name.to_string()
        };
        let mut data = data;
        if let Some(first) = self.versions.first() {
            data.resize(first.data.len(), 0xFF);
        }
        self.versions.push(Version { name, data, created: today(), ..Version::default() });
        self.dirty_data = true;
        self.revision += 1;
        if self.elements.is_empty() {
            let size = self.size();
            self.elements.push(Element {
                usage: "Data".into(),
                start: 0,
                end: size.saturating_sub(1),
                display_offset: 0,
            });
            self.active_element = Some(0);
            self.props.sw_size = format!("{size:X}");
        }
        self.versions.len() - 1
    }

    /// Makes version `i` the original; the old original takes its place.
    pub fn make_original(&mut self, i: usize) {
        if i < self.versions.len() && i != 0 {
            self.versions.swap(0, i);
            if self.current == i {
                self.current = 0;
            } else if self.current == 0 {
                self.current = i;
            }
            self.dirty_data = true;
            self.revision += 1;
        }
    }

    pub fn delete_version(&mut self, i: usize) {
        if self.versions.len() > 1 && i < self.versions.len() {
            self.versions.remove(i);
            if self.current >= self.versions.len() {
                self.current = self.versions.len() - 1;
            } else if self.current > i {
                self.current -= 1;
            }
            self.reference.version = 0;
            self.undo.clear();
            self.redo.clear();
            self.dirty_data = true;
            self.revision += 1;
        }
    }

    /// Grow or cut the project in every version; the checksums go first, as they would be wrong.
    pub fn resize(&mut self, new_size: usize, at_start: bool, fill: u8) {
        let old = self.size();
        for v in &mut self.versions {
            if new_size > old {
                if at_start {
                    let mut d = vec![fill; new_size - old];
                    d.extend_from_slice(&v.data);
                    v.data = d;
                } else {
                    v.data.resize(new_size, fill);
                }
            } else if at_start {
                v.data.drain(0..old - new_size);
            } else {
                v.data.truncate(new_size);
            }
        }
        self.checksums.clear();
        self.syncs.clear();
        self.undo.clear();
        self.redo.clear();
        for e in &mut self.elements {
            e.start = e.start.min(new_size.saturating_sub(1));
            e.end = e.end.min(new_size.saturating_sub(1));
        }
        if self.elements.len() == 1 {
            self.elements[0].end = new_size.saturating_sub(1);
        }
        self.dirty_data = true;
        self.revision += 1;
    }

    /// Names of maps that differ from the reference in version `v`.
    pub fn changed_maps(&self, v: usize) -> Vec<String> {
        let Some(ver) = self.versions.get(v) else { return vec![] };
        let r = self.ref_data();
        self.maps
            .iter()
            .filter(|m| {
                let e = m.end().min(ver.data.len());
                m.start < e && e <= r.len() && ver.data[m.start..e] != r[m.start..e]
            })
            .map(|m| m.name.clone())
            .collect()
    }

    /// Lists the ranges that differ between reference and current data, as (start, end-exclusive).
    pub fn changed_ranges(&self) -> Vec<(usize, usize)> {
        let (a, b) = (self.data(), self.ref_data());
        let n = a.len().min(b.len());
        let mut out = vec![];
        let mut i = 0;
        while i < n {
            if a[i] != b[i] {
                let s = i;
                while i < n && a[i] != b[i] {
                    i += 1;
                }
                out.push((s, i));
            } else {
                i += 1;
            }
        }
        out
    }

    // ---------- comments and markers ----------
    pub fn comment_at(&self, addr: usize) -> Option<&Comment> {
        self.comments.iter().find(|c| addr >= c.addr && addr <= c.end.max(c.addr))
    }

    pub fn toggle_marker(&mut self, addr: usize) {
        if let Some(i) = self.markers.iter().position(|&m| m == addr) {
            self.markers.remove(i);
        } else {
            self.markers.push(addr);
            self.markers.sort_unstable();
        }
        self.dirty_struct = true;
    }

    // ---------- saving ----------
    pub fn save(&mut self, path: &Path) -> Result<(), String> {
        self.props.changed = today();
        let pf = ProjectFile {
            format: 1,
            props: self.props.clone(),
            versions: self.versions.clone(),
            current: self.current,
            reference: self.reference.clone(),
            elements: self.elements.clone(),
            active_element: self.active_element,
            maps: self.maps.clone(),
            potential: self.potential.clone(),
            search_done: self.search_done,
            comments: self.comments.clone(),
            markers: self.markers.clone(),
            checksums: self.checksums.clone(),
            syncs: self.syncs.clone(),
            protect_outside: self.protect_outside,
            next_uid: self.next_uid,
            profiles: self.profiles.clone(),
        };
        let json = serde_json::to_vec_pretty(&pf).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("tmp");
        {
            let f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
            let mut z = zip::ZipWriter::new(f);
            let o = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            z.start_file("project.json", o).map_err(|e| e.to_string())?;
            z.write_all(&json).map_err(|e| e.to_string())?;
            for (i, v) in self.versions.iter().enumerate() {
                z.start_file(format!("v{i}.bin"), o).map_err(|e| e.to_string())?;
                z.write_all(&v.data).map_err(|e| e.to_string())?;
            }
            z.finish().map_err(|e| e.to_string())?;
        }
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
        self.path = Some(path.to_path_buf());
        self.dirty_data = false;
        self.dirty_struct = false;
        Ok(())
    }

    /// Reads only the facts about a project, for the list of projects.
    pub fn peek(path: &Path) -> Result<(Props, usize, usize), String> {
        let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut z = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
        let mut json = vec![];
        z.by_name("project.json").map_err(|e| e.to_string())?.read_to_end(&mut json).map_err(|e| e.to_string())?;
        let pf: ProjectFile = serde_json::from_slice(&json).map_err(|e| e.to_string())?;
        Ok((pf.props, pf.versions.len(), pf.maps.len()))
    }

    pub fn load(path: &Path, id: u64) -> Result<Project, String> {
        let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut z = zip::ZipArchive::new(f).map_err(|e| format!("Not a project file: {e}"))?;
        let mut json = vec![];
        z.by_name("project.json")
            .map_err(|e| e.to_string())?
            .read_to_end(&mut json)
            .map_err(|e| e.to_string())?;
        let pf: ProjectFile = serde_json::from_slice(&json).map_err(|e| e.to_string())?;
        let mut versions = pf.versions;
        for (i, v) in versions.iter_mut().enumerate() {
            z.by_name(&format!("v{i}.bin"))
                .map_err(|e| e.to_string())?
                .read_to_end(&mut v.data)
                .map_err(|e| e.to_string())?;
        }
        if versions.is_empty() {
            return Err("The project has no versions.".into());
        }
        let mut p = Project::new(id);
        p.path = Some(path.to_path_buf());
        p.props = pf.props;
        p.current = pf.current.min(versions.len() - 1);
        p.versions = versions;
        p.reference = pf.reference;
        p.elements = pf.elements;
        p.active_element = pf.active_element;
        p.maps = pf.maps;
        p.potential = pf.potential;
        p.search_done = pf.search_done;
        p.comments = pf.comments;
        p.markers = pf.markers;
        p.checksums = pf.checksums;
        p.syncs = pf.syncs;
        p.protect_outside = pf.protect_outside;
        p.next_uid = pf.next_uid.max(1);
        p.profiles = pf.profiles;
        Ok(p)
    }
}

/// The 8-bit sum shown for a version: the byte sum cut to 16 bits.
pub fn sum8(d: &[u8]) -> u32 {
    d.iter().fold(0u64, |a, &b| a + b as u64) as u32 & 0xFFFF
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_write_all_orgs() {
        for org in DataOrg::ALL {
            let mut d = vec![0u8; 8];
            let e = org.encode(200.0, false);
            d[2..2 + e.len()].copy_from_slice(&e);
            assert_eq!(org.read(&d, 2, false), Some(200.0), "{org:?}");
        }
        assert_eq!(DataOrg::HiLo.encode(0x1234 as f64, false), vec![0x12, 0x34]);
        assert_eq!(DataOrg::LoHi.encode(0x1234 as f64, false), vec![0x34, 0x12]);
        assert_eq!(DataOrg::Byte.encode(300.0, false), vec![255]);
        assert_eq!(DataOrg::Byte.read(&[0xFF], 0, true), Some(-1.0));
        assert_eq!(DataOrg::LoHi.read(&[1], 0, false), None);
    }

    #[test]
    fn undo_restores_bytes_and_sync_follows() {
        let mut p = Project::new(1);
        p.add_version("", vec![0; 32]);
        p.syncs.push(SyncBlock { a: 0, b: 16, len: 4 });
        p.begin_edit("t");
        p.write(1, &[9, 8]);
        p.end_edit();
        assert_eq!(&p.data()[0..4], &[0, 9, 8, 0]);
        assert_eq!(&p.data()[16..20], &[0, 9, 8, 0]);
        assert!(p.undo_one());
        assert!(p.data().iter().all(|&b| b == 0));
        assert!(p.redo_one());
        assert_eq!(p.data()[17], 9);
    }

    #[test]
    fn original_swap_and_size_lock() {
        let mut p = Project::new(1);
        p.add_version("", vec![1; 8]);
        let v = p.add_version("", vec![2; 4]);
        assert_eq!(p.versions[v].data.len(), 8, "versions share one size");
        p.make_original(1);
        assert_eq!(p.versions[0].data[0], 2);
        assert_eq!(p.versions[1].data[0], 1);
    }

    #[test]
    fn save_and_load_round_trip() {
        let mut p = Project::new(1);
        p.add_version("", vec![7; 300]);
        let uid = p.uid();
        p.maps.push(MapDef { uid, name: "Boost".into(), start: 10, ..MapDef::default() });
        p.comments.push(Comment { addr: 5, end: 5, text: "note".into(), color: [1, 2, 3] });
        let path = std::env::temp_dir().join("als_roundtrip_test.alsproj");
        p.save(&path).unwrap();
        let q = Project::load(&path, 2).unwrap();
        assert_eq!(q.versions[0].data, p.versions[0].data);
        assert_eq!(q.maps[0].name, "Boost");
        assert_eq!(q.comments[0].text, "note");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn eight_bit_sum_is_cut_to_16_bits() {
        assert_eq!(sum8(&[0xFF; 0x200]), (0xFF * 0x200) & 0xFFFF);
    }
}
