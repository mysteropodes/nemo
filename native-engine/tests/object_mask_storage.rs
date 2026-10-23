//! Literal original Path metadata controls. Runtime remains UNRUN until Root books it.
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_snapshot::ObjectSnapshot;
use serde_json::{json, Value};

#[path = "object_mask_storage_controls.rs"]
mod controls;

fn target(layer: &str, frame: u32, stroke: &str) -> Value {
    json!({"contextId":"scene-root","layerUid":layer,"strokeId":stroke,
        "frameScope":{"kind":"authored","frame":frame}})
}
fn record(layer: &str, frame: u32, stroke: &str, ext: Value) -> Value {
    let mut record = json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":target(layer, frame, stroke),
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world",
            "handleSpace":"relative","segments":[
                {"point":{"x":10,"y":20},"handleIn":{"x":0,"y":0},"handleOut":{"x":2,"y":3}},
                {"point":{"x":30,"y":20},"handleIn":{"x":-2,"y":-3},"handleOut":{"x":0,"y":0}},
                {"point":{"x":30,"y":40},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}}]},
        "fill":{"kind":"solid","r":0.25,"g":0.5,"b":0.75,"a":0.5},
        "vector":{"stroke":{"kind":"solid","r":1,"g":0,"b":0,"a":1},"hasRealStroke":true,
            "strokeWidth":7,"strokeCap":"round","strokeJoin":"bevel","opacity":0.6}});
    if !ext.is_null() {
        record["vectorExt"] = ext;
    }
    record
}
fn fixture(version: u32) -> Value {
    let mut document = json!({"format":"nemo.native-object-document","formatVersion":version,
        "totalFrames":3,"layers":[{"layerUid":"L0"},{"layerUid":"L1"}],"objects":[
            record("L0",0,"A",json!({"isMask":true,"maskMode":"subtract","maskFeather":3})),
            record("L0",0,"B",json!({"isMask":true,"maskFeather":null})),
            record("L0",0,"ink",json!({"isMask":false,"maskMode":null,"maskFeather":-2})),
            record("L0",1,"later",json!({"isMask":true,"maskFeather":7})),
            record("L1",0,"other",json!({"isMask":true,"maskFeather":9}))]});
    if version == 2 {
        document["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,
            "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
            "contexts":[{"contextId":"scene-root","layers":[
                {"layerUid":"L0","naturalPosition":0,"order":{}},
                {"layerUid":"L1","naturalPosition":1,"order":{}}],"scopes":[
                {"frameScope":{"kind":"authored","frame":0},"elements":[
                    {"layerUid":"L0","strokeId":"A","naturalPosition":0,"order":{}},
                    {"layerUid":"L0","strokeId":"B","naturalPosition":1,"order":{}},
                    {"layerUid":"L0","strokeId":"ink","naturalPosition":2,"order":{}},
                    {"layerUid":"L1","strokeId":"other","naturalPosition":0,"order":{}}]},
                {"frameScope":{"kind":"authored","frame":1},"elements":[
                    {"layerUid":"L0","strokeId":"later","naturalPosition":0,"order":{}}]}]}]});
    }
    document
}
fn make_owner(instance: &str, document: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        instance,
        decode_storage_project(document.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}
fn head(owner: &NativeObjectHistory) -> ObjectSnapshot {
    owner.acquire_snapshot(owner.content_revision()).unwrap()
}
fn state(owner: &NativeObjectHistory) -> Value {
    serde_json::to_value(head(owner).document()).unwrap()
}
fn footprint(owner: &NativeObjectHistory) -> (u64, (usize, usize), String, Vec<u8>) {
    let snapshot = head(owner);
    (
        owner.content_revision(),
        owner.history_depths(),
        snapshot.snapshot_id().into(),
        encode_storage_project(snapshot.document()).unwrap(),
    )
}
fn payload(intent: Value) -> Value {
    json!({"command":"object.vector.mask.edit","stableTarget":target("L0",0,"A"),"maskEdit":intent})
}
fn make_request(owner: &NativeObjectHistory, id: &str, payload: Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":owner.instance_id(),
        "documentId":owner.document_id(),"expectedRevision":owner.content_revision(),
        "operation":"command.document.object.vector.edit","payload":payload})
}
fn dispatch(owner: &mut NativeObjectHistory, request: Value) -> Value {
    serde_json::to_value(
        owner
            .try_dispatch_object_command(&serde_json::from_value(request).unwrap())
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}
fn send(owner: &mut NativeObjectHistory, id: &str, intent: Value) -> Value {
    let request = make_request(owner, id, payload(intent));
    dispatch(owner, request)
}

#[test]
fn original_path_tags_preserve_absence_null_false_zero_unknown_mode_and_authored_paint() {
    for version in [1, 2] {
        for ext in [
            json!({"isMask":null}),
            json!({"isMask":false}),
            json!({"isMask":true}),
            json!({"maskMode":null}),
            json!({"maskMode":""}),
            json!({"maskMode":"future-mode"}),
            json!({"maskMode":"x".repeat(512)}),
            json!({"maskFeather":null}),
            json!({"maskFeather":0}),
            json!({"maskFeather":-3}),
            json!({"isMask":true,"maskMode":"intersect","maskFeather":2.5}),
        ] {
            let mut source = fixture(version);
            source["objects"][0]["vectorExt"] = ext.clone();
            let document = decode_storage_project(source.to_string().as_bytes()).unwrap();
            let stored = serde_json::to_value(&document).unwrap();
            assert_eq!(stored, source);
            let first = &document.objects()[0];
            assert_eq!(first.is_mask(), ext["isMask"] == true);
            assert_eq!(first.mask_mode(), ext["maskMode"].as_str());
            assert_eq!(first.mask_feather(), ext["maskFeather"].as_f64());
            let bytes = encode_storage_project(&document).unwrap();
            assert_eq!(decode_storage_project(&bytes).unwrap(), document);
            assert!(!stored["layers"][0]
                .as_object()
                .unwrap()
                .contains_key("masks"));
        }
        let mut source = fixture(version);
        source["objects"][0]
            .as_object_mut()
            .unwrap()
            .remove("vectorExt");
        let document = decode_storage_project(source.to_string().as_bytes()).unwrap();
        assert!(!document.objects()[0].is_mask());
        assert_eq!(serde_json::to_value(&document).unwrap(), source);
    }
}

#[test]
fn actual_immutable_object_read_returns_tags_without_layer_aggregation_or_evaluation() {
    let owner = make_owner("mask-read", &fixture(2));
    let before = footprint(&owner);
    let snapshot = head(&owner);
    let request = json!({"apiVersion":2,"requestId":"read","instanceId":owner.instance_id(),
        "documentId":owner.document_id(),"operation":"query.document.object",
        "payload":{"atRevision":owner.content_revision(),"stableTarget":target("L0",0,"A")}});
    let response = snapshot
        .dispatch(serde_json::from_value(request).unwrap())
        .unwrap();
    let response = serde_json::to_value(response).unwrap();
    assert_eq!(response["ok"], true);
    assert_eq!(response["result"]["object"], fixture(2)["objects"][0]);
    assert_eq!(footprint(&owner), before);
}

#[test]
fn shared_feather_is_one_atomic_edit_of_only_same_layer_context_and_authored_frame_masks() {
    for version in [1, 2] {
        let source = fixture(version);
        let mut owner = make_owner("mask-feather", &source);
        let origin = head(&owner);
        let before = footprint(&owner);
        let edit = owner
            .prepare_vector_edit_json(
                payload(json!({"kind":"feather","value":5}))
                    .to_string()
                    .as_bytes(),
            )
            .unwrap();
        assert_eq!(footprint(&owner), before);
        assert_eq!(owner.commit_vector_edit(edit), Ok(1));
        let mut expected = source.clone();
        for index in [0, 1] {
            expected["objects"][index]["vectorExt"]["maskFeather"] = json!(5);
        }
        assert_eq!(state(&owner), expected);
        assert_eq!(owner.history_depths(), (1, 0));
        assert_eq!(serde_json::to_value(origin.document()).unwrap(), source);
        assert_eq!(owner.undo(1), Ok(2));
        assert_eq!(state(&owner), source);
        assert_eq!(owner.redo(2), Ok(3));
        assert_eq!(state(&owner), expected);
        let reopened = make_owner("mask-reopen", &state(&owner));
        assert_eq!(state(&reopened), expected);
    }
}

#[test]
fn mode_and_unset_touch_only_the_selected_tags_and_keep_other_vector_extensions() {
    let mut source = fixture(2);
    source["objects"][0]["vectorExt"]["boxAngle"] = json!(12);
    let mut owner = make_owner("mask-mode", &source);
    assert_eq!(
        send(
            &mut owner,
            "mode",
            json!({"kind":"mode","mode":"intersect"})
        )["ok"],
        true
    );
    let mut expected = source;
    expected["objects"][0]["vectorExt"]["maskMode"] = json!("intersect");
    assert_eq!(state(&owner), expected);
    assert_eq!(
        send(&mut owner, "unset", json!({"kind":"unset"}))["ok"],
        true
    );
    expected["objects"][0]["vectorExt"] = json!({"boxAngle":12});
    assert_eq!(state(&owner), expected);
    assert!(!head(&owner).document().objects()[0].is_mask());
    let mut plain = make_owner("unset-empty-ext", &fixture(1));
    assert_eq!(
        send(&mut plain, "unset", json!({"kind":"unset"}))["ok"],
        true
    );
    assert!(!state(&plain)["objects"][0]
        .as_object()
        .unwrap()
        .contains_key("vectorExt"));
}

#[test]
fn numeric_noop_keeps_exact_bytes_revision_redo_and_single_dispatch_receipt() {
    let mut owner = make_owner("mask-noop", &fixture(1));
    assert_eq!(
        send(&mut owner, "feather", json!({"kind":"feather","value":5}))["ok"],
        true
    );
    assert_eq!(
        send(
            &mut owner,
            "mode",
            json!({"kind":"mode","mode":"intersect"})
        )["ok"],
        true
    );
    owner.undo(2).unwrap();
    let before = footprint(&owner);
    let mode = send(
        &mut owner,
        "same-mode",
        json!({"kind":"mode","mode":"subtract"}),
    );
    assert_eq!(mode["result"]["applied"], false);
    assert_eq!(footprint(&owner), before);
    let noop = make_request(
        &owner,
        "noop",
        payload(json!({"kind":"feather","value":5.0})),
    );
    let response = dispatch(&mut owner, noop.clone());
    assert_eq!(
        response["result"],
        json!({"applied":false,"historyEntriesAdded":0,"operation":"mask.edit","strokeId":"A"})
    );
    assert_eq!(footprint(&owner), before);
    let replay = dispatch(&mut owner, noop);
    assert_eq!(replay["ok"], true);
    assert_eq!(replay["result"], response["result"]);
    assert_eq!(footprint(&owner), before);
    assert_eq!(owner.redo(owner.content_revision()), Ok(4));
}

#[test]
fn original_feather_panel_clamps_negative_input_but_imported_negative_storage_is_not_normalized() {
    let source = fixture(1);
    let mut owner = make_owner("mask-zero", &source);
    assert_eq!(
        send(&mut owner, "negative", json!({"kind":"feather","value":-4}))["ok"],
        true
    );
    let mut expected = source;
    expected["objects"][0]["vectorExt"]["maskFeather"] = json!(0);
    expected["objects"][1]["vectorExt"]["maskFeather"] = json!(0);
    assert_eq!(state(&owner), expected);
    assert_eq!(state(&owner)["objects"][2]["vectorExt"]["maskFeather"], -2);
    let before = footprint(&owner);
    assert_eq!(
        send(&mut owner, "zero", json!({"kind":"feather","value":0}))["result"]["applied"],
        false
    );
    assert_eq!(footprint(&owner), before);
}

#[test]
fn existing_vector_create_replace_and_delete_keep_the_real_mask_record_on_the_same_owner() {
    let mut owner = make_owner("mask-vector", &fixture(2));
    let mut created = record(
        "L0",
        0,
        "",
        json!({"isMask":true,"maskMode":"add","maskFeather":2}),
    );
    let create = make_request(
        &owner,
        "create",
        json!({"command":"object.vector.create","record":created}),
    );
    let response = dispatch(&mut owner, create);
    assert_eq!(response["ok"], true);
    let id = response["result"]["strokeId"].as_str().unwrap();
    created["target"]["strokeId"] = json!(id);
    assert_eq!(state(&owner)["objects"][5], created);
    let read = head(&owner);
    assert!(read.document().objects()[5].is_mask());
    created["vectorExt"]["maskMode"] = json!("subtract");
    let request = make_request(
        &owner,
        "replace",
        json!({"command":"object.vector.replace","record":created}),
    );
    assert_eq!(dispatch(&mut owner, request)["ok"], true);
    assert_eq!(state(&owner)["objects"][5], created);
    let request = make_request(
        &owner,
        "delete",
        json!({"command":"object.vector.delete","stableTarget":created["target"]}),
    );
    assert_eq!(dispatch(&mut owner, request)["ok"], true);
    assert_eq!(state(&owner)["objects"].as_array().unwrap().len(), 5);
    assert_eq!(owner.undo(3), Ok(4));
    assert_eq!(state(&owner)["objects"][5], created);
}

#[cfg(feature = "project_media_codec")]
#[test]
fn real_project_file_save_reopen_retains_tags_and_the_pinned_original_snapshot() {
    use crate::object_document::{ProjectEnvelope, ProjectMetadata};
    let source = fixture(2);
    let drawing = decode_storage_project(source.to_string().as_bytes()).unwrap();
    let project = ProjectEnvelope::no_media(ProjectMetadata::new(100, 100, 24.0).unwrap(), drawing);
    let mut owner = NativeObjectHistory::from_admitted_project("mask-project", project).unwrap();
    let original = head(&owner);
    let original_bytes = owner
        .serialize_project_file(0, original.snapshot_id())
        .unwrap();
    assert_eq!(
        send(&mut owner, "shared", json!({"kind":"feather","value":4}))["ok"],
        true
    );
    let current = head(&owner);
    let bytes = owner
        .serialize_project_file(1, current.snapshot_id())
        .unwrap();
    let reopened = NativeObjectHistory::from_project_file("mask-open", &bytes).unwrap();
    assert_eq!(state(&reopened), state(&owner));
    assert_eq!(reopened.history_depths(), (0, 0));
    assert_eq!(
        reopened.project_canvas_dimensions().unwrap(),
        Some((100, 100))
    );
    assert_eq!(
        owner
            .serialize_project_file(0, original.snapshot_id())
            .unwrap(),
        original_bytes
    );
    assert_eq!(serde_json::to_value(original.document()).unwrap(), source);
    assert!(owner
        .serialize_project_file(1, original.snapshot_id())
        .is_err());
    assert_eq!(owner.undo(1), Ok(2));
    assert_eq!(state(&owner), source);
    assert_eq!(owner.redo(2), Ok(3));
    assert_eq!(state(&owner), state(&reopened));
}

#[test]
fn shared_feather_does_not_touch_same_layer_and_frame_in_another_stored_context() {
    let mut source = fixture(2);
    let mut other = record("L0", 0, "opaque", json!({"isMask":true,"maskFeather":19}));
    other["target"]["contextId"] = json!("stored-context");
    source["objects"].as_array_mut().unwrap().push(other);
    let mut context = source["compositionOrder"]["contexts"][0].clone();
    context["contextId"] = json!("stored-context");
    context["scopes"] = json!([{"frameScope":{"kind":"authored","frame":0},"elements":[
        {"layerUid":"L0","strokeId":"opaque","naturalPosition":0,"order":{}}]}]);
    source["compositionOrder"]["contexts"]
        .as_array_mut()
        .unwrap()
        .push(context);
    let mut owner = make_owner("mask-context", &source);
    assert_eq!(
        send(&mut owner, "feather", json!({"kind":"feather","value":6}))["ok"],
        true
    );
    let mut expected = source;
    expected["objects"][0]["vectorExt"]["maskFeather"] = json!(6);
    expected["objects"][1]["vectorExt"]["maskFeather"] = json!(6);
    assert_eq!(state(&owner), expected);
}
