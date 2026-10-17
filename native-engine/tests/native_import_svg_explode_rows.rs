//! Oracle rows and comparison harness for the SVG explode kernel (L10-D, G35).
//!
//! `fixtures/native-import-svg-explode/oracle.txt` was produced by `tools/gen.mjs`, which cuts
//! `_polyClosestPair` / `_mergeHoleIntoExterior` (tools.js) and `worldMatrix` / `collectLeaves` /
//! `explodeLeaf` / `importSVGString` (svg-import.js) verbatim from baseline
//! fbe4b8a33e6d5a229f014154df2ba21988a9fabc and runs them on the shipped Paper.js in a Node vm.
//! Row grammar: see the header of `oracle.txt`. Every check encodes production output into the
//! oracle's own token form and compares the strings, so numbers are compared by IEEE bits.

use super::{
    Bounds, CompoundChild, Contour, ExplodeError, ExplodedLeaf, ExplodedPath, HolePairing,
    ImportLeaf, LeafStyle, PathOrigin, Pt, Seg,
};

pub(crate) const ORACLE: &str = include_str!("fixtures/native-import-svg-explode/oracle.txt");

/// The fields after the kind of every oracle row of `kind` (space separated rows).
pub(crate) fn oracle_rows(kind: &str) -> Vec<Vec<&'static str>> {
    let prefix = format!("{} ", kind);
    ORACLE
        .lines()
        .filter(|line| line.starts_with(&prefix))
        .map(|line| line[prefix.len()..].split(' ').collect())
        .collect()
}

/// `# control <name> K=n ...` counts of the JS baseline under the equivalent source edit.
pub(crate) fn control_count(name: &str, kind: &str) -> usize {
    let prefix = format!("# control {} ", name);
    let line = ORACLE
        .lines()
        .find(|l| l.starts_with(&prefix))
        .expect("control line");
    for pair in line[prefix.len()..].split(' ') {
        let (k, n) = pair.split_once('=').expect("K=n");
        if k == kind {
            return n.parse().expect("count");
        }
    }
    panic!("control {} has no {}", name, kind);
}

pub(crate) fn hex(v: f64) -> String {
    format!("{:016x}", v.to_bits())
}

pub(crate) fn unhex(token: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(token, 16).expect("16 hex digits"))
}

/// `$` for empty; `%XX` for every byte outside `[A-Za-z0-9._+/=;-]`.
pub(crate) fn escape(text: &str) -> String {
    if text.is_empty() {
        return "$".to_string();
    }
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"._+/=;-".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

pub(crate) fn unescape(token: &str) -> String {
    if token == "$" {
        return String::new();
    }
    let bytes = token.as_bytes();
    let mut out: Vec<u8> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let digits = std::str::from_utf8(&bytes[i + 1..i + 3]).expect("hex pair");
            out.push(u8::from_str_radix(digits, 16).expect("hex digits"));
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).expect("utf-8 token")
}

fn dec_pair(token: &str) -> Pt {
    let v: Vec<f64> = token.split(',').map(unhex).collect();
    [v[0], v[1]]
}

pub(crate) fn dec_pts(token: &str) -> Vec<Pt> {
    if token == "-" {
        return Vec::new();
    }
    token.split(';').map(dec_pair).collect()
}

pub(crate) fn dec_segs(token: &str) -> Vec<Seg> {
    if token == "-" {
        return Vec::new();
    }
    token
        .split(';')
        .map(|s| {
            let v: Vec<f64> = s.split(',').map(unhex).collect();
            Seg {
                point: [v[0], v[1]],
                handle_in: [v[2], v[3]],
                handle_out: [v[4], v[5]],
            }
        })
        .collect()
}

pub(crate) fn enc_pts(points: &[Pt]) -> String {
    if points.is_empty() {
        return "-".to_string();
    }
    points
        .iter()
        .map(|p| format!("{},{}", hex(p[0]), hex(p[1])))
        .collect::<Vec<String>>()
        .join(";")
}

