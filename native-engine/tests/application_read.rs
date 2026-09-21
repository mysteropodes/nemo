//! Frozen N19A read oracles through the common dispatcher; no production host activation.
use crate::application::{ExportResourceResolver, NativeApplication, ResourceResolutionError};
use crate::codec::{decode_project, encode_project};
use crate::commands::{
    DispatchErrorCode, OpacityRequest, ResponseEnvelope, OP_QUERY_EVALUATE, OP_QUERY_SERIALIZE,
};
use crate::document::OpacityDocument;
use crate::export_job::{ExportArtifact, ExportCompositor, ExportReadback, StagedArtifactPort};
use crate::history::NativeOpacityHistory;
use crate::protocol::OpaqueResourceHandle;
use crate::read_queries::MAX_READ_RESPONSE_BYTES;
use crate::render_scene::{GeometryPaintInput, RenderScene};
use crate::transaction::OP_HISTORY_UNDO;
use serde_json::{json, Value};

const PROJECT: &[u8] = include_bytes!("fixtures/opacity-v2/project.json");
const LAYER: &str = "r08_curve_layer";
struct Unused;
impl StagedArtifactPort for Unused {
    fn begin_staging(&mut self, _: &str, _: &str) -> Result<(), String> {
        panic!("read staged output")
    }
    fn write_frame(&mut self, _: &str, _: &str, _: &[u8]) -> Result<(), String> {
        panic!("read wrote output")
    }
    fn publish(&mut self, _: &str, _: &str, _: &[String]) -> Result<ExportArtifact, String> {
        panic!("read published output")
    }
    fn cleanup(&mut self, _: &str) -> Result<(), String> {
        Ok(())
    }
}
impl ExportCompositor for Unused {
    type Composition = ();
    fn compose(&mut self, _: &RenderScene) -> Result<(), String> {
        panic!("read rendered")
    }
    fn readback_rgba8(&self, _: &()) -> Result<ExportReadback, String> {
        panic!("read pixels")
    }
}
impl ExportResourceResolver for Unused {
    fn resolve_geometry(
        &mut self,
        _: &OpaqueResourceHandle,
    ) -> Result<GeometryPaintInput, ResourceResolutionError> {
        panic!("read resolved geometry")
    }
}
type App = NativeApplication<Unused, Unused, Unused>;
fn app(document: OpacityDocument) -> App {
    NativeApplication::new("read-fixture", document, Unused, Unused, Unused).unwrap()
}
fn query(app: &App, id: &str, operation: &str, payload: Value) -> OpacityRequest {
    OpacityRequest::query(id, app.instance_id(), app.document_id(), operation, payload)
}
fn set(app: &App, id: &str, value: u32) -> OpacityRequest {
    OpacityRequest::command(
        id,
        app.instance_id(),
        app.document_id(),
        app.content_revision(),
        json!({
            "command": "layer.opacity.set", "stableTarget": {"layerUid": LAYER}, "value": value,
        }),
    )
}
fn error(response: &ResponseEnvelope, code: DispatchErrorCode) {
    assert!(!response.is_ok(), "{response:?}");
    assert_eq!(response.error().unwrap().code(), code);
}
fn serialized(app: &mut App, id: &str, revision: u64) -> ResponseEnvelope {
    app.dispatch(query(
        app,
        id,
        OP_QUERY_SERIALIZE,
        json!({"atRevision": revision}),
    ))
}

