//! Independent history-command oracle through the actual raw-installed owner.
use crate::{
    application_mcp::ApplicationMcp,
    native_application::{admit_release_request, complete_release},
    native_application_contract::NativeReleaseRequest,
    native_dispatch::{NativeObjectHost, ReleaseAdmission},
    native_object_bootstrap::bootstrap_raw,
};
use serde_json::{json, Value};

fn cases() -> Value {
    serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap()
}
fn document() -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,
        "layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],"objects":cases()["records"]})
}
fn install(state: &ApplicationMcp) -> Value {
    let raw = json!({"apiVersion":2,"requestId":"bootstrap-history",
        "instanceId":state.instance_id(),"documentJson":document().to_string()});
    serde_json::to_value(bootstrap_raw(state, &raw.to_string()).unwrap()).unwrap()
}
fn request(receipt: &Value, id: &str, revision: u64, operation: &str, payload: Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":receipt["instanceId"],
        "documentId":receipt["documentId"],"expectedRevision":revision,
        "operation":operation,"payload":payload})
}
fn history(receipt: &Value, id: &str, revision: u64, action: &str) -> Value {
    request(
        receipt,
        id,
        revision,
        &format!("command.document.object.{action}"),
        json!({"command":format!("object.{action}")}),
    )
}
fn dispatch(state: &ApplicationMcp, request: Value) -> Value {
    let native = state.native_state();
    let mut authority = native.lock().unwrap();
    let generation = authority.active_generation().unwrap();
    serde_json::to_value(
        authority
            .active_mut(generation)
            .unwrap()
            .dispatch(serde_json::from_value(request).unwrap())
            .unwrap(),
    )
    .unwrap()
}
fn content(state: &ApplicationMcp) -> Value {
    let native = state.native_state();
    let mut authority = native.lock().unwrap();
    let generation = authority.active_generation().unwrap();
    let host = authority
        .active_mut(generation)
        .unwrap()
        .as_any_mut()
        .downcast_mut::<NativeObjectHost>()
        .unwrap();
    json!({"generation":generation,"revision":host.history.content_revision(),
        "depths":host.history.history_depths(),"document":host.history
            .acquire_snapshot(host.history.content_revision()).unwrap().document()})
}

#[test]
fn history_on_actual_raw_owner_restores_full_document_and_moves_one_entry() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    let fill = request(
        &receipt,
        "fill-before-undo",
        0,
        "command.document.object.fill.set",
        cases()["command"]["request"]["payload"].clone(),
    );
    assert_eq!(dispatch(&state, fill)["ok"], true);
    let after_fill = content(&state);
    assert_eq!(after_fill["revision"], 1);
    assert_eq!(after_fill["depths"], json!([1, 0]));
    assert_eq!(
        after_fill["document"]["objects"][0],
        cases()["command"]["afterRecord"]
    );
    let undo = history(&receipt, "actual-undo", 1, "undo");
    let response = dispatch(&state, undo.clone());
    assert_eq!(
        response["ok"], true,
        "actual staged undo must move the real object's history"
    );
    assert_eq!(response["contentRevision"], 2);
    assert_eq!(
        response["result"],
        json!({"applied":true,"historyEntriesAdded":0})
    );
    let restored = content(&state);
    assert_eq!(restored["document"], document());
    assert_eq!(restored["revision"], 2);
    assert_eq!(restored["depths"], json!([0, 1]));
    assert_eq!(dispatch(&state, undo), response);
    assert_eq!(content(&state), restored);
}

fn fill(state: &ApplicationMcp, receipt: &Value) {
    let r = request(
        receipt,
        "fill",
        0,
        "command.document.object.fill.set",
        cases()["command"]["request"]["payload"].clone(),
    );
    assert_eq!(dispatch(state, r)["ok"], true);
}

