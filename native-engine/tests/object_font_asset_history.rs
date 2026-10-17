//! PRIVATE source proposal. All cases UNRUN; host lifecycle and transport are separate gates.
use super::*;
use crate::object_document::{
    AssetInventory, AssetSource, ObjectDocument, ProjectEnvelope, ProjectMetadata,
};
use std::{cell::Cell, sync::Arc};

const FONT: &[u8] = include_bytes!("../../src/fonts/Roboto-Regular.ttf");
fn project() -> ProjectEnvelope {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let drawing: ObjectDocument = serde_json::from_value(json!({
        "format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,
        "layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],"objects":cases["records"]
    }))
    .unwrap();
    ProjectEnvelope::no_media(ProjectMetadata::new(320, 180, 29.97).unwrap(), drawing)
}
fn owner() -> NativeObjectHistory {
    NativeObjectHistory::from_admitted_project("font-core", project()).unwrap()
}
fn current(owner: &NativeObjectHistory) -> ObjectSnapshot {
    owner.acquire_snapshot(owner.content_revision()).unwrap()
}
fn import(owner: &NativeObjectHistory, id: &str) -> OpacityRequest {
    let mut request = OpacityRequest::command(
        id,
        owner.instance_id(),
        owner.document_id(),
        owner.content_revision(),
        json!({"command":"object.asset.font.import","documentSnapshotId":current(owner).snapshot_id(),
            "lifecycleGeneration":1,"source":{"kind":"bundled","fontKey":"Roboto-Regular"}}),
    );
    request.operation = IMPORT.into();
    request
}
fn query(
    owner: &NativeObjectHistory,
    payload: Value,
) -> Result<ResponseEnvelope, DispatchErrorCode> {
    owner.query_font_asset(&OpacityRequest::query(
        "font-read",
        owner.instance_id(),
        owner.document_id(),
        QUERY,
        payload,
    ))
}
fn result(response: ResponseEnvelope) -> Value {
    assert!(response.ok, "{:?}", response.error);
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    response.result.unwrap()
}
fn read(owner: &NativeObjectHistory, pin: &ObjectSnapshot, action: &str, asset: &str) -> Value {
    result(
        query(
            owner,
            json!({"action":action,"atRevision":pin.content_revision(),
        "documentSnapshotId":pin.snapshot_id(),"assetId":asset}),
        )
        .unwrap(),
    )
}
fn receipt(owner: &NativeObjectHistory, request: &str) -> Value {
    result(query(owner, json!({"action":"receipt","forRequestId":request})).unwrap())
}
fn font(pin: &ObjectSnapshot) -> Arc<AssetRecord> {
    Arc::clone(&pin.project().unwrap().asset_inventory().unwrap().records()[0])
}

#[test]
fn native_import_publishes_one_history_entry_and_exact_serializable_font() {
    let mut owner = owner();
    let old = current(&owner);
    let mut request = import(&owner, "import-one");
    request.payload["source"] =
        json!({"kind":"google","family":"  Roboto  ","weight":700,"italic":true});
    assert!(owner.font_import_status(&request).unwrap().is_none());
    let applied = result(owner.dispatch_font_import(&request, FONT).unwrap());
    assert_eq!(applied["historyEntriesAdded"], 1);
    assert_eq!(applied["encodedBytes"], FONT.len());
    assert_eq!(applied["encodedSha256"], hex(FONT));
    assert!(applied["assetId"]
        .as_str()
        .unwrap()
        .starts_with("native-font-"));
    let pin = current(&owner);
    assert_eq!(pin.content_revision(), 1);
    assert_eq!(owner.history_depths(), (1, 0));
    assert!(old.project().unwrap().asset_inventory().is_none());
    assert_eq!(pin.document(), old.document());
    assert_eq!(
        pin.project().unwrap().project(),
        old.project().unwrap().project()
    );
    let record = font(&pin);
    assert_eq!(record.name(), "  Roboto  ");
    assert_eq!(record.id().as_str(), applied["assetId"].as_str().unwrap());
    assert!(record.source().has_font_admission());
    assert_eq!(
        record.source().encoded_allocation().unwrap().as_slice(),
        FONT
    );
    let saved = owner.serialize_project_file(1, pin.snapshot_id()).unwrap();
    let reopened = NativeObjectHistory::from_project_file("font-reopen", &saved).unwrap();
    assert_ne!(reopened.document_id(), owner.document_id());
    assert_eq!(font(&current(&reopened)).as_ref(), record.as_ref());
    assert_eq!(
        reopened
            .serialize_project_file(0, current(&reopened).snapshot_id())
            .unwrap(),
        saved
    );
    assert_eq!(receipt(&reopened, "import-one")["status"], "not_found");
}

