//! Request replay, origin pins, envelope validation and cancellation of the asset
//! commands: the shared receipt store returns the stored disposition, a changed
//! body is refused, and nothing that fails before the sealed commit leaves a
//! revision, a history entry or a partial record behind.
use crate::history::NativeObjectHistory;
use crate::request_receipts::DispatchErrorCode;
use serde_json::json;

include!("object_raster_asset_attach_support.rs");

#[test]
fn an_exact_retry_returns_the_stored_response_after_undo_and_never_executes_twice() {
    let mut owner = open_history();
    let request = attach_request(&owner, "once", "blue.png", "image/png");
    let png = oracle_bytes("blue-2x1");
    let first = owner.dispatch_asset_command(&request, Some(&png)).unwrap();
    assert_eq!(first.content_revision(), 1);
    assert_eq!(owner.undo(1), Ok(2));
    assert!(records_of(&owner).is_empty());
    let replay = owner.dispatch_asset_command(&request, Some(&png)).unwrap();
    assert_eq!(replay, first);
    assert_eq!(owner.content_revision(), 2);
    assert_eq!(owner.history_depths(), (0, 1));
    assert!(records_of(&owner).is_empty());
    attach_png(&mut owner, "later", "red.png", "red-2x1");
    assert_eq!(
        owner.dispatch_asset_command(&request, Some(&png)).unwrap(),
        first
    );
    assert_eq!(names_of(&owner), ["red.png"]);
}

#[test]
fn the_same_request_id_with_other_bytes_name_or_type_is_refused() {
    let mut owner = open_history();
    let request = attach_request(&owner, "once", "blue.png", "image/png");
    let png = oracle_bytes("blue-2x1");
    owner.dispatch_asset_command(&request, Some(&png)).unwrap();
    assert_eq!(
        owner.dispatch_asset_command(&request, Some(&oracle_bytes("red-2x1"))),
        Err(DispatchErrorCode::InvalidRequest)
    );
    let mut renamed = request.clone();
    renamed.payload["name"] = "other.png".into();
    assert_eq!(
        owner.dispatch_asset_command(&renamed, Some(&png)),
        Err(DispatchErrorCode::InvalidRequest)
    );
    let mut retyped = request.clone();
    retyped.payload["mediaType"] = "image/jpeg".into();
    assert_eq!(
        owner.dispatch_asset_command(&retyped, Some(&png)),
        Err(DispatchErrorCode::InvalidRequest)
    );
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (1, (1, 0))
    );
    assert_eq!(names_of(&owner), ["blue.png"]);
}

#[test]
fn refusals_are_retained_and_replayed_whether_cancelled_corrupt_or_stale() {
    let mut owner = open_history();
    let png = oracle_bytes("blue-2x1");
    let corrupt_bytes = oracle_bytes("signature-then-zeros");
    let cancelled = attach_request(&owner, "cancel", "blue.png", "image/png").cancelled();
    let corrupt = attach_request(&owner, "corrupt", "x.png", "image/png");
    let stale = remove_request(&owner, "stale", "asset-r1");
    let first_cancel = owner
        .dispatch_asset_command(&cancelled, Some(&png))
        .unwrap();
    assert_eq!(
        first_cancel.error().unwrap().code(),
        DispatchErrorCode::CancelledBeforeDispatch
    );
    let first_corrupt = owner
        .dispatch_asset_command(&corrupt, Some(&corrupt_bytes))
        .unwrap();
    assert_eq!(
        refusal_of(&first_corrupt).0,
        DispatchErrorCode::InvalidRequest
    );
    attach_png(&mut owner, "good", "blue.png", "blue-2x1");
    let first_stale = owner.dispatch_asset_command(&stale, None).unwrap();
    assert_eq!(
        first_stale.error().unwrap().code(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(
        owner
            .dispatch_asset_command(&cancelled, Some(&png))
            .unwrap(),
        first_cancel
    );
    assert_eq!(
        owner
            .dispatch_asset_command(&corrupt, Some(&corrupt_bytes))
            .unwrap(),
        first_corrupt
    );
    assert_eq!(
        owner.dispatch_asset_command(&stale, None).unwrap(),
        first_stale
    );
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (1, (1, 0))
    );
    assert_eq!(names_of(&owner), ["blue.png"]);
}

#[test]
fn a_stale_expected_revision_or_snapshot_pin_is_refused_without_publication() {
    let mut owner = open_history();
    let old_pin = current_snapshot_id(&owner);
    attach_png(&mut owner, "r1", "blue.png", "blue-2x1");
    let red = oracle_bytes("red-2x1");
    let pinned = asset_request(
        &owner,
        "pin",
        "attach",
        json!({"documentSnapshotId":old_pin,"name":"red.png","mediaType":"image/png"}),
    );
    let response = owner.dispatch_asset_command(&pinned, Some(&red)).unwrap();
    assert_eq!(
        response.error().unwrap().code(),
        DispatchErrorCode::StaleRevision
    );
    for (id, revision) in [("late", 0), ("early", 9)] {
        let mut request = attach_request(&owner, id, "red.png", "image/png");
        request.expected_revision = Some(revision);
        let response = owner.dispatch_asset_command(&request, Some(&red)).unwrap();
        assert_eq!(
            response.error().unwrap().code(),
            DispatchErrorCode::StaleRevision,
            "{id}"
        );
    }
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (1, (1, 0))
    );
    assert_eq!(names_of(&owner), ["blue.png"]);
}

