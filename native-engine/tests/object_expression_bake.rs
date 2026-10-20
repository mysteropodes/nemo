//! Real stored expression -> native VM/providers -> one native history commit.
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::{ProjectEnvelope, ProjectMetadata};
use crate::object_expression_bake::{dispatch_object_command, prepare};
use crate::request_receipts::{DispatchErrorCode, OpacityRequest};
use serde_json::{json, Value};

fn owner(code: &str, frames: u32) -> NativeObjectHistory {
    let document = decode_storage_project(
        json!({
            "format":"nemo.native-object-document","formatVersion":1,"totalFrames":frames,
            "layers":[{"layerUid":"L0","name":"Ball"}],"objects":[],
            "propertyHolders":[{"target":{"contextId":"scene-root","layerUid":"L0"},
                "controls":[{"key":"xc_gain","name":"Gain","type":"number"}],
                "properties":[{"property":"position","static":[10,20],
                    "track":{"keys":[{"frame":-2,"v":[8,9]},{"frame":90,"v":[1,2]}]},
                    "expression":{"enabled":true,"code":code}},
                    {"property":"xc_gain","static":[7]},
                    {"property":"rotation","static":[29]}]}]
        })
        .to_string()
        .as_bytes(),
    )
    .unwrap();
    NativeObjectHistory::from_admitted_project(
        "bake-test",
        ProjectEnvelope::no_media(ProjectMetadata::new(320, 180, 24.0).unwrap(), document),
    )
    .unwrap()
}
fn drawing(owner: &NativeObjectHistory) -> Value {
    let pin = owner.acquire_snapshot(owner.content_revision()).unwrap();
    serde_json::from_slice(&encode_storage_project(pin.document()).unwrap()).unwrap()
}
fn record(owner: &NativeObjectHistory) -> Value {
    drawing(owner)["propertyHolders"][0]["properties"][0].clone()
}
fn payload() -> Value {
    json!({"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":"L0"},
        "property":"position","action":{"kind":"bake"}})
}
fn request(owner: &NativeObjectHistory, id: &str, payload: Value) -> OpacityRequest {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":id,"instanceId":owner.instance_id(),
        "documentId":owner.document_id(),"expectedRevision":owner.content_revision(),
        "operation":"command.document.object.property.edit","payload":payload}),
    )
    .unwrap()
}
fn dispatch(owner: &mut NativeObjectHistory, request: &OpacityRequest) -> Value {
    serde_json::to_value(dispatch_object_command(owner, request).unwrap().unwrap()).unwrap()
}

#[test]
fn stored_linear_expression_replaces_keys_once_and_undo_redo_restore_exact_documents() {
    let mut owner = owner("[10 + frame * 3.5, 20 - frame * 2]", 49);
    let before = drawing(&owner);
    let request = request(&owner, "bake-linear", payload());
    let result = dispatch(&mut owner, &request);
    assert_eq!(result["ok"], true);
    assert_eq!(result["result"]["historyEntriesAdded"], 1);
    assert_eq!(owner.content_revision(), 1);
    let after = drawing(&owner);
    let baked = record(&owner);
    assert_eq!(baked["expression"]["enabled"], false);
    assert_eq!(
        baked["expression"]["code"],
        "[10 + frame * 3.5, 20 - frame * 2]"
    );
    let keys = baked["track"]["keys"].as_array().unwrap();
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0]["frame"], 0);
    assert_eq!(keys[0]["v"], json!([10.0, 20.0]));
    assert_eq!(keys[1]["frame"], 48);
    assert_eq!(keys[1]["v"], json!([178.0, -76.0]));
    let target = serde_json::from_value(json!({"contextId":"scene-root","layerUid":"L0"})).unwrap();
    let pin = owner.acquire_snapshot(1).unwrap();
    for frame in 0..=48 {
        let actual = pin
            .sample_property_raw(&target, "position", f64::from(frame))
            .unwrap();
        assert!((actual[0] - (10.0 + f64::from(frame) * 3.5)).abs() < 1e-8);
        assert!((actual[1] - (20.0 - f64::from(frame) * 2.0)).abs() < 1e-8);
    }
    assert_eq!(
        after["propertyHolders"][0]["properties"]
            .as_array()
            .unwrap()[1..],
        before["propertyHolders"][0]["properties"]
            .as_array()
            .unwrap()[1..]
    );
    // Same request replays without recompiling/recommitting or losing redo.
    assert_eq!(dispatch(&mut owner, &request), result);
    assert_eq!(owner.content_revision(), 1);
    owner.undo(1).unwrap();
    assert_eq!(drawing(&owner), before);
    owner.redo(owner.content_revision()).unwrap();
    assert_eq!(drawing(&owner), after);
}