#[test]
fn replay_precedes_producer_and_preserves_original_result_after_undo() {
    let mut owner = owner();
    let request = import(&owner, "replay");
    let first = owner.dispatch_font_import(&request, FONT).unwrap();
    let first_json = serde_json::to_value(&first).unwrap();
    let pin = current(&owner);
    owner.undo(1).unwrap();
    assert!(current(&owner)
        .project()
        .unwrap()
        .asset_inventory()
        .is_none());
    let replay = owner.font_import_status(&request).unwrap().unwrap();
    assert_eq!(serde_json::to_value(replay).unwrap(), first_json);
    assert_eq!(
        serde_json::to_value(
            owner
                .dispatch_font_import(&request, b"producer must not be consulted")
                .unwrap()
        )
        .unwrap(),
        first_json
    );
    assert_eq!(owner.content_revision(), 2);
    assert_eq!(owner.history_depths(), (0, 1));
    assert_eq!(receipt(&owner, "replay")["status"], "committed");
    assert_eq!(
        receipt(&owner, "replay")["requestFingerprintSha256"],
        hex(&serde_json::to_vec(&request).unwrap())
    );
    owner.redo(2).unwrap();
    assert!(Arc::ptr_eq(&font(&pin), &font(&current(&owner))));
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let payload = serde_json::to_vec(&cases["command"]["request"]["payload"]).unwrap();
    owner
        .commit_fill(owner.prepare_fill_json(&payload).unwrap())
        .unwrap();
    assert!(Arc::ptr_eq(&font(&pin), &font(&current(&owner))));
}

#[test]
fn changed_body_never_reimports_or_changes_retained_success() {
    let mut owner = owner();
    let request = import(&owner, "same-id");
    owner.dispatch_font_import(&request, FONT).unwrap();
    let pin = current(&owner);
    let saved = pin.serialize_project_file().unwrap();
    let mut changed = request.clone();
    changed.payload["source"] =
        json!({"kind":"google","family":"Roboto","weight":700,"italic":false});
    assert!(matches!(
        owner.dispatch_font_import(&changed, FONT),
        Err(DispatchErrorCode::InvalidRequest)
    ));
    assert_eq!(current(&owner).serialize_project_file().unwrap(), saved);
    assert_eq!(owner.history_depths(), (1, 0));
    assert_eq!(
        receipt(&owner, "same-id")["assetId"],
        font(&pin).id().as_str()
    );
}

#[test]
fn invalid_font_retains_refusal_without_identity_history_or_redo_changes() {
    for bytes in [
        b"not a font".as_slice(),
        b"\x89PNG\r\n\x1a\n".as_slice(),
        b"\x00\x01\x00\x00".as_slice(),
    ] {
        let mut owner = owner();
        let request = import(&owner, "bad-bytes");
        let old = current(&owner);
        let response = owner.dispatch_font_import(&request, bytes).unwrap();
        assert!(!response.ok);
        assert_eq!(
            response.error.unwrap().code,
            if bytes.starts_with(b"\x00\x01\x00\x00") {
                DispatchErrorCode::InvalidRequest
            } else {
                DispatchErrorCode::Unavailable
            }
        );
        assert_eq!(owner.content_revision(), 0);
        assert_eq!(owner.history_depths(), (0, 0));
        assert_eq!(current(&owner).snapshot_id(), old.snapshot_id());
        assert!(old.project().unwrap().asset_inventory().is_none());
        assert_eq!(receipt(&owner, "bad-bytes")["status"], "refused");
        assert!(!owner.dispatch_font_import(&request, FONT).unwrap().ok);
    }
}

