//! Original #2242 production contract through the composed planner, real codec, admitted assets,
//! revision owner, dispatcher and history; no replacement harness or owner is used.
//! Required command: test-history,evaluation,project_media_codec; preserve the existing kernel oracle.
//! Component codec/save admission now passes. Required local writes and actual frame-packet
//! consumption remain positive gates; remaining consumer refusals must not be ignored.
//! Animated replacement/detach must preserve authored tracks despite an unbound old mesh identity.
use super::{plan_binding_edit_json, BindingEditPlan, BindingReport};
use crate::history::NativeObjectHistory;
use crate::object_codec::encode_storage_project;
use crate::object_document::MeshIdSequence;
use crate::request_receipts::{DispatchErrorCode, OpacityRequest};
use serde_json::{json, Value};

#[path = "object_raster_asset_commands_support.rs"]
mod support;

#[path = "object_image_mesh_binding_fixtures.rs"]
mod fixtures;
use fixtures::*;

fn wire(owner: &NativeObjectHistory) -> Value {
    serde_json::from_slice(&support::bytes(owner)).unwrap()
}

fn payload(action: Value) -> Value {
    json!({"command":"object.image.mesh.edit","action":action})
}

fn attach(target: Value) -> Value {
    payload(json!({"kind":"attach","target":target,"mesh":mesh()}))
}

fn detach(target: Value) -> Value {
    payload(json!({"kind":"detach","target":target}))
}

fn duplicate(layer: &str) -> Value {
    payload(json!({"kind":"duplicate","contextId":"scene-root","layerUid":layer}))
}

fn request(owner: &NativeObjectHistory, id: &str, body: Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":owner.instance_id(),
        "documentId":owner.document_id(),"expectedRevision":owner.content_revision(),
        "operation":"command.document.object.image.mesh.edit","payload":body})
}

fn dispatch(owner: &mut NativeObjectHistory, value: &Value) -> Value {
    let request: OpacityRequest = serde_json::from_value(value.clone()).unwrap();
    let reply = owner
        .try_dispatch_object_command(&request)
        .unwrap()
        .unwrap();
    serde_json::to_value(reply).unwrap()
}

fn run(owner: &mut NativeObjectHistory, id: &str, body: Value) -> Value {
    let request = request(owner, id, body);
    let reply = dispatch(owner, &request);
    assert_eq!(reply["ok"], true, "{id}: {reply}");
    reply
}

fn assert_tag(owner: &NativeObjectHistory, target: Value, expected: Option<&str>) {
    let doc = wire(owner);
    let entries = doc["items"]["raster"].as_array().unwrap();
    let entry = entries
        .iter()
        .find(|entry| entry["target"] == target)
        .unwrap();
    assert_eq!(entry["item"]["meshId"].as_str(), expected, "{target}");
}

fn assert_tags(
    owner: &NativeObjectHistory,
    targets: impl IntoIterator<Item = Value>,
    id: Option<&str>,
) {
    for target in targets {
        assert_tag(owner, target, id);
    }
}

fn raster_targets(doc: &Value) -> Vec<Value> {
    doc["items"]["raster"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["target"].clone())
        .collect()
}

fn assert_binding_only(before: &Value, after: &Value) {
    for member in ["layers", "objects", "componentTables", "compositionOrder"] {
        assert_eq!(after[member], before[member], "{member}");
    }
    assert_eq!(after["items"]["text"], before["items"]["text"]);
    assert_eq!(
        raster_targets(after),
        raster_targets(before),
        "full native identities stay stable"
    );
}

fn undo_to(owner: &mut NativeObjectHistory, expected: &[u8]) {
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(support::bytes(owner), expected);
}

fn redo_to(owner: &mut NativeObjectHistory, expected: &[u8]) {
    owner.redo(owner.content_revision()).unwrap();
    assert_eq!(support::bytes(owner), expected);
}

fn plan(owner: &NativeObjectHistory, sequence: MeshIdSequence, body: Value) -> BindingEditPlan {
    plan_binding_edit_json(&support::head(owner), sequence, body.to_string().as_bytes()).unwrap()
}

fn project_file(owner: &NativeObjectHistory) -> Vec<u8> {
    let pin = support::head(owner).snapshot_id().to_owned();
    owner
        .serialize_project_file(owner.content_revision(), &pin)
        .unwrap()
}

