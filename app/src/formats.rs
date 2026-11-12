//! Reading and writing files: binary, two-file 16 bit, Intel hex, Motorola S-records, zip,
//! map lists, map packs and selection files.
use crate::model::*;
use std::io::{Read, Write};
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub addr: u64,
    pub data: Vec<u8>,
}

/// What a dropped or chosen file turned out to be.
pub enum Loaded {
    Raw(Vec<u8>),
    Blocks(Vec<Block>),
    Project(Box<Project>),
    MapPack(Vec<MapDef>),
    A2l(String),
    Selection(SelectionFile),
}

fn ext(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

pub fn load_any(path: &Path, id: u64) -> Result<Loaded, String> {
    let e = ext(path);
    if e == "alsproj" {
        return Project::load(path, id).map(|p| Loaded::Project(Box::new(p)));
    }
    let bytes = std::fs::read(path).map_err(|err| format!("{}: {err}", path.display()))?;
    classify(&e, bytes)
}

fn classify(e: &str, bytes: Vec<u8>) -> Result<Loaded, String> {
    match e {
        "zip" => {
            let (name, inner) = unzip_first(&bytes)?;
            let ie = name.rsplit('.').next().unwrap_or("").to_lowercase();
            classify(&ie, inner)
        }
        "rar" => Err("Rar archives can't be read yet. Unpack the file first, or use a zip file.".into()),
        "hex" | "ihex" | "paf" | "daf" => parse_ihex(&String::from_utf8_lossy(&bytes)).map(Loaded::Blocks),
        "s19" | "s28" | "s37" | "srec" | "mot" => parse_srec(&String::from_utf8_lossy(&bytes)).map(Loaded::Blocks),
        "alsmap" => serde_json::from_slice(&bytes).map(Loaded::MapPack).map_err(|err| err.to_string()),
        "xdf" => crate::xdf::read_xdf(&bytes).map(Loaded::MapPack),
        "alssel" => read_selection(&bytes).map(Loaded::Selection),
        "a2l" => Ok(Loaded::A2l(String::from_utf8_lossy(&bytes).to_string())),
        _ => {
            // Text files in hex format often carry an unusual name.
            if bytes.first() == Some(&b':') && looks_text(&bytes) {
                return parse_ihex(&String::from_utf8_lossy(&bytes)).map(Loaded::Blocks);
            }
            if bytes.first() == Some(&b'S') && bytes.get(1).is_some_and(|c| c.is_ascii_digit()) && looks_text(&bytes) {
                return parse_srec(&String::from_utf8_lossy(&bytes)).map(Loaded::Blocks);
            }
            Ok(Loaded::Raw(bytes))
        }
    }
}

fn looks_text(b: &[u8]) -> bool {
    b.iter().take(4096).all(|&c| c == b'\r' || c == b'\n' || (0x20..0x7F).contains(&c))
}

fn unzip_first(bytes: &[u8]) -> Result<(String, Vec<u8>), String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(|e| e.to_string())?;
        if f.is_file() {
            let name = f.name().map_err(|e| e.to_string())?.to_string();
            let mut out = vec![];
            f.read_to_end(&mut out).map_err(|e| e.to_string())?;
            return Ok((name, out));
        }
    }
    Err("The archive is empty.".into())
}

pub fn write_zip(path: &Path, inner_name: &str, data: &[u8]) -> Result<(), String> {
    let f = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut z = zip::ZipWriter::new(f);
    let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    z.start_file(inner_name, o).map_err(|e| e.to_string())?;
    z.write_all(data).map_err(|e| e.to_string())?;
    z.finish().map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- two files ----------

/// One byte from each file in turn. The original guesses which is the high file; here the file
/// whose bytes jump least is taken as the low one's neighbour, see `guess_high_first`.
pub fn merge_two(a: &[u8], b: &[u8]) -> Vec<u8> {
    let n = a.len().max(b.len());
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        out.push(a.get(i).copied().unwrap_or(0xFF));
        out.push(b.get(i).copied().unwrap_or(0xFF));
    }
    out
}

