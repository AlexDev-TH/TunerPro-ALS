//! Byte, number and text search with wildcards and tolerances.
use crate::model::DataOrg;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Radix {
    Dec,
    #[default]
    Hex,
    Text,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Align {
    /// Follows the cell width of the view.
    #[default]
    Auto,
    All,
    Even,
    Four,
}

#[derive(Clone, Debug, Default)]
pub struct Query {
    pub cells: Vec<Option<f64>>,
    pub text: Option<Vec<Option<u8>>>,
    pub match_case: bool,
    pub org: DataOrg,
    pub from: usize,
    pub to: usize,
    pub up: f64,
    pub down: f64,
    pub align: Align,
    pub skip_code: bool,
}

/// Reads the search text the way the dialog takes it: `0x` and `0d` override the tab's number
/// system for one value, and `?` or `??` matches anything.
pub fn parse_pattern(s: &str, radix: Radix) -> Result<Vec<Option<f64>>, String> {
    let mut out = vec![];
    for t in s.split_whitespace() {
        if t.chars().all(|c| c == '?') {
            out.push(None);
            continue;
        }
        let low = t.to_ascii_lowercase();
        let v = if let Some(h) = low.strip_prefix("0x") {
            u64::from_str_radix(h, 16).map(|v| v as f64).map_err(|_| format!("Bad value: {t}"))?
        } else if let Some(d) = low.strip_prefix("0d") {
            d.parse::<f64>().map_err(|_| format!("Bad value: {t}"))?
        } else if radix == Radix::Hex {
            u64::from_str_radix(&low, 16).map(|v| v as f64).map_err(|_| format!("Bad value: {t}"))?
        } else {
            low.parse::<f64>().map_err(|_| format!("Bad value: {t}"))?
        };
        out.push(Some(v));
    }
    Ok(out)
}

pub fn parse_text(s: &str) -> Vec<Option<u8>> {
    s.bytes().map(|b| if b == b'?' { None } else { Some(b) }).collect()
}

fn step(q: &Query) -> usize {
    match q.align {
        Align::Auto => {
            if q.text.is_some() {
                1
            } else {
                q.org.width()
            }
        }
        Align::All => 1,
        Align::Even => 2,
        Align::Four => 4,
    }
}

/// How far the data at `a` is from the query, or `None` when it doesn't match at all.
pub fn match_at(d: &[u8], a: usize, q: &Query) -> Option<f64> {
    if let Some(t) = &q.text {
        let b = d.get(a..a + t.len())?;
        for (x, y) in b.iter().zip(t) {
            if let Some(y) = y {
                let ok = if q.match_case { x == y } else { x.eq_ignore_ascii_case(y) };
                if !ok {
                    return None;
                }
            }
        }
        return Some(0.0);
    }
    let w = q.org.width();
    let mut err = 0.0;
    for (i, c) in q.cells.iter().enumerate() {
        let v = q.org.read(d, a + i * w, false)?;
        if let Some(c) = c {
            let e = v - c;
            if e > q.up || -e > q.down {
                return None;
            }
            err += e.abs();
        }
    }
    Some(err)
}

fn query_len(q: &Query) -> usize {
    match &q.text {
        Some(t) => t.len(),
        None => q.cells.len() * q.org.width(),
    }
}

/// All hits up to `max`, in address order.
pub fn find_all(d: &[u8], q: &Query, max: usize, is_code: Option<&dyn Fn(usize) -> bool>) -> Vec<usize> {
    let mut out = vec![];
    let n = query_len(q);
    if n == 0 {
        return out;
    }
    let end = q.to.min(d.len().saturating_sub(1));
    let s = step(q);
    let mut a = q.from;
    if s > 1 {
        a = a.div_ceil(s) * s;
    }
    while a + n <= end + 1 {
        if match_at(d, a, q).is_some() {
            let skip = q.skip_code && is_code.is_some_and(|f| f(a));
            if !skip {
                out.push(a);
                if out.len() >= max {
                    break;
                }
            }
        }
        a += s;
    }
    out
}

pub fn find_next(d: &[u8], q: &Query, from: usize, forward: bool) -> Option<usize> {
    let n = query_len(q);
    if n == 0 || d.len() < n {
        return None;
    }
    let s = step(q);
    let last = q.to.min(d.len() - 1);
    if forward {
        let mut a = from.max(q.from);
        if s > 1 {
            a = a.div_ceil(s) * s;
        }
        while a + n <= last + 1 {
            if match_at(d, a, q).is_some() {
                return Some(a);
            }
            a += s;
        }
    } else {
        let mut a = from.min(last + 1).checked_sub(1)?;
        if s > 1 {
            a = a / s * s;
        }
        loop {
            if a < q.from {
                return None;
            }
            if a + n <= last + 1 && match_at(d, a, q).is_some() {
                return Some(a);
            }
            a = a.checked_sub(s)?;
        }
    }
    None
}

/// Search for places that look like a block of data, within a percent tolerance.
/// The result is ordered best first.
pub fn find_similar(d: &[u8], block: &[f64], org: DataOrg, tolerance_percent: f64, max: usize, skip: Option<(usize, usize)>) -> Vec<(usize, f64)> {
    let w = org.width();
    let mut out: Vec<(usize, f64)> = vec![];
    if block.is_empty() || d.len() < block.len() * w {
        return out;
    }
    let mut a = 0;
    let last = d.len() - block.len() * w;
    let limit: Vec<f64> = block.iter().map(|v| (v.abs() * tolerance_percent / 100.0).max(if tolerance_percent > 0.0 { 1.0 } else { 0.0 })).collect();
    while a <= last {
        if let Some((s, e)) = skip {
            if a + block.len() * w > s && a < e {
                a += w;
                continue;
            }
        }
        let mut err = 0.0;
        let mut ok = true;
        for (i, b) in block.iter().enumerate() {
            let v = org.read(d, a + i * w, false).unwrap_or(f64::MAX);
            let e = (v - b).abs();
            if e > limit[i] {
                ok = false;
                break;
            }
            err += e;
        }
        if ok {
            out.push((a, err));
        }
        a += w;
    }
    out.sort_by(|x, y| x.1.partial_cmp(&y.1).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(max);
    out
}

/// Printable text runs of at least `min` characters, for the "As text" list and vehicle search.
pub fn text_runs(d: &[u8], min: usize) -> Vec<(usize, String)> {
    let mut out = vec![];
    let mut start = None;
    for (i, &b) in d.iter().enumerate() {
        let ok = (0x20..0x7F).contains(&b);
        match (ok, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                if i - s >= min {
                    out.push((s, String::from_utf8_lossy(&d[s..i]).to_string()));
                }
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        if d.len() - s >= min {
            out.push((s, String::from_utf8_lossy(&d[s..]).to_string()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(cells: Vec<Option<f64>>, d: &[u8]) -> Query {
        Query { cells, org: DataOrg::Byte, from: 0, to: d.len() - 1, ..Query::default() }
    }

    #[test]
    fn prefixes_override_the_tab() {
        let v = parse_pattern("ff 0d10 0x10 ??", Radix::Hex).unwrap();
        assert_eq!(v, vec![Some(255.0), Some(10.0), Some(16.0), None]);
        let v = parse_pattern("10 0xff ?", Radix::Dec).unwrap();
        assert_eq!(v, vec![Some(10.0), Some(255.0), None]);
        assert!(parse_pattern("zz", Radix::Hex).is_err());
    }

    #[test]
    fn wildcard_and_tolerance() {
        let d = [1u8, 2, 3, 9, 3, 100, 3, 9, 4];
        let hits = find_all(&d, &q(vec![Some(3.0), None, Some(3.0)], &d), 200, None);
        assert_eq!(hits, vec![2, 4]);
        let mut t = q(vec![Some(3.0), Some(9.0), Some(3.0)], &d);
        assert_eq!(find_all(&d, &t, 200, None), vec![2]);
        // 4 with one below allowed: matches the 3s (index 2, 4 and 6) followed by a 9.
        t.cells = vec![Some(4.0), Some(9.0)];
        t.down = 1.0;
        assert_eq!(find_all(&d, &t, 200, None), vec![2, 6]);
    }

    #[test]
    fn text_is_case_insensitive_unless_asked() {
        let d = b"xxLongWord yy";
        let mut t = Query { text: Some(parse_text("longw?rd")), from: 0, to: d.len() - 1, ..Query::default() };
        assert_eq!(find_all(d, &t, 10, None), vec![2]);
        t.match_case = true;
        assert!(find_all(d, &t, 10, None).is_empty());
    }

    #[test]
    fn next_and_back_move_past_the_cursor() {
        let d = [5u8, 0, 5, 0, 5];
        let t = q(vec![Some(5.0)], &d);
        assert_eq!(find_next(&d, &t, 1, true), Some(2));
        assert_eq!(find_next(&d, &t, 2, false), Some(0));
        assert_eq!(find_next(&d, &t, 0, false), None);
    }

    #[test]
    fn similar_finds_the_block_elsewhere_best_first() {
        let mut d = vec![0u8; 64];
        d[10..14].copy_from_slice(&[100, 110, 120, 130]);
        d[40..44].copy_from_slice(&[101, 111, 121, 131]);
        let r = find_similar(&d, &[100.0, 110.0, 120.0, 130.0], DataOrg::Byte, 5.0, 10, None);
        assert_eq!(r[0].0, 10);
        assert_eq!(r[1].0, 40);
        let r = find_similar(&d, &[100.0, 110.0, 120.0, 130.0], DataOrg::Byte, 5.0, 10, Some((10, 14)));
        assert_eq!(r[0].0, 40);
    }

    #[test]
    fn hex_16_bit_respects_org() {
        let d = [0x12, 0x34, 0x00, 0x34, 0x12];
        let mut t = q(vec![Some(0x1234 as f64)], &d);
        t.org = DataOrg::HiLo;
        t.align = Align::All;
        assert_eq!(find_all(&d, &t, 9, None), vec![0]);
        t.org = DataOrg::LoHi;
        assert_eq!(find_all(&d, &t, 9, None), vec![3]);
    }
}
