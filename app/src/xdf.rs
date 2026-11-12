//! TunerPro definition files (.xdf): maps written out as tables, and tables read back as maps.
//! The layout follows files that TunerPro 5 saved itself (format 1.80): plain ASCII, with Thai
//! text as `&#nnn;` of its code page 874 byte.
use crate::model::*;
use std::collections::{HashMap, HashSet};

const TYPE_SIGNED: u32 = 0x01;
const TYPE_LSB_FIRST: u32 = 0x02;
const TYPE_COL_MAJOR: u32 = 0x04;
const TYPE_FLOAT: u32 = 0x10000;

/// TunerPro refuses long descriptions; this many characters stays well inside what it takes.
const MAX_DESCRIPTION: usize = 300;

// ---------- text ----------

fn thai_byte(c: char) -> Option<u8> {
    let u = c as u32;
    ((0x0E01..=0x0E3A).contains(&u) || (0x0E3F..=0x0E5B).contains(&u)).then(|| (u - 0x0E00 + 0xA0) as u8)
}

fn thai_char(b: u8) -> Option<char> {
    ((0xA1..=0xDA).contains(&b) || (0xDF..=0xFB).contains(&b)).then(|| char::from_u32(0x0E00 + (b as u32 - 0xA0))).flatten()
}

/// Text for a definition file: one line of ASCII.
pub fn enc_text(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() {
        match c {
            '&' | '<' | '>' | '"' => o.push_str(&format!("&#{:03};", c as u32)),
            ' '..='~' => o.push(c),
            '\r' | '\n' | '\t' => o.push(' '),
            _ => match thai_byte(c) {
                Some(b) => o.push_str(&format!("&#{b:03};")),
                None => o.push('?'),
            },
        }
    }
    o
}

/// Text from a definition file. A decimal entity up to 255 is a code page 874 byte, as
/// TunerPro 5 writes it; larger ones and hexadecimal ones are Unicode.
pub fn dec_text(s: &str) -> String {
    let mut o = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        o.push_str(&rest[..i]);
        rest = &rest[i..];
        let ch = rest.find(';').filter(|&e| e <= 10).and_then(|e| {
            let ent = &rest[1..e];
            let c = if let Some(h) = ent.strip_prefix("#x").or_else(|| ent.strip_prefix("#X")) {
                u32::from_str_radix(h, 16).ok().and_then(char::from_u32)
            } else if let Some(d) = ent.strip_prefix('#') {
                d.parse::<u32>().ok().and_then(|n| match n {
                    0..=0x7F => char::from_u32(n),
                    0x80..=0xFF => Some(thai_char(n as u8).unwrap_or('?')),
                    _ => char::from_u32(n),
                })
            } else {
                match ent {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    _ => None,
                }
            };
            c.map(|c| (c, e))
        });
        match ch {
            Some((c, e)) => {
                o.push(c);
                rest = &rest[e + 1..];
            }
            None => {
                o.push('&');
                rest = &rest[1..];
            }
        }
    }
    o.push_str(rest);
    o
}

/// The file as text, or why it can't be read as one. Protected definitions aren't text.
fn plain_text(b: &[u8]) -> Result<String, String> {
    let b = b.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(b);
    let first = b.iter().find(|c| !c.is_ascii_whitespace());
    if first != Some(&b'<') {
        return Err("This definition file is protected or isn't XDF text, so its tables can't be read.".into());
    }
    Ok(match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => b.iter().map(|&x| if x < 0x80 { x as char } else { thai_char(x).unwrap_or('?') }).collect(),
    })
}

// ---------- a small XML reader ----------

#[derive(Debug, Default)]
struct Node {
    name: String,
    attrs: Vec<(String, String)>,
    text: String,
    kids: Vec<Node>,
}

impl Node {
    fn kid(&self, name: &str) -> Option<&Node> {
        self.kids.iter().find(|k| k.name == name)
    }
    fn kids_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> {
        self.kids.iter().filter(move |k| k.name == name)
    }
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|a| a.0 == name).map(|a| a.1.as_str())
    }
    fn num(&self, name: &str) -> Option<i64> {
        self.attr(name).and_then(num)
    }
    fn text_of(&self, name: &str) -> Option<String> {
        self.kid(name).map(|k| dec_text(k.text.trim()))
    }
    fn axis(&self, id: &str) -> Option<&Node> {
        self.kids_named("XDFAXIS").find(|a| a.attr("id") == Some(id))
    }
}

fn num(s: &str) -> Option<i64> {
    let s = s.trim();
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => i64::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}

fn tag_end(s: &str, from: usize) -> Option<usize> {
    let mut quote = None;
    for (i, c) in s.as_bytes().iter().enumerate().skip(from) {
        match (quote, c) {
            (Some(q), c) if *c == q => quote = None,
            (Some(_), _) => {}
            (None, b'"' | b'\'') => quote = Some(*c),
            (None, b'>') => return Some(i),
            _ => {}
        }
    }
    None
}

fn parse_tag(inner: &str) -> Node {
    let inner = inner.trim();
    let split = inner.find(|c: char| c.is_whitespace()).unwrap_or(inner.len());
    let mut node = Node { name: inner[..split].to_string(), ..Node::default() };
    let mut rest = inner[split..].trim_start();
    while let Some(eq) = rest.find('=') {
        let key = rest[..eq].trim().to_string();
        let after = rest[eq + 1..].trim_start();
        let Some(q) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else { break };
        let Some(close) = after[1..].find(q) else { break };
        node.attrs.push((key, after[1..1 + close].to_string()));
        rest = after[close + 2..].trim_start();
    }
    node
}

fn parse_xml(s: &str) -> Result<Node, String> {
    let mut stack = vec![Node { name: "#root".into(), ..Node::default() }];
    let mut i = 0;
    while i < s.len() {
        if s.as_bytes()[i] != b'<' {
            let next = s[i..].find('<').map(|e| i + e).unwrap_or(s.len());
            stack.last_mut().unwrap().text.push_str(&s[i..next]);
            i = next;
            continue;
        }
        let skip_to = |end: &str| s[i..].find(end).map(|e| i + e + end.len()).unwrap_or(s.len());
        if s[i..].starts_with("<!--") {
            i = skip_to("-->");
            continue;
        }
        if s[i..].starts_with("<?") {
            i = skip_to("?>");
            continue;
        }
        if s[i..].starts_with("<!") {
            i = skip_to(">");
            continue;
        }
        let end = tag_end(s, i).ok_or("The definition file ends inside a tag.")?;
        let inner = &s[i + 1..end];
        i = end + 1;
        if let Some(name) = inner.strip_prefix('/') {
            if stack.len() < 2 {
                return Err(format!("The definition file closes <{}> that was never opened.", name.trim()));
            }
            let node = stack.pop().unwrap();
            if node.name != name.trim() {
                return Err(format!("The definition file closes <{}> inside <{}>.", name.trim(), node.name));
            }
            stack.last_mut().unwrap().kids.push(node);
        } else if let Some(body) = inner.strip_suffix('/') {
            stack.last_mut().unwrap().kids.push(parse_tag(body));
        } else {
            stack.push(parse_tag(inner));
        }
    }
    if stack.len() != 1 {
        return Err(format!("The definition file never closes <{}>.", stack.last().unwrap().name));
    }
    Ok(stack.pop().unwrap())
}

