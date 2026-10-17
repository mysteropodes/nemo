//! Strict payload tests for `object.frames.repeat`: the command admits exactly its six fields, builds the typed request
//! the transaction runs, round-trips through the one wire definition, and every malformed input is `InvalidRequest`.
use super::*;
use serde_json::{json, Value};

fn good() -> Value {
    json!({"command":"object.frames.repeat","fromLayerUid":"L0","toLayerUid":"L2",
        "fromFrame":2,"toFrame":5,"times":3})
}
fn parse(value: &Value) -> Result<RepeatRequest, DispatchErrorCode> {
    parse_repeat_payload(value.to_string().as_bytes())
}
fn with(field: &str, value: Value) -> Value {
    let mut payload = good();
    payload[field] = value;
    payload
}
fn without(field: &str) -> Value {
    let mut payload = good();
    payload.as_object_mut().unwrap().remove(field);
    payload
}

#[test]
fn the_command_builds_its_request_and_the_wire_form_round_trips() {
    let expected = RepeatRequest {
        from_layer: "L0".into(),
        to_layer: "L2".into(),
        from_frame: 2,
        to_frame: 5,
        times: 3,
    };
    assert_eq!(parse(&good()), Ok(expected.clone()));
    assert_eq!(repeat_payload(&expected), good());
    // the raw count is carried as given: zero, negative and above-fifty counts are clamped by the transaction
    for times in [0, -3, 50, 51, 1_000_000] {
        assert_eq!(parse(&with("times", json!(times))).unwrap().times, times);
    }
}

#[test]
fn malformed_payloads_refuse_as_invalid_request() {
    let invalid = Err(DispatchErrorCode::InvalidRequest);
    for field in [
        "command",
        "fromLayerUid",
        "toLayerUid",
        "fromFrame",
        "toFrame",
        "times",
    ] {
        assert_eq!(parse(&without(field)), invalid, "missing {field}");
        assert_eq!(parse(&with(field, Value::Null)), invalid, "null {field}");
    }
    let cases = [
        with("extra", json!(true)),
        with("command", json!("object.frames.insert")),
        with("command", json!("object.frames.clipboard.duplicate")),
        with("fromLayerUid", json!("")),
        with("toLayerUid", json!("")),
        with("fromLayerUid", json!(7)),
        with("fromFrame", json!("2")),
        with("fromFrame", json!(-1)),
        with("fromFrame", json!(2.5)),
        with("toFrame", json!(4_294_967_296_u64)),
        with("times", json!("2")),
        with("times", json!(2.5)),
        with("times", json!(true)),
        json!([]),
        json!("object.frames.repeat"),
        Value::Null,
    ];
    for (index, value) in cases.iter().enumerate() {
        assert_eq!(parse(value), invalid, "case {index}: {value}");
    }
    let duplicate = br#"{"command":"object.frames.repeat","fromLayerUid":"L0","toLayerUid":"L2",
        "fromFrame":2,"toFrame":5,"times":3,"times":4}"#;
    assert_eq!(parse_repeat_payload(duplicate), invalid, "duplicate member");
    assert_eq!(parse_repeat_payload(b"{"), invalid, "not JSON");
    assert_eq!(parse_repeat_payload(b""), invalid, "empty");
}

#[test]
fn the_existing_message_bound_applies_unchanged() {
    let long = "L".repeat(5000);
    assert_eq!(
        parse(&with("fromLayerUid", json!(long))),
        Err(DispatchErrorCode::InvalidRequest),
        "a payload above 4096 bytes refuses before it is parsed"
    );
    let near = "L".repeat(3900);
    assert!(
        parse(&with("fromLayerUid", json!(near))).is_ok(),
        "a payload under the bound is parsed"
    );
}
