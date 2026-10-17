//! Oracle tests of the component place and expose plans (L04-M). Every expected
//! value is a baseline row the generator executed from the verbatim `placeComponentInstance`
//! and `expose()`, never a value read back from the code under test.
use super::fixture::{convert_oracle, decode, document, layer, s1_document, uids, wire};
use super::*;
use crate::commands::object_fill_owner::component_owner::object_component_convert::plan_convert;
use serde_json::{json, Value};

const FIRST_SYMBOL: &str = "native-component:1:0";

#[test]
fn place_creates_a_locked_once_instance_like_the_baseline_rows() {
    let o = convert_oracle();
    let component = json!({"components":{"format":"nemo.native-component-set","formatVersion":1,
        "components":[{"symbolId":"sym_x","totalFrames":24,"layers":[{"layerUid":"xi"}],"objects":[]}],
        "instances":[]}});
    let doc = decode(&document(
        &[
            layer("a", "A", &[]),
            layer("b", "B", &[]),
            layer("c", "C", &[]),
        ],
        json!({"componentTables":component}),
    ));
    let plan = plan_place(&doc, 2, "sym_x", "Foreign").unwrap();
    assert_eq!(plan.created.as_deref(), Some("native-layer:2:0"));
    let after = wire(&plan.document);
    let layers = after["layers"].as_array().unwrap();
    assert_eq!(layers.len() as u64, o.get("P.layerCount").as_u64().unwrap());
    let at = o.get("P.index").as_u64().unwrap() as usize;
    let placed = &layers[at];
    assert_eq!(placed["layerUid"], "native-layer:2:0");
    assert_eq!(&placed["name"], o.get("P.name"));
    assert_eq!(&placed["locked"], o.get("P.locked"));
    assert_eq!(&placed["visible"], o.get("P.visible"));
    let keys: Vec<Value> = placed["timing"]["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["frame"].clone())
        .collect();
    assert_eq!(&Value::Array(keys), o.get("P.keyFrames"));
    let instance = &after["componentTables"]["components"]["instances"][0];
    assert_eq!(&instance["symbolId"], o.get("P.symbolId"));
    assert_eq!(&instance["symPlayMode"], o.get("P.symPlayMode"));
    assert_eq!(&instance["symSpeed"], o.get("P.symSpeed"));
    assert_eq!(&instance["symPlacedAt"], o.get("P.symPlacedAt"));
    assert_eq!(&instance["symSingleFrame"], o.get("P.symSingleFrame"));
    assert_eq!(
        instance.get("frameKeys").is_some(),
        o.get("P.hasComponentFrame").as_bool().unwrap()
    );
    assert!(
        after["componentTables"].get("storyboard").is_none(),
        "place adds no StoryBoard module"
    );
}

#[test]
fn expose_appends_the_verbatim_entries_and_keeps_absent_and_zero_defaults_apart() {
    let o = convert_oracle();
    let base = plan_convert(&decode(&s1_document(json!({}))), 1, &uids(&["ly_hand"])).unwrap();
    let entry = |i: usize| {
        let row = |field: &str| format!("E.{i}.{field}");
        let default_value = match o.get(&row("defaultPresent")).as_bool() {
            Some(true) => Presence::Present(
                serde_json::from_value::<Number>(o.get(&row("default")).clone()).unwrap(),
            ),
            _ => Presence::Missing,
        };
        ExposeEntry {
            key: o.text(&row("key")),
            label: o.text(&row("label")),
            target_stroke_id: o.text(&row("targetStrokeId")),
            target_field: o.text(&row("targetField")),
            default_value,
        }
    };
    let mut current = base.document;
    for i in 0..3 {
        let plan = plan_expose(&current, FIRST_SYMBOL, &entry(i)).unwrap();
        assert!(plan.changed);
        current = plan.document;
    }
    let props =
        wire(&current)["componentTables"]["components"]["components"][0]["exposedProps"].clone();
    let expected: Vec<Value> = (0..3)
        .map(|i| {
            let row = |field: &str| o.get(&format!("E.{i}.{field}")).clone();
            let mut want = json!({"key":row("key"),"label":row("label"),
                "targetStrokeId":row("targetStrokeId"),"targetField":row("targetField")});
            if row("defaultPresent") == json!(true) {
                want["default"] = row("default");
            }
            want
        })
        .collect();
    assert_eq!(props, Value::Array(expected));
    assert!(props[0].get("default").is_none() && props[2]["default"] == json!(0));
}

#[test]
fn exposing_the_same_entry_twice_is_a_noop_and_a_clashing_key_is_refused() {
    let o = convert_oracle();
    let base = plan_convert(&decode(&s1_document(json!({}))), 1, &uids(&["ly_hand"])).unwrap();
    let entry = ExposeEntry {
        key: o.text("E.1.key"),
        label: "Alpha".to_string(),
        target_stroke_id: "sb".to_string(),
        target_field: "opacity".to_string(),
        default_value: Presence::Present(Number::from(100_u32)),
    };
    let once = plan_expose(&base.document, FIRST_SYMBOL, &entry).unwrap();
    let again = plan_expose(&once.document, FIRST_SYMBOL, &entry).unwrap();
    assert!(!again.changed);
    assert_eq!(again.document, once.document);
    let mut clash = entry.clone();
    clash.label = "Other".to_string();
    assert_eq!(
        plan_expose(&once.document, FIRST_SYMBOL, &clash).unwrap_err(),
        SetupError::DuplicateKey(entry.key.clone())
    );
    assert_eq!(o.get("E.none"), &Value::Null);
    assert_eq!(
        plan_expose(&once.document, "sym_missing", &entry).unwrap_err(),
        SetupError::UnknownComponent("sym_missing".to_string())
    );
}
