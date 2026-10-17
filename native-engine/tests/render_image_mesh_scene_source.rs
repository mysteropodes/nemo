//! Oracle-backed and literal tests for the geometry half of the image-mesh scene join (L03-S, F17/F18).
//!
//! The mesh rows come from the executed baseline (real `image-mesh.js`, `motion.js`
//! `transformImageRect`); literals below are hand-derived from the baseline source or the shipped
//! fixture, never produced by the join under test.

use super::oracle_rows::{
    chain, export_rows, geometry_for, mesh_of, mesh_rows, none_rows, plain_source, prepare_row,
    rect_close, row_named, store_of, IDENTITY_VIEW,
};
use super::{
    plan_draw, prepare_mesh_geometry, MeshDraw, MeshSceneError, MeshSceneSource, MeshStoreRef,
};
use crate::evaluation::motion_parent_chain::ImageRect;
use crate::evaluation::object_image_mesh_kernels::render_image_mesh_plan::{
    plan_mesh_draw, MeshPlanInput, OutlineKind, PlanSkip,
};
use crate::object_document::ImageMeshStore;

const BASE_200: ImageRect = ImageRect {
    x: 320.0,
    y: 180.0,
    width: 200.0,
    height: 200.0,
    rotation: 0.0,
};

#[test]
fn the_oracle_holds_exactly_the_pinned_rows() {
    let rows = mesh_rows();
    assert_eq!(
        (rows.len(), none_rows().len(), export_rows().len()),
        (18, 1, 5)
    );
    assert_eq!(rows.iter().filter(|r| !r.stages.is_empty()).count(), 7);
    assert_eq!(
        rows.iter().filter(|r| r.override_offsets.is_some()).count(),
        4
    );
    assert_eq!(rows.iter().filter(|r| r.pose.is_some()).count(), 3);
    assert_eq!(
        rows.iter()
            .filter(|r| r.expect.opacity != r.opacity)
            .count(),
        2
    );
    assert!(rows.iter().all(|r| !r.expect.kept.is_empty()));
}

#[test]
fn every_oracle_mesh_row_matches_the_executed_baseline() {
    for row in mesh_rows() {
        let name = &row.name;
        let (rect, op, geometry) = prepare_row(&row);
        assert!(rect_close(&rect, &row.expect.rect), "{name}: final rect");
        assert_eq!(op, row.expect.opacity, "{name}: opacity product");
        let payload = &geometry.payload;
        assert_eq!(payload.verts, row.expect.verts, "{name}: verts");
        assert_eq!(payload.uvs, row.expect.uvs, "{name}: uvs");
        assert_eq!(payload.tris, row.expect.tris, "{name}: tris");
        assert_eq!(payload.outline, row.expect.outline, "{name}: outline");
        let plan = geometry.plan().unwrap_or_else(|| panic!("{name}: no plan"));
        assert_eq!(
            f64::from(plan.opacity),
            row.expect.opacity_f32,
            "{name}: f32 opacity"
        );
        let (iw, ih) = (f64::from(row.image.0), f64::from(row.image.1));
        let ordinals: Vec<usize> = plan.triangles.iter().map(|t| t.triangle).collect();
        assert_eq!(ordinals, row.expect.kept, "{name}: drawn triangles");
        for tri in &plan.triangles {
            for k in 0..3 {
                let c = row.expect.tris[3 * tri.triangle + k] as usize;
                assert_eq!(tri.dest[k], row.expect.verts[c], "{name}: dest");
                let uv = row.expect.uvs[c];
                assert_eq!(tri.source[k], [uv[0] * iw, uv[1] * ih], "{name}: source");
            }
        }
        assert_eq!(plan.outline.kind, OutlineKind::Polygon, "{name}");
        let want: Vec<[f64; 2]> = row
            .expect
            .outline
            .iter()
            .map(|i| row.expect.verts[*i as usize])
            .collect();
        assert_eq!(plan.outline.points, want, "{name}: outline polygon");
        let input = MeshPlanInput {
            verts: &row.expect.verts,
            uvs: &row.expect.uvs,
            tris: &row.expect.tris,
            outline: &row.expect.outline,
        };
        let direct = plan_mesh_draw(
            &input,
            row.image.0,
            row.image.1,
            row.expect.opacity_f32 as f32,
            &row.view,
        );
        assert_eq!(
            direct.as_ref(),
            Ok(plan),
            "{name}: the join equals the kernel on the baseline payload"
        );
    }
}

