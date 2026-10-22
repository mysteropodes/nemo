//! Actual atomic source request boundary; source bytes use the retained PNG/font fixtures.
use super::super::super::asset_tests::{project_bytes, project_history, raster, text};
use super::{ImportAssetSource, NativeObjectHistory};
use crate::request_receipts::{OpacityRequest, ResponseEnvelope};
use serde_json::{json, Value};
include!("object_raster_asset_attach_support.rs");

fn source_request(owner: &NativeObjectHistory, id: &str, font: bool) -> OpacityRequest {
    let origin = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let image = raster("picture", "L1", 0, "pixels");
    let mut payload = json!({"command":"object.import.batch","layers":[],"records":[],
        "documentSnapshotId":origin.snapshot_id(),"lifecycleGeneration":1,
        "raster":[{"sourceId":image.source_id,"entry":image.entry}],
        "sources":[{"sourceId":"pixels","name":"blue.png","mediaType":"image/png",
            "source":{"kind":"file","path":"/native-source-test/blue.png"}}]});
    if font {
        let caption = text("caption", "L1", json!({"kind":"asset","assetId":"font"}));
        payload["text"] = json!([{"sourceId":caption.source_id,"entry":caption.entry}]);
        payload["sources"]
            .as_array_mut()
            .unwrap()
            .push(json!({"sourceId":"font","name":"Roboto-Regular.ttf",
            "mediaType":"font/ttf","source":{"kind":"bundled","fontKey":"Roboto-Regular"}}));
    }
    OpacityRequest::history_stage(
        id,
        owner.instance_id(),
        owner.document_id(),
        Some(owner.content_revision()),
        "command.document.object.import.batch",
        payload,
    )
}
fn value(response: ResponseEnvelope) -> Value {
    serde_json::to_value(response).unwrap()
}
fn frozen(owner: &NativeObjectHistory) -> (u64, Vec<u8>, (usize, usize)) {
    (
        owner.content_revision(),
        project_bytes(owner),
        owner.history_depths(),
    )
}
fn png_source(bytes: &[u8]) -> ImportAssetSource<'_> {
    ImportAssetSource {
        source_id: "pixels",
        name: "blue.png",
        media_type: "image/png",
        bytes,
    }
}

#[test]
fn actual_png_source_request_has_one_publication_and_replays_after_undo_without_bytes() {
    let mut owner = project_history(2);
    let request = source_request(&owner, "source-png-1", false);
    let initial = frozen(&owner);
    assert!(owner
        .import_batch_sources_status(&request)
        .unwrap()
        .is_none());
    assert_eq!(
        frozen(&owner),
        initial,
        "probe cannot publish or reserve history"
    );
    let bytes = oracle_bytes("blue-2x1");
    let first = value(
        owner
            .dispatch_import_batch_sources(&request, &[png_source(&bytes)])
            .unwrap(),
    );
    assert_eq!(first["ok"], true);
    assert_eq!(first["contentRevision"], 1);
    assert_eq!(first["result"]["historyEntriesAdded"], 1);
    assert_eq!(first["result"]["assets"][0]["sourceId"], "pixels");
    assert_eq!(first["result"]["objects"][0]["sourceId"], "picture");
    assert_eq!(owner.history_depths(), (1, 0));
    let imported = frozen(&owner);
    assert_eq!(
        value(owner.dispatch_import_batch_sources(&request, &[]).unwrap()),
        first
    );
    assert_eq!(
        frozen(&owner),
        imported,
        "replay never requires bytes or republishes"
    );
    assert_eq!(owner.undo(1).unwrap(), 2);
    let undone = frozen(&owner);
    assert_eq!(undone.1, initial.1);
    assert_eq!(
        value(
            owner
                .import_batch_sources_status(&request)
                .unwrap()
                .unwrap()
        ),
        first
    );
    assert_eq!(
        value(owner.dispatch_import_batch_sources(&request, &[]).unwrap()),
        first
    );
    assert_eq!(
        frozen(&owner),
        undone,
        "retained replay after undo does not reapply"
    );
    assert_eq!(owner.redo(2).unwrap(), 3);
    assert_eq!(project_bytes(&owner), imported.1);
    let reopened = NativeObjectHistory::from_project_file("source-reopened", &imported.1).unwrap();
    assert_eq!(project_bytes(&reopened), imported.1);
}