// ---------- equations ----------

struct Eval<'a> {
    s: &'a [u8],
    i: usize,
    x: f64,
}

impl Eval<'_> {
    fn peek(&mut self) -> Option<u8> {
        while self.s.get(self.i).is_some_and(|c| c.is_ascii_whitespace()) {
            self.i += 1;
        }
        self.s.get(self.i).copied()
    }
    fn expr(&mut self) -> Option<f64> {
        let mut v = self.term()?;
        loop {
            match self.peek() {
                Some(b'+') => {
                    self.i += 1;
                    v += self.term()?;
                }
                Some(b'-') => {
                    self.i += 1;
                    v -= self.term()?;
                }
                _ => return Some(v),
            }
        }
    }
    fn term(&mut self) -> Option<f64> {
        let mut v = self.unary()?;
        loop {
            match self.peek() {
                Some(b'*') => {
                    self.i += 1;
                    v *= self.unary()?;
                }
                Some(b'/') => {
                    self.i += 1;
                    v /= self.unary()?;
                }
                _ => return Some(v),
            }
        }
    }
    fn unary(&mut self) -> Option<f64> {
        match self.peek()? {
            b'-' => {
                self.i += 1;
                Some(-self.unary()?)
            }
            b'+' => {
                self.i += 1;
                self.unary()
            }
            _ => self.primary(),
        }
    }
    fn primary(&mut self) -> Option<f64> {
        match self.peek()? {
            b'(' => {
                self.i += 1;
                let v = self.expr()?;
                (self.peek() == Some(b')')).then(|| {
                    self.i += 1;
                    v
                })
            }
            b'X' | b'x' => {
                self.i += 1;
                // Any other name is a second input, which a map can't carry.
                let more = self.s.get(self.i).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_');
                (!more).then_some(self.x)
            }
            c if c.is_ascii_digit() || c == b'.' => {
                let from = self.i;
                while self.s.get(self.i).is_some_and(|c| c.is_ascii_digit() || *c == b'.') {
                    self.i += 1;
                }
                if matches!(self.s.get(self.i), Some(b'e' | b'E')) {
                    let mut j = self.i + 1;
                    if matches!(self.s.get(j), Some(b'+' | b'-')) {
                        j += 1;
                    }
                    if self.s.get(j).is_some_and(|c| c.is_ascii_digit()) {
                        while self.s.get(j).is_some_and(|c| c.is_ascii_digit()) {
                            j += 1;
                        }
                        self.i = j;
                    }
                }
                std::str::from_utf8(&self.s[from..self.i]).ok()?.parse().ok()
            }
            _ => None,
        }
    }
}

fn eval(eq: &str, x: f64) -> Option<f64> {
    let mut e = Eval { s: eq.as_bytes(), i: 0, x };
    let v = e.expr()?;
    (e.peek().is_none() && v.is_finite()).then_some(v)
}

/// A map shows `stored x factor + offset`, or the same on `1 / stored`. This finds those three
/// for an equation that comes to that, however it is written: (factor, offset, reciprocal).
pub fn linear(eq: &str) -> Option<(f64, f64, bool)> {
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0);
    // Working it out from differences leaves dust in the last digits (0.3572 as 0.357199...).
    let tidy = |v: f64| format!("{v:.11e}").parse::<f64>().unwrap_or(v);
    let (f1, f2, f3, f4) = (eval(eq, 1.0)?, eval(eq, 2.0)?, eval(eq, 3.0)?, eval(eq, 4.0)?);
    if close(f2 - f1, f3 - f2) && close(f3 - f2, f4 - f3) {
        let k = tidy(f2 - f1);
        return Some((k, tidy(f1 - k), false));
    }
    let a = tidy((f1 - f2) * 2.0);
    let b = tidy(f1 - a);
    (close(a / 2.0 + b, f2) && close(a / 3.0 + b, f3) && close(a / 4.0 + b, f4)).then_some((a, b, true))
}

fn num_text(v: f64) -> String {
    if v < 0.0 {
        format!("({v})")
    } else {
        format!("{v}")
    }
}

fn equation(factor: f64, offset: f64, reciprocal: bool) -> String {
    let mut s = match (reciprocal, factor == 1.0) {
        (true, _) => format!("{}/X", num_text(factor)),
        (false, true) => "X".to_string(),
        (false, false) => format!("X*{}", num_text(factor)),
    };
    if offset > 0.0 {
        s.push_str(&format!("+{offset}"));
    } else if offset < 0.0 {
        s.push_str(&format!("-{}", -offset));
    }
    s
}

// ---------- reading ----------

struct Ctx<'a> {
    base: i64,
    subtract: bool,
    cats: HashMap<i64, String>,
    by_id: HashMap<i64, &'a Node>,
}

impl Ctx<'_> {
    /// Where an address of the definition lies in the file.
    fn file_addr(&self, a: i64) -> Option<usize> {
        usize::try_from(if self.subtract { a - self.base } else { a + self.base }).ok()
    }
    /// What to add to a place in the file to get the definition's address for it.
    fn shift(&self) -> i64 {
        if self.subtract {
            self.base
        } else {
            -self.base
        }
    }
}

fn ctx_of(fmt: &Node) -> Ctx<'_> {
    let head = fmt.kid("XDFHEADER");
    let base = head.and_then(|h| h.kid("BASEOFFSET"));
    Ctx {
        base: base.and_then(|b| b.num("offset")).unwrap_or(0),
        subtract: base.and_then(|b| b.num("subtract")).unwrap_or(0) != 0,
        cats: head
            .map(|h| h.kids_named("CATEGORY").filter_map(|c| Some((c.num("index")?, dec_text(c.attr("name")?)))).collect())
            .unwrap_or_default(),
        by_id: fmt.kids_named("XDFTABLE").filter_map(|t| Some((t.num("uniqueid")?, t))).collect(),
    }
}

#[derive(Clone, Copy, Default, Debug)]
struct Emb {
    addr: Option<usize>,
    bits: usize,
    rows: usize,
    cols: usize,
    flags: u32,
    major: i64,
    minor: i64,
}

fn emb(n: &Node, ctx: &Ctx) -> Emb {
    let Some(e) = n.kid("EMBEDDEDDATA") else { return Emb::default() };
    Emb {
        addr: e.num("mmedaddress").and_then(|a| ctx.file_addr(a)),
        bits: e.num("mmedelementsizebits").unwrap_or(8) as usize,
        rows: e.num("mmedrowcount").unwrap_or(1).max(1) as usize,
        cols: e.num("mmedcolcount").unwrap_or(1).max(1) as usize,
        flags: e.num("mmedtypeflags").unwrap_or(0) as u32,
        major: e.num("mmedmajorstridebits").unwrap_or(0),
        minor: e.num("mmedminorstridebits").unwrap_or(0),
    }
}

