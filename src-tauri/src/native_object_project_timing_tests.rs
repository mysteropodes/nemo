//! Explicit P38 test-author timing, immutable reads and native file roundtrips.
//! This does not exercise C2050 held paint or the public timing API.
use crate::application_mcp::ApplicationMcp;
use crate::native_dispatch::NativeObjectHost;
use crate::native_object_bootstrap::project_tests::{p38_project_bytes, request, Scratch};
use crate::native_object_bootstrap::{reopen::bootstrap_request, save::save_request};
use native_engine::{object_codec::decode_storage_project, object_snapshot::ObjectSnapshot};
use serde_json::{json, Value};
use std::{fs, path::Path};

// Historical pre-timing encoder digests: do not replace with new runtime pins.
const CASES: [(u32, u32, u32, bool, &str); 2] = [
    (
        320,
        180,
        24,
        false,
        "e1bc63b0e284f521e170d8236296392bdcf19d4b1f13b4e04683727844fa71a0",
    ),
    (
        640,
        360,
        30,
        true,
        "a438bda3b5c25ec3e8797d1c95fb8ade1da7b0bda675c2279499c659ece46bf6",
    ),
];

/// The caller explicitly chooses keys; never infer them from records or scopes.
pub(super) fn fixture_timing(keys: &[u32]) -> Value {
    let keys: Vec<_> = keys.iter().map(|frame| json!({"frame":frame})).collect();
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":keys,"samples":[]})
}

fn snapshot(bytes: &[u8]) -> ObjectSnapshot {
    let project: Value = serde_json::from_slice(bytes).unwrap();
    let drawing = serde_json::to_vec(&project["drawing"]).unwrap();
    ObjectSnapshot::new(
        "p38-timing-reader",
        decode_storage_project(&drawing).unwrap(),
    )
    .unwrap()
}

fn selector(snapshot: &ObjectSnapshot, layer: &str, frame: u32) -> Value {
    json!({"atRevision":snapshot.content_revision(),"documentSnapshotId":snapshot.snapshot_id(),
        "contextId":"scene-root","layerUid":layer,"frame":frame})
}

fn read(snapshot: &ObjectSnapshot, layer: &str, frame: u32) -> Value {
    serde_json::to_value(
        snapshot
            .query_layer_timing_json(
                &serde_json::to_vec(&selector(snapshot, layer, frame)).unwrap(),
            )
            .unwrap(),
    )
    .unwrap()
}

fn expected(
    snapshot: &ObjectSnapshot,
    layer: &str,
    frame: u32,
    kind: &str,
    source: Option<u32>,
    content: bool,
) -> Value {
    json!({"instanceId":snapshot.instance_id(),"documentId":snapshot.document_id(),
        "atRevision":snapshot.content_revision(),"documentSnapshotId":snapshot.snapshot_id(),
        "contextId":"scene-root","layerUid":layer,"frame":frame,"cellKind":kind,
        "sourceFrame":source,"hasContent":content,"manualEdit":false})
}

fn assert_rows(snapshot: &ObjectSnapshot) {
    // Frozen author oracle: do not ask the implementation to generate expectations.
    for (frame, kind, source) in [
        (0, "key", 0),
        (6, "held", 0),
        (7, "key", 7),
        (12, "held", 7),
        (20, "held", 7),
    ] {
        assert_eq!(
            read(snapshot, "legacy layer:alpha", frame),
            expected(
                snapshot,
                "legacy layer:alpha",
                frame,
                kind,
                Some(source),
                true
            )
        );
        assert_eq!(
            read(snapshot, "0", frame),
            expected(snapshot, "0", frame, "unexposed", None, false)
        );
    }
}

fn refusal(snapshot: &ObjectSnapshot, request: Value, code: &str) {
    let error = snapshot
        .query_layer_timing_json(&serde_json::to_vec(&request).unwrap())
        .unwrap_err();
    assert_eq!(format!("{error:?}"), code);
}

fn open_file(state: &ApplicationMcp, path: &Path) -> Value {
    let raw = json!({"apiVersion":2,"requestId":"timing-open","instanceId":state.instance_id(),
        "sourcePath":path,"projectOnly":true});
    serde_json::to_value(
        bootstrap_request(state, &raw.to_string(), |candidate| candidate == path).unwrap(),
    )
    .unwrap()
}

#[test]
fn explicit_fixture_timing_reads_fixed_sources_without_changing_any_prior_project_value() {
    for (width, height, fps, variant, old_digest) in CASES {
        let bytes = p38_project_bytes(width, height, fps, variant);
        let unchanged = bytes.clone();
        let view = snapshot(&bytes);
        assert_rows(&view);
        assert_eq!(bytes, unchanged);

        let mut previous: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(previous["drawing"]["objects"].as_array().unwrap().len(), 3);
        for layer in previous["drawing"]["layers"].as_array_mut().unwrap() {
            assert!(layer.as_object_mut().unwrap().remove("timing").is_some());
        }
        // Hash the entire normalized old document through its actual native file
        // owner: this pins every old field, including the full Reference12 record.
        let baseline = NativeObjectHost::from_project_file(
            "p38-before-timing".into(),
            &serde_json::to_vec(&previous).unwrap(),
        )
        .unwrap();
        assert_eq!(
            baseline.project_file_info().unwrap()["documentDigest"],
            old_digest
        );
    }
}

