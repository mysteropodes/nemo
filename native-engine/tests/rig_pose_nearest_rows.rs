//! Oracle row parsing and comparison harness for the Paper nearest-location kernel and its rig callers
//! (L03-I, I12). Production, the flawed copies in `flawed` and the primitive runner in `prim_rows` all
//! go through the same row check, [`same_row`].
//!
//! `fixtures/rig-pose-nearest/oracle.txt` (`ORACLE`) is produced by `tools/gen.cjs` (kept outside the repo;
//! sha256 in the leaf HANDOFF) from the baseline at fbe4b8a3: `_boneSegsToPath`, `rigWeighOnePoint`,
//! `ensureLayerRig`, `isSelectablePathChild` and `rigAutoAssignLayer` (`src/js/app.js`) are cut VERBATIM and
//! executed in a Node `vm` with the SHIPPED Paper.js 0.12.18 (`src/paper-full.min.js`) and the real
//! `image-mesh.js` `worldVerts`; the primitive rows call Paper's own static API (`Curve.getLength`,
//! `Curve.getPoint`, `Curve.isStraight`, `Curve.getTimeOf`, `Curve.getNearestTime`, `Numerical.solveQuadratic`,
//! `Numerical.solveCubic`). No golden suite existed for this code; this oracle IS the executed baseline.
//!
//! Tokens (space separated, f64 as 16 hex digits): POINT `hx,hy`; SEG `POINT[~POINT][^POINT]` (`~` handleIn,
//! `^` handleOut); SEGLIST `!` (absent), `_` (empty) or `SEG;SEG`; VALUES eight hex joined by `,`; ROOTS three
//! of hex or `u` (undefined); BONE `id/closed/radius/SEGLIST` (`radius` `-` absent); BONES `BONE|BONE` or `-`.
//! The second token of every row is the TOLERANCE flag: `x` bit exact (`-0` included, NaN equal to NaN), `u`
//! the numbers may differ by [`TOL_REL`] of `max(|value|, row scale)` (the row reaches `Numerical.solveCubic`,
//! whose Newton start uses `Math.pow(|t|, 1/3)`; `tools/gen.cjs` perturbs that start and measures a deviation
//! of about 1e-17 of the scale, 4 orders below the bound). Rows: `nst` (this file), `str len pnt tmo nti slq
//! slc` (`prim_rows`), `wgh rad abo` (the weigh tests); the grammar of each is in `tools/rows.cjs`.

use super::{NearestError, NearestLocation};
use crate::evaluation::object_image_mesh_kernels::rig_pose_nearest_weigh::NearestBone;
use crate::evaluation::property_follow_path_sample::SegmentIn;

pub(crate) const ORACLE: &str = include_str!("fixtures/rig-pose-nearest/oracle.txt");
/// Keep in step with `TOL_REL` of `tools/gen.cjs`.
pub(crate) const TOL_REL: f64 = 1e-13;

pub(crate) fn hex(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits"))
}

/// 16 hex digits of the bits; every NaN is the canonical quiet NaN (JavaScript has no payloads).
pub(crate) fn fmt(x: f64) -> String {
    let bits = if x.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        x.to_bits()
    };
    format!("{bits:016x}")
}

pub(crate) fn point(tok: &str) -> [f64; 2] {
    let (x, y) = tok.split_once(',').expect("point");
    [hex(x), hex(y)]
}

pub(crate) fn fmt_point(p: [f64; 2]) -> String {
    format!("{},{}", fmt(p[0]), fmt(p[1]))
}

pub(crate) fn segment(tok: &str) -> SegmentIn {
    let (rest, handle_out) = match tok.split_once('^') {
        Some((a, b)) => (a, Some(point(b))),
        None => (tok, None),
    };
    let (head, handle_in) = match rest.split_once('~') {
        Some((a, b)) => (a, Some(point(b))),
        None => (rest, None),
    };
    SegmentIn {
        point: point(head),
        handle_in,
        handle_out,
    }
}

/// `!` is `None` (an absent property), `_` an empty list.
pub(crate) fn seglist(tok: &str) -> Option<Vec<SegmentIn>> {
    match tok {
        "!" => None,
        "_" => Some(Vec::new()),
        _ => Some(tok.split(';').map(segment).collect()),
    }
}

pub(crate) fn values(tok: &str) -> [f64; 8] {
    let v: Vec<f64> = tok.split(',').map(hex).collect();
    v.try_into().expect("eight values")
}

/// `u` (undefined) and `n` (null) are `None`.
pub(crate) fn opt(tok: &str) -> Option<f64> {
    match tok {
        "u" | "n" => None,
        _ => Some(hex(tok)),
    }
}

