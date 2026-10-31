//! #2361 DRAW-C: the bitmap-brush resource route over a real project with a real embedded PNG asset ("ink", 2x1) and a
//! real bitmap anchor; no synthetic scene. Authored UNRUN on a machine without the toolchain.
use super::*;
use crate::application::ObjectBrushRuntime;
use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_store::BrushBitmapStore;
use crate::history::NativeObjectHistory;
use crate::object_document::BitmapBrushFields;
use crate::revision::ImageResourceBudget;
use serde_json::{json, Value};

fn owner() -> NativeObjectHistory {
    BitmapBrushFields::runtime_test_owner(false)
}
fn runtime(limit: usize) -> ObjectBrushRuntime {
    ObjectBrushRuntime::with_store(BrushBitmapStore::with_budget(
        ImageResourceBudget::isolated(limit).unwrap(),
    ))
}
fn request(snapshot: &ObjectSnapshot, id: &str, payload: Value) -> OpacityRequest {
    let operation = if payload.get("action").is_some() { QUERY } else { PUBLISH };
    let mut wire = json!({"apiVersion":2,"requestId":id,"instanceId":snapshot.instance_id(),
        "documentId":snapshot.document_id(),"operation":operation,"payload":payload});
    if operation == PUBLISH {
        wire["expectedRevision"] = snapshot.content_revision().into();
    }
    serde_json::from_value(wire).unwrap()
}
fn publish(snapshot: &ObjectSnapshot, asset: &str, brush: &str, width: u32, height: u32) -> OpacityRequest {
    request(
        snapshot,
        "publish",
        json!({"command":"object.brush.bitmap.publish","documentSnapshotId":snapshot.snapshot_id(),
            "assetId":asset,"brushId":brush,"width":width,"height":height}),
    )
}
fn inspect(snapshot: &ObjectSnapshot, brush: &str) -> OpacityRequest {
    request(
        snapshot,
        "inspect",
        json!({"action":"inspect","atRevision":snapshot.content_revision(),
            "documentSnapshotId":snapshot.snapshot_id(),"brushId":brush}),
    )
}
fn scene(snapshot: &ObjectSnapshot, offset: u64) -> OpacityRequest {
    request(
        snapshot,
        "scene",
        json!({"action":"scene","atRevision":snapshot.content_revision(),
            "documentSnapshotId":snapshot.snapshot_id(),"contextId":"scene-root","frame":0,
            "offset":offset,"limit":1}),
    )
}
fn refused(response: &ResponseEnvelope) -> (DispatchErrorCode, String) {
    assert!(!response.ok && response.result.is_none());
    let error = response.error.as_ref().unwrap();
    (error.code, error.details.as_ref().unwrap()["reason"].as_str().unwrap().to_owned())
}

#[test]
fn publish_inspect_and_scene_round_trip_without_a_revision_or_history_entry() {
    let owner = owner();
    let revision = owner.content_revision();
    let snapshot = owner.acquire_snapshot(revision).unwrap();
    let mut runtime = runtime(1 << 20);
    let first = runtime.dispatch_bitmap_resource(&snapshot, &publish(&snapshot, "ink", "b1", 2, 1));
    assert!(first.ok, "{first:?}");
    assert_eq!(first.content_revision, revision);
    assert_eq!(
        first.result.unwrap(),
        json!({"applied":true,"historyEntriesAdded":0,"brushId":"b1","version":"1","width":2,"height":1,"bytes":8})
    );
    // The same asset allocation again reuses the retained bitmap: same version, nothing applied.
    let again = runtime.dispatch_bitmap_resource(&snapshot, &publish(&snapshot, "ink", "b1", 2, 1));
    assert_eq!(again.result.unwrap()["applied"], false);
    let row = runtime.dispatch_bitmap_resource(&snapshot, &inspect(&snapshot, "b1"));
    assert_eq!(
        row.result.unwrap()["brush"],
        json!({"brushId":"b1","resident":true,"version":"1","width":2,"height":1,"bytes":8})
    );
    let absent = runtime.dispatch_bitmap_resource(&snapshot, &inspect(&snapshot, "nope"));
    assert_eq!(absent.result.unwrap()["brush"], json!({"brushId":"nope","resident":false}));
    assert_eq!(owner.content_revision(), revision);
}