#[test]
fn missing_timing_bad_selectors_and_reference_records_never_fabricate_authored_content() {
    for (width, height, fps, variant, _) in CASES {
        let bytes = p38_project_bytes(width, height, fps, variant);
        let view = snapshot(&bytes);
        let original = selector(&view, "legacy layer:alpha", 0);
        for (field, value, code) in [
            ("atRevision", json!(1), "StaleRevision"),
            (
                "documentSnapshotId",
                json!("wrong-snapshot"),
                "StaleRevision",
            ),
            ("contextId", json!("other-context"), "Unavailable"),
            ("layerUid", json!("missing-layer"), "NotFound"),
            ("frame", json!(21), "InvalidRequest"),
            ("scopeKind", json!("reference"), "InvalidRequest"),
        ] {
            let mut bad = original.clone();
            bad[field] = value;
            refusal(&view, bad, code);
        }
        for (index, uid) in [(0, "legacy layer:alpha"), (1, "0")] {
            for hidden in [false, true] {
                let mut missing: Value = serde_json::from_slice(&bytes).unwrap();
                let layer = missing["drawing"]["layers"][index].as_object_mut().unwrap();
                layer.remove("timing");
                if hidden {
                    layer.insert("visible".into(), json!(false));
                }
                let missing_view = snapshot(&serde_json::to_vec(&missing).unwrap());
                refusal(
                    &missing_view,
                    selector(&missing_view, uid, 12),
                    "Unavailable",
                );
            }
        }
        let mut reference_only: Value = serde_json::from_slice(&bytes).unwrap();
        let preserved = reference_only["drawing"]["objects"].clone();
        // Controlled negative only, not fixture intent: an explicit key at12
        // still has NO authored content supplied by the stored Reference12.
        reference_only["drawing"]["layers"][1]["timing"]["keys"] = json!([{"frame":12}]);
        let reference_view = snapshot(&serde_json::to_vec(&reference_only).unwrap());
        assert_eq!(
            read(&reference_view, "0", 12),
            expected(&reference_view, "0", 12, "key", Some(12), false)
        );
        assert_eq!(reference_only["drawing"]["objects"], preserved);
    }
}

#[test]
fn malformed_timing_is_refused_by_the_existing_native_storage_codec() {
    let mut overlap = fixture_timing(&[0]);
    overlap["samples"] = json!([{"frame":0}]);
    let mut unsupported = fixture_timing(&[]);
    unsupported["family"] = json!("component");
    for timing in [
        json!(null),
        fixture_timing(&[7, 0]),
        fixture_timing(&[0, 0]),
        overlap,
        unsupported,
    ] {
        let mut project: Value =
            serde_json::from_slice(&p38_project_bytes(320, 180, 24, false)).unwrap();
        project["drawing"]["layers"][0]["timing"] = timing;
        assert!(decode_storage_project(&serde_json::to_vec(&project["drawing"]).unwrap()).is_err());
    }
}

#[test]
fn native_pinned_save_disk_reopen_preserves_timing_reference_and_immutable_reader_pins() {
    for (width, height, fps, variant, _) in CASES {
        let bytes = p38_project_bytes(width, height, fps, variant);
        let before: Value = serde_json::from_slice(&bytes).unwrap();
        let retained = snapshot(&bytes);
        let old_selector = selector(&retained, "legacy layer:alpha", 7);
        let scratch = Scratch::new();
        let source = scratch.0.join("source.json");
        let destination = scratch.0.join("saved.json");
        fs::write(&source, &bytes).unwrap();
        let state = ApplicationMcp::default();
        let opened = open_file(&state, &source);
        let document = opened["documentId"].as_str().unwrap();
        let info = state.native_project_info(document).unwrap();
        let save = request(&state, document, &destination);
        let receipt = serde_json::to_value(
            save_request(
                &state,
                &save.to_string(),
                |path| path.parent() == Some(scratch.0.as_path()),
                |_, _| Ok(()),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(receipt["published"], true);
        assert_eq!(receipt["cleanupPending"], false);
        let disk = fs::read(&destination).unwrap();
        assert_eq!(
            disk,
            state
                .native_project_file_snapshot(
                    document,
                    0,
                    info["documentSnapshotId"].as_str().unwrap()
                )
                .unwrap()
                .unwrap()
        );
        assert_eq!(serde_json::from_slice::<Value>(&disk).unwrap(), before);
        let reopened = ApplicationMcp::default();
        let next = open_file(&reopened, &destination);
        let next_document = next["documentId"].as_str().unwrap();
        assert_ne!(next_document, document);
        let next_info = reopened.native_project_info(next_document).unwrap();
        assert_ne!(next_info["documentSnapshotId"], info["documentSnapshotId"]);
        for field in ["canvasW", "canvasH", "fps", "totalFrames", "documentDigest"] {
            assert_eq!(next_info[field], info[field]);
        }
        let restored = snapshot(&disk);
        assert_rows(&restored);
        assert_rows(&retained);
        refusal(&restored, old_selector, "StaleRevision");
        assert_eq!(bytes, p38_project_bytes(width, height, fps, variant));
    }
}
