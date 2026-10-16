//! Oracle row parsing and comparison harness for the two-bone IK kernel (L03-C, I12).
//! Production and the mutants in `_controls` run through the same row checks.
//!
//! `fixtures/rig-ik-solve/oracle.txt` (`ORACLE`) is produced by `tools/gen.cjs` (kept outside
//! the repo; sha256 recorded in the leaf HANDOFF) from the baseline at fbe4b8a3: `ensureLayerRig`,
//! `rigAnchorPoint`, `rigSetIK`, `rigDragIKEnd` (`src/js/app.js`) and `otherEnd`, `findIKChain`
//! plus the three Alt-drag statements (`src/js/rig-bridge.js`) are cut VERBATIM and executed in a
//! Node `vm` with the real vendored Paper.js `Point`, `applyRigDeform` counted and
//! `n20AllowLegacyWrite` admitting. There is no existing golden suite for the IK code; this
//! oracle is the executed baseline.
//!
//! Tokens (space separated rows, f64 as 16 hex digits). A POINT is `hx,hy`; a SEGLIST is
//! `P;P;...` or `_` (empty); a BONE is `id/closed/segments/rest` (`id` `~` is the empty string);
//! a RIG is `BONE|BONE|...` in `Object.keys` order, `-` for no bones; a REF is `id,vi`; a
//! CHAIN is `REF+REF+REF+l1+l2+flip` (root, joint, end). Rows:
//! `anc RIG REF out` (`out` `-` null or a POINT), `set RIG ROOT JOINT END flip out` (`out` `-`
//! false or `l1,l2,flip,endId:endVi`), `drg RIG CHAIN TARGET out POST` (`out` `ok:J:E` or
//! `throw:partial`, `POST` the live positions of every bone after the call), `fnd RIG id vi out`
//! (`out` `-` or `REF+REF+REF+REF` = root, joint, endNear, end) and `flw RIG id vi TARGETS out`
//! (`TARGETS` `P+P+...`, `out` `-` no chain, `x` rigSetIK refused, else one POST per step).
//!
//! Tolerance: everything is compared bit for bit (`-0` included, NaN equal to NaN) except the
//! joint/end positions of `drg`/`flw`, which pass through `atan2`/`acos`/`cos`/`sin` and may
//! differ by `TRIG_ULPS` units of roundoff of the group's scale (macOS libm differs from V8
//! by an ulp). The bound is generous on purpose: the angle accumulates `atan2`, `acos` and a sum
//! (about 3 ulps of pi each way, twice for two libms) and `l1` scales it, so 32 ulps of
//! `max(1, |coordinate|, l1, l2)` stays above 2e-15 relative while every flaw moves some value by
//! far more. `hypot` is not left to libm: [`super::hypot2`] is V8's algorithm.

use super::{IkBone, IkChain, IkChainSpec, IkError, IkPose, IkRef, Point2, PointWrite};

pub(crate) const ORACLE: &str = include_str!("fixtures/rig-ik-solve/oracle.txt");
const TRIG_ULPS: f64 = 32.0;

pub(crate) fn hex(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits"))
}

pub(crate) fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

pub(crate) fn same_pt(a: Point2, b: Point2) -> bool {
    same(a[0], b[0]) && same(a[1], b[1])
}

/// Bit-equal, or within `TRIG_ULPS` of `scale` (at least 1) and of the values themselves.
pub(crate) fn near(a: Point2, b: Point2, scale: f64) -> bool {
    a.iter().zip(b.iter()).all(|(x, y)| {
        same(*x, *y)
            || (x.is_finite()
                && y.is_finite()
                && (x - y).abs()
                    <= TRIG_ULPS * f64::EPSILON * scale.max(1.0).max(x.abs()).max(y.abs()))
    })
}

pub(crate) fn point(tok: &str) -> Point2 {
    let (x, y) = tok.split_once(',').expect("point");
    [hex(x), hex(y)]
}

pub(crate) fn id(tok: &str) -> String {
    if tok == "~" {
        String::new()
    } else {
        tok.to_string()
    }
}

fn seglist(tok: &str) -> Vec<Point2> {
    if tok == "_" {
        Vec::new()
    } else {
        tok.split(';').map(point).collect()
    }
}

pub(crate) fn parse_rig(tok: &str) -> Vec<IkBone> {
    if tok == "-" {
        return Vec::new();
    }
    tok.split('|')
        .map(|bone| {
            let f: Vec<&str> = bone.split('/').collect();
            assert_eq!(f.len(), 4, "bone token {bone}");
            IkBone {
                id: id(f[0]),
                closed: f[1] == "1",
                segments: seglist(f[2]),
                rest_segments: seglist(f[3]),
            }
        })
        .collect()
}

pub(crate) fn iref(tok: &str) -> IkRef {
    let (bone, vi) = tok.split_once(',').expect("ref");
    IkRef {
        bone_id: id(bone),
        vi: vi.parse().expect("vi"),
    }
}