fn replace_file(owner: &mut NativeObjectHistory, file: &[u8]) {
    let pin = support::head(owner).snapshot_id().to_owned();
    let replacement = owner.prepare_project_replacement(&pin, file).unwrap();
    owner.commit_project_replacement(replacement).unwrap();
}

fn assert_attach_counts(report: &BindingReport, expected: (usize, usize)) {
    let actual = match report {
        BindingReport::Attach {
            written, skipped, ..
        } => (*written, *skipped),
        _ => panic!("expected attachment report"),
    };
    assert_eq!(actual, expected);
}

fn assert_applied(reply: &Value, applied: bool) {
    assert_eq!(
        reply["result"],
        json!({"applied":applied,"historyEntriesAdded":u64::from(applied)})
    );
}

fn assert_error(owner: &mut NativeObjectHistory, request: &Value, code: &str) {
    assert_eq!(dispatch(owner, request)["error"]["code"], code);
}

fn assert_component_frames(owner: &NativeObjectHistory, mesh_id: Option<&str>) {
    use crate::object_document::FrameScopeKind;
    use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
    let snapshot = support::head(owner);
    for frame in 0..3 {
        let selector = ObjectFrameSelector {
            instance_id: snapshot.instance_id(),
            document_id: snapshot.document_id(),
            content_revision: snapshot.content_revision(),
            document_snapshot_id: snapshot.snapshot_id(),
            context_id: "component-A",
            scope_kind: FrameScopeKind::Authored,
            frame,
        };
        let packet = prepare_object_frame(&snapshot, &selector).unwrap();
        assert_eq!(packet.document_snapshot_id(), snapshot.snapshot_id());
        assert_eq!(packet.content_revision(), owner.content_revision());
        assert_eq!(packet.context_id(), "component-A");
        assert_eq!(packet.frame(), frame);
        assert_eq!(packet.source_layers()[0].layer_uid(), "inner-only");
        assert_eq!(packet.raster_records().len(), 1);
        let entry = packet.raster_records().next().unwrap().entry();
        assert_eq!(serde_json::to_value(&entry.target).unwrap(), inner(frame));
        assert_eq!(entry.item.mesh_id(), mesh_id);
        assert_eq!(entry.item.asset_id().as_str(), "img-1");
        let source = snapshot
            .document()
            .item_records()
            .unwrap()
            .raster_entries()
            .iter()
            .find(|source| source.target == entry.target)
            .unwrap();
        assert!(
            std::ptr::eq(entry, source),
            "the packet borrows its retained raster"
        );
    }
}

#[test]
fn all_24_frames_and_store_publish_as_one_history_entry_and_scrub_identity_survives() {
    let mut owner = support::owner(&drawing(24, None));
    let initial = support::bytes(&owner);
    let targets = raster_targets(&wire(&owner));
    assert_applied(&run(&mut owner, "attach", attach(at(13))), true);
    assert_eq!(owner.history_depths(), (1, 0));
    assert_tags(&owner, targets.clone(), Some("im_1"));
    let attached = support::bytes(&owner);
    assert_eq!(wire(&owner)["imageMeshes"]["im_1"], mesh());
    assert_applied(&run(&mut owner, "detach", detach(at(7))), true);
    assert_eq!(owner.history_depths(), (2, 0));
    assert_tags(&owner, targets, None);
    assert!(wire(&owner)["imageMeshes"].get("im_1").is_none());
    let detached = support::bytes(&owner);
    undo_to(&mut owner, &attached);
    undo_to(&mut owner, &initial);
    redo_to(&mut owner, &attached);
    redo_to(&mut owner, &detached);
}

#[test]
fn real_mixed_slots_count_non_rasters_and_missing_slots_without_touching_other_records() {
    let mut owner = support::owner(&mixed());
    let before = wire(&owner);
    let proposal = plan(&owner, MeshIdSequence::default(), attach(at(0)));
    assert_attach_counts(proposal.report(), (2, 2));
    assert_eq!(wire(&owner), before, "planning never publishes");
    let projected: Value =
        serde_json::from_slice(&encode_storage_project(proposal.document()).unwrap()).unwrap();
    assert_binding_only(&before, &projected);
    run(&mut owner, "mixed", attach(at(0)));
    assert_tag(&owner, at(0), Some("im_1"));
    assert_tag(&owner, at(1), Some("im_1"));
    assert_tag(&owner, target("scene-root", "L0", 0, "other-0"), None);
    assert_eq!(wire(&owner), projected);
}