#[test]
fn actual_redo_readback_and_late_retries_preserve_complete_content() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    fill(&state, &receipt);
    let undo = history(&receipt, "undo", 1, "undo");
    let undo_reply = dispatch(&state, undo.clone());
    let redo = history(&receipt, "redo", 2, "redo");
    let redo_reply = dispatch(&state, redo.clone());
    assert_eq!(redo_reply["contentRevision"], 3);
    assert_eq!(
        redo_reply["result"],
        json!({"applied":true,"historyEntriesAdded":0})
    );
    let mut expected = document();
    expected["objects"][0] = cases()["command"]["afterRecord"].clone();
    let after = content(&state);
    assert_eq!(after["document"], expected);
    assert_eq!(after["depths"], json!([1, 0]));
    let read = json!({"apiVersion":2,"requestId":"read-redo","instanceId":receipt["instanceId"],
        "documentId":receipt["documentId"],"operation":"query.document.object",
        "payload":{"atRevision":3,"stableTarget":cases()["records"][0]["target"]}});
    assert_eq!(
        dispatch(&state, read)["result"]["object"],
        cases()["command"]["afterRecord"]
    );
    assert_eq!(dispatch(&state, undo.clone()), undo_reply);
    assert_eq!(dispatch(&state, redo.clone()), redo_reply);
    assert_eq!(content(&state), after);
    let changed = history(&receipt, "undo", 3, "redo");
    assert_eq!(
        dispatch(&state, changed)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(content(&state), after);
    assert_eq!(
        dispatch(&state, history(&receipt, "later-undo", 3, "undo"))["contentRevision"],
        4
    );
    let before = content(&state);
    assert_eq!(dispatch(&state, redo), redo_reply);
    assert_eq!(content(&state), before);
    assert_eq!(before["document"], document());
}

#[test]
fn actual_history_refusals_preserve_owner_and_are_exactly_replayed() {
    for action in ["undo", "redo"] {
        for (field, value, code) in [
            ("expectedRevision", json!(0), "stale_revision"),
            (
                "payload",
                json!({"command":"object.undo","extra":true}),
                "invalid_request",
            ),
            ("documentId", json!("other-document"), "wrong_document"),
            (
                "cancelledBeforeDispatch",
                json!(true),
                "cancelled_before_dispatch",
            ),
        ] {
            let state = ApplicationMcp::default();
            let receipt = install(&state);
            fill(&state, &receipt);
            let mut r = history(&receipt, "refused", 1, action);
            r[field] = value;
            let before = content(&state);
            let response = dispatch(&state, r.clone());
            assert_eq!(response["error"]["code"], code);
            assert_eq!(dispatch(&state, r), response);
            assert_eq!(content(&state), before);
        }
        let state = ApplicationMcp::default();
        let receipt = install(&state);
        fill(&state, &receipt);
        let mut missing = history(&receipt, "missing", 1, action);
        missing.as_object_mut().unwrap().remove("expectedRevision");
        let before = content(&state);
        let reply = dispatch(&state, missing.clone());
        assert_eq!(reply["error"]["code"], "invalid_request");
        assert_eq!(dispatch(&state, missing), reply);
        assert_eq!(content(&state), before);
    }
}

#[test]
fn public_history_routes_are_registered_and_opacity_aliases_cannot_mutate() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    fill(&state, &receipt);
    let before = content(&state);
    for action in ["undo", "redo"] {
        let r = history(&receipt, "public", 1, action);
        let request: nemo_mcp::contract::NativeApplicationRequest =
            serde_json::from_value(r.clone()).unwrap();
        assert!(request.validate().is_ok());
        let mut alias = r;
        alias["operation"] = json!(format!("command.document.{action}"));
        assert_eq!(dispatch(&state, alias)["error"]["code"], "invalid_request");
        assert_eq!(content(&state), before);
    }
}

