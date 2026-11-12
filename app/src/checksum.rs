//! Manual checksums and copy blocks. Car-specific checksum add-ons are not part of this app.
use crate::model::*;

fn register_mask(c: &Checksum) -> u64 {
    let bits = c.org.width() * 8;
    let bits = if c.fullbyte { bits * 2 } else { bits };
    if bits >= 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}

/// Sum over the checked area, plus the correction cell or range.
pub fn current_sum(d: &[u8], c: &Checksum) -> u64 {
    let w = c.org.width();
    let mut s = 0u64;
    let mut a = c.from;
    while a + w <= c.to + 1 && a + w <= d.len() {
        s = s.wrapping_add(c.org.read(d, a, false).unwrap_or(0.0) as u64);
        a += w;
    }
    if !c.display_only && c.algo == CsAlgo::Additive {
        for t in target_cells(c) {
            s = s.wrapping_add(c.org.read(d, t, false).unwrap_or(0.0) as u64);
        }
    }
    s & register_mask(c)
}

fn target_cells(c: &Checksum) -> Vec<usize> {
    let w = c.org.width();
    let end = c.target_end.max(c.target);
    (c.target..=end).step_by(w).collect()
}

/// Remembers the correct sum, taken from the given data (normally the original).
pub fn init_expected(c: &mut Checksum, reference: &[u8]) {
    c.expected = current_sum(reference, c);
}

pub fn overlaps_target(c: &Checksum) -> bool {
    c.algo == CsAlgo::Additive
        && !c.display_only
        && c.target <= c.to
        && c.target_end.max(c.target) >= c.from
}

pub fn is_ok(d: &[u8], c: &Checksum) -> bool {
    match c.algo {
        CsAlgo::Additive => current_sum(d, c) == c.expected,
        CsAlgo::Copy => {
            let n = c.to + 1 - c.from;
            d.get(c.from..c.from + n) == d.get(c.target..c.target + n)
        }
    }
}

/// Corrects one checksum in the project's current version.
pub fn correct(p: &mut Project, i: usize) -> Result<(), String> {
    let c = p.checksums.get(i).ok_or("No such checksum")?.clone();
    if c.display_only {
        return Ok(());
    }
    match c.algo {
        CsAlgo::Copy => {
            let n = c.to + 1 - c.from;
            let chunk = p.data().get(c.from..c.from + n).ok_or("Area outside the project")?.to_vec();
            p.write(c.target, &chunk);
            Ok(())
        }
        CsAlgo::Additive => {
            if c.target == 0 && c.target_end == 0 {
                return Err("You're trying to correct a manual checksum even though the target address is 0.".into());
            }
            let mask = register_mask(&c);
            let cur = current_sum(p.data(), &c);
            if cur == c.expected {
                return Ok(());
            }
            let cells = target_cells(&c);
            let (lo, hi) = c.org.limits(false);
            if !c.fullbyte {
                let t = c.target;
                let v = c.org.read(p.data(), t, false).unwrap_or(0.0) as u64;
                let delta = c.expected.wrapping_sub(cur) & mask;
                let nv = v.wrapping_add(delta) & mask;
                let bytes = c.org.encode(nv as f64, false);
                p.write(t, &bytes);
                return Ok(());
            }
            // Fullbyte: the whole difference has to be spent across the target range.
            let half = (mask >> 1) as i64;
            let mut delta = (c.expected.wrapping_sub(cur) & mask) as i64;
            if delta > half {
                delta -= mask as i64 + 1;
            }
            let mut writes = vec![];
            for t in cells {
                if delta == 0 {
                    break;
                }
                let v = c.org.read(p.data(), t, false).unwrap_or(0.0) as i64;
                let nv = (v + delta).clamp(lo as i64, hi as i64);
                delta -= nv - v;
                writes.push((t, nv));
            }
            if delta != 0 {
                return Err("The target range that you entered is not big enough to correct a fullbyte checksum. Extend the range or move it to a place with different original values.".into());
            }
            for (t, nv) in writes {
                let bytes = c.org.encode(nv as f64, false);
                p.write(t, &bytes);
            }
            Ok(())
        }
    }
}

/// Runs every automatic checksum whose area changed. A failure leaves the checksum marked wrong.
pub fn auto_correct(p: &mut Project) {
    let Some(g) = &p.open_group else { return };
    let touched: Vec<(usize, usize)> = g.edits.iter().map(|e| (e.addr, e.addr + e.new.len())).collect();
    if touched.is_empty() {
        return;
    }
    for i in 0..p.checksums.len() {
        let c = &p.checksums[i];
        if !c.auto {
            continue;
        }
        let hit = touched.iter().any(|&(a, b)| a <= c.to && b > c.from);
        if hit {
            let _ = correct(p, i);
        }
    }
}

/// Whether an address is covered by any checksum area.
pub fn covered(p: &Project, addr: usize) -> bool {
    p.checksums.iter().any(|c| addr >= c.from && addr <= c.to)
}

