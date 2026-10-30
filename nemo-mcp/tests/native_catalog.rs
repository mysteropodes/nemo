use nemo_mcp::capabilities::NativeCapabilityCatalog;
use serde_json::{json, Value};

#[test]
fn native_catalog_rejects_malformed_later_sources_without_partial_catalog() {
    let first = r#"{"schemaVersion":2,"apiVersion":2,"id":"first","operations":["query.first"]}"#;
    let second = json!({"schemaVersion":2,"apiVersion":2,"id":"second","operations":["query.second"],
        "availability":{"state":"unavailable","reason":"staged fixture"}});
    for (field, replacement, error) in [
        (
            "schemaVersion",
            Some(json!(1)),
            "native capability source 1 must declare schemaVersion/apiVersion 2",
        ),
        (
            "apiVersion",
            Some(json!(1)),
            "native capability source 1 must declare schemaVersion/apiVersion 2",
        ),
        (
            "schemaVersion",
            None,
            "native capability source 1 must declare schemaVersion/apiVersion 2",
        ),
        ("id", None, "native capability source 1 has no id"),
        (
            "id",
            Some(json!("")),
            "native capability source 1 has no id",
        ),
        ("id", Some(json!(7)), "native capability source 1 has no id"),
        (
            "operations",
            None,
            "native capability second has no operations",
        ),
        (
            "operations",
            Some(json!([])),
            "native capability second has no operations",
        ),
        (
            "operations",
            Some(json!({})),
            "native capability second has no operations",
        ),
        (
            "operations",
            Some(json!([7])),
            "native capability second has a non-string operation",
        ),
        (
            "operations",
            Some(json!([""])),
            "native capability second has a non-string operation",
        ),
    ] {
        let mut invalid = second.clone();
        match replacement {
            Some(value) => {
                invalid[field] = value;
            }
            None => {
                invalid.as_object_mut().unwrap().remove(field);
            }
        }
        assert_eq!(
            NativeCapabilityCatalog::from_sources(&[first, &invalid.to_string()]).unwrap_err(),
            error
        );
    }
    for invalid in ["null", "[]", "false"] {
        assert_eq!(
            NativeCapabilityCatalog::from_sources(&[first, invalid]).unwrap_err(),
            "native capability source 1 must be a JSON object"
        );
    }
    assert!(NativeCapabilityCatalog::from_sources(&[first, "{"])
        .unwrap_err()
        .starts_with("native capability source 1:"));
    let catalog = NativeCapabilityCatalog::from_sources(&[first, &second.to_string()]).unwrap();
    assert_eq!(
        catalog.descriptors(),
        &[
            serde_json::from_str::<Value>(first).unwrap(),
            second.clone()
        ]
    );
    assert_eq!(
        catalog.capability_for_operation("query.second"),
        Some(&second)
    );
    assert!(catalog
        .capability_for_operation("query.not_registered")
        .is_none());
}

#[test]
fn public_catalog_entry_rejects_duplicate_ids_and_operation_ownership() {
    let first = r#"{"schemaVersion":2,"apiVersion":2,"id":"first","operations":["query.first"]}"#;
    for (second, reason) in [
        (
            json!({"schemaVersion":2,"apiVersion":2,"id":"first","operations":["query.second"]}),
            "duplicate native capability id first",
        ),
        (
            json!({"schemaVersion":2,"apiVersion":2,"id":"second","operations":["query.first"]}),
            "duplicate native operation query.first",
        ),
        (
            json!({"schemaVersion":2,"apiVersion":2,"id":"second","operations":["query.second","query.second"]}),
            "duplicate native operation query.second",
        ),
    ] {
        let source = second.to_string();
        assert_eq!(
            NativeCapabilityCatalog::from_sources(&[first, &source]).unwrap_err(),
            reason
        );
    }
}

#[test]
fn lifecycle_generation_bound_operations_say_where_a_client_reads_it() {
    let catalog = serde_json::to_value(nemo_mcp::capabilities::native_catalog().descriptors())
        .unwrap();
    for id in [
        "native.object.import",
        "native.object.asset",
        "native.object.brush-bitmap-stroke",
        "native.object.font-assets",
    ] {
        let descriptor = catalog
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["id"] == id)
            .unwrap_or_else(|| panic!("{id} missing"));
        assert!(
            descriptor["semantics"]
                .as_str()
                .is_some_and(|text| text.contains("nativeHostStatus.lifecycleGeneration")),
            "{id} must tell clients to read lifecycleGeneration from nemo_discover"
        );
    }
}
