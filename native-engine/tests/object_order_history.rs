//! Whole versioned images through actual native revision/fill/history owners.
use crate::history::NativeObjectHistory;
use crate::object_codec::{
    decode_project, decode_storage_project, encode_project, encode_storage_project,
    ObjectCodecErrorKind,
};
use crate::object_document::ObjectDocument;
use crate::request_receipts::{DispatchErrorCode, OpacityRequest};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

fn record(context: &str, kind: &str) -> Value {
    json!({"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
        "target":{"contextId":context,"frameScope":{"kind":kind,"frame":7},"layerUid":"L0","strokeId":"same"},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
        "segments":[{"point":{"x":-10.125,"y":0},"handleIn":{"x":1,"y":0},"handleOut":{"x":2,"y":0}},
        {"point":{"x":10,"y":20},"handleIn":{"x":0,"y":-3},"handleOut":{"x":0,"y":0}}]},
        "fill":{"kind":"solid","r":0.2,"g":0.4,"b":0.6,"a":0.75}})
}
fn plan_context(context: &str) -> Value {
    json!({"contextId":context,"layers":[
        {"layerUid":"L0","naturalPosition":0,"order":{"static":[0],"track":{"keys":[]},
        "expression":{"enabled":false,"code":"value + 1"}}},
        {"layerUid":"L1","naturalPosition":1,"order":{"expression":{"enabled":true,"code":""}}}],
        "scopes":[{"frameScope":{"kind":"authored","frame":7},"elements":[
        {"layerUid":"L0","strokeId":"same","naturalPosition":0,"order":{"track":{"keys":[
        {"frame":-0.5,"v":[1],"hold":false,"curvePoints":null,"hIn":[0,0],"hOut":[1,0]},
        {"frame":0,"v":[2],"curvePoints":[{"x":0,"y":0,"tx":1},{"x":1,"y":1}]},
        {"frame":10,"v":[3]},{"frame":40,"v":[4],"curvePoints":[]}]}}}]}]})
}
fn source() -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":21,
        "layers":[{"layerUid":"L1"},{"layerUid":"L0"}],
        "objects":[record("scene-root","authored"),record("scene-root","reference")],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "contexts":[plan_context("scene-root")],"migration":"explicit-plan-required","referencePolicy":"unresolved"}})
}
fn raw(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap()
}
fn owner(value: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new("order-core", decode_storage_project(&raw(value)).unwrap()).unwrap()
}
fn current(owner: &NativeObjectHistory) -> ObjectSnapshot {
    owner.acquire_snapshot(owner.content_revision()).unwrap()
}
fn image(snapshot: &ObjectSnapshot) -> Value {
    serde_json::from_slice(&encode_storage_project(snapshot.document()).unwrap()).unwrap()
}
fn payload(target: &Value) -> Value {
    json!({"command":"object.fill.set","stableTarget":target,"fill":{"kind":"solid","r":0.9,"g":0.1,"b":0.3,"a":0.5}})
}
fn serialize_request(snapshot: &ObjectSnapshot, revision: u64) -> OpacityRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"order-serialize","instanceId":snapshot.instance_id(),
        "documentId":snapshot.document_id(),"operation":"query.document.object.serialize","payload":{"atRevision":revision}})).unwrap()
}
fn serialized(snapshot: &ObjectSnapshot) -> Vec<u8> {
    let response = snapshot
        .dispatch(serialize_request(snapshot, snapshot.content_revision()))
        .unwrap();
    assert!(response.ok, "{:?}", response.error);
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    let result = response.result.unwrap();
    assert_eq!(result["documentSnapshotId"], snapshot.snapshot_id());
    assert_eq!(result["atRevision"], snapshot.content_revision());
    result["documentJson"].as_str().unwrap().as_bytes().to_vec()
}
fn read_request(snapshot: &ObjectSnapshot, target: &Value) -> Value {
    json!({"apiVersion":2,"requestId":"order-read","instanceId":snapshot.instance_id(),"documentId":snapshot.document_id(),
        "operation":"query.document.object","payload":{"atRevision":snapshot.content_revision(),"stableTarget":target}})
}

