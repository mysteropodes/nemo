//! Hand-derived literal tests for the image-mesh draw plan (L03-H, F17/F18).
//!
//! Every expectation is derived on paper (the derivation sits beside it) or is
//! a number the baseline transcript printed for the same input, never a value
//! produced by the kernel under test. The oracle-backed and property tests are
//! in `render_image_mesh_plan.rs`.

use super::oracle_rows::{fixture_payload, plan_of, IDENT, SQUARE_UV};
use super::{
    half_pixel_dilation, plan_mesh_draw, MeshPlanInput, OutlineKind, PlanSkip, Point2,
    MAX_DILATION_FACTOR,
};

#[test]
fn unit_mapping_and_hand_derived_affines() {
    // verts = uv * 100 + (10, 20): the affine is a translation
    let verts = [[10.0, 20.0], [110.0, 20.0], [10.0, 120.0]];
    let uvs = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
    let plan = plan_of(&verts, &uvs, &[0, 1, 2], &[], (100, 100), &IDENT).unwrap();
    assert_eq!(plan.triangles[0].place, [1.0, 0.0, 0.0, 1.0, 10.0, 20.0]);
    // 90 degree turn: (100,0) -> (0,100) and (0,100) -> (-100,0) gives a=0 b=1 c=-1 d=0
    let verts = [[0.0, 0.0], [0.0, 100.0], [-100.0, 0.0]];
    let plan = plan_of(&verts, &uvs, &[0, 1, 2], &[], (100, 100), &IDENT).unwrap();
    assert_eq!(plan.triangles[0].place, [0.0, 1.0, -1.0, 0.0, 0.0, 0.0]);
    // non-square image: source x scales by 200, y by 100
    let verts = [[0.0, 0.0], [400.0, 0.0], [0.0, 100.0]];
    let plan = plan_of(&verts, &uvs, &[0, 1, 2], &[], (200, 100), &IDENT).unwrap();
    assert_eq!(plan.triangles[0].place, [2.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    assert_eq!(
        plan.triangles[0].source,
        [[0.0, 0.0], [200.0, 0.0], [0.0, 100.0]]
    );
}

#[test]
fn right_triangle_dilation_literals_from_the_baseline_transcript() {
    let verts = [[0.0, 0.0], [10.0, 0.0], [0.0, 10.0]];
    let uvs = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
    // identity view: dilate 0.5; area2 = 100; longest = sqrt(200); k = 1 + 0.5*3*sqrt(200)/100
    let plan = plan_of(&verts, &uvs, &[0, 1, 2], &[0, 1, 2], (10, 10), &IDENT).unwrap();
    let t = &plan.triangles[0];
    assert_eq!(t.dilation, 1.2121320343559643);
    let m = -0.7071067811865475; // = -0.5 * sqrt(2): each leg moves out by (k-1)*10/3
    assert_eq!(t.clip[0], [m, m]);
    assert_eq!(t.clip[1], [11.414213562373096, m]);
    assert_eq!(t.clip[2], [m, 11.414213562373096]);
    assert_eq!(t.dest, verts, "dest stays undilated");
    assert_eq!(t.place, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    // zoom 2 halves the world-unit dilation
    let zoom2 = [2.0, 0.0, 0.0, 2.0, 5.0, 5.0];
    let plan = plan_of(&verts, &uvs, &[0, 1, 2], &[0, 1, 2], (10, 10), &zoom2).unwrap();
    assert_eq!(plan.triangles[0].dilation, 1.1060660171779821);
    assert_eq!(
        plan.triangles[0].clip[0],
        [-0.35355339059327395, -0.35355339059327395]
    );
}

#[test]
fn view_scale_literals_and_the_floor() {
    assert_eq!(half_pixel_dilation(&IDENT), 0.5);
    assert_eq!(
        half_pixel_dilation(&[2.0, 0.0, 0.0, 2.0, -100.0, 40.0]),
        0.25
    );
    assert_eq!(
        half_pixel_dilation(&[-1.0, 0.0, 0.0, 1.0, 640.0, 0.0]),
        0.5,
        "|det| is used"
    );
    assert_eq!(half_pixel_dilation(&[2.0, 0.0, 0.0, 0.5, 0.0, 0.0]), 0.5);
    // a degenerate view floors the scale at 1e-9: 0.5 / 1e-9
    let floor = 499999999.99999994;
    assert_eq!(half_pixel_dilation(&[0.0; 6]), floor);
    assert_eq!(
        half_pixel_dilation(&[1e-10, 0.0, 0.0, 1e-10, 0.0, 0.0]),
        floor
    );
    assert_eq!(
        half_pixel_dilation(&[f64::NAN; 6]),
        floor,
        "f64::max ignores NaN"
    );
}

#[test]
fn incomplete_meshes_and_empty_images_draw_nothing_in_the_baseline_order() {
    let four = SQUARE_UV;
    let uv3 = &four[..3];
    let sq = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
    let size = (64, 64);
    let skip = |v: &[Point2], u: &[Point2], t: &[u32], o: &[u32], s: (u32, u32)| {
        plan_of(v, u, t, o, s, &IDENT).unwrap_err()
    };
    assert_eq!(
        skip(&sq[..2], &SQUARE_UV, &[0, 1, 0], &[], size),
        PlanSkip::IncompleteMesh
    );
    assert_eq!(
        skip(&sq, &SQUARE_UV, &[0, 1], &[], size),
        PlanSkip::IncompleteMesh
    );
    assert_eq!(
        skip(&sq, uv3, &[0, 1, 2], &[], size),
        PlanSkip::IncompleteMesh
    );
    assert_eq!(
        skip(&sq, &SQUARE_UV, &[0, 1, 2], &[], (0, 64)),
        PlanSkip::EmptyImage
    );
    assert_eq!(
        skip(&sq, &SQUARE_UV, &[0, 1, 2], &[], (64, 0)),
        PlanSkip::EmptyImage
    );
    assert_eq!(
        skip(&sq, &SQUARE_UV, &[0, 1, 2], &[0, 1, 9], size),
        PlanSkip::OutlineVertexMissing
    );
    // checks run mesh -> image -> outline
    assert_eq!(
        skip(&sq, uv3, &[0, 1, 2], &[0, 1, 9], (0, 0)),
        PlanSkip::IncompleteMesh
    );
    assert_eq!(
        skip(&sq, &SQUARE_UV, &[0, 1, 2], &[0, 1, 9], (0, 0)),
        PlanSkip::EmptyImage
    );
    // extra uvs are fine; an outline of two indices never looks the vertices up
    assert!(plan_of(
        &sq,
        &[SQUARE_UV.to_vec(), vec![[9.0, 9.0]]].concat(),
        &[0, 1, 2],
        &[7, 9],
        size,
        &IDENT
    )
    .is_ok());
}

#[test]
fn short_outline_falls_back_to_the_bounding_box_of_all_vertices() {
    let verts = [
        [5.0, 5.0],
        [50.0, 5.0],
        [50.0, 50.0],
        [5.0, 50.0],
        [-30.0, 200.0],
    ];
    let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.5, 0.5]];
    let tris = [0, 1, 2, 0, 2, 3];
    let short: [&[u32]; 2] = [&[], &[0, 1]];
    for outline in short {
        let plan = plan_of(&verts, &uvs, &tris, outline, (8, 8), &IDENT).unwrap();
        assert_eq!(plan.outline.kind, OutlineKind::BoundingBox);
        // kurbo Rect path order: (x0,y0) (x1,y0) (x1,y1) (x0,y1), over ALL five vertices
        assert_eq!(
            plan.outline.points,
            vec![[-30.0, 5.0], [50.0, 5.0], [50.0, 200.0], [-30.0, 200.0]]
        );
    }
    let three = plan_of(&verts, &uvs, &tris, &[2, 0, 1], (8, 8), &IDENT).unwrap();
    assert_eq!(three.outline.kind, OutlineKind::Polygon);
    assert_eq!(
        three.outline.points,
        vec![[50.0, 50.0], [5.0, 5.0], [50.0, 5.0]]
    );
    // a NaN coordinate is ignored by f64::min/max; all-NaN leaves f64::MAX / f64::MIN
    let nan = [[f64::NAN, 1.0], [3.0, f64::NAN], [2.0, 2.0]];
    let plan = plan_of(&nan, &uvs[..3], &[0, 1, 2], &[], (8, 8), &IDENT).unwrap();
    assert_eq!(
        plan.outline.points,
        vec![[2.0, 1.0], [3.0, 1.0], [3.0, 2.0], [2.0, 2.0]]
    );
    let all = [[f64::NAN; 2]; 3];
    let plan = plan_of(&all, &uvs[..3], &[0, 1, 2], &[], (8, 8), &IDENT).unwrap();
    assert_eq!(plan.outline.points[0], [f64::MAX, f64::MAX]);
    assert_eq!(plan.outline.points[2], [f64::MIN, f64::MIN]);
}

#[test]
fn bad_and_partial_triples_are_skipped_but_ordinals_follow_the_index_list() {
    let sq = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
    let ordinals = |tris: &[u32]| -> Vec<usize> {
        plan_of(&sq, &SQUARE_UV, tris, &[0, 1, 2, 3], (64, 64), &IDENT)
            .unwrap()
            .triangles
            .iter()
            .map(|t| t.triangle)
            .collect()
    };
    assert_eq!(ordinals(&[0, 1, 2, 0, 2, 3]), vec![0, 1]);
    assert_eq!(
        ordinals(&[0, 1, 2, 0, 1, 4, 0, 2, 3]),
        vec![0, 2],
        "index == n is out of range"
    );
    assert_eq!(ordinals(&[0, 1, 2, u32::MAX, 1, 2]), vec![0]);
    assert_eq!(
        ordinals(&[0, 1, 2, 0]),
        vec![0],
        "a trailing partial triple is ignored"
    );
    assert_eq!(ordinals(&[0, 1, 2, 0, 2, 3, 1]), vec![0, 1]);
    assert_eq!(
        ordinals(&[0, 0, 1, 0, 1, 2]),
        vec![1],
        "a repeated vertex has no source area"
    );
    assert_eq!(ordinals(&[0, 1, 2, 3, 3, 3]), vec![0]);
}

#[test]
fn source_determinant_and_destination_area_thresholds() {
    let verts = [[0.0, 0.0], [10.0, 0.0], [0.0, 10.0]];
    let draws = |u1: f64, v2: f64| {
        let uvs = [[0.0, 0.0], [u1, 0.0], [0.0, v2]];
        plan_of(&verts, &uvs, &[0, 1, 2], &[], (1, 1), &IDENT)
            .unwrap()
            .triangles
            .len()
    };
    assert_eq!(draws(1e-6, 0.9e-6), 0, "|det| 9e-13 < 1e-12 has no map");
    assert_eq!(draws(1e-6, 1.1e-6), 1, "|det| 1.1e-12 does");
    assert_eq!(draws(0.0, 1.0), 0);
    let uvs = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
    let k = |d1: f64, d2: f64| {
        let v = [[0.0, 0.0], [d1, 0.0], [0.0, d2]];
        plan_of(&v, &uvs, &[0, 1, 2], &[], (1, 1), &IDENT)
            .unwrap()
            .triangles[0]
            .dilation
    };
    assert_eq!(k(0.9e-6, 1e-6), 1.0, "area2 9e-13 <= 1e-12: not dilated");
    assert_eq!(
        k(1.1e-6, 1e-6),
        MAX_DILATION_FACTOR,
        "a sliver is clamped, not smeared"
    );
    let line = [[0.0, 0.0], [10.0, 0.0], [20.0, 0.0]];
    let p = plan_of(&line, &uvs, &[0, 1, 2], &[], (1, 1), &IDENT).unwrap();
    assert_eq!(
        p.triangles[0].dilation, 1.0,
        "collinear destination keeps k = 1"
    );
    assert_eq!(
        p.triangles[0].clip, line,
        "and a clip equal to the destination"
    );
}

#[test]
fn nan_determinant_is_not_skipped_as_in_the_baseline() {
    let verts = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
    let uvs = [[0.0, 0.0], [f64::NAN, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let plan = plan_of(&verts, &uvs, &[0, 1, 2], &[], (8, 8), &IDENT).unwrap();
    assert_eq!(plan.triangles.len(), 1);
    assert!(plan.triangles[0].place.iter().any(|x| x.is_nan()));
    // area2 = 10000, longest edge sqrt(20000): k = 1 + 1.5 * 141.4213562373095 / 10000
    assert!((plan.triangles[0].dilation - 1.0212132034355964).abs() < 1e-15);
}

#[test]
fn opacity_is_carried_once_by_the_plan_bit_for_bit() {
    let verts = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
    for op in [1.0_f32, 0.5, 0.0, 0.3, f32::from_bits(1), 1.5, -0.25] {
        let one = {
            let i = MeshPlanInput {
                verts: &verts,
                uvs: &SQUARE_UV,
                tris: &[0, 1, 2],
                outline: &[0, 1, 2, 3],
            };
            plan_mesh_draw(&i, 4, 4, op, &IDENT).unwrap()
        };
        let two = {
            let i = MeshPlanInput {
                verts: &verts,
                uvs: &SQUARE_UV,
                tris: &[0, 1, 2, 0, 2, 3],
                outline: &[0, 1, 2, 3],
            };
            plan_mesh_draw(&i, 4, 4, op, &IDENT).unwrap()
        };
        assert_eq!(one.opacity.to_bits(), op.to_bits());
        assert_eq!(two.opacity.to_bits(), op.to_bits());
        assert_eq!(one.triangles.len() + 1, two.triangles.len());
    }
}

#[test]
fn scene_and_export_callers_get_the_same_geometry() {
    let payload = fixture_payload(0.1, 0.05, 0.0);
    let input = MeshPlanInput::from_payload(&payload);
    let view = [1.25, 0.0, 0.0, 1.25, 7.0, -3.0];
    let screen = plan_mesh_draw(&input, 200, 200, 0.5, &view).unwrap();
    let export = plan_mesh_draw(&input, 200, 200, 1.0, &view).unwrap();
    let again = plan_mesh_draw(&input, 200, 200, 0.5, &view).unwrap();
    assert_eq!(
        screen, again,
        "pure: the same payload, size and view give the same plan"
    );
    assert_eq!(screen.outline, export.outline);
    assert_eq!(
        screen.triangles, export.triangles,
        "opacity never reaches a triangle"
    );
    assert_ne!(screen.opacity, export.opacity);
}

#[test]
fn from_payload_borrows_the_four_lists_without_copying() {
    let payload = fixture_payload(0.0, 0.0, 0.0);
    let input = MeshPlanInput::from_payload(&payload);
    assert!(std::ptr::eq(input.verts.as_ptr(), payload.verts.as_ptr()));
    assert!(std::ptr::eq(input.uvs.as_ptr(), payload.uvs.as_ptr()));
    assert!(std::ptr::eq(input.tris.as_ptr(), payload.tris.as_ptr()));
    assert!(std::ptr::eq(
        input.outline.as_ptr(),
        payload.outline.as_ptr()
    ));
    assert_eq!(
        payload.outline,
        (0..8).collect::<Vec<u32>>(),
        "outline is the vertex prefix"
    );
}

#[test]
fn plan_is_detached_from_the_input_it_was_built_from() {
    let mut verts = vec![[0.0, 0.0], [10.0, 0.0], [0.0, 10.0]];
    let uvs = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
    let before = plan_of(&verts, &uvs, &[0, 1, 2], &[0, 1, 2], (10, 10), &IDENT).unwrap();
    let kept = before.clone();
    verts[1] = [999.0, 999.0];
    assert_eq!(before, kept);
    let after = plan_of(&verts, &uvs, &[0, 1, 2], &[0, 1, 2], (10, 10), &IDENT).unwrap();
    assert_ne!(after, kept);
}