pub fn status_text(p: &Project) -> String {
    let n = p.checksums.iter().filter(|c| !c.display_only).count();
    if n == 0 && p.syncs.is_empty() {
        return "No checksum".into();
    }
    let bad = p
        .checksums
        .iter()
        .filter(|c| !c.display_only && !is_ok(p.data(), c))
        .count();
    if bad > 0 {
        format!("Sums: {bad} of {n} wrong")
    } else if n > 0 {
        format!("Sums: {n} ok")
    } else {
        format!("Mirrors: {}", p.syncs.len())
    }
}

/// Fills the area so that a checksum over the found region can be offered.
/// Finds repeated blocks in the file, which often are mirrored data the ECU compares.
pub fn search_blocks(d: &[u8]) -> Vec<(usize, usize, usize)> {
    const BLOCK: usize = 1024;
    let mut out = vec![];
    if d.len() < BLOCK * 4 {
        return out;
    }
    let mut seen = std::collections::HashMap::new();
    let mut a = 0;
    while a + BLOCK <= d.len() {
        let b = &d[a..a + BLOCK];
        let first = b[0];
        if b.iter().all(|&x| x == first) {
            a += BLOCK;
            continue;
        }
        let h = fnv(b);
        if let Some(&prev) = seen.get(&h) {
            if d[prev..prev + BLOCK] == *b {
                let mut len = BLOCK;
                while a + len < d.len() && prev + len < a && d[prev + len] == d[a + len] {
                    len += 1;
                }
                out.push((prev, a, len));
                a += len;
                continue;
            }
        }
        seen.insert(h, a);
        a += BLOCK;
    }
    out
}

fn fnv(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf29ce484222325u64, |h, &x| (h ^ x as u64).wrapping_mul(0x100000001b3))
}

/// The six answers to a change that falls outside every checksum area.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProtectAnswer {
    Always,
    AllOfOperation,
    ThisOne,
    IgnoreThis,
    IgnoreAllProtected,
    CancelOperation,
}

/// Walks one operation's writes, stopping at each one that no checksum area covers.
#[derive(Clone, Debug, Default)]
pub struct ProtectRun {
    pub label: String,
    /// Address, bytes, and whether a checksum area covers the address.
    pub writes: Vec<(usize, Vec<u8>, bool)>,
    pub at: usize,
    pub accepted: Vec<(usize, Vec<u8>)>,
    pub turn_off: bool,
}

impl ProtectRun {
    /// Moves on to the next write that needs asking. `None` means the run is finished.
    pub fn next_question(&mut self) -> Option<usize> {
        while let Some((a, b, covered)) = self.writes.get(self.at) {
            if !*covered {
                return Some(*a);
            }
            self.accepted.push((*a, b.clone()));
            self.at += 1;
        }
        None
    }

    pub fn answer(&mut self, ans: ProtectAnswer) {
        let Some((a, b, _)) = self.writes.get(self.at).cloned() else { return };
        match ans {
            ProtectAnswer::ThisOne => {
                self.accepted.push((a, b));
                self.at += 1;
            }
            ProtectAnswer::IgnoreThis => self.at += 1,
            ProtectAnswer::AllOfOperation | ProtectAnswer::Always => {
                self.turn_off = ans == ProtectAnswer::Always;
                self.accepted.extend(self.writes[self.at..].iter().map(|w| (w.0, w.1.clone())));
                self.at = self.writes.len();
            }
            ProtectAnswer::IgnoreAllProtected => {
                self.accepted.extend(self.writes[self.at..].iter().filter(|w| w.2).map(|w| (w.0, w.1.clone())));
                self.at = self.writes.len();
            }
            ProtectAnswer::CancelOperation => {
                self.accepted.clear();
                self.at = self.writes.len();
            }
        }
    }
}

/// Corrects every checksum. False when one couldn't be corrected.
pub fn correct_all(p: &mut Project) -> bool {
    let mut ok = true;
    p.begin_edit("Apply all checksums");
    for i in 0..p.checksums.len() {
        ok &= correct(p, i).is_ok();
    }
    p.end_edit();
    ok
}