pub(crate) fn enc_segs(segs: &[Seg]) -> String {
    if segs.is_empty() {
        return "-".to_string();
    }
    let one = |s: &Seg| {
        [
            s.point[0],
            s.point[1],
            s.handle_in[0],
            s.handle_in[1],
            s.handle_out[0],
            s.handle_out[1],
        ]
        .iter()
        .map(|v| hex(*v))
        .collect::<Vec<String>>()
        .join(",")
    };
    segs.iter().map(one).collect::<Vec<String>>().join(";")
}

pub(crate) fn dec_rect(token: &str) -> Bounds {
    let v: Vec<f64> = token.split(',').map(unhex).collect();
    Bounds {
        x: v[0],
        y: v[1],
        width: v[2],
        height: v[3],
    }
}

pub(crate) fn dec_style(token: &str) -> LeafStyle {
    let f: Vec<&str> = token.split(',').collect();
    let paint = |t: &str| if t == "~" { None } else { Some(unescape(t)) };
    LeafStyle {
        fill: paint(f[0]),
        stroke: paint(f[1]),
        stroke_width: unhex(f[2]),
        opacity: unhex(f[3]),
    }
}

pub(crate) fn enc_style(style: &LeafStyle) -> String {
    let paint = |p: &Option<String>| match p {
        None => "~".to_string(),
        Some(text) => escape(text),
    };
    format!(
        "{},{},{},{}",
        paint(&style.fill),
        paint(&style.stroke),
        hex(style.stroke_width),
        hex(style.opacity)
    )
}

fn flag(token: &str) -> bool {
    assert!(token == "0" || token == "1", "flag token {}", token);
    token == "1"
}

/// A leaf in the `E` grammar: `P <closed> <segs> <style>` or `C <style> <child>...`.
pub(crate) fn dec_leaf(fields: &[&str]) -> ImportLeaf {
    match fields[0] {
        "P" => ImportLeaf::Path {
            contour: Contour {
                segments: dec_segs(fields[2]),
                closed: flag(fields[1]),
            },
            style: dec_style(fields[3]),
        },
        "C" => ImportLeaf::Compound {
            style: dec_style(fields[1]),
            children: fields[2..]
                .iter()
                .map(|child| {
                    let p: Vec<&str> = child.split(':').collect();
                    CompoundChild {
                        contour: Contour {
                            segments: dec_segs(p[3]),
                            closed: flag(p[0]),
                        },
                        clockwise: flag(p[1]),
                        bounds: dec_rect(p[2]),
                    }
                })
                .collect(),
        },
        other => panic!("leaf kind {}", other),
    }
}

pub(crate) fn enc_rect(b: &Bounds) -> String {
    [b.x, b.y, b.width, b.height]
        .iter()
        .map(|v| hex(*v))
        .collect::<Vec<String>>()
        .join(",")
}

/// The `E`-grammar tokens of a leaf (the inverse of [`dec_leaf`]), space separated.
pub(crate) fn enc_leaf(leaf: &ImportLeaf) -> String {
    match leaf {
        ImportLeaf::Path { contour, style } => format!(
            "P {} {} {}",
            if contour.closed { "1" } else { "0" },
            enc_segs(&contour.segments),
            enc_style(style)
        ),
        ImportLeaf::Compound { children, style } => {
            let mut parts: Vec<String> = vec!["C".to_string(), enc_style(style)];
            for c in children {
                parts.push(format!(
                    "{}:{}:{}:{}",
                    if c.contour.closed { "1" } else { "0" },
                    if c.clockwise { "1" } else { "0" },
                    enc_rect(&c.bounds),
                    enc_segs(&c.contour.segments)
                ));
            }
            parts.join(" ")
        }
    }
}

/// True when `line` is exactly one line of the oracle.
pub(crate) fn oracle_has(line: &str) -> bool {
    ORACLE.lines().any(|l| l == line)
}

