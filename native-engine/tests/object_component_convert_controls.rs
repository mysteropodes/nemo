//! Negative and mutation controls of the component convert plans (L04-M): the
//! oracle check must notice every single deviation, and every refusal must be the
//! typed reason of its own rule, leaving the input document untouched.
use super::fixture::{
    convert_oracle, decode, document, layer, s1_document, s2_document, uids, wire,
};
use super::verify::verify;
use super::*;
use crate::commands::object_fill_owner::component_owner::object_component_place::plan_place;
use serde_json::{json, Value};

const S1_SYMBOL: &str = "native-component:1:0";
const S2_SYMBOL: &str = "native-component:5:0";

fn s1_after() -> Value {
    let doc = decode(&s1_document(json!({})));
    wire(&plan_convert(&doc, 1, &uids(&["ly_hand"])).unwrap().document)
}
fn s2_after() -> Value {
    let doc = decode(&s2_document());
    wire(
        &plan_convert(&doc, 5, &uids(&["ly_a", "ly_b", "ly_c"]))
            .unwrap()
            .document,
    )
}

type Mutation = (&'static str, fn(&mut Value));

fn comp(v: &mut Value) -> &mut Value {
    let list = v["componentTables"]["components"]["components"]
        .as_array_mut()
        .unwrap();
    list.last_mut().unwrap()
}
fn inst(v: &mut Value) -> &mut Value {
    let list = v["componentTables"]["components"]["instances"]
        .as_array_mut()
        .unwrap();
    list.last_mut().unwrap()
}

#[test]
fn the_unmutated_conversions_pass_the_check() {
    let o = convert_oracle();
    verify(&s1_after(), S1_SYMBOL, "S1", &o).unwrap();
    verify(&s2_after(), S2_SYMBOL, "S2", &o).unwrap();
}

#[test]
fn every_single_deviation_of_a_single_conversion_is_noticed() {
    let o = convert_oracle();
    let mutations: [Mutation; 20] = [
        ("play mode", |v| inst(v)["symPlayMode"] = json!("loop")),
        ("speed", |v| inst(v)["symSpeed"] = json!(2)),
        ("placed at", |v| inst(v)["symPlacedAt"] = json!(4)),
        ("single frame", |v| inst(v)["symSingleFrame"] = json!(5)),
        ("frame key stamped", |v| {
            inst(v)["frameKeys"] = json!([{"frame":0,"componentFrame":0}])
        }),
        ("component name", |v| comp(v)["name"] = json!("Hand")),
        ("inner locked", |v| {
            comp(v)["layers"][0]["locked"] = json!(true)
        }),
        ("inner hidden", |v| {
            comp(v)["layers"][0]["visible"] = json!(false)
        }),
        ("inner key frames", |v| {
            comp(v)["layers"][0]["timing"]["keys"] = json!([{"frame":0}])
        }),
        ("inner groups dropped", |v| {
            comp(v)["layers"][0]
                .as_object_mut()
                .unwrap()
                .remove("groups");
        }),
        ("group mode", |v| {
            comp(v)["layers"][0]["groups"]["entries"][0]["combineMode"] = json!("subtract")
        }),
        ("stroke dropped", |v| {
            comp(v)["objects"].as_array_mut().unwrap().pop();
        }),
        ("stroke renamed", |v| {
            comp(v)["objects"][0]["target"]["strokeId"] = json!("zz")
        }),
        ("stroke moved", |v| {
            comp(v)["objects"][0]["target"]["frameScope"]["frame"] = json!(3)
        }),
        ("instance unlocked", |v| {
            v["layers"][0]["locked"] = json!(false)
        }),
        ("instance keeps keys", |v| {
            v["layers"][0]["timing"]["keys"] = json!([{"frame":0},{"frame":12}])
        }),
        ("instance keeps a stroke", |v| {
            let mut copy = comp(v)["objects"][0].clone();
            copy["target"]["contextId"] = json!("scene-root");
            copy["target"]["layerUid"] = json!("ly_hand");
            v["objects"].as_array_mut().unwrap().push(copy);
        }),
        ("module missing", |v| {
            v["componentTables"]["storyboard"]["modules"] = json!([])
        }),
        ("next id", |v| {
            v["componentTables"]["storyboard"]["nextId"] = json!(1)
        }),
        ("module slot", |v| {
            v["componentTables"]["storyboard"]["modules"][0]["x"] = json!(0)
        }),
    ];
    for (name, mutate) in mutations {
        let mut v = s1_after();
        mutate(&mut v);
        assert!(
            verify(&v, S1_SYMBOL, "S1", &o).is_err(),
            "{name} went unnoticed"
        );
    }
}

#[test]
fn every_single_deviation_of_a_merge_is_noticed() {
    let o = convert_oracle();
    let mutations: [Mutation; 10] = [
        ("block not collapsed", |v| {
            let l = v["layers"][1].clone();
            v["layers"].as_array_mut().unwrap().push(l);
        }),
        ("wrong slot", |v| {
            v["layers"].as_array_mut().unwrap().swap(1, 2)
        }),
        ("duplicator lost", |v| {
            v["componentTables"]["duplicators"]["duplicators"] = json!([])
        }),
        ("inner order", |v| {
            comp(v)["layers"].as_array_mut().unwrap().swap(0, 1)
        }),
        ("hidden source unlocked state", |v| {
            comp(v)["layers"][1]["visible"] = json!(true)
        }),
        ("source lock carried", |v| {
            comp(v)["layers"][1]["locked"] = json!(true)
        }),
        ("inner uid", |v| {
            comp(v)["layers"][0]["layerUid"] = json!("other")
        }),
        ("sound count", |v| {
            comp(v)["layers"].as_array_mut().unwrap().truncate(2)
        }),
        ("first source folder carried", |v| {
            v["layers"][1]["folderId"] = json!("fold_1")
        }),
        ("path effects dropped", |v| {
            comp(v)["layers"][0]
                .as_object_mut()
                .unwrap()
                .remove("pathFx");
        }),
    ];
    for (name, mutate) in mutations {
        let mut v = s2_after();
        mutate(&mut v);
        assert!(
            verify(&v, S2_SYMBOL, "S2", &o).is_err(),
            "{name} went unnoticed"
        );
    }
}

#[test]
fn refusals_are_typed_and_leave_the_input_untouched() {
    let s1 = decode(&s1_document(json!({})));
    let s2 = decode(&s2_document());
    let before = (s1.clone(), s2.clone());
    let cases: Vec<(&str, Result<SetupPlan, SetupError>, SetupError)> = vec![
        (
            "unknown",
            plan_convert(&s1, 1, &uids(&["nope"])),
            SetupError::UnknownLayer("nope".into()),
        ),
        ("empty", plan_convert(&s1, 1, &[]), SetupError::NoLayers),
        (
            "revision",
            plan_convert(&s1, 9_007_199_254_740_992, &uids(&["ly_hand"])),
            SetupError::RevisionOutOfRange,
        ),
    ];
    for (name, got, want) in cases {
        assert_eq!(got.unwrap_err(), want, "{name}");
    }
    assert_eq!((s1, s2), before);
}

#[test]
fn an_instance_inside_the_selection_is_refused_with_the_baseline_reason() {
    let first = plan_convert(&decode(&s1_document(json!({}))), 1, &uids(&["ly_hand"])).unwrap();
    let error = plan_convert(&first.document, 2, &uids(&["ly_other", "ly_hand"])).unwrap_err();
    assert_eq!(
        error,
        SetupError::BadSource {
            layer_uid: "ly_hand".into(),
            refusal: SourceRefusal::Component
        }
    );
}

#[test]
fn an_unnamed_source_and_a_v1_document_have_no_conversion() {
    let mut value = s1_document(json!({}));
    value["layers"][0].as_object_mut().unwrap().remove("name");
    let unnamed = plan_convert(&decode(&value), 1, &uids(&["ly_hand"])).unwrap_err();
    assert_eq!(unnamed, SetupError::Unnamed("ly_hand".into()));
    let v1 = decode(
        &json!({"format":"nemo.native-object-document","formatVersion":1,
        "totalFrames":20,"layers":[{"layerUid":"L"}],"objects":[]}),
    );
    assert_eq!(
        plan_convert(&v1, 1, &uids(&["L"])).unwrap_err(),
        SetupError::V1Document
    );
    assert_eq!(
        plan_place(&v1, 1, "sym", "x").unwrap_err(),
        SetupError::V1Document
    );
}

fn holder(layer: &str, stroke: Option<&str>) -> Value {
    let mut target = json!({"contextId":"scene-root","layerUid":layer});
    if let Some(stroke) = stroke {
        target["strokeId"] = json!(stroke);
    }
    json!({"propertyHolders":[{"target":target,"properties":[{"property":"opacity","static":[50]}]}]})
}

#[test]
fn motion_that_cannot_enter_a_component_is_refused_never_dropped() {
    let element = decode(&s1_document(holder("ly_hand", Some("sa"))));
    assert_eq!(
        plan_convert(&element, 1, &uids(&["ly_hand"])).unwrap_err(),
        SetupError::ElementHoldersUnavailable {
            layer_uid: "ly_hand".into()
        }
    );
    let mut merged = s2_document();
    merged["propertyHolders"] = holder("ly_b", None)["propertyHolders"].clone();
    assert_eq!(
        plan_convert(&decode(&merged), 5, &uids(&["ly_a", "ly_b"])).unwrap_err(),
        SetupError::HoldersUnavailable {
            layer_uid: "ly_b".into()
        }
    );
    // A layer-level holder of a single conversion stays on the instance, and a
    // holder on a layer that is not part of the merge is untouched.
    let kept = decode(&s1_document(holder("ly_hand", None)));
    assert!(plan_convert(&kept, 1, &uids(&["ly_hand"])).is_ok());
    let mut elsewhere = s2_document();
    elsewhere["propertyHolders"] = holder("ly_y", None)["propertyHolders"].clone();
    let plan = plan_convert(&decode(&elsewhere), 5, &uids(&["ly_a", "ly_b"])).unwrap();
    assert_eq!(
        wire(&plan.document)["propertyHolders"][0]["target"]["layerUid"],
        "ly_y"
    );
}

#[test]
fn minted_identities_take_the_first_free_suffix() {
    let first = plan_convert(&decode(&s1_document(json!({}))), 1, &uids(&["ly_hand"])).unwrap();
    let second = plan_convert(&first.document, 1, &uids(&["ly_other"])).unwrap();
    assert_eq!(second.symbol_id.as_deref(), Some("native-component:1:1"));
    let taken = document(
        &[
            layer("native-layer:5:0", "Taken", &[(0, "a")]),
            layer("p", "P", &[(0, "b")]),
        ],
        json!({}),
    );
    let merged = plan_convert(&decode(&taken), 5, &uids(&["native-layer:5:0", "p"])).unwrap();
    assert_eq!(merged.created.as_deref(), Some("native-layer:5:1"));
}

#[test]
fn a_motion_holder_alone_never_creates_a_component() {
    let doc = decode(&s1_document(holder("ly_hand", None)));
    assert!(doc.property_holders().is_some());
    assert!(doc.component_tables().is_none());
    assert!(doc.component_set().is_none());
}

#[test]
fn a_widget_layer_in_the_selection_is_refused_with_the_baseline_reason() {
    let uid = "native-layer:1:0";
    let widget = json!({"kind":"joystick","pos":[100.0,100.0],"size":[160,160],"screenSpace":false,
        "x":{"key":"xc_native_layer_1_0_x","min":-100,"max":100,"rest":0,"label":"X"},
        "y":{"key":"xc_native_layer_1_0_y","min":-100,"max":100,"rest":0,"label":"Y"}});
    let holders = json!([{"target":{"contextId":"scene-root","layerUid":uid},
        "controls":[{"key":"xc_native_layer_1_0_x","name":"X","type":"number","default":[0]},
            {"key":"xc_native_layer_1_0_y","name":"Y","type":"number","default":[0]}],
        "properties":[{"property":"xc_native_layer_1_0_x","static":[0]},
            {"property":"xc_native_layer_1_0_y","static":[0]}]}]);
    let layers = [
        layer(uid, "Joystick", &[])
            .with(json!({"widget":widget,"color":"#ffb86c","parentLayerUid":null})),
        layer("ly_other", "Other", &[(0, "oa")]),
    ];
    let doc = decode(&document(&layers, json!({"propertyHolders":holders})));
    let want = SetupError::BadSource {
        layer_uid: uid.into(),
        refusal: SourceRefusal::Widget,
    };
    assert_eq!(plan_convert(&doc, 1, &uids(&[uid])).unwrap_err(), want);
    assert_eq!(
        plan_convert(&doc, 1, &uids(&["ly_other", uid])).unwrap_err(),
        want
    );
}

#[test]
fn a_merged_effector_layer_leaves_no_record_and_other_effectors_stay() {
    let mut value = s2_document();
    value["componentTables"]["duplicators"]["effectorLayers"] = json!([
        {"contextId":"scene-root","layerUid":"ly_b","effector":{"targetLayerUid":"ly_y"}},
        {"contextId":"scene-root","layerUid":"ly_y","effector":{"targetLayerUid":"ly_x"}}
    ]);
    let doc = decode(&value);
    let plan = plan_convert(&doc, 5, &uids(&["ly_a", "ly_b", "ly_c"])).unwrap();
    let after = wire(&plan.document);
    let table = &after["componentTables"]["duplicators"];
    let records = table["effectorLayers"].as_array().unwrap();
    assert_eq!(records.len(), 1, "the merged layer's record is gone");
    assert_eq!(records[0]["layerUid"], "ly_y");
    assert_eq!(table["duplicators"][0]["layerUid"], "native-layer:5:0");
}
