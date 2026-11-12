//! Accept/reject coverage for the four project transport primitives.
use crate::contract::{command_payload_schema, Operation, WRITE_OPERATIONS};
use serde_json::{json, Value};

const PROJECT: [Operation; 4] = [
    Operation::ProjectOpen,
    Operation::ProjectNew,
    Operation::ProjectSaveAs,
    Operation::ProjectExport,
];

fn accepts(operation: Operation, payload: Value) -> bool {
    operation.check_payload(&payload).is_ok()
}

#[test]
fn examples_are_accepted_and_classified_as_fixed_writes() {
    for operation in PROJECT {
        assert!(
            accepts(operation, operation.payload_example()),
            "{operation:?}"
        );
        assert!(!operation.is_property_operation());
        assert!(!operation.is_query());
        assert!(WRITE_OPERATIONS.contains(&operation));
        for key in operation.required_payload_keys() {
            assert!(operation.payload_example().get(*key).is_some(), "{key}");
        }
    }
}

#[test]
fn accepts_optional_fields_and_boundaries() {
    assert!(accepts(Operation::ProjectOpen, json!({"path": "/a"})));
    assert!(accepts(
        Operation::ProjectNew,
        json!({"canvasW": 16384, "canvasH": 1, "fps": 240})
    ));
    assert!(accepts(
        Operation::ProjectExport,
        json!({"format": "svg", "destination": "/tmp/x.svg", "start": 3, "end": 3})
    ));
    assert!(accepts(
        Operation::ProjectExport,
        json!({"format": "lottie", "destination": "/x"})
    ));
}

#[test]
fn rejects_missing_unknown_and_mistyped_fields() {
    let bad = [
        (Operation::ProjectOpen, json!({})),
        (Operation::ProjectOpen, json!({"path": 5})),
        (Operation::ProjectOpen, json!({"path": ""})),
        (Operation::ProjectOpen, json!({"path": "relative/p.nemo"})),
        (Operation::ProjectOpen, json!({"path": "/a", "extra": 1})),
        (
            Operation::ProjectOpen,
            json!({"path": "/a", "replaceUnsaved": true}),
        ),
        (
            Operation::ProjectOpen,
            json!({"path": format!("/{}", "a".repeat(4096))}),
        ),
        (Operation::ProjectNew, json!({"canvasW": 10, "canvasH": 10})),
        (
            Operation::ProjectNew,
            json!({"canvasW": 0, "canvasH": 10, "fps": 24}),
        ),
        (
            Operation::ProjectNew,
            json!({"canvasW": 16385, "canvasH": 10, "fps": 24}),
        ),
        (
            Operation::ProjectNew,
            json!({"canvasW": 10, "canvasH": 10, "fps": 241}),
        ),
        (
            Operation::ProjectNew,
            json!({"canvasW": 10.5, "canvasH": 10, "fps": 24}),
        ),
        (
            Operation::ProjectNew,
            json!({"canvasW": -1, "canvasH": 10, "fps": 24}),
        ),
        (
            Operation::ProjectSaveAs,
            json!({"path": "/a", "replaceUnsaved": true}),
        ),
        (Operation::ProjectSaveAs, json!({"path": "p"})),
        (
            Operation::ProjectExport,
            json!({"format": "gif", "destination": "/x"}),
        ),
        (Operation::ProjectExport, json!({"format": "png"})),
        (
            Operation::ProjectExport,
            json!({"format": "png", "destination": "x"}),
        ),
        (
            Operation::ProjectExport,
            json!({"format": "png", "destination": "/x", "start": 5, "end": 4}),
        ),
        (
            Operation::ProjectExport,
            json!({"format": "png", "destination": "/x", "start": -1}),
        ),
        (
            Operation::ProjectExport,
            json!({"format": "png", "destination": "/x", "end": 9007199254740992u64}),
        ),
    ];
    for (operation, payload) in bad {
        assert!(
            !accepts(operation, payload.clone()),
            "{operation:?} {payload}"
        );
    }
}

#[test]
fn replay_refuses_recorded_project_operations() {
    let payload = json!({"request": {"operation": "project.open", "payload": {"path": "/a"}}});
    assert!(Operation::DiagnosticsReplay
        .check_payload(&payload)
        .is_err());
}

#[test]
fn command_schema_advertises_project_templates() {
    let mut generator = schemars::SchemaGenerator::default();
    let schema = serde_json::to_value(command_payload_schema(&mut generator)).unwrap();
    let description = schema["description"].as_str().unwrap();
    for operation in PROJECT {
        assert!(description.contains(operation.label()), "{operation:?}");
    }
    let branches = schema["anyOf"].as_array().unwrap();
    assert!(branches
        .iter()
        .any(|branch| branch["required"] == json!(["path"])));
    assert!(branches
        .iter()
        .any(|branch| branch["required"] == json!(["canvasW", "canvasH", "fps"])));
}

#[test]
fn a_project_only_schema_carries_only_the_project_branches() {
    let mut generator = schemars::SchemaGenerator::default();
    let schema = serde_json::to_value(crate::capability_contract::payload_schema(
        &mut generator,
        &PROJECT,
    ))
    .unwrap();
    let branches = schema["anyOf"].as_array().unwrap();
    assert_eq!(branches.len(), PROJECT.len(), "one branch per project operation, no legacy or property branch");
}

#[test]
fn a_frame_beyond_i64_is_still_structurally_an_integer() {
    // Range belongs to the document owner; admission only checks the JSON type.
    let payload = json!({"property": "opacity", "layerId": "layer-1", "value": 40, "frame": u64::MAX});
    assert!(Operation::PropertyKeySet.check_payload(&payload).is_ok());
}

#[test]
fn only_open_and_new_may_omit_the_document_pin() {
    use crate::contract::ApplicationRequest;
    let request = |operation: &str, payload: Value| -> ApplicationRequest {
        serde_json::from_value(json!({"apiVersion": 1, "requestId": "r", "instanceId": "instance-A",
            "operation": operation, "payload": payload}))
        .unwrap()
    };
    assert!(request("project.open", json!({"path": "/a.json"})).validate().is_ok());
    assert!(request("project.new", json!({"canvasW": 320, "canvasH": 180, "fps": 24})).validate().is_ok());
    assert!(request("project.saveAs", json!({"path": "/a.json"})).validate().is_err());
    assert!(request("project.export", json!({"format": "png", "destination": "/out"})).validate().is_err());
    let anonymous: ApplicationRequest = serde_json::from_value(json!({"apiVersion": 1, "requestId": "r",
        "operation": "project.open", "payload": {"path": "/a.json"}})).unwrap();
    assert!(anonymous.validate().is_err(), "every write still names its instance");
}
