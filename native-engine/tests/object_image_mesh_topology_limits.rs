//! Documented v1 limits, typed refusals, purity and the geometry join of the image-mesh
//! topology kernel (L03-G, F17). Oracle and basic literal tests are in the sibling file.

use super::oracle_rows::{near, opts, rows, square_mesh, tri_area_sum, L_SHAPE, SLOT_C};
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_delaunay::DelaunayError;
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_geometry::{
    point_in_poly, scene_payload, MeshGeometryError, MeshRect, MeshView,
};
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_topology::{
    rebuild, set_outline, signed_area2, triangulate, DensityOpts, TopologyError,
};

#[test]
fn concave_l_outline_pins_the_exact_surviving_triangles() {
    let m = square_mesh(2, 2);
    let out = set_outline(&m, &L_SHAPE, DensityOpts::default())
        .unwrap()
        .unwrap();
    assert_eq!((out.outline.len(), out.verts.len()), (14, 14));
    assert_eq!(
        out.tris,
        vec![
            5, 4, 6, 6, 2, 7, 12, 10, 8, 8, 10, 9, 4, 2, 6, 7, 12, 8, 13, 12, 7, 1, 13, 7, 2, 1, 7,
            4, 3, 2, 12, 11, 10, 1, 0, 13
        ]
    );
    assert!(near(tri_area_sum(&out), 0.75), "the L is fully covered");
}

#[test]
fn deeply_concave_outline_loses_a_sliver_the_documented_v1_limit() {
    let m = square_mesh(2, 2);
    let out = set_outline(&m, &SLOT_C, opts(Some(1.0), Some(1.0)))
        .unwrap()
        .unwrap();
    let covered = tri_area_sum(&out);
    let polygon = signed_area2(&SLOT_C).abs() / 2.0;
    let (_, want_covered, want_polygon) = rows().lost[0].clone();
    assert!(
        near(covered, want_covered) && near(covered, 0.832),
        "{covered}"
    );
    assert!(
        near(polygon, want_polygon) && near(polygon, 0.8656),
        "{polygon}"
    );
    assert!(
        polygon - covered > 0.03,
        "unconstrained Delaunay drops a sliver"
    );
    for t in out.tris.chunks_exact(3) {
        let c = |k: usize| out.verts[t[k] as usize];
        let (cx, cy) = (
            (c(0)[0] + c(1)[0] + c(2)[0]) / 3.0,
            (c(0)[1] + c(1)[1] + c(2)[1]) / 3.0,
        );
        assert!(
            point_in_poly(&out.outline, cx, cy),
            "a kept triangle has an inside centroid"
        );
    }
}

#[test]
fn triangulate_filters_by_centroid_and_short_inputs_short_circuit() {
    let grid: Vec<[f64; 2]> = (0..9)
        .map(|k| [f64::from(k % 3) / 2.0, f64::from(k / 3) / 2.0])
        .collect();
    let full = triangulate(&grid, grid.len()).unwrap();
    assert!(!full.is_empty() && full.len() % 3 == 0);
    assert!(
        triangulate(&grid, 0).unwrap().is_empty(),
        "empty outline keeps nothing"
    );
    let nan_pair = [[f64::NAN, 0.0], [0.0, 0.0]];
    assert_eq!(
        triangulate(&nan_pair, 2),
        Ok(Vec::new()),
        "fewer than three points never reach Delaunay"
    );
    let line = [[0.0, 0.0], [1.0, 1.0], [2.0, 2.0], [3.0, 3.0]];
    assert_eq!(
        triangulate(&line, 4),
        Ok(Vec::new()),
        "collinear points give no triangles"
    );
}

#[test]
fn unfinishable_or_non_finite_inputs_are_typed_refusals() {
    let m = square_mesh(2, 2);
    assert_eq!(
        set_outline(&m, &L_SHAPE, opts(Some(f64::INFINITY), None)),
        Err(TopologyError::Densify(MeshGeometryError::DensifyUnbounded))
    );
    let bad = [[0.0, 0.0], [1.0, 0.0], [f64::NAN, 1.0]];
    assert_eq!(
        triangulate(&bad, 3),
        Err(TopologyError::Triangulation(
            DelaunayError::NonFiniteCoordinate { point: 2 }
        ))
    );
    assert_eq!(
        set_outline(&m, &bad, DensityOpts::default()),
        Err(TopologyError::Triangulation(
            DelaunayError::NonFiniteCoordinate { point: 4 }
        ))
    );
    let mut broken = m.clone();
    broken.outline[3] = [f64::INFINITY, 0.5];
    assert_eq!(
        rebuild(&broken),
        Err(TopologyError::Triangulation(
            DelaunayError::NonFiniteCoordinate { point: 3 }
        ))
    );
    assert_eq!(
        m,
        square_mesh(2, 2),
        "a refusal leaves the stored mesh as it was"
    );
}

#[test]
fn results_are_detached_and_inputs_are_never_mutated() {
    let mut m = square_mesh(2, 2);
    m.offsets[8] = [0.1, 0.0];
    let before = m.clone();
    let mut out = set_outline(&m, &L_SHAPE, opts(Some(3.0), Some(3.0)))
        .unwrap()
        .unwrap();
    out.outline[0] = [9.0, 9.0];
    out.verts[0] = [9.0, 9.0];
    out.offsets[0] = [9.0, 9.0];
    assert_eq!(m, before);
    let again = set_outline(&m, &L_SHAPE, opts(Some(3.0), Some(3.0)))
        .unwrap()
        .unwrap();
    assert_ne!(again, out);
    assert_eq!(
        rebuild(&m).unwrap(),
        rebuild(&m).unwrap(),
        "pure: same input, same output"
    );
}

#[test]
fn the_created_topology_feeds_the_geometry_kernel_scene_payload_unchanged() {
    let m = square_mesh(2, 2);
    let view = MeshView {
        outline: &m.outline,
        verts: &m.verts,
        offsets: &m.offsets,
        tris: &m.tris,
    };
    let rect = MeshRect {
        x: 0.0,
        y: 0.0,
        width: 200.0,
        height: 100.0,
        rotation: 0.0,
    };
    let payload = scene_payload(&view, &rect, None).expect("a mesh with triangles");
    assert_eq!(payload.tris, m.tris);
    assert_eq!(payload.outline, (0..8).collect::<Vec<u32>>());
    let want = [
        [0.0, 0.0],
        [100.0, 0.0],
        [200.0, 0.0],
        [200.0, 50.0],
        [200.0, 100.0],
        [100.0, 100.0],
        [0.0, 100.0],
        [0.0, 50.0],
        [100.0, 50.0],
    ];
    assert_eq!(payload.verts, want.to_vec());
    assert_eq!(payload.uvs[8], [0.5, 0.5]);
}