fn org_of(bits: usize, flags: u32) -> Option<DataOrg> {
    let lsb = flags & TYPE_LSB_FIRST != 0;
    Some(match (bits, flags & TYPE_FLOAT != 0, lsb) {
        (8, false, _) => DataOrg::Byte,
        (16, false, true) => DataOrg::LoHi,
        (16, false, false) => DataOrg::HiLo,
        (32, false, true) => DataOrg::LoHiLoHi,
        (32, false, false) => DataOrg::HiLoHiLo,
        (32, true, true) => DataOrg::FloatLoHi,
        (32, true, false) => DataOrg::FloatHiLo,
        _ => return None,
    })
}

/// (factor, offset, reciprocal), and the equation itself when a map can't carry it.
fn scale_of(n: &Node) -> ((f64, f64, bool), Option<String>) {
    let eq = n.kid("MATH").and_then(|m| m.attr("equation")).map(dec_text).unwrap_or_else(|| "X".into());
    match linear(&eq) {
        Some(s) => (s, None),
        None => ((1.0, 0.0, false), Some(eq)),
    }
}

fn decimals(n: &Node) -> u32 {
    n.text_of("decimalpl").and_then(|s| s.parse().ok()).unwrap_or(0)
}

fn axis_of(ax: Option<&Node>, n: usize, ctx: &Ctx) -> Axis {
    let mut a = Axis::default();
    let Some(ax) = ax else { return a };
    if let Some(u) = ax.text_of("units").filter(|u| !u.is_empty()) {
        a.unit = u;
    }
    // An axis can point at another table of the definition that holds its values.
    let link = ax.kid("embedinfo").filter(|e| e.num("type") == Some(3)).and_then(|e| e.num("linkobjid"));
    let src = match link {
        Some(id) => ctx.by_id.get(&id).and_then(|t| t.axis("z")),
        None => Some(ax),
    };
    if let Some(src) = src {
        let e = emb(src, ctx);
        if let (Some(addr), Some(org)) = (e.addr, org_of(e.bits, e.flags)) {
            let step = [e.major, e.minor].into_iter().find(|s| *s > 0).map(|s| s as usize / 8).unwrap_or(org.width());
            let ((factor, offset, reciprocal), _) = scale_of(src);
            a.src = AxisSrc::Rom;
            a.addr = addr;
            a.org = org;
            a.signed = e.flags & TYPE_SIGNED != 0;
            a.skip = step.saturating_sub(org.width());
            a.factor = factor;
            a.offset = offset;
            a.reciprocal = reciprocal;
            a.precision = decimals(src);
            if let Some(u) = src.text_of("units").filter(|u| !u.is_empty()) {
                a.unit = u;
            }
            return a;
        }
    }
    // Otherwise the values are typed into the definition.
    let mut labels: Vec<(i64, &str)> = ax.kids_named("LABEL").filter_map(|l| Some((l.num("index")?, l.attr("value")?))).collect();
    labels.sort_by_key(|l| l.0);
    let vals: Option<Vec<f64>> = labels.iter().map(|l| l.1.trim().parse::<f64>().ok()).collect();
    if let Some(v) = vals {
        let counting = v.iter().enumerate().all(|(i, x)| *x == (i + 1) as f64);
        let flat = v.windows(2).all(|w| w[0] == w[1]);
        if v.len() == n && n > 1 && !counting && !flat {
            a.precision = labels
                .iter()
                .map(|l| l.1.trim().split_once('.').map(|p| p.1.trim_end_matches('0').len()).unwrap_or(0))
                .max()
                .unwrap_or(0)
                .min(6) as u32;
            a.src = AxisSrc::User;
            a.user = v;
        }
    }
    a
}

fn folder_of(t: &Node, ctx: &Ctx) -> String {
    // A table can sit in several categories; the last one is the most specific.
    filed_under(t).last().and_then(|m| ctx.cats.get(&(m.1 - 1))).cloned().unwrap_or_else(|| "Maps".into())
}

/// Fills what a table and a single value share. `holder` carries the address and the equation.
fn item_to_map(item: &Node, holder: &Node, ctx: &Ctx) -> Option<MapDef> {
    let e = emb(holder, ctx);
    let org = org_of(e.bits, e.flags)?;
    let ((factor, offset, reciprocal), odd) = scale_of(holder);
    let mut m = MapDef {
        name: item.text_of("title").filter(|t| !t.is_empty()).unwrap_or_else(|| "Map".into()),
        comment: item.text_of("description").unwrap_or_default(),
        folder: folder_of(item, ctx),
        start: e.addr?,
        org,
        signed: e.flags & TYPE_SIGNED != 0,
        factor,
        offset,
        reciprocal,
        precision: decimals(holder),
        ..MapDef::default()
    };
    if let Some(u) = holder.text_of("units").filter(|u| !u.is_empty()) {
        m.unit = u;
    }
    if let Some(eq) = odd {
        let note = format!("XDF equation: {eq}");
        m.comment = if m.comment.is_empty() { note } else { format!("{} | {note}", m.comment) };
    }
    Some(m)
}

fn table_to_map(t: &Node, ctx: &Ctx) -> Option<MapDef> {
    let z = t.axis("z")?;
    let mut m = item_to_map(t, z, ctx)?;
    let e = emb(z, ctx);
    let x = axis_of(t.axis("x"), e.cols, ctx);
    let y = axis_of(t.axis("y"), e.rows, ctx);
    if e.rows == 1 && e.cols == 1 {
        m.kind = MapKind::Single;
        (m.cols, m.rows) = (1, 1);
    } else if e.rows == 1 || e.cols == 1 {
        // One line of values, whichever way the definition lays it out.
        m.kind = MapKind::OneDim;
        (m.cols, m.rows) = (e.rows * e.cols, 1);
        m.x = if e.cols == 1 { y } else { x };
    } else if e.flags & TYPE_COL_MAJOR != 0 {
        m.kind = MapKind::TwoDimInv;
        (m.cols, m.rows) = (e.rows, e.cols);
        (m.x, m.y) = (y, x);
    } else {
        m.kind = MapKind::TwoDim;
        (m.cols, m.rows) = (e.cols, e.rows);
        (m.x, m.y) = (x, y);
    }
    Some(m)
}

fn constant_to_map(c: &Node, ctx: &Ctx) -> Option<MapDef> {
    let mut m = item_to_map(c, c, ctx)?;
    m.kind = MapKind::Single;
    (m.cols, m.rows) = (1, 1);
    Some(m)
}

fn format_of(root: &Node) -> Result<&Node, String> {
    root.kid("XDFFORMAT").ok_or_else(|| "This file has no XDF definition in it.".to_string())
}