#[test]
fn object_order_history_preserves_full_image_at_fill_undo_redo_and_old_acquisition() {
    let before = source();
    let mut after = before.clone();
    after["objects"][0]["fill"] = payload(&before["objects"][0]["target"])["fill"].clone();
    let mut history = owner(&before);
    let zero = current(&history);
    let candidate = history
        .prepare_fill_json(&raw(&payload(&before["objects"][0]["target"])))
        .unwrap();
    assert_eq!(history.commit_fill(candidate), Ok(1));
    let one = current(&history);
    assert_eq!(history.undo(1), Ok(2));
    let two = current(&history);
    assert_eq!(history.redo(2), Ok(3));
    let three = current(&history);
    assert_eq!(history.history_depths(), (1, 0));
    for (snapshot, expected, revision) in [
        (&zero, &before, 0),
        (&one, &after, 1),
        (&two, &before, 2),
        (&three, &after, 3),
    ] {
        assert_eq!(snapshot.content_revision(), revision);
        assert_eq!(image(snapshot), *expected);
        let saved = serialized(snapshot);
        assert_eq!(saved, encode_storage_project(snapshot.document()).unwrap());
        assert_eq!(serde_json::from_slice::<Value>(&saved).unwrap(), *expected);
        assert_eq!(expected["compositionOrder"], before["compositionOrder"]);
        assert_eq!(expected["objects"][1], before["objects"][1]);
    }
    drop(history);
    assert_eq!(image(&zero), before);
    assert_eq!(image(&three), after);
}

#[test]
fn object_order_history_same_snapshot_disk_reload_gets_fresh_core_identity_zero_history() {
    let initial = source();
    let mut history = owner(&initial);
    let candidate = history
        .prepare_fill_json(&raw(&payload(&initial["objects"][0]["target"])))
        .unwrap();
    history.commit_fill(candidate).unwrap();
    let pinned = current(&history);
    let bytes = serialized(&pinned);
    let expected = image(&pinned);
    drop(history);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "nemo-order-history-{}-{nonce}.json",
        std::process::id()
    ));
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    file.write_all(&bytes).unwrap();
    file.sync_all().unwrap();
    drop(file);
    let disk = std::fs::read(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(disk, bytes);
    let fresh =
        NativeObjectHistory::new("order-core", decode_storage_project(&disk).unwrap()).unwrap();
    assert_ne!(fresh.document_id(), pinned.document_id());
    assert_eq!(fresh.content_revision(), 0);
    assert_eq!(fresh.history_depths(), (0, 0));
    let reopened = current(&fresh);
    assert_eq!(image(&reopened), expected);
    assert_eq!(serialized(&reopened), bytes);
    assert_eq!(
        reopened
            .query_json(&raw(&read_request(
                &pinned,
                &initial["objects"][0]["target"]
            )))
            .unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
}

#[test]
fn object_order_history_nonroot_authored_selection_and_reference_behavior_refuse_without_mutation()
{
    let mut value = source();
    value["objects"]
        .as_array_mut()
        .unwrap()
        .push(record("symbol-A", "authored"));
    value["compositionOrder"]["contexts"]
        .as_array_mut()
        .unwrap()
        .push(plan_context("symbol-A"));
    let mut history = owner(&value);
    let zero = current(&history);
    for index in [0, 2] {
        let response = zero
            .query_json(&raw(&read_request(
                &zero,
                &value["objects"][index]["target"],
            )))
            .unwrap();
        assert_eq!(
            serde_json::to_value(response).unwrap()["result"]["object"],
            value["objects"][index]
        );
    }
    let reference = payload(&value["objects"][1]["target"]);
    assert_eq!(
        history.prepare_fill_json(&raw(&reference)).unwrap_err(),
        DispatchErrorCode::Unavailable
    );
    assert_eq!(
        zero.query_json(&raw(&read_request(&zero, &value["objects"][1]["target"])))
            .unwrap_err(),
        DispatchErrorCode::Unavailable
    );
    assert_eq!(history.content_revision(), 0);
    assert_eq!(history.history_depths(), (0, 0));
    assert_eq!(image(&current(&history)), value);
    let candidate = history
        .prepare_fill_json(&raw(&payload(&value["objects"][2]["target"])))
        .unwrap();
    history.commit_fill(candidate).unwrap();
    let changed = image(&current(&history));
    assert_eq!(changed["objects"][0], value["objects"][0]);
    assert_eq!(changed["objects"][1], value["objects"][1]);
    assert_eq!(
        changed["objects"][2]["fill"],
        payload(&value["objects"][2]["target"])["fill"]
    );
    assert_eq!(changed["compositionOrder"], value["compositionOrder"]);
}

#[test]
fn object_order_history_invalid_constructor_and_strict_canonical_v1_cannot_bypass_storage_admission(
) {
    let mut invalid = source();
    invalid.as_object_mut().unwrap().remove("compositionOrder");
    let direct: ObjectDocument = serde_json::from_value(invalid).unwrap();
    assert!(ObjectSnapshot::new("direct", direct).is_err());
    invalid = source();
    invalid["compositionOrder"]["contexts"][0]["layers"][0]["naturalPosition"] = json!(2);
    let direct: ObjectDocument = serde_json::from_value(invalid).unwrap();
    assert!(NativeObjectHistory::new("direct", direct).is_err());
    let mut legacy = source();
    legacy["formatVersion"] = json!(1);
    legacy.as_object_mut().unwrap().remove("compositionOrder");
    let v1 = decode_project(&raw(&legacy)).unwrap();
    let v1bytes = encode_project(&v1).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&v1bytes).unwrap(), legacy);
    let history = NativeObjectHistory::new("legacy", v1).unwrap();
    assert_eq!(serialized(&current(&history)), v1bytes);
    for extra in [Value::Null, source()["compositionOrder"].clone()] {
        let mut wrong = legacy.clone();
        wrong["compositionOrder"] = extra;
        assert_eq!(
            decode_project(&raw(&wrong)).unwrap_err().kind(),
            ObjectCodecErrorKind::Unsupported
        );
        assert!(decode_storage_project(&raw(&wrong)).is_err());
    }
    assert!(encode_project(current(&owner(&source())).document()).is_err());
}

