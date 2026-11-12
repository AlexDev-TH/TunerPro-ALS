//! Reads map definitions from ASAP2 (A2L) description files, a public text format.
use crate::model::*;
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
struct Layout {
    /// (position, kind, datatype)
    items: Vec<(i64, String, String)>,
}

#[derive(Debug, Clone, Default)]
struct Compu {
    unit: String,
    factor: f64,
    offset: f64,
}

#[derive(Debug, Clone, Default)]
pub struct A2lMap {
    pub map: MapDef,
}

fn tokenize(s: &str) -> Vec<String> {
    let mut out = vec![];
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
        } else if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if c == b'"' {
            let st = i + 1;
            i += 1;
            while i < b.len() && !(b[i] == b'"' && b[i - 1] != b'\\') {
                i += 1;
            }
            out.push(format!("\"{}", String::from_utf8_lossy(&b[st..i.min(b.len())])));
            i += 1;
        } else if c.is_ascii_whitespace() {
            i += 1;
        } else {
            let st = i;
            while i < b.len() && !b[i].is_ascii_whitespace() {
                i += 1;
            }
            out.push(String::from_utf8_lossy(&b[st..i]).to_string());
        }
    }
    out
}

fn text(t: &str) -> String {
    t.strip_prefix('"').unwrap_or(t).to_string()
}

fn num(t: &str) -> Option<i64> {
    let l = t.to_ascii_lowercase();
    if let Some(h) = l.strip_prefix("0x") {
        i64::from_str_radix(h, 16).ok()
    } else {
        l.parse::<i64>().ok().or_else(|| l.parse::<f64>().ok().map(|f| f as i64))
    }
}

fn data_org(t: &str, big: bool) -> DataOrg {
    match t {
        "UBYTE" | "SBYTE" => DataOrg::Byte,
        "UWORD" | "SWORD" => {
            if big {
                DataOrg::HiLo
            } else {
                DataOrg::LoHi
            }
        }
        "ULONG" | "SLONG" => {
            if big {
                DataOrg::HiLoHiLo
            } else {
                DataOrg::LoHiLoHi
            }
        }
        "FLOAT32_IEEE" => {
            if big {
                DataOrg::FloatHiLo
            } else {
                DataOrg::FloatLoHi
            }
        }
        _ => DataOrg::Byte,
    }
}

fn block_end(t: &[String], from: usize) -> usize {
    let mut depth = 0;
    let mut i = from;
    while i < t.len() {
        if t[i] == "/begin" {
            depth += 1;
        } else if t[i] == "/end" {
            depth -= 1;
            if depth == 0 {
                return i;
            }
        }
        i += 1;
    }
    t.len()
}

