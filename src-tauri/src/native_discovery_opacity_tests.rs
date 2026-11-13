//! The opacity counterpart of `native_discovery_availability_tests.rs`: discovery's report
//! for an opacity document, held against the real opacity owner (#1049).
use super::{fixture, ApplicationMcp, NativeApplicationRequest};
use nemo_mcp::{
    contract::NativeStatusRequest,
    discovery::{start_here, Instance},
    discovery_catalog::{self as catalog, Applicability},
};
use serde_json::{json, Value};

const FOREIGN: &str = "native object operation has no active object owner";
const LAYER: &str = "r08_curve_layer";

fn send(state: &ApplicationMcp, request: Value) -> Result<Value, String> {
    let request: NativeApplicationRequest =
        serde_json::from_value(request).map_err(|e| e.to_string())?;
    state
        .dispatch_native(request)
        .map(|r| serde_json::to_value(r).unwrap())
}

fn live_instance(state: &ApplicationMcp) -> Instance {
    let status = state
        .native_status(NativeStatusRequest {
            api_version: 2,
            request_id: "status-opacity".into(),
            instance_id: state.instance_id().into(),
        })
        .unwrap();
    assert_eq!(status.document_kind.as_deref(), Some("opacity"));
    Instance {
        full: json!({"instanceId": state.instance_id()}),
        instance_id: state.instance_id().into(),
        document_id: status.document_id.clone(),
        revision: status.content_revision,
        v1_operations: None,
        status: Some(status),
    }
}

fn revision(response: &Value) -> u64 {
    response["contentRevision"]
        .as_u64()
        .expect("response carries the revision")
}

#[test]
fn the_opacity_guide_steps_run_on_an_opacity_document() {
    let (state, _) = fixture();
    let guide = start_here(Some(&live_instance(&state)));
    let mut current = 0;
    let mut ran = 0;
    for step in guide["steps"].as_array().unwrap() {
        let mut request = step["request"].clone();
        if request["apiVersion"] != 2 {
            continue; // project.* is the app's lifecycle, covered elsewhere
        }
        let text = request.to_string().replace("<layerUid>", LAYER);
        request = serde_json::from_str(&text).unwrap();
        if request.get("expectedRevision").is_some() {
            request["expectedRevision"] = json!(current);
        }
        let response = send(&state, request).unwrap_or_else(|e| panic!("{}: {e}", step["id"]));
        assert_eq!(response["ok"], true, "{}: {response}", step["id"]);
        current = revision(&response);
        ran += 1;
    }
    assert!(ran >= 4, "only {ran} opacity steps ran");
    assert!(
        current >= 3,
        "set, undo and redo each advance the revision: {current}"
    );
}

#[test]
fn no_operation_declared_for_object_documents_succeeds_on_an_opacity_document() {
    let (state, document) = fixture();
    let (mut checked, mut foreign) = (0, 0);
    for (index, operation) in catalog::all_native_operations().into_iter().enumerate() {
        if !matches!(
            catalog::native_applicability("opacity", operation),
            Applicability::OtherKind { .. }
        ) {
            continue;
        }
        let example = catalog::descriptor(operation).and_then(|d| {
            d["examples"]
                .as_array()?
                .iter()
                .find(|e| e["request"]["operation"] == operation)
                .map(|e| e["request"].clone())
        });
        let mut request = example.unwrap_or_else(|| json!({"payload": {}}));
        request["apiVersion"] = json!(2);
        request["requestId"] = json!(format!("foreign-{index}"));
        request["instanceId"] = json!(state.instance_id());
        request["documentId"] = json!(document);
        request["operation"] = json!(operation);
        if request.get("expectedRevision").is_some() {
            request["expectedRevision"] = json!(0);
        }
        if let Ok(response) = send(&state, request) {
            assert_eq!(
                response["ok"], false,
                "{operation} succeeded on an opacity document"
            );
            if response["error"]["message"] == FOREIGN {
                assert_eq!(response["error"]["code"], "unavailable", "{operation}");
                foreign += 1;
            }
        }
        checked += 1;
    }
    assert!(
        checked > 100 && foreign > 100,
        "{checked} checked, {foreign} refused by kind"
    );
}

#[test]
fn operations_reported_available_for_an_opacity_document_are_not_refused_as_foreign() {
    let (state, document) = fixture();
    let available = catalog::available_native_operations("opacity");
    assert_eq!(available.len(), 22, "{available:?}");
    let mut answered = 0;
    for (index, operation) in available.iter().enumerate() {
        let mut request = json!({"apiVersion": 2, "requestId": format!("own-{index}"),
            "instanceId": state.instance_id(), "documentId": document, "operation": operation, "payload": {}});
        if operation.starts_with("command.") || operation.starts_with("transaction.begin") {
            request["expectedRevision"] = json!(0);
        }
        let Ok(response) = send(&state, request) else {
            continue;
        };
        let declared: Vec<&str> = ["commonErrorCodes", "domainErrorCodes"]
            .iter()
            .flat_map(|k| {
                catalog::descriptor(operation).unwrap()[*k]
                    .as_array()
                    .into_iter()
                    .flatten()
            })
            .filter_map(Value::as_str)
            .collect();
        if response["ok"] == true {
            answered += 1;
            continue;
        }
        assert_ne!(response["error"]["message"], FOREIGN, "{operation}");
        let code = response["error"]["code"].as_str().unwrap();
        assert!(
            declared.is_empty() || declared.contains(&code),
            "{operation}: {code}"
        );
    }
    assert!(answered >= 4, "only {answered} answered");
}
