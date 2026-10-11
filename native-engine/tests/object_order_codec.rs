//! Independent v2 wire/data expectations through actual native codec/migration.
use crate::object_codec::{
    decode_ordered_project, decode_project, encode_ordered_project, encode_project,
    migrate_ordered_project, ObjectCodecErrorKind,
};
use serde_json::{json, Value};

fn plan() -> Value {
    let frozen: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-order-v2/baseline.json"
    ))
    .unwrap();
    frozen["storageExample"].clone()
}
fn record(context: &str, kind: &str, frame: u32, layer: &str) -> Value {
    json!({"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
        "target":{"contextId":context,"frameScope":{"kind":kind,"frame":frame},"layerUid":layer,"strokeId":"a"},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
        "segments":[{"point":{"x":10.125,"y":20},"handleIn":{"x":-2,"y":0},"handleOut":{"x":3.25,"y":-1}},
        {"point":{"x":40,"y":20},"handleIn":{"x":-3,"y":2},"handleOut":{"x":0,"y":6}},
        {"point":{"x":25,"y":50},"handleIn":{"x":4,"y":-2},"handleOut":{"x":-5,"y":-3}}]},
        "fill":{"kind":"solid","r":0.2,"g":0.4,"b":0.6,"a":0.75}})
}
fn v1() -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,
        "layers":[{"layerUid":"L2"},{"layerUid":"L0"},{"layerUid":"L1"}],
        "objects":[record("scene-root","authored",7,"L0"),record("scene-root","authored",7,"L1"),
        record("scene-root","authored",8,"L0"),record("scene-root","reference",7,"L0")]})
}
fn v2() -> Value {
    let mut value = v1();
    value["formatVersion"] = json!(2);
    value["compositionOrder"] = plan();
    value
}
fn bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap()
}
fn reject(value: &Value) {
    assert!(
        decode_ordered_project(&bytes(value)).is_err(),
        "accepted invalid {value}"
    );
}
fn put(value: &Value, path: &str, replacement: Value) -> Value {
    let mut result = value.clone();
    *result.pointer_mut(path).unwrap() = replacement;
    result
}

#[test]
fn order_v2_exact_presence_and_real_disk_reload_preserve_all_authored_data() {
    let mut input = v2();
    let keys = input
        .pointer_mut("/compositionOrder/contexts/0/layers/0/order/track/keys")
        .unwrap()
        .as_array_mut()
        .unwrap();
    keys.insert(
        0,
        json!({"frame":-0.5,"v":[-2.25],"hold":false,"curvePoints":null,"hIn":[0,0],"hOut":[3,4]}),
    );
    keys.push(json!({"frame":40,"v":[5],"curvePoints":[]}));
    keys[1]["curvePoints"][0]["tx"] = json!(1);
    input["compositionOrder"]["contexts"][0]["layers"][0]["order"]["static"] = json!([0]);
    input["compositionOrder"]["contexts"][0]["layers"][0]["order"]["expression"]["code"] =
        json!("value + 变量");
    let original = input.clone();
    let document = decode_ordered_project(&bytes(&input)).unwrap();
    let encoded = encode_ordered_project(&document).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&encoded).unwrap(), original);
    assert_eq!(document.total_frames(), 21);
    assert_eq!(document.layers()[0].layer_uid(), "L2");
    assert_eq!(
        document.objects()[3].target().scope_kind(),
        &crate::object_document::FrameScopeKind::Reference
    );
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("nemo-order-v2-{}-{nonce}.json", std::process::id()));
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    file.write_all(&encoded).unwrap();
    file.sync_all().unwrap();
    drop(file);
    let actual = std::fs::read(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(actual, encoded);
    let reopened = decode_ordered_project(&actual).unwrap();
    assert_eq!(reopened, document);
    assert_eq!(encode_ordered_project(&reopened).unwrap(), encoded);
    assert_eq!(input, original);
}

