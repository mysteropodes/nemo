//! Actual native owner, existing providers and authenticated ACK controls. Authored UNRUN.
use super::property_edit_tests::{history, round_trip, saved};
use crate::application_mcp::revision_sync::import_batch_dispatch::{
    is_import_batch_sources, BatchSource, SourceBytes,
};
use crate::application_mcp::{revision_sync::RevisionControl, ApplicationMcp};
use nemo_mcp::contract::NativeApplicationRequest;
use serde_json::{json, Value};
use std::{cell::Cell, path::PathBuf, sync::Arc};

const FONT: &[u8] = include_bytes!("../../src/fonts/Roboto-Regular.ttf");

fn state() -> ApplicationMcp {
    let state = ApplicationMcp::default();
    state.install_native_blank_project(320, 180, 24.).unwrap();
    state
}

fn info(state: &ApplicationMcp) -> Value {
    let native = state.native_state();
    let authority = native.lock().unwrap();
    let id = authority.active().unwrap().1.document_id().to_owned();
    drop(authority);
    state.native_project_info(&id).unwrap()
}

fn request(state: &ApplicationMcp, id: &str) -> NativeApplicationRequest {
    let pin = info(state);
    serde_json::from_value(json!({
        "apiVersion":2,"requestId":id,"instanceId":state.instance_id(),
        "documentId":pin["documentId"],"expectedRevision":pin["contentRevision"],
        "operation":"command.document.object.import.batch","payload":{
            "command":"object.import.batch","documentSnapshotId":pin["documentSnapshotId"],
            "lifecycleGeneration":pin["lifecycleGeneration"],"layers":[],"records":[],
            "sources":[{"sourceId":"font","name":"Body","mediaType":"font/ttf",
                "source":{"kind":"bundled","fontKey":"Roboto-Regular"}}]
        }
    }))
    .unwrap()
}

fn mixed(state: &ApplicationMcp, id: &str, path: &str) -> NativeApplicationRequest {
    let mut command = request(state, id);
    command.payload["sources"].as_array_mut().unwrap().insert(
        0,
        json!({
            "sourceId":"pixels","name":"Pixels","mediaType":"image/png",
            "source":{"kind":"file","path":path}
        }),
    );
    command.payload["layers"] = json!([{"sourceId":"page","name":"Imported"}]);
    let target = json!({"contextId":"scene-root","layerUid":"page","strokeId":"",
        "frameScope":{"kind":"authored","frame":0}});
    command.payload["raster"] = json!([{"sourceId":"bitmap","entry":{
        "target":target,"item":{"assetId":"pixels","x":10,"y":20.5,
            "width":2,"height":1,"opacity":0.5,"rotation":0}
    }}]);
    command.payload["text"] = json!([{"sourceId":"text","entry":{
        "target":target,"block":{"text":"Actual text","font":{"kind":"asset","assetId":"font"},
            "size":17.5,"color":"#ff00aa80","align":"left",
            "fixedWidth":null,"anchorTopLeft":{"x":-12.75,"y":104.5}}
    }}]);
    command
}

fn png() -> Vec<u8> {
    let corpus =
        include_str!("../../native-engine/tests/fixtures/object-raster-asset-attach/oracle.txt");
    let row = corpus
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|row| row["name"] == "blue-2x1")
        .unwrap();
    let hex = row["hex"].as_str().unwrap();
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
        .collect()
}

