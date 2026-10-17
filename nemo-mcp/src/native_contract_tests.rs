use super::*;
use serde_json::json;

#[test]
fn diagnostics_result_rejects_open_shapes_bad_dispositions_and_unordered_sequences() {
    let record = json!({"sequence":1,"requestId":"edit-1","operation":"history.undo","contentRevision":2,"ok":true});
    let valid = json!({"records":[record.clone()],"truncated":false});
    assert!(diagnostics_result(&valid));
    let mut redacted = valid.clone();
    redacted["records"][0]
        .as_object_mut()
        .unwrap()
        .remove("requestId");
    assert!(!diagnostics_result(&redacted));
    redacted["records"][0]["requestIdRedacted"] = json!(true);
    assert!(diagnostics_result(&redacted));
    redacted["records"][0]["requestIdRedacted"] = json!(false);
    assert!(!diagnostics_result(&redacted));
    let mut bad = Vec::new();
    for (key, value) in [
        ("payload", json!({"private":"secret"})),
        ("sequence", json!(0)),
        ("ok", json!(false)),
        ("errorCode", json!("internal")),
        ("operation", json!("query.diagnostics.recent")),
        ("targetId", json!("/private/path")),
        ("targetId", json!("file:/private/path")),
        ("requestId", json!("file:/private/path")),
        ("requestIdRedacted", json!(true)),
    ] {
        let mut altered = valid.clone();
        altered["records"][0][key] = value;
        bad.push(altered);
    }
    bad.push(json!({"records":[record.clone(), record.clone()],"truncated":false}));
    let mut gap = record.clone();
    gap["sequence"] = json!(3);
    bad.push(json!({"records":[record.clone(), gap.clone()],"truncated":true}));
    bad.push(json!({"records":[gap],"truncated":false}));
    bad.push(json!({"records":vec![record;33],"truncated":true}));
    bad.push(json!({"records":[],"truncated":false,"privatePath":"secret"}));
    for value in bad {
        assert!(!diagnostics_result(&value));
    }
    assert!(!validate_result(
        "query.diagnostics.recent",
        "doc",
        &json!({"limit":32}),
        &valid
    ));
}

fn serialized(document: Value) -> Value {
    json!({
        "atRevision": 0,
        "documentSnapshotId": "native-opacity:native-document-1:0",
        "document": document
    })
}

fn document() -> Value {
    json!({
        "format": "nemo.native-opacity-document", "formatVersion": 1, "totalFrames": 21,
        "layers": [{"layerUid": "layer-a", "motionStatic": {"opacity": [25]}}]
    })
}

fn decimal_keyed_document() -> Value {
    json!({
        "format": "nemo.native-opacity-document", "formatVersion": 1, "totalFrames": 21,
        "layers": [{"layerUid": "layer-a", "motion": {"opacity": {"keys": [{
            "frame": 0, "v": [25], "curvePoints": [
                {"x": 0.0, "y": 0.0}, {"x": 0.25, "y": 0.156},
                {"x": 0.5, "y": 0.5}, {"x": 0.75, "y": 0.844},
                {"x": 1.0, "y": 1.0}
            ], "hOut": [0.0, 0.0], "hIn": [0.0, 0.0]
        }]}}}]
    })
}

#[test]
fn serialized_document_contract_accepts_semantic_numbers_and_rejects_bad_shapes() {
    let payload = json!({"atRevision": 0});
    let valid = document();
    assert!(validate_result(
        "query.document.serialize",
        "native-document-1",
        &payload,
        &serialized(valid.clone())
    ));
    assert!(validate_result(
        "query.document.serialize",
        "native-document-1",
        &payload,
        &serialized(decimal_keyed_document()),
    ));
    let mut open = valid.clone();
    open["legacyVersion"] = json!(13);
    let mut duplicate = valid.clone();
    let duplicate_layer = duplicate["layers"][0].clone();
    duplicate["layers"]
        .as_array_mut()
        .unwrap()
        .push(duplicate_layer);
    let mut malformed_track = valid;
    malformed_track["layers"][0] = json!({
        "layerUid": "layer-a", "motion": {"opacity": {"keys": [{
            "frame": 0, "v": [25], "curvePoints": [], "hOut": [0, 0], "hIn": [0, 0]
        }]}}
    });
    for rejected in [
        json!({}),
        json!({"version": 13, "layers": []}),
        open,
        duplicate,
        malformed_track,
    ] {
        assert!(!validate_result(
            "query.document.serialize",
            "native-document-1",
            &payload,
            &serialized(rejected)
        ));
    }
}

