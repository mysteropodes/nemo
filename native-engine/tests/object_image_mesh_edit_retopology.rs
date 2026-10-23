//! Retopology through the mesh edit transaction (L03-L, F17/F18): `setOutline` and the density control over
//! the REAL topology kernel (L03-G, reached through the read-queries facade) and the real owner. Compiled
//! only with the `evaluation` feature, exactly like the facade's kernel call.
use super::object_image_mesh_edit_oracle as oracle;
use super::object_image_mesh_edit_rows::*;
use super::*;
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_geometry::{
    scene_payload, MeshRect, MeshView, ScenePayload,
};
use serde_json::{json, Value};

fn ring(n: usize, radius: f64) -> Vec<[f64; 2]> {
    (0..n)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / n as f64;
            [0.5 + radius * a.cos(), 0.5 + radius * a.sin()]
        })
        .collect()
}

#[test]
fn outline_rows_match_the_executed_baseline() {
    let (mut applied, mut refused) = (0, 0);
    for row in oracle::outline_rows() {
        let start = oracle::start(row.start);
        let mut owner = owner_with(&start);
        let before = state(&owner);
        let body = payload(ID, set_outline(&row.poly, row.cols, row.rows));
        let reply = run(&mut owner, "o", body);
        let what = format!(
            "{} {} points {:?}/{:?}",
            row.start,
            row.poly.len(),
            row.cols,
            row.rows
        );
        let Some(expected) = &row.result else {
            // The baseline returned null (fewer than three points): a typed refusal, nothing publishes.
            assert_eq!(code(&reply), "invalid_request", "{what}");
            assert_eq!(state(&owner), before, "{what}");
            refused += 1;
            continue;
        };
        assert_eq!(reply["ok"], true, "{what}: {reply}");
        let got = plain(&owner, ID);
        assert!(oracle::same_mesh(&got, expected), "{what}");
        assert_invariants(&got, &what);
        let changed = !oracle::same_mesh(expected, &start);
        assert_eq!(
            reply["result"],
            json!({"applied":changed,"historyEntriesAdded":u64::from(changed)}),
            "{what}"
        );
        assert_eq!(owner.history_depths(), (usize::from(changed), 0), "{what}");
        applied += 1;
    }
    assert_eq!((applied, refused), (52, 16));
}

#[test]
fn density_rows_match_the_executed_baseline() {
    let mut unchanged = 0;
    for row in oracle::density_rows() {
        let start = oracle::start(row.start);
        let mut owner = owner_with(&start);
        let reply = run_ok(
            &mut owner,
            "d",
            payload(ID, set_density(row.cols, row.rows)),
        );
        let what = format!("{} {}x{}", row.start, row.cols, row.rows);
        let got = plain(&owner, ID);
        assert!(oracle::same_mesh(&got, &row.result), "{what}");
        assert_invariants(&got, &what);
        assert_eq!((got.cols, got.rows), (row.cols, row.rows), "{what}");
        let changed = !oracle::same_mesh(&row.result, &start);
        unchanged += usize::from(!changed);
        assert_eq!(
            reply["result"],
            json!({"applied":changed,"historyEntriesAdded":u64::from(changed)}),
            "{what}"
        );
    }
    assert!(
        unchanged >= 1,
        "at least one density row is an exact no-op (4x4 over the 4x4 mesh)"
    );
}

fn scene(owner: &NativeObjectHistory, id: &str, rect: &MeshRect) -> ScenePayload {
    let mesh = plain(owner, id);
    let view = MeshView {
        outline: &mesh.outline,
        verts: &mesh.verts,
        offsets: &mesh.offsets,
        tris: &mesh.tris,
    };
    scene_payload(&view, rect, None).expect("the fixture mesh has triangles")
}

