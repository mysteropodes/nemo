//! Original #2242 production contract, registered by the new command planner after shared composition.
//! These tests use the real codec, admitted assets, revision owner, dispatcher and history. They are
//! UNRUN while the planner/facade/owner registration is pending; no replacement harness or owner is used.
//! Required command: test-history,evaluation,project_media_codec; preserve the existing kernel oracle.
//! Required positive component gates below intentionally retain the full contract. Current blockers:
//! item admission uses root layers/frame bounds; component frame reads reject items; the planner
//! refuses component writes. Lead must compose those owners/consumers rather than ignore these gates.
//! Sequence commit, undo/replacement lifetime, and thin kernel facade are shared composition work.
//! Animated replacement/detach additionally need an explicit preserved-but-unbound holder state:
//! current holder admission requires a raster still carrying the old mesh id. Tracks must not be
//! silently deleted to make binding validation pass.
use super::{plan_binding_edit_json, BindingEditPlan, BindingReport};
use crate::history::NativeObjectHistory;
use crate::object_codec::encode_storage_project;
use crate::object_document::MeshIdSequence;
use crate::request_receipts::{DispatchErrorCode, OpacityRequest};
use serde_json::{json, Value};

#[path = "object_raster_asset_commands_support.rs"]
mod support;

fn mesh() -> Value {
    // Literal corpus fixture: eight boundary vertices, one interior vertex and eight triangles.
    json!({"outline":[[0,0],[0.5,0],[1,0],[1,0.5],[1,1],[0.5,1],[0,1],[0,0.5]],
        "verts":[[0,0],[0.5,0],[1,0],[1,0.5],[1,1],[0.5,1],[0,1],[0,0.5],[0.5,0.5]],
        "tris":[8,0,1,8,1,2,8,2,3,8,3,4,8,4,5,8,5,6,8,6,7,8,7,0],
        "offsets":[[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0]],"cols":2,"rows":2})
}

fn target(context: &str, layer: &str, frame: u32, stroke: &str) -> Value {
    json!({"contextId":context,"layerUid":layer,"frameScope":{"kind":"authored","frame":frame},
        "strokeId":stroke})
}

fn at(frame: u32) -> Value {
    target("scene-root", "L0", frame, &format!("raster-{frame}"))
}

fn inner(frame: u32) -> Value {
    target(
        "component-A",
        "inner-only",
        frame,
        &format!("inner-{frame}"),
    )
}

fn entry(target: Value, tag: Option<&str>) -> Value {
    let mut item = support::raster_item("img-1", 3, 5, 200, 100);
    if let Some(id) = tag {
        item["meshId"] = json!(id);
    }
    json!({"target":target,"item":item})
}

fn push_raster(doc: &mut Value, target: Value, tag: Option<&str>) {
    doc["items"]["raster"]
        .as_array_mut()
        .unwrap()
        .push(entry(target, tag));
}

fn fixture_path(context: &str, layer: &str, frame: u32, stroke: &str) -> Value {
    let mut path = support::legacy_record(layer, stroke);
    path["target"] = target(context, layer, frame, stroke);
    path
}

fn text(target: Value) -> Value {
    json!({"target":target,"block":{"text":"Hi",
        "font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":48,
        "color":"#112233","align":"left","anchorTopLeft":{"x":10,"y":20}}})
}

fn drawing(frames: u32, tag: Option<&str>) -> Value {
    let raster: Vec<_> = (0..frames).map(|frame| entry(at(frame), tag)).collect();
    let mut value = support::drawing(json!({"totalFrames":frames,
        "layers":[{"layerUid":"L0"},{"layerUid":"L2"}],"items":{"raster":raster}}));
    if let Some(id) = tag {
        let mut store = serde_json::Map::new();
        store.insert(id.to_string(), mesh());
        value["imageMeshes"] = Value::Object(store);
    }
    value
}

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

fn element(layer: &str, stroke: &str, position: u32) -> Value {
    json!({"layerUid":layer,"strokeId":stroke,"naturalPosition":position,"order":{}})
}

fn scope(frame: u32, elements: Vec<Value>) -> Value {
    json!({"frameScope":{"kind":"authored","frame":frame},"elements":elements})
}

fn mixed() -> Value {
    let mut value = drawing(4, None);
    value["formatVersion"] = json!(2);
    // Raster-array indices disagree with authored mixed slots; frame 2 has a path at slot 1,
    // and frame 3 has only slot 0. Different stroke IDs across frames must not prevent propagation.
    value["items"]["raster"] = json!([
        entry(at(0), None),
        entry(at(1), None),
        entry(target("scene-root", "L0", 0, "other-0"), None)
    ]);
    value["objects"] = json!([
        (0, "path-0"),
        (2, "path-2-first"),
        (2, "path-2"),
        (3, "path-3")
    ]
    .map(|(frame, stroke)| fixture_path("scene-root", "L0", frame, stroke)));
    value["items"]["text"] = json!([text(target("scene-root", "L0", 1, "text-1"))]);
    let e = |stroke, position| element("L0", stroke, position);
    value["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,
        "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
        "contexts":[{"contextId":"scene-root","layers":[
            {"layerUid":"L0","naturalPosition":0,"order":{}},
            {"layerUid":"L2","naturalPosition":1,"order":{}}],"scopes":[
            scope(0,vec![e("path-0",0),e("raster-0",1),e("other-0",2)]),
            scope(1,vec![e("text-1",0),e("raster-1",1)]),
            scope(2,vec![e("path-2-first",0),e("path-2",1)]),scope(3,vec![e("path-3",0)])]}]});
    value
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
    let mut value = drawing(24, Some("im_1"));
    for frame in 0..24 {
        let copy = target("scene-root", "L2", frame, &format!("copy-{frame}"));
        push_raster(&mut value, copy, Some("im_1"));
    }
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

