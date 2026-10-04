//! Frozen expectations are authored independently of capture and replay output.
use super::*;
use crate::codec::{decode_project, encode_project};
use serde_json::Value;

pub(super) const BUNDLE: &str = r#"{
  "format":"nemo.native-opacity-reproduction","formatVersion":1,"apiVersion":2,
  "fixture":{"id":"native-opacity-static","version":1,"sha256":"895ca05a43295104301c472287149b1c01aa6ef681db80dcbb79e58a55b36050"},
  "command":"layer.opacity.set","stableTarget":{"layerUid":"r08_curve_layer"},
  "clock":null,"seed":null,"versions":{"nativeEngine":"0.1.0"},
  "commands":[
    {"id":1,"expectedRevision":0,"value":40,"revision":1,"applied":true},
    {"id":2,"expectedRevision":1,"value":60,"revision":2,"applied":true},
    {"id":3,"expectedRevision":2,"value":60,"revision":2,"applied":false}
  ]
}"#;

fn oracle() -> Value {
    json!({
        "initial":{"revision":0,"opacity":25,"undoDepth":0,"redoDepth":0},
        "steps":[
            {"id":1,"expectedRevision":0,"applied":true,"state":{"revision":1,"opacity":40,"undoDepth":1,"redoDepth":0}},
            {"id":2,"expectedRevision":1,"applied":true,"state":{"revision":2,"opacity":60,"undoDepth":2,"redoDepth":0}},
            {"id":3,"expectedRevision":2,"applied":false,"state":{"revision":2,"opacity":60,"undoDepth":2,"redoDepth":0}}
        ]
    })
}

fn frozen_document() -> Value {
    json!({
        "format":"nemo.native-opacity-document","formatVersion":1,"totalFrames":21,
        "layers":[{"layerUid":"r08_curve_layer","motionStatic":{"opacity":[25]},
            "motion":{"opacity":{"keys":[
                {"frame":0,"v":[20],"curvePoints":[{"x":0,"y":0},{"x":0.25,"y":0.156},{"x":0.5,"y":0.5},{"x":0.75,"y":0.844},{"x":1,"y":1}],"hOut":[0,0],"hIn":[0,0]},
                {"frame":20,"v":[80],"curvePoints":[{"x":0,"y":0},{"x":0.25,"y":0.156},{"x":0.5,"y":0.5},{"x":0.75,"y":0.844},{"x":1,"y":1}],"hOut":[0,0],"hIn":[0,0]}
            ]}}
        }]
    })
}

fn document(app: &IsolatedApplication, revision: u64) -> Value {
    serde_json::from_slice(
        &encode_project(app.acquire_snapshot(revision).unwrap().document()).unwrap(),
    )
    .unwrap()
}

fn set(app: &IsolatedApplication, id: &str, value: Value) -> OpacityRequest {
    OpacityRequest::command(
        id,
        app.instance_id(),
        app.document_id(),
        app.content_revision(),
        json!({"command":"layer.opacity.set","stableTarget":{"layerUid":TARGET},"value":value}),
    )
}

#[test]
fn independent_oracle_checks_every_transition_and_exact_unchanged_keyed_document() {
    let report = replay_reproduction_bundle(BUNDLE.as_bytes()).unwrap();
    assert_eq!(serde_json::to_value(&report).unwrap(), oracle());
    let bundle = bundle::decode(BUNDLE.as_bytes()).unwrap();
    let mut app = create_isolated().unwrap();
    assert_eq!(document(&app, 0), frozen_document());
    assert_eq!(replay_validated(&bundle, &mut app).unwrap(), report);
    for (revision, opacity) in [(0, 25), (1, 40), (2, 60)] {
        let mut expected = frozen_document();
        expected["layers"][0]["motionStatic"]["opacity"] = json!([opacity]);
        assert_eq!(document(&app, revision), expected);
    }
    assert_eq!(app.history.history_depths(), (2, 0));
    assert_eq!(app.content_revision(), 2);
    assert_eq!(
        app.requests.len(),
        3,
        "all commands used production dispatch"
    );
    assert_eq!(app.diagnostics.retained_len(), 3);
    for (frame, expected) in [(0, 20.0), (10, 50.0), (20, 80.0)] {
        let response = app.dispatch(OpacityRequest::query(
            format!("evaluate-{frame}"),
            app.instance_id(),
            app.document_id(),
            "query.document.evaluate",
            json!({"atRevision":2,"contextId":"scene-root","frame":frame}),
        ));
        assert!(response.is_ok());
        assert_eq!(
            response.result().unwrap()["layers"][0]["value"].as_f64(),
            Some(expected)
        );
    }
}

