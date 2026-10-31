//! F06 one-field layer policy contract; storage and history stay in native owner.
use crate::native_contract::{exact, object};
use serde_json::Value;
pub(crate) const OP: &str = "command.document.object.layer.policy.edit";
pub(super) fn request(value: &Value) -> bool {
    exact(value, &["command", "layerUid", "flag", "value"], &[]).is_some_and(|p| {
        p.get("command").and_then(Value::as_str) == Some("object.layer.policy.edit")
            && p.get("layerUid")
                .and_then(Value::as_str)
                .is_some_and(|uid| !uid.is_empty())
            && match (p.get("flag").and_then(Value::as_str), p.get("value")) {
                (Some("locked" | "shy"), Some(value)) => value.is_boolean(),
                // D37: one layer key-lock mode, null clears it.
                (Some("keyLock"), Some(value)) => {
                    value.is_null() || matches!(value.as_str(), Some("in" | "out" | "layer"))
                }
                _ => false,
            }
    })
}
pub(super) fn result(value: &Value) -> bool {
    object::visibility_result(value)
}
pub(crate) fn revision_matches(expected: Option<u64>, revision: u64, value: &Value) -> bool {
    result(value)
        && expected.and_then(|before| before.checked_add(u64::from(value["applied"] == true)))
            == Some(revision)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::NativeApplicationRequest;
    use serde_json::json;
    fn envelope(payload: Value) -> NativeApplicationRequest {
        serde_json::from_value(json!({"apiVersion":2,"requestId":"policy","instanceId":"instance","documentId":"document",
            "expectedRevision":7,"operation":OP,"payload":payload})).unwrap()
    }
    #[test]
    fn policy_shape_is_closed_and_requires_revision_without_inventing_id_grammar() {
        for flag in ["locked", "shy"] {
            for value in [false, true] {
                let p = json!({"command":"object.layer.policy.edit","layerUid":"data:opaque authored layer","flag":flag,"value":value});
                assert!(request(&p));
                assert!(envelope(p).validate().is_ok());
            }
        }
        for value in [json!(null), json!("in"), json!("out"), json!("layer")] {
            let p = json!({"command":"object.layer.policy.edit","layerUid":"layer","flag":"keyLock","value":value});
            assert!(request(&p));
            assert!(envelope(p).validate().is_ok());
        }
        for (flag, value) in [
            ("keyLock", json!(true)),
            ("keyLock", json!(false)),
            ("keyLock", json!("both")),
            ("keyLock", json!("")),
            ("keyLock", json!(0)),
            ("keyLock", json!(["in"])),
            ("keyLock", json!({"in":1})),
            ("locked", json!(null)),
            ("locked", json!("in")),
            ("shy", json!("layer")),
        ] {
            let p = json!({"command":"object.layer.policy.edit","layerUid":"layer","flag":flag,"value":value});
            assert!(!request(&p), "{p}");
            assert!(envelope(p).validate_input().is_err());
        }
        let missing = json!({"command":"object.layer.policy.edit","layerUid":"layer","flag":"keyLock"});
        assert!(!request(&missing));
        let valid = json!({"command":"object.layer.policy.edit","layerUid":"layer","flag":"locked","value":true});
        for key in ["command", "layerUid", "flag", "value"] {
            let mut p = valid.clone();
            p.as_object_mut().unwrap().remove(key);
            assert!(!request(&p));
            let mut p = valid.clone();
            p[key] = Value::Null;
            assert!(!request(&p));
        }
        for (key, value) in [
            ("extra", json!(1)),
            ("command", json!("object.fill.set")),
            ("flag", json!("solo")),
            ("flag", json!({"locked":null})),
            ("layerUid", json!("")),
            ("value", json!(1)),
            ("value", json!("true")),
        ] {
            let mut p = valid.clone();
            p[key] = value;
            assert!(!request(&p));
            assert!(envelope(p).validate_input().is_err());
        }
        assert!(!request(&json!([
            "object.layer.policy.edit",
            "layer",
            "locked",
            true
        ])));
        let mut r = envelope(valid);
        r.expected_revision = None;
        assert!(r.validate_input().is_err());
        let descriptor = crate::capabilities::native_catalog()
            .capability_for_operation(OP)
            .unwrap();
        assert_eq!(descriptor["id"], "native.layer-policy");
        assert_eq!(descriptor["authority"]["fallback"], "none");
    }
    #[test]
    fn policy_result_correlates_applied_history_and_exact_revision() {
        let changed = json!({"applied":true,"historyEntriesAdded":1});
        let noop = json!({"applied":false,"historyEntriesAdded":0});
        assert!(revision_matches(Some(7), 8, &changed));
        assert!(revision_matches(Some(7), 7, &noop));
        assert!(!revision_matches(Some(7), 7, &changed));
        assert!(!revision_matches(Some(7), 8, &noop));
        assert!(!revision_matches(None, 8, &changed));
        assert!(!revision_matches(Some(u64::MAX), 0, &changed));
        for bad in [
            json!({"applied":true,"historyEntriesAdded":0}),
            json!({"applied":false,"historyEntriesAdded":1}),
            json!({"applied":true,"historyEntriesAdded":1,"document":{}}),
        ] {
            assert!(!result(&bad));
        }
    }
}