fn component_drawing() -> Value {
    let mut value = drawing(2, Some("im_1"));
    value["formatVersion"] = json!(2);
    for frame in 0..3 {
        push_raster(&mut value, inner(frame), Some("im_1"));
    }
    value["componentTables"] = json!({"components":{"format":"nemo.native-component-set",
        "formatVersion":1,"components":[{"symbolId":"component-A","totalFrames":3,
            "layers":[{"layerUid":"inner-only"}],"objects":[]}],"instances":[]}});
    let root_scopes: Vec<_> = (0..2)
        .map(|frame| scope(frame, vec![element("L0", &format!("raster-{frame}"), 0)]))
        .collect();
    let inner_scopes: Vec<_> = (0..3)
        .map(|frame| {
            scope(
                frame,
                vec![element("inner-only", &format!("inner-{frame}"), 0)],
            )
        })
        .collect();
    value["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,
        "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
        "contexts":[{"contextId":"scene-root","layers":[
            {"layerUid":"L0","naturalPosition":0,"order":{}},{"layerUid":"L2","naturalPosition":1,"order":{}}],
            "scopes":root_scopes},{"contextId":"component-A","layers":[
            {"layerUid":"inner-only","naturalPosition":0,"order":{}}],"scopes":inner_scopes}]});
    value
}

#[test]
fn required_component_references_survive_root_detach_and_component_writes_use_local_frames() {
    // Required positive acceptance gate: currently blocked at REAL codec item admission because
    // inner-only is not a root layer and component frame 2 is beyond the root's two-frame bound.
    let mut owner = support::owner(&component_drawing());
    run(&mut owner, "root-off", detach(at(0)));
    assert_eq!(wire(&owner)["imageMeshes"]["im_1"], mesh());
    assert_tags(&owner, (0..3).map(inner), Some("im_1"));
    run(&mut owner, "component-off", detach(inner(2)));
    assert!(wire(&owner)["imageMeshes"].get("im_1").is_none());
    run(&mut owner, "component-on", attach(inner(2)));
    assert_tags(&owner, (0..3).map(inner), Some("im_2"));
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
    let mut value = component_drawing();
    let other = target("component-A", "inner-only", 2, "other-inner");
    push_raster(&mut value, other.clone(), Some("im_1"));
    value["componentTables"]["components"]["components"][0]["objects"] =
        json!([0, 2].map(|frame| fixture_path(
            "component-A",
            "inner-only",
            frame,
            &format!("inner-path-{frame}")
        )));
    value["items"]["text"] = json!([text(target("component-A", "inner-only", 1, "inner-text"))]);
    let e = |stroke, position| element("inner-only", stroke, position);
    value["compositionOrder"]["contexts"][1]["scopes"] = json!([
        scope(0, vec![e("inner-path-0", 0), e("inner-0", 1)]),
        scope(1, vec![e("inner-1", 0), e("inner-text", 1)]),
        scope(
            2,
            vec![e("inner-path-2", 0), e("inner-2", 1), e("other-inner", 2)]
        )
    ]);
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

fn vertex_holder(layer: &str) -> Value {
    json!({"target":{"contextId":"scene-root","layerUid":layer,"strokeId":"im_1"},
        "properties":[{"property":"vtx8","static":[0.1,0.2],"track":{"keys":[
            {"frame":0,"v":[0.1,0.2]},{"frame":23,"v":[0.3,0.4]}]}}]})
}

#[test]
fn duplicate_remaps_real_mesh_vertex_holder_and_rig_bind_without_rewriting_the_original() {
    let mut value = drawing(24, Some("im_1"));
    push_raster(
        &mut value,
        target("scene-root", "L2", 0, "copy"),
        Some("im_1"),
    );
    value["propertyHolders"] = json!([vertex_holder("L0"), vertex_holder("L2")]);
    let row = include_str!("fixtures/object-rig-record/oracle.txt")
        .lines()
        .find(|line| line.starts_with("rig\tmesh_bind_two_bones_rebind_replaces\t"))
        .unwrap();
    let rig: Value = serde_json::from_str(row.split('\t').nth(2).unwrap()).unwrap();
    value["layers"][0]["rig"] = rig.clone();
    value["layers"][1]["rig"] = rig;
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
    let mut value = drawing(24, Some("im_1"));
    value["propertyHolders"] = json!([vertex_holder("L0")]);
    let mut owner = support::owner(&value);
    // Required positive gate: current holder admission rejects the now unbound old identity.
    run(&mut owner, "replace-animated", attach(at(0)));
    assert_eq!(wire(&owner)["propertyHolders"], value["propertyHolders"]);
    owner.undo(owner.content_revision()).unwrap();
    run(&mut owner, "detach-animated", detach(at(0)));
    assert_eq!(wire(&owner)["propertyHolders"], value["propertyHolders"]);
}
