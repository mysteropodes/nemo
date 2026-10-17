//! Mutation and guard controls for the image-mesh draw plan (L03-H, F17/F18).
//!
//! `reference` is an independent second implementation of the baseline plan
//! with one named flaw switched on. `Flaw::Clean` must pass exactly the checks
//! production passes (so the harness is the one the mutants are judged by), and
//! every real flaw must be rejected by at least as many oracle rows as the
//! generator measured for the same flaw in Node.

use super::oracle_rows::{dilation_run, fixture_run, plan_cases, plan_run, Case, PlanResult, Run};
use super::{
    half_pixel_dilation, plan_mesh_draw, AffineCoeffs, MeshDrawPlan, MeshPlanInput, OutlineClip,
    OutlineKind, PlanSkip, Point2, TrianglePlan,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flaw {
    Clean,
    ImageDilated,
    NoKClamp,
    ViewUnfloored,
    FullPixel,
    ShortestEdge,
    DetLoose,
    NoBbox,
    OutlineMissingSkipped,
    AboutVertex,
    UvSwap,
    NoAreaGuard,
}

/// Each real flaw with the number of oracle rows the Node generator measured it
/// to change (plan rows plus dilation rows).
const FLAWS: [(Flaw, usize); 11] = [
    (Flaw::ImageDilated, 111),
    (Flaw::NoKClamp, 40),
    (Flaw::ViewUnfloored, 3),
    (Flaw::FullPixel, 82),
    (Flaw::ShortestEdge, 69),
    (Flaw::DetLoose, 2),
    (Flaw::NoBbox, 40),
    (Flaw::OutlineMissingSkipped, 6),
    (Flaw::AboutVertex, 111),
    (Flaw::UvSwap, 42),
    (Flaw::NoAreaGuard, 15),
];

fn dilation(flaw: Flaw, view: &AffineCoeffs) -> f64 {
    let mut scale = (view[0] * view[3] - view[1] * view[2]).abs().sqrt();
    if flaw != Flaw::ViewUnfloored {
        scale = scale.max(1e-9);
    }
    let half = if flaw == Flaw::FullPixel { 1.0 } else { 0.5 };
    half / scale
}

fn outline(flaw: Flaw, input: &MeshPlanInput<'_>) -> Result<OutlineClip, PlanSkip> {
    if input.outline.len() >= 3 {
        let mut points = Vec::new();
        for &i in input.outline {
            match input.verts.get(i as usize) {
                Some(p) => points.push(*p),
                None if flaw == Flaw::OutlineMissingSkipped => {}
                None => return Err(PlanSkip::OutlineVertexMissing),
            }
        }
        return Ok(OutlineClip {
            kind: OutlineKind::Polygon,
            points,
        });
    }
    if flaw == Flaw::NoBbox {
        return Err(PlanSkip::OutlineVertexMissing);
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in input.verts {
        x0 = x0.min(p[0]);
        y0 = y0.min(p[1]);
        x1 = x1.max(p[0]);
        y1 = y1.max(p[1]);
    }
    Ok(OutlineClip {
        kind: OutlineKind::BoundingBox,
        points: vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]],
    })
}

fn solve(flaw: Flaw, s: &[Point2], d: &[Point2]) -> Option<AffineCoeffs> {
    let (e1x, e1y) = (s[1][0] - s[0][0], s[1][1] - s[0][1]);
    let (e2x, e2y) = (s[2][0] - s[0][0], s[2][1] - s[0][1]);
    let det = e1x * e2y - e2x * e1y;
    let limit = if flaw == Flaw::DetLoose { 1e-9 } else { 1e-12 };
    if det.abs() < limit {
        return None;
    }
    let (f1x, f1y) = (d[1][0] - d[0][0], d[1][1] - d[0][1]);
    let (f2x, f2y) = (d[2][0] - d[0][0], d[2][1] - d[0][1]);
    let a = (f1x * e2y - f2x * e1y) / det;
    let b = (f1y * e2y - f2y * e1y) / det;
    let c = (f2x * e1x - f1x * e2x) / det;
    let e = (f2y * e1x - f1y * e2x) / det;
    Some([
        a,
        b,
        c,
        e,
        d[0][0] - (a * s[0][0] + c * s[0][1]),
        d[0][1] - (b * s[0][0] + e * s[0][1]),
    ])
}

