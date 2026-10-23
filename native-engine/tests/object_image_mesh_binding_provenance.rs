//! Architect-approved retained mesh-holder provenance through real owner, codec and read ports.
//! The public selector stays context/layer/stroke only; provenance belongs to the stored holder.
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::PropertyHolderTarget;
use crate::request_receipts::{DispatchErrorCode as E, OpacityRequest};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

#[path = "object_image_mesh_binding_fixtures.rs"]
mod fixtures;
#[path = "object_raster_asset_commands_support.rs"]
mod support;
use fixtures::*;

fn wire(owner: &NativeObjectHistory) -> Value {
    serde_json::from_slice(&support::bytes(owner)).unwrap()
}

fn run(owner: &mut NativeObjectHistory, id: &str, payload: Value) {
    let request: OpacityRequest = serde_json::from_value(json!({"apiVersion":2,
        "requestId":id,"instanceId":owner.instance_id(),"documentId":owner.document_id(),
        "expectedRevision":owner.content_revision(),"operation":"command.document.object.image.mesh.edit",
        "payload":payload})).unwrap();
    let reply = owner
        .try_dispatch_object_command(&request)
        .unwrap()
        .unwrap();
    assert!(reply.is_ok(), "{id}: {reply:?}");
}

fn selector(context: &str, layer: &str, mesh_id: &str) -> Value {
    json!({"contextId":context,"layerUid":layer,"strokeId":mesh_id})
}

fn marked(mut holder: Value, id: &str) -> Value {
    holder["meshSource"] = json!({"meshId":id});
    holder
}

fn with_controls(mut value: Value) -> Value {
    for holder in value["propertyHolders"].as_array_mut().unwrap() {
        holder["controls"] =
            json!([{"key":"ctrl_gain","name":"Gain","type":"number","default":[0.5]}]);
        holder["properties"]
            .as_array_mut()
            .unwrap()
            .push(json!({"property":"ctrl_gain","static":[0.75]}));
    }
    value
}

fn file(owner: &NativeObjectHistory) -> Vec<u8> {
    let snapshot = support::head(owner);
    owner
        .serialize_project_file(owner.content_revision(), snapshot.snapshot_id())
        .unwrap()
}

fn replace(owner: &mut NativeObjectHistory, bytes: &[u8]) {
    let snapshot = support::head(owner);
    let candidate = owner
        .prepare_project_replacement(snapshot.snapshot_id(), bytes)
        .unwrap();
    owner.commit_project_replacement(candidate).unwrap();
}

fn query(snapshot: &ObjectSnapshot, view: Value) -> Value {
    json!({"instanceId":snapshot.instance_id(),"documentId":snapshot.document_id(),
        "atRevision":snapshot.content_revision(),"documentSnapshotId":snapshot.snapshot_id(),"view":view})
}

fn stored_metadata(owner: &NativeObjectHistory, target: Value) -> Value {
    let snapshot = support::head(owner);
    let request = query(
        &snapshot,
        json!({"kind":"storedProperty","target":target,"property":"vtx8"}),
    );
    snapshot
        .query_property_metadata_json(request.to_string().as_bytes())
        .unwrap()
}

fn property(target: Value, name: &str) -> Value {
    json!({"command":"object.property.edit","target":target,"property":name,
        "action":{"kind":"setKey","frame":2,"values":[0.4,0.5]}})
}

fn assert_unbound(owner: &NativeObjectHistory, target: Value) {
    let snapshot = support::head(owner);
    let typed: PropertyHolderTarget = serde_json::from_value(target.clone()).unwrap();
    for name in ["vtx8", "opacity", "ctrl_gain"] {
        assert_eq!(
            snapshot.sample_property_raw(&typed, name, 2.),
            Err(E::Unavailable),
            "{name}"
        );
    }
    let request = query(
        &snapshot,
        json!({"kind":"rawSample","target":target.clone(),"property":"vtx8","frame":2}),
    );
    assert_eq!(
        snapshot.query_property_read_json(request.to_string().as_bytes()),
        Err(E::Unavailable)
    );
    let metadata = stored_metadata(owner, target);
    assert_eq!(metadata["items"][0]["unit"], "%");
    assert_eq!(metadata["items"][0]["dimension"], 2);
    assert_eq!(metadata["items"][0]["authored"]["keyCount"], 2);
}