/// High bytes of table data change slowly, so the calmer file is the high one.
pub fn guess_high_first(a: &[u8], b: &[u8]) -> bool {
    let jumps = |d: &[u8]| d.windows(2).map(|w| (w[0] as i64 - w[1] as i64).abs()).sum::<i64>();
    jumps(a) <= jumps(b)
}

pub fn split_two(d: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut a = Vec::with_capacity(d.len() / 2 + 1);
    let mut b = Vec::with_capacity(d.len() / 2 + 1);
    for (i, &x) in d.iter().enumerate() {
        if i % 2 == 0 {
            a.push(x);
        } else {
            b.push(x);
        }
    }
    (a, b)
}

// ---------- line swapping ----------

/// Moves address and data lines as an adapter board would. `addr[i]` is the file line wired to
/// line i; same for `data`. Applied in reverse when exporting.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LineSwap {
    pub addr: Vec<u8>,
    pub data: Vec<u8>,
}

impl LineSwap {
    pub fn identity(addr_lines: usize) -> LineSwap {
        LineSwap { addr: (0..addr_lines as u8).collect(), data: (0..8).collect() }
    }
    pub fn is_identity(&self) -> bool {
        self.addr.iter().enumerate().all(|(i, &v)| i == v as usize)
            && self.data.iter().enumerate().all(|(i, &v)| i == v as usize)
    }
    pub fn valid(&self) -> bool {
        let unique = |v: &[u8]| {
            let mut s = v.to_vec();
            s.sort_unstable();
            s.iter().enumerate().all(|(i, &x)| i == x as usize)
        };
        unique(&self.addr) && unique(&self.data) && self.data.len() == 8
    }
    fn map_byte(&self, b: u8) -> u8 {
        let mut o = 0;
        for (i, &from) in self.data.iter().enumerate() {
            if b >> from & 1 == 1 {
                o |= 1 << i;
            }
        }
        o
    }
    fn map_addr(&self, a: usize) -> usize {
        let mut o = 0;
        for (i, &from) in self.addr.iter().enumerate() {
            if a >> from & 1 == 1 {
                o |= 1 << i;
            }
        }
        o
    }
    fn inverse(&self) -> LineSwap {
        let inv = |v: &[u8]| {
            let mut o = vec![0u8; v.len()];
            for (i, &x) in v.iter().enumerate() {
                o[x as usize] = i as u8;
            }
            o
        };
        LineSwap { addr: inv(&self.addr), data: inv(&self.data) }
    }
    pub fn apply(&self, d: &[u8]) -> Vec<u8> {
        let mask = (1usize << self.addr.len()) - 1;
        let mut o = d.to_vec();
        for (i, &b) in d.iter().enumerate() {
            let t = (i & !mask) | self.map_addr(i & mask);
            if let Some(c) = o.get_mut(t) {
                *c = self.map_byte(b);
            }
        }
        o
    }
    pub fn unapply(&self, d: &[u8]) -> Vec<u8> {
        self.inverse().apply(d)
    }
}

// ---------- Intel hex ----------

pub fn parse_ihex(s: &str) -> Result<Vec<Block>, String> {
    let mut blocks: Vec<Block> = vec![];
    let mut base = 0u64;
    for (n, line) in s.lines().enumerate() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        let Some(hex) = l.strip_prefix(':') else {
            return Err(format!("Error in line {}: this isn't an Intel hex record.", n + 1));
        };
        let b = unhex(hex).ok_or(format!("Error in line {}: bad hex digits.", n + 1))?;
        if b.len() < 5 || b.len() != b[0] as usize + 5 {
            return Err(format!("Error in line {}: bad length.", n + 1));
        }
        if b.iter().fold(0u8, |a, &x| a.wrapping_add(x)) != 0 {
            return Err(format!("Error in line {}: Bad checksum. The file import was aborted.", n + 1));
        }
        let addr = u16::from_be_bytes([b[1], b[2]]) as u64;
        let payload = &b[4..b.len() - 1];
        match b[3] {
            0 => push_block(&mut blocks, base + addr, payload),
            1 => break,
            2 => base = (u16::from_be_bytes([payload[0], payload[1]]) as u64) << 4,
            4 => base = (u16::from_be_bytes([payload[0], payload[1]]) as u64) << 16,
            _ => {}
        }
    }
    if blocks.is_empty() {
        return Err("The file contains no data.".into());
    }
    Ok(blocks)
}