#[test]
fn pinned_serialization_survives_25_40_60_undo_and_preserves_ids_and_tracks() {
    let source: Value = serde_json::from_slice(PROJECT).unwrap();
    let mut app = app(decode_project(PROJECT).unwrap());
    let pinned = query(
        &app,
        "pinned-25",
        OP_QUERY_SERIALIZE,
        json!({"atRevision": 0}),
    );
    let initial = app.dispatch(pinned.clone());
    assert_eq!(initial.result().unwrap()["document"], source);
    assert_eq!(initial.result().unwrap()["atRevision"], 0);
    assert_eq!(
        initial.result().unwrap()["documentSnapshotId"],
        app.acquire_snapshot(0).unwrap().id()
    );
    assert_eq!(initial.result().unwrap().as_object().unwrap().len(), 3);
    assert!(app.dispatch(set(&app, "set-40", 40)).is_ok());
    let forty = serialized(&mut app, "forty", 1);
    assert!(app.dispatch(set(&app, "set-60", 60)).is_ok());
    assert_eq!(app.dispatch(pinned.clone()), initial);
    assert_eq!(
        serialized(&mut app, "fresh-pin", 0).result(),
        initial.result()
    );
    let undo = OpacityRequest::history_stage(
        "undo",
        app.instance_id(),
        app.document_id(),
        Some(2),
        OP_HISTORY_UNDO,
        json!({}),
    );
    assert!(app.dispatch(undo).is_ok());
    let undone = serialized(&mut app, "undone", 3);
    assert_eq!(
        undone.result().unwrap()["document"],
        forty.result().unwrap()["document"]
    );
    let stored = &undone.result().unwrap()["document"]["layers"][0];
    assert_eq!(stored["motionStatic"]["opacity"], json!([40]));
    assert_eq!(stored["layerUid"], LAYER);
    assert_eq!(stored["motion"], source["layers"][0]["motion"]);
    assert_eq!(app.dispatch(pinned), initial);
    assert_eq!(app.content_revision(), 3);
    assert_eq!(
        initial.content_revision(),
        0,
        "retry preserves original envelope head"
    );
}

#[test]
fn evaluate_uses_exact_snapshot_context_frame_and_frozen_static_keyed_oracles() {
    let mut document: Value = serde_json::from_slice(PROJECT).unwrap();
    document["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid": "static", "motionStatic": {"opacity": [25]}}));
    let mut app = app(decode_project(&serde_json::to_vec(&document).unwrap()).unwrap());
    for (frame, expected) in [(0, 20.0), (10, 50.0), (20, 80.0)] {
        let request = query(
            &app,
            &format!("frame-{frame}"),
            OP_QUERY_EVALUATE,
            json!({"atRevision": 0, "contextId": "scene-root", "frame": frame}),
        );
        let response = app.dispatch(request.clone());
        assert_eq!(
            response.result().unwrap(),
            &json!({
                "documentSnapshotId": app.acquire_snapshot(0).unwrap().id(),
                "documentId": app.document_id(), "contentRevision": 0,
                "contextId": "scene-root", "frame": frame,
                "layers": [{"layerUid": LAYER, "value": expected}, {"layerUid": "static", "value": 25.0}],
            })
        );
        if frame == 0 {
            assert!(app.dispatch(set(&app, "advance", 40)).is_ok());
        }
        assert_eq!(
            app.dispatch(request),
            response,
            "same result and original head after edits"
        );
    }
    assert_eq!(app.content_revision(), 1);
    assert_eq!(
        serde_json::from_slice::<Value>(
            &encode_project(app.acquire_snapshot(0).unwrap().document()).unwrap()
        )
        .unwrap(),
        document
    );
}

