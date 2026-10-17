//! Geometry-level pixel tests for the image-mesh scene join (L03-S, acceptance bullets 1 and 5).
//!
//! No GPU: the CPU reference in `raster` answers which source coordinate the plan paints at a world
//! point (outline mask, painter order, inverse image affine). It proves where pixels land and what
//! is masked; the vello coverage of the same plan (the baseline's "52 px of 2 073 600") is measured
//! by the compositor leaf. Image sizes are chosen so a source texel is a whole number of world
//! units and every sample sits half a texel from a texel boundary, so rounding of the stored
//! coordinates (R2) can never flip a texel.

use super::oracle_rows::{geometry_for, mesh_rows, plain_source, prepare_row, row_named};
use super::plan_draw;
use super::raster::{mesh_source_at, pixel_centers, plain_source_at, texel};
use super::{MeshDraw, MeshGeometry, MeshSceneSource};
use crate::evaluation::motion_parent_chain::ImageRect;
use crate::evaluation::object_image_mesh_kernels::render_image_mesh_plan::{
    MeshDrawPlan, OutlineKind,
};

const FAN_8: &str = r#"{"outline":[[0,0],[0.5,0],[1,0],[1,0.5],[1,1],[0.5,1],[0,1],[0,0.5]],"verts":[[0,0],[0.5,0],[1,0],[1,0.5],[1,1],[0.5,1],[0,1],[0,0.5],[0.5,0.5]],"tris":[8,0,1,8,1,2,8,2,3,8,3,4,8,4,5,8,5,6,8,6,7,8,7,0],"offsets":[[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0]],"cols":2,"rows":2}"#;
const DIAMOND: &str = r#"{"outline":[[0.5,0],[1,0.5],[0.5,1],[0,0.5]],"verts":[[0.5,0],[1,0.5],[0.5,1],[0,0.5],[0.5,0.5]],"tris":[4,0,1,4,1,2,4,2,3,4,3,0],"offsets":[[0,0],[0,0],[0,0],[0,0],[0,0]],"cols":1,"rows":1}"#;
const SHORT_OUTLINE: &str = r#"{"outline":[[0,0],[1,0]],"verts":[[0,0],[1,0],[1,1],[0,1],[0.5,0.5]],"tris":[4,0,1,4,1,2,4,2,3,4,3,0],"offsets":[[0,0],[0,0],[0,0],[0,0],[0,0]],"cols":1,"rows":1}"#;

fn rect(x: f64, y: f64, w: f64, h: f64) -> ImageRect {
    ImageRect {
        x,
        y,
        width: w,
        height: h,
        rotation: 0.0,
    }
}

fn geometry(
    base: ImageRect,
    image: (u32, u32),
    source: &MeshSceneSource<'_>,
    mesh: &str,
) -> MeshGeometry {
    geometry_for(base, image, 1.0, source, mesh)
        .unwrap()
        .unwrap()
}

/// (painted samples, samples whose painted texel differs from the plain image's) over a pixel box.
fn compare_with_plain(
    plan: &MeshDrawPlan,
    base: &ImageRect,
    image: (u32, u32),
    window: (i64, i64, i64, i64),
) -> (usize, usize) {
    let (mut painted, mut mismatches) = (0, 0);
    for p in pixel_centers(window.0, window.1, window.2, window.3) {
        let mesh = mesh_source_at(plan, p).map(texel);
        let plain = plain_source_at((base.x, base.y, base.width, base.height), image, p).map(texel);
        painted += usize::from(mesh.is_some());
        mismatches += usize::from(mesh != plain);
    }
    (painted, mismatches)
}

#[test]
fn a_rest_mesh_paints_exactly_the_texels_of_the_plain_image() {
    let base = rect(320.0, 180.0, 200.0, 200.0);
    let g = geometry(base, (50, 50), &plain_source("im_1"), FAN_8);
    let plan = g.plan().unwrap();
    assert_eq!(plan.outline.kind, OutlineKind::Polygon);
    let (painted, mismatches) = compare_with_plain(plan, &base, (50, 50), (300, 160, 540, 400));
    assert_eq!(
        painted,
        200 * 200,
        "every pixel of the rect is painted, none outside it"
    );
    assert_eq!(mismatches, 0, "a rest mesh is the plain image");
}

#[test]
fn the_deformed_vertex_pixels_land_at_the_offset_position() {
    let rows = mesh_rows();
    let (_, _, g) = prepare_row(&rows[row_named(&rows, "corpus_stored_offset")]);
    let plan = g.plan().unwrap();
    let at_offset = mesh_source_at(plan, [440.0, 280.0]).expect("painted");
    assert!(
        (at_offset[0] - 32.0).abs() < 1e-6 && (at_offset[1] - 32.0).abs() < 1e-6,
        "{at_offset:?}"
    );
    let at_rest = mesh_source_at(plan, [420.0, 280.0]).expect("painted");
    assert!(
        (at_rest[0] - 32.0).abs() > 5.0,
        "the rest position now shows other pixels: {at_rest:?}"
    );
    // left of the vertex the picture is compressed, right of it stretched (hand-derived from the fan)
    assert!(
        (at_rest[0] - 80.0 / 3.0).abs() < 1e-6 && (at_rest[1] - 32.0).abs() < 1e-6,
        "{at_rest:?}"
    );
    let right = mesh_source_at(plan, [480.0, 280.0]).unwrap();
    assert!(
        (right[0] - 48.0).abs() < 1e-6 && (right[1] - 32.0).abs() < 1e-6,
        "{right:?}"
    );
}

