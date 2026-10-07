//! Independent N25A content oracle through the sole staged object history owner.
use crate::commands::{prepare_object_fill_json, PreparedObjectFill};
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_project, encode_project};
use crate::object_document::ObjectDocument;
use crate::request_receipts::{DispatchErrorCode, OpacityRequest};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

fn cases() -> Value {
    serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap()
}
fn document() -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,
        "layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],
        "objects":cases()["records"]})
}
fn history(value: &Value, instance: &str) -> NativeObjectHistory {
    NativeObjectHistory::new(
        instance,
        decode_project(&serde_json::to_vec(value).unwrap()).unwrap(),
    )
    .unwrap()
}
fn payload() -> Value {
    cases()["command"]["request"]["payload"].clone()
}
fn prepare(owner: &NativeObjectHistory, value: &Value) -> PreparedObjectFill {
    owner
        .prepare_fill_json(&serde_json::to_vec(value).unwrap())
        .unwrap()
}
fn bytes(snapshot: &ObjectSnapshot) -> Vec<u8> {
    encode_project(snapshot.document()).unwrap()
}
fn current(owner: &NativeObjectHistory) -> ObjectSnapshot {
    owner.acquire_snapshot(owner.content_revision()).unwrap()
}
fn state(owner: &NativeObjectHistory) -> (u64, (usize, usize), Vec<u8>) {
    (
        owner.content_revision(),
        owner.history_depths(),
        bytes(&current(owner)),
    )
}
fn assert_content(snapshot: &ObjectSnapshot, expected: &Value) {
    let encoded = bytes(snapshot);
    assert_eq!(
        serde_json::from_slice::<Value>(&encoded).unwrap(),
        *expected
    );
    assert_eq!(decode_project(&encoded).unwrap(), *snapshot.document());
}
fn read(snapshot: &ObjectSnapshot, target: &Value) -> Value {
    let request = json!({"apiVersion":2,"requestId":"pinned-read","instanceId":snapshot.instance_id(),
        "documentId":snapshot.document_id(),"operation":"query.document.object",
        "payload":{"atRevision":snapshot.content_revision(),"stableTarget":target}});
    let raw = serde_json::to_value(
        snapshot
            .query_json(&serde_json::to_vec(&request).unwrap())
            .unwrap(),
    )
    .unwrap();
    let typed: OpacityRequest = serde_json::from_value(request).unwrap();
    let common = serde_json::to_value(snapshot.dispatch(typed).unwrap()).unwrap();
    assert_eq!(raw, common);
    assert_eq!(common["contentRevision"], snapshot.content_revision());
    assert_eq!(common["result"]["atRevision"], snapshot.content_revision());
    assert_eq!(
        common["result"]["documentSnapshotId"],
        snapshot.snapshot_id()
    );
    common["result"]["object"].clone()
}

#[test]
fn fixed_fill_undo_redo_publish_monotonic_revisions_and_keep_every_pinned_read() {
    let source = document();
    let fixed = cases();
    let oracle = &fixed["command"]["preservationOracle"];
    let index = oracle["beforeRecordIndex"].as_u64().unwrap() as usize;
    assert_eq!(source["objects"][index], fixed["records"][index]);
    let mut expected_after = source.clone();
    expected_after["objects"][index] = fixed["command"]["afterRecord"].clone();
    let mut owner = history(&source, "object-history");
    let zero = current(&owner);
    let candidate = prepare(&owner, &payload());
    let retry = candidate.clone();
    assert_eq!(owner.commit_fill(candidate), Ok(1));
    assert_eq!(owner.history_depths(), (1, 0));
    assert_eq!(
        owner.commit_fill(retry),
        Err(DispatchErrorCode::StaleRevision)
    );
    let one = current(&owner);
    for key in oracle["unchanged"].as_array().unwrap() {
        let key = key.as_str().unwrap();
        assert_eq!(
            expected_after["objects"][index][key],
            source["objects"][index][key]
        );
    }
    assert_eq!(owner.undo(1), Ok(2));
    assert_eq!(owner.history_depths(), (0, 1));
    let two = current(&owner);
    assert_eq!(owner.redo(2), Ok(3));
    assert_eq!(owner.history_depths(), (1, 0));
    let three = current(&owner);
    for (snapshot, expected) in [
        (&zero, &source),
        (&one, &expected_after),
        (&two, &source),
        (&three, &expected_after),
    ] {
        assert_content(snapshot, expected);
        assert_eq!(
            read(snapshot, &payload()["stableTarget"]),
            expected["objects"][index]
        );
        assert_eq!(
            bytes(&owner.acquire_snapshot(snapshot.content_revision()).unwrap()),
            bytes(snapshot)
        );
        assert_eq!(snapshot.document_id(), owner.document_id());
    }
    assert_ne!(zero.snapshot_id(), two.snapshot_id());
    assert_ne!(one.snapshot_id(), three.snapshot_id());
    assert!(owner.acquire_snapshot(4).is_none());
    assert!(owner.acquire_snapshot(9_007_199_254_740_992).is_none());
}