#[test]
fn stale_revision_snapshot_and_cancellation_stop_before_source_resolution() {
    for mode in ["revision", "snapshot", "cancel"] {
        let mut owner = owner();
        let mut request = import(&owner, mode);
        match mode {
            "revision" => request.expected_revision = Some(9),
            "snapshot" => request.payload["documentSnapshotId"] = "other-snapshot".into(),
            _ => request.cancelled_before_dispatch = true,
        }
        let refusal = owner.font_import_status(&request).unwrap().unwrap();
        assert!(!refusal.ok);
        let expected = if mode == "cancel" {
            DispatchErrorCode::CancelledBeforeDispatch
        } else {
            DispatchErrorCode::StaleRevision
        };
        assert_eq!(refusal.error.unwrap().code, expected);
        assert_eq!(owner.content_revision(), 0);
        assert_eq!(owner.history_depths(), (0, 0));
        assert_eq!(receipt(&owner, mode)["status"], "refused");
    }
}

#[test]
fn owner_change_during_host_io_is_refused_by_second_origin_check() {
    let mut owner = owner();
    let delayed = import(&owner, "delayed");
    assert!(owner.font_import_status(&delayed).unwrap().is_none());
    let other = import(&owner, "other");
    owner.dispatch_font_import(&other, FONT).unwrap();
    let pin = current(&owner);
    assert!(!owner.dispatch_font_import(&delayed, FONT).unwrap().ok);
    assert_eq!(owner.content_revision(), 1);
    assert_eq!(owner.history_depths(), (1, 0));
    assert!(Arc::ptr_eq(&font(&pin), &font(&current(&owner))));
}

#[test]
fn strict_descriptor_and_envelope_refusals_publish_no_receipt_or_asset() {
    let invalid = [
        json!({"kind":"bundled","fontKey":"unknown"}),
        json!({"kind":"bundled","fontKey":"Roboto-Regular","assetId":"caller"}),
        json!({"kind":"google","family":" ","weight":400,"italic":false}),
        json!({"kind":"google","family":"Roboto","weight":0,"italic":false}),
        json!({"kind":"google","family":"Roboto","weight":400,"italic":false,"url":"https://example.invalid"}),
    ];
    for source in invalid {
        let mut owner = owner();
        let mut request = import(&owner, "strict");
        request.payload["source"] = source;
        assert!(matches!(
            owner.font_import_status(&request),
            Err(DispatchErrorCode::InvalidRequest)
        ));
        assert_eq!(receipt(&owner, "strict")["status"], "not_found");
        assert_eq!(owner.history_depths(), (0, 0));
    }
    for mode in ["instance", "document", "generation"] {
        let mut owner = owner();
        let mut request = import(&owner, mode);
        match mode {
            "instance" => request.instance_id = "other".into(),
            "document" => request.document_id = "other".into(),
            _ => request.payload["lifecycleGeneration"] = 0.into(),
        }
        assert!(owner.font_import_status(&request).is_err());
        assert_eq!(owner.content_revision(), 0);
    }
}

