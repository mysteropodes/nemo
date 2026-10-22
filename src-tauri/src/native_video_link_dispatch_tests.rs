//! T12 actual native authority/barrier with a controlled video-probe boundary.
//! Authored for macOS; the Linux cloud container cannot build the Tauri crate.
use crate::application_mcp::revision_sync::video_dispatch::{
    video_source::{self, tests::FakeDecoder, VideoSource},
    VIDEO_LINK,
};
use crate::application_mcp::{ApplicationMcp, NativeApplicationRequest};
use native_engine::{commands::LinkedVideoFacts, native_media_frame_export::NativeVideoDecoder};
use nemo_mcp::contract::NativeApplicationResponse;
use serde_json::{json, Value};
use std::path::Path;
use std::{sync::Arc, time::Duration};

fn state() -> ApplicationMcp {
    let state = ApplicationMcp::default();
    state.install_native_blank_project(320, 180, 24.).unwrap();
    state
}
fn info(state: &ApplicationMcp) -> Value {
    let native = state.native_state();
    let lock = native.lock().unwrap();
    let id = lock.active().unwrap().1.document_id().to_owned();
    drop(lock);
    state.native_project_info(&id).unwrap()
}
fn revision(state: &ApplicationMcp) -> u64 {
    info(state)["contentRevision"].as_u64().unwrap()
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
fn input(state: &ApplicationMcp, id: &str, intent: Value) -> NativeApplicationRequest {
    let pin = info(state);
    serde_json::from_value(json!({"apiVersion":2,"requestId":id,"instanceId":state.instance_id(),
        "documentId":pin["documentId"],"expectedRevision":pin["contentRevision"],"operation":VIDEO_LINK,
        "payload":{"documentSnapshotId":pin["documentSnapshotId"],"lifecycleGeneration":pin["lifecycleGeneration"],
            "source":{"kind":"file","path":"/admitted/clip.mov"},"intent":intent}}))
    .unwrap()
}
fn facts(locator: &str, frame_count: u32) -> LinkedVideoFacts {
    LinkedVideoFacts {
        locator: locator.into(),
        width: 64,
        height: 36,
        frame_count,
    }
}
fn never(_: VideoSource) -> std::future::Ready<Result<LinkedVideoFacts, String>> {
    panic!("this request must not access the video source")
}
fn ok_decoder() -> Box<dyn NativeVideoDecoder> {
    Box::new(FakeDecoder::frames(48))
}
fn zero_decoder() -> Box<dyn NativeVideoDecoder> {
    Box::new(FakeDecoder::frames(0))
}
fn failing_decoder() -> Box<dyn NativeVideoDecoder> {
    let mut decoder = FakeDecoder::frames(48);
    decoder.info = Err("unsupported codec".into());
    Box::new(decoder)
}
fn temp_clip() -> (std::path::PathBuf, String) {
    let directory = std::env::temp_dir().join(format!("nemo-video-link-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("clip.mov");
    std::fs::write(&path, b"container bytes").unwrap();
    (directory, path.to_str().unwrap().to_owned())
}
fn refused_before_publication(reply: &NativeApplicationResponse) -> Value {
    assert!(!reply.ok);
    let error = reply.error.as_ref().unwrap();
    assert_eq!(error.code, "unavailable");
    error.details.clone().unwrap()
}

#[tokio::test]
async fn probe_runs_outside_both_locks_and_publishes_exactly_one_revision_that_saves() {
    let state = state();
    let native = state.native_state();
    let before = revision(&state);
    let reply = state
        .revisions
        .dispatch_video_using(
            state.instance_id(),
            &native,
            input(&state, "host-video", json!({"kind":"newLayer"})),
            false,
            |source| {
                assert!(native.try_lock().is_ok());
                assert!(state.revisions.0.try_lock().is_ok());
                let VideoSource::File { path } = source;
                assert_eq!(path, "/admitted/clip.mov");
                async { Ok(facts("/canonical/clip.mov", 48)) }
            },
        )
        .await
        .unwrap()
        .response;
    assert!(reply.ok, "{reply:?}");
    assert_eq!(reply.content_revision, before + 1);
    let result = reply.result.unwrap();
    assert_eq!(result["newLayer"], true);
    assert_eq!(result["placementId"], "placement-host-video");
    assert_eq!(
        (
            &result["sourceWidth"],
            &result["sourceHeight"],
            &result["sourceFrameCount"]
        ),
        (&json!(64), &json!(36), &json!(48))
    );
    assert_eq!(revision(&state), before + 1);
    let file: Value = serde_json::from_slice(&save(&state)).unwrap();
    assert_eq!(file["assets"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn exact_replay_and_changed_body_answer_before_any_source_access() {
    let state = state();
    let native = state.native_state();
    let request = input(&state, "replay", json!({"kind":"newLayer"}));
    let first = state
        .revisions
        .dispatch_video_using(
            state.instance_id(),
            &native,
            request.clone(),
            false,
            |_| async { Ok(facts("/canonical/clip.mov", 48)) },
        )
        .await
        .unwrap()
        .response;
    assert!(first.ok);
    let committed = save(&state);
    let again = state
        .revisions
        .dispatch_video_using(state.instance_id(), &native, request.clone(), false, never)
        .await
        .unwrap()
        .response;
    assert_eq!(
        serde_json::to_value(again).unwrap(),
        serde_json::to_value(&first).unwrap()
    );
    let mut changed = request;
    changed.payload["source"]["path"] = json!("/another.mov");
    let refused = state
        .revisions
        .dispatch_video_using(state.instance_id(), &native, changed, false, never)
        .await
        .unwrap()
        .response;
    assert_eq!(refused.error.unwrap().code, "invalid_request");
    assert_eq!(save(&state), committed);
}

#[tokio::test]
async fn replace_keeps_the_placement_identity_and_adds_one_more_revision() {
    let state = state();
    let native = state.native_state();
    let created = state
        .revisions
        .dispatch_video_using(
            state.instance_id(),
            &native,
            input(&state, "create", json!({"kind":"newLayer"})),
            false,
            |_| async { Ok(facts("/canonical/a.mov", 48)) },
        )
        .await
        .unwrap()
        .response;
    let placement = created.result.unwrap()["placementId"].clone();
    let before = revision(&state);
    let replaced = state
        .revisions
        .dispatch_video_using(
            state.instance_id(),
            &native,
            input(
                &state,
                "replace",
                json!({"kind":"replace","placementId":placement}),
            ),
            false,
            |_| async { Ok(facts("/canonical/b.mov", 12)) },
        )
        .await
        .unwrap()
        .response;
    assert!(replaced.ok, "{replaced:?}");
    let result = replaced.result.unwrap();
    assert_eq!(result["placementId"], placement);
    assert_eq!(result["newLayer"], false);
    assert_eq!(result["sourceFrameCount"], 12);
    assert_eq!(revision(&state), before + 1);
    let missing = state
        .revisions
        .dispatch_video_using(
            state.instance_id(),
            &native,
            input(
                &state,
                "absent",
                json!({"kind":"replace","placementId":"absent"}),
            ),
            false,
            never,
        )
        .await
        .unwrap()
        .response;
    assert_eq!(missing.error.unwrap().code, "not_found");
    assert_eq!(revision(&state), before + 1);
}

#[tokio::test]
async fn stale_lifecycle_cancel_and_absent_external_subscriber_stop_before_source_work() {
    for mode in ["lifecycle", "cancel", "subscriber", "stale"] {
        let state = state();
        let original = save(&state);
        let mut request = input(&state, mode, json!({"kind":"newLayer"}));
        match mode {
            "lifecycle" => request.payload["lifecycleGeneration"] = 999.into(),
            "cancel" => request.cancelled_before_dispatch = true,
            "stale" => request.expected_revision = Some(5),
            _ => {}
        }
        let reply = state
            .revisions
            .dispatch_video_using(
                state.instance_id(),
                &state.native_state(),
                request,
                mode == "subscriber",
                never,
            )
            .await
            .unwrap()
            .response;
        assert!(!reply.ok, "{mode}");
        assert_eq!(save(&state), original, "{mode}");
    }
}

#[tokio::test]
async fn scope_relative_directory_decoder_failure_and_zero_frames_publish_nothing() {
    let (directory, clip) = temp_clip();
    let cases: [(&str, String, bool, video_source::DecoderFactory); 5] = [
        ("scope", clip.clone(), false, ok_decoder),
        ("relative", "relative/clip.mov".into(), true, ok_decoder),
        (
            "directory",
            directory.to_str().unwrap().into(),
            true,
            ok_decoder,
        ),
        ("decoder", clip.clone(), true, failing_decoder),
        ("frames", clip.clone(), true, zero_decoder),
    ];
    for (label, path, allowed, decoder) in cases {
        let state = state();
        let original = save(&state);
        let mut request = input(&state, label, json!({"kind":"newLayer"}));
        request.payload["source"]["path"] = json!(path);
        let reply = state
            .revisions
            .dispatch_video_using(
                state.instance_id(),
                &state.native_state(),
                request,
                false,
                move |source| {
                    video_source::resolve_with(source, Arc::new(move |_: &Path| allowed), decoder)
                },
            )
            .await
            .unwrap()
            .response;
        let details = refused_before_publication(&reply);
        assert_eq!(details["disposition"], "notDispatched", "{label}");
        assert_eq!(save(&state), original, "{label}");
    }
    let state = state();
    let mut request = input(&state, "real", json!({"kind":"newLayer"}));
    request.payload["source"]["path"] = json!(clip);
    let reply = state
        .revisions
        .dispatch_video_using(
            state.instance_id(),
            &state.native_state(),
            request,
            false,
            |source| video_source::resolve_with(source, Arc::new(|_: &Path| true), ok_decoder),
        )
        .await
        .unwrap()
        .response;
    assert!(reply.ok, "{reply:?}");
    assert_eq!(reply.result.unwrap()["sourceFrameCount"], 48);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn a_probe_that_outlives_its_limit_is_indeterminate_and_publishes_nothing() {
    let state = state();
    let original = save(&state);
    let reply = state
        .revisions
        .dispatch_video_within(
            state.instance_id(),
            &state.native_state(),
            input(&state, "slow", json!({"kind":"newLayer"})),
            false,
            Duration::from_millis(20),
            |_| async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                Ok(facts("/canonical/clip.mov", 48))
            },
        )
        .await
        .unwrap()
        .response;
    let details = refused_before_publication(&reply);
    assert_eq!(details["disposition"], "indeterminate");
    assert_eq!(save(&state), original);
}

#[tokio::test]
async fn wrong_owner_returns_typed_envelopes_before_source_io_without_poisoning_the_request() {
    let state = state();
    let native = state.native_state();
    let genuine = input(&state, "typed-errors", json!({"kind":"newLayer"}));
    let initial = save(&state);
    for (field, code) in [
        ("instanceId", "wrong_instance"),
        ("documentId", "wrong_document"),
    ] {
        let mut altered = serde_json::to_value(&genuine).unwrap();
        altered[field] = json!("foreign");
        let altered: NativeApplicationRequest = serde_json::from_value(altered).unwrap();
        let response = state
            .revisions
            .dispatch_video_using(state.instance_id(), &native, altered, false, never)
            .await
            .unwrap()
            .response;
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, code);
        assert_eq!(response.instance_id, state.instance_id());
        assert_eq!(save(&state), initial);
    }
    let first = state
        .revisions
        .dispatch_video_using(state.instance_id(), &native, genuine, false, |_| async {
            Ok(facts("/canonical/clip.mov", 48))
        })
        .await
        .unwrap()
        .response;
    assert!(
        first.ok,
        "foreign-id failures must not reserve the genuine request id"
    );
}

#[test]
fn the_synchronous_route_refuses_to_probe_outside_the_scoped_async_path() {
    let state = state();
    let before = revision(&state);
    let error = state
        .revisions
        .dispatch(
            state.instance_id(),
            &state.native_state(),
            input(&state, "sync", json!({"kind":"newLayer"})),
            false,
        )
        .err()
        .unwrap();
    assert_eq!(error, "native video source requires async scoped dispatch");
    assert_eq!(revision(&state), before);
}