pub(crate) fn bone(tok: &str) -> NearestBone {
    let f: Vec<&str> = tok.split('/').collect();
    NearestBone {
        id: f[0].to_string(),
        closed: f[1] == "1",
        radius: if f[2] == "-" { None } else { Some(hex(f[2])) },
        rest_segments: seglist(f[3]),
    }
}

pub(crate) fn bones(tok: &str) -> Vec<NearestBone> {
    if tok == "-" {
        return Vec::new();
    }
    tok.split('|').map(bone).collect()
}

/// Splits a row into its skeleton (every 16-hex run replaced by `#`) and those numbers.
fn skeleton(row: &str) -> (String, Vec<f64>) {
    let mut skel = String::new();
    let mut nums = Vec::new();
    let mut run = String::new();
    let flush = |run: &mut String, skel: &mut String, nums: &mut Vec<f64>| {
        if run.len() == 16 {
            nums.push(hex(run));
            skel.push('#');
        } else {
            skel.push_str(run);
        }
        run.clear();
    };
    for c in row.chars() {
        if c.is_ascii_digit() || ('a'..='f').contains(&c) {
            run.push(c);
        } else {
            flush(&mut run, &mut skel, &mut nums);
            skel.push(c);
        }
    }
    flush(&mut run, &mut skel, &mut nums);
    (skel, nums)
}

fn deviation_ok(x: f64, y: f64, scale: f64) -> bool {
    if x == y || (x.is_nan() && y.is_nan()) {
        return true;
    }
    if !x.is_finite() || !y.is_finite() {
        return false;
    }
    (x - y).abs() / x.abs().max(y.abs()).max(scale) <= TOL_REL
}

/// The row comparison of `tools/gen.cjs` (`rowDeviation`): flag `x` is textual equality; flag `u` needs the
/// same skeleton and every number within [`TOL_REL`] of `max(|value|, scale)`, the scale being the largest
/// finite magnitude among the oracle row's numbers.
pub(crate) fn same_row(want: &str, got: &str) -> bool {
    if want == got {
        return true;
    }
    if want.split(' ').nth(1) != Some("u") {
        return false;
    }
    let ((ws, wn), (gs, gn)) = (skeleton(want), skeleton(got));
    if ws != gs || wn.len() != gn.len() {
        return false;
    }
    let scale = wn
        .iter()
        .filter(|x| x.is_finite())
        .fold(0.0_f64, |m, x| m.max(x.abs()));
    wn.iter().zip(&gn).all(|(&x, &y)| deviation_ok(x, y, scale))
}

/// The row without its last token, plus `out`.
pub(crate) fn with_out(row: &str, out: &str) -> String {
    let cut = row.rfind(' ').expect("row has an out token");
    format!("{} {out}", &row[..cut])
}

/// `kind: row N: want ... got ...` for one failed row (`controls` reads the kind from the first word).
pub(crate) fn failure(n: usize, row: &str, got: &str) -> String {
    let kind = row.split(' ').next().unwrap_or("");
    format!("{kind}: row {n}: want {row} got {got}")
}

/// The kernel operation the `nst` rows exercise; `flawed` implements it with one flaw at a time.
pub(crate) trait Ops {
    fn nearest(
        &self,
        segs: &[SegmentIn],
        closed: bool,
        p: [f64; 2],
    ) -> Result<Option<NearestLocation>, NearestError>;
}

pub(crate) struct Production;

impl Ops for Production {
    fn nearest(
        &self,
        segs: &[SegmentIn],
        closed: bool,
        p: [f64; 2],
    ) -> Result<Option<NearestLocation>, NearestError> {
        super::nearest_location(segs, closed, p)
    }
}

pub(crate) fn nst_out(r: Result<Option<NearestLocation>, NearestError>) -> String {
    match r {
        Ok(None) => "null".to_string(),
        Err(_) => "unres".to_string(),
        Ok(Some(l)) => format!(
            "{}/{}/{}/{}/{}",
            fmt_point(l.point),
            fmt(l.offset),
            fmt(l.distance),
            l.curve_index,
            fmt(l.time)
        ),
    }
}

/// Rows of one kind with their 1-based line numbers.
pub(crate) fn rows_of(kind: &'static str) -> impl Iterator<Item = (usize, &'static str)> {
    ORACLE
        .lines()
        .enumerate()
        .filter(move |(_, r)| r.split(' ').next() == Some(kind))
        .map(|(i, r)| (i + 1, r))
}

/// Every `nst` row that `ops` does not reproduce.
pub(crate) fn failures(ops: &dyn Ops) -> Vec<String> {
    let mut bad = Vec::new();
    for (n, row) in rows_of("nst") {
        let t: Vec<&str> = row.split(' ').collect();
        let segs = seglist(t[3]).expect("segments");
        let got = with_out(row, &nst_out(ops.nearest(&segs, t[2] == "1", point(t[4]))));
        if !same_row(row, &got) {
            bad.push(failure(n, row, &got));
        }
    }
    bad
}