#[test]
fn every_triangle_centroid_paints_its_own_source_centroid_for_any_view() {
    let rows = mesh_rows();
    let row = &rows[row_named(&rows, "corpus_stored_offset")];
    let (_, op, g) = prepare_row(row);
    for view in [
        [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        [2.0, 0.0, 0.0, 2.0, 40.0, 10.0],
        [0.25, 0.0, 0.0, 0.25, 0.0, 0.0],
    ] {
        let MeshDraw::Plan(plan) = plan_draw(&g.payload, row.image, op as f32, &view) else {
            panic!("draws");
        };
        for tri in &plan.triangles {
            let centroid = |p: &[[f64; 2]; 3]| {
                [
                    (p[0][0] + p[1][0] + p[2][0]) / 3.0,
                    (p[0][1] + p[1][1] + p[2][1]) / 3.0,
                ]
            };
            let (dest, source) = (centroid(&tri.dest), centroid(&tri.source));
            let got = mesh_source_at(&plan, dest).expect("centroid is painted");
            assert!(
                (got[0] - source[0]).abs() < 1e-9 && (got[1] - source[1]).abs() < 1e-9,
                "{view:?} {got:?} {source:?}"
            );
        }
    }
}

#[test]
fn outline_vertices_are_the_mask_and_pixels_outside_the_outline_are_not_painted() {
    let base = rect(100.0, 100.0, 100.0, 100.0);
    let g = geometry(base, (50, 50), &plain_source("im_1"), DIAMOND);
    let plan = g.plan().unwrap();
    let (painted, mismatches) = compare_with_plain(plan, &base, (50, 50), (90, 90, 210, 210));
    assert!(
        (4900..=5100).contains(&painted),
        "the diamond is half the rect: {painted}"
    );
    assert_eq!(
        mismatches,
        10_000 - painted,
        "outside the diamond the plain image paints and the mesh does not"
    );
    assert!(
        mesh_source_at(plan, [101.5, 101.5]).is_none(),
        "the rect corner is masked"
    );
    assert_eq!(
        mesh_source_at(plan, [150.5, 150.5]).map(texel),
        plain_source_at((100.0, 100.0, 100.0, 100.0), (50, 50), [150.5, 150.5]).map(texel)
    );

    // moving the TOP outline vertex to the centre moves the mask with it
    let mut offsets = vec![[0.0, 0.0]; 5];
    offsets[0] = [0.0, 0.5];
    let source = MeshSceneSource {
        offsets: Some(&offsets),
        ..plain_source("im_1")
    };
    let moved = geometry(base, (50, 50), &source, DIAMOND);
    let moved_plan = moved.plan().unwrap();
    assert_eq!(moved_plan.outline.points[0], [150.0, 150.0]);
    assert!(
        mesh_source_at(plan, [150.5, 110.5]).is_some(),
        "inside the rest diamond"
    );
    assert!(
        mesh_source_at(moved_plan, [150.5, 110.5]).is_none(),
        "outside once the outline vertex moved"
    );
    assert!(
        mesh_source_at(moved_plan, [150.5, 190.5]).is_some(),
        "the lower half is still inside"
    );
}

#[test]
fn a_short_outline_masks_by_the_bounding_box_of_all_vertices() {
    let base = rect(100.0, 100.0, 50.0, 50.0);
    let g = geometry(base, (25, 25), &plain_source("im_1"), SHORT_OUTLINE);
    let plan = g.plan().unwrap();
    assert_eq!(plan.outline.kind, OutlineKind::BoundingBox);
    assert_eq!(
        plan.outline.points,
        vec![
            [100.0, 100.0],
            [150.0, 100.0],
            [150.0, 150.0],
            [100.0, 150.0]
        ]
    );
    let (painted, mismatches) = compare_with_plain(plan, &base, (25, 25), (90, 90, 160, 160));
    assert_eq!((painted, mismatches), (2500, 0));
}

#[test]
fn an_animated_pose_moves_the_pixels_like_a_stored_offset_does() {
    let base = rect(320.0, 180.0, 200.0, 200.0);
    let mut pose: Vec<Option<[f64; 2]>> = vec![None; 9];
    pose[8] = Some([0.1, 0.0]);
    let source = MeshSceneSource {
        pose: Some(&pose),
        ..plain_source("im_1")
    };
    let g = geometry(base, (64, 64), &source, FAN_8);
    let at = mesh_source_at(g.plan().unwrap(), [440.0, 280.0]).unwrap();
    assert!(
        (at[0] - 32.0).abs() < 1e-6 && (at[1] - 32.0).abs() < 1e-6,
        "{at:?}"
    );
}