struct ScopedPng(PathBuf);
impl ScopedPng {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("nemo-import-{}.png", uuid::Uuid::new_v4()));
        std::fs::write(&path, png()).unwrap();
        Self(path)
    }
}
impl Drop for ScopedPng {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn subscribe(state: &ApplicationMcp) -> Value {
    let binding = state
        .revisions
        .control(&state.native_state(), RevisionControl::Binding)
        .unwrap();
    state
        .revisions
        .control(
            &state.native_state(),
            RevisionControl::Subscribe {
                binding: serde_json::from_value(binding.clone()).unwrap(),
            },
        )
        .unwrap();
    binding
}

#[tokio::test]
async fn scoped_file_and_actual_bundled_font_join_items_once_and_survive_history_and_reopen() {
    let state = state();
    let initial_pin = info(&state);
    let id = initial_pin["documentId"].as_str().unwrap();
    let initial = saved(&state, id);
    let file = ScopedPng::new();
    let command = mixed(&state, "scoped-batch", file.0.to_str().unwrap());
    command.validate().unwrap();
    assert!(serde_json::to_vec(&command).unwrap().len() <= 4096);
    let allowed = file.0.canonicalize().unwrap();
    let first = state
        .revisions
        .dispatch_scoped(
            state.instance_id(),
            &state.native_state(),
            command.clone(),
            false,
            Some(Arc::new(move |path| path == allowed.as_path())),
        )
        .await
        .unwrap()
        .response;
    assert!(first.ok, "{first:?}");
    assert_eq!(first.content_revision, 1);
    let result = first.result.as_ref().unwrap();
    assert_eq!(result["applied"], true);
    assert_eq!(result["historyEntriesAdded"], 1);
    assert_eq!(result["assets"].as_array().unwrap().len(), 2);
    assert!(serde_json::to_vec(&first).unwrap().len() <= 4096);
    let changed = saved(&state, id);
    let document: Value = serde_json::from_slice(&changed).unwrap();
    let original: Value = serde_json::from_slice(&initial).unwrap();
    assert_eq!(document["project"], original["project"]);
    assert_eq!(document["assets"][0]["name"], "Pixels");
    assert_eq!(document["assets"][1]["name"], "Body");
    assert_eq!(
        document["drawing"]["items"]["raster"][0]["item"]["assetId"],
        document["assets"][0]["id"]
    );
    assert_eq!(
        document["drawing"]["items"]["text"][0]["block"]["font"]["assetId"],
        document["assets"][1]["id"]
    );
    let reopened = ApplicationMcp::default();
    let (reopened_id, _, _) = reopened.install_native_project_file(&changed).unwrap();
    assert_eq!(saved(&reopened, &reopened_id), changed);
    drop(file);
    // Replay needs neither a live file nor a new filesystem permission.
    let replay = state
        .revisions
        .dispatch_scoped(
            state.instance_id(),
            &state.native_state(),
            command,
            false,
            None,
        )
        .await
        .unwrap()
        .response;
    assert_eq!(replay, first);
    assert!(history(&state, id, "undo-batch", 1, false).ok);
    assert_eq!(saved(&state, id), initial);
    assert!(history(&state, id, "redo-batch", 2, true).ok);
    assert_eq!(saved(&state, id), changed);
}

#[tokio::test]
async fn real_authenticated_batch_requires_subscription_and_one_correlated_ack_then_replays_without_io(
) {
    let state = state();
    let command = request(&state, "wire-batch");
    let id = command.document_id.clone();
    let initial = saved(&state, &id);
    let (refused, events) = round_trip(&state, command.clone(), None).await;
    assert!(!refused.ok);
    assert_eq!(refused.error.unwrap().code, "unavailable");
    assert_eq!(events, 0);
    assert_eq!(saved(&state, &id), initial);
    let binding = subscribe(&state);
    // Existing helper proves commit before event, bad ACK rejection and blocked second write.
    let (first, events) = round_trip(&state, command.clone(), Some(&binding)).await;
    assert!(first.ok, "{first:?}");
    assert_eq!(first.content_revision, 1);
    assert_eq!(first.result.as_ref().unwrap()["historyEntriesAdded"], 1);
    assert_eq!(events, 1);
    let changed = saved(&state, &id);
    let (replay, events) = round_trip(&state, command.clone(), None).await;
    assert_eq!(replay, first);
    assert_eq!(events, 0);
    let mut altered = command.clone();
    altered.payload["sources"][0]["name"] = json!("Changed body");
    let (refused, events) = round_trip(&state, altered, None).await;
    assert_eq!(refused.error.unwrap().code, "invalid_request");
    assert_eq!(events, 0);
    assert_eq!(saved(&state, &id), changed);
    assert!(history(&state, &id, "wire-undo", 1, false).ok);
    assert_eq!(saved(&state, &id), initial);
    let replay = state
        .revisions
        .dispatch_import_batch_using(
            state.instance_id(),
            &state.native_state(),
            command,
            false,
            |_| async {
                panic!("retained source batch must not resolve again");
                #[allow(unreachable_code)]
                Ok(SourceBytes::Font(
                    crate::application_mcp::font_source::FontBytes::Bundled(FONT),
                ))
            },
        )
        .await
        .unwrap()
        .response;
    assert_eq!(replay, first);
    assert!(history(&state, &id, "wire-redo", 2, true).ok);
    assert_eq!(saved(&state, &id), changed);
}

#[tokio::test]
async fn all_sources_resolve_outside_locks_and_later_failure_or_invalid_bytes_publish_nothing() {
    for corrupt in [false, true] {
        let state = state();
        let command = mixed(&state, "all-or-nothing", "/admitted/pixels.png");
        let initial = saved(&state, &command.document_id);
        let native = state.native_state();
        let calls = Cell::new(0);
        let response = state
            .revisions
            .dispatch_import_batch_using(
                state.instance_id(),
                &native,
                command.clone(),
                false,
                |source| {
                    assert!(native.try_lock().is_ok());
                    assert!(state.revisions.0.try_lock().is_ok());
                    calls.set(calls.get() + 1);
                    let result = match source {
                        BatchSource::Image(_) => Ok(SourceBytes::Image(png())),
                        BatchSource::Font(_) if corrupt => Ok(SourceBytes::Font(
                            crate::application_mcp::font_source::FontBytes::Downloaded(
                                b"not-font".to_vec(),
                            ),
                        )),
                        BatchSource::Font(_) => Err("second source failed".into()),
                    };
                    async move { result }
                },
            )
            .await
            .unwrap()
            .response;
        assert!(!response.ok);
        assert_eq!(calls.get(), 2);
        assert_eq!(info(&state)["contentRevision"], 0);
        assert_eq!(saved(&state, &command.document_id), initial);
        if !corrupt {
            assert_eq!(
                response.error.unwrap().details.unwrap()["disposition"],
                "indeterminate"
            );
        }
    }
}

#[tokio::test]
async fn stale_cancel_lifecycle_and_foreign_identity_refuse_before_any_source_resolution() {
    for mode in [
        "stale",
        "cancel",
        "lifecycle",
        "instance",
        "document",
        "snapshot",
    ] {
        let state = state();
        let mut command = request(&state, mode);
        let id = command.document_id.clone();
        let initial = saved(&state, &id);
        match mode {
            "stale" => command.expected_revision = Some(1),
            "cancel" => command.cancelled_before_dispatch = true,
            "lifecycle" => command.payload["lifecycleGeneration"] = json!(999),
            "instance" => command.instance_id = "foreign".into(),
            "document" => command.document_id = "foreign".into(),
            "snapshot" => command.payload["documentSnapshotId"] = json!("foreign"),
            _ => unreachable!(),
        }
        let response = state
            .revisions
            .dispatch_import_batch_using(
                state.instance_id(),
                &state.native_state(),
                command,
                false,
                |_| async {
                    panic!("preflight must stop source resolution");
                    #[allow(unreachable_code)]
                    Ok(SourceBytes::Font(
                        crate::application_mcp::font_source::FontBytes::Bundled(FONT),
                    ))
                },
            )
            .await
            .unwrap()
            .response;
        assert!(!response.ok, "{mode}");
        if mode == "instance" {
            assert_eq!(response.error.unwrap().code, "wrong_instance");
        } else if mode == "document" {
            assert_eq!(response.error.unwrap().code, "wrong_document");
        }
        assert_eq!(saved(&state, &id), initial);
        assert_eq!(info(&state)["contentRevision"], 0);
    }
}

#[tokio::test]
async fn actual_scoped_source_denial_and_present_malformed_sources_never_fall_back_to_vector_import(
) {
    let state = state();
    let file = ScopedPng::new();
    let command = mixed(&state, "denied", file.0.to_str().unwrap());
    let initial = saved(&state, &command.document_id);
    let response = state
        .revisions
        .dispatch_scoped(
            state.instance_id(),
            &state.native_state(),
            command.clone(),
            false,
            Some(Arc::new(|_| false)),
        )
        .await
        .unwrap()
        .response;
    assert!(!response.ok);
    assert_eq!(saved(&state, &command.document_id), initial);
    for malformed in [
        Value::Null,
        json!([]),
        json!({}),
        json!([{"sourceId":"partial"}]),
    ] {
        let mut invalid = request(&state, "malformed");
        invalid.payload["sources"] = malformed;
        assert!(is_import_batch_sources(&invalid));
        assert!(invalid.validate().is_err());
        assert!(state
            .revisions
            .dispatch_scoped(
                state.instance_id(),
                &state.native_state(),
                invalid,
                false,
                None,
            )
            .await
            .is_err());
        assert_eq!(saved(&state, &command.document_id), initial);
    }
}

#[tokio::test]
async fn actual_owner_release_during_source_resolution_prevents_late_batch_publication() {
    use crate::{
        native_application::admit_release_request,
        native_application_contract::NativeReleaseRequest,
    };
    let state = state();
    let pin = info(&state);
    let command = request(&state, "late-batch");
    let native = state.native_state();
    let response = state
        .revisions
        .dispatch_import_batch_using(state.instance_id(), &native, command, false, |_| {
            assert!(native.try_lock().is_ok());
            assert!(state.revisions.0.try_lock().is_ok());
            let release: NativeReleaseRequest = serde_json::from_value(json!({
                "apiVersion":2,"requestId":"close-batch","instanceId":state.instance_id(),
                "documentId":pin["documentId"],"expectedRevision":0
            }))
            .unwrap();
            admit_release_request(&native, &release).unwrap();
            async {
                Ok(SourceBytes::Font(
                    crate::application_mcp::font_source::FontBytes::Bundled(FONT),
                ))
            }
        })
        .await
        .unwrap()
        .response;
    assert!(!response.ok);
    assert!(native.lock().unwrap().active().is_none());
}

#[tokio::test]
async fn discovered_generation_admits_real_source_batch_after_owner_replacement() {
    let state = state();
    let old_generation = info(&state)["lifecycleGeneration"].as_u64().unwrap();
    state.retire_native_for_test(json!({"status":"succeeded"}));
    state.install_native_blank_project(320, 180, 24.).unwrap();
    let status = serde_json::to_value(state.native_status(serde_json::from_value(json!({
        "apiVersion":2,"requestId":"batch-generation-discovery","instanceId":state.instance_id()
    })).unwrap()).unwrap()).unwrap();
    let generation = status["lifecycleGeneration"]
        .as_u64()
        .expect("An official client must discover the actual source-batch lifecycle pin");
    assert!(
        generation > old_generation,
        "a guessed initial generation cannot qualify"
    );
    let file = ScopedPng::new();
    let mut command = mixed(&state, "discovered-source-batch", file.0.to_str().unwrap());
    command.document_id = status["documentId"].as_str().unwrap().into();
    command.expected_revision = status["contentRevision"].as_u64();
    command.payload["lifecycleGeneration"] = json!(generation);
    command.validate().unwrap();
    let id = command.document_id.clone();
    let initial = saved(&state, &id);
    let mut stale = command.clone();
    stale.request_id = "retired-source-batch".into();
    stale.payload["lifecycleGeneration"] = json!(old_generation);
    let refused = state
        .revisions
        .dispatch_import_batch_using(
            state.instance_id(),
            &state.native_state(),
            stale,
            false,
            |_| async {
                panic!("retired generation must refuse before resolving any source");
                #[allow(unreachable_code)]
                Ok(SourceBytes::Font(
                    crate::application_mcp::font_source::FontBytes::Bundled(FONT),
                ))
            },
        )
        .await
        .unwrap()
        .response;
    assert!(!refused.ok);
    assert_eq!(saved(&state, &id), initial);
    let allowed = file.0.canonicalize().unwrap();
    let response = state
        .revisions
        .dispatch_scoped(
            state.instance_id(),
            &state.native_state(),
            command,
            false,
            Some(Arc::new(move |path| path == allowed.as_path())),
        )
        .await
        .unwrap()
        .response;
    assert!(response.ok, "{response:?}");
    assert_eq!(response.content_revision, 1);
    let result = response.result.unwrap();
    assert_eq!(result["historyEntriesAdded"], 1);
    assert_eq!(result["assets"].as_array().unwrap().len(), 2);
    assert_eq!(result["objects"].as_array().unwrap().len(), 2);
    let changed = saved(&state, &id);
    assert_ne!(changed, initial);
    assert!(history(&state, &id, "discovered-batch-undo", 1, false).ok);
    assert_eq!(saved(&state, &id), initial);
    assert!(history(&state, &id, "discovered-batch-redo", 2, true).ok);
    assert_eq!(saved(&state, &id), changed);
}