#[test]
fn order_v2_explicit_migration_uses_plan_not_v1_indices_and_never_changes_inputs() {
    let old = v1();
    let input = decode_project(&bytes(&old)).unwrap();
    let original = encode_project(&input).unwrap();
    let supplied = plan();
    let migrated = migrate_ordered_project(&input, &bytes(&supplied)).unwrap();
    let actual: Value =
        serde_json::from_slice(&encode_ordered_project(&migrated).unwrap()).unwrap();
    assert_eq!(actual, v2());
    assert_eq!(actual["layers"][0]["layerUid"], "L2");
    assert_eq!(
        actual["compositionOrder"]["contexts"][0]["layers"][0]["layerUid"],
        "L0"
    );
    assert_eq!(encode_project(&input).unwrap(), original);
    assert_eq!(serde_json::from_slice::<Value>(&original).unwrap(), old);
    let incomplete = put(&supplied, "/contexts/0/layers", json!([]));
    assert!(migrate_ordered_project(&input, &bytes(&incomplete)).is_err());
    assert!(migrate_ordered_project(&input, b"{}").is_err());
    assert_eq!(encode_project(&input).unwrap(), original);
    assert!(decode_project(&encode_ordered_project(&migrated).unwrap()).is_err());
    assert!(decode_ordered_project(&original).is_err());
}

#[test]
fn order_v2_complete_contexts_and_scopes_do_not_alias_v1_scene_root_or_key_frames() {
    let mut value = v2();
    value["objects"]
        .as_array_mut()
        .unwrap()
        .push(record("symbol-A", "authored", 7, "L0"));
    let mut other = value["compositionOrder"]["contexts"][0].clone();
    other["contextId"] = json!("symbol-A");
    other["scopes"] = json!([{"frameScope":{"kind":"authored","frame":7},"elements":[
        {"layerUid":"L0","strokeId":"a","naturalPosition":0,"order":{"track":{"keys":[
        {"frame":-2.5,"v":[1]},{"frame":100.5,"v":[9]}]}}}]}]);
    value["compositionOrder"]["contexts"]
        .as_array_mut()
        .unwrap()
        .push(other);
    let encoded = encode_ordered_project(&decode_ordered_project(&bytes(&value)).unwrap()).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&encoded).unwrap(), value);
    let mut legacy = value.clone();
    legacy["formatVersion"] = json!(1);
    legacy.as_object_mut().unwrap().remove("compositionOrder");
    assert!(decode_project(&bytes(&legacy)).is_err());
    reject(&put(
        &value,
        "/compositionOrder/contexts/1/scopes/0/frameScope/frame",
        json!(21),
    ));
    reject(&put(
        &value,
        "/compositionOrder/contexts/1/contextId",
        json!("scene-root"),
    ));
    reject(&put(
        &value,
        "/compositionOrder/contexts/1/scopes/0/elements/0/layerUid",
        json!("L1"),
    ));
}

const DTOS: &[&str] = &[
    "",
    "/compositionOrder",
    "/compositionOrder/contexts/0",
    "/compositionOrder/contexts/0/layers/0",
    "/compositionOrder/contexts/0/layers/0/order",
    "/compositionOrder/contexts/0/layers/0/order/track",
    "/compositionOrder/contexts/0/layers/0/order/track/keys/0",
    "/compositionOrder/contexts/0/layers/0/order/track/keys/0/curvePoints/0",
    "/compositionOrder/contexts/0/layers/0/order/expression",
    "/compositionOrder/contexts/0/scopes/0",
    "/compositionOrder/contexts/0/scopes/0/frameScope",
    "/compositionOrder/contexts/0/scopes/0/elements/0",
    "/compositionOrder/contexts/0/scopes/0/elements/0/order",
];