/// The tables and single values of a definition as maps. Switches and patches have no map form.
pub fn read_xdf(bytes: &[u8]) -> Result<Vec<MapDef>, String> {
    let text = plain_text(bytes)?;
    let root = parse_xml(&text)?;
    let fmt = format_of(&root)?;
    let ctx = ctx_of(fmt);
    let mut maps: Vec<MapDef> = fmt.kids_named("XDFTABLE").filter_map(|t| table_to_map(t, &ctx)).collect();
    maps.extend(fmt.kids_named("XDFCONSTANT").filter_map(|c| constant_to_map(c, &ctx)));
    if maps.is_empty() {
        return Err("This definition has no table with an address.".into());
    }
    Ok(maps)
}

// ---------- writing ----------

fn type_flags(org: DataOrg, signed: bool, col_major: bool) -> u32 {
    let mut f = 0;
    if signed {
        f |= TYPE_SIGNED;
    }
    if org.width() > 1 && !org.big_endian() {
        f |= TYPE_LSB_FIRST;
    }
    if col_major {
        f |= TYPE_COL_MAJOR;
    }
    if org.is_float() {
        f |= TYPE_FLOAT;
    }
    f
}

fn flags_attr(f: u32) -> String {
    if f == 0 {
        String::new()
    } else {
        format!("mmedtypeflags=\"0x{f:02X}\" ")
    }
}

fn addr_text(a: usize, shift: i64) -> Option<String> {
    let v = a as i64 + shift;
    (v >= 0).then(|| format!("0x{v:X}"))
}

struct Out<'a> {
    s: String,
    nl: &'a str,
}

impl Out<'_> {
    fn line(&mut self, indent: usize, t: &str) {
        for _ in 0..indent {
            self.s.push(' ');
        }
        self.s.push_str(t);
        self.s.push_str(self.nl);
    }
    fn math(&mut self, indent: usize, eq: &str) {
        self.line(indent, &format!("<MATH equation=\"{eq}\">"));
        self.line(indent + 2, "<VAR id=\"X\" />");
        self.line(indent, "</MATH>");
    }
    fn units(&mut self, indent: usize, unit: &str) {
        let u = unit.trim();
        if !u.is_empty() && u != "-" {
            self.line(indent, &format!("<units>{}</units>", enc_text(u)));
        }
    }
    fn tail(&mut self, indent: usize) {
        self.line(indent, "<datatype>0</datatype>");
        self.line(indent, "<unittype>0</unittype>");
        self.line(indent, "<DALINK index=\"0\" />");
    }
}

fn axis_xml(o: &mut Out, id: &str, a: &Axis, n: usize, d: &[u8], shift: i64) {
    o.line(4, &format!("<XDFAXIS id=\"{id}\" uniqueid=\"0x0\">"));
    let addr = if a.src == AxisSrc::Rom && n > 1 { addr_text(a.addr, shift) } else { None };
    if let Some(addr) = addr {
        let stride = if a.skip > 0 { (a.org.width() + a.skip) * 8 } else { 0 };
        o.line(
            6,
            &format!(
                "<EMBEDDEDDATA {}mmedaddress=\"{addr}\" mmedelementsizebits=\"{}\" mmedcolcount=\"{n}\" mmedmajorstridebits=\"{stride}\" mmedminorstridebits=\"0\" />",
                flags_attr(type_flags(a.org, a.signed, false)),
                a.org.width() * 8
            ),
        );
        o.units(6, &a.unit);
        o.line(6, &format!("<indexcount>{n}</indexcount>"));
        o.line(6, &format!("<decimalpl>{}</decimalpl>", a.precision));
        o.line(6, "<embedinfo type=\"1\" />");
        o.tail(6);
        o.math(6, &equation(a.factor, a.offset, a.reciprocal));
    } else {
        o.line(6, "<EMBEDDEDDATA mmedelementsizebits=\"8\" mmedmajorstridebits=\"-32\" mmedminorstridebits=\"0\" />");
        o.units(6, &a.unit);
        o.line(6, &format!("<indexcount>{n}</indexcount>"));
        let counting = n > 1 && matches!(a.src, AxisSrc::None | AxisSrc::Enumerate);
        if counting {
            o.line(6, "<outputtype>4</outputtype>");
        }
        o.tail(6);
        if n <= 1 {
            o.line(6, "<LABEL index=\"0\" value=\"0.00\" />");
        } else if counting {
            for i in 0..n {
                o.line(6, &format!("<LABEL index=\"{i}\" value=\"{}\" />", i + 1));
            }
        } else {
            // Axes that add up, count down or run backwards have no form here: their values go in.
            for (i, v) in a.values(d, n).into_iter().enumerate() {
                o.line(6, &format!("<LABEL index=\"{i}\" value=\"{}\" />", enc_text(&a.text(v))));
            }
        }
        o.math(6, "X");
    }
    o.line(4, "</XDFAXIS>");
}

fn description_of(m: &MapDef) -> String {
    let parts: Vec<&str> = [m.desc.trim(), m.comment.trim()].into_iter().filter(|p| !p.is_empty() && *p != "-").collect();
    parts.join(" | ").chars().take(MAX_DESCRIPTION).collect()
}

fn shown_range(m: &MapDef, d: &[u8]) -> (f64, f64) {
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for i in 0..m.cells() {
        let Some(raw) = m.org.read(d, m.start + i * m.org.width(), m.signed) else { continue };
        let r = if m.reciprocal && raw != 0.0 { 1.0 / raw } else { raw };
        let v = r * m.factor + m.offset;
        if v.is_finite() {
            lo = lo.min(v);
            hi = hi.max(v);
        }
    }
    if lo > hi {
        (0.0, 0.0)
    } else {
        (lo, hi)
    }
}

/// The categories an item is filed under, as (slot, category) pairs sorted by slot. The
/// category counts from 1, as the file does.
fn filed_under(item: &Node) -> Vec<(i64, i64)> {
    let mut mems: Vec<(i64, i64)> = item.kids_named("CATEGORYMEM").filter_map(|c| Some((c.num("index")?, c.num("category")?))).collect();
    mems.sort();
    mems
}