#[test]
fn scoped_equal_ids_multistep_history_restore_whole_content_and_branch_edit_truncates_redo() {
    let mut source = document();
    let mut later = source["objects"][0].clone();
    later["target"]["frameScope"]["frame"] = json!(8);
    let mut reference = source["objects"][0].clone();
    reference["target"]["frameScope"]["kind"] = json!("reference");
    let mut layer = source["objects"][0].clone();
    layer["target"]["layerUid"] = json!("0");
    source["objects"]
        .as_array_mut()
        .unwrap()
        .extend([later, reference, layer]);
    let mut owner = history(&source, "scoped-history");
    let mut expected = vec![source.clone()];
    for index in 0..5 {
        let mut request = payload();
        request["stableTarget"] = source["objects"][index]["target"].clone();
        assert_eq!(
            owner.commit_fill(prepare(&owner, &request)),
            Ok(index as u64 + 1)
        );
        let mut after = expected.last().unwrap().clone();
        after["objects"][index]["fill"] = request["fill"].clone();
        expected.push(after);
        assert_content(&current(&owner), expected.last().unwrap());
    }
    for index in (0..5).rev() {
        let revision = owner.content_revision();
        assert_eq!(owner.undo(revision), Ok(revision + 1));
        assert_content(&current(&owner), &expected[index]);
    }
    assert_eq!(owner.history_depths(), (0, 5));
    for after in expected.iter().skip(1) {
        let revision = owner.content_revision();
        assert_eq!(owner.redo(revision), Ok(revision + 1));
        assert_content(&current(&owner), after);
    }
    assert_eq!(owner.undo(15), Ok(16));
    let pinned = current(&owner);
    let mut branch = payload();
    branch["stableTarget"] = source["objects"][4]["target"].clone();
    branch["fill"]["r"] = json!(0.25);
    assert_eq!(owner.commit_fill(prepare(&owner, &branch)), Ok(17));
    assert_eq!(owner.history_depths(), (5, 0));
    let before = state(&owner);
    assert_eq!(owner.redo(17), Err(DispatchErrorCode::Unavailable));
    assert_eq!(state(&owner), before);
    let mut branched = expected[4].clone();
    branched["objects"][4]["fill"] = branch["fill"].clone();
    assert_content(&current(&owner), &branched);
    assert_content(&pinned, &expected[4]);
    assert_eq!(owner.undo(17), Ok(18));
    assert_content(&current(&owner), &expected[4]);
    assert_eq!(owner.redo(18), Ok(19));
    assert_content(&current(&owner), &branched);
}

