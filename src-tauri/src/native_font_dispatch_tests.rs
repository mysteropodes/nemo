//! PRIVATE real authority/history with injected source I/O. All tests UNRUN.
use super::*;
use crate::application_mcp::ApplicationMcp;
use base64::Engine;
use serde_json::{json, Value};
use std::cell::Cell;
const FONT: &[u8] = include_bytes!("../../src/fonts/Roboto-Regular.ttf");
fn state() -> ApplicationMcp {
    let state = ApplicationMcp::default();
    state.install_native_blank_project(320, 180, 24.).unwrap();
    state
}
fn info(state: &ApplicationMcp) -> Value {
    let native = state.native_state();
    let lock = native.lock().unwrap();
    let doc = lock.active().unwrap().1.document_id().to_owned();
    drop(lock);
    state.native_project_info(&doc).unwrap()
}
fn import(state: &ApplicationMcp, id: &str) -> NativeApplicationRequest {
    let pin = info(state);
    serde_json::from_value(json!({"apiVersion":2,"requestId":id,"instanceId":state.instance_id(),
        "documentId":pin["documentId"],"expectedRevision":pin["contentRevision"],"operation":font_source::IMPORT,
        "payload":{"command":"object.asset.font.import","documentSnapshotId":pin["documentSnapshotId"],
            "lifecycleGeneration":pin["lifecycleGeneration"],"source":{"kind":"bundled","fontKey":"Roboto-Regular"}}})).unwrap()
}
fn query(state: &ApplicationMcp, payload: Value) -> NativeApplicationRequest {
    let pin = info(state);
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":"font-query","instanceId":state.instance_id(),
        "documentId":pin["documentId"],"operation":font_source::QUERY,"payload":payload}),
    )
    .unwrap()
}
fn save(state: &ApplicationMcp) -> Vec<u8> {
    let pin = info(state);
    state
        .native_project_file_snapshot(
            pin["documentId"].as_str().unwrap(),
            pin["contentRevision"].as_u64().unwrap(),
            pin["documentSnapshotId"].as_str().unwrap(),
        )
        .unwrap()
        .unwrap()
}
async fn apply(state: &ApplicationMcp, request: NativeApplicationRequest) -> Delivery {
    state
        .revisions
        .dispatch_font_using(
            state.instance_id(),
            &state.native_state(),
            request,
            false,
            |_| async { Ok(FontBytes::Bundled(FONT)) },
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn real_import_has_no_io_locks_and_preserves_identity_through_save_reopen() {
    let state = state();
    let request = import(&state, "host-import");
    let native = state.native_state();
    let delivery = state
        .revisions
        .dispatch_font_using(state.instance_id(), &native, request.clone(), false, |_| {
            assert!(native.try_lock().is_ok());
            assert!(state.revisions.0.try_lock().is_ok());
            async { Ok(FontBytes::Bundled(FONT)) }
        })
        .await
        .unwrap();
    assert!(delivery.response.ok);
    let result = delivery.response.result.unwrap();
    let id = result["assetId"].as_str().unwrap();
    let original = save(&state);
    let saved: Value = serde_json::from_slice(&original).unwrap();
    assert_eq!(saved["assets"][0]["id"], id);
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(saved["assets"][0]["source"]["data"].as_str().unwrap())
            .unwrap(),
        FONT
    );
    let reopened = ApplicationMcp::default();
    reopened.install_native_project_file(&original).unwrap();
    let pin = info(&reopened);
    let response=reopened.dispatch_native_async(query(&reopened,json!({"action":"list","atRevision":0,"documentSnapshotId":pin["documentSnapshotId"],"offset":0,"limit":1}))).await.unwrap();
    assert!(response.ok);
    assert_eq!(response.result.unwrap()["rows"][0]["assetId"], id);
    assert_eq!(save(&reopened), original);
}

#[tokio::test]
async fn retained_original_import_replays_before_any_source_work() {
    let state = state();
    let request = import(&state, "original");
    let first = apply(&state, request.clone()).await.response;
    let second = state
        .revisions
        .dispatch_font_using(
            state.instance_id(),
            &state.native_state(),
            request,
            false,
            |_| async {
                panic!("no source replay");
                #[allow(unreachable_code)]
                Ok(FontBytes::Bundled(FONT))
            },
        )
        .await
        .unwrap()
        .response;
    assert_eq!(first, second);
    assert_eq!(info(&state)["contentRevision"], 1);
}

#[tokio::test]
async fn source_failure_preserves_existing_font_and_retains_indeterminate_disposition() {
    let state = state();
    apply(&state, import(&state, "first")).await;
    let original = save(&state);
    let response = state
        .revisions
        .dispatch_font_using(
            state.instance_id(),
            &state.native_state(),
            import(&state, "failed"),
            false,
            |_| async { Err("source failed".into()) },
        )
        .await
        .unwrap()
        .response;
    assert!(!response.ok);
    assert_eq!(
        response.error.unwrap().details.unwrap()["disposition"],
        "indeterminate"
    );
    assert_eq!(save(&state), original);
    assert_eq!(info(&state)["contentRevision"], 1);
}

#[tokio::test]
async fn cancelled_or_stale_lifecycle_refuses_before_source_work() {
    for cancelled in [true, false] {
        let state = state();
        let original = save(&state);
        let mut request = import(&state, "refused");
        if cancelled {
            request.cancelled_before_dispatch = true;
        } else {
            request.payload["lifecycleGeneration"] = 999.into();
        }
        let delivery = state
            .revisions
            .dispatch_font_using(
                state.instance_id(),
                &state.native_state(),
                request,
                false,
                |_| async {
                    panic!("no source");
                    #[allow(unreachable_code)]
                    Ok(FontBytes::Bundled(FONT))
                },
            )
            .await
            .unwrap();
        assert!(!delivery.response.ok);
        assert_eq!(save(&state), original);
    }
}

#[tokio::test]
async fn actual_release_during_source_io_prevents_old_owner_publication() {
    use crate::{
        native_application::admit_release_request,
        native_application_contract::NativeReleaseRequest,
    };
    let state = state();
    let request = import(&state, "late");
    let pin = info(&state);
    let native = state.native_state();
    let response = state
        .revisions
        .dispatch_font_using(state.instance_id(), &native, request, false, |_| {
            let release: NativeReleaseRequest =
                serde_json::from_value(json!({"apiVersion":2,"requestId":"close-font",
            "instanceId":state.instance_id(),"documentId":pin["documentId"],"expectedRevision":0}))
                .unwrap();
            admit_release_request(&native, &release).unwrap();
            async { Ok(FontBytes::Bundled(FONT)) }
        })
        .await
        .unwrap()
        .response;
    assert!(!response.ok);
    assert!(native.lock().unwrap().active().is_none());
}

#[tokio::test]
async fn malformed_producer_bytes_cannot_replace_existing_admitted_font() {
    let state = state();
    apply(&state, import(&state, "accepted")).await;
    let original = save(&state);
    let response = state
        .revisions
        .dispatch_font_using(
            state.instance_id(),
            &state.native_state(),
            import(&state, "invalid"),
            false,
            |_| async { Ok(FontBytes::Downloaded(b"not-font".to_vec())) },
        )
        .await
        .unwrap()
        .response;
    assert!(!response.ok);
    assert_eq!(save(&state), original);
    assert_eq!(info(&state)["contentRevision"], 1);
}

#[tokio::test]
async fn absent_external_subscriber_stops_producer_before_io() {
    let state = state();
    let calls = Cell::new(0);
    let response = state
        .revisions
        .dispatch_font_using(
            state.instance_id(),
            &state.native_state(),
            import(&state, "external"),
            true,
            |_| {
                calls.set(calls.get() + 1);
                async { Ok(FontBytes::Bundled(FONT)) }
            },
        )
        .await
        .unwrap()
        .response;
    assert!(!response.ok);
    assert_eq!(calls.get(), 0);
    assert_eq!(info(&state)["contentRevision"], 0);
}

#[tokio::test]
async fn existing_pending_revision_barrier_prevents_second_font_execution() {
    let state = state();
    let native = state.native_state();
    let binding = state
        .revisions
        .control(&native, RevisionControl::Binding)
        .unwrap();
    state
        .revisions
        .control(
            &native,
            RevisionControl::Subscribe {
                binding: serde_json::from_value(binding).unwrap(),
            },
        )
        .unwrap();
    let first = state
        .revisions
        .dispatch_font_using(
            state.instance_id(),
            &native,
            import(&state, "external-one"),
            true,
            |_| async { Ok(FontBytes::Bundled(FONT)) },
        )
        .await
        .unwrap();
    assert!(first.response.ok && first.notification.is_some());
    let second = state
        .revisions
        .dispatch_font_using(
            state.instance_id(),
            &native,
            import(&state, "external-two"),
            true,
            |_| async {
                panic!("pending barrier must stop I/O");
                #[allow(unreachable_code)]
                Ok(FontBytes::Bundled(FONT))
            },
        )
        .await
        .unwrap();
    assert!(!second.response.ok);
    assert_eq!(info(&state)["contentRevision"], 1);
}

#[test]
fn serde_native_request_encoding_matches_shared_fingerprint_vectors() {
    let vectors: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-font-v1/fingerprint-vectors.json"
    ))
    .unwrap();
    for vector in vectors["vectors"].as_array().unwrap() {
        let request: OpacityRequest = serde_json::from_value(vector["request"].clone()).unwrap();
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            vector["canonicalJson"].as_str().unwrap()
        );
    }
}
