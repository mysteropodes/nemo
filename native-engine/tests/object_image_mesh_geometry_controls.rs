//! Controls for the image-mesh geometry kernel (L03-A): mutants the oracle rows must
//! reject, a mutation control proving the "mesh is never written" check can fail, guards
//! against a vacuous oracle, and the edge literals the oracle cannot reach.
//!
//! A mutant here is a deliberate single-flaw copy of one baseline rule, run through the
//! same row harness as production; production must pass it and the mutant must not. The
//! generator (`tools/gen.cjs`) proves every one of these flaws changes at least one row of
//! the baseline output in JavaScript before the oracle is written.

use std::f64::consts::PI;

use super::oracle_rows::{
    densify_run, hex, normalized_run, pip_run, points, rect, resample_run, rotates, round_run,
    rows, same_pts, scene_run, OwnedMesh,
};
use super::tests::{fixture_mesh, near, OUTLINE8};
use super::{
    densify, js_round, normalized_of, point_in_poly, r5, resample_offsets, scene_payload,
    world_verts, MeshGeometryError, MeshRect, MeshView, Pose, ScenePayload,
};

#[derive(Clone, Copy, PartialEq)]
enum Flaw {
    /// Tolerance `+1e-6` instead of `-1e-6`: a point just outside is "inside" no longer.
    ToleranceSign,
    /// No nearest-vertex fallback: a point outside every triangle reads zero.
    DroppedFallback,
    /// `w0` and `w1` swapped in the blend of the three vertex offsets.
    SwappedWeights,
}

fn resample_mutant(
    flaw: Flaw,
    old_pts: &[[f64; 2]],
    old_offs: Option<&[[f64; 2]]>,
    old_tris: &[u32],
    new_pts: &[[f64; 2]],
) -> Vec<[f64; 2]> {
    let offs = match old_offs {
        Some(offs) if !old_pts.is_empty() => offs,
        _ => return new_pts.iter().map(|_| [0.0, 0.0]).collect(),
    };
    let off_at = |i: usize| offs.get(i).copied().unwrap_or([0.0, 0.0]);
    let tol = if flaw == Flaw::ToleranceSign {
        1e-6
    } else {
        -1e-6
    };
    new_pts
        .iter()
        .map(|q| {
            for tri in old_tris.chunks_exact(3) {
                let ids = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
                let (Some(a), Some(b), Some(c)) = (
                    old_pts.get(ids[0]),
                    old_pts.get(ids[1]),
                    old_pts.get(ids[2]),
                ) else {
                    continue;
                };
                let d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
                if d == 0.0 || d.is_nan() {
                    continue;
                }
                let w0 = ((b[1] - c[1]) * (q[0] - c[0]) + (c[0] - b[0]) * (q[1] - c[1])) / d;
                let w1 = ((c[1] - a[1]) * (q[0] - c[0]) + (a[0] - c[0]) * (q[1] - c[1])) / d;
                let w2 = 1.0 - w0 - w1;
                if w0 < tol || w1 < tol || w2 < tol {
                    continue;
                }
                let (u0, u1) = if flaw == Flaw::SwappedWeights {
                    (w1, w0)
                } else {
                    (w0, w1)
                };
                let (oa, ob, oc) = (off_at(ids[0]), off_at(ids[1]), off_at(ids[2]));
                return [
                    oa[0] * u0 + ob[0] * u1 + oc[0] * w2,
                    oa[1] * u0 + ob[1] * u1 + oc[1] * w2,
                ];
            }
            if flaw == Flaw::DroppedFallback {
                return [0.0, 0.0];
            }
            // No triangle holds the point: production with no triangles IS the fallback.
            resample_offsets(old_pts, old_offs, &[], &[*q])[0]
        })
        .collect()
}

#[test]
fn production_resample_passes_the_harness_the_mutants_are_run_through() {
    let (n, bad) = resample_run(&resample_offsets);
    assert_eq!((n, bad.len()), (19, 0), "{bad:?}");
}

#[test]
fn mutant_tolerance_sign_is_rejected() {
    let (n, bad) = resample_run(&|a, b, c, d| resample_mutant(Flaw::ToleranceSign, a, b, c, d));
    assert_eq!(n, 19);
    assert!(!bad.is_empty(), "a +1e-6 tolerance slipped through");
}