#[test]
fn scene_reports_a_missing_ink_as_a_typed_entry_until_it_is_published_under_the_reported_id() {
    let owner = owner();
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let mut runtime = runtime(1 << 20);
    let missing = runtime.dispatch_bitmap_resource(&snapshot, &scene(&snapshot, 0)).result.unwrap();
    assert_eq!(missing["totalEntries"], 1);
    assert_eq!(missing["nextOffset"], Value::Null);
    let entry = &missing["entries"][0];
    assert_eq!(entry["layerUid"], "L");
    assert_eq!(entry["strokeId"], "anchor");
    assert_eq!(entry["unavailable"], json!({"code":"ink-missing"}));
    // Publishing the asset under the id the scene reported is what makes the ink drawn.
    let brush = entry["brushId"].as_str().unwrap().to_owned();
    assert!(runtime.dispatch_bitmap_resource(&snapshot, &publish(&snapshot, "ink", &brush, 2, 1)).ok);
    let drawn = runtime.dispatch_bitmap_resource(&snapshot, &scene(&snapshot, 0)).result.unwrap();
    assert_eq!(drawn["entries"][0]["ink"], json!({"version":"1","width":2,"height":1,"bytes":8}));
    assert!(drawn["entries"][0].get("unavailable").is_none());
    // A page past the single record is empty and an offset beyond the total is refused.
    let past = runtime.dispatch_bitmap_resource(&snapshot, &scene(&snapshot, 1)).result.unwrap();
    assert_eq!((past["entries"].as_array().unwrap().len(), past["nextOffset"].clone()), (0, Value::Null));
    let beyond = runtime.dispatch_bitmap_resource(&snapshot, &scene(&snapshot, 2));
    assert_eq!(refused(&beyond), (DispatchErrorCode::InvalidRequest, "offset-beyond-total".into()));
}

#[test]
fn typed_refusals_leave_the_store_and_the_quota_unchanged() {
    use DispatchErrorCode::*;
    let owner = owner();
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let mut runtime = runtime(1 << 20);
    let mut run = |request: OpacityRequest| refused(&runtime.dispatch_bitmap_resource(&snapshot, &request));
    assert_eq!(run(publish(&snapshot, "ink", "b", 3, 3)), (InvalidRequest, "asset-wrong-size".into()));
    assert_eq!(run(publish(&snapshot, "absent", "b", 2, 1)), (NotFound, "asset-missing".into()));
    assert_eq!(run(publish(&snapshot, "ink", "b", 0, 1)), (InvalidRequest, "malformed-request".into()));
    assert_eq!(run(publish(&snapshot, "ink", "b", 4097, 1)), (InvalidRequest, "malformed-request".into()));
    let mut stale = publish(&snapshot, "ink", "b", 2, 1);
    stale.expected_revision = Some(snapshot.content_revision() + 1);
    assert_eq!(run(stale), (StaleRevision, "stale-snapshot".into()));
    let mut other = publish(&snapshot, "ink", "b", 2, 1);
    other.payload["documentSnapshotId"] = "other-snapshot".into();
    assert_eq!(run(other), (StaleRevision, "stale-snapshot".into()));
    let mut wrong = publish(&snapshot, "ink", "b", 2, 1);
    wrong.instance_id = "someone-else".into();
    assert_eq!(run(wrong), (WrongInstance, "wrong-instance".into()));
    let mut cancelled = publish(&snapshot, "ink", "b", 2, 1);
    cancelled.cancelled_before_dispatch = true;
    assert_eq!(run(cancelled), (CancelledBeforeDispatch, "cancelled".into()));
    let mut extra = publish(&snapshot, "ink", "b", 2, 1);
    extra.payload["pixels"] = "AAAA".into();
    assert_eq!(run(extra), (InvalidRequest, "malformed-request".into()));
    let mut old = inspect(&snapshot, "b");
    old.payload["atRevision"] = (snapshot.content_revision() + 1).into();
    assert_eq!(run(old), (StaleRevision, "stale-snapshot".into()));
    // Nothing above reached the store.
    let row = runtime.dispatch_bitmap_resource(&snapshot, &inspect(&snapshot, "b")).result.unwrap();
    assert_eq!(row["brush"]["resident"], false);
}

#[test]
fn an_exhausted_quota_is_a_typed_refusal_and_a_released_session_is_unavailable() {
    use DispatchErrorCode::*;
    let owner = owner();
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    // The 2x1 bitmap costs 8 bytes plus the decode scratch; four bytes of quota fits none of it.
    let mut tight = runtime(4);
    let over = tight.dispatch_bitmap_resource(&snapshot, &publish(&snapshot, "ink", "b", 2, 1));
    assert_eq!(refused(&over), (Unavailable, "over-quota".into()));
    let row = tight.dispatch_bitmap_resource(&snapshot, &inspect(&snapshot, "b")).result.unwrap();
    assert_eq!(row["brush"]["resident"], false);
    let mut closed = runtime(1 << 20);
    assert!(closed.dispatch_bitmap_resource(&snapshot, &publish(&snapshot, "ink", "b", 2, 1)).ok);
    closed.release();
    for request in [publish(&snapshot, "ink", "b", 2, 1), inspect(&snapshot, "b"), scene(&snapshot, 0)] {
        let response = closed.dispatch_bitmap_resource(&snapshot, &request);
        assert_eq!(refused(&response), (Unavailable, "session-released".into()));
    }
}

#[test]
fn only_the_two_declared_operations_are_owned() {
    assert!(ObjectBrushRuntime::owns_bitmap_resource(PUBLISH));
    assert!(ObjectBrushRuntime::owns_bitmap_resource(QUERY));
    for other in ["command.document.object.brush.bitmap.stroke", "query.document.object"] {
        assert!(!ObjectBrushRuntime::owns_bitmap_resource(other));
    }
}
