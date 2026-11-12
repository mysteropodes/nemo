//! #2358: the public MCP shapes are what the planner's request type reads; the shared bounds agree with the engine.
use super::*;
use crate::object_document::Presence;
use crate::tween_kernels::TweenRequest;

fn examples() -> Vec<Value> {
    let descriptor: Value = serde_json::from_str(include_str!(
        "../../engineering/application/capabilities-v2/native-object-tween.json"
    ))
    .unwrap();
    descriptor["examples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["request"]["payload"].clone())
        .collect()
}
/// Parse and the per-member validity the planner applies before it touches the document.
fn reads(payload: &Value) -> bool {
    let Ok(request) = serde_json::from_value::<TweenRequest>(payload.clone()) else {
        return false;
    };
    request.options.value().is_none_or(|o| o.valid())
        && request.span.value().is_none_or(|s| s.valid())
        && !matches!(
            (&request.starts, &request.selection),
            (Presence::Present(_), Presence::Present(_))
        )
}
fn with(mut payload: Value, pointer: &str, value: Value) -> Value {
    let (parent, key) = pointer.rsplit_once('/').unwrap();
    match payload.pointer_mut(parent).unwrap() {
        Value::Array(items) => items[key.parse::<usize>().unwrap()] = value,
        target => target[key] = value,
    }
    payload
}

#[test]
fn every_public_example_is_read_by_the_planner_request() {
    let payloads = examples();
    assert_eq!(payloads.len(), 4);
    for payload in &payloads {
        assert!(reads(payload), "{payload}");
    }
}

#[test]
fn the_numeric_bounds_and_closed_members_the_public_contract_states_are_the_engines() {
    let regenerate = examples()[1].clone();
    let reassign = examples()[3].clone();
    assert!(reads(&with(
        regenerate.clone(),
        "/options/resamplePoints",
        json!(2)
    )));
    assert!(reads(&with(
        regenerate.clone(),
        "/options/resamplePoints",
        json!(256)
    )));
    assert!(reads(&with(
        regenerate.clone(),
        "/options/step",
        json!(999)
    )));
    let refused = [
        with(regenerate.clone(), "/options/resamplePoints", json!(1)),
        with(regenerate.clone(), "/options/resamplePoints", json!(257)),
        with(regenerate.clone(), "/options/step", json!(0)),
        with(regenerate.clone(), "/options/step", json!(1000)),
        with(regenerate.clone(), "/options/easing/0/x", json!(-0.1)),
        with(regenerate.clone(), "/options/easing/1/x", json!(1.1)),
        with(
            regenerate.clone(),
            "/options/easing",
            json!([{"x":0.5,"y":0},{"x":0.5,"y":1}]),
        ),
        with(regenerate.clone(), "/options/easing/0/z", json!(1)),
        with(regenerate.clone(), "/options/easing/0/tx", json!("1")),
        with(
            regenerate.clone(),
            "/options/easing",
            json!(vec![json!({"x":0,"y":0}); 257]),
        ),
        with(regenerate.clone(), "/options/skipManual", json!("yes")),
        with(regenerate.clone(), "/starts", json!([-1])),
        with(regenerate.clone(), "/starts", json!([4294967296u64])),
        with(regenerate.clone(), "/layerUid", json!(5)),
        with(reassign.clone(), "/span/overrides/0/aIds", json!(["a"])),
        with(reassign.clone(), "/span/overrides/0", json!({"bId":"b"})),
        with(
            reassign.clone(),
            "/span/overrides/0",
            json!({"aIds":[],"bId":"b"}),
        ),
        with(
            reassign.clone(),
            "/span/overrides",
            json!(vec![json!({"aId":"a","bId":"b"}); 129]),
        ),
        with(
            reassign.clone(),
            "/span/manualStrokes",
            json!(vec!["m"; 129]),
        ),
        with(reassign.clone(), "/span/manualStrokes/0", json!("")),
        with(reassign.clone(), "/span/extra", json!(1)),
    ];
    for payload in refused {
        assert!(!reads(&payload), "{payload}");
    }
    let mut both = regenerate;
    both["selection"] = json!([1]);
    assert!(!reads(&both), "starts and selection are exclusive");
}