#[test]
fn pinned_list_inspect_and_chunks_reassemble_original_bytes_after_head_changes() {
    let mut owner = owner();
    let request = import(&owner, "chunk-source");
    let id = result(owner.dispatch_font_import(&request, FONT).unwrap())["assetId"]
        .as_str()
        .unwrap()
        .to_owned();
    let pin = current(&owner);
    owner.undo(1).unwrap();
    let list = result(query(&owner, json!({"action":"list","atRevision":1,"documentSnapshotId":pin.snapshot_id(),"offset":0,"limit":1})).unwrap());
    assert_eq!(list["rows"][0]["assetId"], id);
    assert_eq!(list["totalFontRecords"], 1);
    assert!(list["nextOffset"].is_null());
    assert_eq!(
        read(&owner, &pin, "inspect", &id)["font"]["encodedSha256"],
        hex(FONT)
    );
    let mut assembled = Vec::new();
    while assembled.len() < FONT.len() {
        let value = result(
            query(
                &owner,
                json!({"action":"read","atRevision":1,"documentSnapshotId":pin.snapshot_id(),
            "assetId":id,"offset":assembled.len(),"length":1536}),
            )
            .unwrap(),
        );
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(value["base64"].as_str().unwrap())
            .unwrap();
        assert!(!bytes.is_empty() && bytes.len() <= 1536);
        assert_eq!(value["returnedLength"], bytes.len());
        assert_eq!(value["offset"], assembled.len());
        assert_eq!(value["totalLength"], FONT.len());
        assert_eq!(value["encodedSha256"], hex(FONT));
        assembled.extend(bytes);
    }
    assert_eq!(assembled, FONT);
    assert_eq!(owner.content_revision(), 2);
    assert_eq!(owner.history_depths(), (0, 1));
}

#[test]
fn reads_refuse_wrong_pins_ids_ranges_and_mutating_envelopes() {
    let mut owner = owner();
    let request = import(&owner, "ranges");
    let id = result(owner.dispatch_font_import(&request, FONT).unwrap())["assetId"]
        .as_str()
        .unwrap()
        .to_owned();
    let pin = current(&owner);
    for (offset, length) in [(0, 0), (0, 1537), (FONT.len(), 1), (usize::MAX, 1)] {
        assert!(query(
            &owner,
            json!({"action":"read","atRevision":1,"documentSnapshotId":pin.snapshot_id(),
            "assetId":id,"offset":offset,"length":length})
        )
        .is_err());
    }
    for (revision, snapshot, asset) in [
        (99, pin.snapshot_id(), id.as_str()),
        (1, "wrong", id.as_str()),
        (1, pin.snapshot_id(), "absent"),
    ] {
        assert!(query(&owner, json!({"action":"inspect","atRevision":revision,"documentSnapshotId":snapshot,"assetId":asset})).is_err());
    }
    let mut request = OpacityRequest::query(
        "mutating-read",
        owner.instance_id(),
        owner.document_id(),
        QUERY,
        json!({"action":"receipt","forRequestId":"ranges"}),
    );
    request.expected_revision = Some(1);
    assert!(owner.query_font_asset(&request).is_err());
    assert_eq!(owner.content_revision(), 1);
    assert_eq!(owner.history_depths(), (1, 0));
}

#[test]
fn linked_font_is_discoverable_metadata_only_and_never_resolved() {
    let id = "legacy font / with spaces";
    let record = Arc::new(
        AssetRecord::new(
            AssetId::new(id.into()).unwrap(),
            "  Legacy Font  ".into(),
            AssetKind::Font,
            AssetSource::linked("opaque://must-not-resolve".into(), 1024).unwrap(),
        )
        .unwrap(),
    );
    let project = project().with_asset_inventory(Some(
        AssetInventory::new(vec![record], vec![], 1024, 0).unwrap(),
    ));
    let owner = NativeObjectHistory::from_admitted_project("font-linked", project).unwrap();
    let pin = current(&owner);
    let value = read(&owner, &pin, "inspect", id);
    assert_eq!(value["font"]["name"], "  Legacy Font  ");
    assert_eq!(value["font"]["available"], false);
    assert_eq!(value["font"]["reason"], "linked_font_unavailable");
    assert!(query(&owner, json!({"action":"read","atRevision":0,"documentSnapshotId":pin.snapshot_id(),"assetId":id,"offset":0,"length":1})).is_err());
}

