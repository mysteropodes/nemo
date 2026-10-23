//! Implicit component edits and protected opaque-context storage compatibility through real consumers.
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFrameError, ObjectFrameSelector};
use crate::request_receipts::{DispatchErrorCode as E, OpacityRequest};
use serde_json::{json, Value};

#[path = "object_image_mesh_binding_fixtures.rs"]
mod fixtures;
#[path = "object_raster_asset_commands_support.rs"]
mod support;
use fixtures::*;

fn wire(owner: &NativeObjectHistory) -> Value {
    serde_json::from_slice(&support::bytes(owner)).unwrap()
}

fn run(owner: &mut NativeObjectHistory, id: &str, body: Value) -> Value {
    let request: OpacityRequest = serde_json::from_value(json!({"apiVersion":2,"requestId":id,
        "instanceId":owner.instance_id(),"documentId":owner.document_id(),
        "expectedRevision":owner.content_revision(),"operation":"command.document.object.image.mesh.edit",
        "payload":body})).unwrap();
    let reply = owner
        .try_dispatch_object_command(&request)
        .unwrap()
        .unwrap();
    assert!(reply.is_ok(), "{id}: {reply:?}");
    serde_json::to_value(reply).unwrap()
}

fn tag(owner: &NativeObjectHistory, target: Value) -> Value {
    wire(owner)["items"]["raster"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["target"] == target)
        .unwrap()["item"]["meshId"]
        .clone()
}

fn frame_selector<'a>(
    snapshot: &'a crate::revision::ObjectSnapshot,
    context: &'a str,
    frame: u32,
) -> ObjectFrameSelector<'a> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: context,
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}

fn assert_implicit_packets(owner: &NativeObjectHistory, expected_tag: Value) {
    let snapshot = support::head(owner);
    for frame in 0..3 {
        let packet =
            prepare_object_frame(&snapshot, &frame_selector(&snapshot, "component-A", frame))
                .unwrap();
        assert_eq!(packet.raster_records().len(), 1);
        assert_eq!(packet.text_records().len(), 1);
        let raster = packet.raster_records().next().unwrap();
        let text = packet.text_records().next().unwrap();
        assert_eq!(
            serde_json::to_value(&raster.entry().target).unwrap(),
            inner(frame)
        );
        assert_eq!(raster.entry().item.mesh_id(), expected_tag.as_str());
        assert_eq!(
            serde_json::to_value(&text.entry().target).unwrap(),
            target(
                "component-A",
                "inner-only",
                frame,
                &format!("inner-text-{frame}")
            )
        );
        let indices: Vec<_> = packet.numeric_order().unwrap().layers()[0]
            .elements()
            .iter()
            .map(|element| element.source_object_index())
            .collect();
        assert_eq!(indices, vec![2 + frame as usize, 5 + frame as usize]);
        assert_eq!(
            (raster.source_index(), text.source_index()),
            (indices[0], indices[1])
        );
    }
}

fn project_file(owner: &NativeObjectHistory) -> Vec<u8> {
    let snapshot = support::head(owner);
    owner
        .serialize_project_file(owner.content_revision(), snapshot.snapshot_id())
        .unwrap()
}