#[test]
fn mutant_dropped_fallback_is_rejected() {
    let (_, bad) = resample_run(&|a, b, c, d| resample_mutant(Flaw::DroppedFallback, a, b, c, d));
    assert!(
        !bad.is_empty(),
        "a missing nearest-vertex fallback slipped through"
    );
}

#[test]
fn mutant_swapped_weights_is_rejected() {
    let (_, bad) = resample_run(&|a, b, c, d| resample_mutant(Flaw::SwappedWeights, a, b, c, d));
    assert!(
        !bad.is_empty(),
        "swapped barycentric weights slipped through"
    );
}

#[test]
fn mutant_half_up_by_floor_plus_half_is_rejected() {
    // floor(x + 0.5) is not Math.round: it loses -0 and rounds 0.49999999999999994 up.
    let naive = |mode: u8, x: f64| {
        let k = if mode == 2 { 100.0 } else { 100000.0 };
        (x * k + 0.5).floor() / k
    };
    let (n, bad) = round_run(&naive);
    assert_eq!(n, 108);
    assert!(
        bad.len() >= 4,
        "only {} rows rejected the naive rounder",
        bad.len()
    );
}

fn crossing_with_ge(poly: &[[f64; 2]], x: f64, y: f64) -> bool {
    let mut inside = false;
    let mut j = poly.len().wrapping_sub(1);
    for i in 0..poly.len() {
        let (xi, yi, xj, yj) = (poly[i][0], poly[i][1], poly[j][0], poly[j][1]);
        if ((yi >= y) != (yj >= y)) && (x < (xj - xi) * (y - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[test]
fn mutant_crossing_test_with_ge_is_rejected() {
    assert!(pip_run(&point_in_poly).1.is_empty());
    let (_, bad) = pip_run(&crossing_with_ge);
    assert!(!bad.is_empty(), "yi >= y slipped through");
}

#[test]
fn mutant_densify_with_ceil_is_rejected() {
    let ceil = |poly: &[[f64; 2]], max_len: f64| -> Result<Vec<[f64; 2]>, MeshGeometryError> {
        let mut out = Vec::new();
        for (i, p) in poly.iter().enumerate() {
            let q = poly[(i + 1) % poly.len()];
            out.push(*p);
            let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
            let n = ((dx * dx + dy * dy).sqrt() / max_len).ceil();
            let mut s = 1.0;
            while s <= n {
                let t = s / (n + 1.0);
                out.push([p[0] + dx * t, p[1] + dy * t]);
                s += 1.0;
            }
        }
        Ok(out)
    };
    let (_, bad) = densify_run(&ceil);
    assert!(!bad.is_empty(), "ceil instead of floor slipped through");
}

/// The corpus helper reads x,y as the CENTRE; production rects are top-left.
fn scene_with_centre_rect(m: &MeshView, r: &MeshRect, p: Pose) -> Option<ScenePayload> {
    let tl = MeshRect {
        x: r.x - r.width / 2.0,
        y: r.y - r.height / 2.0,
        ..*r
    };
    scene_payload(m, &tl, p)
}

/// Uvs that follow the offset instead of staying at the rest position.
fn scene_with_deformed_uvs(m: &MeshView, r: &MeshRect, p: Pose) -> Option<ScenePayload> {
    let mut payload = scene_payload(m, r, p)?;
    for (i, uv) in payload.uvs.iter_mut().enumerate() {
        let o = m.offsets.get(i).copied().unwrap_or([0.0, 0.0]);
        *uv = [r5(m.verts[i][0] + o[0]), r5(m.verts[i][1] + o[1])];
    }
    Some(payload)
}

#[test]
fn mutant_centre_convention_is_rejected() {
    let (n, bad) = scene_run(&scene_with_centre_rect);
    assert_eq!(n, 71);
    assert!(
        bad.len() >= 50,
        "centre convention rejected by only {} rows",
        bad.len()
    );
}

#[test]
fn mutant_uvs_from_the_deformed_position_are_rejected() {
    let (_, bad) = scene_run(&scene_with_deformed_uvs);
    assert!(
        !bad.is_empty(),
        "uvs that follow the offset slipped through"
    );
}

#[test]
fn mutant_unguarded_zero_size_division_is_rejected() {
    let unguarded = |r: &MeshRect, wx: f64, wy: f64| {
        let deg = if r.rotation == 0.0 || r.rotation.is_nan() {
            0.0
        } else {
            r.rotation
        };
        let rad = deg * PI / 180.0;
        let (cos, sin) = (rad.cos(), rad.sin());
        let (dx, dy) = (wx - (r.x + r.width / 2.0), wy - (r.y + r.height / 2.0));
        [
            (dx * cos + dy * sin) / r.width + 0.5,
            (-dx * sin + dy * cos) / r.height + 0.5,
        ]
    };
    assert!(normalized_run(&normalized_of).1.is_empty());
    let (_, bad) = normalized_run(&unguarded);
    assert!(!bad.is_empty(), "a divide-by-zero slipped through");
}

// ---- mutation control: the "kernel never writes the mesh" check can fail ----

fn state_matches(a: &OwnedMesh, b: &OwnedMesh) -> bool {
    same_pts(&a.outline, &b.outline)
        && same_pts(&a.verts, &b.verts)
        && same_pts(&a.offsets, &b.offsets)
        && a.tris == b.tris
}

/// A world-vertex pass that folds the offset into the rest position as it goes.
fn world_verts_folding_offsets(m: &mut OwnedMesh, rect: &MeshRect) -> Vec<[f64; 2]> {
    for (v, o) in m.verts.iter_mut().zip(m.offsets.iter_mut()) {
        *v = [v[0] + o[0], v[1] + o[1]];
        *o = [0.0, 0.0];
    }
    world_verts(&m.view(), rect, None)
}

#[test]
fn kernel_never_mutates_the_supplied_mesh() {
    let rect = MeshRect {
        x: 320.0,
        y: 180.0,
        width: 200.0,
        height: 200.0,
        rotation: 30.0,
    };
    let mut meshes = vec![fixture_mesh([0.1, 0.0])];
    meshes.extend(rows("sp").map(|(_, t)| super::oracle_rows::mesh(t[1])));
    for m in &meshes {
        let before = m.clone();
        let _ = world_verts(&m.view(), &rect, None);
        let _ = scene_payload(&m.view(), &rect, None);
        let _ = resample_offsets(&m.verts, Some(m.offsets.as_slice()), &m.tris, &m.verts);
        let _ = point_in_poly(&m.outline, 0.5, 0.5);
        assert!(state_matches(&before, m), "kernel wrote into the mesh");
    }
}

#[test]
fn mutation_control_folding_offsets_into_rest_is_detected() {
    let rect = MeshRect {
        x: 320.0,
        y: 180.0,
        width: 200.0,
        height: 200.0,
        rotation: 0.0,
    };
    let mut m = fixture_mesh([0.1, 0.0]);
    let before = m.clone();
    let folded = world_verts_folding_offsets(&mut m, &rect);
    assert!(
        !state_matches(&before, &m),
        "the check must see rest + offset collapse"
    );
    // The deformed picture is unchanged, which is why only the state check can catch it.
    assert!(same_pts(&folded, &world_verts(&before.view(), &rect, None)));
}

#[test]
fn returned_payload_is_detached_from_the_mesh() {
    let rect = MeshRect {
        x: 0.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
        rotation: 0.0,
    };
    let m = fixture_mesh([0.1, 0.0]);
    let before = m.clone();
    let mut p = scene_payload(&m.view(), &rect, None).unwrap();
    p.tris[0] = 99;
    p.uvs[0] = [f64::NAN, f64::NAN];
    p.verts.clear();
    assert!(state_matches(&before, &m));
}

// ---- guards: the oracle exercises both outcomes of every family ----

#[test]
fn oracle_is_not_vacuous() {
    assert!(rows("pip").any(|(_, t)| t[4] == "1") && rows("pip").any(|(_, t)| t[4] == "0"));
    assert!(rows("sp").any(|(_, t)| t[4] == "-") && rows("sp").any(|(_, t)| t[4] != "-"));
    assert!(rows("den").any(|(_, t)| points(t[3]).len() > points(t[1]).len()));
    assert!(rows("den").any(|(_, t)| points(t[3]).len() == points(t[1]).len()));
    assert!(rows("rnd").any(|(_, t)| hex(t[3]).to_bits() == (-0.0f64).to_bits()));
    assert!(rows("rb").all(|(_, t)| points(t[5]).iter().any(|o| o[0] != 0.0 || o[1] != 0.0)));
    assert!(rows("rs").any(|(_, t)| t[2] == "-") && rows("rs").any(|(_, t)| t[1] == "_"));
    assert!(rows("wv").any(|(_, t)| t[3] != "-") && rows("wv").any(|(_, t)| t[3] == "-"));
    assert!(rows("nz").any(|(_, t)| rotates(&rect(t[1]))));
}

// ---- edge literals the oracle cannot reach ----

#[test]
fn js_round_edges() {
    assert_eq!(js_round(-0.5).to_bits(), (-0.0f64).to_bits());
    assert_eq!(js_round(-0.2).to_bits(), (-0.0f64).to_bits());
    assert_eq!(js_round(0.49999999999999994).to_bits(), 0.0f64.to_bits());
    assert_eq!(
        (js_round(2.5), js_round(-2.5), js_round(-1.5), js_round(0.5)),
        (3.0, -2.0, -1.0, 1.0)
    );
    assert_eq!(js_round(4503599627370495.5), 4503599627370496.0);
    assert!(js_round(f64::NAN).is_nan());
    assert_eq!(js_round(f64::NEG_INFINITY), f64::NEG_INFINITY);
}

#[test]
fn zero_size_rect_divides_by_one_in_normalized_of() {
    let flat = MeshRect {
        x: 10.0,
        y: 10.0,
        width: 0.0,
        height: 0.0,
        rotation: 0.0,
    };
    assert_eq!(normalized_of(&flat, 15.0, 13.0), [5.5, 3.5]);
    let flat_y = MeshRect {
        x: 10.0,
        y: 10.0,
        width: 200.0,
        height: 0.0,
        rotation: 0.0,
    };
    let n = normalized_of(&flat_y, 20.0, 13.0);
    assert!(
        near([n[0], 0.0], [0.05, 0.0], 1e-15) && n[1] == 3.5,
        "{n:?}"
    );
}

#[test]
fn densify_of_the_unit_square_at_step_one_is_the_outline8() {
    let square = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    assert_eq!(densify(&square, 1.0).unwrap(), OUTLINE8.to_vec());
    assert_eq!(
        densify(&square, -1.0).unwrap(),
        square.to_vec(),
        "negative step inserts nothing"
    );
    assert_eq!(densify(&[], 1.0).unwrap(), Vec::<[f64; 2]>::new());
}

#[test]
fn densify_refuses_a_step_the_baseline_could_never_finish() {
    let edge = [[0.0, 0.0], [1.0, 0.0]];
    assert_eq!(
        densify(&edge, 0.0),
        Err(MeshGeometryError::DensifyUnbounded)
    );
    assert_eq!(
        densify(&edge, 1e-9),
        Err(MeshGeometryError::DensifyUnbounded)
    );
    assert_eq!(
        densify(&[[0.0, 0.0], [0.0, 0.0]], 0.0).unwrap().len(),
        2,
        "0/0 is NaN: no inserts"
    );
}

#[test]
fn resample_tolerance_zeros_and_fallback_literals() {
    let tri = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
    let offs = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
    let r = |q: [f64; 2]| resample_offsets(&tri, Some(&offs[..]), &[0, 1, 2], &[q])[0];
    assert_eq!(r([0.25, 0.25]), [0.25, 0.25], "barycentric 0.5/0.25/0.25");
    assert!(
        near(r([-5e-7, 0.5]), [-5e-7, 0.5], 1e-12),
        "weight -5e-7 is inside the -1e-6 tolerance"
    );
    assert_eq!(
        r([-2e-6, 0.5]),
        [0.0, 0.0],
        "weight -2e-6 is outside: nearest vertex (a first on a tie)"
    );
    assert_eq!(
        r([2.0, 2.0]),
        [1.0, 0.0],
        "outside the field: nearest vertex, first of equals"
    );
    let zeros = resample_offsets(&[], Some(&offs[..]), &[0, 1, 2], &[[0.5, 0.5]]);
    assert_eq!(
        zeros[0].map(f64::to_bits),
        [0, 0],
        "no old points: +0 zeros"
    );
    let falsy = resample_offsets(&tri, None, &[0, 1, 2], &[[0.5, 0.5]]);
    assert_eq!(
        falsy[0].map(f64::to_bits),
        [0, 0],
        "no old offsets: +0 zeros"
    );
}
