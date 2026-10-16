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
