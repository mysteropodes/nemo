//! #2256 Q4: the closed `object.duplicator.config` payload and the `duplicator` read selector, against the shipped
//! transport schema and the contract table (literal payloads, spelled out independently of the table under test).
use super::*;
use serde_json::{json, Value};

const CONFIG: &str = "command.document.object.duplicator.config";
const READ: &str = "query.document.object.component.read";

fn config(extra: Value) -> Value {
    let mut payload = json!({"command":"object.duplicator.config","layerUid":"layer-a"});
    for (key, value) in extra.as_object().expect("object") {
        payload[key] = value.clone();
    }
    payload
}

#[test]
fn every_member_is_optional_and_bounded_by_its_kind() {
    for ok in [
        json!({}),
        json!({"mode":"grid"}),
        json!({"mode":"path","rows":1,"cols":30.5,"spacingX":-3,"count":500,"radius":0,"startAngle":-90}),
        json!({"radialOrient":false,"pathAlignTangent":true,"seed":0}),
        json!({"seed":4_294_967_295u64}),
        json!({"staggerRandom":{}}),
        json!({"staggerRandom":{"position":true,"rotation":false,"scale":true,"opacity":false,"hue":true}}),
        json!({"timeOffset":{}}),
        json!({"timeOffset":{"enabled":true,"offsetFrames":-2,"direction":"random"}}),
    ] {
        assert!(request(CONFIG, &config(ok.clone())), "{ok}");
        assert!(crate::native_contract::validate_request(CONFIG, &config(ok.clone())), "{ok}");
    }
    for bad in [
        json!({"mode":"spiral"}),
        json!({"mode":null}),
        json!({"mode":1}),
        json!({"rows":"3"}),
        json!({"rows":null}),
        json!({"count":true}),
        json!({"seed":-1}),
        json!({"seed":1.5}),
        json!({"seed":4_294_967_296u64}),
        json!({"radialOrient":1}),
        json!({"pathAlignTangent":"true"}),
        json!({"pathLayerUid":"layer-b"}),
        json!({"endAngle":90}),
        json!({"staggerRandom":[]}),
        json!({"staggerRandom":null}),
        json!({"staggerRandom":{"depth":true}}),
        json!({"staggerRandom":{"hue":1}}),
        json!({"timeOffset":{"direction":"sideways"}}),
        json!({"timeOffset":{"enabled":"yes"}}),
        json!({"timeOffset":{"offsetFrames":"1"}}),
        json!({"timeOffset":{"extra":1}}),
        json!({"timeOffset":7}),
    ] {
        assert!(!request(CONFIG, &config(bad.clone())), "{bad}");
    }
    let mut anonymous = config(json!({}));
    anonymous.as_object_mut().unwrap().remove("layerUid");
    assert!(!request(CONFIG, &anonymous));
    assert!(!request(CONFIG, &json!({"command":"object.duplicator.set","layerUid":"layer-a"})));
}

#[test]
fn the_shipped_schema_and_the_capability_descriptor_declare_the_same_closed_payload() {
    let schema: Value = serde_json::from_str(include_str!("../../engineering/application/native-transport-v2.schema.json")).unwrap();
    let payload = &schema["$defs"]["ObjectDuplicatorConfigPayload"];
    assert_eq!(payload["additionalProperties"], false);
    assert_eq!(payload["required"], json!(["command", "layerUid"]));
    let mut members: Vec<&str> = payload["properties"].as_object().unwrap().keys().map(String::as_str).collect();
    members.sort_unstable();
    let mut table: Vec<&str> = SPECS.iter().find(|row| row.0 == "duplicator.config").unwrap().3.split(' ')
        .map(|m| m.split(':').next().unwrap()).collect();
    table.push("command");
    table.sort_unstable();
    assert_eq!(members, table, "the schema and the table name the same members");
    let requests = schema["$defs"]["Request"]["oneOf"].as_array().unwrap();
    let branches = requests.iter().filter(|b| b["allOf"][1]["properties"]["operation"]["const"] == CONFIG).count();
    assert_eq!(branches, 1);
    let descriptor: Value = serde_json::from_str(include_str!("../../engineering/application/capabilities-v2/native-object-duplicator.json")).unwrap();
    assert!(descriptor["operations"].as_array().unwrap().contains(&json!(CONFIG)));
}

#[test]
fn the_duplicator_read_selector_names_one_layer_and_nothing_else() {
    for selector in [json!({"kind":"duplicator","layerUid":"layer-a"}), json!({"kind":"duplicator","layerUid":"x".repeat(128)})] {
        let payload = json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","selector":selector});
        assert!(read::request(&payload), "{payload}");
        assert!(crate::native_contract::validate_request(READ, &payload), "{payload}");
    }
    for selector in [
        json!({"kind":"duplicator"}),
        json!({"kind":"duplicator","layerUid":""}),
        json!({"kind":"duplicator","layerUid":"x".repeat(129)}),
        json!({"kind":"duplicator","layerUid":7}),
        json!({"kind":"duplicator","layerUid":"a","cursor":0}),
        json!({"kind":"duplicator","contextId":"a"}),
    ] {
        let payload = json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","selector":selector});
        assert!(!read::request(&payload), "{payload}");
    }
}

#[test]
fn a_malformed_duplicator_view_is_not_an_answer() {
    let payload = json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","selector":{"kind":"duplicator","layerUid":"ly_dup"}});
    let view = json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","kind":"duplicator","layerUid":"ly_dup",
        "mode":"grid","rows":2,"cols":3,"spacingX":150,"spacingY":150,"count":8,"radius":200,"startAngle":0,
        "radialOrient":false,"pathAlignTangent":true,"pathLayerUid":"ly_path","seed":17,"sourceMode":"shape",
        "staggerRandom":{"position":false,"rotation":false,"scale":false,"opacity":false,"hue":false},
        "timeOffset":{"enabled":false,"offsetFrames":1,"direction":"forward"}});
    assert!(read::result(&payload, &view));
    let patch = |key: &str, value: Value| {
        let mut bad = view.clone();
        bad[key] = value;
        bad
    };
    for bad in [
        patch("layerUid", json!("other")),
        patch("mode", json!("spiral")),
        patch("rows", json!("2")),
        patch("seed", json!(-1)),
        patch("radialOrient", json!(0)),
        patch("pathLayerUid", json!("")),
        patch("sourceMode", json!("pool")),
        patch("staggerRandom", json!({"position":true})),
        patch("timeOffset", json!({"enabled":false,"offsetFrames":1,"direction":"sideways"})),
        patch("kind", json!("context")),
        patch("atRevision", json!(8)),
    ] {
        assert!(!read::result(&payload, &bad), "{bad}");
    }
    let mut extra = view.clone();
    extra["extra"] = json!(true);
    assert!(!read::result(&payload, &extra));
}