/// One map as a table, or as a single value, filed under `categories` (see `filed_under`).
/// Nothing comes back when the map lies before the start of the definition's address space.
fn item_xml(m: &MapDef, d: &[u8], uid: i64, categories: &[(i64, i64)], shift: i64, nl: &str) -> Option<String> {
    let addr = addr_text(m.start, shift)?;
    let mut o = Out { s: String::new(), nl };
    let single = m.kind == MapKind::Single || m.cells() == 1;
    let title = if m.name.trim().is_empty() { format!("Map {:X}", m.start) } else { m.name.clone() };
    let desc = description_of(m);
    let bits = m.org.width() * 8;
    let eq = equation(m.factor, m.offset, m.reciprocal);
    if single {
        o.line(2, &format!("<XDFCONSTANT uniqueid=\"0x{uid:X}\">"));
    } else {
        o.line(2, &format!("<XDFTABLE uniqueid=\"0x{uid:X}\" flags=\"0x0\">"));
    }
    o.line(4, &format!("<title>{}</title>", enc_text(&title)));
    if !desc.is_empty() {
        o.line(4, &format!("<description>{}</description>", enc_text(&desc)));
    }
    for (slot, category) in categories {
        o.line(4, &format!("<CATEGORYMEM index=\"{slot}\" category=\"{category}\" />"));
    }
    if single {
        o.line(
            4,
            &format!(
                "<EMBEDDEDDATA {}mmedaddress=\"{addr}\" mmedelementsizebits=\"{bits}\" mmedmajorstridebits=\"0\" mmedminorstridebits=\"0\" />",
                flags_attr(type_flags(m.org, m.signed, false))
            ),
        );
        o.units(4, &m.unit);
        o.line(4, &format!("<decimalpl>{}</decimalpl>", m.precision));
        o.tail(4);
        o.math(4, &eq);
        o.line(2, "</XDFCONSTANT>");
        return Some(o.s);
    }
    // Stored the other way round, a map's columns are the file's rows.
    let inv = m.kind == MapKind::TwoDimInv;
    let (rows, cols, x, y) = if inv { (m.cols, m.rows, &m.y, &m.x) } else { (m.rows, m.cols, &m.x, &m.y) };
    axis_xml(&mut o, "x", x, cols, d, shift);
    axis_xml(&mut o, "y", y, rows, d, shift);
    o.line(4, "<XDFAXIS id=\"z\">");
    o.line(
        6,
        &format!(
            "<EMBEDDEDDATA {}mmedaddress=\"{addr}\" mmedelementsizebits=\"{bits}\" mmedrowcount=\"{rows}\" mmedcolcount=\"{cols}\" mmedmajorstridebits=\"0\" mmedminorstridebits=\"0\" />",
            flags_attr(type_flags(m.org, m.signed, inv))
        ),
    );
    o.units(6, &m.unit);
    let (lo, hi) = shown_range(m, d);
    o.line(6, &format!("<decimalpl>{}</decimalpl>", m.precision));
    o.line(6, &format!("<min>{lo:.6}</min>"));
    o.line(6, &format!("<max>{hi:.6}</max>"));
    o.line(6, &format!("<outputtype>{}</outputtype>", if m.radix == 16 { 3 } else { 1 }));
    o.math(6, &eq);
    o.line(4, "</XDFAXIS>");
    o.line(2, "</XDFTABLE>");
    Some(o.s)
}

