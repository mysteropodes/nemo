use super::tests::{envelope, response, valid, P};
use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::{json, Value};

#[test]
fn receipts_are_the_compact_command_result_and_a_duplicate_names_its_minted_layer() {
    for (suffix, _) in valid() {
        let operation = format!("{P}{suffix}");
        if suffix == "layer.duplicate" {
            assert!(result(
                &operation,
                &json!({"applied":true,"historyEntriesAdded":1,"layerUid":"native-layer:8:0"})
            ));
            for bad in [
                json!({"applied":true,"historyEntriesAdded":1}),
                json!({"applied":false,"historyEntriesAdded":0,"layerUid":"x"}),
                json!({"applied":true,"historyEntriesAdded":1,"layerUid":""}),
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
fn the_shared_result_check_routes_the_object_edit_operations_through_this_module() {
    let applied = json!({"applied":true,"historyEntriesAdded":1});
    let noop = json!({"applied":false,"historyEntriesAdded":0});
    let minted = json!({"applied":true,"historyEntriesAdded":1,"layerUid":"native-layer:8:0"});
    for (suffix, payload) in valid() {
        let operation = format!("{P}{suffix}");
        let shared = |body: &Value, receipt: &Value| {
            crate::native_contract::validate_result(&operation, "document-A", body, receipt)
        };
        if suffix == "layer.duplicate" {
            assert!(shared(&payload, &minted), "{suffix}");
            for flawed in [
                &applied,
                &noop,
                &json!({"applied":true,"historyEntriesAdded":1,"layerUid":""}),
                &json!({"applied":false,"historyEntriesAdded":0,"layerUid":"x"}),
            ] {
                assert!(!shared(&payload, flawed), "{suffix}: {flawed}");
            }
        } else {
            for accepted in [&applied, &noop] {
                assert!(shared(&payload, accepted), "{suffix}: {accepted}");
            }
            for flawed in [
                &minted,
                &json!({"applied":true,"historyEntriesAdded":0}),
                &json!({"applied":false,"historyEntriesAdded":1}),
                &json!({"applied":true,"historyEntriesAdded":1,"extra":true}),
                &Value::Null,
            ] {
                assert!(!shared(&payload, flawed), "{suffix}: {flawed}");
            }
        }
        // The request is validated first: a payload outside the closed shape never reaches the receipt check.
        let mut foreign = payload.clone();
        foreign["command"] = json!("object.layer.policy.edit");
        let receipt = if suffix == "layer.duplicate" {
            &minted
        } else {
            &applied
        };
        assert!(!shared(&foreign, receipt), "{suffix}");
    }
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
    let command = envelope("layer.rename", valid()[2].1.clone());
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
    let mut foreign = command.clone();
    foreign.operation = "command.document.object.layer.visibility.set".into();
    assert!(
        response_matches(&foreign, &refusal),
        "undeclared operations are not judged here"
    );
}

#[test]
fn the_catalog_declares_each_operation_once_under_five_owner_descriptors() {
    let family = |suffix: &str| match suffix {
        "layer.delete" | "layer.duplicate" | "layer.rename" | "layer.reorder" => {
            "native.object.layer-lifecycle"
        }
        s if s.starts_with("group.") => "native.object.group",
        "selection.delete" | "selection.duplicate" | "layer.split" | "clipboard.paste" => {
            "native.object.selection-edit"
        }
        "fill.trace" => "native.object.fill",
        _ => "native.object.boolean",
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
    assert_eq!(declared, 18);
}

#[test]
fn every_descriptor_example_is_a_valid_request_with_a_correlated_valid_response() {
    let mut seen = 0;
    for descriptor in crate::capabilities::native_catalog().descriptors() {
        let id = descriptor["id"].as_str().unwrap();
        if !id.starts_with("native.object.")
            || ![
                "layer-lifecycle",
                "group",
                "selection-edit",
                "boolean",
                "fill",
            ]
            .iter()
            .any(|f| id.ends_with(f))
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
    assert_eq!(seen, 18, "one copyable example per declared operation");
}