#[test]
fn fixture_offset_edit_moves_vertex_8_by_20_px_in_the_scene_payload_and_reset_restores_it() {
    // tests/fixtures/mesh: a 200 px display rect whose centre is (320, 180), i.e. top-left (220, 80).
    let rect = MeshRect {
        x: 220.0,
        y: 80.0,
        width: 200.0,
        height: 200.0,
        rotation: 0.0,
    };
    let mut owner = owner_of_text(&doc_text(CORPUS_ZERO));
    let rest = scene(&owner, "im_1", &rect);
    assert_eq!(rest.verts[8], [320.0, 180.0]);
    run_ok(&mut owner, "drag", payload("im_1", set_offset(8, 0.1, 0.0)));
    let dragged = scene(&owner, "im_1", &rect);
    assert_eq!(
        dragged.verts[8],
        [340.0, 180.0],
        "vertex 8 moved by 0.1 * 200 = 20 px"
    );
    for i in (0..9).filter(|i| *i != 8) {
        assert_eq!(dragged.verts[i], rest.verts[i], "vertex {i} did not move");
    }
    assert_eq!(
        dragged.uvs, rest.uvs,
        "the offset moves where pixels land, never where they are sampled"
    );
    assert_eq!(dragged.tris, rest.tris);
    assert_eq!(dragged.outline, (0..8).collect::<Vec<u32>>());
    run_ok(&mut owner, "reset", payload("im_1", reset_offsets()));
    assert_eq!(scene(&owner, "im_1", &rect), rest);
}

#[test]
fn absent_density_and_offsets_stay_absent_through_an_outline_retopology() {
    let hexagon = [
        [0.5, 0.05],
        [0.9, 0.27],
        [0.9, 0.73],
        [0.5, 0.95],
        [0.1, 0.73],
        [0.1, 0.27],
    ];
    let mut entry = oracle::start("g4").entry();
    for member in ["offsets", "cols", "rows"] {
        entry.as_object_mut().unwrap().remove(member);
    }
    let mut owner = owner_of(&doc(1, json!({ "im_1": entry })));
    let reply = run_ok(
        &mut owner,
        "outline",
        payload("im_1", set_outline(&hexagon, None, None)),
    );
    assert_eq!(reply["result"]["applied"], true);
    let after = wire(&owner)["imageMeshes"]["im_1"].clone();
    for member in ["offsets", "cols", "rows"] {
        assert!(
            after.get(member).is_none(),
            "{member} was never authored and is not invented"
        );
    }
    assert_ne!(after["verts"], oracle::start("g4").entry()["verts"]);
    // An explicit density is authored; the other axis, still equal to its default, stays absent.
    run_ok(
        &mut owner,
        "cols",
        payload("im_1", set_outline(&hexagon, Some(6), None)),
    );
    let after = wire(&owner)["imageMeshes"]["im_1"].clone();
    assert_eq!(after["cols"], json!(6));
    assert!(after.get("rows").is_none() && after.get("offsets").is_none());
    // A mesh that authored an off-default density keeps it authored.
    let mut owner = owner_with(&oracle::start("g3x2"));
    run_ok(
        &mut owner,
        "tri",
        payload(
            ID,
            set_outline(&[[0.1, 0.1], [0.9, 0.2], [0.5, 0.9]], None, None),
        ),
    );
    let kept = wire(&owner)["imageMeshes"][ID].clone();
    assert_eq!(
        (kept["cols"].clone(), kept["rows"].clone()),
        (json!(3), json!(2))
    );
}