#[test]
fn order_v2_every_dto_rejects_unknown_positional_and_raw_duplicate_members() {
    let source = v2();
    for path in DTOS {
        let mut unknown = source.clone();
        unknown
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), json!(0));
        reject(&unknown);
        let fields = source.pointer(path).unwrap().as_object().unwrap();
        reject(&put(
            &source,
            path,
            Value::Array(fields.values().cloned().collect()),
        ));
        let (key, value) = fields.iter().next().unwrap();
        let needle = serde_json::to_string(source.pointer(path).unwrap()).unwrap();
        let duplicate = format!(
            "{},{}:{}}}",
            &needle[..needle.len() - 1],
            serde_json::to_string(key).unwrap(),
            value
        );
        let raw = String::from_utf8(bytes(&source))
            .unwrap()
            .replacen(&needle, &duplicate, 1);
        assert_ne!(raw.as_bytes(), bytes(&source));
        assert!(
            decode_ordered_project(raw.as_bytes()).is_err(),
            "duplicate at {path}"
        );
        let escaped = key
            .chars()
            .map(|c| format!("\\u{:04x}", c as u32))
            .collect::<String>();
        let duplicate = format!(
            "{},\"{}\":{}}}",
            &needle[..needle.len() - 1],
            escaped,
            value
        );
        let raw = String::from_utf8(bytes(&source))
            .unwrap()
            .replacen(&needle, &duplicate, 1);
        assert!(
            decode_ordered_project(raw.as_bytes()).is_err(),
            "decoded duplicate at {path}"
        );
    }
}

#[test]
fn order_v2_wrong_null_types_versions_and_shape_refuse_before_any_output() {
    let source = v2();
    for path in [
        "/compositionOrder",
        "/compositionOrder/contexts/0/layers/0/order/static",
        "/compositionOrder/contexts/0/layers/0/order/track",
        "/compositionOrder/contexts/0/layers/0/order/expression",
        "/compositionOrder/contexts/0/layers/0/order/track/keys/0/hold",
        "/compositionOrder/contexts/0/layers/0/order/track/keys/0/curvePoints/0/x",
    ] {
        reject(&put(&source, path, Value::Null));
        reject(&put(&source, path, json!("wrong")));
    }
    for (path, wrong) in [
        ("/formatVersion", json!(1)),
        ("/formatVersion", json!(3)),
        ("/compositionOrder/formatVersion", json!(2)),
        ("/compositionOrder/documentFormatVersion", json!(1)),
        ("/compositionOrder/format", json!("wrong")),
        ("/compositionOrder/migration", json!("inferred")),
        ("/compositionOrder/referencePolicy", json!("overlay")),
        ("/compositionOrder/contexts/0/contextId", json!("")),
        (
            "/compositionOrder/contexts/0/layers/0/naturalPosition",
            json!(-1),
        ),
        (
            "/compositionOrder/contexts/0/layers/0/naturalPosition",
            json!(0.5),
        ),
        (
            "/compositionOrder/contexts/0/layers/0/order/static",
            json!([]),
        ),
        (
            "/compositionOrder/contexts/0/layers/0/order/static",
            json!([1, 2]),
        ),
        (
            "/compositionOrder/contexts/0/layers/0/order/track/keys/0/v",
            json!([]),
        ),
        (
            "/compositionOrder/contexts/0/layers/0/order/expression/enabled",
            json!("false"),
        ),
        (
            "/compositionOrder/contexts/0/scopes/0/frameScope/kind",
            json!({"authored":null}),
        ),
        (
            "/compositionOrder/contexts/0/scopes/0/frameScope/frame",
            json!(7.5),
        ),
    ] {
        reject(&put(&source, path, wrong));
    }
    let raw =
        String::from_utf8(bytes(&source))
            .unwrap()
            .replacen("\"frame\":10", "\"frame\":1e400", 1);
    assert!(decode_ordered_project(raw.as_bytes()).is_err());
}

