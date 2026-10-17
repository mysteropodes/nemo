use super::tests::{envelope, response, valid, P};
use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::{json, Value};

const CREATED: [&str; 2] = ["component.place", "duplicator.objects"];
const EITHER: &str = "component.convert";

#[test]
fn receipts_are_the_compact_result_or_name_the_layer_the_operation_minted() {
    let command = json!({"applied":true,"historyEntriesAdded":1});
    let noop = json!({"applied":false,"historyEntriesAdded":0});
    let minted = json!({"applied":true,"historyEntriesAdded":1,"layerUid":"native-layer:8:0"});
    for (suffix, _) in valid() {
        let operation = format!("{P}{suffix}");
        if CREATED.contains(&suffix) {
            assert!(result(&operation, &minted), "{suffix}");
            for bad in [
                command.clone(),
                noop.clone(),
                json!({"applied":false,"historyEntriesAdded":0,"layerUid":"x"}),
                json!({"applied":true,"historyEntriesAdded":1,"layerUid":""}),
                json!({"applied":true,"historyEntriesAdded":0,"layerUid":"x"}),
                Value::Null,
            ] {
                assert!(!result(&operation, &bad), "{suffix}: {bad}");
            }
            continue;
        }
        assert!(result(&operation, &command), "{suffix}");
        assert!(result(&operation, &noop), "{suffix}");
        assert_eq!(
            result(&operation, &minted),
            suffix == EITHER,
            "{suffix}: only a merge convert may name a minted layer"
        );
        for bad in [
            json!({"applied":true,"historyEntriesAdded":0}),
            json!({"applied":false,"historyEntriesAdded":1}),
            json!({"applied":1,"historyEntriesAdded":1}),
            json!({"applied":true}),
            json!({"applied":true,"historyEntriesAdded":1,"document":{}}),
            Value::Null,
            json!([]),
        ] {
            assert!(!result(&operation, &bad), "{suffix}: {bad}");
        }
    }
    assert!(!result("command.document.object.layer.create", &minted));
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
    let command = envelope("duplicator.path", valid()[5].1.clone());
    let reply = |revision: u64, instance: &str, document: &str, applied: bool| {
        response(
            json!({"apiVersion":2,"requestId":"component-1","instanceId":instance,"documentId":document,
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
        json!({"apiVersion":2,"requestId":"component-1","instanceId":"instance-A","documentId":"document-A",
        "contentRevision":7,"ok":false,"error":{"code":"stale_revision","message":"stale"}}),
    );
    assert!(
        !response_matches(&command, &refusal),
        "a success correlation needs a result"
    );
    let mut foreign = command.clone();
    foreign.operation = "command.document.object.layer.visibility.set".into();
    assert!(
        response_matches(&foreign, &refusal),
        "undeclared operations are not judged here"
    );
}

#[test]
fn the_catalog_declares_each_operation_once_under_two_owner_descriptors() {
    let family = |suffix: &str| {
        if suffix.starts_with("component.") {
            "native.object.component"
        } else {
            "native.object.duplicator"
        }
    };
    let mut declared = 0;
    for (suffix, _) in valid() {
        let operation = format!("{P}{suffix}");
        let descriptor = crate::capabilities::native_catalog()
            .capability_for_operation(&operation)
            .unwrap_or_else(|| panic!("{operation} is not declared"));
        assert_eq!(descriptor["id"], family(suffix), "{suffix}");
        assert_eq!(descriptor["authority"]["fallback"], "none");
        assert_eq!(descriptor["authority"]["adapterOwnsWritableMirror"], false);
        assert_eq!(descriptor["availability"]["state"], "available");
        assert_eq!(descriptor["resourceBoundary"]["maxRequestBytes"], 4096);
        declared += 1;
    }
    assert_eq!(declared, 6);
}

#[test]
fn every_descriptor_example_is_a_valid_request_with_a_correlated_valid_response() {
    let mut seen = 0;
    for descriptor in crate::capabilities::native_catalog().descriptors() {
        let id = descriptor["id"].as_str().unwrap();
        if id != "native.object.component" && id != "native.object.duplicator" {
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
