//! Oracle-backed and literal tests for the image-mesh geometry kernel (L03-A, F17/F18).
//!
//! The oracle rows and their comparison harness live in
//! `object_image_mesh_geometry_rows.rs`; the mutation and guard controls in
//! `object_image_mesh_geometry_controls.rs`. Literal expectations below are
//! hand-derived or copied from decoded oracle rows, never produced by the
//! kernel under test.

use super::oracle_rows::{
    densify_run, hex, ints, normalized_run, pip_run, point, points, resample_run, round_run, rows,
    same_pts, scene_run, world_run, OwnedMesh, Run, ORACLE,
};
use super::{
    densify, normalized_of, point_in_poly, r2, r5, resample_offsets, scene_payload, world_verts,
    MeshRect,
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

#[test]
fn oracle_round_rows_match_baseline() {
    let f = |mode: u8, x: f64| if mode == 2 { r2(x) } else { r5(x) };
    assert_clean("R2/R5", round_run(&f), 108);
}

#[test]
fn oracle_point_in_poly_rows_match_baseline() {
    assert_clean("pointInPoly", pip_run(&point_in_poly), 228);
}

#[test]
fn oracle_densify_rows_match_baseline() {
    assert_clean("densify", densify_run(&densify), 81);
}

#[test]
fn oracle_world_verts_rows_match_baseline() {
    assert_clean("worldVerts", world_run(&world_verts), 71);
}

#[test]
fn oracle_scene_payload_rows_match_baseline() {
    assert_clean("scenePayloadFor", scene_run(&scene_payload), 71);
}

#[test]
fn oracle_normalized_of_rows_match_baseline() {
    assert_clean("normalizedOf", normalized_run(&normalized_of), 119);
}

#[test]
fn oracle_resample_rows_match_baseline_and_production_rebuild() {
    assert_clean("resampleOffsets", resample_run(&resample_offsets), 19);
}

#[test]
fn oracle_row_counts_are_complete() {
    let counts = [
        ("rnd", 108),
        ("pip", 228),
        ("den", 81),
        ("wv", 71),
        ("sp", 71),
        ("nz", 119),
        ("ref", 8),
        ("rs", 16),
        ("rb", 3),
        ("mesh", 5),
    ];
    for (op, n) in counts {
        assert_eq!(rows(op).count(), n, "{op} rows");
    }
    let total: usize = counts.iter().map(|(_, n)| n).sum();
    assert_eq!(ORACLE.lines().count(), total, "no unknown oracle op");
}

// ---- fixture mesh: outline8 / verts9 / triangles8, offsets[8] = [0.1, 0] ----

const R0: MeshRect = MeshRect {
    x: 320.0,
    y: 180.0,
    width: 200.0,
    height: 200.0,
    rotation: 0.0,
};
pub(crate) const OUTLINE8: [[f64; 2]; 8] = [
    [0.0, 0.0],
    [0.5, 0.0],
    [1.0, 0.0],
    [1.0, 0.5],
    [1.0, 1.0],
    [0.5, 1.0],
    [0.0, 1.0],
    [0.0, 0.5],
];

/// The corpus mesh: 2x2 grid, a fan of 8 triangles around the centre vertex.
pub(crate) fn fixture_mesh(offset8: [f64; 2]) -> OwnedMesh {
    let mut verts = OUTLINE8.to_vec();
    verts.push([0.5, 0.5]);
    let mut tris = Vec::new();
    for i in 0..8u32 {
        tris.extend([8, i, (i + 1) % 8]);
    }
    let mut offsets = vec![[0.0, 0.0]; 9];
    offsets[8] = offset8;
    OwnedMesh {
        outline: OUTLINE8.to_vec(),
        verts,
        tris,
        offsets,
    }
}

pub(crate) fn near(a: [f64; 2], b: [f64; 2], tol: f64) -> bool {
    (a[0] - b[0]).abs() <= tol && (a[1] - b[1]).abs() <= tol
}

fn one_vertex(u: f64, v: f64) -> OwnedMesh {
    OwnedMesh {
        outline: Vec::new(),
        verts: vec![[u, v]],
        tris: Vec::new(),
        offsets: Vec::new(),
    }
}

#[test]
fn fixture_outline_is_the_vertex_prefix_and_offsets_stay_separate() {
    let m = fixture_mesh([0.1, 0.0]);
    assert_eq!((m.outline.len(), m.verts.len(), m.tris.len()), (8, 9, 24));
    assert_eq!(
        &m.verts[..8],
        &m.outline[..],
        "outline = verts[0..8) in order"
    );
    assert_eq!(m.verts[8], [0.5, 0.5], "rest centre");
    assert_eq!(m.offsets[8], [0.1, 0.0], "offset kept apart from rest");
    assert!(m.offsets[..8].iter().all(|o| *o == [0.0, 0.0]));
    let before = m.clone();
    let _ = world_verts(&m.view(), &R0, None);
    let _ = scene_payload(&m.view(), &R0, None);
    assert_eq!((&m.verts, &m.offsets), (&before.verts, &before.offsets));
}

#[test]
fn created_meshes_keep_the_outline_prefix_and_valid_topology() {
    let mut seen = 0;
    for (_, t) in rows("mesh") {
        seen += 1;
        let (cols, rows_n): (usize, usize) = (t[1].parse().unwrap(), t[2].parse().unwrap());
        let (outline, verts, tris, offsets) =
            (points(t[3]), points(t[4]), ints(t[5]), points(t[6]));
        assert_eq!(outline.len(), 2 * (cols + rows_n), "perimeter points");
        assert_eq!(verts.len(), outline.len() + (cols - 1) * (rows_n - 1));
        assert!(
            same_pts(&verts[..outline.len()], &outline),
            "{cols}x{rows_n}"
        );
        assert!(offsets.len() == verts.len() && offsets.iter().all(|o| *o == [0.0, 0.0]));
        assert_eq!(tris.len() % 3, 0);
        for tri in tris.chunks(3) {
            let c: Vec<[f64; 2]> = tri.iter().map(|&i| verts[i as usize]).collect();
            let (cx, cy) = (
                (c[0][0] + c[1][0] + c[2][0]) / 3.0,
                (c[0][1] + c[1][1] + c[2][1]) / 3.0,
            );
            assert!(
                point_in_poly(&outline, cx, cy),
                "centroid inside the outline"
            );
        }
    }
    assert_eq!(seen, 5);
}

#[test]
fn fixture_vertex_8_world_is_rest_420_280_then_deformed_440_280() {
    let rest = fixture_mesh([0.0, 0.0]);
    let moved = fixture_mesh([0.1, 0.0]);
    assert_eq!(world_verts(&rest.view(), &R0, None)[8], [420.0, 280.0]);
    assert_eq!(world_verts(&moved.view(), &R0, None)[8], [440.0, 280.0]);
}

#[test]
fn corpus_centre_convention_differs_from_production_top_left_by_half_the_size() {
    // reference.cjs meshVertexWorld reads rect.x,y as the CENTRE: 320,180 -> 340,180.
    // Production rects are TOP-LEFT: the same mesh over x:320,y:180 lands 100,100 further.
    let (rest, moved) = (fixture_mesh([0.0, 0.0]), fixture_mesh([0.1, 0.0]));
    let mut seen = 0;
    for (_, t) in rows("ref").filter(|(_, t)| hex(t[1].split(',').next().unwrap()) == 320.0) {
        let (uv, want) = (point(t[2]), point(t[3]));
        let m = if uv == [0.5, 0.5] {
            &rest
        } else if uv == [0.6, 0.5] {
            &moved
        } else {
            continue;
        };
        seen += 1;
        let centre = MeshRect {
            x: 320.0 - 100.0,
            y: 180.0 - 100.0,
            ..R0
        };
        let converted = world_verts(&m.view(), &centre, None)[8];
        assert!(
            near(converted, want, 1e-9),
            "corpus {want:?} = production over x - w/2"
        );
        let production = world_verts(&m.view(), &R0, None)[8];
        assert!(
            near(production, [want[0] + 100.0, want[1] + 100.0], 1e-9),
            "{production:?}"
        );
    }
    assert_eq!(seen, 2);
    assert!(near(
        world_verts(&rest.view(), &R0, None)[8],
        [420.0, 280.0],
        0.0
    ));
    assert!(near(
        world_verts(&moved.view(), &R0, None)[8],
        [440.0, 280.0],
        0.0
    ));
}

#[test]
fn scene_payload_of_the_fixture_has_rest_uvs_fan_triangles_and_outline_indices() {
    let m = fixture_mesh([0.1, 0.0]);
    let p = scene_payload(&m.view(), &R0, None).expect("triangles present");
    let corners = [
        [320.0, 180.0],
        [420.0, 180.0],
        [520.0, 180.0],
        [520.0, 280.0],
        [520.0, 380.0],
        [420.0, 380.0],
        [320.0, 380.0],
        [320.0, 280.0],
        [440.0, 280.0],
    ];
    assert_eq!(p.verts, corners, "deformed world positions");
    let mut rest = OUTLINE8.to_vec();
    rest.push([0.5, 0.5]);
    assert_eq!(
        p.uvs, rest,
        "uvs are REST positions, the offset never moves them"
    );
    assert_eq!(p.tris, m.tris, "triangles copied");
    assert_eq!(p.outline, (0..8).collect::<Vec<u32>>());
    let mut bare = m.clone();
    bare.tris.clear();
    assert_eq!(
        scene_payload(&bare.view(), &R0, None),
        None,
        "no triangles, no payload"
    );
}

#[test]
fn scene_payload_rounds_half_toward_positive_infinity_and_keeps_negative_zero() {
    // Vertex 0: world = rect centre (u = v = 0.5) = (-0.005, 0.025) -> R2 = (-0, 0.03).
    // Vertex 1: rest (-0.000005, 0.000005) -> R5 uvs = (-0, 0.00001).
    let m = OwnedMesh {
        outline: vec![[0.0, 0.0], [1.0, 0.0]],
        verts: vec![[0.5, 0.5], [-0.000005, 0.000005]],
        tris: vec![0, 1, 1],
        offsets: Vec::new(),
    };
    let r = MeshRect {
        x: -0.01,
        y: 0.0,
        width: 0.01,
        height: 0.05,
        rotation: 0.0,
    };
    let p = scene_payload(&m.view(), &r, None).unwrap();
    assert_eq!(
        p.verts[0][0].to_bits(),
        (-0.0f64).to_bits(),
        "R2(-0.005) is -0"
    );
    assert_eq!(p.verts[0][1], 0.03, "R2(0.025) rounds the half up");
    assert_eq!(
        p.uvs[1][0].to_bits(),
        (-0.0f64).to_bits(),
        "R5(-0.000005) is -0"
    );
    assert_eq!(p.uvs[1][1], 0.00001);
    assert_eq!(p.outline, vec![0, 1]);
    for (x, want) in [
        (0.005, 0.01),
        (-0.015, -0.01),
        (0.025, 0.03),
        (-0.025, -0.02),
    ] {
        assert_eq!(r2(x), want, "r2({x})");
    }
    assert_eq!(r5(0.000005), 0.00001);
}

#[test]
fn pose_is_added_to_stored_offsets_and_none_leaves_them_untouched() {
    let m = fixture_mesh([0.1, 0.0]);
    let plain = world_verts(&m.view(), &R0, None);
    let mut pose: Vec<Option<[f64; 2]>> = vec![None; 9];
    pose[8] = Some([0.05, 0.1]);
    let w = world_verts(&m.view(), &R0, Some(pose.as_slice()));
    assert!(
        near(w[8], [450.0, 300.0], 1e-9),
        "0.5 + 0.1 + 0.05 and 0.5 + 0 + 0.1: {:?}",
        w[8]
    );
    assert!(
        same_pts(&w[..8], &plain[..8]),
        "vertices without a pose entry keep their offsets"
    );
    let none: Vec<Option<[f64; 2]>> = vec![None; 9];
    assert!(same_pts(
        &world_verts(&m.view(), &R0, Some(none.as_slice())),
        &plain
    ));
    let empty: Vec<Option<[f64; 2]>> = Vec::new();
    assert!(
        same_pts(&world_verts(&m.view(), &R0, Some(empty.as_slice())), &plain),
        "short pose"
    );
    let p = scene_payload(&m.view(), &R0, Some(pose.as_slice())).unwrap();
    assert_eq!(p.verts[8], [r2(w[8][0]), r2(w[8][1])]);
    assert_eq!(
        p.uvs[8],
        [0.5, 0.5],
        "the pose moves where pixels land, not where they are read"
    );
}

#[test]
fn rotation_about_the_rect_centre_is_pinned_at_0_30_90_and_minus_45() {
    // Right-middle vertex (1, 0.5) is 100 right of the centre (420, 280); (0.5, 1) is 100 below it.
    let right = one_vertex(1.0, 0.5);
    let below = one_vertex(0.5, 1.0);
    for (deg, want_right, want_below) in [
        (0.0, [520.0, 280.0], [420.0, 380.0]),
        (
            30.0,
            [506.60254037844385, 330.0],
            [370.0, 366.60254037844385],
        ),
        (90.0, [420.0, 380.0], [320.0, 280.0]),
        (
            -45.0,
            [490.7106781186548, 209.28932188134524],
            [490.7106781186548, 350.7106781186548],
        ),
    ] {
        let r = MeshRect {
            rotation: deg,
            ..R0
        };
        assert!(
            near(world_verts(&right.view(), &r, None)[0], want_right, 1e-9),
            "{deg} right"
        );
        assert!(
            near(world_verts(&below.view(), &r, None)[0], want_below, 1e-9),
            "{deg} below"
        );
    }
}

#[test]
fn normalized_of_inverts_the_world_mapping_to_1e_12() {
    let rects = [
        R0,
        MeshRect {
            rotation: 30.0,
            ..R0
        },
        MeshRect {
            rotation: 90.0,
            ..R0
        },
        MeshRect {
            rotation: -45.0,
            ..R0
        },
        MeshRect {
            x: -50.5,
            y: 12.25,
            width: 300.0,
            height: 120.0,
            rotation: 17.0,
        },
    ];
    for r in rects {
        for (u, v) in [
            (0.0, 0.0),
            (1.0, 1.0),
            (0.5, 0.5),
            (0.25, 0.9),
            (-0.3, 1.4),
            (0.123, 0.456),
        ] {
            let w = world_verts(&one_vertex(u, v).view(), &r, None)[0];
            assert!(
                near(normalized_of(&r, w[0], w[1]), [u, v], 1e-12),
                "{u},{v} rot {}",
                r.rotation
            );
        }
    }
}