#[cfg(feature = "font_asset_wire")]
#[test]
fn actual_png_and_font_source_request_keeps_declared_order_in_one_checkpoint() {
    let mut owner = project_history(2);
    let request = source_request(&owner, "source-png-font-1", true);
    let png = oracle_bytes("blue-2x1");
    let font = include_bytes!("../../src/fonts/Roboto-Regular.ttf");
    let sources = [
        png_source(&png),
        ImportAssetSource {
            source_id: "font",
            name: "Roboto-Regular.ttf",
            media_type: "font/ttf",
            bytes: font,
        },
    ];
    let response = value(
        owner
            .dispatch_import_batch_sources(&request, &sources)
            .unwrap(),
    );
    assert_eq!(response["ok"], true);
    assert_eq!(response["contentRevision"], 1);
    assert_eq!(
        response["result"]["assets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["sourceId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["pixels", "font"]
    );
    assert_eq!(
        response["result"]["objects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["sourceId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["picture", "caption"]
    );
    assert_eq!(owner.history_depths(), (1, 0));
    let saved = project_bytes(&owner);
    let reopened = NativeObjectHistory::from_project_file("source-font-reopened", &saved).unwrap();
    assert_eq!(project_bytes(&reopened), saved);
}

#[test]
fn source_request_changed_body_cannot_poison_the_genuine_retained_answer() {
    let mut owner = project_history(2);
    let request = source_request(&owner, "source-body-1", false);
    let png = oracle_bytes("blue-2x1");
    let first = value(
        owner
            .dispatch_import_batch_sources(&request, &[png_source(&png)])
            .unwrap(),
    );
    assert_eq!(first["ok"], true);
    let before = frozen(&owner);
    let mut changed = request.clone();
    changed.payload["sources"][0]["source"]["path"] = json!("/native-source-test/other.png");
    let refused = value(
        owner
            .import_batch_sources_status(&changed)
            .unwrap()
            .unwrap(),
    );
    assert_eq!(refused["ok"], false);
    assert_eq!(refused["error"]["code"], "invalid_request");
    assert_eq!(frozen(&owner), before);
    assert_eq!(
        value(
            owner
                .import_batch_sources_status(&request)
                .unwrap()
                .unwrap()
        ),
        first
    );
}

#[test]
fn source_request_refuses_cancel_stale_foreign_and_unmatched_bytes_without_touching_redo() {
    let png = oracle_bytes("blue-2x1");
    for scenario in [
        "cancel", "stale", "snapshot", "instance", "document", "source", "bytes",
    ] {
        let mut owner = project_history(2);
        super::super::super::support::prime_redo(&mut owner);
        let before = frozen(&owner);
        let mut request = source_request(&owner, &format!("source-refuse-{scenario}"), false);
        let mut bytes = png_source(&png);
        let expected = match scenario {
            "cancel" => {
                request.cancelled_before_dispatch = true;
                "cancelled_before_dispatch"
            }
            "stale" => {
                request.expected_revision = Some(0);
                "stale_revision"
            }
            "snapshot" => {
                request.payload["documentSnapshotId"] = json!("native-object:other:0");
                "stale_revision"
            }
            "instance" => {
                request.instance_id = "other-instance".into();
                "wrong_instance"
            }
            "document" => {
                request.document_id = "other-document".into();
                "wrong_document"
            }
            "source" => {
                bytes.source_id = "unrequested";
                "invalid_request"
            }
            "bytes" => {
                bytes.bytes = b"not a PNG";
                "invalid_request"
            }
            _ => unreachable!(),
        };
        let response = value(
            owner
                .dispatch_import_batch_sources(&request, &[bytes])
                .unwrap(),
        );
        assert_eq!(response["ok"], false, "{scenario}");
        assert_eq!(response["error"]["code"], expected, "{scenario}");
        assert_eq!(
            frozen(&owner),
            before,
            "{scenario} must preserve full revision/file/history/redo"
        );
        assert!(
            owner.redo(before.0).is_ok(),
            "{scenario} must leave the actual redo usable"
        );
    }
}

#[test]
fn cancellation_after_source_preparation_drops_the_unpublished_batch_and_preserves_redo() {
    use std::cell::Cell;
    let mut owner = project_history(2);
    super::super::super::support::prime_redo(&mut owner);
    let before = frozen(&owner);
    let request = source_request(&owner, "source-cancel-prepared", false);
    let png = oracle_bytes("blue-2x1");
    let calls = Cell::new(0);
    let response = value(
        owner
            .dispatch_import_batch_sources_cancellable(&request, &[png_source(&png)], &|| {
                calls.set(calls.get() + 1);
                calls.get() == 2
            })
            .unwrap(),
    );
    assert_eq!(
        calls.get(),
        2,
        "the second cancellation fence follows actual preparation"
    );
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "cancelled_before_dispatch");
    assert_eq!(frozen(&owner), before);
    assert!(owner.redo(before.0).is_ok());
}
