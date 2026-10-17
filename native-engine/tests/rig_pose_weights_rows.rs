//! Oracle row parsing and comparison harness for the rig weights and anchors kernel (L03-D, I12).
//! Production and the flawed copies in `_controls` run through the same row checks via [`Ops`].
//!
//! `fixtures/rig-pose-weights/oracle.txt` (`ORACLE`) is produced by `tools/gen.cjs` (kept outside
//! the repo; sha256 recorded in the leaf HANDOFF) from the baseline at fbe4b8a3:
//! `rigWeighOnePointByControl`, `_rigChordAngle`, `_rigNormalizeAngle` and the nested `anchorFor`
//! (`src/js/app.js`) are cut VERBATIM and executed in a Node `vm` with the real vendored Paper.js
//! `Point`; three statements of `rigBindStroke` are transcribed (each checked to occur verbatim in
//! it). There is no existing golden suite for this code; this oracle is the executed baseline.
//!
//! Tokens (space separated rows, f64 as 16 hex digits). A POINT is `hx,hy`; a SEGLIST is `!` (the
//! property is absent), `_` (empty) or `S;S;...` with `S` a POINT, `?` (segment without `.point`) or
//! `N` (`point: null`); a BONE is `id/radius/live/rest` (`radius` `-` absent); a RIG is
//! `BONE|BONE|...` in listed order. Rows:
//! `nrm A OUT`, `chd SEGLIST K OUT` (`OUT` hex or `throw`), `wgt RIG IDS P RADIUS SOFT OUT`
//! (`IDS` `-` empty, else `id,id`; `OUT` `_` or `id:k:hexw;...`), `bnd RIG IDS PTS RADIUS SOFT OUT`
//! (`PTS` `_` or `P;P`; `RADIUS`/`SOFT` `u` undefined, `n` null or hex; `OUT` `-` or one `wgt`
//! style list per point joined by `|`), `bpr RADIUS SOFT OUT` (`OUT` `hexr,hexs`),
//! `anc RIG BONE:PT:OFF:W OUT` (`PT` `u`, `n` or an integer; `OFF` `u`, `n` or hex; `OUT` `null`,
//! `off`, `throw` or `ctl:rx,ry,cx,cy,da`).
//!
//! Tolerance: everything is compared bit for bit (`-0` included, NaN equal to NaN) except values
//! that pass through `atan2` (`chd` results and `da`), which may differ by `TRIG_ULPS` units of
//! roundoff of the scale (macOS libm differs from V8 by an ulp). `%` is `fmod` on both sides, so
//! `nrm` is bit-exact.

use super::{
    anchor_for, bind_radius, chord_angle, clamp_softness, normalize_angle, weigh_bind_points,
    weigh_one_point_by_control, Anchor, AnchorError, ChordError, ControlWeight, Point2, WeightBone,
    WeightEntry, WeightParts,
};

pub(crate) const ORACLE: &str = include_str!("fixtures/rig-pose-weights/oracle.txt");
const TRIG_ULPS: f64 = 32.0;

/// The kernel operations the rows exercise; `_controls` implements it with one flaw at a time.
pub(crate) trait Ops {
    fn norm(&self, a: f64) -> f64;
    fn chord(&self, segs: &[Option<Point2>], k: usize) -> Result<f64, ChordError>;
    fn weigh(
        &self,
        bones: &[WeightBone],
        ids: &[String],
        p: Point2,
        radius: f64,
        softness: f64,
    ) -> Vec<ControlWeight>;
    fn bind(
        &self,
        bones: &[WeightBone],
        ids: &[String],
        pts: &[Point2],
        radius: Option<f64>,
        softness: Option<f64>,
    ) -> Vec<Vec<ControlWeight>>;
    fn params(&self, radius: Option<f64>, softness: Option<f64>) -> (f64, f64);
    fn anchor(&self, bones: &[WeightBone], entry: &WeightEntry) -> Result<Anchor, AnchorError>;
    /// The shape discriminator; production is [`WeightEntry::classify`].
    fn classify(&self, parts: WeightParts) -> WeightEntry {
        WeightEntry::classify(parts)
    }
}

