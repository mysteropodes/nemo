//! The object document's refusal of opacity-only operations against every capability
//! declaration, in both directions: each declared opacity-only operation is refused with
//! the declared alternative, and nothing an object document owns is refused.
use super::{opacity_document_alternatives, opacity_document_refusal};
use crate::object_codec::decode_storage_project;
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::OpacityRequest;
use serde_json::{json, Value};

fn declarations() -> Vec<Value> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../engineering/application/capabilities-v2");
    let mut all = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "json") {
            all.push(serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap());
        }
    }
    all
}

fn object_snapshot() -> ObjectSnapshot {
    let document = json!({"format":"nemo.native-object-document","formatVersion":1,
        "totalFrames":1,"layers":[{"layerUid":"A","timing":{"format":"nemo.native-layer-timing",
        "formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}],"objects":[]});
    ObjectSnapshot::new(
        "refusal-instance",
        decode_storage_project(document.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}

#[test]
fn every_declared_opacity_only_operation_is_refused_with_its_declared_alternative() {
    let snapshot = object_snapshot();
    let (mut refused, mut named) = (0, 0);
    for declaration in declarations() {
        let opacity_only = declaration["documentKinds"] == json!(["opacity"]);
        for operation in declaration["operations"].as_array().into_iter().flatten() {
            let operation = operation.as_str().unwrap();
            let refusal = opacity_document_refusal(operation);
            if !opacity_only {
                assert!(
                    refusal.is_none(),
                    "{operation} is owned by an object document"
                );
                continue;
            }
            let message = refusal.unwrap_or_else(|| panic!("{operation} is not refused"));
            let hint = match declaration["useInstead"]["object"][operation].as_str() {
                Some(other) => {
                    named += 1;
                    format!("Use {other} instead.")
                }
                None => "No equivalent exists on this document.".to_owned(),
            };
            assert!(message.ends_with(&hint), "{operation}: {message}");
            let request = OpacityRequest::query(
                "refusal-probe",
                snapshot.instance_id(),
                snapshot.document_id(),
                operation,
                json!({}),
            );
            let response = serde_json::to_value(snapshot.dispatch(request).unwrap()).unwrap();
            assert_eq!(response["ok"], false, "{operation}");
            assert_eq!(response["error"]["code"], "invalid_request", "{operation}");
            assert_eq!(response["error"]["message"], json!(message), "{operation}");
            refused += 1;
        }
    }
    assert!(
        refused >= 22 && named >= 16,
        "{refused} refused, {named} named"
    );
    // No entry the declarations do not back.
    assert_eq!(opacity_document_alternatives().len(), refused);
}
