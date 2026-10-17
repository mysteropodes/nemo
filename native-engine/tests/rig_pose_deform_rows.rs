//! Oracle row reader and runner for the rig deform kernel (L03-E, I12).
//!
//! `fixtures/rig-pose-deform/oracle.txt` (`ORACLE`) is produced by `tools/gen.cjs` (kept outside the
//! repo; sha256 in the leaf HANDOFF) from the baseline at fbe4b8a3: `applyRigDeform` (containing
//! `displaceOne`, `anchorFor` and `bonePaths`) is cut VERBATIM from `src/js/app.js` and executed in
//! a Node `vm` against the real vendored Paper.js `Path`/`Segment`/`Point` and the real
//! `image-mesh.js` `normalizedOf`. No suite existed before; this oracle is the executed baseline.
//!
//! Tokens (space separated; every f64 is 16 hex digits of its bits). POINT `hx,hy`. SEG `POINT[~POINT][^POINT]`
//! (`~` handleIn, `^` handleOut) or `?`/`N` (no usable `.point`). SEGLIST `!` (absent), `_` (empty) or
//! `SEG;SEG`. BONE `id/closed/live/rest`; RIG `BONE|BONE` or `-`. ENTRY `bone:pt:off:w` (`pt` `u`/`n`/integer,
//! `off` `u`/`n`/hex); WLIST `ENTRY;ENTRY` or `_`; WEIGHTS `WLIST|WLIST` or `-`. TOL `-` (bit exact) or the
//! index of the first hex number allowed to differ by libm roundoff (`1e-9` relative).
//! Rows: `pth TOL RIG ROTATE REST RH WEIGHTS LIVE CENTER OUT`, `msh TOL RIG ROTATE VERTS REST WEIGHTS RECT OUT`,
//! `anc TOL RIG ENTRY OUT`, and `mut NAME <row with its baseline OUT> MUTOUT` (the output of a textually
//! mutated baseline, see `_controls`). Other fields: `RH` `!`/`_`/`I/O;I/O` (`-` or POINT); `LIVE` `!`/count;
//! `CENTER` `-` or `P/I/O;..+WEIGHTS+COUNT`; `RECT` `-` or `x,y,w,h,rot`; `OUT` `skip`/`throw`/`null`/`_`/lists.