/// One exploded path in the oracle's output token (`origin:` prefix only for `E` rows).
pub(crate) fn enc_out(path: &ExplodedPath, with_origin: bool) -> String {
    let origin = match &path.origin {
        PathOrigin::Passthrough => "P".to_string(),
        PathOrigin::Island { child } => format!("I{}", child),
        PathOrigin::Merged { .. } => "N".to_string(),
    };
    let body = format!(
        "{}:{}:{}",
        if path.contour.closed { "1" } else { "0" },
        enc_segs(&path.contour.segments),
        enc_style(&path.style)
    );
    if with_origin {
        format!("{}:{}", origin, body)
    } else {
        body
    }
}

pub(crate) fn enc_outs(paths: &[ExplodedPath], with_origin: bool) -> String {
    if paths.is_empty() {
        return "none".to_string();
    }
    paths
        .iter()
        .map(|p| enc_out(p, with_origin))
        .collect::<Vec<String>>()
        .join(" ")
}

/// The four kernel functions a row check runs through (production or a deliberate mutant).
pub(crate) struct Impl<'a> {
    pub(crate) closest: &'a dyn Fn(&[Pt], &[Pt]) -> (usize, usize),
    pub(crate) pick: &'a dyn Fn(HolePairing, &[Seg], &[Seg]) -> (usize, usize),
    pub(crate) merge: &'a dyn Fn(&[Seg], &[Seg], (usize, usize)) -> Option<Vec<Seg>>,
    pub(crate) explode: &'a dyn Fn(&ImportLeaf) -> Result<ExplodedLeaf, ExplodeError>,
}

pub(crate) fn production() -> Impl<'static> {
    Impl {
        closest: &super::closest_pair,
        pick: &super::pick_pair,
        merge: &super::merge_hole_into_exterior,
        explode: &super::explode_leaf,
    }
}

fn merge_row(imp: &Impl, fields: &[&str], arrays: bool) -> String {
    let ext = dec_segs(fields[0]);
    let hole = dec_segs(fields[1]);
    let pairing = if arrays {
        HolePairing::BaselineArrayPoints
    } else {
        HolePairing::ClosestPoints
    };
    let pair = (imp.pick)(pairing, &ext[..], &hole[..]);
    match (imp.merge)(&ext[..], &hole[..], pair) {
        Some(out) => enc_segs(&out),
        None => "invalid".to_string(),
    }
}

/// The expected result field (after `=>`) of a row, rejoined.
fn expected(fields: &[&str]) -> String {
    let at = fields.iter().position(|f| *f == "=>").expect("=>");
    fields[at + 1..].join(" ")
}

/// Rows of `kind` (`P`, `Mo`, `Ma`, `E`) whose result differs from the oracle: (rows, bad, first bad row).
pub(crate) fn check(kind: &str, imp: &Impl) -> (usize, usize, Option<String>) {
    let rows = oracle_rows(kind);
    let mut bad = 0;
    let mut first: Option<String> = None;
    for fields in &rows {
        let want = expected(fields);
        let got = match kind {
            "P" => {
                let a = dec_pts(fields[0]);
                let b = dec_pts(fields[1]);
                let (i, j) = (imp.closest)(&a[..], &b[..]);
                format!("{} {}", i, j)
            }
            "Mo" => merge_row(imp, fields, false),
            "Ma" => merge_row(imp, fields, true),
            "E" => {
                let at = fields.iter().position(|f| *f == "=>").expect("=>");
                let leaf = dec_leaf(&fields[..at]);
                match (imp.explode)(&leaf) {
                    Ok(out) => enc_outs(&out.paths, true),
                    Err(_) => "throws".to_string(),
                }
            }
            other => panic!("kind {}", other),
        };
        if got != want {
            bad += 1;
            if first.is_none() {
                first = Some(format!(
                    "{} {} => want {} got {}",
                    kind,
                    fields[..fields.len().min(3)].join(" "),
                    want,
                    got
                ));
            }
        }
    }
    (rows.len(), bad, first)
}