#[test]
fn read_failures_cancel_replay_identity_and_replacement_are_deterministic() {
    let mut app = app(decode_project(PROJECT).unwrap());
    let serialize = query(&app, "cancel", OP_QUERY_SERIALIZE, json!({"atRevision": 0}));
    let mut explicit_null = serde_json::to_value(&serialize).unwrap();
    explicit_null["expectedRevision"] = Value::Null;
    assert!(serde_json::from_value::<OpacityRequest>(explicit_null).is_err());
    error(
        &app.dispatch(serialize.clone().cancelled()),
        DispatchErrorCode::CancelledBeforeDispatch,
    );
    assert!(
        app.dispatch(serialize.clone()).is_ok(),
        "first cancellation is unretained"
    );
    let mut changed = serialize.clone();
    changed.payload = json!({"atRevision": 1});
    error(&app.dispatch(changed), DispatchErrorCode::InvalidRequest);
    for (index, (operation, payload, code)) in [
        (
            OP_QUERY_SERIALIZE,
            json!({}),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            OP_QUERY_SERIALIZE,
            json!({"atRevision": 0, "extra": true}),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            OP_QUERY_SERIALIZE,
            json!({"atRevision": -1}),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            OP_QUERY_EVALUATE,
            json!({"contextId": "scene-root", "frame": 0}),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            OP_QUERY_EVALUATE,
            json!({"atRevision": 0, "contextId": "scene-root", "frame": 21}),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            OP_QUERY_EVALUATE,
            json!({"atRevision": 0, "contextId": "scene-root", "frame": -1}),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            OP_QUERY_EVALUATE,
            json!({"atRevision": 0, "contextId": "scene-root", "frame": 0.5}),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            OP_QUERY_EVALUATE,
            json!({"atRevision": 0, "contextId": "scene-root", "frame": 4294967296_u64}),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            OP_QUERY_EVALUATE,
            json!({"atRevision": 0, "contextId": "other", "frame": 0}),
            DispatchErrorCode::Unavailable,
        ),
        (
            OP_QUERY_EVALUATE,
            json!({"atRevision": 0, "contextId": "bad id", "frame": 0}),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            OP_QUERY_EVALUATE,
            json!({"atRevision": 0, "contextId": "scene-root", "frame": 0, "extra": 1}),
            DispatchErrorCode::InvalidRequest,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        error(
            &app.dispatch(query(&app, &format!("invalid-{index}"), operation, payload)),
            code,
        );
    }
    for operation in [OP_QUERY_SERIALIZE, OP_QUERY_EVALUATE] {
        let revision = app.content_revision() + 1;
        let payload = if operation == OP_QUERY_SERIALIZE {
            json!({"atRevision": revision})
        } else {
            json!({"atRevision": revision, "contextId": "scene-root", "frame": 0})
        };
        let missing = query(&app, operation, operation, payload);
        let failure = app.dispatch(missing.clone());
        error(&failure, DispatchErrorCode::NotFound);
        // Retain failed reads even after their requested revision becomes available.
        assert!(app
            .dispatch(set(
                &app,
                &format!("advance-{revision}"),
                30 + revision as u32
            ))
            .is_ok());
        assert_eq!(app.dispatch(missing), failure);
        let mut invalid = query(
            &app,
            &format!("revision-{operation}"),
            operation,
            json!({"atRevision": 0}),
        );
        invalid.expected_revision = Some(0);
        error(&app.dispatch(invalid), DispatchErrorCode::InvalidRequest);
    }
    for (field, code) in [
        ("instanceId", DispatchErrorCode::WrongInstance),
        ("documentId", DispatchErrorCode::WrongDocument),
    ] {
        let mut wrong = serde_json::to_value(&serialize).unwrap();
        wrong[field] = json!("other");
        error(&app.dispatch(serde_json::from_value(wrong).unwrap()), code);
    }
    app.replace_document(decode_project(PROJECT).unwrap())
        .unwrap();
    error(&app.dispatch(serialize), DispatchErrorCode::WrongDocument);
    assert!(
        serialized(&mut app, "cancel", 0).is_ok(),
        "replacement clears old receipts"
    );
}

#[test]
fn full_envelope_overflow_is_unavailable_and_failure_retries_pin_the_original_head() {
    let mut source: Value = serde_json::from_slice(PROJECT).unwrap();
    source["layers"] = json!((0..150)
        .map(|index| json!({
            "layerUid": format!("layer-{index}"), "motionStatic": {"opacity": [25]},
        }))
        .collect::<Vec<_>>());
    let mut app = app(decode_project(&serde_json::to_vec(&source).unwrap()).unwrap());
    for operation in [OP_QUERY_SERIALIZE, OP_QUERY_EVALUATE] {
        let payload = if operation == OP_QUERY_SERIALIZE {
            json!({"atRevision": 0})
        } else {
            json!({"atRevision": 0, "contextId": "scene-root", "frame": 0})
        };
        let request = query(&app, operation, operation, payload);
        let failure = app.dispatch(request.clone());
        error(&failure, DispatchErrorCode::Unavailable);
        assert!(serde_json::to_vec(&failure).unwrap().len() <= MAX_READ_RESPONSE_BYTES);
        let mut command = set(
            &app,
            &format!("edit-{operation}"),
            if operation == OP_QUERY_SERIALIZE {
                40
            } else {
                60
            },
        );
        command.payload["stableTarget"]["layerUid"] = json!("layer-0");
        assert!(app.dispatch(command).is_ok());
        assert!(app.content_revision() > failure.content_revision());
        assert_eq!(app.dispatch(request), failure);
    }
}

#[test]
fn response_limit_includes_identity_envelope_even_when_the_result_alone_fits() {
    assert_eq!(MAX_READ_RESPONSE_BYTES, 4096);
    for operation in [OP_QUERY_SERIALIZE, OP_QUERY_EVALUATE] {
        let mut proved = false;
        for count in 1..150 {
            let source = json!({"format": "nemo.native-opacity-document", "formatVersion": 1,
                "totalFrames": 21, "layers": (0..count).map(|index| json!({
                    "layerUid": format!("layer-{index}"), "motionStatic": {"opacity": [25]},
                })).collect::<Vec<_>>()});
            let mut app = app(decode_project(&serde_json::to_vec(&source).unwrap()).unwrap());
            let snapshot = app.acquire_snapshot(0).unwrap();
            let (payload, result) = if operation == OP_QUERY_SERIALIZE {
                (
                    json!({"atRevision": 0}),
                    json!({"atRevision": 0,
                    "documentSnapshotId": snapshot.id(), "document": source}),
                )
            } else {
                (
                    json!({"atRevision": 0, "contextId": "scene-root", "frame": 0}),
                    json!({
                        "documentSnapshotId": snapshot.id(), "documentId": app.document_id(),
                        "contentRevision": 0, "contextId": "scene-root", "frame": 0,
                        "layers": (0..count).map(|index| json!({
                            "layerUid": format!("layer-{index}"), "value": 25.0,
                        })).collect::<Vec<_>>(),
                    }),
                )
            };
            let expected = json!({"apiVersion": 2, "requestId": "limit", "instanceId": app.instance_id(),
                "documentId": app.document_id(), "contentRevision": 0, "ok": true, "result": result});
            if serde_json::to_vec(&expected).unwrap().len() > 4096 {
                assert!(serde_json::to_vec(&result).unwrap().len() <= 4096);
                error(
                    &app.dispatch(query(&app, "limit", operation, payload)),
                    DispatchErrorCode::Unavailable,
                );
                proved = true;
                break;
            }
        }
        assert!(
            proved,
            "fixture must cross the full-envelope limit for {operation}"
        );
    }
}

#[test]
fn reads_do_not_change_history_or_revisions_and_codec_invariants_fail_closed() {
    let mut history =
        NativeOpacityHistory::new("history-read", decode_project(PROJECT).unwrap()).unwrap();
    for operation in [OP_QUERY_SERIALIZE, OP_QUERY_EVALUATE] {
        let payload = if operation == OP_QUERY_SERIALIZE {
            json!({"atRevision": 0})
        } else {
            json!({"atRevision": 0, "contextId": "scene-root", "frame": 10})
        };
        let read = OpacityRequest::query(
            operation,
            history.instance_id(),
            history.document_id(),
            operation,
            payload,
        );
        assert!(history.handle(read).is_ok());
        assert_eq!(history.content_revision(), 0);
        assert_eq!(history.history_depths(), (0, 0));
    }
    let mut corrupt = decode_project(PROJECT).unwrap();
    corrupt.layers[0].motion_static.as_mut().unwrap().opacity[0] = 101.into();
    let mut corrupt_app = app(corrupt);
    error(
        &serialized(&mut corrupt_app, "invariant", 0),
        DispatchErrorCode::Internal,
    );

    let mut unsupported: Value = serde_json::from_slice(PROJECT).unwrap();
    unsupported["layers"][0]["layerUid"] = json!("layer identity with spaces");
    let mut app = app(decode_project(&serde_json::to_vec(&unsupported).unwrap()).unwrap());
    for operation in [OP_QUERY_SERIALIZE, OP_QUERY_EVALUATE] {
        let payload = if operation == OP_QUERY_SERIALIZE {
            json!({"atRevision": 0})
        } else {
            json!({"atRevision": 0, "contextId": "scene-root", "frame": 0})
        };
        error(
            &app.dispatch(query(&app, operation, operation, payload)),
            DispatchErrorCode::Unavailable,
        );
        assert_eq!(app.content_revision(), 0);
    }
}