#[test]
fn receipt_miss_and_foreign_operation_disposition_stay_indeterminate() {
    let mut owner = owner();
    assert_eq!(receipt(&owner, "missing")["status"], "not_found");
    let request = import(&owner, "foreign");
    owner.receipts.retain(
        "foreign".into(),
        RequestFingerprint::new(&request).unwrap(),
        RetainedDisposition::Exact {
            content_revision: 0,
            outcome: Err(CompactFailure {
                code: DispatchErrorCode::Unavailable,
                message: "other operation".into(),
                details: None,
            }),
        },
    );
    assert_eq!(receipt(&owner, "foreign")["status"], "not_found");
    assert_eq!(owner.history_depths(), (0, 0));
}

#[test]
fn malformed_and_quota_failures_precede_allocation_and_preserve_old_pins() {
    let owner = owner();
    let pin = current(&owner);
    let saved = pin.serialize_project_file().unwrap();
    let calls = Cell::new(0);
    let malformed = pin.admit_font_asset_using::<()>(
        "bad".into(),
        "Bad".into(),
        b"not-font",
        |_| {
            calls.set(calls.get() + 1);
            Ok(())
        },
        |_| panic!("no copy"),
        |_, _| panic!("no bind"),
    );
    assert!(malformed.is_err());
    assert_eq!(calls.get(), 0);
    let budget = ObjectSnapshot::encoded_test_budget(FONT.len() - 1);
    let full = pin.admit_font_asset_using(
        "full".into(),
        "Full".into(),
        FONT,
        |n| budget.reserve(n).map_err(|_| "reserve"),
        |_| panic!("no copy"),
        |_, _| panic!("no bind"),
    );
    assert!(full.is_err());
    assert_eq!(budget.used(), Ok(0));
    assert_eq!(pin.serialize_project_file().unwrap(), saved);
}

#[test]
fn allocator_bind_and_unwind_failures_release_original_capacity() {
    let owner = owner();
    let pin = current(&owner);
    for stage in ["allocate", "capacity", "bind", "unwind"] {
        let budget = ObjectSnapshot::encoded_test_budget(FONT.len());
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pin.admit_font_asset_using(
                "failed".into(),
                "Failed".into(),
                FONT,
                |n| budget.reserve(n).map_err(|_| "reserve"),
                |p| {
                    if stage == "allocate" {
                        return Err("allocate");
                    }
                    if stage == "unwind" {
                        panic!("injected");
                    }
                    let mut bytes = p.allocate().map_err(|_| "allocate")?;
                    if stage == "capacity" {
                        bytes.pop();
                    }
                    Ok(bytes)
                },
                |_, _| {
                    assert_eq!(stage, "bind");
                    Err("bind")
                },
            )
        }));
        assert!(outcome.is_err() || outcome.unwrap().is_err());
        assert_eq!(budget.used(), Ok(0));
        assert!(pin.project().unwrap().asset_inventory().is_none());
    }
}

#[test]
fn actual_admitted_opaque_id_requires_positive_short_read_within_wire_ceiling() {
    let owner = owner();
    let pin = current(&owner);
    let id = format!("x{}", "\u{1}".repeat(254));
    let record = pin
        .admit_font_asset(id.clone(), "Original".into(), FONT)
        .unwrap();
    let original = Arc::clone(record.source().encoded_allocation().unwrap());
    let project = project().with_asset_inventory(Some(
        AssetInventory::new(vec![record], vec![], 1024, 0).unwrap(),
    ));
    let owner = NativeObjectHistory::from_admitted_project("i".repeat(128), project).unwrap();
    let pin = current(&owner);
    assert!(Arc::ptr_eq(
        &original,
        font(&pin).source().encoded_allocation().unwrap()
    ));
    let request = OpacityRequest::query(
        "q".repeat(128),
        owner.instance_id(),
        owner.document_id(),
        QUERY,
        json!({"action":"read","atRevision":0,"documentSnapshotId":pin.snapshot_id(),"assetId":id,"offset":0,"length":1536}),
    );
    let value = result(owner.query_font_asset(&request).unwrap());
    let count = value["returnedLength"].as_u64().unwrap() as usize;
    assert!(count > 0 && count < 1536);
    assert_eq!(value["assetId"], id);
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(value["base64"].as_str().unwrap())
            .unwrap(),
        FONT[..count]
    );
}