#[test]
fn common_result_dispatch_validates_font_import_receipt_shape() {
    let operation = "command.document.object.asset.font.import";
    let payload = json!({"command":"object.asset.font.import","documentSnapshotId":"snapshot-0",
        "lifecycleGeneration":1,"source":{"kind":"bundled","fontKey":"Roboto-Regular"}});
    let valid = json!({"applied":true,"historyEntriesAdded":1,"assetId":" font / 雪 ",
        "mediaType":"font/ttf","encodedBytes":3,"encodedSha256":"a".repeat(64)});
    assert!(validate_result(
        operation,
        "font-document",
        &payload,
        &valid
    ));
    for (key, value) in [
        ("applied", json!(false)),
        ("historyEntriesAdded", json!(2)),
        ("encodedSha256", json!("A".repeat(64))),
        ("base64", json!("QUJD")),
    ] {
        let mut bad = valid.clone();
        bad[key] = value;
        assert!(
            !validate_result(operation, "font-document", &payload, &bad),
            "{key}"
        );
    }
}

#[test]
fn common_result_dispatch_preserves_opaque_linked_font_identity_without_locator() {
    let operation = "query.document.object.asset.font";
    let payload = json!({"action":"inspect","atRevision":1,
        "documentSnapshotId":"snapshot-1","assetId":" font / 雪 "});
    let valid = json!({"atRevision":1,"documentSnapshotId":"snapshot-1","font":{
        "assetId":" font / 雪 ","name":" Exact font ","sourceKind":"linked",
        "available":false,"reason":"linked_font_unavailable"}});
    assert!(validate_result(
        operation,
        "font-document",
        &payload,
        &valid
    ));
    let mut wrong = valid.clone();
    wrong["font"]["assetId"] = json!("font / 雪");
    assert!(!validate_result(
        operation,
        "font-document",
        &payload,
        &wrong
    ));
    let mut leaked = valid;
    leaked["font"]["locator"] = json!("opaque://must-not-resolve");
    assert!(!validate_result(
        operation,
        "font-document",
        &payload,
        &leaked
    ));
}

#[test]
fn numeric_selectors_preserve_zero_and_refuse_missing_or_unsafe_values() {
    for value in [
        None,
        Some(json!(null)),
        Some(json!(true)),
        Some(json!("0")),
        Some(json!(-1)),
        Some(json!(0.5)),
    ] {
        assert_eq!(revision(value.as_ref()), None);
        assert_eq!(frame(value.as_ref()), None);
    }
    for (value, expected_revision, expected_frame) in [
        (0_u64, Some(0), Some(0)),
        (4_294_967_295, Some(4_294_967_295), Some(4_294_967_295)),
        (4_294_967_296, Some(4_294_967_296), None),
        (9_007_199_254_740_991, Some(9_007_199_254_740_991), None),
        (9_007_199_254_740_992, None, None),
        (u64::MAX, None, None),
    ] {
        let encoded = json!(value);
        assert_eq!(
            revision(Some(&encoded)),
            expected_revision,
            "revision {value}"
        );
        assert_eq!(frame(Some(&encoded)), expected_frame, "frame {value}");
    }
}

#[test]
fn common_result_dispatch_routes_each_batch_eight_family_to_its_own_receipt_validator() {
    let catalog = crate::capabilities::native_catalog();
    for operation in [
        "command.document.object.frames.insert",
        "command.document.object.component.convert",
        "command.document.object.rig.edit",
        "command.document.object.vector.edit",
        "command.document.object.import.batch",
    ] {
        let example = catalog
            .capability_for_operation(operation)
            .expect("declared")["examples"]
            .as_array()
            .unwrap()
            .iter()
            .find(|example| example["request"]["operation"] == operation)
            .expect("registered example");
        let document_id = example["request"]["documentId"].as_str().unwrap();
        let payload = &example["request"]["payload"];
        let valid = &example["response"]["result"];
        assert!(
            validate_result(operation, document_id, payload, valid),
            "{operation}"
        );
        let mut open = valid.clone();
        open["privatePath"] = json!("/private/path");
        assert!(
            !validate_result(operation, document_id, payload, &open),
            "{operation} open"
        );
        assert!(
            !validate_result(operation, document_id, payload, &json!({})),
            "{operation} empty"
        );
    }
}