#[test]
fn legacy_transitions_retain_exact_tracks_controls_and_atomic_history_through_project_replacement()
{
    for (name, action, released) in [
        ("replace", attach(at(0)), false),
        ("detach", detach(at(0)), true),
    ] {
        let value = with_controls(animated_drawing());
        let original_holder = value["propertyHolders"][0].clone();
        let expected = marked(original_holder.clone(), "im_1");
        let mut owner = support::owner(&value);
        let initial = support::bytes(&owner);
        let retained = support::head(&owner);
        assert!(wire(&owner)["propertyHolders"][0]
            .get("meshSource")
            .is_none());
        let untouched = support::state(&owner);
        run(
            &mut owner,
            "same-legacy-binding",
            payload(json!({"kind":"propagate","target":at(0),"meshId":"im_1"})),
        );
        assert_eq!(
            support::state(&owner),
            untouched,
            "an equal binding does not canonicalize a legacy holder"
        );
        drop(
            owner
                .prepare_image_mesh_edit_json(action.to_string().as_bytes())
                .unwrap(),
        );
        assert_eq!(
            support::bytes(&owner),
            initial,
            "preparation cannot canonicalize live state"
        );
        run(&mut owner, name, action);
        assert_eq!(owner.history_depths(), (1, 0));
        assert_eq!(wire(&owner)["propertyHolders"][0], expected);
        assert_eq!(wire(&owner)["imageMeshes"].get("im_1").is_none(), released);
        assert_eq!(
            encode_storage_project(retained.document()).unwrap(),
            initial
        );
        assert_unbound(&owner, selector("scene-root", "L0", "im_1"));
        let after = support::bytes(&owner);
        owner.undo(owner.content_revision()).unwrap();
        assert_eq!(
            support::bytes(&owner),
            initial,
            "undo restores legacy absence too"
        );
        owner.redo(owner.content_revision()).unwrap();
        assert_eq!(support::bytes(&owner), after);
        let saved = file(&owner);
        let reopened = NativeObjectHistory::from_project_file("provenance-reopen", &saved).unwrap();
        assert_eq!(support::bytes(&reopened), after);
        assert_unbound(&reopened, selector("scene-root", "L0", "im_1"));
        replace(&mut owner, &saved);
        assert_eq!(file(&owner), saved);
        assert_eq!(wire(&owner)["propertyHolders"][0], expected);
    }
}

