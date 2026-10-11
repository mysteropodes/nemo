//! Independent full v2 bytes through actual host and private file consumers.
use crate::application_mcp::ApplicationMcp;
use crate::native_application::{admit_release_request, complete_release};
use crate::native_application_contract::NativeReleaseRequest;
use crate::native_dispatch::ReleaseAdmission;
use crate::native_object_bootstrap::{
    bootstrap_raw, reopen::bootstrap_request, save::save_request,
};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

const ORIGINAL: &str = r#"{"format":"nemo.native-object-document","formatVersion":2,"totalFrames":21,"layers":[{"layerUid":"L1"},{"layerUid":"L0"}],"objects":[{"schemaVersion":1,"family":"closed-solid-fill-cubic-path","target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":7},"layerUid":"L0","strokeId":"same"},"geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative","segments":[{"point":{"x":-10.125,"y":0},"handleIn":{"x":1,"y":0},"handleOut":{"x":2,"y":0}},{"point":{"x":10,"y":20},"handleIn":{"x":0,"y":-3},"handleOut":{"x":0,"y":0}}]},"fill":{"kind":"solid","r":0.2,"g":0.4,"b":0.6,"a":0.75}},{"schemaVersion":1,"family":"closed-solid-fill-cubic-path","target":{"contextId":"scene-root","frameScope":{"kind":"reference","frame":7},"layerUid":"L0","strokeId":"same"},"geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative","segments":[{"point":{"x":-10.125,"y":0},"handleIn":{"x":1,"y":0},"handleOut":{"x":2,"y":0}},{"point":{"x":10,"y":20},"handleIn":{"x":0,"y":-3},"handleOut":{"x":0,"y":0}}]},"fill":{"kind":"solid","r":0.2,"g":0.4,"b":0.6,"a":0.75}}],"compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,"contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{"static":[0],"track":{"keys":[]},"expression":{"enabled":false,"code":"value + 1"}}},{"layerUid":"L1","naturalPosition":1,"order":{"expression":{"enabled":true,"code":""}}}],"scopes":[{"frameScope":{"kind":"authored","frame":7},"elements":[{"layerUid":"L0","strokeId":"same","naturalPosition":0,"order":{"track":{"keys":[{"frame":-0.5,"v":[1],"hold":false,"curvePoints":null,"hIn":[0,0],"hOut":[1,0]},{"frame":0,"v":[2],"curvePoints":[{"x":0,"y":0,"tx":1},{"x":1,"y":1}]},{"frame":10,"v":[3]},{"frame":40,"v":[4],"curvePoints":[]}]}}}]}]}],"migration":"explicit-plan-required","referencePolicy":"unresolved"}}"#;
const EDITED: &str = r#"{"format":"nemo.native-object-document","formatVersion":2,"totalFrames":21,"layers":[{"layerUid":"L1"},{"layerUid":"L0"}],"objects":[{"schemaVersion":1,"family":"closed-solid-fill-cubic-path","target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":7},"layerUid":"L0","strokeId":"same"},"geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative","segments":[{"point":{"x":-10.125,"y":0},"handleIn":{"x":1,"y":0},"handleOut":{"x":2,"y":0}},{"point":{"x":10,"y":20},"handleIn":{"x":0,"y":-3},"handleOut":{"x":0,"y":0}}]},"fill":{"kind":"solid","r":0.9,"g":0.1,"b":0.3,"a":0.5}},{"schemaVersion":1,"family":"closed-solid-fill-cubic-path","target":{"contextId":"scene-root","frameScope":{"kind":"reference","frame":7},"layerUid":"L0","strokeId":"same"},"geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative","segments":[{"point":{"x":-10.125,"y":0},"handleIn":{"x":1,"y":0},"handleOut":{"x":2,"y":0}},{"point":{"x":10,"y":20},"handleIn":{"x":0,"y":-3},"handleOut":{"x":0,"y":0}}]},"fill":{"kind":"solid","r":0.2,"g":0.4,"b":0.6,"a":0.75}}],"compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,"contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{"static":[0],"track":{"keys":[]},"expression":{"enabled":false,"code":"value + 1"}}},{"layerUid":"L1","naturalPosition":1,"order":{"expression":{"enabled":true,"code":""}}}],"scopes":[{"frameScope":{"kind":"authored","frame":7},"elements":[{"layerUid":"L0","strokeId":"same","naturalPosition":0,"order":{"track":{"keys":[{"frame":-0.5,"v":[1],"hold":false,"curvePoints":null,"hIn":[0,0],"hOut":[1,0]},{"frame":0,"v":[2],"curvePoints":[{"x":0,"y":0,"tx":1},{"x":1,"y":1}]},{"frame":10,"v":[3]},{"frame":40,"v":[4],"curvePoints":[]}]}}}]}]}],"migration":"explicit-plan-required","referencePolicy":"unresolved"}}"#;

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("nemo-host-v2-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn count(&self) -> usize {
        fs::read_dir(&self.0).unwrap().count()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn install(state: &ApplicationMcp, bytes: &str) -> Value {
    serde_json::to_value(
        bootstrap_raw(
            state,
            &json!({"apiVersion":2,"requestId":"v2-install",
        "instanceId":state.instance_id(),"documentJson":bytes})
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap()
}
fn call(state: &ApplicationMcp, pins: &Value, op: &str, rev: u64, payload: Value) -> Value {
    let mut req = json!({"apiVersion":2,"requestId":format!("{op}-{rev}"),"instanceId":pins["instanceId"],
        "documentId":pins["documentId"],"operation":op,"payload":payload});
    if op.starts_with("command.") {
        req["expectedRevision"] = json!(rev);
    }
    serde_json::to_value(
        state
            .serialize_object_file_snapshot(&req.to_string())
            .unwrap(),
    )
    .unwrap()
}
fn snapshot(state: &ApplicationMcp, pins: &Value, rev: u64) -> Value {
    let result = call(
        state,
        pins,
        "query.document.object.serialize",
        rev,
        json!({"atRevision":rev}),
    );
    assert_eq!(result["ok"], true, "{result}");
    result["result"].clone()
}
fn advance(state: &ApplicationMcp, pins: &Value, rev: u64, action: &str) {
    let payload = if action == "fill.set" {
        json!({"command":"object.fill.set","stableTarget":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":7},
        "layerUid":"L0","strokeId":"same"},"fill":{"kind":"solid","r":0.9,"g":0.1,"b":0.3,"a":0.5}})
    } else {
        json!({"command":format!("object.{action}")})
    };
    let result = call(
        state,
        pins,
        &format!("command.document.object.{action}"),
        rev,
        payload,
    );
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["contentRevision"], rev + 1);
}
fn release(state: &ApplicationMcp, pins: &Value, rev: u64) {
    let request: NativeReleaseRequest =
        serde_json::from_value(json!({"apiVersion":2,"requestId":"v2-release",
        "instanceId":pins["instanceId"],"documentId":pins["documentId"],"expectedRevision":rev}))
        .unwrap();
    let native = state.native_state();
    let generation = match admit_release_request(&native, &request).unwrap() {
        ReleaseAdmission::Execute { generation } => generation,
        _ => panic!("retained release"),
    };
    complete_release(&native, generation, &request, || {
        Ok((vec![], "already_absent"))
    })
    .unwrap();
}
fn save_json(state: &ApplicationMcp, pins: &Value, rev: u64, path: &Path) -> Value {
    let acquired = snapshot(state, pins, rev);
    json!({"apiVersion":2,"requestId":format!("v2-save-{rev}"),"instanceId":pins["instanceId"],
        "documentId":pins["documentId"],"atRevision":rev,"documentSnapshotId":acquired["documentSnapshotId"],
        "destinationPath":path.to_str().unwrap()})
}
fn saved(state: &ApplicationMcp, request: &Value, root: &Path) -> Value {
    serde_json::to_value(
        save_request(
            state,
            &request.to_string(),
            |path| path.starts_with(root),
            |_, _| Ok(()),
        )
        .unwrap(),
    )
    .unwrap()
}
fn reopened(state: &ApplicationMcp, path: &Path, root: &Path) -> Value {
    let request = json!({"apiVersion":2,"requestId":"v2-file","instanceId":state.instance_id(),"sourcePath":path.to_str().unwrap()});
    serde_json::to_value(
        bootstrap_request(state, &request.to_string(), |path| path.starts_with(root)).unwrap(),
    )
    .unwrap()
}
fn identity(state: &ApplicationMcp) -> Option<(u64, String, u64)> {
    let native = state.native_state();
    let authority = native.lock().unwrap();
    authority.active().map(|(generation, owner)| {
        (
            generation,
            owner.document_id().to_owned(),
            owner.content_revision(),
        )
    })
}

#[test]
fn host_v2_actual_fill_undo_redo_save_bytes_and_production_file_reopen() {
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let pins = install(&state, ORIGINAL);
    assert_eq!(snapshot(&state, &pins, 0)["documentJson"], ORIGINAL);
    for (rev, action, expected) in [
        (1, "fill.set", EDITED),
        (2, "undo", ORIGINAL),
        (3, "redo", EDITED),
    ] {
        advance(&state, &pins, rev - 1, action);
        let acquired = snapshot(&state, &pins, rev);
        assert_eq!(acquired["documentJson"], expected);
        let path = scratch.0.join(format!("revision-{rev}.json"));
        let request = save_json(&state, &pins, rev, &path);
        let receipt = saved(&state, &request, &scratch.0);
        assert_eq!(receipt["published"], true);
        assert_eq!(receipt["cleanupPending"], false);
        assert_eq!(receipt["atRevision"], rev);
        assert_eq!(receipt["byteCount"], expected.len());
        for key in [
            "apiVersion",
            "requestId",
            "instanceId",
            "documentId",
            "documentSnapshotId",
        ] {
            assert_eq!(receipt[key], request[key]);
        }
        assert_eq!(fs::read(&path).unwrap(), expected.as_bytes());
        assert_eq!(snapshot(&state, &pins, rev), acquired);
        assert_eq!(identity(&state).unwrap().2, rev);
        let data: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let original: Value = serde_json::from_str(ORIGINAL).unwrap();
        assert_eq!(data["compositionOrder"], original["compositionOrder"]);
        assert_eq!(data["objects"][1], original["objects"][1]);
    }
    release(&state, &pins, 3);
    assert_eq!(identity(&state), None);
    let fresh = reopened(&state, &scratch.0.join("revision-3.json"), &scratch.0);
    assert_ne!(fresh["documentId"], pins["documentId"]);
    assert_eq!(fresh["contentRevision"], 0);
    assert_eq!(snapshot(&state, &fresh, 0)["documentJson"], EDITED);
    let empty = call(
        &state,
        &fresh,
        "command.document.object.undo",
        0,
        json!({"command":"object.undo"}),
    );
    assert_eq!(empty["ok"], false);
    assert_eq!(empty["error"]["code"], "unavailable");
    let old = call(
        &state,
        &pins,
        "query.document.object.serialize",
        3,
        json!({"atRevision":3}),
    );
    assert_eq!(old["ok"], false);
    assert_eq!(old["error"]["code"], "wrong_document");
    assert_eq!(snapshot(&state, &fresh, 0)["documentJson"], EDITED);
}

#[test]
fn host_v2_scoped_authored_and_reference_behavior_are_separate() {
    let mut doc: Value = serde_json::from_str(ORIGINAL).unwrap();
    let mut other = doc["objects"][0].clone();
    other["target"]["contextId"] = json!("symbol-A");
    doc["objects"].as_array_mut().unwrap().push(other);
    let mut context = doc["compositionOrder"]["contexts"][0].clone();
    context["contextId"] = json!("symbol-A");
    doc["compositionOrder"]["contexts"]
        .as_array_mut()
        .unwrap()
        .push(context);
    let state = ApplicationMcp::default();
    let pins = install(&state, &doc.to_string());
    for index in [0, 2] {
        let target = &doc["objects"][index]["target"];
        let result = call(
            &state,
            &pins,
            "query.document.object",
            0,
            json!({"atRevision":0,"stableTarget":target}),
        );
        assert_eq!(result["ok"], true, "{result}");
        assert_eq!(result["result"]["object"]["target"], *target);
    }
    let reference = &doc["objects"][1]["target"];
    let before = identity(&state);
    let read = call(
        &state,
        &pins,
        "query.document.object",
        0,
        json!({"atRevision":0,"stableTarget":reference}),
    );
    assert_eq!(read["ok"], false);
    assert_eq!(read["error"]["code"], "unavailable");
    let fill = call(
        &state,
        &pins,
        "command.document.object.fill.set",
        0,
        json!({"command":"object.fill.set",
        "stableTarget":reference,"fill":{"kind":"solid","r":1,"g":0,"b":0,"a":1}}),
    );
    assert_eq!(fill["ok"], false);
    assert_eq!(fill["error"]["code"], "unavailable");
    assert_eq!(identity(&state), before);
    let native = state.native_state();
    let authority = native.lock().unwrap();
    assert_eq!(authority.active().unwrap().1.content_revision(), 0);
}

#[cfg(test)]
#[path = "native_object_v2_refusal_tests.rs"]
mod refusals;