#[test]
fn another_real_layer_keeps_a_mesh_until_its_last_reference_detaches() {
    let mut value = drawing(24, Some("im_1"));
    let other = target("scene-root", "L2", 0, "other");
    push_raster(&mut value, other.clone(), Some("im_1"));
    let mut owner = support::owner(&value);
    run(&mut owner, "first", detach(at(0)));
    assert_eq!(wire(&owner)["imageMeshes"]["im_1"], mesh());
    assert_tag(&owner, other.clone(), Some("im_1"));
    run(&mut owner, "last", detach(other));
    assert!(wire(&owner)["imageMeshes"].get("im_1").is_none());
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(wire(&owner)["imageMeshes"]["im_1"], mesh());
}

#[test]
fn duplicated_layer_shares_one_fresh_deep_copy_across_its_frames_and_editing_is_independent() {
    let value = duplicated_layer_drawing();
    let mut owner = support::owner(&value);
    run(&mut owner, "duplicate", duplicate("L2"));
    for frame in 0..24 {
        assert_tag(&owner, at(frame), Some("im_1"));
        assert_tag(
            &owner,
            target("scene-root", "L2", frame, &format!("copy-{frame}")),
            Some("im_2"),
        );
    }
    assert_eq!(
        wire(&owner)["imageMeshes"]["im_1"],
        wire(&owner)["imageMeshes"]["im_2"]
    );
    run(
        &mut owner,
        "drag-copy",
        json!({"command":"object.image.mesh.edit","meshId":"im_2",
        "action":{"kind":"setOffset","index":8,"du":0.25,"dv":-0.5}}),
    );
    assert_eq!(wire(&owner)["imageMeshes"]["im_1"], mesh());
    assert_eq!(
        wire(&owner)["imageMeshes"]["im_2"]["offsets"][8],
        json!([0.25, -0.5])
    );
    owner.undo(owner.content_revision()).unwrap();
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(
        wire(&owner),
        value,
        "duplicate and sculpt are separate atomic edits"
    );
}

#[test]
fn lifetime_ids_never_rewind_with_detach_undo_redo_or_project_replacement() {
    let mut owner = support::owner(&drawing(24, None));
    let original_file = project_file(&owner);
    run(&mut owner, "first", attach(at(0)));
    assert_tag(&owner, at(0), Some("im_1"));
    run(&mut owner, "detach", detach(at(0)));
    owner.undo(owner.content_revision()).unwrap();
    owner.redo(owner.content_revision()).unwrap();
    run(&mut owner, "second", attach(at(0)));
    assert_tag(&owner, at(0), Some("im_2"));
    owner.undo(owner.content_revision()).unwrap();
    run(&mut owner, "branch", attach(at(0)));
    assert_tag(&owner, at(0), Some("im_3"));
    replace_file(&mut owner, &original_file);
    assert_eq!(owner.history_depths(), (0, 0));
    run(&mut owner, "after-replacement", attach(at(0)));
    assert_tag(&owner, at(0), Some("im_4"));
}