pub fn write_ihex(d: &[u8], start_addr: u64, per_line: usize) -> String {
    let mut out = String::new();
    let per = per_line.clamp(1, 255);
    let mut upper = u64::MAX;
    let rec = |out: &mut String, b: &[u8]| {
        let c = b.iter().fold(0u8, |a, &x| a.wrapping_add(x)).wrapping_neg();
        out.push(':');
        for x in b {
            out.push_str(&format!("{x:02X}"));
        }
        out.push_str(&format!("{c:02X}\r\n"));
    };
    let mut off = 0;
    while off < d.len() {
        let a = start_addr + off as u64;
        if a >> 16 != upper {
            upper = a >> 16;
            rec(&mut out, &[2, 0, 0, 4, (upper >> 8) as u8, upper as u8]);
        }
        let room = (0x10000 - (a & 0xFFFF)) as usize;
        let n = per.min(d.len() - off).min(room);
        let mut b = vec![n as u8, (a >> 8) as u8, a as u8, 0];
        b.extend_from_slice(&d[off..off + n]);
        rec(&mut out, &b);
        off += n;
    }
    rec(&mut out, &[0, 0, 0, 1]);
    out
}

// ---------- Motorola ----------

pub fn parse_srec(s: &str) -> Result<Vec<Block>, String> {
    let mut blocks = vec![];
    for (n, line) in s.lines().enumerate() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        if !l.starts_with('S') || l.len() < 4 {
            return Err(format!("Error in line {}: this isn't a Motorola record.", n + 1));
        }
        let t = l.as_bytes()[1];
        let b = unhex(&l[2..]).ok_or(format!("Error in line {}: bad hex digits.", n + 1))?;
        if b.len() != b[0] as usize + 1 {
            return Err(format!("Error in line {}: bad length.", n + 1));
        }
        if b.iter().fold(0u8, |a, &x| a.wrapping_add(x)) != 0xFF {
            return Err(format!("Error in line {}: Bad checksum. The file import was aborted.", n + 1));
        }
        let alen = match t {
            b'1' => 2,
            b'2' => 3,
            b'3' => 4,
            _ => continue,
        };
        let addr = b[1..1 + alen].iter().fold(0u64, |a, &x| a << 8 | x as u64);
        push_block(&mut blocks, addr, &b[1 + alen..b.len() - 1]);
    }
    if blocks.is_empty() {
        return Err("The file contains no data.".into());
    }
    Ok(blocks)
}

