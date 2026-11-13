use super::*;
use crate::{
    contract::{NativeApplicationRequest, Operation},
    discovery_catalog::{self as catalog, Applicability},
};

const DOC: &str = "native-object-document-1";

fn instance(kind: Option<&str>, document: &str) -> Instance {
    let status = NativeHostStatus {
        api_version: NATIVE_API_VERSION,
        request_id: "status-1".into(),
        instance_id: "11111111-1111-4111-8111-111111111111".into(),
        available: kind.is_some(),
        document_id: kind.map(|_| document.to_owned()),
        content_revision: kind.map(|_| 7),
        lifecycle_generation: kind.map(|_| 1),
        document_kind: kind.map(str::to_owned),
        pid: Some(4242),
        reason: kind.is_none().then(|| "no document".to_owned()),
    };
    status.validate().unwrap();
    Instance {
        full: json!({"instanceId": status.instance_id, "buildId": "0.7.0:abc"}),
        instance_id: status.instance_id.clone(),
        document_id: status.document_id.clone(),
        revision: status.content_revision,
        v1_operations: kind.map(|_| vec!["capabilities".to_owned(), "snapshot".to_owned()]),
        status: Some(status),
    }
}

fn fill_tokens(value: &Value) -> Value {
    match value {
        Value::String(text) if text == "<documentSnapshotId>" => {
            json!(format!("native-object:{DOC}:7"))
        }
        Value::String(text) if text == "<layerUid>" => json!("layer-1"),
        Value::Array(items) => Value::Array(items.iter().map(fill_tokens).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), fill_tokens(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[test]
fn every_start_here_example_is_a_valid_request() {
    for kind in ["object", "opacity"] {
        let live = instance(
            Some(kind),
            if kind == "object" {
                DOC
            } else {
                "native-document-1"
            },
        );
        let guide = start_here(Some(&live));
        assert_eq!(guide["documentKind"], kind);
        for step in guide["steps"].as_array().unwrap() {
            let request = fill_tokens(&step["request"]);
            let what = format!("{kind} step {}", step["id"]);
            match step["tool"].as_str().unwrap() {
                "nemo_query" => {
                    let operation: Operation =
                        serde_json::from_value(request["operation"].clone()).unwrap();
                    assert!(operation.is_query(), "{what}");
                    operation
                        .check_payload(&request["payload"])
                        .unwrap_or_else(|e| panic!("{what}: {e}"));
                }
                _ if request["apiVersion"] == 1 => {
                    let parsed: ApplicationRequest = serde_json::from_value(request).unwrap();
                    parsed.validate().unwrap_or_else(|e| panic!("{what}: {e}"));
                }
                _ => {
                    let parsed: NativeApplicationRequest = serde_json::from_value(request).unwrap();
                    parsed.validate().unwrap_or_else(|e| panic!("{what}: {e}"));
                }
            }
        }
    }
    let none = instance(None, DOC);
    for step in start_here(Some(&none))["steps"].as_array().unwrap() {
        let parsed: ApplicationRequest = serde_json::from_value(step["request"].clone()).unwrap();
        parsed
            .validate()
            .unwrap_or_else(|e| panic!("none step {}: {e}", step["id"]));
    }
}

#[test]
fn every_guide_step_is_available_for_its_kind() {
    for (kind, section) in catalog::guide()["kinds"].as_object().unwrap() {
        for step in section["steps"].as_array().unwrap() {
            let operation = step["operation"].as_str().unwrap();
            if step["apiVersion"] == 2 {
                assert_eq!(
                    catalog::native_applicability(kind, operation),
                    Applicability::Available,
                    "{kind} {operation}"
                );
            } else {
                assert!(
                    catalog::guide()["legacyOperations"]
                        .get(operation)
                        .is_some(),
                    "{operation}"
                );
            }
        }
    }
}

#[test]
fn compact_answer_for_one_instance_stays_under_twelve_thousand_characters() {
    for kind in ["object", "opacity"] {
        let live = instance(Some(kind), DOC);
        let answer = json!({"apiVersion": 1, "nativeApiVersion": NATIVE_API_VERSION, "detail": "guide",
            "instances": [instance_row(&live)], "startHere": start_here(Some(&live)), "more": more(Some(&live))});
        let size = serde_json::to_string(&answer).unwrap().len();
        assert!(size <= 12_000, "{kind} compact answer is {size} characters");
    }
}

#[test]
fn every_native_operation_declares_kinds_and_a_purpose() {
    for operation in catalog::all_native_operations() {
        let d = catalog::descriptor(operation).unwrap();
        let kinds = catalog::document_kinds(d);
        assert!(
            !kinds.is_empty() && kinds.iter().all(|k| matches!(*k, "object" | "opacity")),
            "{operation}"
        );
        assert!(
            d["purposes"][operation]
                .as_str()
                .is_some_and(|p| !p.is_empty() && p.len() <= 110),
            "{operation} purpose"
        );
    }
}

#[test]
fn use_instead_names_an_operation_the_kind_accepts() {
    let object = catalog::available_native_operations("object");
    let mut seen = 0;
    for operation in catalog::all_native_operations()
        .into_iter()
        .chain(Operation::labels().iter().copied())
    {
        if let Some(other) = catalog::use_instead("object", operation) {
            seen += 1;
            let v1_project = other.starts_with("project.");
            assert!(
                v1_project || object.contains(&other.as_str()),
                "{operation} -> {other}"
            );
        }
    }
    assert!(seen >= 20);
    let message = catalog::inapplicable_message("object", "command.document.apply").unwrap();
    assert!(
        message.contains("command.document.object.property.edit"),
        "{message}"
    );
    assert!(catalog::inapplicable_message("object", "command.document.object.undo").is_none());
}

#[test]
fn availability_follows_the_open_document_kind() {
    assert_eq!(
        catalog::native_applicability("object", "command.document.object.property.edit"),
        Applicability::Available
    );
    assert!(matches!(
        catalog::native_applicability("object", "command.document.apply"),
        Applicability::OtherKind { .. }
    ));
    assert_eq!(
        catalog::native_applicability("opacity", "command.document.apply"),
        Applicability::Available
    );
    assert!(matches!(
        catalog::native_applicability("opacity", "command.document.object.undo"),
        Applicability::OtherKind { .. }
    ));
    assert!(matches!(
        catalog::native_applicability("object", "job.export.media.begin"),
        Applicability::Unavailable(_)
    ));
    assert_eq!(
        catalog::native_applicability("object", "query.nothing"),
        Applicability::Undeclared
    );
}

#[test]
fn operation_lists_are_paged_and_filterable() {
    let live = instance(Some("object"), DOC);
    let selected = live.selected();
    let first = catalog::list_page(&selected, None, 1).unwrap();
    let pages = first["pages"].as_u64().unwrap();
    assert!(pages >= 3 && first["families"].as_array().is_some());
    let mut names = 0;
    for page in 1..=pages {
        let answer = catalog::list_page(&selected, None, page as usize).unwrap();
        let size = serde_json::to_string(&answer).unwrap().len();
        assert!(size <= 20_000, "page {page} is {size} characters");
        names += answer["operations"].as_array().unwrap().len();
    }
    assert_eq!(names as u64, first["total"].as_u64().unwrap());
    let property = catalog::list_page(&selected, Some("property"), 1).unwrap();
    assert!(property["operations"]
        .as_array()
        .unwrap()
        .iter()
        .all(|e| e["family"] == "property"));
    let apply = catalog::list_page(&selected, Some("opacity"), 1).unwrap();
    let apply = &apply["operations"][0];
    assert_eq!(apply["available"], false);
    assert_eq!(apply["useInstead"], "command.document.object.property.edit");
    assert!(catalog::list_page(&selected, Some("nonsense"), 1)
        .unwrap_err()
        .contains("families:"));
    assert!(catalog::list_page(&selected, None, pages as usize + 1).is_err());
}

#[test]
fn every_operation_has_a_contract_with_schema_codes_and_availability() {
    let live = instance(Some("object"), DOC);
    let selected = live.selected();
    for operation in catalog::all_native_operations() {
        let contract = discovery_contract::contract(operation, Some(2), &selected)
            .unwrap_or_else(|| panic!("{operation}"));
        assert_eq!(contract["operation"], operation);
        assert!(
            contract["requestSchema"].is_object(),
            "{operation} request schema"
        );
        assert!(
            contract["refusalCodes"]
                .as_array()
                .is_some_and(|c| !c.is_empty()),
            "{operation}"
        );
        assert!(contract["availability"]["state"].is_string());
        let size = serde_json::to_string(&contract).unwrap().len();
        assert!(size <= 80_000, "{operation} contract is {size} characters");
    }
    for operation in Operation::labels() {
        let contract = discovery_contract::contract(operation, Some(1), &selected)
            .unwrap_or_else(|| panic!("{operation}"));
        assert!(contract["example"].is_object(), "{operation}");
    }
    let edit =
        discovery_contract::contract("command.document.object.property.edit", None, &selected)
            .unwrap();
    assert_eq!(edit["availability"]["state"], "available");
    assert_eq!(edit["example"]["instanceId"], live.instance_id);
    assert_eq!(edit["example"]["expectedRevision"], 7);
    let apply = discovery_contract::contract("command.document.apply", None, &selected).unwrap();
    assert_eq!(apply["availability"]["state"], "not-for-open-document");
    assert_eq!(
        apply["availability"]["useInstead"],
        "command.document.object.property.edit"
    );
}

#[path = "discovery_live_tests.rs"]
mod live;