fn opaque_drawing() -> Value {
    let mut value = component_drawing();
    value.as_object_mut().unwrap().remove("componentTables");
    value["items"]["raster"]
        .as_array_mut()
        .unwrap()
        .retain(|entry| entry["target"]["contextId"] == "scene-root");
    value["items"]["raster"].as_array_mut().unwrap().push(entry(
        target("opaque-storage", "L0", 1, "opaque-raster"),
        Some("im_1"),
    ));
    value["compositionOrder"]["contexts"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    value["compositionOrder"]["contexts"].as_array_mut().unwrap().push(json!({
        "contextId":"opaque-storage","layers":[
            {"layerUid":"L0","naturalPosition":0,"order":{}},{"layerUid":"L2","naturalPosition":1,"order":{}}],
        "scopes":[scope(1,vec![element("L0","opaque-raster",0)])]}));
    value
}

#[test]
fn implicit_mixed_component_order_supports_local_binding_edits_and_unrelated_root_edits() {
    let value = implicit_component_drawing();
    let mut entered = support::owner(&value);
    let initial = support::bytes(&entered);
    assert_implicit_packets(&entered, json!("im_1"));
    run(&mut entered, "implicit-local-attach", attach(inner(2)));
    assert_implicit_packets(&entered, json!("im_2"));
    assert_eq!(tag(&entered, at(0)), "im_1");
    assert_eq!(wire(&entered)["items"]["text"], value["items"]["text"]);
    assert_eq!(
        wire(&entered)["compositionOrder"],
        value["compositionOrder"]
    );
    let attached = support::bytes(&entered);
    run(&mut entered, "implicit-local-detach", detach(inner(0)));
    assert_implicit_packets(&entered, Value::Null);
    assert!(wire(&entered)["imageMeshes"].get("im_2").is_none());
    entered.undo(entered.content_revision()).unwrap();
    assert_eq!(support::bytes(&entered), attached);
    entered.undo(entered.content_revision()).unwrap();
    assert_eq!(support::bytes(&entered), initial);
    let mut root = support::owner(&value);
    run(&mut root, "implicit-root-detach", detach(at(0)));
    assert_implicit_packets(&root, json!("im_1"));
    assert_eq!(tag(&root, at(0)), Value::Null);
    assert_eq!(wire(&root)["imageMeshes"]["im_1"], mesh());
    run(&mut root, "implicit-root-attach", attach(at(1)));
    assert_eq!(tag(&root, at(0)), "im_2");
    assert_implicit_packets(&root, json!("im_1"));
    assert_eq!(wire(&root)["items"]["text"], value["items"]["text"]);
    assert_eq!(wire(&root)["compositionOrder"], value["compositionOrder"]);
}

#[test]
fn opaque_v2_items_roundtrip_and_keep_mesh_references_without_becoming_binding_or_frame_targets() {
    let value = opaque_drawing();
    let document = decode_storage_project(value.to_string().as_bytes()).unwrap();
    let saved = encode_storage_project(&document).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&saved).unwrap(), value);
    assert_eq!(
        encode_storage_project(&decode_storage_project(&saved).unwrap()).unwrap(),
        saved
    );
    let mut owner = support::owner(&value);
    run(&mut owner, "opaque-root-detach", detach(at(0)));
    let opaque = target("opaque-storage", "L0", 1, "opaque-raster");
    assert_eq!(tag(&owner, opaque.clone()), "im_1");
    assert_eq!(wire(&owner)["imageMeshes"]["im_1"], mesh());
    let before = support::state(&owner);
    let release = payload(json!({"kind":"releaseIfUnused","meshId":"im_1"}));
    let reply = run(&mut owner, "opaque-retains", release.clone());
    assert_eq!(
        reply["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(support::state(&owner), before);
    for action in [attach(opaque.clone()), detach(opaque)] {
        assert_eq!(
            owner
                .prepare_image_mesh_edit_json(action.to_string().as_bytes())
                .unwrap_err(),
            E::NotFound
        );
        assert_eq!(support::state(&owner), before);
    }
    let snapshot = support::head(&owner);
    assert_eq!(
        prepare_object_frame(&snapshot, &frame_selector(&snapshot, "opaque-storage", 1))
            .unwrap_err(),
        ObjectFrameError::Unavailable
    );
    let reopened =
        NativeObjectHistory::from_project_file("opaque-reopen", &project_file(&owner)).unwrap();
    assert_eq!(support::bytes(&reopened), support::bytes(&owner));
    // A later admitted replacement removes the final stored reference; release then works normally.
    let mut unreferenced = wire(&owner);
    unreferenced["items"]["raster"]
        .as_array_mut()
        .unwrap()
        .retain(|entry| entry["target"]["contextId"] == "scene-root");
    unreferenced["compositionOrder"]["contexts"][1]["scopes"] = json!([]);
    let replacement_file = project_file(&support::owner(&unreferenced));
    let pin = support::head(&owner).snapshot_id().to_owned();
    let replacement = owner
        .prepare_project_replacement(&pin, &replacement_file)
        .unwrap();
    owner.commit_project_replacement(replacement).unwrap();
    run(&mut owner, "opaque-final-release", release);
    assert!(wire(&owner)["imageMeshes"].get("im_1").is_none());
}

#[test]
fn restored_opaque_storage_does_not_weaken_declared_component_bounds_or_explicit_root_coverage() {
    let mut component_layer = component_drawing();
    component_layer["items"]["raster"][2]["target"]["layerUid"] = json!("L0");
    assert!(decode_storage_project(component_layer.to_string().as_bytes()).is_err());
    let mut component_frame = component_drawing();
    component_frame["items"]["raster"][4]["target"]["frameScope"]["frame"] = json!(3);
    assert!(decode_storage_project(component_frame.to_string().as_bytes()).is_err());
    let mut opaque_frame = opaque_drawing();
    opaque_frame["items"]["raster"][2]["target"]["frameScope"]["frame"] = json!(2);
    assert!(decode_storage_project(opaque_frame.to_string().as_bytes()).is_err());
    let mut opaque_layer = opaque_drawing();
    opaque_layer["items"]["raster"][2]["target"]["layerUid"] = json!("inner-only");
    assert!(decode_storage_project(opaque_layer.to_string().as_bytes()).is_err());
    let mut uncovered = implicit_component_drawing();
    uncovered["compositionOrder"]["contexts"][0]["scopes"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    assert!(decode_storage_project(uncovered.to_string().as_bytes()).is_err());
    let owner = support::owner(&implicit_component_drawing());
    let before = support::state(&owner);
    let unknown = target("genuinely-unknown", "L0", 0, "raster-0");
    assert_eq!(
        owner
            .prepare_image_mesh_edit_json(attach(unknown).to_string().as_bytes())
            .unwrap_err(),
        E::NotFound
    );
    assert_eq!(support::state(&owner), before);
}

#[test]
fn opaque_generic_item_writes_are_unavailable_without_narrowing_declared_component_targets() {
    let mut owner = support::owner(&opaque_drawing());
    let before = support::state(&owner);
    let opaque = target("opaque-storage", "L0", 1, "opaque-raster");
    let empty = target("opaque-storage", "L0", 0, "");
    for body in [
        json!({"command":"object.raster.move","stableTarget":opaque.clone(),"x":7,"y":8}),
        json!({"command":"object.raster.delete","stableTarget":opaque}),
        json!({"command":"object.raster.create","target":empty.clone(),"item":support::raster_item("img-1",1,2,3,4)}),
        json!({"command":"object.text.create","target":empty,"block":text(target("opaque-storage","L0",0,""))["block"]}),
    ] {
        assert_eq!(support::prepare(&owner, &body).unwrap_err(), E::Unavailable);
        assert_eq!(support::state(&owner), before);
    }
    // The added context guard must not reject a real component or alias its full target to root.
    let mut value = component_drawing();
    value["componentTables"]["components"]["components"][0]["layers"][0]["layerUid"] = json!("L0");
    for item in value["items"]["raster"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .filter(|entry| entry["target"]["contextId"] == "component-A")
    {
        item["target"]["layerUid"] = json!("L0");
    }
    value["compositionOrder"]["contexts"][1]["layers"][0]["layerUid"] = json!("L0");
    for scope in value["compositionOrder"]["contexts"][1]["scopes"]
        .as_array_mut()
        .unwrap()
    {
        scope["elements"][0]["layerUid"] = json!("L0");
    }
    let mut component = support::owner(&value);
    let moved = target("component-A", "L0", 0, "inner-0");
    let body = json!({"command":"object.raster.move","stableTarget":moved.clone(),"x":7,"y":8});
    support::run(&mut component, &body).unwrap();
    let after = wire(&component);
    assert_eq!(after["items"]["raster"][0], value["items"]["raster"][0]);
    let local = after["items"]["raster"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["target"] == moved)
        .unwrap();
    assert_eq!(
        (local["item"]["x"].clone(), local["item"]["y"].clone()),
        (json!(7), json!(8))
    );
}