pub(crate) struct Production;

impl Ops for Production {
    fn norm(&self, a: f64) -> f64 {
        normalize_angle(a)
    }
    fn chord(&self, segs: &[Option<Point2>], k: usize) -> Result<f64, ChordError> {
        chord_angle(segs, k)
    }
    fn weigh(
        &self,
        bones: &[WeightBone],
        ids: &[String],
        p: Point2,
        radius: f64,
        softness: f64,
    ) -> Vec<ControlWeight> {
        weigh_one_point_by_control(bones, ids, p, radius, softness)
    }
    fn bind(
        &self,
        bones: &[WeightBone],
        ids: &[String],
        pts: &[Point2],
        radius: Option<f64>,
        softness: Option<f64>,
    ) -> Vec<Vec<ControlWeight>> {
        weigh_bind_points(bones, ids, pts, radius, softness)
    }
    fn params(&self, radius: Option<f64>, softness: Option<f64>) -> (f64, f64) {
        (bind_radius(radius), clamp_softness(softness))
    }
    fn anchor(&self, bones: &[WeightBone], entry: &WeightEntry) -> Result<Anchor, AnchorError> {
        anchor_for(bones, entry)
    }
}

pub(crate) fn hex(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits"))
}

pub(crate) fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

/// Bit-equal, or within `TRIG_ULPS` of `scale` (at least 1) and of the values themselves.
pub(crate) fn near(x: f64, y: f64, scale: f64) -> bool {
    same(x, y)
        || (x.is_finite()
            && y.is_finite()
            && (x - y).abs() <= TRIG_ULPS * f64::EPSILON * scale.max(1.0).max(x.abs()).max(y.abs()))
}

fn point(tok: &str) -> Point2 {
    let (x, y) = tok.split_once(',').expect("point");
    [hex(x), hex(y)]
}

fn seglist(tok: &str) -> Option<Vec<Option<Point2>>> {
    match tok {
        "!" => None,
        "_" => Some(Vec::new()),
        _ => Some(
            tok.split(';')
                .map(|s| {
                    if s == "?" || s == "N" {
                        None
                    } else {
                        Some(point(s))
                    }
                })
                .collect(),
        ),
    }
}

pub(crate) fn parse_rig(tok: &str) -> Vec<WeightBone> {
    if tok == "-" {
        return Vec::new();
    }
    tok.split('|')
        .map(|bone| {
            let f: Vec<&str> = bone.split('/').collect();
            assert_eq!(f.len(), 4, "bone token {bone}");
            WeightBone {
                id: f[0].to_string(),
                radius: if f[1] == "-" { None } else { Some(hex(f[1])) },
                segments: seglist(f[2]),
                rest_segments: seglist(f[3]),
            }
        })
        .collect()
}

fn ids(tok: &str) -> Vec<String> {
    if tok == "-" {
        Vec::new()
    } else {
        tok.split(',').map(str::to_string).collect()
    }
}

/// `u` (undefined) and `n` (null) are both `None`: the baseline does not tell them apart.
fn opt(tok: &str) -> Option<f64> {
    if tok == "u" || tok == "n" {
        None
    } else {
        Some(hex(tok))
    }
}

fn wtok(list: &[ControlWeight]) -> String {
    if list.is_empty() {
        return "_".to_string();
    }
    let parts: Vec<String> = list
        .iter()
        .map(|c| {
            assert!(c.stray_offset.is_none(), "the weigher emits no offset");
            format!("{}:{}:{:016x}", c.bone_id, c.pt_index, c.w.to_bits())
        })
        .collect();
    parts.join(";")
}

fn expect(what: &str, got: &str, want: &str) -> Result<(), String> {
    if got == want {
        Ok(())
    } else {
        Err(format!("{what}: got {got} want {want}"))
    }
}

fn hex_pair(a: f64, b: f64) -> String {
    format!("{:016x},{:016x}", a.to_bits(), b.to_bits())
}