#[test]
fn object_order_history_noop_stale_malformed_and_oversized_responses_keep_complete_owner_image() {
    let value = source();
    let mut history = owner(&value);
    let target = &value["objects"][0]["target"];
    let mut noop = payload(target);
    noop["fill"] = value["objects"][0]["fill"].clone();
    let candidate = history.prepare_fill_json(&raw(&noop)).unwrap();
    assert_eq!(history.commit_fill(candidate), Ok(0));
    assert_eq!(history.undo(0), Err(DispatchErrorCode::Unavailable));
    let candidate = history.prepare_fill_json(&raw(&payload(target))).unwrap();
    let stale = candidate.clone();
    history.commit_fill(candidate).unwrap();
    assert_eq!(
        history.commit_fill(stale),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(history.undo(0), Err(DispatchErrorCode::StaleRevision));
    let before = image(&current(&history));
    assert!(history
        .prepare_fill_json(b"{\"command\":\"object.fill.set\",\"command\":\"object.fill.set\"}")
        .is_err());
    assert_eq!(image(&current(&history)), before);
    assert_eq!(history.content_revision(), 1);
    let snapshot = current(&history);
    let response = snapshot.dispatch(serialize_request(&snapshot, 0)).unwrap();
    assert!(!response.ok);
    assert_eq!(response.error.unwrap().code, DispatchErrorCode::NotFound);
    let mut large = source();
    large["compositionOrder"]["contexts"][0]["layers"][0]["order"]["expression"]["code"] =
        json!("x".repeat(4096));
    let big = owner(&large);
    let snapshot = current(&big);
    assert_eq!(image(&snapshot), large);
    let response = snapshot.dispatch(serialize_request(&snapshot, 0)).unwrap();
    assert!(!response.ok);
    assert_eq!(response.error.unwrap().code, DispatchErrorCode::Unavailable);
    assert_eq!(big.content_revision(), 0);
    assert_eq!(big.history_depths(), (0, 0));
    assert_eq!(image(&current(&big)), large);
}

#[cfg(feature = "evaluation")]
#[test]
fn object_order_history_v2_authored_numeric_packet_preserves_pins_and_reference_refusal() {
    use crate::object_document::FrameScopeKind;
    use crate::object_frame_packet::{prepare_object_frame, ObjectFrameError, ObjectFrameSelector};
    let mut initial = source();
    // Explicit empty curves are linear: frame 7 between (0, 2) and (10, 3) is 2.7.
    initial["compositionOrder"]["contexts"][0]["scopes"][0]["elements"][0]["order"]["track"]
        ["keys"][1]["curvePoints"] = json!([]);
    let mut history = owner(&initial);
    let zero = current(&history);
    let before = encode_storage_project(zero.document()).unwrap();
    let candidate = history
        .prepare_fill_json(&raw(&payload(&initial["objects"][0]["target"])))
        .unwrap();
    assert_eq!(history.commit_fill(candidate), Ok(1));
    assert_eq!(history.undo(1), Ok(2));
    let snapshot = current(&history);
    let snapshot_id = snapshot.snapshot_id().to_owned();
    assert_eq!(history.history_depths(), (0, 1));
    let selector = ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: 2,
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 7,
    };
    let packet = prepare_object_frame(&snapshot, &selector).unwrap();
    assert_eq!(packet.instance_id(), snapshot.instance_id());
    assert_eq!(packet.document_id(), snapshot.document_id());
    assert_eq!(packet.content_revision(), 2);
    assert_eq!(packet.document_snapshot_id(), snapshot_id);
    assert_eq!(packet.context_id(), "scene-root");
    assert_eq!(packet.scope_kind(), &FrameScopeKind::Authored);
    assert_eq!(packet.frame(), 7);
    assert_eq!(
        packet
            .source_layers()
            .iter()
            .map(|layer| layer.layer_uid())
            .collect::<Vec<_>>(),
        vec!["L1", "L0"]
    );
    assert_eq!(packet.source_frame_for_layer(0), Some(7));
    assert_eq!(packet.source_frame_for_layer(1), Some(7));
    let ordered = packet.numeric_order().unwrap().layers();
    // L0's explicit zero clamps to the front; the public sequence is back-to-front.
    // L1's enabled empty expression has no numeric routing, so its natural rank is 1.
    assert_eq!(
        ordered
            .iter()
            .map(|layer| (
                packet.source_layers()[layer.source_layer_index()].layer_uid(),
                layer.value(),
                layer.explicit()
            ))
            .collect::<Vec<_>>(),
        vec![("L1", 1.0, false), ("L0", 0.0, true)]
    );
    assert!(ordered[0].elements().is_empty());
    assert_eq!(ordered[1].elements().len(), 1);
    let element = &ordered[1].elements()[0];
    assert_eq!(element.source_object_index(), 0);
    assert!((element.value() - 2.7).abs() < 1e-12);
    let records: Vec<_> = packet.records().collect();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].source_layer_index(), 1);
    assert_eq!(records[0].source_object_index(), 0);
    assert!(std::ptr::eq(
        records[0].record(),
        &snapshot.document().objects()[0]
    ));
    assert_eq!(
        serde_json::to_value(records[0].record()).unwrap(),
        initial["objects"][0]
    );
    let mut reference = selector.clone();
    reference.scope_kind = FrameScopeKind::Reference;
    assert_eq!(
        prepare_object_frame(&snapshot, &reference).unwrap_err(),
        ObjectFrameError::Unavailable
    );
    assert_eq!(history.content_revision(), 2);
    assert_eq!(history.history_depths(), (0, 1));
    assert_eq!(current(&history).snapshot_id(), snapshot_id);
    assert_eq!(image(&current(&history)), initial);
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
    assert_eq!(encode_storage_project(zero.document()).unwrap(), before);
    assert_eq!(zero.content_revision(), 0);
    assert_ne!(zero.snapshot_id(), snapshot_id);
    drop(history);
    assert_eq!(packet.content_revision(), 2);
    assert_eq!(packet.document_snapshot_id(), snapshot_id);
    assert_eq!(
        serde_json::to_value(packet.records().next().unwrap().record()).unwrap(),
        initial["objects"][0]
    );
}