#[test]
fn actual_history_receipt_cannot_cross_terminal_release_and_new_incarnation() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    fill(&state, &receipt);
    let old = history(&receipt, "undo", 1, "undo");
    assert_eq!(dispatch(&state, old.clone())["contentRevision"], 2);
    let release = NativeReleaseRequest {
        api_version: 2,
        request_id: "release-history".into(),
        instance_id: state.instance_id().into(),
        document_id: receipt["documentId"].as_str().unwrap().into(),
        expected_revision: 2,
        cancelled_before_dispatch: false,
    };
    let generation = match admit_release_request(&state.native_state(), &release).unwrap() {
        ReleaseAdmission::Execute { generation } => generation,
        _ => panic!("unexpected retry"),
    };
    complete_release(&state.native_state(), generation, &release, || {
        Ok((vec![], "already_absent"))
    })
    .unwrap();
    assert!(state
        .native_state()
        .lock()
        .unwrap()
        .active_generation()
        .is_err());
    let fresh = install(&state);
    assert_ne!(fresh["documentId"], receipt["documentId"]);
    let before = content(&state);
    assert_eq!(dispatch(&state, old)["error"]["code"], "wrong_document");
    assert_eq!(content(&state), before);
    fill(&state, &fresh);
    assert_eq!(
        dispatch(&state, history(&fresh, "undo", 1, "undo"))["contentRevision"],
        2
    );
    assert_eq!(content(&state)["document"], document());
}

mod inactive {
    use super::*;
    use crate::native_dispatch::{NativeDispatch, PROJECT_INFO_LIMIT_BYTES};
    use native_engine::{
        object_document::FrameScopeKind, object_frame_packet::ObjectFrameSelector,
        scheduler::OutputSpec,
    };