#[test]
fn accepted_a2_capture_bytes_replay_through_the_public_raw_byte_boundary() {
    let mut app = create_isolated().unwrap();
    app.opt_in_reproduction().unwrap();
    for (index, value) in [40, 60, 60].into_iter().enumerate() {
        assert!(app
            .dispatch(set(&app, &format!("private-capture-{index}"), json!(value)))
            .is_ok());
    }
    let bytes = app.export_reproduction_bundle().unwrap();
    assert_eq!(
        serde_json::to_value(replay_reproduction_bundle(&bytes).unwrap()).unwrap(),
        oracle()
    );
    for value in [
        json!(25.0),
        json!(-0.0),
        json!(0),
        json!(100),
        json!(1.25e-300),
    ] {
        let mut app = create_isolated().unwrap();
        app.opt_in_reproduction().unwrap();
        for index in 0..2 {
            assert!(app
                .dispatch(set(&app, &format!("numeric-{index}"), value.clone()))
                .is_ok());
        }
        let report =
            replay_reproduction_bundle(&app.export_reproduction_bundle().unwrap()).unwrap();
        assert_eq!(report.steps[1].state.opacity.as_f64(), value.as_f64());
        assert!(!report.steps[1].applied);
        assert_eq!(
            report.steps[1].state.undo_depth,
            usize::from(value.as_f64() != Some(25.0))
        );
    }
}

#[test]
fn generic_one_to_thirty_two_prefixes_are_valid_without_authentication_claims() {
    let mut app = create_isolated().unwrap();
    app.opt_in_reproduction().unwrap();
    for index in 0..32 {
        let value = if index % 2 == 0 { 12 } else { 88 };
        assert!(app
            .dispatch(set(&app, &format!("prefix-{index}"), json!(value)))
            .is_ok());
        let report =
            replay_reproduction_bundle(&app.export_reproduction_bundle().unwrap()).unwrap();
        assert_eq!(report.steps.len(), index + 1);
        let last = report.steps.last().unwrap();
        assert_eq!(last.state.opacity, Number::from(value));
        assert_eq!(last.state.revision, (index + 1) as u64);
        assert_eq!(last.state.undo_depth, index + 1);
        assert_eq!(last.state.redo_depth, 0);
    }
    let mut noops = create_isolated().unwrap();
    noops.opt_in_reproduction().unwrap();
    for index in 0..32 {
        assert!(noops
            .dispatch(set(&noops, &format!("noop-{index}"), json!(25)))
            .is_ok());
    }
    let report = replay_reproduction_bundle(&noops.export_reproduction_bundle().unwrap()).unwrap();
    assert_eq!(report.steps.len(), 32);
    assert!(report
        .steps
        .iter()
        .all(|step| !step.applied && step.state == report.initial));
}