/// Parse and check one row against `ops`; `Err` names the first mismatch.
pub(crate) fn run(ops: &dyn Ops, row: &str) -> Result<(), String> {
    let f: Vec<&str> = row.split(' ').collect();
    match f[0] {
        "nrm" => {
            let got = ops.norm(hex(f[1]));
            if same(got, hex(f[2])) {
                Ok(())
            } else {
                Err(format!("nrm {row}: got {got:?}"))
            }
        }
        "chd" => {
            let segs = seglist(f[1]).expect("chd list");
            let got = ops.chord(&segs, f[2].parse().expect("k"));
            match (got, f[3]) {
                (Err(_), "throw") => Ok(()),
                (Ok(v), out) if out != "throw" => {
                    if near(v, hex(out), std::f64::consts::PI) {
                        Ok(())
                    } else {
                        Err(format!("chd {row}: got {v:?}"))
                    }
                }
                (g, out) => Err(format!("chd: got {g:?} want {out}")),
            }
        }
        "wgt" => {
            let got = ops.weigh(
                &parse_rig(f[1]),
                &ids(f[2]),
                point(f[3]),
                hex(f[4]),
                hex(f[5]),
            );
            expect("wgt", &wtok(&got), f[6])
        }
        "bnd" => {
            let pts: Vec<Point2> = if f[3] == "_" {
                Vec::new()
            } else {
                f[3].split(';').map(point).collect()
            };
            let got = ops.bind(&parse_rig(f[1]), &ids(f[2]), &pts, opt(f[4]), opt(f[5]));
            let text = if got.is_empty() {
                "-".to_string()
            } else {
                got.iter().map(|l| wtok(l)).collect::<Vec<_>>().join("|")
            };
            expect("bnd", &text, f[6])
        }
        "bpr" => {
            let (r, s) = ops.params(opt(f[1]), opt(f[2]));
            expect("bpr", &hex_pair(r, s), f[3])
        }
        "anc" => run_anchor(ops, &f),
        other => Err(format!("unknown row kind {other}")),
    }
}

fn run_anchor(ops: &dyn Ops, f: &[&str]) -> Result<(), String> {
    let e: Vec<&str> = f[2].split(':').collect();
    assert_eq!(e.len(), 4, "entry token {}", f[2]);
    let parts = WeightParts {
        bone_id: e[0].to_string(),
        pt_index: if e[1] == "u" || e[1] == "n" {
            None
        } else {
            Some(e[1].parse().expect("ptIndex"))
        },
        offset: opt(e[2]),
        w: hex(e[3]),
    };
    let got = ops.anchor(&parse_rig(f[1]), &ops.classify(parts));
    match (got, f[3]) {
        (Ok(Anchor::Unresolved), "null")
        | (Ok(Anchor::OffsetModel { .. }), "off")
        | (Err(_), "throw") => Ok(()),
        (Ok(Anchor::Control(c)), out) if out.starts_with("ctl:") => {
            let v: Vec<f64> = out[4..].split(',').map(hex).collect();
            let exact = same(c.rest_x, v[0])
                && same(c.rest_y, v[1])
                && same(c.cur_x, v[2])
                && same(c.cur_y, v[3]);
            if exact && near(c.da, v[4], std::f64::consts::PI) {
                Ok(())
            } else {
                Err(format!("anc ctl: got {c:?} want {out}"))
            }
        }
        (g, out) => Err(format!("anc: got {g:?} want {out}")),
    }
}

pub(crate) fn rows() -> Vec<&'static str> {
    ORACLE.lines().collect()
}

/// Every row that `ops` fails, as `"<index> <message>"`.
pub(crate) fn failures(ops: &dyn Ops) -> Vec<String> {
    rows()
        .iter()
        .enumerate()
        .filter_map(|(i, row)| run(ops, row).err().map(|m| format!("{i} {m}")))
        .collect()
}

/// Row counts by kind: nrm, chd, wgt, bnd, bpr, anc.
pub(crate) fn counts() -> [usize; 6] {
    let mut n = [0usize; 6];
    for row in rows() {
        let slot = ["nrm", "chd", "wgt", "bnd", "bpr", "anc"]
            .iter()
            .position(|k| row.starts_with(&format!("{k} ")))
            .expect("row kind");
        n[slot] += 1;
    }
    n
}
