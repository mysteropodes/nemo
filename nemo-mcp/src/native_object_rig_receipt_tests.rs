use super::tests::{corpus, envelope, OPERATIONS, P};
use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::{json, Value};

fn response(value: Value) -> NativeApplicationResponse {
    serde_json::from_value(value).expect("a response envelope")
}

#[test]
fn receipts_are_the_compact_command_result_and_a_creation_names_its_minted_layer() {
    for suffix in OPERATIONS {
        let operation = format!("{P}{suffix}");
        if suffix == "widget.create" {
            assert!(result(
                &operation,
                &json!({"applied":true,"historyEntriesAdded":1,"layerUid":"native-layer:8:0"})
            ));
            for bad in [
                json!({"applied":true,"historyEntriesAdded":1}),
                json!({"applied":false,"historyEntriesAdded":0,"layerUid":"x"}),
                json!({"applied":true,"historyEntriesAdded":1,"layerUid":""}),
                json!({"applied":true,"historyEntriesAdded":1,"layerUid":"x","extra":1}),
            ] {
                assert!(!result(&operation, &bad), "{bad}");
            }
            continue;
        }
        assert!(
            result(&operation, &json!({"applied":true,"historyEntriesAdded":1})),
            "{suffix}"
        );
        assert!(
            result(
                &operation,
                &json!({"applied":false,"historyEntriesAdded":0})
            ),
            "{suffix}"
        );
        for bad in [
            json!({"applied":true,"historyEntriesAdded":0}),
            json!({"applied":false,"historyEntriesAdded":1}),
            json!({"applied":true,"historyEntriesAdded":1,"layerUid":"x"}),
            json!({"applied":1,"historyEntriesAdded":1}),
            json!({"applied":true}),
            Value::Null,
            json!([]),
        ] {
            assert!(!result(&operation, &bad), "{suffix}: {bad}");
        }
    }
    assert!(!result(
        "command.document.object.layer.create",
        &json!({"applied":true,"historyEntriesAdded":1,"layerUid":"x"})
    ));
}

#[test]
fn the_revision_advances_by_exactly_the_applied_edit() {
    let changed = json!({"applied":true,"historyEntriesAdded":1});
    let noop = json!({"applied":false,"historyEntriesAdded":0});
    assert!(revision_matches(Some(7), 8, &changed));
    assert!(revision_matches(Some(7), 7, &noop));
    assert!(!revision_matches(Some(7), 7, &changed));
    assert!(!revision_matches(Some(7), 8, &noop));
    assert!(!revision_matches(None, 8, &changed));
    assert!(!revision_matches(Some(u64::MAX), 0, &changed));
}

#[test]
fn responses_must_name_the_same_owner_and_document_and_advance_exactly() {
    let corpus = corpus();
    let first = &corpus["valid"][0];
    let command = envelope(
        first["operation"].as_str().unwrap(),
        first["payload"].clone(),
    );
    let reply = |revision: u64, instance: &str, document: &str, applied: bool| {
        response(
            json!({"apiVersion":2,"requestId":"edit-1","instanceId":instance,"documentId":document,
            "contentRevision":revision,"ok":true,"result":{"applied":applied,"historyEntriesAdded":u64::from(applied)}}),
        )
    };
    assert!(response_matches(
        &command,
        &reply(8, "instance-A", "document-A", true)
    ));
    assert!(response_matches(
        &command,
        &reply(7, "instance-A", "document-A", false)
    ));
    assert!(!response_matches(
        &command,
        &reply(7, "instance-A", "document-A", true)
    ));
    assert!(!response_matches(
        &command,
        &reply(8, "instance-A", "document-A", false)
    ));
    assert!(!response_matches(
        &command,
        &reply(8, "instance-B", "document-A", true)
    ));
    assert!(!response_matches(
        &command,
        &reply(8, "instance-A", "document-B", true)
    ));
    let refusal = response(
        json!({"apiVersion":2,"requestId":"edit-1","instanceId":"instance-A","documentId":"document-A",
        "contentRevision":7,"ok":false,"error":{"code":"stale_revision","message":"stale"}}),
    );
    assert!(
        !response_matches(&command, &refusal),
        "a success correlation needs a result"
    );
    let mut foreign: NativeApplicationRequest = command.clone();
    foreign.operation = "command.document.object.layer.visibility.set".into();
    assert!(
        response_matches(&foreign, &refusal),
        "undeclared operations are not judged here"
    );
}

#[test]
fn the_catalog_declares_each_operation_once_under_three_owner_descriptors() {
    let family = |suffix: &str| match suffix {
        "image.mesh.edit" => "native.object.image-mesh",
        s if s.starts_with("widget.") => "native.object.widget",
        _ => "native.object.rig",
    };
    for suffix in OPERATIONS {
        let operation = format!("{P}{suffix}");
        let descriptor = crate::capabilities::native_catalog()
            .capability_for_operation(&operation)
            .unwrap_or_else(|| panic!("{operation} is not declared"));
        assert_eq!(descriptor["id"], family(suffix), "{suffix}");
        assert_eq!(descriptor["authority"]["fallback"], "none");
        assert_eq!(descriptor["authority"]["adapterOwnsWritableMirror"], false);
        assert_eq!(descriptor["availability"]["state"], "available");
        assert_eq!(descriptor["resourceBoundary"]["maxRequestBytes"], 4096);
    }
}

#[test]
fn every_descriptor_example_is_a_valid_request_with_a_correlated_valid_response() {
    let mut seen = 0;
    for descriptor in crate::capabilities::native_catalog().descriptors() {
        let id = descriptor["id"].as_str().unwrap();
        if !["image-mesh", "widget", "rig"]
            .iter()
            .any(|family| id == format!("native.object.{family}"))
        {
            continue;
        }
        for example in descriptor["examples"].as_array().unwrap() {
            let command: NativeApplicationRequest =
                serde_json::from_value(example["request"].clone()).unwrap();
            assert!(admitted(&command.operation), "{id}");
            assert!(command.validate().is_ok(), "{id}: {}", command.operation);
            let reply: NativeApplicationResponse =
                serde_json::from_value(example["response"].clone()).unwrap();
            assert!(
                response_matches(&command, &reply),
                "{id}: {}",
                command.operation
            );
            assert!(
                result(&command.operation, reply.result.as_ref().unwrap()),
                "{id}: {}",
                command.operation
            );
            seen += 1;
        }
    }
    assert_eq!(seen, 6, "one copyable example per declared operation");
}

#[test]
fn the_request_gate_requires_a_revision_and_the_closed_payload() {
    let corpus = corpus();
    for row in corpus["valid"].as_array().unwrap() {
        let operation = row["operation"].as_str().unwrap();
        let mut command = envelope(operation, row["payload"].clone());
        assert!(command.validate().is_ok(), "{operation}");
        command.expected_revision = None;
        assert!(
            command.validate().is_err(),
            "{operation} without expectedRevision"
        );
    }
    for row in corpus["flaws"].as_array().unwrap().iter().take(60) {
        let command = envelope(row["operation"].as_str().unwrap(), row["payload"].clone());
        assert!(
            command.validate().is_err(),
            "{}: {}",
            row["path"],
            row["why"]
        );
    }
}