#[test]
fn adjacent_float_commands_keep_exact_captured_numbers_and_dispositions() {
    let mut app = create_isolated().unwrap();
    app.opt_in_reproduction().unwrap();
    let tiny = 1.25e-300_f64;
    let values = [tiny, f64::from_bits(tiny.to_bits() + 1), tiny];
    for (index, value) in values.into_iter().enumerate() {
        assert!(app
            .dispatch(set(&app, &format!("adjacent-{index}"), json!(value)))
            .is_ok());
    }
    let bytes = app.export_reproduction_bundle().unwrap();
    for raw in [
        String::from_utf8(bytes).unwrap(),
        BUNDLE.replace("\"value\":40", "\"va\\u006cue\":1.25e-300"),
    ] {
        let report = replay_reproduction_bundle(raw.as_bytes()).unwrap();
        assert_eq!(report.steps[0].state.opacity.as_f64(), Some(tiny));
    }
    let report = replay_reproduction_bundle(&app.export_reproduction_bundle().unwrap()).unwrap();
    for (index, value) in values.into_iter().enumerate() {
        assert_eq!(report.steps[index].state.opacity.as_f64(), Some(value));
        assert!(report.steps[index].applied);
        assert_eq!(report.steps[index].state.revision, index as u64 + 1);
    }
}

fn matches_oracle(bytes: &[u8]) -> bool {
    replay_reproduction_bundle(bytes)
        .ok()
        .and_then(|report| serde_json::to_value(report).ok())
        .as_ref()
        == Some(&oracle())
}

#[test]
fn omitted_reordered_and_altered_commands_fail_the_separately_frozen_oracle() {
    let baseline: Value = serde_json::from_str(BUNDLE).unwrap();
    assert!(matches_oracle(BUNDLE.as_bytes()));
    for index in 0..3 {
        let mut omitted = baseline.clone();
        omitted["commands"].as_array_mut().unwrap().remove(index);
        assert!(!matches_oracle(&serde_json::to_vec(&omitted).unwrap()));
    }
    let mut reordered = baseline.clone();
    reordered["commands"].as_array_mut().unwrap().swap(0, 1);
    let mut altered = baseline.clone();
    altered["commands"][0]["value"] = json!(41);
    for mutation in [reordered, altered] {
        assert!(!matches_oracle(&serde_json::to_vec(&mutation).unwrap()));
    }
    // These three rewrites are internally consistent generic bundles. Only the
    // independent acceptance oracle, not an unkeyed hash, rejects their intent.
    for commands in [
        json!([{"id":1,"expectedRevision":0,"value":40,"revision":1,"applied":true}]),
        json!([{"id":1,"expectedRevision":0,"value":60,"revision":1,"applied":true},{"id":2,"expectedRevision":1,"value":40,"revision":2,"applied":true},{"id":3,"expectedRevision":2,"value":60,"revision":3,"applied":true}]),
        json!([{"id":1,"expectedRevision":0,"value":41,"revision":1,"applied":true},{"id":2,"expectedRevision":1,"value":60,"revision":2,"applied":true},{"id":3,"expectedRevision":2,"value":60,"revision":2,"applied":false}]),
    ] {
        let mut mutation = baseline.clone();
        mutation["commands"] = commands;
        let bytes = serde_json::to_vec(&mutation).unwrap();
        assert!(replay_reproduction_bundle(&bytes).is_ok());
        assert!(!matches_oracle(&bytes));
    }
}

fn stores(app: &IsolatedApplication) -> Vec<u8> {
    format!(
        "{:?}",
        (
            document(app, app.content_revision()),
            format!("{:?}", app.history),
            format!("{:?}", app.requests),
            format!("{:?}", app.reproduction),
            app.diagnostics.retained_len(),
            app.exports.release_snapshot(),
            app.release_progress(),
            app.replacement_progress()
        )
    )
    .into_bytes()
}