#[test]
fn the_mesh_follows_the_final_rect_of_the_chain_and_nothing_else() {
    let rows = mesh_rows();
    for name in [
        "grid4_translate",
        "grid4_rotate_30",
        "grid32_two_stage_chain",
        "corpus_translated_chain",
    ] {
        let row = &rows[row_named(&rows, name)];
        let (_, op, chained) = prepare_row(row);
        let source = MeshSceneSource {
            mesh_id: "im_1",
            view: row.view,
            offsets: row.override_offsets.as_deref(),
            pose: row.pose.as_deref(),
        };
        // the oracle's own final rect, no stages: the same payload
        let (flat, _) = chain(row.expect.rect, &[], row.expect.opacity);
        let direct = geometry_for(flat, row.image, op as f32, &source, &row.mesh)
            .unwrap()
            .unwrap();
        assert_eq!(chained.payload.verts, direct.payload.verts, "{name}");
        assert_eq!(chained.payload.uvs, direct.payload.uvs, "{name}");
    }
    let identity = &rows[row_named(&rows, "grid4_identity")];
    let moved = &rows[row_named(&rows, "grid4_translate")];
    for (a, b) in identity.expect.verts.iter().zip(moved.expect.verts.iter()) {
        assert!((b[0] - a[0] - 15.5).abs() <= 0.0100001 && (b[1] - a[1] + 7.25).abs() <= 0.0100001);
    }
}

#[test]
fn a_mesh_without_triangles_is_drawn_as_the_plain_image() {
    for row in none_rows() {
        let outcome = geometry_for(row.base, row.image, 1.0, &plain_source("im_1"), &row.mesh);
        assert_eq!(outcome, Ok(None), "{}", row.name);
    }
}

#[test]
fn the_shipped_fixture_vertex_moves_from_420_280_to_440_280() {
    let rows = mesh_rows();
    let stored = &rows[row_named(&rows, "corpus_stored_offset")];
    let zero = &rows[row_named(&rows, "corpus_zero_override")];
    assert_eq!(zero.expect.verts[8], [420.0, 280.0]);
    assert_eq!(stored.expect.verts[8], [440.0, 280.0]);
    assert_eq!(stored.expect.uvs[8], [0.5, 0.5]);
    let (_, _, geometry) = prepare_row(stored);
    assert_eq!(geometry.payload.verts[8], [440.0, 280.0]);
    let plan = geometry.plan().unwrap();
    let hit: Vec<_> = plan
        .triangles
        .iter()
        .filter(|t| t.dest.contains(&[440.0, 280.0]))
        .collect();
    assert_eq!(
        hit.len(),
        8,
        "the centre vertex belongs to all eight fan triangles"
    );
    for tri in hit {
        let at = tri.dest.iter().position(|d| *d == [440.0, 280.0]).unwrap();
        assert_eq!(tri.source[at], [32.0, 32.0]);
        let m = tri.place;
        let mapped = [
            m[0] * 32.0 + m[2] * 32.0 + m[4],
            m[1] * 32.0 + m[3] * 32.0 + m[5],
        ];
        assert!((mapped[0] - 440.0).abs() < 1e-9 && (mapped[1] - 280.0).abs() < 1e-9);
    }
}

#[test]
fn rig_offsets_replace_the_stored_sculpt_and_the_animated_pose_adds_on_top() {
    let rows = mesh_rows();
    let row = &rows[row_named(&rows, "corpus_stored_offset")];
    let vertex8 = |source: &MeshSceneSource<'_>| {
        let geometry = geometry_for(BASE_200, (64, 64), 1.0, source, &row.mesh)
            .unwrap()
            .unwrap();
        (geometry.payload.verts[8], geometry.payload.uvs[8])
    };
    let mut pose: Vec<Option<[f64; 2]>> = vec![None; 9];
    pose[8] = Some([0.05, 0.0]);
    let on_stored = MeshSceneSource {
        pose: Some(&pose),
        ..plain_source("im_1")
    };
    assert_eq!(
        vertex8(&on_stored).0,
        [450.0, 280.0],
        "stored 0.1 + pose 0.05"
    );
    let zeros = vec![[0.0, 0.0]; 9];
    let posed = MeshSceneSource {
        offsets: Some(&zeros),
        pose: Some(&pose),
        ..plain_source("im_1")
    };
    assert_eq!(
        vertex8(&posed).0,
        [430.0, 280.0],
        "rig zeros replace the stored 0.1, pose adds 0.05"
    );
    let mut rig = zeros.clone();
    rig[8] = [-0.1, 0.0];
    let moved = MeshSceneSource {
        offsets: Some(&rig),
        ..plain_source("im_1")
    };
    let (vert, uv) = vertex8(&moved);
    assert_eq!(vert, [400.0, 280.0]);
    assert_eq!(
        uv,
        [0.5, 0.5],
        "the offset moves where pixels land, never where they are sampled"
    );
}