    fn file(version: u32, width: u32) -> Vec<u8> {
        const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAGklEQVR4nGMQVDL+7xKa1sBQ3jGTYdXuM/8BN4oHrU/zhk0AAAAASUVORK5CYII=";
        let mut drawing = document();
        drawing["formatVersion"] = json!(version);
        drawing["layers"][0]["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":7}],"samples":[]});
        drawing["layers"][1]["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[],"samples":[]});
        if version == 2 {
            drawing["compositionOrder"] = json!({
                "format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
                "contexts":[{"contextId":"scene-root","layers":[
                    {"layerUid":"legacy layer:alpha","naturalPosition":0,"order":{}},
                    {"layerUid":"0","naturalPosition":1,"order":{}}],
                    "scopes":[{"frameScope":{"kind":"authored","frame":7},"elements":[
                        {"layerUid":"legacy layer:alpha","strokeId":"ink/path_A","naturalPosition":0,"order":{}}]}]}],
                "migration":"explicit-plan-required","referencePolicy":"unresolved"});
        }
        serde_json::to_vec(
            &json!({"format":"nemo.native-project-document","formatVersion":1,
            "project":{"canvasW":width,"canvasH":180,"fps":24},"drawing":drawing,
            "viewportReference":{"kind":"still-image","assetId":"media-1","name":"plate.png",
                "visible":false,"opacity":0.5,"offsetFrames":-7},
            "assets":[{"id":"media-1","mediaType":"image/png","encoding":"base64","data":PNG}]}),
        )
        .unwrap()
    }

    #[derive(Clone)]
    struct Pins(String, String, u64, String);
    impl Pins {
        fn current(host: &NativeObjectHost) -> Self {
            Self(
                host.instance_id().into(),
                host.document_id().into(),
                host.content_revision(),
                host.project_file_info().unwrap()["documentSnapshotId"]
                    .as_str()
                    .unwrap()
                    .into(),
            )
        }
        fn prepare(&self, host: &mut NativeObjectHost, bytes: &[u8]) -> Result<Value, String> {
            host.prepare_inactive_project(&self.0, &self.1, self.2, &self.3, bytes)
        }
        fn inspect(&self, host: &NativeObjectHost, token: &str) -> Result<Value, String> {
            host.inactive_project_info(&self.0, &self.1, self.2, &self.3, token)
        }
        fn close(&self, host: &mut NativeObjectHost, token: &str) -> Result<(), String> {
            host.close_inactive_project(&self.0, &self.1, self.2, &self.3, token)
        }
    }
    fn state(host: &NativeObjectHost) -> (String, String, Value, Vec<u8>) {
        let pins = Pins::current(host);
        let bytes = host
            .project_file_snapshot(pins.2, &pins.3)
            .unwrap()
            .unwrap();
        (pins.0, pins.1, host.project_file_info().unwrap(), bytes)
    }
    fn edit(host: &mut NativeObjectHost, action: &str, red: f64) {
        let mut payload = cases()["command"]["request"]["payload"].clone();
        if action == "fill.set" {
            payload["fill"]["r"] = json!(red);
        } else {
            payload = json!({"command":format!("object.{action}")});
        }
        let request = json!({"apiVersion":2,"requestId":format!("edit-{}",host.content_revision()),
            "instanceId":host.instance_id(),"documentId":host.document_id(),
            "expectedRevision":host.content_revision(),"operation":format!("command.document.object.{action}"),"payload":payload});
        let response = serde_json::to_value(
            host.dispatch(serde_json::from_value(request).unwrap())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(response["ok"], true);
    }

    #[test]
    fn cpu_inactive_host_keeps_dirty_a_and_one_b_under_current_pins() {
        for version in [1, 2] {
            let mut a =
                NativeObjectHost::from_project_file("inactive-host".into(), &file(version, 320))
                    .unwrap();
            let clean = state(&a);
            edit(&mut a, "fill.set", 0.8);
            edit(&mut a, "fill.set", 0.2);
            edit(&mut a, "undo", 0.0);
            let before = state(&a);
            assert_ne!(before.3, clean.3);
            assert_eq!(a.history.history_depths(), (1, 1));
            let pins = Pins::current(&a);
            let bytes = file(version, 640);
            for part in 0..4 {
                let mut bad = pins.clone();
                match part {
                    0 => bad.0.push('x'),
                    1 => bad.1.push('x'),
                    2 => bad.2 += 1,
                    _ => bad.3.push('x'),
                }
                assert!(bad.prepare(&mut a, &bytes).is_err());
                assert_eq!(state(&a), before);
                assert!(a.inactive_project.is_none());
            }
            for invalid in [
                b"invalid".to_vec(),
                serde_json::to_vec(&document()).unwrap(),
            ] {
                assert!(pins.prepare(&mut a, &invalid).is_err());
                assert_eq!(state(&a), before);
                assert!(a.inactive_project.is_none());
            }
            let info = pins.prepare(&mut a, &bytes).unwrap();
            let token = info["preparationToken"].as_str().unwrap();
            assert_eq!(info["canvasW"], 640);
            assert_eq!(info["reference"]["name"], "plate.png");
            assert_ne!(info["documentId"], a.document_id());
            assert_eq!(info["origin"]["documentSnapshotId"], pins.3);
            assert_eq!(state(&a), before);
            let mut detached = pins.inspect(&a, token).unwrap();
            detached["canvasW"] = json!(1);
            assert_eq!(pins.inspect(&a, token).unwrap(), info);
            assert!(pins
                .prepare(&mut a, b"invalid")
                .unwrap_err()
                .contains("already retained"));
            assert!(pins.close(&mut a, "wrong-token").is_err());
            assert!(pins.inspect(&a, "wrong-token").is_err());
            assert_eq!(state(&a), before);
            edit(&mut a, "redo", 0.0);
            let changed = state(&a);
            assert!(pins.inspect(&a, token).is_err());
            assert!(pins.close(&mut a, token).is_err());
            let fresh = Pins::current(&a);
            assert_eq!(fresh.inspect(&a, token).unwrap(), info);
            fresh.close(&mut a, token).unwrap();
            assert_eq!(state(&a), changed);
            assert!(a.inactive_project.is_none());
            assert!(fresh.close(&mut a, token).is_err());
            let next = fresh.prepare(&mut a, &bytes).unwrap();
            assert_ne!(next["preparationToken"], token);
            assert!(fresh.close(&mut a, token).is_err());
            assert_eq!(
                fresh
                    .inspect(&a, next["preparationToken"].as_str().unwrap())
                    .unwrap(),
                next
            );
        }
    }

    #[test]
    fn cpu_inactive_host_refuses_file_and_metadata_limits_before_publication() {
        let mut a =
            NativeObjectHost::from_project_file("inactive-bounds".into(), &file(1, 320)).unwrap();
        let before = state(&a);
        let pins = Pins::current(&a);
        // The real strict engine admission ceiling, without allocating decoded media.
        assert!(pins
            .prepare(&mut a, &vec![b' '; 32 * 1024 * 1024 + 1])
            .unwrap_err()
            .contains("Unavailable"));
        let mut large: Value = serde_json::from_slice(&file(1, 640)).unwrap();
        let layers = large["drawing"]["layers"].as_array_mut().unwrap();
        layers.extend((0..3).map(|i| {
            json!({"layerUid":format!("extra-layer-{i}"),"name":"x".repeat(PROJECT_INFO_LIMIT_BYTES / 3)})
        }));
        assert!(pins
            .prepare(&mut a, &serde_json::to_vec(&large).unwrap())
            .unwrap_err()
            .contains("metadata exceeds UI limit"));
        assert!(a.inactive_project.is_none());
        assert_eq!(state(&a), before);
        assert!(pins.prepare(&mut a, &file(1, 640)).is_ok());
    }

    #[test]
    fn cpu_inactive_host_release_and_replacement_drop_b_without_stale_success() {
        let mut a =
            NativeObjectHost::from_project_file("inactive-release".into(), &file(1, 320)).unwrap();
        edit(&mut a, "fill.set", 0.8);
        let before = state(&a);
        let pins = Pins::current(&a);
        let info = pins.prepare(&mut a, &file(2, 640)).unwrap();
        let token = info["preparationToken"].as_str().unwrap();
        let denied = a
            .prepare_project_replacement(&pins.3, &file(1, 800))
            .unwrap();
        assert!(a.commit_project_replacement(denied).is_err());
        assert_eq!(pins.inspect(&a, token).unwrap(), info);
        assert_eq!(state(&a), before);
        let replacement = a
            .prepare_project_replacement(&pins.3, &file(1, 800))
            .unwrap();
        assert!(a.release_project().unwrap().reconciliation_complete());
        assert!(a.inactive_project.is_none());
        assert!(pins.prepare(&mut a, &file(1, 640)).is_err());
        assert!(pins.inspect(&a, token).is_err());
        assert!(pins.close(&mut a, token).is_err());
        assert_eq!(state(&a), before);
        assert!(a.release_project().unwrap().reconciliation_complete());
        a.commit_project_replacement(replacement).unwrap();
        assert!(a.inactive_project.is_none());
        assert_ne!(a.document_id(), pins.1);
        assert!(pins.prepare(&mut a, &file(1, 640)).is_err());
        let fresh = Pins::current(&a);
        assert!(fresh.prepare(&mut a, &file(1, 640)).is_ok());
        assert!(fresh.close(&mut a, token).is_err());
    }

    #[test]
    fn cpu_inactive_prepare_and_close_preserve_borrowed_preview_preparation() {
        let mut a =
            NativeObjectHost::from_project_file("inactive-preview".into(), &file(2, 320)).unwrap();
        let pins = Pins::current(&a);
        let selector = ObjectFrameSelector {
            instance_id: &pins.0,
            document_id: &pins.1,
            content_revision: pins.2,
            document_snapshot_id: &pins.3,
            context_id: "scene-root",
            scope_kind: FrameScopeKind::Authored,
            frame: 7,
        };
        let output = OutputSpec::new("frame", "rgba8", 320, 180, "srgb", "straight").unwrap();
        a.with_prepared_object_preview(&selector, output.clone(), |a| {
            let info = pins.prepare(a, &file(1, 640))?;
            assert!(a
                .with_prepared_object_preview(&selector, output.clone(), |_| Ok(()))
                .unwrap_err()
                .contains("already borrowed"));
            pins.close(a, info["preparationToken"].as_str().unwrap())?;
            assert!(a
                .with_prepared_object_preview(&selector, output.clone(), |_| Ok(()))
                .unwrap_err()
                .contains("already borrowed"));
            Ok(())
        })
        .unwrap();
        assert!(a
            .with_prepared_object_preview(&selector, output, |_| Ok(()))
            .is_ok());
    }
}