fn clip_of(flaw: Flaw, d: &[Point2], dilate: f64) -> (f64, Vec<Point2>) {
    let (mut gx, mut gy) = (
        (d[0][0] + d[1][0] + d[2][0]) / 3.0,
        (d[0][1] + d[1][1] + d[2][1]) / 3.0,
    );
    let area2 = ((d[1][0] - d[0][0]) * (d[2][1] - d[0][1])
        - (d[2][0] - d[0][0]) * (d[1][1] - d[0][1]))
        .abs();
    let mut k = 1.0_f64;
    if area2 > 1e-12 || flaw == Flaw::NoAreaGuard {
        let len = |a: Point2, b: Point2| {
            ((b[0] - a[0]) * (b[0] - a[0]) + (b[1] - a[1]) * (b[1] - a[1])).sqrt()
        };
        let (l0, l1, l2) = (len(d[0], d[1]), len(d[1], d[2]), len(d[2], d[0]));
        let reach = if flaw == Flaw::ShortestEdge {
            l0.min(l1).min(l2)
        } else {
            l0.max(l1).max(l2)
        };
        k = 1.0 + dilate * 3.0 * reach / area2;
        if !k.is_finite() || (flaw != Flaw::NoKClamp && k > 4.0) {
            k = 4.0;
        }
    }
    if flaw == Flaw::AboutVertex {
        gx = d[0][0];
        gy = d[0][1];
    }
    (
        k,
        d.iter()
            .map(|p| [gx + (p[0] - gx) * k, gy + (p[1] - gy) * k])
            .collect(),
    )
}

fn reference(
    flaw: Flaw,
    input: &MeshPlanInput<'_>,
    iw: u32,
    ih: u32,
    opacity: f32,
    view: &AffineCoeffs,
) -> PlanResult {
    let n = input.verts.len();
    if n < 3 || input.tris.len() < 3 || input.uvs.len() < n {
        return Err(PlanSkip::IncompleteMesh);
    }
    let (iw, ih) = (f64::from(iw), f64::from(ih));
    if iw <= 0.0 || ih <= 0.0 {
        return Err(PlanSkip::EmptyImage);
    }
    let dilate = dilation(flaw, view);
    let outline = outline(flaw, input)?;
    let (sx, sy) = if flaw == Flaw::UvSwap {
        (ih, iw)
    } else {
        (iw, ih)
    };
    let mut triangles = Vec::new();
    for (ordinal, tri) in input.tris.chunks_exact(3).enumerate() {
        let idx: Vec<usize> = tri.iter().map(|&i| i as usize).collect();
        if idx.iter().any(|&i| i >= n) {
            continue;
        }
        let src: Vec<Point2> = idx
            .iter()
            .map(|&i| [input.uvs[i][0] * sx, input.uvs[i][1] * sy])
            .collect();
        let dst: Vec<Point2> = idx.iter().map(|&i| input.verts[i]).collect();
        let Some(placed) = solve(flaw, &src, &dst) else {
            continue;
        };
        let (k, clip) = clip_of(flaw, &dst, dilate);
        let place = if flaw == Flaw::ImageDilated {
            solve(flaw, &src, &clip).unwrap_or(placed)
        } else {
            placed
        };
        triangles.push(TrianglePlan {
            triangle: ordinal,
            source: [src[0], src[1], src[2]],
            dest: [dst[0], dst[1], dst[2]],
            place,
            dilation: k,
            clip: [clip[0], clip[1], clip[2]],
        });
    }
    Ok(MeshDrawPlan {
        opacity,
        outline,
        triangles,
    })
}

fn flawed(flaw: Flaw, c: &Case) -> PlanResult {
    reference(flaw, &c.input(), c.width, c.height, c.opacity, &c.view)
}

fn plan_rows(flaw: Flaw) -> Run {
    plan_run(&|c: &Case| flawed(flaw, c))
}

fn fixture_rows(flaw: Flaw) -> Run {
    fixture_run(&|c: &Case| flawed(flaw, c))
}

fn dilation_rows(flaw: Flaw) -> Run {
    let f = |v: &AffineCoeffs| dilation(flaw, v);
    dilation_run(&f)
}

fn rejected(flaw: Flaw) -> usize {
    plan_rows(flaw).1.len() + fixture_rows(flaw).1.len() + dilation_rows(flaw).1.len()
}

#[test]
fn reference_without_a_flaw_passes_the_harness_the_mutants_are_judged_by() {
    assert_eq!(rejected(Flaw::Clean), 0);
    assert_eq!(plan_rows(Flaw::Clean).0, 144);
    assert_eq!(fixture_rows(Flaw::Clean).0, 10);
    assert_eq!(dilation_rows(Flaw::Clean).0, 16);
}

#[test]
fn reference_without_a_flaw_equals_production_on_every_corpus_case() {
    for c in plan_cases() {
        let got = plan_mesh_draw(&c.input(), c.width, c.height, 0.75, &c.view);
        let want = reference(Flaw::Clean, &c.input(), c.width, c.height, 0.75, &c.view);
        assert_eq!(format!("{got:?}"), format!("{want:?}"), "{}", c.label);
    }
}

#[test]
fn every_flaw_is_rejected_by_at_least_the_rows_the_generator_measured() {
    for (flaw, measured) in FLAWS {
        let n = rejected(flaw);
        assert!(
            n >= measured,
            "{flaw:?}: {n} rejected rows, generator measured {measured}"
        );
    }
}