#[test]
fn a_density_that_rebuilds_the_same_mesh_is_an_exact_noop() {
    // The baseline density control re-densifies the CURRENT outline, so 4x4 over the plain 4x4 mesh adds
    // outline points; over the hexagon mesh (edges already short) it rebuilds the identical mesh.
    let row = oracle::density_rows()
        .into_iter()
        .find(|r| r.start == "hexs" && (r.cols, r.rows) == (4, 4))
        .unwrap();
    let hexs = oracle::start("hexs");
    assert!(
        oracle::same_mesh(&row.result, &hexs),
        "the baseline rebuilds the same mesh"
    );
    let mut owner = owner_with(&hexs);
    let before = state(&owner);
    let reply = run_ok(&mut owner, "same", payload(ID, set_density(4, 4)));
    assert_eq!(
        reply["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(
        state(&owner),
        before,
        "bytes, revision and history are kept"
    );
    // The same geometry with offsets, cols and rows never authored: the kernel's zero offsets and its
    // default density equal what absence evaluates as, so nothing is authored and nothing changes.
    let mut entry = hexs.entry();
    for member in ["offsets", "cols", "rows"] {
        entry.as_object_mut().unwrap().remove(member);
    }
    let mut owner = owner_of(&doc(1, json!({ "im_1": entry })));
    let before = state(&owner);
    let reply = run_ok(&mut owner, "same", payload("im_1", set_density(4, 4)));
    assert_eq!(
        reply["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(state(&owner), before, "absent stays absent");
    // And the plain 4x4 mesh does change: the outline grows by the re-densified midpoints.
    let mut owner = owner_with(&oracle::start("g4"));
    let reply = run_ok(&mut owner, "grow", payload(ID, set_density(4, 4)));
    assert_eq!(
        reply["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(plain(&owner, ID).outline.len(), 32);
}

#[test]
fn retopology_undo_and_redo_restore_the_exact_bytes() {
    let mut owner = owner_with(&oracle::start("g4s"));
    let before = bytes(&owner);
    run_ok(&mut owner, "density", payload(ID, set_density(6, 3)));
    let after = bytes(&owner);
    assert_ne!(before, after);
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(bytes(&owner), before);
    owner.redo(owner.content_revision()).unwrap();
    assert_eq!(bytes(&owner), after);
    assert_eq!(owner.history_depths(), (1, 0));
    // A later real edit clears the redo stack like every other history entry.
    owner.undo(owner.content_revision()).unwrap();
    run_ok(&mut owner, "offset", payload(ID, set_offset(1, 0.1, 0.1)));
    assert_eq!(owner.history_depths(), (1, 0));
}

#[test]
fn the_sculpt_is_carried_across_a_retopology_and_never_folded_into_the_rest_positions() {
    // The corpus drag: vertex 8 at rest (0.5, 0.5) carries [0.1, 0]. Rebuilt at 4x4 the grid has a vertex
    // at exactly (0.5, 0.5), which must inherit the sculpt while keeping its REST position.
    let mut owner = owner_of_text(&doc_text(CORPUS_DRAGGED));
    run_ok(&mut owner, "density", payload("im_1", set_density(4, 4)));
    let mesh = plain(&owner, "im_1");
    assert_invariants(&mesh, "corpus 4x4");
    let center = mesh
        .verts
        .iter()
        .position(|v| *v == [0.5, 0.5])
        .expect("the grid centre");
    // Outline 8 re-densified to 24 points, 9 interior grid vertices: literals of the executed baseline.
    assert_eq!((mesh.outline.len(), mesh.verts.len(), center), (24, 33, 28));
    assert_eq!(mesh.offsets[center], [0.1, 0.0]);
    assert!(mesh
        .verts
        .iter()
        .all(|v| v[0] >= 0.0 && v[0] <= 1.0 && v[1] >= 0.0 && v[1] <= 1.0));
    assert_eq!((mesh.cols, mesh.rows), (4, 4));
}

#[test]
fn flaw_twins_and_boundary_requests_are_accepted() {
    let square = [[0.1, 0.1], [0.9, 0.1], [0.9, 0.9], [0.1, 0.9]];
    let corner = [[0.0, 0.0], [4.0, 0.0], [0.0, 4.0]];
    let mut accepted: Vec<(&str, Value)> = vec![
        (
            "outline twin",
            payload(ID, set_outline(&square, Some(4), Some(4))),
        ),
        ("density twin", payload(ID, set_density(6, 3))),
        (
            "outline without density",
            payload(ID, set_outline(&square, None, None)),
        ),
        (
            "coordinate exactly 4",
            payload(ID, set_outline(&corner, None, None)),
        ),
        (
            "512 points",
            payload(ID, set_outline(&ring(512, 0.3), None, None)),
        ),
        ("density 64 is the bound", payload(ID, set_density(2, 64))),
        ("density 1", payload(ID, set_density(1, 1))),
        (
            "float-spelled density",
            payload(ID, with(set_density(4, 4), |a| a["cols"] = json!(4.0))),
        ),
    ];
    accepted.push((
        "reversed winding",
        payload(
            ID,
            set_outline(&[square[3], square[2], square[1], square[0]], None, None),
        ),
    ));
    for (what, body) in accepted {
        let mut owner = owner_with(&oracle::start("g4s"));
        let reply = run(&mut owner, "twin", body);
        assert_eq!(reply["ok"], true, "{what}: {reply}");
        assert_invariants(&plain(&owner, ID), what);
    }
}

#[test]
fn a_large_outline_request_passes_the_enlarged_envelope_limit() {
    let outline = ring(220, 0.3);
    let mut owner = owner_with(&oracle::start("g4"));
    let body = payload(ID, set_outline(&outline, Some(4), Some(4)));
    let req = request(&owner, "large", body);
    assert!(
        serde_json::to_vec(&req).unwrap().len() > 4096,
        "past the default message bound"
    );
    let reply = dispatch(&mut owner, &req);
    assert_eq!(reply["ok"], true, "{reply}");
    let mesh = plain(&owner, ID);
    assert_invariants(&mesh, "large outline");
    assert!(mesh.outline.len() >= 220);
    assert!(oracle::same_pts(&mesh.outline[..1], &outline[..1]));
}

#[test]
fn a_stored_outline_past_the_cost_bound_refuses_a_density_retopology() {
    // An imported record can carry an outline far longer than any request may; a density change over it is
    // refused before the kernel runs (its cost grows with the outline), and a smaller stored mesh is fine.
    let big = ring(4097, 0.4);
    let mesh = oracle::Mesh {
        outline: big.clone(),
        verts: big.clone(),
        tris: vec![0, 1, 2],
        offsets: vec![[0.0, 0.0]; big.len()],
        cols: 4,
        rows: 4,
    };
    let mut owner = owner_with(&mesh);
    let before = state(&owner);
    let reply = run(&mut owner, "big", payload(ID, set_density(4, 4)));
    assert_eq!(code(&reply), "invalid_request");
    assert_eq!(state(&owner), before);
}

#[test]
fn the_retopology_oracle_is_not_vacuous() {
    let rows = oracle::outline_rows();
    let moved = rows
        .iter()
        .filter(|r| {
            r.result
                .as_ref()
                .is_some_and(|m| !oracle::same_mesh(m, &oracle::start(r.start)))
        })
        .count();
    assert!(moved >= 45, "{moved}");
    let carried = rows
        .iter()
        .filter(|r| {
            let start = oracle::start(r.start);
            start.offsets.iter().any(|o| o[0] != 0.0 || o[1] != 0.0)
                && r.result
                    .as_ref()
                    .is_some_and(|m| m.offsets.iter().any(|o| o[0] != 0.0 || o[1] != 0.0))
        })
        .count();
    assert!(
        carried >= 25,
        "{carried}: sculpt carried across a new outline"
    );
    let reversed = rows
        .iter()
        .filter(|r| r.poly.len() == 6 && r.poly[0] == [0.1, 0.27])
        .count();
    assert!(
        reversed >= 8,
        "{reversed}: negative-area polygons are exercised"
    );
    assert!(rows.iter().filter(|r| r.cols.is_some()).count() >= 20);
    assert!(
        oracle::density_rows()
            .iter()
            .filter(|r| r.start == "g4s")
            .count()
            == 5
    );
}
