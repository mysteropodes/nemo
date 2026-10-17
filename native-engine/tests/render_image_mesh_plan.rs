//! Oracle-backed and property tests for the image-mesh draw plan (L03-H, F17/F18).
//!
//! The oracle rows and their comparison harness live in
//! `render_image_mesh_plan_rows.rs`; the hand-derived literal tests in
//! `render_image_mesh_plan_literals.rs`; the mutation and guard controls in
//! `render_image_mesh_plan_controls.rs`.

use super::oracle_rows::{
    dilation_run, distance, fixture_payload, fixture_run, plan_cases, plan_run, production, Run,
    IDENT,
};
use super::{
    half_pixel_dilation, plan_mesh_draw, MeshPlanInput, OutlineKind, PlanSkip, Point2,
    MAX_DILATION_FACTOR,
};

fn assert_clean(family: &str, run: Run, expected_rows: usize) {
    assert!(
        run.1.is_empty(),
        "{family}: {} rows differ:\n{}",
        run.1.len(),
        run.1.join("\n")
    );
    assert_eq!(run.0, expected_rows, "{family} row count");
}

/// Distance from `p` to the infinite line through `a` and `b`.
fn line_distance(p: Point2, a: Point2, b: Point2) -> f64 {
    let cross = (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
    cross.abs() / distance(a, b)
}

#[test]
fn oracle_plan_rows_match_baseline() {
    assert_clean("plan", plan_run(&production), 144);
}

#[test]
fn oracle_fixture_chain_rows_match_baseline_through_the_l03a_payload() {
    assert_clean("fx", fixture_run(&production), 10);
}

#[test]
fn oracle_dilation_rows_match_baseline() {
    assert_clean("dil", dilation_run(&half_pixel_dilation), 16);
}

#[test]
fn oracle_is_not_vacuous() {
    let mut skips = [0usize; 3];
    let (mut polygons, mut boxes, mut drawn, mut clamped, mut undilated) = (0, 0, 0, 0, 0);
    for c in plan_cases() {
        match plan_mesh_draw(&c.input(), c.width, c.height, 1.0, &c.view) {
            Err(PlanSkip::IncompleteMesh) => skips[0] += 1,
            Err(PlanSkip::EmptyImage) => skips[1] += 1,
            Err(PlanSkip::OutlineVertexMissing) => skips[2] += 1,
            Ok(plan) => {
                match plan.outline.kind {
                    OutlineKind::Polygon => polygons += 1,
                    OutlineKind::BoundingBox => boxes += 1,
                }
                for t in &plan.triangles {
                    drawn += 1;
                    if t.dilation == MAX_DILATION_FACTOR {
                        clamped += 1;
                    }
                    if t.dilation == 1.0 {
                        undilated += 1;
                    }
                }
            }
        }
    }
    // exact corpus shape, taken from the decoded oracle rows
    assert_eq!(
        skips,
        [9, 4, 6],
        "incomplete / empty image / outline vertex"
    );
    assert_eq!((polygons, boxes), (85, 40));
    assert_eq!(drawn, 566, "triangles drawn across the plan rows");
    assert_eq!((clamped, undilated), (169, 15), "k = 4 and k = 1 triangles");
}

#[test]
fn place_maps_every_source_vertex_onto_its_destination_within_1e_9() {
    let mut checked = 0;
    for c in plan_cases() {
        let Ok(plan) = plan_mesh_draw(&c.input(), c.width, c.height, 1.0, &c.view) else {
            continue;
        };
        for t in &plan.triangles {
            let e1 = (
                t.source[1][0] - t.source[0][0],
                t.source[1][1] - t.source[0][1],
            );
            let e2 = (
                t.source[2][0] - t.source[0][0],
                t.source[2][1] - t.source[0][1],
            );
            let det = e1.0 * e2.1 - e2.0 * e1.1;
            let scale: f64 = t
                .dest
                .iter()
                .flatten()
                .fold(1.0, |m: f64, x| m.max(x.abs()));
            let finite = t.dest.iter().flatten().all(|x| x.is_finite())
                && t.place.iter().all(|x| x.is_finite());
            if !finite || !det.is_finite() || det.abs() < 1e-3 || scale > 1e100 {
                continue;
            }
            let [a, b, cc, d, tx, ty] = t.place;
            for i in 0..3 {
                let (sx, sy) = (t.source[i][0], t.source[i][1]);
                let mapped = [a * sx + cc * sy + tx, b * sx + d * sy + ty];
                let tol = 1e-9 * scale.max(sx.abs()).max(sy.abs());
                assert!(
                    distance(mapped, t.dest[i]) <= tol,
                    "{} tri {} vertex {i}: {mapped:?} vs {:?}",
                    c.label,
                    t.triangle,
                    t.dest[i]
                );
                checked += 1;
            }
        }
    }
    assert!(checked >= 1500, "{checked} vertex checks");
}

#[test]
fn dilation_scales_about_the_centroid_and_moves_every_edge_out_by_half_a_pixel() {
    let mut checked = 0;
    for c in plan_cases() {
        let Ok(plan) = plan_mesh_draw(&c.input(), c.width, c.height, 1.0, &c.view) else {
            continue;
        };
        let dilate = half_pixel_dilation(&c.view);
        for t in &plan.triangles {
            let all_finite = t
                .dest
                .iter()
                .chain(&t.clip)
                .flatten()
                .all(|x| x.is_finite());
            let extent = t.dest.iter().flatten().fold(1.0_f64, |m, x| m.max(x.abs()));
            if !all_finite || t.dilation >= MAX_DILATION_FACTOR || t.dilation == 1.0 {
                continue;
            }
            if !dilate.is_finite() || extent > 1e6 {
                continue;
            }
            let g = [
                (t.dest[0][0] + t.dest[1][0] + t.dest[2][0]) / 3.0,
                (t.dest[0][1] + t.dest[1][1] + t.dest[2][1]) / 3.0,
            ];
            let cg = [
                (t.clip[0][0] + t.clip[1][0] + t.clip[2][0]) / 3.0,
                (t.clip[0][1] + t.clip[1][1] + t.clip[2][1]) / 3.0,
            ];
            assert!(
                distance(g, cg) <= 1e-9 * extent,
                "{} centroid moved",
                c.label
            );
            let mut moved = Vec::new();
            for e in 0..3 {
                let (a, b) = (e, (e + 1) % 3);
                moved.push(line_distance(t.clip[a], t.dest[a], t.dest[b]));
            }
            // the clip is a scaled copy: each edge is at least `dilate` outside the original,
            // and the longest edge (the binding one) is outside by exactly `dilate`
            let lens: Vec<f64> = (0..3)
                .map(|e| distance(t.dest[e], t.dest[(e + 1) % 3]))
                .collect();
            let longest = (0..3).fold(0, |best, e| if lens[e] > lens[best] { e } else { best });
            let tol = 1e-8 * extent.max(dilate);
            assert!(
                moved.iter().all(|m| *m >= dilate - tol),
                "{} edge {moved:?}",
                c.label
            );
            assert!(
                (moved[longest] - dilate).abs() <= tol,
                "{} longest {moved:?}",
                c.label
            );
            checked += 1;
        }
    }
    assert!(checked >= 300, "{checked} unclamped dilated triangles");
}

#[test]
fn fixture_centre_fan_moves_twenty_pixels_while_sources_stay_put() {
    let rest = fixture_payload(0.0, 0.0, 0.0);
    let moved = fixture_payload(0.1, 0.0, 0.0);
    let view = IDENT;
    let a = plan_mesh_draw(&MeshPlanInput::from_payload(&rest), 200, 200, 1.0, &view).unwrap();
    let b = plan_mesh_draw(&MeshPlanInput::from_payload(&moved), 200, 200, 1.0, &view).unwrap();
    assert_eq!(a.triangles.len(), 8);
    assert_eq!(b.triangles.len(), 8);
    for (ra, rb) in a.triangles.iter().zip(&b.triangles) {
        // every triangle is [centre, i, i+1]: slot 0 is the centre vertex
        assert_eq!(ra.dest[0], [420.0, 280.0]);
        assert_eq!(rb.dest[0], [440.0, 280.0]);
        assert_eq!(rb.dest[0][0] - ra.dest[0][0], 20.0);
        assert_eq!(ra.dest[1], rb.dest[1]);
        assert_eq!(ra.dest[2], rb.dest[2]);
        assert_eq!(ra.source, rb.source, "uvs sample the REST position");
        assert_ne!(ra.place, rb.place);
        assert_ne!(ra.clip[0], rb.clip[0]);
    }
    assert_eq!(a.outline, b.outline, "only vertex 8 moved, the mask stays");
    assert_eq!(a.outline.kind, OutlineKind::Polygon);
    assert_eq!(a.outline.points[0], [320.0, 180.0]);
    assert_eq!(a.outline.points[4], [520.0, 380.0]);
    // rest sources are the 200x200 image pixels of the uvs: centre (100,100), corner (0,0)
    assert_eq!(a.triangles[0].source[0], [100.0, 100.0]);
    assert_eq!(a.triangles[0].source[1], [0.0, 0.0]);
}