#[test]
fn replay_regenerates_all_transport_ids_and_leaves_a_live_authority_unchanged() {
    let mut private = frozen_document();
    private["layers"][0]["layerUid"] = json!("private-user-document-sentinel");
    let mut user = NativeApplication::new(
        "private-user-instance",
        decode_project(&serde_json::to_vec(&private).unwrap()).unwrap(),
        Denied,
        Denied,
        Denied,
    )
    .unwrap();
    let command = OpacityRequest::command(
        "private-user-request",
        user.instance_id(),
        user.document_id(),
        0,
        json!({"command":"layer.opacity.set","stableTarget":{"layerUid":"private-user-document-sentinel"},"value":73}),
    );
    assert!(user.dispatch(command).is_ok());
    let before = stores(&user);
    let mut first = create_isolated().unwrap();
    let mut second = create_isolated().unwrap();
    assert_ne!(first.instance_id(), second.instance_id());
    assert_ne!(first.document_id(), second.document_id());
    let bundle = bundle::decode(BUNDLE.as_bytes()).unwrap();
    let first_report = replay_validated(&bundle, &mut first).unwrap();
    assert_eq!(
        replay_validated(&bundle, &mut second).unwrap(),
        first_report
    );
    assert!(first
        .requests
        .keys()
        .all(|id| !second.requests.contains_key(id)));
    assert!(!first.requests.contains_key("private-user-request"));
    assert_eq!(
        replay_reproduction_bundle(BUNDLE.as_bytes()).unwrap(),
        first_report
    );
    assert!(replay_reproduction_bundle(br#"{"private":"secret/path"}"#).is_err());
    assert_eq!(stores(&user), before);
    let report = serde_json::to_string(&first_report).unwrap();
    for private in [
        "private-user",
        user.document_id(),
        first.instance_id(),
        first.document_id(),
    ] {
        assert!(!report.contains(private));
    }
}

#[test]
fn all_side_effect_ports_are_denied_and_runtime_mismatch_errors_are_fixed() {
    use crate::render_scene::{prepare, LayerGeometry, OpaqueSrgbPaint};
    use crate::scheduler::{EvaluationKey, FrameScheduler, OutputSpec};
    let mut app = create_isolated().unwrap();
    let handle: OpaqueResourceHandle = serde_json::from_value(json!({
        "resourceId":"private-asset", "resourceVersion":"v1"
    }))
    .unwrap();
    assert_eq!(
        Denied.resolve_geometry(&handle),
        Err(ResourceResolutionError::new(
            ResourceResolutionErrorKind::Unavailable,
            DENIED
        ))
    );
    let snapshot = app.acquire_snapshot(0).unwrap();
    let key = EvaluationKey::new(
        snapshot.id(),
        "scene-root",
        0,
        "final",
        OutputSpec::new("frame", "rgba8", 320, 180, "srgb", "straight").unwrap(),
        [("geometry".into(), "v1".into())],
    )
    .unwrap();
    let mut scheduler = FrameScheduler::new();
    let scheduled = scheduler.schedule(snapshot, key).unwrap();
    let geometry = GeometryPaintInput::new(
        "geometry",
        "v1",
        vec![LayerGeometry::new(
            TARGET,
            [0.0, 0.0, 40.0, 40.0],
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            OpaqueSrgbPaint::new(255, 0, 0),
        )
        .unwrap()],
    )
    .unwrap();
    assert_eq!(
        Denied.compose(&prepare(&scheduler, &scheduled, &geometry).unwrap()),
        Err(DENIED.into())
    );
    assert_eq!(app.exports.port().readback_rgba8(&()), Err(DENIED.into()));
    assert_eq!(
        Denied.begin_staging("private-job", "private/path"),
        Err(DENIED.into())
    );
    assert_eq!(
        Denied.write_frame("private-job", "private/path", b"secret"),
        Err(DENIED.into())
    );
    assert_eq!(
        Denied.publish("private-job", "private/path", &[]),
        Err(DENIED.into())
    );
    assert_eq!(Denied.cleanup("private-job"), Err(DENIED.into()));
    let bundle = bundle::decode(BUNDLE.as_bytes()).unwrap();
    assert!(app.dispatch(set(&app, "preexisting", json!(35))).is_ok());
    assert_eq!(
        replay_validated(&bundle, &mut app),
        Err(ReproductionReplayError::ReplayMismatch)
    );
    let error = replay_with_factory(BUNDLE.as_bytes(), || {
        Err(ReproductionReplayError::CatalogUnavailable)
    });
    assert_eq!(error, Err(ReproductionReplayError::CatalogUnavailable));
    assert_eq!(
        serde_json::to_string(&error.unwrap_err()).unwrap(),
        "\"catalog_unavailable\""
    );
}