pub fn write_srec(d: &[u8], start_addr: u64, per_line: usize) -> String {
    let mut out = String::new();
    let per = per_line.clamp(1, 250);
    let end = start_addr + d.len() as u64;
    let (t, alen, term) = if end <= 0x1_0000 {
        ('1', 2usize, '9')
    } else if end <= 0x100_0000 {
        ('2', 3, '8')
    } else {
        ('3', 4, '7')
    };
    let rec = |out: &mut String, t: char, b: &[u8]| {
        let mut sum = 0u8;
        out.push('S');
        out.push(t);
        for x in b {
            out.push_str(&format!("{x:02X}"));
            sum = sum.wrapping_add(*x);
        }
        out.push_str(&format!("{:02X}\r\n", !sum));
    };
    for (i, c) in d.chunks(per).enumerate() {
        let a = start_addr + (i * per) as u64;
        let mut b = vec![(alen + c.len() + 1) as u8];
        for k in (0..alen).rev() {
            b.push((a >> (k * 8)) as u8);
        }
        b.extend_from_slice(c);
        rec(&mut out, t, &b);
    }
    let mut b = vec![(alen + 1) as u8];
    b.extend(std::iter::repeat_n(0u8, alen));
    rec(&mut out, term, &b);
    out
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

fn push_block(blocks: &mut Vec<Block>, addr: u64, data: &[u8]) {
    if let Some(l) = blocks.last_mut() {
        if l.addr + l.data.len() as u64 == addr {
            l.data.extend_from_slice(data);
            return;
        }
    }
    blocks.push(Block { addr, data: data.to_vec() });
}

/// The default project size for a set of blocks: the next round size holding them all.
pub fn suggest_size(blocks: &[Block]) -> (usize, u64) {
    let lo = blocks.iter().map(|b| b.addr).min().unwrap_or(0);
    let hi = blocks.iter().map(|b| b.addr + b.data.len() as u64).max().unwrap_or(0);
    let span = (hi - lo).max(1) as usize;
    // An address base that is a multiple of the size drops out when mapping.
    let size = span.next_power_of_two();
    (size, lo - lo % size as u64)
}

/// Lays hex blocks into one data block. Returns the data and whether blocks overlap.
pub fn map_blocks(blocks: &[Block], size: usize, offset: u64) -> (Vec<u8>, bool) {
    let mut d = vec![0xFFu8; size];
    let mut used = vec![false; size];
    let mut overlap = false;
    for b in blocks {
        for (i, &x) in b.data.iter().enumerate() {
            let a = (b.addr + i as u64).wrapping_sub(offset) as usize % size.max(1);
            if used[a] {
                overlap = true;
            }
            used[a] = true;
            d[a] = x;
        }
    }
    (d, overlap)
}

// ---------- map lists ----------

pub fn maps_csv(p: &Project) -> String {
    let mut s = String::from("Address;Name;Id;Folder;Columns;Rows;Bits;Organisation;Factor;Offset;Unit;Description\r\n");
    for m in &p.maps {
        s.push_str(&format!(
            "{:X};{};{};{};{};{};{};{};{};{};{};{}\r\n",
            m.start,
            csv(&m.name),
            csv(&m.id_name),
            csv(&m.folder),
            m.cols,
            m.rows,
            m.org.width() * 8,
            m.org.label(),
            m.factor,
            m.offset,
            csv(&m.unit),
            csv(&m.desc)
        ));
    }
    s
}

fn csv(s: &str) -> String {
    if s.contains(';') || s.contains('"') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Renames maps from a list: an edited CSV file comes back with new names, units and folders.
/// Returns how many were updated.
pub fn update_maps_from_csv(p: &mut Project, text: &str) -> usize {
    let mut n = 0;
    for line in text.lines().skip(1) {
        let f: Vec<String> = split_csv(line);
        let Some(a) = f.first().and_then(|a| usize::from_str_radix(a.trim(), 16).ok()) else { continue };
        if let Some(m) = p.maps.iter_mut().find(|m| m.start == a) {
            if let Some(v) = f.get(1) {
                m.name = v.clone();
            }
            if let Some(v) = f.get(2) {
                m.id_name = v.clone();
            }
            if let Some(v) = f.get(3) {
                m.folder = v.clone();
            }
            if let Some(v) = f.get(10) {
                m.unit = v.clone();
            }
            if let Some(v) = f.get(11) {
                m.desc = v.clone();
            }
            n += 1;
        }
    }
    if n > 0 {
        p.dirty_struct = true;
    }
    n
}

fn split_csv(line: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut quoted = false;
    let mut it = line.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '"' if quoted && it.peek() == Some(&'"') => {
                cur.push('"');
                it.next();
            }
            '"' => quoted = !quoted,
            ';' | ',' if !quoted => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

// ---------- selection files ----------

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectionFile {
    pub addr: usize,
    pub data: Vec<u8>,
}

const BLK_MAGIC: &[u8; 6] = b"ALSSEL";

pub fn write_selection(s: &SelectionFile) -> Vec<u8> {
    let mut o = BLK_MAGIC.to_vec();
    o.extend_from_slice(&(s.addr as u64).to_le_bytes());
    o.extend_from_slice(&s.data);
    o
}

pub fn read_selection(b: &[u8]) -> Result<SelectionFile, String> {
    if b.get(..6) != Some(BLK_MAGIC.as_slice()) || b.len() < 14 {
        return Err("This selection file wasn't made by this program.".into());
    }
    let addr = u64::from_le_bytes(b[6..14].try_into().map_err(|_| "Short file")?) as usize;
    Ok(SelectionFile { addr, data: b[14..].to_vec() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ihex_round_trip_across_64k() {
        let d: Vec<u8> = (0..70000u32).map(|i| (i % 251) as u8).collect();
        let s = write_ihex(&d, 0x8000, 32);
        let b = parse_ihex(&s).unwrap();
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].addr, 0x8000);
        assert_eq!(b[0].data, d);
    }

    #[test]
    fn ihex_rejects_bad_checksum() {
        assert!(parse_ihex(":0100000001FF\n").unwrap_err().contains("Bad checksum"));
        assert!(parse_ihex(":0100000001FE\n:00000001FF\n").is_ok());
    }

    #[test]
    fn srec_round_trip_each_address_size() {
        for start in [0u64, 0x20000, 0x2000000] {
            let d: Vec<u8> = (0..1000u32).map(|i| (i % 199) as u8).collect();
            let s = write_srec(&d, start, 16);
            let b = parse_srec(&s).unwrap();
            assert_eq!(b[0].addr, start);
            assert_eq!(b[0].data, d);
        }
    }

    #[test]
    fn known_srec_line() {
        // The textbook line from the format's description.
        let b = parse_srec("S1137AF00A0A0D0000000000000000000000000061\n").unwrap();
        assert_eq!(b[0].addr, 0x7AF0);
        assert_eq!(&b[0].data[..3], &[0x0A, 0x0A, 0x0D]);
    }

    #[test]
    fn mapping_blocks_and_overlap() {
        let blocks = vec![
            Block { addr: 0x8_0000, data: vec![1, 2] },
            Block { addr: 0x8_0100, data: vec![3] },
        ];
        let (size, base) = suggest_size(&blocks);
        assert_eq!(size, 0x200);
        assert_eq!(base, 0x8_0000);
        let (d, over) = map_blocks(&blocks, size, base);
        assert_eq!((d[0], d[1], d[0x100]), (1, 2, 3));
        assert!(!over);
        let (_, over) = map_blocks(&blocks, 0x100, base);
        assert!(over, "a too small size folds the blocks onto each other");
    }

    #[test]
    fn two_files_merge_and_split() {
        let m = merge_two(&[1, 3], &[2, 4]);
        assert_eq!(m, vec![1, 2, 3, 4]);
        assert_eq!(split_two(&m), (vec![1, 3], vec![2, 4]));
        assert!(guess_high_first(&[1, 1, 1, 2], &[9, 200, 3, 180]));
    }

    #[test]
    fn line_swap_inverts() {
        let mut s = LineSwap::identity(4);
        assert!(s.is_identity() && s.valid());
        s.addr.swap(0, 1);
        s.data.swap(3, 7);
        assert!(s.valid() && !s.is_identity());
        let d: Vec<u8> = (0..32).map(|i| (i * 7) as u8).collect();
        let x = s.apply(&d);
        assert_ne!(x, d);
        assert_eq!(s.unapply(&x), d);
        let bad = LineSwap { addr: vec![0, 0], data: (0..8).collect() };
        assert!(!bad.valid());
    }

    #[test]
    fn csv_quotes_and_updates() {
        let mut p = Project::new(1);
        p.add_version("", vec![0; 64]);
        let uid = p.uid();
        p.maps.push(MapDef { uid, name: "a;b".into(), start: 0x10, ..MapDef::default() });
        let text = maps_csv(&p);
        assert!(text.contains("10;\"a;b\";"));
        let n = update_maps_from_csv(&mut p, &text.replace("\"a;b\"", "\"Boost; limit\""));
        assert_eq!(n, 1);
        assert_eq!(p.maps[0].name, "Boost; limit");
    }

    #[test]
    fn selection_file_round_trip() {
        let s = SelectionFile { addr: 0x1234, data: vec![1, 2, 3] };
        assert_eq!(read_selection(&write_selection(&s)).unwrap(), s);
        assert!(read_selection(b"xx").is_err());
    }
}