/// Maps found in the file. `offset` is subtracted from every address.
pub fn parse(s: &str, offset: i64, project_size: usize) -> Result<(Vec<A2lMap>, Vec<String>), String> {
    let t = tokenize(s);
    let mut layouts: HashMap<String, Layout> = HashMap::new();
    let mut compus: HashMap<String, Compu> = HashMap::new();
    let mut axis_pts: HashMap<String, (i64, String, String, String, usize)> = HashMap::new();
    let mut groups: HashMap<String, String> = HashMap::new();
    let mut big = false;
    let mut warnings = vec![];
    let mut i = 0;
    let n = t.len();
    while i + 1 < n {
        if t[i] == "BYTE_ORDER" {
            big = matches!(t[i + 1].as_str(), "MSB_FIRST" | "BIG_ENDIAN");
        }
        if t[i] != "/begin" {
            i += 1;
            continue;
        }
        let kind = t[i + 1].clone();
        match kind.as_str() {
            "RECORD_LAYOUT" => {
                let e = block_end(&t, i);
                let mut l = Layout::default();
                let mut k = i + 3;
                while k + 2 < e {
                    match t[k].as_str() {
                        "FNC_VALUES" | "AXIS_PTS_X" | "AXIS_PTS_Y" | "NO_AXIS_PTS_X" | "NO_AXIS_PTS_Y" => {
                            if let Some(pos) = num(&t[k + 1]) {
                                l.items.push((pos, t[k].clone(), t[k + 2].clone()));
                            }
                        }
                        _ => {}
                    }
                    k += 1;
                }
                l.items.sort_by_key(|x| x.0);
                layouts.insert(t[i + 2].clone(), l);
                i = e;
            }
            "COMPU_METHOD" => {
                let e = block_end(&t, i);
                let mut c = Compu { unit: t.get(i + 6).map(|u| text(u)).unwrap_or_default(), factor: 1.0, offset: 0.0 };
                let mut k = i;
                while k < e {
                    if t[k] == "COEFFS_LINEAR" && k + 2 < e {
                        c.factor = t[k + 1].parse().unwrap_or(1.0);
                        c.offset = t[k + 2].parse().unwrap_or(0.0);
                    }
                    if t[k] == "COEFFS" && k + 6 < e {
                        // RAT_FUNC with the common linear shape: x*f + o = (b*v + c... ) inverted.
                        let g = |o: usize| t[k + o].parse::<f64>().unwrap_or(0.0);
                        let (a, b, cc, d, e2, f) = (g(1), g(2), g(3), g(4), g(5), g(6));
                        if a == 0.0 && d == 0.0 && e2 == 0.0 && b != 0.0 && f != 0.0 {
                            c.factor = f / b;
                            c.offset = -cc / b;
                        }
                    }
                    k += 1;
                }
                compus.insert(t[i + 2].clone(), c);
                i = e;
            }
            "AXIS_PTS" => {
                let e = block_end(&t, i);
                if e > i + 11 {
                    let addr = num(&t[i + 4]).unwrap_or(0);
                    let lay = t[i + 6].clone();
                    let conv = t[i + 8].clone();
                    let cnt = num(&t[i + 9]).unwrap_or(0) as usize;
                    axis_pts.insert(t[i + 2].clone(), (addr, lay, conv, t.get(i + 3).map(|x| text(x)).unwrap_or_default(), cnt));
                }
                i = e;
            }
            "GROUP" => {
                let e = block_end(&t, i);
                let name = t[i + 2].clone();
                let mut k = i;
                while k < e {
                    if t[k] == "/begin" && t[k + 1] == "REF_CHARACTERISTIC" {
                        let e2 = block_end(&t, k);
                        for r in &t[k + 2..e2] {
                            groups.insert(r.clone(), name.clone());
                        }
                        k = e2;
                    }
                    k += 1;
                }
                i = e;
            }
            _ => i += 2,
        }
    }

    let mut out = vec![];
    i = 0;
    while i + 1 < n {
        if !(t[i] == "/begin" && t[i + 1] == "CHARACTERISTIC") {
            i += 1;
            continue;
        }
        let e = block_end(&t, i);
        if e < i + 10 {
            i = e;
            continue;
        }
        let name = t[i + 2].clone();
        let desc = text(&t[i + 3]);
        let ctype = t[i + 4].clone();
        let addr = num(&t[i + 5]).unwrap_or(0);
        let lay_name = t[i + 6].clone();
        let conv = t[i + 8].clone();
        let Some(lay) = layouts.get(&lay_name) else {
            warnings.push(format!("{name}: layout {lay_name} is missing"));
            i = e;
            continue;
        };
        let fnc = lay.items.iter().find(|x| x.1 == "FNC_VALUES");
        let org = data_org(fnc.map(|x| x.2.as_str()).unwrap_or("UBYTE"), big);
        let c = compus.get(&conv).cloned().unwrap_or(Compu { unit: "-".into(), factor: 1.0, offset: 0.0 });
        let mut m = MapDef {
            name: name.clone(),
            id_name: name.clone(),
            desc,
            unit: if c.unit.is_empty() { "-".into() } else { c.unit.clone() },
            factor: c.factor,
            offset: c.offset,
            org,
            folder: "Maps".into(),
            comment: String::new(),
            ..MapDef::default()
        };
        if m.factor.fract() != 0.0 {
            m.precision = 2;
        }
        // Axes
        let mut dims: Vec<(usize, Option<Axis>)> = vec![];
        let mut k = i;
        while k < e {
            if t[k] == "/begin" && t[k + 1] == "AXIS_DESCR" {
                let e2 = block_end(&t, k);
                let conv2 = t[k + 4].clone();
                let cnt = num(&t[k + 5]).unwrap_or(1).max(1) as usize;
                let cc = compus.get(&conv2).cloned().unwrap_or(Compu { unit: "-".into(), factor: 1.0, offset: 0.0 });
                let mut ax = None;
                let mut q = k;
                while q < e2 {
                    if t[q] == "AXIS_PTS_REF" && q + 1 < e2 {
                        if let Some((a, l, cv, d, _)) = axis_pts.get(&t[q + 1]) {
                            let lay2 = layouts.get(l);
                            let ty = lay2.and_then(|ly| ly.items.iter().find(|x| x.1.starts_with("AXIS_PTS"))).map(|x| x.2.clone()).unwrap_or_else(|| "UWORD".into());
                            // Bytes before the points inside the axis record, such as a count.
                            let pre: usize = lay2
                                .map(|ly| {
                                    ly.items
                                        .iter()
                                        .take_while(|x| !x.1.starts_with("AXIS_PTS"))
                                        .map(|x| data_org(&x.2, big).width())
                                        .sum()
                                })
                                .unwrap_or(0);
                            let ccv = compus.get(cv).cloned().unwrap_or(cc.clone());
                            ax = Some(Axis {
                                desc: if d.is_empty() { "-".into() } else { d.clone() },
                                unit: if ccv.unit.is_empty() { "-".into() } else { ccv.unit.clone() },
                                src: AxisSrc::Rom,
                                addr: (a - offset).max(0) as usize + pre,
                                org: data_org(&ty, big),
                                factor: ccv.factor,
                                offset: ccv.offset,
                                precision: if ccv.factor.fract() != 0.0 { 2 } else { 0 },
                                ..Axis::default()
                            });
                        }
                    }
                    q += 1;
                }
                dims.push((cnt, ax));
                k = e2;
            }
            k += 1;
        }
        // Axis points inside the map's own record come before the values.
        let mut pos = (addr - offset).max(0) as usize;
        let mut set_inline = |dims: &mut Vec<(usize, Option<Axis>)>, lay: &Layout, m: &MapDef| -> usize {
            let mut v = pos;
            for it in &lay.items {
                match it.1.as_str() {
                    "NO_AXIS_PTS_X" | "NO_AXIS_PTS_Y" => v += data_org(&it.2, big).width(),
                    "AXIS_PTS_X" | "AXIS_PTS_Y" => {
                        let d = if it.1 == "AXIS_PTS_X" { 0 } else { 1 };
                        let org2 = data_org(&it.2, big);
                        if let Some(slot) = dims.get_mut(d) {
                            slot.1 = Some(Axis { src: AxisSrc::Rom, addr: v, org: org2, ..Axis::default() });
                            v += slot.0 * org2.width();
                        }
                    }
                    "FNC_VALUES" => break,
                    _ => {}
                }
            }
            let _ = m;
            pos = v;
            v
        };
        let start = set_inline(&mut dims, lay, &m);
        m.start = start;
        let (cols, rows) = match ctype.as_str() {
            "VALUE" => (1, 1),
            "CURVE" | "VAL_BLK" => (dims.first().map(|d| d.0).unwrap_or(1), 1),
            _ => (dims.first().map(|d| d.0).unwrap_or(1), dims.get(1).map(|d| d.0).unwrap_or(1)),
        };
        m.cols = cols.max(1);
        m.rows = rows.max(1);
        m.kind = match (m.cols, m.rows) {
            (1, 1) => MapKind::Single,
            (_, 1) => MapKind::OneDim,
            _ => MapKind::TwoDim,
        };
        if let Some(Some(a)) = dims.first().map(|d| d.1.clone()) {
            m.x = a;
        }
        if let Some(Some(a)) = dims.get(1).map(|d| d.1.clone()) {
            m.y = a;
        }
        if m.end() > project_size {
            warnings.push(format!("{name}: the address {:X} is outside the project", m.start));
            i = e;
            continue;
        }
        m.folder = groups.get(&name).cloned().unwrap_or_else(|| "Maps".into());
        out.push(A2lMap { map: m });
        i = e;
    }
    if out.is_empty() && warnings.is_empty() {
        return Err("No maps were found in this file.".into());
    }
    Ok((out, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
/* a comment with /begin inside */
/begin PROJECT P "x"
 /begin MODULE M ""
  /begin MOD_COMMON "" BYTE_ORDER MSB_FIRST /end MOD_COMMON
  /begin RECORD_LAYOUT L_MAP
     NO_AXIS_PTS_X 1 UWORD
     NO_AXIS_PTS_Y 2 UWORD
     AXIS_PTS_X 3 UWORD INDEX_INCR DIRECT
     AXIS_PTS_Y 4 UWORD INDEX_INCR DIRECT
     FNC_VALUES 5 UWORD COLUMN_DIR DIRECT
  /end RECORD_LAYOUT
  /begin RECORD_LAYOUT L_VAL FNC_VALUES 1 UBYTE COLUMN_DIR DIRECT /end RECORD_LAYOUT
  /begin RECORD_LAYOUT L_AX NO_AXIS_PTS_X 1 UWORD AXIS_PTS_X 2 UWORD INDEX_INCR DIRECT /end RECORD_LAYOUT
  /begin COMPU_METHOD C1 "torque" LINEAR "%6.2" "Nm" COEFFS_LINEAR 0.5 -10 /end COMPU_METHOD
  /begin AXIS_PTS AX_RPM "engine speed" 0x80100 NO_INPUT L_AX 0 C1 8 0 8000 /end AXIS_PTS
  /begin CHARACTERISTIC TQ_MAX "max torque" MAP 0x80000 L_MAP 0 C1 0 500
     /begin AXIS_DESCR STD_AXIS IN1 C1 4 0 100 /end AXIS_DESCR
     /begin AXIS_DESCR STD_AXIS IN2 C1 3 0 100 /end AXIS_DESCR
  /end CHARACTERISTIC
  /begin CHARACTERISTIC K1 "single" VALUE 0x80200 L_VAL 0 NO_COMPU_METHOD 0 255 /end CHARACTERISTIC
  /begin CHARACTERISTIC FAR "outside" VALUE 0x900000 L_VAL 0 NO_COMPU_METHOD 0 255 /end CHARACTERISTIC
  /begin CHARACTERISTIC CV "with shared axis" CURVE 0x80300 L_VAL 0 NO_COMPU_METHOD 0 255
     /begin AXIS_DESCR COM_AXIS IN1 C1 8 0 100 AXIS_PTS_REF AX_RPM /end AXIS_DESCR
  /end CHARACTERISTIC
  /begin GROUP G1 "" /begin REF_CHARACTERISTIC TQ_MAX /end REF_CHARACTERISTIC /end GROUP
 /end MODULE
/end PROJECT
"#;

    #[test]
    fn reads_map_value_and_curve() {
        let (maps, warn) = parse(SAMPLE, 0x80000, 0x1000).unwrap();
        assert_eq!(maps.len(), 3, "{warn:?}");
        let m = &maps[0].map;
        assert_eq!(m.name, "TQ_MAX");
        assert_eq!((m.cols, m.rows), (4, 3));
        assert_eq!(m.org, DataOrg::HiLo);
        assert_eq!(m.x.addr, 4);
        assert_eq!(m.y.addr, 4 + 8);
        assert_eq!(m.start, 4 + 8 + 6);
        assert_eq!((m.factor, m.offset), (0.5, -10.0));
        assert_eq!(m.unit, "Nm");
        assert_eq!(maps[0].map.folder, "G1");
        let k = &maps[1].map;
        assert_eq!((k.cols, k.rows, k.start), (1, 1, 0x200));
        assert_eq!(k.kind, MapKind::Single);
        let c = &maps[2].map;
        assert_eq!(c.x.src, AxisSrc::Rom);
        assert_eq!(c.x.addr, 0x102, "two count bytes come before the points");
        assert_eq!(c.cols, 8);
        assert!(warn.iter().any(|w| w.starts_with("FAR")));
    }

    #[test]
    fn empty_file_says_so() {
        assert!(parse("nothing here", 0, 16).is_err());
    }
}