#[test]
fn numeric_equivalent_noop_preserves_exact_representation_revision_and_redo() {
    let mut source = document();
    source["objects"][0]["fill"] = json!({"kind":"solid","r":1,"g":0.0,"b":0,"a":1.0});
    let mut owner = history(&source, "noop-history");
    let original = bytes(&current(&owner));
    let mut noop = payload();
    noop["fill"] = json!({"kind":"solid","r":1.0,"g":0,"b":0.0,"a":1});
    let stale_noop = prepare(&owner, &noop);
    assert_eq!(owner.commit_fill(prepare(&owner, &payload())), Ok(1));
    assert_eq!(owner.undo(1), Ok(2));
    let before = state(&owner);
    assert_eq!(before.2, original);
    assert_eq!(
        owner.commit_fill(stale_noop),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(state(&owner), before);
    let candidate = prepare(&owner, &noop);
    assert!(!candidate.changed());
    assert_eq!(owner.commit_fill(candidate), Ok(2));
    assert_eq!(state(&owner), before);
    assert_eq!(owner.redo(2), Ok(3));
    let mut expected = source;
    expected["objects"][0]["fill"] = payload()["fill"].clone();
    assert_content(&current(&owner), &expected);
}

#[test]
fn foreign_origin_stale_expectation_and_absent_history_fail_atomically() {
    let source = document();
    let mut owner = history(&source, "atomic-history");
    let foreign_instance = history(&source, "other-instance");
    let foreign_document = history(&source, "atomic-history");
    assert_ne!(owner.document_id(), foreign_document.document_id());
    let mut noop = payload();
    noop["fill"] = source["objects"][0]["fill"].clone();
    for request in [payload(), noop] {
        let before = state(&owner);
        assert_eq!(
            owner.commit_fill(prepare(&foreign_instance, &request)),
            Err(DispatchErrorCode::WrongInstance)
        );
        assert_eq!(
            owner.commit_fill(prepare(&foreign_document, &request)),
            Err(DispatchErrorCode::WrongDocument)
        );
        assert_eq!(state(&owner), before);
    }
    let before = state(&owner);
    assert_eq!(owner.undo(0), Err(DispatchErrorCode::Unavailable));
    assert_eq!(owner.redo(0), Err(DispatchErrorCode::Unavailable));
    assert_eq!(owner.undo(1), Err(DispatchErrorCode::StaleRevision));
    assert_eq!(
        owner.redo(9_007_199_254_740_992),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(state(&owner), before);
    let stale_snapshot = current(&owner);
    assert_eq!(owner.commit_fill(prepare(&owner, &payload())), Ok(1));
    let stale = prepare_object_fill_json(&stale_snapshot, &serde_json::to_vec(&payload()).unwrap())
        .unwrap();
    let before = state(&owner);
    assert_eq!(
        owner.commit_fill(stale),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(owner.undo(0), Err(DispatchErrorCode::StaleRevision));
    assert_eq!(owner.redo(0), Err(DispatchErrorCode::StaleRevision));
    assert_eq!(state(&owner), before);
    assert_eq!(owner.undo(1), Ok(2));
    let before = state(&owner);
    assert_eq!(owner.redo(1), Err(DispatchErrorCode::StaleRevision));
    assert_eq!(owner.undo(1), Err(DispatchErrorCode::StaleRevision));
    assert_eq!(state(&owner), before);
}

#[test]
fn strict_preparation_and_constructor_admission_fail_without_an_owner_mutation() {
    let source = document();
    let owner = history(&source, "invalid-history");
    let before = state(&owner);
    let raw = serde_json::to_string(&payload()).unwrap();
    let duplicated = raw.replacen("\"r\":0.8", "\"r\":0.8,\"r\":0.8", 1);
    assert_ne!(raw, duplicated);
    let wrong_shape =
        serde_json::to_vec(&json!({"command":"object.fill.set","stableTarget":[],"fill":[]}))
            .unwrap();
    for bytes in [
        b"[]".to_vec(),
        duplicated.into_bytes(),
        wrong_shape,
        b"{}".to_vec(),
    ] {
        assert_eq!(
            owner.prepare_fill_json(&bytes),
            Err(DispatchErrorCode::InvalidRequest)
        );
        assert_eq!(state(&owner), before);
    }
    let mut absent = payload();
    absent["stableTarget"]["strokeId"] = json!("absent");
    assert_eq!(
        owner.prepare_fill_json(&serde_json::to_vec(&absent).unwrap()),
        Err(DispatchErrorCode::NotFound)
    );
    assert_eq!(state(&owner), before);
    for (path, value) in [
        ("/totalFrames", json!(0)),
        ("/objects/0/fill/r", json!(1.1)),
        ("/objects/0/target/layerUid", json!("absent")),
    ] {
        let mut invalid = source.clone();
        *invalid.pointer_mut(path).unwrap() = value;
        let unchecked: ObjectDocument = serde_json::from_value(invalid).unwrap();
        assert!(NativeObjectHistory::new("invalid-new", unchecked).is_err());
    }
    assert!(NativeObjectHistory::new(
        "",
        decode_project(&serde_json::to_vec(&source).unwrap()).unwrap()
    )
    .is_err());
    assert_eq!(state(&owner), before);
}

#[test]
fn selected_revision_metadata_is_used_for_success_and_failure_envelopes() {
    let mut owner = history(&document(), "metadata-history");
    assert_eq!(owner.commit_fill(prepare(&owner, &payload())), Ok(1));
    let pinned = current(&owner);
    let before = bytes(&pinned);
    let mut request = OpacityRequest::query(
        "failure-read",
        owner.instance_id(),
        owner.document_id(),
        "query.document.object",
        json!({"atRevision":0,"stableTarget":payload()["stableTarget"]}),
    );
    let response = serde_json::to_value(pinned.dispatch(request.clone()).unwrap()).unwrap();
    assert_eq!(response["contentRevision"], 1);
    assert_eq!(response["error"]["code"], "not_found");
    request.document_id = "old-document".into();
    let response = serde_json::to_value(pinned.dispatch(request).unwrap()).unwrap();
    assert_eq!(response["contentRevision"], 1);
    assert_eq!(response["error"]["code"], "wrong_document");
    assert_eq!(bytes(&pinned), before);
    assert_eq!(
        read(&pinned, &payload()["stableTarget"]),
        cases()["command"]["afterRecord"]
    );
}