use super::{
    apply_rig_deform, BindInput, BindPose, CenterBind, CenterRest, DeformFailure, MeshBind,
    MeshTarget, PathBind, RestHandle, SegmentPose,
};
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_geometry::MeshRect;
use crate::evaluation::object_image_mesh_kernels::rig_pose_deform_anchor::{
    AnchorResolver, DeformBone, DeformSegment,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_weights::{
    Point2, WeightEntry, WeightParts,
};

pub(crate) const ORACLE: &str = include_str!("fixtures/rig-pose-deform/oracle.txt");
const REL_TOL: f64 = 1e-9;

fn num(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex f64"))
}
fn hex(x: f64) -> String {
    format!("{:016x}", x.to_bits())
}
fn pt(s: &str) -> Point2 {
    let (x, y) = s.split_once(',').expect("point");
    [num(x), num(y)]
}
fn hp(p: Point2) -> String {
    format!("{},{}", hex(p[0]), hex(p[1]))
}
fn opt_pt(s: &str) -> Option<Point2> {
    (s != "-").then(|| pt(s))
}
fn list<T>(s: &str, f: impl Fn(&str) -> T) -> Vec<T> {
    if s == "_" || s == "-" {
        Vec::new()
    } else {
        s.split(';').map(f).collect()
    }
}

fn seg(tok: &str) -> DeformSegment {
    let none = DeformSegment {
        point: None,
        handle_in: None,
        handle_out: None,
    };
    if tok == "?" || tok == "N" {
        return none;
    }
    let (head, ho) = match tok.split_once('^') {
        Some((h, o)) => (h, Some(pt(o))),
        None => (tok, None),
    };
    let (p, hi) = match head.split_once('~') {
        Some((p, i)) => (p, Some(pt(i))),
        None => (head, None),
    };
    DeformSegment {
        point: Some(pt(p)),
        handle_in: hi,
        handle_out: ho,
    }
}
fn seglist(s: &str) -> Option<Vec<DeformSegment>> {
    (s != "!").then(|| list(s, seg))
}
fn bones(s: &str) -> Vec<DeformBone> {
    if s == "-" {
        return Vec::new();
    }
    s.split('|')
        .map(|b| {
            let f: Vec<&str> = b.split('/').collect();
            DeformBone {
                id: f[0].to_string(),
                closed: f[1] == "1",
                segments: seglist(f[2]),
                rest_segments: seglist(f[3]),
            }
        })
        .collect()
}
fn optional_num(s: &str) -> Option<f64> {
    (s != "u" && s != "n").then(|| num(s))
}
fn entry(tok: &str) -> WeightEntry {
    let f: Vec<&str> = tok.split(':').collect();
    WeightEntry::classify(WeightParts {
        bone_id: f[0].to_string(),
        pt_index: (f[1] != "u" && f[1] != "n").then(|| f[1].parse().expect("ptIndex")),
        offset: optional_num(f[2]),
        w: num(f[3]),
    })
}
fn weights(s: &str) -> Vec<Vec<WeightEntry>> {
    if s == "-" {
        return Vec::new();
    }
    s.split('|').map(|l| list(l, entry)).collect()
}
fn points(s: &str) -> Vec<Point2> {
    list(s, pt)
}
fn handle_pair(s: &str) -> (Option<Point2>, Option<Point2>) {
    let mut it = s.split('/');
    (
        opt_pt(it.next().unwrap_or("-")),
        opt_pt(it.next().unwrap_or("-")),
    )
}

fn pose_text(p: &SegmentPose) -> String {
    let mut s = hp(p.point);
    if let Some(h) = p.handle_in {
        s.push('~');
        s.push_str(&hp(h));
    }
    if let Some(h) = p.handle_out {
        s.push('^');
        s.push_str(&hp(h));
    }
    s
}
fn poses_text(l: &[SegmentPose]) -> String {
    if l.is_empty() {
        "_".to_string()
    } else {
        l.iter().map(pose_text).collect::<Vec<_>>().join(";")
    }
}
fn bind_text(r: Result<Vec<BindPose>, DeformFailure>) -> String {
    let Ok(mut poses) = r else {
        return "throw".to_string();
    };
    match poses.pop() {
        Some(BindPose::Skipped) | None => "skip".to_string(),
        Some(BindPose::Mesh(o)) if o.is_empty() => "_".to_string(),
        Some(BindPose::Mesh(o)) => o.iter().map(|p| hp(*p)).collect::<Vec<_>>().join(";"),
        Some(BindPose::Path(p)) => match &p.center {
            None => poses_text(&p.segments),
            Some(c) => format!("{}#{}", poses_text(&p.segments), poses_text(c)),
        },
    }
}

fn run_pth(f: &[&str]) -> String {
    let bones = bones(f[2]);
    let rest = points(f[4]);
    let rh: Option<Vec<RestHandle>> = (f[5] != "!").then(|| {
        list(f[5], |e| {
            let (i, o) = handle_pair(e);
            RestHandle {
                handle_in: i,
                handle_out: o,
            }
        })
    });
    let w = weights(f[6]);
    let center = if f[8] == "-" {
        None
    } else {
        let c: Vec<&str> = f[8].split('+').collect();
        let rows: Vec<CenterRest> = list(c[0], |e| {
            let (p, h) = e.split_once('/').expect("center rest");
            let (i, o) = handle_pair(h);
            CenterRest {
                point: pt(p),
                handle_in: i,
                handle_out: o,
            }
        });
        Some((rows, weights(c[1]), c[2].parse::<usize>().expect("count")))
    };
    let bind = PathBind {
        rest: &rest,
        rest_handles: rh.as_deref(),
        weights: &w,
        rotate: f[3] == "1",
        live_len: (f[7] != "!").then(|| f[7].parse().expect("live")),
        center: center.as_ref().map(|(r, cw, n)| CenterBind {
            rest: r,
            weights: cw,
            live_len: *n,
        }),
    };
    bind_text(apply_rig_deform(&bones, &[BindInput::Path(bind)]))
}

fn run_msh(f: &[&str]) -> String {
    let bones = bones(f[2]);
    let (verts, rest, w) = (points(f[4]), points(f[5]), weights(f[6]));
    let target = (f[7] != "-").then(|| {
        let r: Vec<f64> = f[7].split(',').map(num).collect();
        MeshTarget {
            verts: &verts,
            rect: MeshRect {
                x: r[0],
                y: r[1],
                width: r[2],
                height: r[3],
                rotation: r[4],
            },
        }
    });
    let bind = MeshBind {
        rest: &rest,
        weights: &w,
        rotate: f[3] == "1",
        target,
    };
    bind_text(apply_rig_deform(&bones, &[BindInput::Mesh(bind)]))
}

fn run_anc(f: &[&str]) -> String {
    let bones = bones(f[2]);
    let mut resolver = AnchorResolver::new(&bones);
    match resolver.resolve(&entry(f[3])) {
        Err(_) => "throw".to_string(),
        Ok(None) => "null".to_string(),
        Ok(Some(a)) => format!(
            "ctl:{}",
            [a.rest_x, a.rest_y, a.cur_x, a.cur_y, a.da]
                .map(hex)
                .join(",")
        ),
    }
}

/// A `pth`/`msh`/`anc` row: what production writes for it, what the baseline wrote, and the
/// index of the first number allowed to differ by libm roundoff.
pub(crate) struct RowRun {
    pub(crate) actual: String,
    pub(crate) expected: String,
    pub(crate) tol_from: Option<usize>,
}

/// Runs a plain row (fields already split, the last one the baseline output).
pub(crate) fn run_row(f: &[&str]) -> RowRun {
    let actual = match f[0] {
        "pth" => run_pth(f),
        "msh" => run_msh(f),
        "anc" => run_anc(f),
        other => panic!("unknown row kind {other}"),
    };
    RowRun {
        actual,
        expected: f[f.len() - 1].to_string(),
        tol_from: (f[1] != "-").then(|| f[1].parse().expect("tol")),
    }
}

fn split_hex(s: &str) -> (String, Vec<f64>) {
    let b = s.as_bytes();
    let (mut skeleton, mut nums, mut i) = (String::new(), Vec::new(), 0);
    while i < b.len() {
        if i + 16 <= b.len() && b[i..i + 16].iter().all(|c| c.is_ascii_hexdigit()) {
            nums.push(num(&s[i..i + 16]));
            skeleton.push('H');
            i += 16;
        } else {
            skeleton.push(b[i] as char);
            i += 1;
        }
    }
    (skeleton, nums)
}

/// Same shape; numbers bit-equal (any NaN equals any NaN), those from `tol_from` on within `REL_TOL`.
pub(crate) fn agrees(expected: &str, actual: &str, tol_from: Option<usize>) -> bool {
    let ((es, en), (as_, an)) = (split_hex(expected), split_hex(actual));
    if es != as_ || en.len() != an.len() {
        return false;
    }
    en.iter().zip(&an).enumerate().all(|(i, (e, a))| {
        if (e.is_nan() && a.is_nan()) || e.to_bits() == a.to_bits() {
            return true;
        }
        tol_from.is_some_and(|t| i >= t)
            && e.is_finite()
            && a.is_finite()
            && (e - a).abs() <= REL_TOL * 1.0f64.max(e.abs()).max(a.abs())
    })
}

#[test]
fn every_oracle_row_matches_production() {
    let (mut counts, mut bad) = ([0usize; 3], Vec::new());
    for (n, line) in ORACLE.lines().enumerate() {
        let f: Vec<&str> = line.split(' ').collect();
        if f[0] == "mut" {
            continue;
        }
        counts[["pth", "msh", "anc"]
            .iter()
            .position(|k| *k == f[0])
            .expect("kind")] += 1;
        let r = run_row(&f);
        if !agrees(&r.expected, &r.actual, r.tol_from) {
            bad.push(format!(
                "row {} ({}): expected {} got {}",
                n + 1,
                f[0],
                r.expected,
                r.actual
            ));
        }
    }
    assert!(
        bad.is_empty(),
        "{} rows differ, first: {:?}",
        bad.len(),
        &bad[..bad.len().min(3)]
    );
    assert_eq!(
        counts,
        [150, 73, 155],
        "oracle row counts per kind (pth, msh, anc)"
    );
}
