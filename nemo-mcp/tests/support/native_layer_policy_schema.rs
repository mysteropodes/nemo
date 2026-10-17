//! Layer-policy boolean projection and preserved legacy numeric refusal controls.
use jsonschema::Validator;
use serde_json::{json, Value};
pub(super) fn check(validator: &Validator, legacy_request: &Value) {
    // Root projection must not reimpose legacy numeric value on native booleans.
    let mut policy: Value = serde_json::from_str::<Value>(include_str!(
        "../../../engineering/application/capabilities-v2/native-layer-policy.json"
    ))
    .unwrap()["examples"][0]["request"]
        .clone();
    for flag in ["locked", "shy"] {
        policy["payload"]["flag"] = json!(flag);
        for value in [true, false] {
            policy["payload"]["value"] = json!(value);
            assert!(validator.is_valid(&policy));
        }
        for invalid in [json!(0), json!(1), json!(null), json!("true")] {
            policy["payload"]["value"] = invalid;
            assert!(!validator.is_valid(&policy));
        }
    }
    let mut legacy_boolean = legacy_request.clone();
    legacy_boolean["payload"]["value"] = json!(true);
    assert!(!validator.is_valid(&legacy_boolean));
}