#[test]
fn refusals_are_typed_stable_and_leave_no_geometry() {
    let rows = mesh_rows();
    let text = &rows[row_named(&rows, "corpus_stored_offset")].mesh;
    let store = store_of(text);
    let none: MeshStoreRef<'_> = None;
    let short = vec![[0.0, 0.0]; 8];
    let mut nan = vec![[0.0, 0.0]; 9];
    nan[3] = [f64::NAN, 0.0];
    let bad_pose = vec![Some([f64::INFINITY, 0.0])];
    let run = |source: MeshSceneSource<'_>, store: MeshStoreRef<'_>| {
        prepare_mesh_geometry(&BASE_200, (64, 64), 1.0, &source, store)
    };
    let cases: Vec<(Result<_, MeshSceneError>, MeshSceneError, &str)> = vec![
        (
            run(plain_source(""), Some(&store)),
            MeshSceneError::EmptyMeshId,
            "mesh-scene-empty-mesh-id",
        ),
        (
            run(plain_source("im_1"), none),
            MeshSceneError::StoreUnavailable {
                mesh_id: "im_1".to_owned(),
            },
            "mesh-scene-store-unavailable",
        ),
        (
            run(plain_source("im_9"), Some(&store)),
            MeshSceneError::MeshMissing {
                mesh_id: "im_9".to_owned(),
            },
            "mesh-scene-mesh-missing",
        ),
        (
            run(
                MeshSceneSource {
                    offsets: Some(&short),
                    ..plain_source("im_1")
                },
                Some(&store),
            ),
            MeshSceneError::OffsetsLengthMismatch {
                mesh_id: "im_1".to_owned(),
                expected: 9,
                got: 8,
            },
            "mesh-scene-offsets-length-mismatch",
        ),
        (
            run(
                MeshSceneSource {
                    offsets: Some(&nan),
                    ..plain_source("im_1")
                },
                Some(&store),
            ),
            MeshSceneError::NonFiniteMotion {
                mesh_id: "im_1".to_owned(),
            },
            "mesh-scene-non-finite-motion",
        ),
        (
            run(
                MeshSceneSource {
                    pose: Some(&bad_pose),
                    ..plain_source("im_1")
                },
                Some(&store),
            ),
            MeshSceneError::NonFiniteMotion {
                mesh_id: "im_1".to_owned(),
            },
            "mesh-scene-non-finite-motion",
        ),
    ];
    for (outcome, want, code) in cases {
        let error = outcome.expect_err("refused");
        assert_eq!(error, want);
        assert_eq!(error.code(), code);
        assert!(!error.reason().is_empty());
    }
}

#[test]
fn a_mesh_the_renderer_cannot_draw_draws_nothing_instead_of_a_flat_image() {
    // two vertices and a degenerate triple: admitted by the record, "incomplete" for the renderer
    let tiny = r#"{"outline":[],"verts":[[0,0],[1,1]],"tris":[0,1,0],"offsets":[[0,0],[0,0]],"cols":1,"rows":1}"#;
    let store = ImageMeshStore::try_new(vec![("im_1".to_owned(), mesh_of(tiny))]).unwrap();
    let geometry =
        prepare_mesh_geometry(&BASE_200, (8, 8), 1.0, &plain_source("im_1"), Some(&store))
            .unwrap()
            .unwrap();
    assert_eq!(geometry.draw, MeshDraw::Nothing(PlanSkip::IncompleteMesh));
    assert_eq!(
        (geometry.plan().is_none(), geometry.triangle_count()),
        (true, 0)
    );
}

#[test]
fn another_view_replans_the_same_payload_and_only_the_seam_dilation_follows_it() {
    let rows = mesh_rows();
    let (_, op, geometry) = prepare_row(&rows[row_named(&rows, "grid4_identity")]);
    let row = &rows[row_named(&rows, "grid4_identity")];
    assert_eq!(row.view, IDENTITY_VIEW);
    let same = plan_draw(&geometry.payload, row.image, op as f32, &IDENTITY_VIEW);
    assert_eq!(
        same, geometry.draw,
        "an identical view replans an identical draw"
    );
    let zoomed = plan_draw(
        &geometry.payload,
        row.image,
        op as f32,
        &[2.0, 0.0, 0.0, 2.0, 100.0, 50.0],
    );
    let (MeshDraw::Plan(a), MeshDraw::Plan(b)) = (&geometry.draw, &zoomed) else {
        panic!("both draw");
    };
    assert_eq!((a.opacity, &a.outline), (b.opacity, &b.outline));
    assert_eq!(a.triangles.len(), b.triangles.len());
    let mut clips_differ = 0;
    for (ta, tb) in a.triangles.iter().zip(b.triangles.iter()) {
        assert_eq!(
            (ta.triangle, ta.source, ta.dest, ta.place),
            (tb.triangle, tb.source, tb.dest, tb.place)
        );
        assert!(
            tb.dilation <= ta.dilation,
            "a larger device scale needs a smaller seam factor"
        );
        if ta.clip != tb.clip {
            clips_differ += 1;
        }
    }
    assert!(
        clips_differ > 0,
        "the half-device-pixel dilation follows the view"
    );
}