#[test]
fn order_v2_coverage_duplicate_gap_and_reference_plan_refusals_are_atomic() {
    let source = v2();
    for path in [
        "/compositionOrder/contexts",
        "/compositionOrder/contexts/0/layers",
        "/compositionOrder/contexts/0/scopes",
        "/compositionOrder/contexts/0/scopes/0/elements",
    ] {
        reject(&put(&source, path, json!([])));
    }
    let layers = source["compositionOrder"]["contexts"][0]["layers"]
        .as_array()
        .unwrap();
    reject(&put(
        &source,
        "/compositionOrder/contexts/0/layers",
        json!(&layers[..2]),
    )); // Empty L2 cannot disappear.
    for (path, wrong) in [
        (
            "/compositionOrder/contexts/0/layers/1/layerUid",
            json!("L0"),
        ),
        (
            "/compositionOrder/contexts/0/layers/1/naturalPosition",
            json!(0),
        ),
        (
            "/compositionOrder/contexts/0/layers/1/naturalPosition",
            json!(3),
        ),
        (
            "/compositionOrder/contexts/0/layers/1/layerUid",
            json!("outside"),
        ),
        (
            "/compositionOrder/contexts/0/scopes/1/frameScope/frame",
            json!(7),
        ),
        (
            "/compositionOrder/contexts/0/scopes/0/elements/1/layerUid",
            json!("L0"),
        ),
        (
            "/compositionOrder/contexts/0/scopes/0/elements/1/naturalPosition",
            json!(1),
        ),
        (
            "/compositionOrder/contexts/0/scopes/0/elements/1/strokeId",
            json!("missing"),
        ),
        (
            "/compositionOrder/contexts/0/scopes/0/frameScope/kind",
            json!("reference"),
        ),
    ] {
        reject(&put(&source, path, wrong));
    }
    let mut repeated = source.clone();
    repeated["compositionOrder"]["contexts"]
        .as_array_mut()
        .unwrap()
        .push(source["compositionOrder"]["contexts"][0].clone());
    reject(&repeated);
    let mut overriding = source.clone();
    overriding["compositionOrder"]["contexts"][0]["scopes"][0]["layers"] = json!([]);
    reject(&overriding);
    let mut invalid = source.clone();
    invalid["objects"][0]["fill"]["a"] = json!(2);
    reject(&invalid);
    invalid["objects"][0] = source["objects"][0].clone();
    invalid["objects"]
        .as_array_mut()
        .unwrap()
        .push(source["objects"][0].clone());
    reject(&invalid);
}

#[test]
fn order_v2_key_curve_handle_and_utf8_budget_boundaries_preserve_nonactive_metadata() {
    let source = v2();
    let root = "/compositionOrder/contexts/0/layers/0/order";
    for (tail, wrong) in [
        ("/track/keys/1/frame", json!(0)),
        ("/track/keys/1/frame", json!(-1)),
        ("/track/keys/0/v", json!([true])),
        ("/track/keys/0/curvePoints", json!([{"x":0,"y":0}])),
        ("/track/keys/0/curvePoints/0/x", json!(0.2)),
        ("/track/keys/0/curvePoints/1/x", json!(0)),
        ("/track/keys/0/curvePoints/0/tx", Value::Null),
    ] {
        let path = format!("{root}{tail}");
        let mut value = source.clone();
        if tail.ends_with("/tx") {
            value
                .pointer_mut(&format!("{root}/track/keys/0/curvePoints/0"))
                .unwrap()["tx"] = json!(1);
        }
        reject(&put(&value, &path, wrong));
    }
    let mut handles = source.clone();
    handles
        .pointer_mut(&format!("{root}/track/keys/0"))
        .unwrap()["hIn"] = json!([1]);
    reject(&handles);
    let code = "é".repeat(2048);
    let valid = put(&source, &format!("{root}/expression/code"), json!(code));
    assert_eq!(
        serde_json::from_slice::<Value>(
            &encode_ordered_project(&decode_ordered_project(&bytes(&valid)).unwrap()).unwrap()
        )
        .unwrap(),
        valid
    );
    let too_large = put(
        &valid,
        &format!("{root}/expression/code"),
        json!("é".repeat(2049)),
    );
    reject(&too_large);
    assert_eq!(
        decode_ordered_project(&bytes(&put(
            &source,
            "/compositionOrder/referencePolicy",
            json!("overlay")
        )))
        .unwrap_err()
        .kind(),
        ObjectCodecErrorKind::Unsupported
    );
}