#[test]
fn a_malformed_snapshot_pin_is_a_retained_invalid_request_for_attach_and_remove() {
    let mut owner = open_history();
    attach_png(&mut owner, "r1", "blue.png", "blue-2x1");
    let (revision, depths, saved) = (
        owner.content_revision(),
        owner.history_depths(),
        saved_bytes(&owner),
    );
    let long = "a".repeat(129);
    let pins = [
        "",
        "has space",
        "-leading",
        "\u{fc}ber",
        "a;b",
        long.as_str(),
    ];
    for (index, pin) in pins.iter().enumerate() {
        let attach = asset_request(
            &owner,
            &format!("bad-attach-{index}"),
            "attach",
            json!({"documentSnapshotId":pin,"name":"red.png","mediaType":"image/png"}),
        );
        let response = owner
            .dispatch_asset_command(&attach, Some(&oracle_bytes("red-2x1")))
            .unwrap();
        assert_eq!(
            response.error().unwrap().code(),
            DispatchErrorCode::InvalidRequest,
            "attach pin {pin:?}"
        );
        let remove = asset_request(
            &owner,
            &format!("bad-remove-{index}"),
            "remove",
            json!({"documentSnapshotId":pin,"assetId":"asset-r1"}),
        );
        let response = owner.dispatch_asset_command(&remove, None).unwrap();
        assert_eq!(
            response.error().unwrap().code(),
            DispatchErrorCode::InvalidRequest,
            "remove pin {pin:?}"
        );
        // Retained like every other refusal: the exact retry returns the stored one.
        assert_eq!(
            owner
                .dispatch_asset_command(&remove, None)
                .unwrap()
                .error()
                .unwrap()
                .code(),
            DispatchErrorCode::InvalidRequest
        );
    }
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (revision, depths)
    );
    assert_eq!(saved_bytes(&owner), saved);
    assert_eq!(names_of(&owner), ["blue.png"]);
}

#[test]
fn another_instance_or_another_document_never_reaches_the_owner() {
    let mut owner = open_history();
    let png = oracle_bytes("blue-2x1");
    let other_document = open_history();
    let request = attach_request(&other_document, "x1", "blue.png", "image/png");
    assert_eq!(
        owner.dispatch_asset_command(&request, Some(&png)),
        Err(DispatchErrorCode::WrongDocument)
    );
    let stranger =
        NativeObjectHistory::from_project_file("elsewhere", &blank_project_file()).unwrap();
    let request = attach_request(&stranger, "x2", "blue.png", "image/png");
    assert_eq!(
        owner.dispatch_asset_command(&request, Some(&png)),
        Err(DispatchErrorCode::WrongInstance)
    );
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (0, (0, 0))
    );
    assert!(records_of(&owner).is_empty());
}

