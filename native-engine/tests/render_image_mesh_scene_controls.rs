//! Negative and mutation controls for the geometry half of the image-mesh scene join (L03-S).
//!
//! Every control first shows that the unedited twin agrees with the executed baseline and then that
//! ONE edit of an input moves the result, so the oracle comparison in `tests` cannot pass by
//! accident. An independent second implementation of the zero-rotation payload (its own world mapping,
//! switchable flaws) is checked against production on every row it can cover, and each flaw must be
//! caught.

use super::oracle_rows::{
    geometry_for, mesh_rows, plain_source, prepare_row, row_named, Expect, MeshRow,
};
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_geometry::{
    r2, r5, ScenePayload,
};

fn clone_row(row: &MeshRow) -> MeshRow {
    MeshRow {
        name: row.name.clone(),
        mesh: row.mesh.clone(),
        override_offsets: row.override_offsets.clone(),
        pose: row.pose.clone(),
        image: row.image,
        base: row.base,
        stages: row.stages.clone(),
        opacity: row.opacity,
        view: row.view,
        expect: Expect {
            rect: row.expect.rect,
            opacity: row.expect.opacity,
            opacity_f32: row.expect.opacity_f32,
            verts: row.expect.verts.clone(),
            uvs: row.expect.uvs.clone(),
            tris: row.expect.tris.clone(),
            outline: row.expect.outline.clone(),
            kept: row.expect.kept.clone(),
        },
    }
}

fn payload_of(row: &MeshRow, edit: impl Fn(&MeshRow) -> MeshRow) -> ScenePayload {
    let (_, _, geometry) = prepare_row(&edit(row));
    geometry.payload
}

fn same(row: &MeshRow) -> MeshRow {
    clone_row(row)
}

/// True when the edited row still reproduces the baseline's world vertices and uvs.
fn agrees_with_baseline(row: &MeshRow, edit: impl Fn(&MeshRow) -> MeshRow) -> bool {
    let payload = payload_of(row, edit);
    payload.verts == row.expect.verts && payload.uvs == row.expect.uvs
}

#[test]
fn the_unedited_twin_of_every_row_agrees_with_the_baseline() {
    for row in mesh_rows() {
        assert!(agrees_with_baseline(&row, same), "{}", row.name);
    }
}

#[test]
fn ignoring_the_motion_chain_is_caught_on_every_row_that_has_one() {
    let mut caught = 0;
    for row in mesh_rows().iter().filter(|r| !r.stages.is_empty()) {
        let flawed = |r: &MeshRow| MeshRow {
            stages: Vec::new(),
            ..clone_row(r)
        };
        assert!(!agrees_with_baseline(row, flawed), "{}", row.name);
        caught += 1;
    }
    assert_eq!(caught, 7);
}

#[test]
fn ignoring_the_rig_offsets_or_the_pose_is_caught() {
    let (mut overrides, mut poses) = (0, 0);
    for row in mesh_rows().iter() {
        if row.override_offsets.is_some() {
            let flawed = |r: &MeshRow| MeshRow {
                override_offsets: None,
                ..clone_row(r)
            };
            assert!(!agrees_with_baseline(row, flawed), "{}", row.name);
            overrides += 1;
        }
        if row.pose.is_some() {
            let flawed = |r: &MeshRow| MeshRow {
                pose: None,
                ..clone_row(r)
            };
            assert!(!agrees_with_baseline(row, flawed), "{}", row.name);
            poses += 1;
        }
    }
    assert_eq!((overrides, poses), (4, 3));
}

fn mesh_value(row: &MeshRow) -> serde_json::Value {
    serde_json::from_str(&row.mesh).unwrap()
}

fn pairs(value: &serde_json::Value) -> Vec<[f64; 2]> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|p| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()])
        .collect()
}

#[test]
fn a_rig_offset_replaces_the_stored_sculpt_it_is_not_added_to_it() {
    let rows = mesh_rows();
    let row = &rows[row_named(&rows, "grid32_rig_override_replaces_stored")];
    let stored = pairs(&mesh_value(row)["offsets"]);
    assert!(
        stored.iter().any(|o| *o != [0.0, 0.0]),
        "the row really carries a stored sculpt"
    );
    let posed = row.override_offsets.clone().unwrap();
    let added: Vec<[f64; 2]> = stored
        .iter()
        .zip(posed.iter())
        .map(|(s, o)| [s[0] + o[0], s[1] + o[1]])
        .collect();
    assert!(agrees_with_baseline(row, same));
    let flawed = |r: &MeshRow| MeshRow {
        override_offsets: Some(added.clone()),
        ..clone_row(r)
    };
    assert!(
        !agrees_with_baseline(row, flawed),
        "adding the sculpt to the rig offsets is not the baseline"
    );
    let (_, _, geometry) = prepare_row(row);
    assert_eq!(
        geometry.payload.uvs, row.expect.uvs,
        "uvs never follow offsets"
    );
}

#[test]
fn the_item_opacity_is_carried_by_the_plan_once_and_never_by_a_triangle() {
    let rows = mesh_rows();
    for name in ["grid4_scale_opacity", "grid32_two_stage_chain"] {
        let row = &rows[row_named(&rows, name)];
        let (_, op, geometry) = prepare_row(row);
        let plan = geometry.plan().unwrap();
        assert_eq!(f64::from(plan.opacity), row.expect.opacity_f32, "{name}");
        assert_ne!(
            f64::from(plan.opacity),
            row.opacity,
            "{name}: the stage opacities changed it"
        );
        let unmultiplied = geometry_for(
            row.base,
            row.image,
            row.opacity as f32,
            &plain_source("im_1"),
            &row.mesh,
        )
        .unwrap()
        .unwrap();
        assert_ne!(unmultiplied.plan().unwrap().opacity, plan.opacity, "{name}");
        assert_eq!(op as f32, plan.opacity, "{name}");
    }
}