fn stamp() -> String {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (days, rem) = ((s / 86400) as i64, s % 86400);
    let z = days + 719468;
    let (era, doe) = (z.div_euclid(146097), z.rem_euclid(146097));
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    format!("{month:02}/{day:02}/{year} {:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// A whole new definition holding these maps, one category per map folder. It carries no
/// title of its own, like the files TunerPro saves, so TunerPro shows it by its file name.
pub fn write_xdf(maps: &[MapDef], d: &[u8]) -> String {
    let nl = "\r\n";
    let mut folders: Vec<&str> = vec![];
    for m in maps {
        if !folders.contains(&m.folder.as_str()) {
            folders.push(&m.folder);
        }
    }
    let mut o = Out { s: String::new(), nl };
    o.line(0, &format!("<!-- Written {} -->", stamp()));
    o.line(0, "<XDFFORMAT version=\"1.80\">");
    o.line(2, "<XDFHEADER>");
    o.line(4, "<flags>0x1</flags>");
    o.line(4, "<description></description>");
    o.line(4, "<BASEOFFSET offset=\"0\" subtract=\"0\" />");
    o.line(4, "<DEFAULTS datasizeinbits=\"8\" sigdigits=\"2\" outputtype=\"1\" signed=\"0\" lsbfirst=\"0\" float=\"0\" />");
    o.line(
        4,
        &format!(
            "<REGION type=\"0xFFFFFFFF\" startaddress=\"0x0\" size=\"0x{:X}\" regioncolor=\"0x0\" regionflags=\"0x0\" name=\"Binary File\" desc=\"This region describes the bin file edited by this XDF\" />",
            d.len()
        ),
    );
    for (i, f) in folders.iter().enumerate() {
        o.line(4, &format!("<CATEGORY index=\"0x{i:X}\" name=\"{}\" />", enc_text(f)));
    }
    o.line(2, "</XDFHEADER>");
    for (i, m) in maps.iter().enumerate() {
        let cat = folders.iter().position(|f| *f == m.folder).unwrap_or(0) as i64 + 1;
        if let Some(x) = item_xml(m, d, 0x1000 + i as i64, &[(0, cat)], 0, nl) {
            o.s.push_str(&x);
        }
    }
    o.s.push_str("</XDFFORMAT>");
    o.s.push_str(nl);
    o.s
}

pub struct Merged {
    pub bytes: Vec<u8>,
    /// Maps written as new tables.
    pub added: usize,
    /// Maps the definition already had at the same place and size.
    pub kept: usize,
    /// Maps that lie outside the definition's address space.
    pub outside: usize,
}

fn find_last(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).rposition(|w| w == needle)
}

/// Start of the line `at` is on, as long as only blanks come before it.
fn line_start(b: &[u8], at: usize) -> usize {
    let mut i = at;
    while i > 0 && (b[i - 1] == b' ' || b[i - 1] == b'\t') {
        i -= 1;
    }
    i
}

/// Adds the maps a definition doesn't have yet to the end of it. Everything already in the
/// file stays byte for byte, so its switches, patches and descriptions survive.
pub fn merge_xdf(existing: &[u8], maps: &[MapDef], d: &[u8]) -> Result<Merged, String> {
    let text = plain_text(existing)?;
    let root = parse_xml(&text)?;
    let fmt = format_of(&root)?;
    let ctx = ctx_of(fmt);
    let shift = ctx.shift();
    let nl = if existing.windows(2).any(|w| w == b"\r\n") { "\r\n" } else { "\n" };

    let mut have: HashSet<(usize, usize)> = HashSet::new();
    for t in fmt.kids_named("XDFTABLE") {
        if let Some(z) = t.axis("z") {
            let e = emb(z, &ctx);
            if let Some(a) = e.addr {
                have.insert((a, e.rows * e.cols * e.bits / 8));
            }
        }
    }
    for c in fmt.kids_named("XDFCONSTANT") {
        let e = emb(c, &ctx);
        if let Some(a) = e.addr {
            have.insert((a, e.bits / 8));
        }
    }
    // How the definition files the items of each category, so a new map sits with the others
    // of its folder (a definition can nest its categories, as "AlexDev-TH" then "2 Fuel").
    let mut filing: HashMap<i64, Vec<(i64, i64)>> = HashMap::new();
    for item in fmt.kids.iter().filter(|k| k.name == "XDFTABLE" || k.name == "XDFCONSTANT") {
        let mems = filed_under(item);
        if let Some(last) = mems.last() {
            filing.entry(last.1 - 1).or_insert(mems);
        }
    }
    let mut used: HashSet<i64> = fmt.kids.iter().filter_map(|k| k.num("uniqueid")).collect();
    let mut cats: HashMap<String, i64> = ctx.cats.iter().map(|(i, n)| (n.clone(), *i)).collect();
    let mut next_cat = ctx.cats.keys().copied().max().unwrap_or(-1);
    let mut next_id = 0x1000;

    let (mut new_cats, mut body) = (String::new(), String::new());
    let (mut added, mut kept, mut outside) = (0, 0, 0);
    for m in maps {
        if have.contains(&(m.start, m.byte_len())) {
            kept += 1;
            continue;
        }
        while used.contains(&next_id) {
            next_id += 1;
        }
        let known = cats.get(&m.folder).copied();
        let cat = known.unwrap_or(next_cat + 1);
        let members = known.and_then(|c| filing.get(&c).cloned()).unwrap_or_else(|| vec![(0, cat + 1)]);
        match item_xml(m, d, next_id, &members, shift, nl) {
            Some(x) => {
                if known.is_none() {
                    next_cat = cat;
                    cats.insert(m.folder.clone(), cat);
                    new_cats.push_str(&format!("    <CATEGORY index=\"0x{cat:X}\" name=\"{}\" />{nl}", enc_text(&m.folder)));
                }
                body.push_str(&x);
                used.insert(next_id);
                have.insert((m.start, m.byte_len()));
                added += 1;
            }
            None => outside += 1,
        }
    }

    let mut bytes = existing.to_vec();
    if added > 0 {
        let end = find_last(&bytes, b"</XDFFORMAT>").ok_or("The definition file has no end mark.")?;
        let at = line_start(&bytes, end);
        bytes.splice(at..at, body.bytes());
        if !new_cats.is_empty() {
            let head = find_last(&bytes[..at], b"</XDFHEADER>").ok_or("The definition file has no header to add a category to.")?;
            let at = line_start(&bytes, head);
            bytes.splice(at..at, new_cats.bytes());
        }
    }
    Ok(Merged { bytes, added, kept, outside })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> Vec<u8> {
        let mut d = vec![0u8; 0x400];
        for (i, b) in d.iter_mut().enumerate() {
            *b = (i * 7 % 251) as u8;
        }
        d
    }

    fn fuel() -> MapDef {
        MapDef {
            name: "น้ำมัน 1-4".into(),
            folder: "น้ำมัน".into(),
            comment: "ตารางหลัก <A&B>".into(),
            unit: "ms".into(),
            start: 0x7C,
            cols: 5,
            rows: 4,
            org: DataOrg::LoHi,
            factor: 0.004,
            precision: 3,
            x: Axis { src: AxisSrc::Rom, addr: 0x40, org: DataOrg::LoHi, factor: 0.25, unit: "rpm".into(), ..Axis::default() },
            y: Axis { src: AxisSrc::Rom, addr: 0x60, org: DataOrg::Byte, skip: 1, ..Axis::default() },
            ..MapDef::default()
        }
    }

    #[test]
    fn thai_text_is_written_the_way_tunerpro_5_does() {
        assert_eq!(enc_text("แกน TPS"), "&#225;&#161;&#185; TPS");
        assert_eq!(dec_text("&#225;&#161;&#185; TPS"), "แกน TPS");
        assert_eq!(enc_text("a<b & \"c\""), "a&#060;b &#038; &#034;c&#034;");
        assert_eq!(dec_text(&enc_text("a<b & \"c\"")), "a<b & \"c\"");
        assert_eq!(dec_text("x &amp; y &#3649; &#x0E01; &bogus; &"), "x & y แ ก &bogus; &");
        assert_eq!(enc_text("line\r\nbreak"), "line  break");
    }

    #[test]
    fn equations_come_down_to_factor_and_offset() {
        let near = |a: Option<(f64, f64, bool)>, b: (f64, f64, bool)| {
            let a = a.expect("should be readable");
            assert!((a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9 && a.2 == b.2, "{a:?} != {b:?}");
        };
        near(linear("X"), (1.0, 0.0, false));
        near(linear("X*.0969"), (0.0969, 0.0, false));
        near(linear("X/10.65"), (1.0 / 10.65, 0.0, false));
        near(linear("(X - 40) * 0.5"), (0.5, -20.0, false));
        near(linear("0.75*x+-3"), (0.75, -3.0, false));
        near(linear("1500000/X"), (1500000.0, 0.0, true));
        near(linear("100/X+2"), (100.0, 2.0, true));
        assert_eq!(linear("X*X"), None);
        assert_eq!(linear("X+Y"), None);
        assert_eq!(linear("TH(X;1)"), None);
        near(linear(&equation(0.004, 0.0, false)), (0.004, 0.0, false));
        near(linear(&equation(-0.5, -40.0, false)), (-0.5, -40.0, false));
        near(linear(&equation(25e6, 3.0, true)), (25e6, 3.0, true));
    }

    #[test]
    fn maps_survive_a_trip_through_a_definition() {
        let d = data();
        let line = MapDef { name: "Line".into(), kind: MapKind::OneDim, start: 0x200, cols: 9, rows: 1, org: DataOrg::HiLo, signed: true, offset: -40.0, ..MapDef::default() };
        let one = MapDef { name: "Limiter".into(), kind: MapKind::Single, start: 0x300, cols: 1, rows: 1, org: DataOrg::Byte, factor: 50.0, ..MapDef::default() };
        let inv = MapDef { name: "Turned".into(), kind: MapKind::TwoDimInv, start: 0x320, cols: 3, rows: 6, ..MapDef::default() };
        let text = write_xdf(&[fuel(), line.clone(), one.clone(), inv.clone()], &d);
        assert!(text.is_ascii(), "the file has to be plain ASCII");
        assert!(text.contains("mmedtypeflags=\"0x02\" mmedaddress=\"0x7C\" mmedelementsizebits=\"16\" mmedrowcount=\"4\" mmedcolcount=\"5\""));
        assert!(text.contains("<CATEGORYMEM index=\"0\" category=\"1\" />"), "categories count from 1 in a table");
        let back = read_xdf(text.as_bytes()).unwrap();
        assert_eq!(back.len(), 4);
        // Tables come first, single values after them.
        let (f, l, i, o) = (&back[0], &back[1], &back[2], &back[3]);
        assert_eq!((f.name.as_str(), f.folder.as_str(), f.unit.as_str()), ("น้ำมัน 1-4", "น้ำมัน", "ms"));
        assert_eq!(f.comment, "ตารางหลัก <A&B>");
        assert_eq!((f.start, f.cols, f.rows, f.org, f.kind), (0x7C, 5, 4, DataOrg::LoHi, MapKind::TwoDim));
        assert!((f.factor - 0.004).abs() < 1e-12 && f.precision == 3);
        assert_eq!((f.x.src, f.x.addr, f.x.org, f.x.unit.as_str()), (AxisSrc::Rom, 0x40, DataOrg::LoHi, "rpm"));
        assert!((f.x.factor - 0.25).abs() < 1e-12);
        assert_eq!((f.y.src, f.y.addr, f.y.org, f.y.skip), (AxisSrc::Rom, 0x60, DataOrg::Byte, 1));
        assert_eq!((l.start, l.cols, l.rows, l.org, l.signed, l.kind), (0x200, 9, 1, DataOrg::HiLo, true, MapKind::OneDim));
        assert_eq!(l.offset, -40.0);
        assert_eq!((i.start, i.cols, i.rows, i.kind), (0x320, 3, 6, MapKind::TwoDimInv));
        assert_eq!((o.start, o.kind, o.factor), (0x300, MapKind::Single, 50.0));
    }

    /// Cut down from a file TunerPro 5 saved: typed labels, a column of values, and an axis
    /// that points at another table.
    const SAVED: &str = "<!-- Written 08/18/2026 21:30:04 -->\r\n<XDFFORMAT version=\"1.80\">\r\n  <XDFHEADER>\r\n    <flags>0x1</flags>\r\n    <description></description>\r\n    <BASEOFFSET offset=\"0\" subtract=\"0\" />\r\n    <CATEGORY index=\"0x0\" name=\"AlexDev-TH\" />\r\n    <CATEGORY index=\"0x1\" name=\"1 &#225;&#161;&#185;\" />\r\n    <CATEGORY index=\"0x2\" name=\"2 Fuel\" />\r\n  </XDFHEADER>\r\n  <XDFCONSTANT uniqueid=\"0x3A10\">\r\n    <title>note only</title>\r\n    <EMBEDDEDDATA mmedelementsizebits=\"8\" mmedmajorstridebits=\"0\" mmedminorstridebits=\"0\" />\r\n    <MATH equation=\"X\">\r\n      <VAR id=\"X\" />\r\n    </MATH>\r\n  </XDFCONSTANT>\r\n  <XDFTABLE uniqueid=\"0x337B\" flags=\"0x30\">\r\n    <title>&#225;&#161;&#185; TPS</title>\r\n    <CATEGORYMEM index=\"0\" category=\"1\" />\r\n    <CATEGORYMEM index=\"1\" category=\"2\" />\r\n    <XDFAXIS id=\"x\" uniqueid=\"0x0\">\r\n      <EMBEDDEDDATA mmedelementsizebits=\"8\" mmedmajorstridebits=\"-32\" mmedminorstridebits=\"0\" />\r\n      <indexcount>1</indexcount>\r\n      <LABEL index=\"0\" value=\"1\" />\r\n    </XDFAXIS>\r\n    <XDFAXIS id=\"y\" uniqueid=\"0x0\">\r\n      <EMBEDDEDDATA mmedelementsizebits=\"8\" mmedmajorstridebits=\"-32\" mmedminorstridebits=\"0\" />\r\n      <indexcount>4</indexcount>\r\n      <LABEL index=\"0\" value=\"1\" />\r\n      <LABEL index=\"1\" value=\"2\" />\r\n      <LABEL index=\"2\" value=\"3\" />\r\n      <LABEL index=\"3\" value=\"4\" />\r\n    </XDFAXIS>\r\n    <XDFAXIS id=\"z\">\r\n      <EMBEDDEDDATA mmedtypeflags=\"0x02\" mmedaddress=\"0x4A\" mmedelementsizebits=\"16\" mmedrowcount=\"4\" mmedmajorstridebits=\"0\" mmedminorstridebits=\"0\" />\r\n      <decimalpl>1</decimalpl>\r\n      <MATH equation=\"X*.0969\">\r\n        <VAR id=\"X\" />\r\n      </MATH>\r\n    </XDFAXIS>\r\n  </XDFTABLE>\r\n  <XDFTABLE uniqueid=\"0x2000\" flags=\"0x30\">\r\n    <title>Fuel</title>\r\n    <description>main table</description>\r\n    <CATEGORYMEM index=\"0\" category=\"1\" />\r\n    <CATEGORYMEM index=\"1\" category=\"3\" />\r\n    <XDFAXIS id=\"x\" uniqueid=\"0x0\">\r\n      <EMBEDDEDDATA mmedelementsizebits=\"8\" mmedmajorstridebits=\"-32\" mmedminorstridebits=\"0\" />\r\n      <indexcount>4</indexcount>\r\n      <embedinfo type=\"3\" linkobjid=\"0x337B\" />\r\n    </XDFAXIS>\r\n    <XDFAXIS id=\"y\" uniqueid=\"0x0\">\r\n      <EMBEDDEDDATA mmedelementsizebits=\"8\" mmedmajorstridebits=\"-32\" mmedminorstridebits=\"0\" />\r\n      <indexcount>3</indexcount>\r\n      <LABEL index=\"0\" value=\"1000.00\" />\r\n      <LABEL index=\"1\" value=\"2500.50\" />\r\n      <LABEL index=\"2\" value=\"4000.00\" />\r\n    </XDFAXIS>\r\n    <XDFAXIS id=\"z\">\r\n      <EMBEDDEDDATA mmedtypeflags=\"0x02\" mmedaddress=\"0x7C\" mmedelementsizebits=\"16\" mmedrowcount=\"3\" mmedcolcount=\"4\" mmedmajorstridebits=\"0\" mmedminorstridebits=\"0\" />\r\n      <units>ms</units>\r\n      <decimalpl>2</decimalpl>\r\n      <MATH equation=\"X*X\">\r\n        <VAR id=\"X\" />\r\n      </MATH>\r\n    </XDFAXIS>\r\n  </XDFTABLE>\r\n  <XDFPATCH uniqueid=\"0x10\">\r\n    <title>keep me</title>\r\n    <XDFPATCHENTRY name=\"b3\" address=\"0xC010\" datasize=\"0x2\" patchdata=\"B164\" basedata=\"D854\" />\r\n  </XDFPATCH>\r\n</XDFFORMAT>";

    #[test]
    fn reads_what_tunerpro_saved() {
        let maps = read_xdf(SAVED.as_bytes()).unwrap();
        assert_eq!(maps.len(), 2, "a value with no address isn't a map");
        let tps = &maps[0];
        assert_eq!((tps.name.as_str(), tps.folder.as_str()), ("แกน TPS", "1 แกน"));
        assert_eq!((tps.start, tps.cols, tps.rows, tps.org, tps.kind), (0x4A, 4, 1, DataOrg::LoHi, MapKind::OneDim));
        assert!((tps.factor - 0.0969).abs() < 1e-12 && tps.precision == 1);
        let fuel = &maps[1];
        assert_eq!((fuel.start, fuel.cols, fuel.rows, fuel.folder.as_str()), (0x7C, 4, 3, "2 Fuel"));
        assert_eq!((fuel.x.src, fuel.x.addr, fuel.x.org), (AxisSrc::Rom, 0x4A, DataOrg::LoHi), "the axis follows its link");
        assert!((fuel.x.factor - 0.0969).abs() < 1e-12);
        assert_eq!((fuel.y.src, fuel.y.user.clone(), fuel.y.precision), (AxisSrc::User, vec![1000.0, 2500.5, 4000.0], 1));
        assert_eq!(fuel.factor, 1.0);
        assert_eq!(fuel.comment, "main table | XDF equation: X*X");
    }

    #[test]
    fn merging_adds_only_what_is_new_and_keeps_the_rest() {
        let d = data();
        let known = MapDef { name: "again".into(), start: 0x7C, cols: 4, rows: 3, org: DataOrg::LoHi, ..MapDef::default() };
        let fresh = MapDef { name: "Ignition".into(), folder: "3 ไฟ".into(), start: 0x100, cols: 5, rows: 5, ..MapDef::default() };
        let other = MapDef { name: "Idle".into(), folder: "2 Fuel".into(), start: 0x180, cols: 6, rows: 1, kind: MapKind::OneDim, ..MapDef::default() };
        let m = merge_xdf(SAVED.as_bytes(), &[known, fresh, other], &d).unwrap();
        assert_eq!((m.added, m.kept, m.outside), (2, 1, 0));
        let text = String::from_utf8(m.bytes.clone()).unwrap();
        // Nothing that was there has moved or changed.
        let cut = SAVED.find("  </XDFHEADER>").unwrap();
        assert!(text.starts_with(&SAVED[..cut]));
        assert!(text.contains("<XDFPATCHENTRY name=\"b3\" address=\"0xC010\" datasize=\"0x2\" patchdata=\"B164\" basedata=\"D854\" />"));
        assert!(text.trim_end().ends_with("</XDFFORMAT>"));
        assert!(text.contains("    <CATEGORY index=\"0x3\" name=\"3 &#228;&#191;\" />\r\n  </XDFHEADER>"));
        assert!(text.contains("<CATEGORYMEM index=\"0\" category=\"4\" />"), "the new category, counted from 1");
        assert_eq!(text.matches("<CATEGORYMEM index=\"1\" category=\"3\" />").count(), 2, "a map in an existing folder is filed like the table already there");
        assert_eq!(text.matches("<CATEGORYMEM index=\"0\" category=\"1\" />").count(), 3);
        let ids: Vec<i64> = parse_xml(&text).unwrap().kid("XDFFORMAT").unwrap().kids.iter().filter_map(|k| k.num("uniqueid")).collect();
        assert_eq!(ids.len(), ids.iter().collect::<HashSet<_>>().len(), "every id is used once");
        assert_eq!(read_xdf(&m.bytes).unwrap().len(), 4);
        // A second time round there is nothing left to add.
        let again = merge_xdf(&m.bytes, &[MapDef { start: 0x100, cols: 5, rows: 5, ..MapDef::default() }], &d).unwrap();
        assert_eq!((again.added, again.kept), (0, 1));
        assert_eq!(again.bytes, m.bytes);
    }

    #[test]
    fn a_definition_with_its_own_address_space_gets_addresses_in_it() {
        let d = data();
        let based = SAVED.replace("<BASEOFFSET offset=\"0\" subtract=\"0\" />", "<BASEOFFSET offset=\"32768\" subtract=\"1\" />");
        // 0x804A in the definition is 0x4A in the file.
        let based = based.replace("mmedaddress=\"0x4A\"", "mmedaddress=\"0x804A\"").replace("mmedaddress=\"0x7C\"", "mmedaddress=\"0x807C\"");
        assert_eq!(read_xdf(based.as_bytes()).unwrap()[0].start, 0x4A);
        let m = merge_xdf(based.as_bytes(), &[MapDef { name: "New".into(), start: 0x100, cols: 5, rows: 5, ..MapDef::default() }], &d).unwrap();
        assert!(String::from_utf8(m.bytes).unwrap().contains("mmedaddress=\"0x8100\""));
    }

    #[test]
    fn protected_and_broken_files_say_so() {
        assert!(read_xdf(&[0x05, 0x22, 0x97, 0x00, 0x00]).unwrap_err().contains("protected"));
        assert!(merge_xdf(&[0x05, 0x22, 0x97], &[], &[]).is_err());
        assert!(read_xdf(b"<XDFFORMAT version=\"1.80\"><XDFHEADER>").is_err());
        assert!(read_xdf(b"<XDFFORMAT version=\"1.80\"></XDFFORMAT>").unwrap_err().contains("no table"));
    }

    /// Checks a real definition when `XDF_REAL` names one: `cargo test real_definition -- --ignored --nocapture`.
    /// With `BIN_REAL` (its bin) and `XDF_OUT` (a folder) it also writes two files to try in
    /// TunerPro: every map as a new definition, and the original with one more map added.
    #[test]
    #[ignore]
    fn real_definition() {
        let path = std::env::var("XDF_REAL").expect("set XDF_REAL to a .xdf file");
        let bytes = std::fs::read(&path).unwrap();
        let maps = read_xdf(&bytes).unwrap();
        for m in &maps {
            println!(
                "{:05X} {:>3}x{:<3} {:14} f={:<10} x:{:?}@{:X} y:{:?}@{:X}  [{}] {}",
                m.start,
                m.cols,
                m.rows,
                m.org.label(),
                m.factor,
                m.x.src,
                m.x.addr,
                m.y.src,
                m.y.addr,
                m.folder,
                m.name
            );
        }
        let again = merge_xdf(&bytes, &maps, &[]).unwrap();
        println!("{} maps; merging them back adds {} and keeps {}", maps.len(), again.added, again.kept);
        assert_eq!(again.added, 0);
        assert_eq!(again.bytes, bytes);
        if let (Ok(out), Ok(bin)) = (std::env::var("XDF_OUT"), std::env::var("BIN_REAL")) {
            let d = std::fs::read(bin).unwrap();
            let out = std::path::Path::new(&out);
            std::fs::write(out.join("ALS-fresh.xdf"), write_xdf(&maps, &d)).unwrap();
            let free = (0..d.len().saturating_sub(70)).rev().find(|a| !maps.iter().any(|m| m.start < a + 70 && *a < m.end())).unwrap();
            let extra = MapDef { name: "แมพทดสอบ ALS".into(), folder: "ALS".into(), start: free, cols: 10, rows: 7, ..MapDef::default() };
            let mut all = maps.clone();
            all.push(extra);
            let m = merge_xdf(&bytes, &all, &d).unwrap();
            assert_eq!((m.added, m.kept), (1, maps.len()));
            std::fs::write(out.join("ALS-merged.xdf"), m.bytes).unwrap();
            println!("wrote ALS-fresh.xdf and ALS-merged.xdf (extra map at {free:X})");
        }
    }
}