pub(crate) fn chain(tok: &str) -> IkChain {
    let f: Vec<&str> = tok.split('+').collect();
    assert_eq!(f.len(), 6, "chain token {tok}");
    IkChain {
        root: iref(f[0]),
        joint: iref(f[1]),
        end: iref(f[2]),
        l1: hex(f[3]),
        l2: hex(f[4]),
        flip: f[5] == "1",
    }
}

fn spec(tok: &str) -> IkChainSpec {
    let f: Vec<&str> = tok.split('+').collect();
    assert_eq!(f.len(), 4, "spec token {tok}");
    IkChainSpec {
        root: iref(f[0]),
        joint: iref(f[1]),
        end_near: iref(f[2]),
        end: iref(f[3]),
    }
}

/// The rows of one op, each split into its space separated tokens.
pub(crate) fn rows(op: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|l| l.split(' ').next() == Some(op))
        .map(|l| l.split(' ').collect())
        .collect()
}

/// A bone whose live and rest vertices are the same points.
pub(crate) fn bone(id: &str, pts: &[Point2]) -> IkBone {
    IkBone {
        id: id.to_string(),
        closed: false,
        segments: pts.to_vec(),
        rest_segments: pts.to_vec(),
    }
}

/// `{boneId, vi}` from literals.
pub(crate) fn r(id: &str, vi: usize) -> IkRef {
    IkRef {
        bone_id: id.to_string(),
        vi,
    }
}

pub(crate) fn live(bones: &[IkBone], r: &IkRef) -> Option<Point2> {
    bones
        .iter()
        .find(|b| b.id == r.bone_id)?
        .segments
        .get(r.vi)
        .copied()
}

/// Largest magnitude of every live and rest coordinate, at least 1.
pub(crate) fn scale_of(bones: &[IkBone], extra: &[f64]) -> f64 {
    let all = bones
        .iter()
        .flat_map(|b| b.segments.iter().chain(b.rest_segments.iter()));
    all.flat_map(|p| p.iter().copied())
        .chain(extra.iter().copied().filter(|v| v.is_finite()))
        .fold(1.0_f64, |m, v| m.max(v.abs()))
}

/// Applies the writes to the working bones (the consumer step the kernel only prepares).
pub(crate) fn apply(bones: &mut [IkBone], writes: &[PointWrite]) {
    for w in writes {
        let bone = bones
            .iter_mut()
            .find(|b| b.id == w.target.bone_id)
            .expect("bone");
        bone.segments[w.target.vi] = w.point;
    }
}

/// True when the live positions of `bones` equal the POST token (`near` within `scale`).
pub(crate) fn post_matches(post: &str, bones: &[IkBone], scale: f64) -> bool {
    let expected: Vec<&str> = if post == "-" {
        Vec::new()
    } else {
        post.split('|').collect()
    };
    expected.len() == bones.len()
        && expected.iter().zip(bones).all(|(tok, bone)| {
            let pts = seglist(tok);
            pts.len() == bone.segments.len()
                && pts
                    .iter()
                    .zip(&bone.segments)
                    .all(|(a, b)| near(*a, *b, scale))
        })
}

/// The function table a run goes through: production or a flawed copy from `_controls`.
pub(crate) struct Ops {
    pub(crate) anchor: fn(&[IkBone], &IkRef) -> Option<Point2>,
    pub(crate) set: fn(&[IkBone], &IkRef, &IkRef, &IkRef, bool) -> Result<IkChain, IkError>,
    pub(crate) raw: fn(Point2, f64, f64, bool, Point2) -> IkPose,
    pub(crate) drag: fn(&[IkBone], &IkChain, Point2) -> Result<IkPose, IkError>,
    pub(crate) find: fn(&[IkBone], &str, usize) -> Option<IkChainSpec>,
    pub(crate) writes:
        fn(&[IkBone], &IkChainSpec, &IkChain, Point2) -> Result<Vec<PointWrite>, IkError>,
}

pub(crate) const PRODUCTION: Ops = Ops {
    anchor: super::anchor_point,
    set: super::set_ik,
    raw: super::solve_raw,
    drag: super::drag_ik_end,
    find: super::find_ik_chain,
    writes: super::drag_writes,
};

/// Wrong-row counts per op, plus the baseline facts the tests pin.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Tally {
    pub(crate) anc: usize,
    pub(crate) set: usize,
    pub(crate) raw: usize,
    pub(crate) drag: usize,
    pub(crate) fnd: usize,
    pub(crate) flw: usize,
    /// `drg` rows where the baseline threw / threw after writing the joint / returned a NaN or infinity.
    pub(crate) threw: usize,
    pub(crate) threw_partial: usize,
    pub(crate) non_finite: usize,
}

impl Tally {
    pub(crate) fn wrong(&self) -> usize {
        self.anc + self.set + self.raw + self.drag + self.fnd + self.flw
    }
}

fn anc_wrong(ops: &Ops) -> usize {
    let mut bad = 0;
    for r in rows("anc") {
        let got = (ops.anchor)(&parse_rig(r[1]), &iref(r[2]));
        let ok = match (r[3], got) {
            ("-", None) => true,
            ("-", Some(_)) | (_, None) => false,
            (tok, Some(p)) => same_pt(point(tok), p),
        };
        bad += usize::from(!ok);
    }
    bad
}

