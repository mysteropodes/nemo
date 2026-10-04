use crate::native_reproduction_session::*;
use crate::{
    native_application::{admit_release_request, complete_release},
    native_application_contract::{admit_project, GeometryResourceInput, NativeReleaseRequest},
    native_dispatch::{NativePhase, ReleaseAdmission, ReplacementProgress},
};
use native_engine::commands::{OpacityRequest, ResponseEnvelope};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, sync::OnceLock};

const FIXTURE: &[u8] = include_bytes!("../../native-engine/fixtures/reproduction-opacity-v1.json");
const LAYER: &str = "r08_curve_layer";

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("nemo-catalog-test-{}", uuid::Uuid::new_v4())))
    }
    fn ports(&self) -> HostResult<(DesktopArtifactPort, SharedCompositor)> {
        static COMPOSITOR: OnceLock<SharedCompositor> = OnceLock::new();
        let compositor = COMPOSITOR
            .get_or_init(|| {
                SharedCompositor::new(
                    Compositor::new().expect("native host tests require the real GPU context"),
                )
            })
            .clone();
        let artifacts = DesktopArtifactPort::new(self.0.clone(), Vec::new())
            .map_err(|error| host_error("unavailable", error))?;
        Ok((artifacts, compositor))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn value(state: &ApplicationMcp) -> Value {
    json!({"apiVersion":2,"instanceId":state.instance_id(),"optIn":true,
        "fixture":{"id":"native-opacity-static","version":1,
            "sha256":"895ca05a43295104301c472287149b1c01aa6ef681db80dcbb79e58a55b36050"}})
}
fn request(state: &ApplicationMcp) -> NativeReproductionSessionRequest {
    serde_json::from_value(value(state)).unwrap()
}
fn open(state: &ApplicationMcp, scratch: &Scratch) -> NativeReproductionSessionReceipt {
    open_session(state, request(state), || scratch.ports()).unwrap()
}
fn dispatch(state: &ApplicationMcp, request: OpacityRequest) -> ResponseEnvelope {
    let native = state.native_state();
    let mut authority = native.lock().unwrap();
    let generation = authority.active_generation().unwrap();
    authority.active_mut(generation).unwrap().dispatch(request)
}
fn query(state: &ApplicationMcp, id: &str, operation: &str) -> ResponseEnvelope {
    let document = state
        .native_state()
        .lock()
        .unwrap()
        .active()
        .unwrap()
        .1
        .document_id()
        .to_owned();
    dispatch(
        state,
        OpacityRequest::query(id, state.instance_id(), document, operation, json!({})),
    )
}
fn set(state: &ApplicationMcp, id: &str, value: Value) -> OpacityRequest {
    let native = state.native_state();
    let guard = native.lock().unwrap();
    let (_, app) = guard.active().unwrap();
    OpacityRequest::command(
        id,
        app.instance_id(),
        app.document_id(),
        app.content_revision(),
        json!({"command":"layer.opacity.set","stableTarget":{"layerUid":LAYER},"value":value}),
    )
}
fn denied<T>(result: HostResult<T>) -> String {
    result.err().expect("must reject").code
}
fn status(state: &ApplicationMcp) -> Value {
    query(state, "status", "query.reproduction.status")
        .result()
        .unwrap()
        .clone()
}
fn release(state: &ApplicationMcp, fail: bool) {
    let native = state.native_state();
    let request = {
        let guard = native.lock().unwrap();
        let (_, app) = guard.active().unwrap();
        NativeReleaseRequest {
            api_version: 2,
            request_id: "close".into(),
            instance_id: state.instance_id().into(),
            document_id: app.document_id().into(),
            expected_revision: app.content_revision(),
            cancelled_before_dispatch: false,
        }
    };
    let ReleaseAdmission::Execute { generation } =
        admit_release_request(&native, &request).unwrap()
    else {
        panic!("new release");
    };
    let receipt = complete_release(&native, generation, &request, || {
        if fail {
            Err(host_error("cleanup_failed", "injected cleanup failure"))
        } else {
            Ok((Vec::new(), "not_present"))
        }
    })
    .unwrap();
    assert_eq!(receipt.reentry_available, !fail);
}
fn imported() -> crate::native_application_contract::AdmittedProject {
    let resources: Vec<GeometryResourceInput> = serde_json::from_value(json!([{
        "resourceId":"geometry","resourceVersion":"v1","layers":[{
            "layerUid":LAYER,"bounds":[20,60,40,80],"transform":[1,0,0,1,0,0],
            "paint":{"red":255,"green":0,"blue":0}}]}]))
    .unwrap();
    admit_project(&serde_json::from_slice(FIXTURE).unwrap(), &resources).unwrap()
}

#[test]
fn explicit_catalog_admission_installs_one_authority_and_shared_v2_capture() {
    let state = ApplicationMcp::default();
    let scratch = Scratch::new();
    let receipt = open(&state, &scratch);
    let wire = serde_json::to_value(&receipt).unwrap();
    assert_eq!(wire["origin"], "embedded_catalog");
    assert_eq!(wire["fixture"], value(&state)["fixture"]);
    assert_eq!(wire["instanceId"], state.instance_id());
    assert_eq!(wire["contentRevision"], 0);
    assert_eq!(wire["viewportAvailable"], false);
    assert_eq!(wire["resourceCount"], 0);
    assert_eq!(
        wire["reproduction"],
        json!({"state":"recording","reason":null,"commandCount":0,"exportable":false})
    );
    assert!(serde_json::to_vec(&receipt).unwrap().len() < 1024);
    let native = state.native_state();
    {
        let guard = native.lock().unwrap();
        let (generation, app) = guard.active().unwrap();
        assert_eq!(generation, receipt.lifecycle_generation);
        assert_eq!(app.document_id(), receipt.document_id);
    }
    let command = set(&state, "set-40", json!(40));
    assert!(dispatch(&state, command.clone()).is_ok());
    assert!(dispatch(&state, command).is_ok()); // Identical retry never appends twice.
    assert_eq!(status(&state)["commandCount"], 1);
    let export = query(&state, "export", "query.reproduction.export");
    assert!(export.is_ok());
    let bytes = serde_json::to_vec(&export.result().unwrap()["bundle"]).unwrap();
    let replay = native_engine::application::replay_reproduction_bundle(&bytes).unwrap();
    assert_eq!(replay.steps.len(), 1);
    assert_eq!(replay.steps[0].state.opacity, serde_json::Number::from(40));
    assert_eq!(replay.steps[0].state.undo_depth, 1);
    assert_eq!(fs::read_dir(&scratch.0).unwrap().count(), 0);
}

#[test]
fn closed_request_and_exact_selector_reject_before_any_ports_or_authority_change() {
    let state = ApplicationMcp::default();
    let valid = value(&state);
    let fixture = &valid["fixture"];
    for malformed in [
        Value::Null,
        json!([]),
        json!([2, state.instance_id(), true, fixture]),
        json!({"apiVersion":2,"instanceId":state.instance_id(),"optIn":true,
            "fixture":[fixture["id"],fixture["version"],fixture["sha256"]]}),
    ] {
        assert!(serde_json::from_value::<NativeReproductionSessionRequest>(malformed).is_err());
    }
    let wire = serde_json::to_string(&valid).unwrap();
    for duplicate in [
        wire.replacen("\"optIn\":true", "\"optIn\":true,\"optIn\":true", 1),
        wire.replacen("\"version\":1", "\"version\":1,\"version\":1", 1),
    ] {
        assert!(serde_json::from_str::<NativeReproductionSessionRequest>(&duplicate).is_err());
    }
    for extra in [
        "projection",
        "resources",
        "outputBindings",
        "viewport",
        "documentId",
        "sessionId",
    ] {
        let mut candidate = value(&state);
        candidate[extra] = json!("private-caller-data");
        assert!(serde_json::from_value::<NativeReproductionSessionRequest>(candidate).is_err());
    }
    for key in ["apiVersion", "instanceId", "optIn", "fixture"] {
        let mut candidate = value(&state);
        candidate.as_object_mut().unwrap().remove(key);
        assert!(serde_json::from_value::<NativeReproductionSessionRequest>(candidate).is_err());
    }
    let mut extra = value(&state);
    extra["fixture"]["bytes"] = json!(FIXTURE);
    assert!(serde_json::from_value::<NativeReproductionSessionRequest>(extra).is_err());
    for (path, replacement) in [
        ("/apiVersion", json!(1)),
        ("/instanceId", json!("foreign")),
        ("/optIn", json!(false)),
        ("/fixture/id", json!("user")),
        ("/fixture/version", json!(2)),
        ("/fixture/sha256", json!("0".repeat(64))),
        ("/instanceId", json!("a".repeat(129))),
    ] {
        let mut candidate = value(&state);
        *candidate.pointer_mut(path).unwrap() = replacement;
        let result = open_session(&state, serde_json::from_value(candidate).unwrap(), || {
            panic!("no port access on rejection")
        });
        assert!(result.is_err(), "{path}");
        assert!(matches!(
            state.native_state().lock().unwrap().phase,
            NativePhase::Vacant
        ));
    }
}

#[test]
fn byte_identical_ordinary_import_remains_ineligible_and_is_never_replaced() {
    let state = ApplicationMcp::default();
    let scratch = Scratch::new();
    let (artifacts, compositor) = scratch.ports().unwrap();
    let application = DesktopNativeApplication::new(
        state.instance_id().into(),
        imported(),
        artifacts,
        compositor,
    )
    .unwrap();
    let document = application.document_id().to_owned();
    let reservation = state.reserve_native_install().unwrap();
    state
        .install_dispatch(reservation.generation(), Box::new(application))
        .unwrap();
    assert!(!query(&state, "opt-in", "command.reproduction.opt_in").is_ok());
    assert_eq!(status(&state)["state"], "disabled");
    let before = query(&state, "serialize-before", "query.document.serialize");
    assert_eq!(
        denied(open_session(&state, request(&state), || panic!(
            "must not construct ports"
        ))),
        "duplicate_bootstrap"
    );
    let after = query(&state, "serialize-after", "query.document.serialize");
    assert_eq!(before.result(), after.result());
    let native = state.native_state();
    let guard = native.lock().unwrap();
    assert_eq!(guard.active_generation().unwrap(), reservation.generation());
    assert_eq!(guard.active().unwrap().1.document_id(), document);
    assert_eq!(guard.active().unwrap().1.content_revision(), 0);
}

#[test]
fn duplicate_inflight_and_active_sessions_cannot_restart_capture() {
    let state = ApplicationMcp::default();
    let reservation = state.reserve_native_install().unwrap();
    assert_eq!(
        denied(open_session(&state, request(&state), || panic!("reserved"))),
        "duplicate_bootstrap"
    );
    drop(reservation);
    let scratch = Scratch::new();
    let opened = open(&state, &scratch);
    assert!(dispatch(&state, set(&state, "first", json!(40))).is_ok());
    assert_eq!(
        denied(open_session(&state, request(&state), || panic!("active"))),
        "duplicate_bootstrap"
    );
    assert_eq!(status(&state)["commandCount"], 1);
    assert_eq!(
        state
            .native_state()
            .lock()
            .unwrap()
            .active_generation()
            .unwrap(),
        opened.lifecycle_generation
    );
}

#[test]
fn stale_install_and_failed_port_construction_leave_no_authority_and_allow_fresh_retry() {
    let state = ApplicationMcp::default();
    let stale = state.reserve_native_install().unwrap();
    let generation = stale.generation();
    drop(stale);
    let current = state.reserve_native_install().unwrap();
    assert_eq!(
        denied(install_session(
            &state.native_state(),
            generation,
            state.instance_id().into(),
            || panic!("stale reservation cannot create ports")
        )),
        "duplicate_bootstrap"
    );
    state
        .native_state()
        .lock()
        .unwrap()
        .require_installing_generation(current.generation())
        .unwrap();
    drop(current);
    assert_eq!(
        denied(open_session(&state, request(&state), || Err(host_error(
            "unavailable",
            "injected port failure"
        )))),
        "unavailable"
    );
    assert!(matches!(
        state.native_state().lock().unwrap().phase,
        NativePhase::Vacant
    ));
    let scratch = Scratch::new();
    assert!(open(&state, &scratch).lifecycle_generation > generation);
}

#[test]
fn release_reentry_gets_fresh_identity_and_late_old_command_cannot_enter_journal() {
    let state = ApplicationMcp::default();
    let scratch = Scratch::new();
    let old = open(&state, &scratch);
    let late = set(&state, "late-old", json!(70));
    assert!(dispatch(&state, set(&state, "first", json!(40))).is_ok());
    release(&state, false);
    let next = open(&state, &scratch);
    assert_ne!(next.document_id, old.document_id);
    assert!(next.lifecycle_generation > old.lifecycle_generation);
    assert!(!dispatch(&state, late).is_ok());
    assert_eq!(
        status(&state),
        json!({"state":"recording","reason":null,"commandCount":0,"exportable":false})
    );
    assert!(!query(&state, "empty-export", "query.reproduction.export").is_ok());
}

#[test]
fn indeterminate_cleanup_blocks_catalog_reentry_before_ports() {
    let state = ApplicationMcp::default();
    let scratch = Scratch::new();
    open(&state, &scratch);
    release(&state, true);
    assert_eq!(
        denied(open_session(&state, request(&state), || panic!(
            "cleanup fenced"
        ))),
        "duplicate_bootstrap"
    );
    assert!(state.native_state().lock().unwrap().active().is_none());
}

#[test]
fn replacement_removes_catalog_origin_and_capture_even_for_identical_import() {
    let state = ApplicationMcp::default();
    let scratch = Scratch::new();
    open(&state, &scratch);
    assert!(dispatch(&state, set(&state, "first", json!(40))).is_ok());
    {
        let native = state.native_state();
        let mut guard = native.lock().unwrap();
        let generation = guard.active_generation().unwrap();
        let desktop = guard
            .active_mut(generation)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<DesktopNativeApplication>()
            .unwrap();
        let prepared = DesktopNativeApplication::prepare_replacement(imported()).unwrap();
        desktop
            .replace_project_prepared(prepared, &mut ReplacementProgress::default(), |_| Ok(()))
            .unwrap();
    }
    assert_eq!(
        status(&state),
        json!({"state":"invalid","reason":"replaced","commandCount":0,"exportable":false})
    );
    assert!(!query(&state, "rearm", "command.reproduction.opt_in").is_ok());
    assert!(!query(&state, "export", "query.reproduction.export").is_ok());
}

#[test]
fn host_capture_preserves_failure_retry_history_transaction_and_limit_fences() {
    for case in [
        "failed_mutation",
        "changed_retry",
        "history.undo",
        "history.redo",
        "transaction.begin",
        "command_limit",
    ] {
        let state = ApplicationMcp::default();
        let scratch = Scratch::new();
        open(&state, &scratch);
        let first = set(&state, "first", json!(40));
        assert!(dispatch(&state, first.clone()).is_ok());
        let expected = match case {
            "failed_mutation" => {
                assert!(!dispatch(&state, set(&state, "bad", json!(101))).is_ok());
                "failed_mutation"
            }
            "changed_retry" => {
                let mut changed = serde_json::to_value(first).unwrap();
                changed["payload"]["value"] = json!(60);
                assert!(!dispatch(&state, serde_json::from_value(changed).unwrap()).is_ok());
                "changed_retry"
            }
            "command_limit" => {
                for index in 1..33 {
                    assert!(
                        dispatch(&state, set(&state, &format!("limit-{index}"), json!(40))).is_ok()
                    );
                }
                "command_limit"
            }
            _ => {
                let mut transition =
                    serde_json::to_value(set(&state, "transition", json!(60))).unwrap();
                transition["operation"] = json!(case);
                transition["payload"] = if case == "transaction.begin" {
                    json!({"stableTarget":{"layerUid":LAYER}})
                } else {
                    json!({})
                };
                dispatch(&state, serde_json::from_value(transition).unwrap());
                "unsupported_transition"
            }
        };
        assert_eq!(
            status(&state),
            json!({"state":"invalid","reason":expected,"commandCount":0,"exportable":false}),
            "{case}"
        );
        assert!(!query(&state, "export", "query.reproduction.export").is_ok());
        assert!(!query(&state, "rearm", "command.reproduction.opt_in").is_ok());
    }
}
