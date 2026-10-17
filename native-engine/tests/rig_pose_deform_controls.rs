//! Negative and mutation controls of the rig deform kernel (L03-E, I12).
//!
//! 1. Mutants: `tools/gen.cjs` re-runs the VERBATIM baseline with 18 textual flaws (a wrong divisor,
//!    a flipped rotation, handles never/always rotated, a one-zero-component offset test, an unbounded
//!    loop, an absolute mesh offset, a wrapped offset-model angle, unresolved entries folded into the
//!    weight sum, a required `restHandles`, ...) and pins, per mutant, rows where the mutated baseline
//!    answers differently. Production must still equal the real baseline on each of those rows and
//!    must NOT equal the mutant, under the very comparator the oracle test uses.
//! 2. The comparator itself: it must reject what it is supposed to reject.
//! 3. Production never writes through its inputs and keeps no state between calls.

use super::oracle_rows::{agrees, run_row, ORACLE};
use super::{
    apply_rig_deform, BindInput, BindPose, CenterBind, CenterRest, MeshBind, MeshTarget, PathBind,
    RestHandle,
};
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_geometry::MeshRect;
use crate::evaluation::object_image_mesh_kernels::rig_pose_deform_anchor::{
    DeformBone, DeformSegment,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_weights::{
    ControlWeight, OffsetWeight, Point2, WeightEntry,
};

const MUTANTS: [&str; 18] = [
    "divide-mesh-by-sumW",
    "divide-path-by-sumW",
    "divide-center-by-sumW",
    "rotate-sign-mesh",
    "rotate-sign-path",
    "rotate-sign-center",
    "handle-in-sign",
    "handle-out-sign",
    "handles-never-rotate",
    "handles-always-rotate",
    "center-handles-by-total-weight",
    "one-zero-component-is-zero",
    "mesh-loop-ignores-rest-length",
    "path-loop-ignores-rest-length",
    "mesh-offset-absolute",
    "offset-model-angle-normalized",
    "unresolved-folded-into-sumW",
    "rest-handles-required",
];

// Pins the comparator's discrimination (each mutant row is told apart from the baseline at the oracle
// tolerance); production's sensitivity to a real bug comes from `every_oracle_row_matches_production`.
#[test]
fn production_equals_the_baseline_and_differs_from_every_mutant_on_the_pinned_rows() {
    let mut seen: Vec<(&str, usize)> = MUTANTS.iter().map(|m| (*m, 0)).collect();
    for line in ORACLE.lines().filter(|l| l.starts_with("mut ")) {
        let f: Vec<&str> = line.split(' ').collect();
        let (name, row, mutant_out) = (f[1], &f[2..f.len() - 1], f[f.len() - 1]);
        let r = run_row(row);
        assert!(
            agrees(&r.expected, &r.actual, r.tol_from),
            "{name}: production differs from the baseline: {} vs {}",
            r.expected,
            r.actual
        );
        assert!(
            !agrees(mutant_out, &r.actual, r.tol_from),
            "{name}: production equals the mutant on a row pinned to tell them apart"
        );
        assert!(
            !agrees(mutant_out, &r.expected, r.tol_from),
            "{name}: the comparator cannot tell the mutant from the baseline"
        );
        seen.iter_mut()
            .find(|(m, _)| *m == name)
            .expect("known mutant")
            .1 += 1;
    }
    for (name, n) in seen {
        assert!(n >= 1, "mutant {name} has no discriminating row");
    }
}

#[test]
fn the_comparator_rejects_what_it_must_reject() {
    let one = "3ff0000000000000";
    let next = "3ff0000000000001"; // one ulp above 1.0
    let far = "3ff0000100000000"; // about 1e-6 above 1.0
    let nan = "7ff8000000000000";
    let neg_nan = "fff8000000000000";
    let pair = |a: &str, b: &str| format!("{a},{b}");
    // bit exact: one ulp is a failure, identical bits and any NaN pair are not
    assert!(agrees(one, one, None));
    assert!(!agrees(one, next, None));
    assert!(agrees(nan, neg_nan, None));
    assert!(!agrees(nan, one, None));
    // tolerant from an index on: one ulp passes there, a visible difference never, an earlier slot stays exact
    assert!(agrees(&pair(one, one), &pair(one, next), Some(1)));
    assert!(!agrees(&pair(one, one), &pair(next, one), Some(1)));
    assert!(!agrees(&pair(one, one), &pair(one, far), Some(1)));
    assert!(!agrees(
        &pair(one, one),
        &pair(one, "bff0000000000000"),
        Some(0)
    ));
    // the shape must match: separators, handles, markers and number counts
    assert!(!agrees(&pair(one, one), &format!("{one};{one}"), Some(0)));
    assert!(!agrees(one, &format!("{one}~{one}"), Some(0)));
    assert!(!agrees("throw", "skip", None));
    assert!(agrees("throw", "throw", None));
    assert!(!agrees(one, "_", None));
}

fn dseg(p: Point2) -> DeformSegment {
    DeformSegment {
        point: Some(p),
        handle_in: None,
        handle_out: None,
    }
}
fn bone(id: &str, rest: &[Point2], live: &[Point2]) -> DeformBone {
    DeformBone {
        id: id.to_string(),
        closed: false,
        segments: Some(live.iter().map(|p| dseg(*p)).collect()),
        rest_segments: Some(rest.iter().map(|p| dseg(*p)).collect()),
    }
}
fn ctl(b: &str, pt: usize, w: f64) -> WeightEntry {
    WeightEntry::Control(ControlWeight {
        bone_id: b.to_string(),
        pt_index: pt,
        w,
        stray_offset: None,
    })
}
fn off(b: &str, offset: f64, w: f64) -> WeightEntry {
    WeightEntry::Offset(OffsetWeight {
        bone_id: b.to_string(),
        offset: Some(offset),
        w,
    })
}

/// Owned inputs of one rig with a stroke, a vector-brush centerline and a mesh bind.
#[derive(Clone, PartialEq, Debug)]
struct Rig {
    bones: Vec<DeformBone>,
    rest: Vec<Point2>,
    rest_handles: Vec<RestHandle>,
    weights: Vec<Vec<WeightEntry>>,
    center_rest: Vec<CenterRest>,
    center_weights: Vec<Vec<WeightEntry>>,
    verts: Vec<Point2>,
    mesh_rest: Vec<Point2>,
    mesh_weights: Vec<Vec<WeightEntry>>,
}

fn rig() -> Rig {
    let arm = bone(
        "b1",
        &[[0.0, 0.0], [100.0, 0.0], [200.0, 0.0]],
        &[[0.0, 0.0], [0.0, 100.0], [0.0, 200.0]],
    );
    Rig {
        bones: vec![arm],
        rest: vec![[100.0, 0.0], [100.0, 40.0], [50.0, 0.0]],
        rest_handles: vec![
            RestHandle {
                handle_in: Some([-30.0, 1.0]),
                handle_out: Some([30.0, 2.0]),
            },
            RestHandle {
                handle_in: None,
                handle_out: Some([5.0, 5.0]),
            },
            RestHandle {
                handle_in: None,
                handle_out: None,
            },
        ],
        weights: vec![
            vec![ctl("b1", 1, 1.0)],
            vec![off("b1", 120.0, 0.75), ctl("b1", 2, 0.5)],
            vec![ctl("b1", 0, 0.5), ctl("b1", 1, 0.5)],
        ],
        center_rest: vec![CenterRest {
            point: [10.0, 10.0],
            handle_in: Some([-4.0, 0.0]),
            handle_out: Some([4.0, 0.0]),
        }],
        center_weights: vec![vec![ctl("b1", 0, 0.5), off("b1", 60.0, 0.5)]],
        verts: vec![[0.25, 0.25], [0.75, 0.5]],
        mesh_rest: vec![[150.0, 87.5], [250.0, 100.0]],
        mesh_weights: vec![vec![ctl("b1", 2, 0.5)], vec![ctl("b1", 1, 1.0)]],
    }
}

fn poses(r: &Rig, rotate: bool) -> Vec<BindPose> {
    let rect = MeshRect {
        x: 100.0,
        y: 50.0,
        width: 200.0,
        height: 100.0,
        rotation: 0.0,
    };
    let path = PathBind {
        rest: &r.rest,
        rest_handles: Some(&r.rest_handles),
        weights: &r.weights,
        rotate,
        live_len: Some(3),
        center: Some(CenterBind {
            rest: &r.center_rest,
            weights: &r.center_weights,
            live_len: 1,
        }),
    };
    let mesh = MeshBind {
        rest: &r.mesh_rest,
        weights: &r.mesh_weights,
        rotate,
        target: Some(MeshTarget {
            verts: &r.verts,
            rect,
        }),
    };
    let binds = [BindInput::Path(path), BindInput::Mesh(mesh)];
    apply_rig_deform(&r.bones, &binds).expect("deforms")
}

#[test]
fn deforming_never_writes_through_its_inputs_and_keeps_no_state() {
    for rotate in [false, true] {
        let r = rig();
        let before = r.clone();
        let first = poses(&r, rotate);
        assert_eq!(
            r, before,
            "rest points, handles, weights, bones and vertices stay as supplied"
        );
        // a pose is a value: the same inputs give the same answer again (rest handles are the input,
        // never the previously written ones), also after an unrelated call with another pose
        let mut other = r.clone();
        other.bones[0].segments = other.bones[0].rest_segments.clone();
        let idle = poses(&other, rotate);
        assert_ne!(
            idle, first,
            "a different live pose gives a different result"
        );
        assert_eq!(poses(&r, rotate), first);
        // detached output: editing the result cannot reach the inputs
        let mut edited = first.clone();
        if let Some(BindPose::Path(p)) = edited.get_mut(0) {
            p.segments[0].point = [9e9, 9e9];
        }
        assert_eq!(r, before);
        assert_ne!(edited, first);
    }
}

#[test]
fn the_rest_pose_gives_back_the_rest_geometry() {
    let mut r = rig();
    r.bones[0].segments = r.bones[0].rest_segments.clone();
    for rotate in [false, true] {
        let out = poses(&r, rotate);
        let BindPose::Path(p) = &out[0] else {
            panic!("path pose")
        };
        let pts: Vec<Point2> = p.segments.iter().map(|s| s.point).collect();
        assert_eq!(
            pts, r.rest,
            "a bone at its rest pose moves nothing (rotate {rotate})"
        );
        assert_eq!(p.segments[0].handle_in, Some([-30.0, 1.0]));
        assert_eq!(p.segments[0].handle_out, Some([30.0, 2.0]));
        assert_eq!(p.segments[2].handle_in, None);
        let BindPose::Mesh(m) = &out[1] else {
            panic!("mesh pose")
        };
        assert_eq!(
            m,
            &vec![[0.0, 0.0], [0.0, 0.0]],
            "no displacement writes the exact pair [0, 0]"
        );
    }
}

#[test]
fn the_oracle_pins_every_outcome_class_and_the_fixture_size() {
    let plain: Vec<&str> = ORACLE.lines().filter(|l| !l.starts_with("mut ")).collect();
    let count = |s: &str| {
        plain
            .iter()
            .filter(|l| l.rsplit(' ').next() == Some(s))
            .count()
    };
    assert_eq!((count("throw"), count("skip"), count("null")), (41, 4, 87));
    assert_eq!(plain.len(), 378);
    assert_eq!(
        ORACLE.lines().count() - plain.len(),
        36,
        "two pinned rows per mutant"
    );
    // rows exist for the exact (no trig) class and the tolerant class of every kind
    for kind in ["pth", "msh", "anc"] {
        let tol = |exact: bool| {
            plain
                .iter()
                .filter(|l| l.starts_with(kind))
                .any(|l| (l.split(' ').nth(1) == Some("-")) == exact)
        };
        assert!(
            tol(true) && tol(false),
            "{kind} needs bit-exact and tolerant rows"
        );
    }
}