fn set_wrong(ops: &Ops) -> usize {
    let mut bad = 0;
    for r in rows("set") {
        let got = (ops.set)(
            &parse_rig(r[1]),
            &iref(r[2]),
            &iref(r[3]),
            &iref(r[4]),
            r[5] == "1",
        );
        let ok = match (r[6], got) {
            ("-", Err(_)) => true,
            ("-", Ok(_)) | (_, Err(_)) => false,
            (tok, Ok(c)) => {
                let f: Vec<&str> = tok.split(',').collect();
                let (end_id, end_vi) = f[3].split_once(':').expect("key");
                same(c.l1, hex(f[0]))
                    && same(c.l2, hex(f[1]))
                    && c.flip == (f[2] == "1")
                    && c.key() == format!("{}:{}", id(end_id), end_vi)
                    && c.root == iref(r[2])
                    && c.joint == iref(r[3])
                    && c.end == iref(r[4])
            }
        };
        bad += usize::from(!ok);
    }
    bad
}

fn drg_run(ops: &Ops, t: &mut Tally) {
    for r in rows("drg") {
        let bones = parse_rig(r[1]);
        let ch = chain(r[2]);
        let target = point(r[3]);
        let typed = (ops.drag)(&bones, &ch, target);
        if let Some(rest) = r[4].strip_prefix("throw:") {
            t.threw += 1;
            t.threw_partial += usize::from(rest == "1");
            t.drag += usize::from(!matches!(typed, Err(IkError::SegmentMissing(_))));
            continue;
        }
        let f: Vec<&str> = r[4]
            .strip_prefix("ok:")
            .expect("ok row")
            .split(':')
            .collect();
        let (joint, end) = (point(f[0]), point(f[1]));
        let root = live(&bones, &ch.root).expect("baseline read the root");
        let scale = scale_of(&bones, &[ch.l1, ch.l2]);
        let raw = (ops.raw)(root, ch.l1, ch.l2, ch.flip, target);
        t.raw += usize::from(!(near(raw.joint, joint, scale) && near(raw.end, end, scale)));
        if joint.iter().chain(end.iter()).any(|v| !v.is_finite()) {
            t.non_finite += 1;
            t.drag += usize::from(typed != Err(IkError::NonFinitePose));
            continue;
        }
        let good = match typed {
            Ok(pose) => {
                let mut after = bones.clone();
                let writes = [
                    PointWrite {
                        target: ch.joint.clone(),
                        point: pose.joint,
                    },
                    PointWrite {
                        target: ch.end.clone(),
                        point: pose.end,
                    },
                ];
                apply(&mut after, &writes);
                near(pose.joint, joint, scale)
                    && near(pose.end, end, scale)
                    && post_matches(r[5], &after, scale)
            }
            Err(_) => false,
        };
        t.drag += usize::from(!good);
    }
}

fn fnd_wrong(ops: &Ops) -> usize {
    let mut bad = 0;
    for r in rows("fnd") {
        let got = (ops.find)(&parse_rig(r[1]), &id(r[2]), r[3].parse().expect("vi"));
        let ok = match (r[4], got) {
            ("-", None) => true,
            ("-", Some(_)) | (_, None) => false,
            (tok, Some(s)) => s == spec(tok),
        };
        bad += usize::from(!ok);
    }
    bad
}

/// The Alt-drag flow: find, rigSetIK (flip false, as the bridge calls it), then one
/// drag step per target with the joint mirrored onto the end bone's near vertex.
fn flw_wrong(ops: &Ops) -> usize {
    let mut bad = 0;
    for r in rows("flw") {
        let mut bones = parse_rig(r[1]);
        let found = (ops.find)(&bones, &id(r[2]), r[3].parse().expect("vi"));
        let Some(sp) = found else {
            bad += usize::from(r[5] != "-");
            continue;
        };
        let Ok(ch) = (ops.set)(&bones, &sp.root, &sp.joint, &sp.end, false) else {
            bad += usize::from(r[5] != "x");
            continue;
        };
        let steps: Vec<&str> = r[5].split('/').collect();
        let targets: Vec<Point2> = r[4].split('+').map(point).collect();
        let mut ok = steps.len() == targets.len();
        for (t, step) in targets.iter().zip(&steps) {
            let scale = scale_of(&bones, &[ch.l1, ch.l2, t[0], t[1]]);
            match (ops.writes)(&bones, &sp, &ch, *t) {
                Ok(writes) => {
                    apply(&mut bones, &writes);
                    ok &= post_matches(step, &bones, scale);
                }
                Err(_) => ok = false,
            }
        }
        bad += usize::from(!ok);
    }
    bad
}

/// Runs every oracle row through `ops`.
pub(crate) fn run(ops: &Ops) -> Tally {
    let mut t = Tally {
        anc: anc_wrong(ops),
        set: set_wrong(ops),
        ..Tally::default()
    };
    drg_run(ops, &mut t);
    t.fnd = fnd_wrong(ops);
    t.flw = flw_wrong(ops);
    t
}