#[test]
fn the_view_moves_only_the_seam_clips_never_where_pixels_come_from() {
    let rows = mesh_rows();
    let row = &rows[row_named(&rows, "grid4_view_zoom_2")];
    let plain = &rows[row_named(&rows, "grid4_identity")];
    let (_, _, zoomed) = prepare_row(row);
    let (_, _, identity) = prepare_row(plain);
    assert_eq!(
        zoomed.payload, identity.payload,
        "the view never enters the payload"
    );
    let (a, b) = (zoomed.plan().unwrap(), identity.plan().unwrap());
    assert!(a
        .triangles
        .iter()
        .zip(b.triangles.iter())
        .all(|(x, y)| x.place == y.place && x.dest == y.dest));
    assert!(a
        .triangles
        .iter()
        .zip(b.triangles.iter())
        .any(|(x, y)| x.clip != y.clip));
}

// ---- an independent second implementation of the zero-rotation payload ---------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Flaw {
    Faithful,
    IgnoreOverride,
    IgnorePose,
    IgnoreStored,
    UvFromDeformed,
    RectCentreAsOrigin,
}

/// verts / uvs of `scenePayloadFor` for a rect without rotation, written independently of the kernel.
fn reference(row: &MeshRow, flaw: Flaw) -> (Vec<[f64; 2]>, Vec<[f64; 2]>) {
    let mesh = mesh_value(row);
    let rest = pairs(&mesh["verts"]);
    let stored = pairs(&mesh["offsets"]);
    let rect = row.expect.rect;
    let (ox, oy) = if flaw == Flaw::RectCentreAsOrigin {
        (rect.x - rect.width / 2.0, rect.y - rect.height / 2.0)
    } else {
        (rect.x, rect.y)
    };
    let (cx, cy) = (ox + rect.width / 2.0, oy + rect.height / 2.0);
    let (mut verts, mut uvs) = (Vec::new(), Vec::new());
    for (i, r) in rest.iter().enumerate() {
        let mut off = if flaw == Flaw::IgnoreStored {
            [0.0, 0.0]
        } else {
            stored[i]
        };
        if let (Some(posed), false) = (&row.override_offsets, flaw == Flaw::IgnoreOverride) {
            off = posed[i];
        }
        if let (Some(Some(p)), false) = (
            row.pose.as_ref().map(|p| p.get(i).copied().flatten()),
            flaw == Flaw::IgnorePose,
        ) {
            off = [off[0] + p[0], off[1] + p[1]];
        }
        let (u, v) = (r[0] + off[0], r[1] + off[1]);
        verts.push([
            r2(cx + (u - 0.5) * rect.width),
            r2(cy + (v - 0.5) * rect.height),
        ]);
        let src = if flaw == Flaw::UvFromDeformed {
            [u, v]
        } else {
            *r
        };
        uvs.push([r5(src[0]), r5(src[1])]);
    }
    (verts, uvs)
}

#[test]
fn the_independent_reference_agrees_with_production_and_every_flaw_is_caught() {
    let rows = mesh_rows();
    let flat: Vec<&MeshRow> = rows
        .iter()
        .filter(|r| r.expect.rect.rotation == 0.0)
        .collect();
    assert!(
        flat.len() >= 14,
        "the reference covers every row without rotation"
    );
    for row in &flat {
        let (_, _, geometry) = prepare_row(row);
        let (verts, uvs) = reference(row, Flaw::Faithful);
        assert_eq!(geometry.payload.verts, verts, "{}", row.name);
        assert_eq!(geometry.payload.uvs, uvs, "{}", row.name);
    }
    for flaw in [
        Flaw::IgnoreOverride,
        Flaw::IgnorePose,
        Flaw::IgnoreStored,
        Flaw::UvFromDeformed,
        Flaw::RectCentreAsOrigin,
    ] {
        let caught = flat
            .iter()
            .filter(|row| {
                let (_, _, geometry) = prepare_row(row);
                let (verts, uvs) = reference(row, flaw);
                geometry.payload.verts != verts || geometry.payload.uvs != uvs
            })
            .count();
        assert!(caught > 0, "{flaw:?} changed no row");
    }
}

// ---- preparing is a pure read ----------------------------------------------------------------------------------

#[test]
fn preparing_is_deterministic_and_never_mutates_the_store_or_the_rig_inputs() {
    use super::oracle_rows::{chain, store_of};
    use super::{prepare_mesh_geometry, MeshSceneSource};
    let rows = mesh_rows();
    let row = &rows[row_named(&rows, "grid32_override_and_pose")];
    let store = store_of(&row.mesh);
    let before = store.clone();
    let offsets = row.override_offsets.clone().unwrap();
    let pose = row.pose.clone().unwrap();
    let (rect, op) = chain(row.base, &row.stages, row.opacity);
    let source = MeshSceneSource {
        mesh_id: "im_1",
        view: row.view,
        offsets: Some(&offsets),
        pose: Some(&pose),
    };
    let run = || prepare_mesh_geometry(&rect, row.image, op as f32, &source, Some(&store));
    let first = run().unwrap().unwrap();
    assert_eq!(run().unwrap().unwrap(), first);
    assert_eq!(store, before);
    assert_eq!(Some(&offsets), row.override_offsets.as_ref());
    assert_eq!(Some(&pose), row.pose.as_ref());
}