/// Removes all checksum and sync blocks, optionally putting the corrected cells back.
pub fn delete_all(p: &mut Project, restore: bool) {
    let targets: Vec<(usize, usize)> = p.checksums.iter().filter(|c| !c.display_only).map(|c| (c.target, c.target_end.max(c.target) + c.org.width())).collect();
    // First the blocks go, so that nothing corrects the restored values again.
    p.checksums.clear();
    p.syncs.clear();
    p.dirty_struct = true;
    if restore {
        p.begin_edit("Delete checksums");
        for (a, b) in targets {
            if let Some(chunk) = p.ref_data().get(a..b).map(|x| x.to_vec()) {
                p.write(a, &chunk);
            }
        }
        p.end_edit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run() -> ProtectRun {
        ProtectRun {
            writes: vec![(1, vec![1], true), (2, vec![2], false), (3, vec![3], true), (4, vec![4], false), (5, vec![5], false)],
            ..ProtectRun::default()
        }
    }

    fn addrs(r: &ProtectRun) -> Vec<usize> {
        r.accepted.iter().map(|w| w.0).collect()
    }

    #[test]
    fn protect_asks_only_about_uncovered_addresses() {
        let mut r = run();
        assert_eq!(r.next_question(), Some(2));
        r.answer(ProtectAnswer::ThisOne);
        assert_eq!(r.next_question(), Some(4));
        r.answer(ProtectAnswer::IgnoreThis);
        assert_eq!(r.next_question(), Some(5));
        r.answer(ProtectAnswer::ThisOne);
        assert_eq!(r.next_question(), None);
        assert_eq!(addrs(&r), vec![1, 2, 3, 5]);
    }

    #[test]
    fn protect_bulk_answers() {
        let mut r = run();
        r.next_question();
        r.answer(ProtectAnswer::AllOfOperation);
        assert_eq!(r.next_question(), None);
        assert_eq!(addrs(&r), vec![1, 2, 3, 4, 5]);
        assert!(!r.turn_off);
        let mut r = run();
        r.next_question();
        r.answer(ProtectAnswer::Always);
        assert!(r.turn_off);
        let mut r = run();
        r.next_question();
        r.answer(ProtectAnswer::IgnoreAllProtected);
        assert_eq!(r.next_question(), None);
        assert_eq!(addrs(&r), vec![1, 3], "covered ones still go through");
        let mut r = run();
        r.next_question();
        r.answer(ProtectAnswer::CancelOperation);
        assert_eq!(r.next_question(), None);
        assert!(r.accepted.is_empty());
    }

    #[test]
    fn delete_all_keeps_or_restores() {
        let mut p = project();
        let d = p.data().to_vec();
        p.current = p.add_version("", d);
        let mut c = Checksum { from: 0, to: 31, org: DataOrg::Byte, target: 32, target_end: 32, auto: true, ..Default::default() };
        init_expected(&mut c, p.data());
        p.checksums.push(c.clone());
        p.begin_edit("t");
        p.write(5, &[110]);
        p.end_edit();
        assert_ne!(p.data()[32], 10);
        delete_all(&mut p, false);
        assert!(p.checksums.is_empty());
        assert_ne!(p.data()[32], 10, "keeps the current value");
        p.checksums.push(c);
        delete_all(&mut p, true);
        assert_eq!(p.data()[32], 10, "restores the original value");
    }

    fn project() -> Project {
        let mut p = Project::new(1);
        p.add_version("", vec![10; 64]);
        p
    }

    #[test]
    fn additive_keeps_sum_constant() {
        let mut p = project();
        let mut c = Checksum { from: 0, to: 31, org: DataOrg::Byte, target: 32, target_end: 32, auto: true, ..Default::default() };
        init_expected(&mut c, p.data());
        p.checksums.push(c);
        p.begin_edit("t");
        p.write(5, &[110]); // +100
        p.end_edit();
        assert_eq!(p.data()[32], 166, "10 - 100 wraps inside the 8-bit register");
        assert!(is_ok(p.data(), &p.checksums[0]));
    }

    #[test]
    fn fullbyte_spends_the_whole_difference_in_the_range() {
        let mut p = project();
        let mut c = Checksum { from: 0, to: 31, org: DataOrg::Byte, target: 32, target_end: 63, fullbyte: true, auto: false, ..Default::default() };
        init_expected(&mut c, p.data());
        p.checksums.push(c);
        p.begin_edit("t");
        p.write(0, &[255]); // +245: more than one target cell can give
        p.end_edit();
        assert!(!is_ok(p.data(), &p.checksums[0]));
        correct(&mut p, 0).unwrap();
        assert!(is_ok(p.data(), &p.checksums[0]));
        let spent: i64 = p.data()[32..64].iter().map(|&b| 10 - b as i64).sum();
        assert_eq!(spent, 245);
    }

    #[test]
    fn fullbyte_reports_a_too_small_range() {
        let mut p = project();
        let mut c = Checksum { from: 0, to: 31, org: DataOrg::Byte, target: 32, target_end: 32, fullbyte: true, ..Default::default() };
        init_expected(&mut c, p.data());
        p.checksums.push(c);
        p.write(0, &[255]);
        assert!(correct(&mut p, 0).is_err());
    }

    #[test]
    fn zero_target_is_refused() {
        let mut p = project();
        let mut c = Checksum { from: 8, to: 31, org: DataOrg::Byte, ..Default::default() };
        init_expected(&mut c, p.data());
        p.checksums.push(c);
        p.write(9, &[1]);
        assert!(correct(&mut p, 0).is_err());
    }
}
