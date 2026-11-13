//! Discovery answers a client copies from: live identity in examples, availability while
//! nothing is open, the staleness guidance and the declarations' own consistency.
use super::{instance, Instance};
use crate::{
    contract::NativeApplicationRequest,
    discovery_catalog::{self as catalog, Applicability, Selected},
    discovery_contract,
};
use serde_json::{json, Value};

const LIVE_DOC: &str = "native-object-document-9";
const FIXTURE_DOC: &str = "native-object-document-1";

fn collect<'a>(value: &'a Value, key: &str, out: &mut Vec<&'a Value>) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                if k == key {
                    out.push(v);
                }
                collect(v, key, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|v| collect(v, key, out)),
        _ => {}
    }
}

fn examples_of(contract: &Value) -> Vec<&Value> {
    let mut all = vec![&contract["example"]];
    all.extend(contract["otherExamples"].as_array().into_iter().flatten());
    all.into_iter().filter(|e| e.is_object()).collect()
}

#[test]
fn contract_examples_carry_the_open_documents_identity_not_the_fixtures() {
    let live: Instance = instance(Some("object"), LIVE_DOC);
    let selected = live.selected();
    let pin = format!("native-object:{LIVE_DOC}:7");
    let mut pins = 0;
    for operation in catalog::all_native_operations() {
        let contract = discovery_contract::contract(operation, Some(2), &selected).unwrap();
        for example in examples_of(&contract) {
            let text = example.to_string();
            assert!(!text.contains(FIXTURE_DOC), "{operation}: {text}");
            let mut found = Vec::new();
            collect(example, "documentSnapshotId", &mut found);
            for id in found {
                pins += 1;
                assert_eq!(id, &json!(pin), "{operation}");
            }
            for key in ["layerUid", "contextId"] {
                let mut found = Vec::new();
                collect(example, key, &mut found);
                for id in found {
                    let id = id.as_str().unwrap_or_default();
                    assert!(
                        id.starts_with('<') || id == "scene-root",
                        "{operation} {key} {id}"
                    );
                }
            }
            let mut revisions = Vec::new();
            collect(example, "atRevision", &mut revisions);
            assert!(revisions.iter().all(|r| **r == json!(7)), "{operation}");
        }
    }
    assert!(pins >= 20, "only {pins} pinned examples seen");
    let read =
        discovery_contract::contract("query.document.object.property.read", Some(2), &selected)
            .unwrap();
    assert_eq!(read["example"]["payload"]["documentSnapshotId"], json!(pin));
    assert_eq!(read["example"]["payload"]["atRevision"], 7);
    let edit =
        discovery_contract::contract("command.document.object.property.edit", Some(2), &selected)
            .unwrap();
    let kinds: Vec<&str> = examples_of(&edit)
        .iter()
        .filter_map(|e| e["payload"]["action"]["kind"].as_str())
        .collect();
    assert!(
        kinds.contains(&"setValue") && kinds.contains(&"setKey"),
        "{kinds:?}"
    );
}

#[test]
fn contract_examples_are_placeholders_where_there_is_no_pin_to_give() {
    let none = Selected {
        kind: None,
        v1_operations: None,
        identity: None,
    };
    let read = discovery_contract::contract("query.document.object.property.read", Some(2), &none)
        .unwrap();
    assert_eq!(read["example"]["instanceId"], "<instanceId>");
    assert_eq!(read["example"]["documentId"], "<documentId>");
    assert_eq!(
        read["example"]["payload"]["documentSnapshotId"],
        "<documentSnapshotId>"
    );
    let opacity = instance(Some("opacity"), "native-document-1");
    let read = discovery_contract::contract("query.document.opacity", Some(2), &opacity.selected())
        .unwrap();
    assert!(!read["example"].to_string().contains(FIXTURE_DOC));
}

#[test]
fn no_native_operation_is_available_while_nothing_is_open() {
    let none = Selected {
        kind: None,
        v1_operations: None,
        identity: None,
    };
    for operation in catalog::all_native_operations() {
        let summary = catalog::summary(operation, 2, &none).unwrap();
        assert_eq!(summary["available"], false, "{operation}");
        let contract = discovery_contract::contract(operation, Some(2), &none).unwrap();
        assert_eq!(
            contract["availability"]["state"], "unavailable",
            "{operation}"
        );
        if catalog::declared_applicability(operation) == Applicability::Available {
            assert_eq!(contract["availability"]["reason"], catalog::NO_DOCUMENT);
        }
    }
    // The v1 lifecycle operations still work with nothing open.
    assert_eq!(
        catalog::summary("project.open", 1, &none).unwrap()["available"],
        true
    );
    let listed = vec!["capabilities".to_owned(), "snapshot".to_owned()];
    let legacy_only = Selected {
        kind: None,
        v1_operations: Some(&listed),
        identity: None,
    };
    let refused = catalog::summary("property.set", 1, &legacy_only).unwrap();
    assert_eq!(refused["available"], false);
    assert_eq!(refused["why"], catalog::NO_DOCUMENT);
    assert!(!refused["why"].as_str().unwrap().contains("open  document"));
    assert_eq!(
        catalog::v1_applicability(None, Some(&listed), "property.set"),
        Applicability::Unavailable(catalog::NO_DOCUMENT.to_owned())
    );
}

#[test]
fn the_guide_says_the_pin_and_revision_go_stale_after_every_command_or_open() {
    let notes = catalog::guide()["notes"].to_string();
    for needle in [
        "valid only for the current revision",
        "repeat the pin step",
        "stale_revision",
        "project.open",
        "nemo_discover again",
        "requestId that is new",
    ] {
        assert!(notes.contains(needle), "notes lack {needle:?}: {notes}");
    }
    let object = catalog::guide()["kinds"]["object"]["steps"].to_string();
    assert!(object.contains("take a new one after every command or open"));
    assert!(object.contains("Then call nemo_discover again: the documentId and revision change"));
}

#[test]
fn schema_examples_are_object_requests_the_object_document_accepts() {
    let examples = discovery_contract::command_schema_examples();
    assert_eq!(examples.len(), 3);
    for example in examples {
        let operation = example["operation"].as_str().unwrap().to_owned();
        let operation = operation.as_str();
        assert_eq!(
            catalog::native_applicability("object", operation),
            Applicability::Available,
            "{operation}"
        );
        let parsed: NativeApplicationRequest = serde_json::from_value(example).unwrap();
        parsed
            .validate()
            .unwrap_or_else(|e| panic!("{operation}: {e}"));
    }
}

#[test]
fn declared_alternatives_are_for_operations_the_kind_does_not_own() {
    let mut pairs = 0;
    for operation in catalog::all_native_operations() {
        for kind in ["object", "opacity"] {
            if catalog::use_instead(kind, operation).is_some() {
                pairs += 1;
                assert!(
                    matches!(
                        catalog::native_applicability(kind, operation),
                        Applicability::OtherKind { .. }
                    ),
                    "{kind} accepts {operation} yet declares an alternative for it"
                );
            }
        }
    }
    assert!(pairs >= 16, "{pairs}");
}