#[test]
fn malformed_envelopes_fail_closed_and_leave_no_receipt() {
    let mut owner = open_history();
    let png = oracle_bytes("blue-2x1");
    let good = attach_request(&owner, "env", "blue.png", "image/png");
    assert_eq!(
        owner.dispatch_asset_command(&good, None),
        Err(DispatchErrorCode::InvalidRequest)
    );
    let remove = remove_request(&owner, "env-rm", "asset-x");
    assert_eq!(
        owner.dispatch_asset_command(&remove, Some(&png)),
        Err(DispatchErrorCode::InvalidRequest)
    );
    let mut no_revision = good.clone();
    no_revision.expected_revision = None;
    let mut old_api = good.clone();
    old_api.api_version = 1;
    let mut unknown = good.clone();
    unknown.operation = "command.document.object.asset.rename".into();
    let mut bad_id = good.clone();
    bad_id.request_id = "has space".into();
    let mut array_payload = good.clone();
    array_payload.payload = json!([1]);
    for (label, request) in [
        ("expectedRevision", no_revision),
        ("apiVersion", old_api),
        ("operation", unknown),
        ("requestId", bad_id),
        ("payload", array_payload),
    ] {
        assert_eq!(
            owner.dispatch_asset_command(&request, Some(&png)),
            Err(DispatchErrorCode::InvalidRequest),
            "{label}"
        );
    }
    let huge = vec![0u8; 16 * 1024 * 1024 + 1];
    assert_eq!(
        owner.dispatch_asset_command(&good, Some(&huge)),
        Err(DispatchErrorCode::InvalidRequest)
    );
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (0, (0, 0))
    );
    // No attempt above retained a receipt: the same request id now commits.
    let response = owner.dispatch_asset_command(&good, Some(&png)).unwrap();
    assert!(response.is_ok());
    assert_eq!(owner.content_revision(), 1);
}

#[test]
fn an_unknown_payload_member_is_a_retained_invalid_request_response() {
    let mut owner = open_history();
    let pin = current_snapshot_id(&owner);
    let png = oracle_bytes("blue-2x1");
    let request = asset_request(
        &owner,
        "extra",
        "attach",
        json!({"documentSnapshotId":pin,"name":"blue.png","mediaType":"image/png","extra":true}),
    );
    let response = owner.dispatch_asset_command(&request, Some(&png)).unwrap();
    assert_eq!(
        refusal_of(&response),
        (DispatchErrorCode::InvalidRequest, None)
    );
    assert_eq!(
        owner.dispatch_asset_command(&request, Some(&png)).unwrap(),
        response
    );
    let remove = asset_request(
        &owner,
        "extra-rm",
        "remove",
        json!({"documentSnapshotId":pin,"assetId":"asset-x","force":true}),
    );
    let response = owner.dispatch_asset_command(&remove, None).unwrap();
    assert_eq!(
        refusal_of(&response),
        (DispatchErrorCode::InvalidRequest, None)
    );
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (0, (0, 0))
    );
}

#[test]
fn a_mid_flight_cancel_drops_the_preparation_and_publishes_nothing() {
    let mut owner = open_history();
    let before = saved_bytes(&owner);
    let png = oracle_bytes("blue-2x1");
    let polls = std::cell::Cell::new(0);
    let request = attach_request(&owner, "mid", "blue.png", "image/png");
    let response = owner
        .dispatch_asset_command_cancellable(&request, Some(&png), &|| {
            polls.set(polls.get() + 1);
            true
        })
        .unwrap();
    assert_eq!(polls.get(), 1);
    assert_eq!(
        refusal_of(&response),
        (
            DispatchErrorCode::CancelledBeforeDispatch,
            Some("cancelled".into())
        )
    );
    assert_eq!(saved_bytes(&owner), before);
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (0, (0, 0))
    );
    assert!(records_of(&owner).is_empty());
    // The attempt is retained: an exact retry replays it without polling again.
    let replay = owner
        .dispatch_asset_command_cancellable(&request, Some(&png), &|| {
            polls.set(polls.get() + 1);
            false
        })
        .unwrap();
    assert_eq!(replay, response);
    assert_eq!(polls.get(), 1);
    // A fresh id under the same bytes commits, polling exactly once more.
    let fresh = attach_request(&owner, "mid2", "blue.png", "image/png");
    let response = owner
        .dispatch_asset_command_cancellable(&fresh, Some(&png), &|| {
            polls.set(polls.get() + 1);
            false
        })
        .unwrap();
    assert!(response.is_ok());
    assert_eq!(polls.get(), 2);
    assert_eq!(names_of(&owner), ["blue.png"]);
}