#[test]
fn constant_scalar_broadcast_uses_static_and_survives_real_project_save_fresh_reopen() {
    let mut owner = owner("3", 25);
    let req = request(&owner, "bake-constant", payload());
    assert_eq!(dispatch(&mut owner, &req)["ok"], true);
    assert_eq!(record(&owner)["static"], json!([3.0, 3.0]));
    assert!(record(&owner).get("track").is_none());
    let pin = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let bytes = owner
        .serialize_project_file(pin.content_revision(), pin.snapshot_id())
        .unwrap();
    let reopened = NativeObjectHistory::from_project_file("reopened-bake", &bytes).unwrap();
    assert_eq!(drawing(&reopened), drawing(&owner));
    assert_eq!(
        reopened.project_canvas_dimensions().unwrap(),
        Some((320, 180))
    );
}

#[test]
fn real_control_and_project_fps_providers_are_sampled_instead_of_supplied_callbacks() {
    let mut owner = owner("control('Gain') + framesToTime(frame) * comp.fps", 25);
    let req = request(&owner, "bake-providers", payload());
    assert_eq!(dispatch(&mut owner, &req)["ok"], true);
    let baked = record(&owner);
    let keys = baked["track"]["keys"].as_array().unwrap();
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0]["v"], json!([7.0, 7.0]));
    assert_eq!(keys[1]["v"], json!([31.0, 31.0]));
}

#[test]
fn stored_sine_expression_reaches_the_real_native_track_sampler_with_fewer_keys() {
    let mut owner = owner("[Math.sin(frame * Math.PI / 12) * 100, 20]", 25);
    let req = request(&owner, "bake-sine", payload());
    assert_eq!(dispatch(&mut owner, &req)["ok"], true);
    assert!(record(&owner)["track"]["keys"].as_array().unwrap().len() < 16);
    let target = serde_json::from_value(json!({"contextId":"scene-root","layerUid":"L0"})).unwrap();
    let pin = owner.acquire_snapshot(owner.content_revision()).unwrap();
    for frame in 0..=24 {
        let actual = pin
            .sample_property_raw(&target, "position", f64::from(frame))
            .unwrap();
        let expected = (f64::from(frame) * std::f64::consts::PI / 12.0).sin() * 100.0;
        assert!((actual[0] - expected).abs() <= 1.0 + 1e-8);
        assert!((actual[1] - 20.0).abs() <= 1e-4);
    }
}

#[test]
fn syntax_throw_timeout_and_nonfinite_results_leave_expression_keys_and_history_untouched() {
    for code in ["(", "throw new Error('bake failure')", "for (;;) {}", "NaN"] {
        let mut owner = owner(code, 25);
        let before = drawing(&owner);
        let req = request(&owner, "bake-failed", payload());
        assert_eq!(dispatch(&mut owner, &req)["ok"], false, "{code}");
        assert_eq!(drawing(&owner), before);
        assert_eq!(owner.content_revision(), 0);
        assert!(owner.undo(0).is_err());
    }
}

#[test]
fn cancellation_stale_and_foreign_envelopes_cannot_start_or_publish_a_bake() {
    for refusal in ["cancel", "stale", "foreign"] {
        let mut owner = owner("for (;;) {}", 25);
        let before = drawing(&owner);
        let mut value = serde_json::to_value(request(&owner, "bake-refused", payload())).unwrap();
        match refusal {
            "cancel" => value["cancelledBeforeDispatch"] = json!(true),
            "stale" => value["expectedRevision"] = json!(1),
            _ => value["documentId"] = json!("foreign-document"),
        }
        let req = serde_json::from_value(value).unwrap();
        assert_eq!(dispatch(&mut owner, &req)["ok"], false);
        assert_eq!(drawing(&owner), before);
        assert_eq!(owner.content_revision(), 0);
    }
}

#[test]
fn prepared_bake_is_sealed_to_its_original_owner_revision() {
    let mut owner = owner("frame", 25);
    let candidate = prepare(&owner, &payload().to_string().into_bytes()).unwrap();
    let mut edit = payload();
    edit["action"] = json!({"kind":"setValue","frame":0,"values":[91,92]});
    let req = request(&owner, "ordinary-edit", edit);
    assert_eq!(dispatch(&mut owner, &req)["ok"], true);
    let before = drawing(&owner);
    assert_eq!(
        owner.commit_property_edit(candidate),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(drawing(&owner), before);
}

#[test]
fn missing_expression_bad_action_and_scratch_overflow_refuse_before_publication() {
    for variant in ["missing", "extra", "oversized"] {
        let mut owner = owner("frame", if variant == "oversized" { 100_000 } else { 25 });
        let before = drawing(&owner);
        let mut p = payload();
        if variant == "missing" {
            p["property"] = json!("rotation");
        }
        if variant == "extra" {
            p["action"]["samples"] = json!([[1, 2]]);
        }
        let req = request(&owner, "bake-invalid", p);
        assert_eq!(dispatch(&mut owner, &req)["ok"], false);
        assert_eq!(drawing(&owner), before);
        assert_eq!(owner.content_revision(), 0);
    }
}
