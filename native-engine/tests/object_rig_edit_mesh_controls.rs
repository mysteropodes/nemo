//! Negative controls and envelope guards of the mesh binds of the rig edit transaction (L03-O, #2251):
//! unknown, foreign and ambiguous meshes, malformed `bindMesh` payloads against their accepted twin, and
//! the cancel / stale / foreign-owner / replay guards. Every refusal publishes nothing.
use super::object_rig_edit_mesh_rows::*;
use super::object_rig_edit_rows::*;
use super::*;
use serde_json::{json, Value};

const RECT: fn() -> Value = || json!({"x":20,"y":10,"w":300,"h":200});

/// The rig and mesh store of the `bind_mesh_two_bones` row with the given raster entries.
fn owner_with(rasters: &[Value]) -> NativeObjectHistory {
    let row = mesh_row("bind_mesh_two_bones");
    owner_of_text(&mesh_doc_text(
        row.before,
        &[],
        rasters,
        &row.scenario["meshes"],
    ))
}

fn raster_on(layer: &str, frame: u32, mesh: Option<&str>, rect: &Value) -> Value {
    raster_entry(layer, &format!("raster:{layer}:{frame}"), frame, raster_item(mesh, rect))
}

fn bind(extra: Value) -> Value {
    let mut action = json!({"kind":"bindMesh","meshId":"im_1","radius":200});
    for (key, value) in extra.as_object().unwrap() {
        action[key] = value.clone();
    }
    action
}

fn refused(owner: &mut NativeObjectHistory, id: &str, action: Value, want: &str) {
    let start = state(owner);
    let reply = run(owner, id, payload(action));
    assert_eq!(reply["ok"], false, "{id}: {reply}");
    assert_eq!(code(&reply), want, "{id}");
    assert_eq!(state(owner), start, "{id}: nothing was published");
}

#[test]
fn an_unknown_foreign_or_untagged_mesh_is_not_found_and_publishes_nothing() {
    let rect = RECT();
    // Control: the same document binds.
    let mut ok = owner_with(&[raster_on(LAYER, 0, Some("im_1"), &rect)]);
    run_ok(&mut ok, "control", payload(bind(json!({}))));
    // Unknown meshId (the store holds im_1 only).
    let mut owner = owner_with(&[raster_on(LAYER, 0, Some("im_1"), &rect)]);
    refused(&mut owner, "unknown", bind(json!({"meshId":"im_2"})), "not_found");
    // A raster on ANOTHER layer carries the mesh: this layer has none.
    let mut owner = owner_with(&[raster_on("L1", 0, Some("im_1"), &rect)]);
    refused(&mut owner, "other-layer", bind(json!({})), "not_found");
    // The mesh is in the store but no raster carries it.
    let mut owner = owner_with(&[raster_on(LAYER, 0, None, &rect)]);
    refused(&mut owner, "untagged", bind(json!({})), "not_found");
    // A raster is tagged with an id the store does not hold (a dangling tag).
    let mut owner = owner_with(&[raster_on(LAYER, 0, Some("im_9"), &rect)]);
    refused(&mut owner, "dangling", bind(json!({"meshId":"im_9"})), "not_found");
    // The raster sits on a frame the request does not name.
    let mut owner = owner_with(&[raster_on(LAYER, 2, Some("im_1"), &rect)]);
    refused(&mut owner, "other-frame", bind(json!({"frame":0})), "not_found");
    run_ok(&mut owner, "that-frame", payload(bind(json!({"frame":2}))));
    // A stroke id is not a mesh id.
    let mut owner = owner_with(&[raster_on(LAYER, 0, Some("im_1"), &rect)]);
    refused(&mut owner, "stroke-id", bind(json!({"meshId":"raster:L0:0"})), "not_found");
    // The layer has no rig, or does not exist.
    let mut owner = owner_with(&[raster_on(LAYER, 0, Some("im_1"), &rect)]);
    let body = json!({"command":"object.rig.edit","layerUid":"L1","action":bind(json!({}))});
    let start = state(&owner);
    assert_eq!(code(&run(&mut owner, "no-rig", body)), "not_found");
    let body = json!({"command":"object.rig.edit","layerUid":"nope","action":bind(json!({}))});
    assert_eq!(code(&run(&mut owner, "no-layer", body)), "not_found");
    assert_eq!(state(&owner), start);
}

#[test]
fn a_mesh_placed_differently_on_its_frames_needs_a_frame_and_is_never_guessed() {
    let (left, right) = (RECT(), json!({"x":120,"y":10,"w":300,"h":200}));
    let rasters = [
        raster_on(LAYER, 0, Some("im_1"), &left),
        raster_on(LAYER, 1, Some("im_1"), &right),
    ];
    let mut owner = owner_with(&rasters);
    refused(&mut owner, "ambiguous", bind(json!({})), "invalid_request");
    run_ok(&mut owner, "frame-0", payload(bind(json!({"frame":0}))));
    let first = rig_text(&owner);
    run_ok(&mut owner, "frame-1", payload(bind(json!({"frame":1}))));
    assert_ne!(rig_text(&owner), first, "the rest pose follows the raster of the named frame");
    // The same placement on every frame needs no frame.
    let mut owner = owner_with(&[
        raster_on(LAYER, 0, Some("im_1"), &left),
        raster_on(LAYER, 1, Some("im_1"), &left),
    ]);
    run_ok(&mut owner, "same-everywhere", payload(bind(json!({}))));
}