#[test]
fn preparation_is_detached_and_discarded_stale_or_foreign_candidates_publish_nothing() {
    let mut owner = support::owner(&drawing(24, None));
    let body = attach(at(0)).to_string();
    let initial = support::state(&owner);
    drop(owner.prepare_image_mesh_edit_json(body.as_bytes()).unwrap());
    assert_eq!(support::state(&owner), initial);
    let stale = owner.prepare_image_mesh_edit_json(body.as_bytes()).unwrap();
    let candidate = owner.prepare_image_mesh_edit_json(body.as_bytes()).unwrap();
    let mut foreign = support::owner(&drawing(24, None));
    let foreign_before = support::state(&foreign);
    assert_eq!(
        foreign.commit_image_mesh_edit(candidate).unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    assert_eq!(support::state(&foreign), foreign_before);
    run(&mut owner, "commit", attach(at(0)));
    assert_tag(&owner, at(0), Some("im_1")); // discarded preparation spends no id
    let committed = support::state(&owner);
    assert_eq!(
        owner.commit_image_mesh_edit(stale).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(support::state(&owner), committed);
}

#[test]
fn replay_cancellation_stale_and_malformed_payloads_cannot_publish_or_erase_redo() {
    let mut owner = support::owner(&drawing(24, None));
    let initial = support::state(&owner);
    for (id, field, value, code) in [
        (
            "cancel",
            "cancelledBeforeDispatch",
            json!(true),
            "cancelled_before_dispatch",
        ),
        ("stale", "expectedRevision", json!(99), "stale_revision"),
        ("foreign", "documentId", json!("foreign"), "wrong_document"),
    ] {
        let mut request = request(&owner, id, attach(at(0)));
        request[field] = value;
        assert_error(&mut owner, &request, code);
        assert_eq!(support::state(&owner), initial);
    }
    let mut invalid = attach(at(0));
    invalid["action"]["unexpected"] = json!(true);
    let bad = request(&owner, "malformed", invalid);
    assert_error(&mut owner, &bad, "invalid_request");
    assert_eq!(support::state(&owner), initial);
    let original = request(&owner, "once", attach(at(0)));
    let reply = dispatch(&mut owner, &original);
    assert_applied(&reply, true);
    let applied = support::state(&owner);
    assert_eq!(dispatch(&mut owner, &original), reply);
    assert_eq!(support::state(&owner), applied);
    let mut conflict = original.clone();
    conflict["payload"]["action"]["mesh"]["offsets"][0] = json!([0.2, 0]);
    assert_error(&mut owner, &conflict, "invalid_request");
    assert_eq!(support::state(&owner), applied);
    owner.undo(owner.content_revision()).unwrap();
    let undone = support::state(&owner);
    assert_eq!(dispatch(&mut owner, &original), reply);
    assert_eq!(support::state(&owner), undone);
    assert_applied(&run(&mut owner, "noop", detach(at(0))), false);
    assert_eq!(
        support::state(&owner),
        undone,
        "a no-op keeps the pending redo"
    );
    run(&mut owner, "fresh", attach(at(0)));
    assert_tag(&owner, at(0), Some("im_2"));
}

#[test]
fn real_save_reload_keeps_all_frame_tags_and_the_mesh_store() {
    let mut owner = support::owner(&drawing(24, None));
    run(&mut owner, "attach", attach(at(9)));
    let saved_drawing = support::bytes(&owner);
    let reopened =
        NativeObjectHistory::from_project_file("reopened", &project_file(&owner)).unwrap();
    assert_eq!(support::bytes(&reopened), saved_drawing);
    assert_eq!(reopened.history_depths(), (0, 0));
    assert_tags(&reopened, (0..24).map(at), Some("im_1"));
}

#[test]
fn absent_video_provider_is_typed_unavailable_and_preserves_the_owner() {
    let mut owner = support::owner(&drawing(24, None));
    let initial = support::state(&owner);
    for (id, action) in [
        (
            "video-attach",
            json!({"kind":"attachVideo","contextId":"scene-root","layerUid":"L0","mesh":mesh()}),
        ),
        (
            "video-detach",
            json!({"kind":"detachVideo","contextId":"scene-root","layerUid":"L0"}),
        ),
    ] {
        let request = request(&owner, id, payload(action));
        assert_error(&mut owner, &request, "unavailable");
        assert_eq!(support::state(&owner), initial);
    }
}

#[test]
fn direct_propagate_and_release_use_the_kernel_without_spending_an_id_or_publishing() {
    let mut owner = support::owner(&drawing(24, Some("im_1")));
    let before = support::state(&owner);
    let mut sequence = MeshIdSequence::default();
    sequence.observe("im_73").unwrap();
    let release = payload(json!({"kind":"releaseIfUnused","meshId":"im_1"}));
    let retained = plan(&owner, sequence, release.clone());
    assert!(matches!(retained.report(), BindingReport::Released(false)));
    assert!(!retained.changed());
    assert_eq!(retained.origin_sequence().last(), 73);
    assert_eq!(retained.sequence().last(), 73);
    let clear = payload(json!({"kind":"propagate","target":at(8),"meshId":null}));
    let cleared = plan(&owner, sequence, clear.clone());
    assert!(matches!(
        cleared.report(),
        BindingReport::Propagate {
            written: 24,
            skipped: 0
        }
    ));
    assert!(cleared.changed());
    assert_eq!(cleared.sequence().last(), 73);
    assert!(cleared
        .document()
        .image_meshes()
        .unwrap()
        .get("im_1")
        .is_some());
    assert_eq!(support::state(&owner), before);
    run(&mut owner, "clear-tags", clear);
    let cleared_state = support::state(&owner);
    let released = plan(&owner, sequence, release.clone());
    assert!(matches!(released.report(), BindingReport::Released(true)));
    assert!(released.changed());
    assert_eq!(released.sequence().last(), 73);
    assert_eq!(support::state(&owner), cleared_state);
    run(&mut owner, "release", release);
    assert!(wire(&owner)["imageMeshes"].get("im_1").is_none());
}

#[test]
fn v2_mixed_order_save_reload_preserves_every_target_slot_and_authored_order() {
    let mut owner = support::owner(&mixed());
    run(&mut owner, "mixed-save", attach(at(0)));
    let expected = support::bytes(&owner);
    let mut reopened =
        NativeObjectHistory::from_project_file("v2-reopened", &project_file(&owner)).unwrap();
    assert_eq!(support::bytes(&reopened), expected);
    run(&mut reopened, "mixed-detach", detach(at(1)));
    assert_tag(&reopened, at(0), None);
    assert_tag(&reopened, at(1), None);
    assert_eq!(
        wire(&reopened)["compositionOrder"],
        mixed()["compositionOrder"]
    );
}

#[test]
fn importing_a_high_mesh_id_raises_the_existing_owner_lifetime_sequence() {
    let mut owner = support::owner(&drawing(24, None));
    run(&mut owner, "first-before-import", attach(at(0)));
    let imported = support::owner(&drawing(24, Some("im_900")));
    replace_file(&mut owner, &project_file(&imported));
    run(&mut owner, "after-high-import", attach(at(0)));
    assert_tag(&owner, at(0), Some("im_901"));
    run(&mut owner, "detach-high", detach(at(0)));
    run(&mut owner, "after-high-release", attach(at(0)));
    assert_tag(&owner, at(0), Some("im_902"));
}

#[test]
fn required_component_references_survive_root_detach_and_component_writes_use_local_frames() {
    // Inner-only and frame 2 exercise the component's own layer and bounds through real consumers.
    let mut owner = support::owner(&component_timed_drawing());
    run(&mut owner, "root-off", detach(at(0)));
    assert_eq!(wire(&owner)["imageMeshes"]["im_1"], mesh());
    assert_tags(&owner, (0..3).map(inner), Some("im_1"));
    assert_component_frames(&owner, Some("im_1"));
    run(&mut owner, "component-off", detach(inner(2)));
    assert!(wire(&owner)["imageMeshes"].get("im_1").is_none());
    assert_component_frames(&owner, None);
    run(&mut owner, "component-on", attach(inner(2)));
    assert_tags(&owner, (0..3).map(inner), Some("im_2"));
    assert_component_frames(&owner, Some("im_2"));
    assert_tag(&owner, at(0), None);
    assert_tag(&owner, at(1), None);
}

#[test]
fn required_component_tags_and_meshes_survive_real_project_save_reload() {
    let owner = support::owner(&component_drawing());
    let reopened =
        NativeObjectHistory::from_project_file("component-reopened", &project_file(&owner))
            .unwrap();
    assert_eq!(support::bytes(&reopened), support::bytes(&owner));
}

#[test]
fn required_component_mixed_slots_use_local_order_and_leave_root_and_other_slots_untouched() {
    let value = component_mixed();
    let other = target("component-A", "inner-only", 2, "other-inner");
    // Required positive gate: inner-only and component frame 2 must pass the real admission path.
    let mut owner = support::owner(&value);
    let before = wire(&owner);
    let initial = support::bytes(&owner);
    let proposal = plan(&owner, MeshIdSequence::default(), attach(inner(2)));
    assert_attach_counts(proposal.report(), (2, 1));
    assert_eq!(
        support::bytes(&owner),
        initial,
        "component planning publishes nothing"
    );
    run(&mut owner, "mixed-component-on", attach(inner(2)));
    assert_eq!(owner.history_depths(), (1, 0));
    assert_tags(&owner, [0, 2].map(inner), Some("im_2"));
    assert_tags(&owner, [inner(1), other, at(0), at(1)], Some("im_1"));
    let after = wire(&owner);
    assert_binding_only(&before, &after);
    run(&mut owner, "mixed-component-off", detach(inner(2)));
    assert_tag(&owner, inner(0), None);
    assert_tag(&owner, inner(2), None);
    assert_tag(&owner, inner(1), Some("im_1"));
    assert_tags(&owner, (0..2).map(at), Some("im_1"));
    assert!(wire(&owner)["imageMeshes"].get("im_2").is_none());
    assert_eq!(wire(&owner)["imageMeshes"]["im_1"], mesh());
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(wire(&owner), after);
    undo_to(&mut owner, &initial);
}

#[test]
fn duplicate_remaps_real_mesh_vertex_holder_and_rig_bind_without_rewriting_the_original() {
    let value = animated_duplicate_drawing();
    let mut owner = support::owner(&value);
    run(&mut owner, "duplicate-animated", duplicate("L2"));
    let after = wire(&owner);
    assert_eq!(after["propertyHolders"][0], value["propertyHolders"][0]);
    let mut copied = vertex_holder("L2");
    copied["target"]["strokeId"] = json!("im_2");
    assert_eq!(after["propertyHolders"][1], copied);
    assert_eq!(after["layers"][0]["rig"], value["layers"][0]["rig"]);
    let mut copied_rig = value["layers"][1]["rig"].clone();
    copied_rig["binds"][0]["meshId"] = json!("im_2");
    assert_eq!(after["layers"][1]["rig"], copied_rig);
}

#[test]
fn required_animated_binding_replacement_and_detach_preserve_authored_tracks() {
    let value = animated_drawing();
    let mut owner = support::owner(&value);
    // Required positive gate: current holder admission rejects the now unbound old identity.
    run(&mut owner, "replace-animated", attach(at(0)));
    assert_eq!(wire(&owner)["propertyHolders"], value["propertyHolders"]);
    owner.undo(owner.content_revision()).unwrap();
    run(&mut owner, "detach-animated", detach(at(0)));
    assert_eq!(wire(&owner)["propertyHolders"], value["propertyHolders"]);
}

#[test]
fn binding_history_and_replacement_conserve_retained_image_records_and_encoded_payloads() {
    use crate::object_document::{AssetId, AssetRecord};
    use std::sync::Arc;

    let images = |owner: &NativeObjectHistory| {
        let snapshot = support::head(owner);
        let inventory = snapshot.project().unwrap().asset_inventory().unwrap();
        ["img-1", "img-2"]
            .map(|id| Arc::clone(inventory.get(&AssetId::new(id.into()).unwrap()).unwrap()))
    };
    let saved_assets = |owner: &NativeObjectHistory| {
        let file: Value = serde_json::from_slice(&project_file(owner)).unwrap();
        serde_json::to_vec(&file["assets"]).unwrap()
    };
    let value = tagged_copy_drawing();
    let mut owner = support::owner(&value);
    let retained = support::head(&owner);
    let original = images(&owner);
    let asset_bytes = saved_assets(&owner);
    let assert_retained = |owner: &NativeObjectHistory, expected: &[Arc<AssetRecord>]| {
        for ((id, before), after) in ["img-1", "img-2"].iter().zip(expected).zip(images(owner)) {
            assert_eq!(after.id().as_str(), *id);
            assert!(Arc::ptr_eq(before, &after), "{id}: retained record");
            let before_bytes = before.source().encoded_allocation().unwrap();
            let after_bytes = after.source().encoded_allocation().unwrap();
            assert!(
                Arc::ptr_eq(before_bytes, after_bytes),
                "{id}: encoded allocation"
            );
            assert_eq!(after_bytes.as_slice(), support::png());
        }
        assert_eq!(saved_assets(owner), asset_bytes);
    };
    for (id, action) in [
        ("resource-attach", attach(at(0))),
        ("resource-duplicate", duplicate("L2")),
        ("resource-detach", detach(at(0))),
    ] {
        run(&mut owner, id, action);
        assert_retained(&owner, &original);
    }
    for redo in [false, false, false, true, true, true] {
        if redo {
            owner.redo(owner.content_revision()).unwrap();
        } else {
            owner.undo(owner.content_revision()).unwrap();
        }
        assert_retained(&owner, &original);
    }
    let saved = project_file(&owner);
    replace_file(&mut owner, &saved);
    assert_eq!(project_file(&owner), saved);
    let replacement = images(&owner);
    for (before, after) in original.iter().zip(&replacement) {
        // File admission may create new allocations; exact IDs and encoded bytes must survive it.
        assert_eq!(after.id(), before.id());
        assert_eq!(
            after.source().encoded_allocation(),
            before.source().encoded_allocation()
        );
    }
    assert_retained(&owner, &replacement);
    run(&mut owner, "replacement-resource-attach", attach(at(0)));
    owner.undo(owner.content_revision()).unwrap();
    owner.redo(owner.content_revision()).unwrap();
    assert_retained(&owner, &replacement);
    let old_inventory = retained.project().unwrap().asset_inventory().unwrap();
    for before in &original {
        assert!(Arc::ptr_eq(old_inventory.get(before.id()).unwrap(), before));
    }
}