#[test]
fn retained_unbound_reads_are_unavailable_and_public_selectors_cannot_create_provenance() {
    let mut owner = support::owner(&with_controls(animated_drawing()));
    run(&mut owner, "detach-read", detach(at(0)));
    let target = selector("scene-root", "L0", "im_1");
    assert_unbound(&owner, target.clone());
    let before = support::state(&owner);
    assert_eq!(
        owner
            .prepare_property_edit_json(property(target.clone(), "vtx7").to_string().as_bytes())
            .unwrap_err(),
        E::Unavailable
    );
    let unknown = selector("scene-root", "L0", "im_404");
    assert_eq!(
        owner
            .prepare_property_edit_json(property(unknown, "vtx8").to_string().as_bytes())
            .unwrap_err(),
        E::NotFound
    );
    for (field, value) in [
        ("meshSource", json!({"meshId":"im_1"})),
        ("kind", json!("mesh")),
    ] {
        let mut forged = target.clone();
        forged[field] = value;
        assert!(serde_json::from_value::<PropertyHolderTarget>(forged.clone()).is_err());
        let snapshot = support::head(&owner);
        let metadata = query(
            &snapshot,
            json!({"kind":"storedProperty","target":forged,"property":"vtx8"}),
        );
        assert_eq!(
            snapshot.query_property_metadata_json(metadata.to_string().as_bytes()),
            Err(E::InvalidRequest)
        );
        assert_eq!(
            owner
                .prepare_property_edit_json(property(forged, "vtx8").to_string().as_bytes())
                .unwrap_err(),
            E::InvalidRequest
        );
    }
    assert_eq!(support::state(&owner), before);
    // Existing authored rows remain editable; retained provenance cannot create new geometry rows.
    let candidate = owner
        .prepare_property_edit_json(property(target.clone(), "vtx8").to_string().as_bytes())
        .unwrap();
    owner.commit_property_edit(candidate).unwrap();
    assert_eq!(
        wire(&owner)["propertyHolders"][0]["properties"][0]["track"]["keys"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        support::head(&owner).sample_property_raw(
            &serde_json::from_value(target).unwrap(),
            "vtx8",
            2.
        ),
        Err(E::Unavailable)
    );
}

#[test]
fn component_transition_retains_only_the_exact_local_holder_and_never_borrows_root_geometry() {
    let mut value = component_drawing();
    let root = vertex_holder("L0");
    let mut local = vertex_holder("inner-only");
    local["target"]["contextId"] = json!("component-A");
    value["propertyHolders"] = json!([root.clone(), local.clone()]);
    let value = with_controls(value);
    let root = value["propertyHolders"][0].clone();
    let local = value["propertyHolders"][1].clone();
    let mut owner = support::owner(&value);
    run(&mut owner, "component-provenance", detach(inner(2)));
    assert_eq!(
        wire(&owner)["propertyHolders"],
        json!([root, marked(local, "im_1")])
    );
    assert_eq!(
        wire(&owner)["imageMeshes"]["im_1"],
        mesh(),
        "root still owns a live binding"
    );
    assert_unbound(&owner, selector("component-A", "inner-only", "im_1"));
    let root_target = serde_json::from_value(selector("scene-root", "L0", "im_1")).unwrap();
    assert_eq!(
        support::head(&owner)
            .sample_property_raw(&root_target, "vtx8", 0.)
            .unwrap(),
        vec![0.1, 0.2]
    );
    let after = support::bytes(&owner);
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(wire(&owner)["propertyHolders"], value["propertyHolders"]);
    owner.redo(owner.content_revision()).unwrap();
    assert_eq!(support::bytes(&owner), after);
    let reopened =
        NativeObjectHistory::from_project_file("component-provenance", &file(&owner)).unwrap();
    assert_eq!(support::bytes(&reopened), after);
    assert_unbound(&reopened, selector("component-A", "inner-only", "im_1"));
}

#[test]
fn duplicate_marks_only_the_destination_and_remaps_existing_provenance_with_a_deep_mesh_copy() {
    for already_marked in [false, true] {
        let mut value = with_controls(animated_duplicate_drawing());
        if already_marked {
            for holder in value["propertyHolders"].as_array_mut().unwrap() {
                *holder = marked(holder.clone(), "im_1");
            }
        }
        let source = value["propertyHolders"][0].clone();
        let mut destination = value["propertyHolders"][1].clone();
        destination["target"]["strokeId"] = json!("im_2");
        destination = marked(destination, "im_2");
        let mut owner = support::owner(&value);
        run(&mut owner, "duplicate-provenance", duplicate("L2"));
        assert_eq!(
            wire(&owner)["propertyHolders"],
            json!([source, destination])
        );
        assert_eq!(
            wire(&owner)["imageMeshes"]["im_1"],
            wire(&owner)["imageMeshes"]["im_2"]
        );
        run(
            &mut owner,
            "sculpt-destination",
            json!({"command":"object.image.mesh.edit","meshId":"im_2",
            "action":{"kind":"setOffset","index":8,"du":0.25,"dv":-0.5}}),
        );
        assert_eq!(wire(&owner)["imageMeshes"]["im_1"], mesh());
        assert_eq!(
            wire(&owner)["imageMeshes"]["im_2"]["offsets"][8],
            json!([0.25, -0.5])
        );
        let saved = file(&owner);
        let reopened =
            NativeObjectHistory::from_project_file("duplicate-provenance", &saved).unwrap();
        assert_eq!(
            wire(&reopened)["propertyHolders"],
            wire(&owner)["propertyHolders"]
        );
        owner.undo(owner.content_revision()).unwrap();
        owner.undo(owner.content_revision()).unwrap();
        assert_eq!(wire(&owner), value);
    }
}

#[test]
fn strict_source_admission_rejects_malformed_mismatched_duplicate_and_untyped_orphaned_holders() {
    let mut good = with_controls(animated_drawing());
    good["propertyHolders"][0] = marked(good["propertyHolders"][0].clone(), "im_1");
    assert!(decode_storage_project(good.to_string().as_bytes()).is_ok());
    for source in [
        Value::Null,
        json!([]),
        json!("im_1"),
        json!({}),
        json!({"meshId":null}),
        json!({"meshId":1}),
        json!({"meshId":""}),
        json!({"meshId":"im_2"}),
        json!({"meshId":"im_1","unknown":true}),
    ] {
        let mut value = good.clone();
        value["propertyHolders"][0]["meshSource"] = source;
        assert!(
            decode_storage_project(value.to_string().as_bytes()).is_err(),
            "{value}"
        );
    }
    let raw = good.to_string().replace(
        "\"meshSource\":{\"meshId\":\"im_1\"}",
        "\"meshSource\":{\"meshId\":\"im_1\",\"meshId\":\"im_1\"}",
    );
    assert_ne!(raw, good.to_string());
    assert!(decode_storage_project(raw.as_bytes()).is_err());
    let mut duplicate = good.clone();
    duplicate["propertyHolders"]
        .as_array_mut()
        .unwrap()
        .push(vertex_holder("L0"));
    assert!(
        decode_storage_project(duplicate.to_string().as_bytes()).is_err(),
        "provenance does not split selector identity"
    );
    for field in ["layerUid", "contextId"] {
        let mut value = good.clone();
        value["propertyHolders"][0]["target"][field] = json!("absent");
        assert!(decode_storage_project(value.to_string().as_bytes()).is_err());
    }
    let mut orphan = animated_drawing();
    for entry in orphan["items"]["raster"].as_array_mut().unwrap() {
        entry["item"].as_object_mut().unwrap().remove("meshId");
    }
    orphan.as_object_mut().unwrap().remove("imageMeshes");
    assert!(
        decode_storage_project(orphan.to_string().as_bytes()).is_err(),
        "untyped legacy source cannot be inferred"
    );
}

#[test]
fn retained_source_collisions_remain_refused_after_detach_and_reopen() {
    let mut value = animated_drawing();
    value["propertyHolders"][0] = marked(value["propertyHolders"][0].clone(), "im_1");
    let mut owner = support::owner(&value);
    let bound = wire(&owner);
    run(&mut owner, "detach-collisions", detach(at(0)));
    let reopened =
        NativeObjectHistory::from_project_file("collision-reopen", &file(&owner)).unwrap();
    for source in [bound, wire(&owner), wire(&reopened)] {
        for kind in ["path", "text", "raster"] {
            let mut collision = source.clone();
            let target = target("scene-root", "L0", 0, "im_1");
            match kind {
                "path" => collision["objects"]
                    .as_array_mut()
                    .unwrap()
                    .push(fixture_path("scene-root", "L0", 0, "im_1")),
                "text" => collision["items"]["text"] = json!([text(target)]),
                _ => push_raster(&mut collision, target, None),
            }
            assert!(
                decode_storage_project(collision.to_string().as_bytes()).is_err(),
                "{kind}"
            );
        }
    }
}

#[test]
fn later_native_raster_creation_cannot_reuse_a_retained_mesh_identity_after_reopen() {
    let id = "native-raster:1:0";
    let mut value = animated_drawing();
    let mesh = value["imageMeshes"]
        .as_object_mut()
        .unwrap()
        .remove("im_1")
        .unwrap();
    value["imageMeshes"][id] = mesh;
    value["propertyHolders"][0]["target"]["strokeId"] = json!(id);
    for entry in value["items"]["raster"].as_array_mut().unwrap() {
        entry["item"]["meshId"] = json!(id);
    }
    let mut owner = support::owner(&value);
    run(&mut owner, "detach-native-id", detach(at(0)));
    let mut reopened =
        NativeObjectHistory::from_project_file("native-id-reopen", &file(&owner)).unwrap();
    assert_eq!(reopened.content_revision(), 0);
    let before = support::state(&reopened);
    let create = json!({"command":"object.raster.create","target":target("scene-root","L0",0,""),
        "item":support::raster_item("img-1",1,2,3,4)});
    assert_eq!(support::run(&mut reopened, &create), Err(E::InvalidRequest));
    assert_eq!(
        support::state(&reopened),
        before,
        "generated same-ID insertion publishes nothing"
    );
    assert_eq!(
        wire(&reopened)["propertyHolders"][0]["meshSource"]["meshId"],
        id
    );
}