#[test]
fn mutant_dilating_the_image_affine_is_rejected_and_production_keeps_it_exact() {
    assert!(plan_rows(Flaw::ImageDilated).1.len() >= 111);
    for c in plan_cases() {
        let Ok(plan) = plan_mesh_draw(&c.input(), c.width, c.height, 1.0, &c.view) else {
            continue;
        };
        for t in &plan.triangles {
            let (a, b, cc, d, tx, ty) = (
                t.place[0], t.place[1], t.place[2], t.place[3], t.place[4], t.place[5],
            );
            let det = (t.source[1][0] - t.source[0][0]) * (t.source[2][1] - t.source[0][1])
                - (t.source[2][0] - t.source[0][0]) * (t.source[1][1] - t.source[0][1]);
            let finite = t.dest.iter().flatten().all(|x| x.is_finite())
                && t.place.iter().all(|x| x.is_finite());
            if !finite
                || !det.is_finite()
                || det.abs() < 1e-3
                || t.dilation == 1.0
                || t.dilation == 4.0
            {
                continue;
            }
            let ext = t.dest.iter().flatten().fold(1.0_f64, |m, x| m.max(x.abs()));
            if ext > 1e100 {
                continue;
            }
            // a dilated destination would put the first source vertex off `dest[0]`
            let at0 = [
                a * t.source[0][0] + cc * t.source[0][1] + tx,
                b * t.source[0][0] + d * t.source[0][1] + ty,
            ];
            assert!(
                (at0[0] - t.dest[0][0]).abs() <= 1e-9 * ext,
                "{} vertex 0",
                c.label
            );
            assert!(
                (at0[1] - t.dest[0][1]).abs() <= 1e-9 * ext,
                "{} vertex 0",
                c.label
            );
            assert_ne!(
                t.clip[0], t.dest[0],
                "{}: the clip, not the affine, carries the dilation",
                c.label
            );
        }
    }
}

#[test]
fn mutant_without_the_clamp_would_smear_a_sliver_and_production_does_not() {
    let verts = [[0.0, 0.0], [1000.0, 0.0], [500.0, 0.001]];
    let uvs = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
    let input = MeshPlanInput {
        verts: &verts,
        uvs: &uvs,
        tris: &[0, 1, 2],
        outline: &[0, 1, 2],
    };
    let view = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let real = plan_mesh_draw(&input, 8, 8, 1.0, &view).unwrap();
    let smeared = reference(Flaw::NoKClamp, &input, 8, 8, 1.0, &view).unwrap();
    // area2 = 1, longest edge 1000: k = 1 + 0.5 * 3 * 1000 / 1 = 1501 unclamped
    assert_eq!(real.triangles[0].dilation, 4.0);
    assert_eq!(smeared.triangles[0].dilation, 1501.0);
}

#[test]
fn mutant_bounding_box_fallback_removed_would_draw_nothing_and_production_draws() {
    let verts = [[0.0, 0.0], [10.0, 0.0], [0.0, 10.0]];
    let uvs = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
    let input = MeshPlanInput {
        verts: &verts,
        uvs: &uvs,
        tris: &[0, 1, 2],
        outline: &[],
    };
    let view = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    assert!(plan_mesh_draw(&input, 8, 8, 1.0, &view).is_ok());
    assert!(reference(Flaw::NoBbox, &input, 8, 8, 1.0, &view).is_err());
}

#[test]
fn mutant_ignoring_a_missing_outline_vertex_would_clip_a_smaller_polygon() {
    let verts = [[0.0, 0.0], [10.0, 0.0], [0.0, 10.0]];
    let uvs = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
    let input = MeshPlanInput {
        verts: &verts,
        uvs: &uvs,
        tris: &[0, 1, 2],
        outline: &[0, 1, 7, 2],
    };
    let view = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    assert_eq!(
        plan_mesh_draw(&input, 8, 8, 1.0, &view).unwrap_err(),
        PlanSkip::OutlineVertexMissing
    );
    let broken = reference(Flaw::OutlineMissingSkipped, &input, 8, 8, 1.0, &view).unwrap();
    assert_eq!(broken.outline.points.len(), 3);
}

#[test]
fn production_is_stateless_and_leaves_its_input_untouched() {
    for c in plan_cases().into_iter().take(40) {
        let (verts, uvs, tris, outline) = (
            c.verts.clone(),
            c.uvs.clone(),
            c.tris.clone(),
            c.outline.clone(),
        );
        let first = plan_mesh_draw(&c.input(), c.width, c.height, 0.5, &c.view);
        let second = plan_mesh_draw(&c.input(), c.width, c.height, 0.5, &c.view);
        assert_eq!(format!("{first:?}"), format!("{second:?}"), "{}", c.label);
        assert_eq!(format!("{:?}", c.verts), format!("{verts:?}"));
        assert_eq!(format!("{:?}", c.uvs), format!("{uvs:?}"));
        assert_eq!(c.tris, tris);
        assert_eq!(c.outline, outline);
    }
}

#[test]
fn production_dilation_matches_the_unflawed_reference_on_every_view() {
    for c in plan_cases() {
        let got = half_pixel_dilation(&c.view);
        let want = dilation(Flaw::Clean, &c.view);
        assert!(
            got.to_bits() == want.to_bits() || (got.is_nan() && want.is_nan()),
            "{}",
            c.label
        );
    }
}