#[test]
fn malformed_bind_mesh_payloads_are_refused_and_their_twin_is_accepted() {
    let rect = RECT();
    let make = || owner_with(&[raster_on(LAYER, 0, Some("im_1"), &rect)]);
    let twin = json!({"kind":"bindMesh","meshId":"im_1","frame":0,"boneIds":["bone1","bone2"],
        "radius":120,"rotate":true,"softness":0.5});
    run_ok(&mut make(), "twin", payload(twin.clone()));
    let flaws = [
        ("no-id", with(twin.clone(), |a| { a.as_object_mut().unwrap().remove("meshId"); })),
        ("blank-id", with(twin.clone(), |a| a["meshId"] = json!(""))),
        ("number-id", with(twin.clone(), |a| a["meshId"] = json!(7))),
        ("unknown-member", with(twin.clone(), |a| a["extra"] = json!(1))),
        ("stroke-member", with(twin.clone(), |a| a["strokeId"] = json!("s1"))),
        ("fraction-frame", with(twin.clone(), |a| a["frame"] = json!(0.5))),
        ("negative-frame", with(twin.clone(), |a| a["frame"] = json!(-1))),
        ("negative-radius", with(twin.clone(), |a| a["radius"] = json!(-5))),
        ("string-radius", with(twin.clone(), |a| a["radius"] = json!("5"))),
        ("string-rotate", with(twin.clone(), |a| a["rotate"] = json!("yes"))),
        ("huge-softness", with(twin.clone(), |a| a["softness"] = json!(1e12))),
        ("bone-ids-object", with(twin.clone(), |a| a["boneIds"] = json!({}))),
        ("blank-bone", with(twin.clone(), |a| a["boneIds"] = json!(["bone1", ""]))),
    ];
    for (id, action) in flaws {
        refused(&mut make(), id, action, "invalid_request");
    }
    // Well-formed but unresolvable bone lists.
    refused(&mut make(), "unknown-bone", with(twin.clone(), |a| a["boneIds"] = json!(["ghost"])), "invalid_request");
    refused(&mut make(), "duplicate-bone", with(twin.clone(), |a| a["boneIds"] = json!(["bone1", "bone1"])), "invalid_request");
    // A locked layer refuses every rig edit.
    let row = mesh_row("bind_mesh_two_bones");
    let text = text_for_mesh_row(&row).replacen(
        &format!("{{\"layerUid\":\"{LAYER}\""),
        &format!("{{\"layerUid\":\"{LAYER}\",\"locked\":true"),
        1,
    );
    refused(&mut owner_of_text(&text), "locked", row.action.clone(), "unavailable");
}

#[test]
fn bind_mesh_and_auto_assign_refuse_cancel_stale_foreign_and_replay_without_a_second_execution() {
    let rect = RECT();
    let auto = json!({"kind":"autoAssign","frame":0,"defaultRadius":200,"meshes":true});
    for (op, action) in [("bind", bind(json!({}))), ("auto", auto)] {
        let mut owner = owner_with(&[raster_on(LAYER, 0, Some("im_1"), &rect)]);
        let original = state(&owner);
        for (id, patch, want) in [
            ("cancel", json!({"cancelledBeforeDispatch":true}), "cancelled_before_dispatch"),
            ("stale", json!({"expectedRevision":99}), "stale_revision"),
            ("foreign-instance", json!({"instanceId":"foreign"}), "wrong_instance"),
            ("foreign-document", json!({"documentId":"foreign"}), "wrong_document"),
            ("wrong-api", json!({"apiVersion":1}), "invalid_request"),
        ] {
            let mut req = request(&owner, id, payload(action.clone()));
            for (member, value) in patch.as_object().unwrap() {
                req[member] = value.clone();
            }
            let reply = dispatch(&mut owner, &req);
            assert_eq!(code(&reply), want, "{op} {id}");
            assert_eq!(state(&owner), original, "{op} {id}");
        }
        let req = request(&owner, "replay", payload(action.clone()));
        let first = dispatch(&mut owner, &req);
        assert_eq!(first["ok"], true, "{op}: {first}");
        let applied = state(&owner);
        assert_eq!(applied.2, (1, 0), "{op}: one history entry");
        assert_eq!(dispatch(&mut owner, &req), first, "{op}: same body, same receipt");
        assert_eq!(state(&owner), applied, "{op}: the replay executed nothing");
        owner.undo(owner.content_revision()).unwrap();
        assert_eq!(bytes(&owner), original.0, "{op}: undo restores the exact bytes");
    }
}

#[test]
fn a_prepared_mesh_bind_is_sealed_to_its_origin() {
    let rect = RECT();
    let mut owner = owner_with(&[raster_on(LAYER, 0, Some("im_1"), &rect)]);
    let body = serde_json::to_vec(&payload(bind(json!({})))).unwrap();
    let prepared = owner.prepare_rig_edit_json(&body).unwrap();
    // A second edit lands first: the prepared candidate is stale and publishes nothing.
    run_ok(
        &mut owner,
        "other",
        payload(json!({"kind":"setBoneRadius","boneId":"bone1","radius":50})),
    );
    let start = state(&owner);
    assert!(owner.commit_rig_edit(prepared).is_err());
    assert_eq!(state(&owner), start);
}
