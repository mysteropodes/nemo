//! Negative and mutation controls for the native component records (L04-G).
//!
//! Every control starts from the baseline table of `tests/object_component_records.rs`
//! (`baseline_table_is_admissible` proves the unmutated table passes, so no control
//! can be vacuous), changes exactly one thing, and pins the exact error variant.
use super::tests::{hosts, set_json, set_value};
use super::*;
use crate::object_document::ObjectLayer;
use serde_json::{json, Value};

type Outcome = Result<(), ComponentRecordError>;

fn run(edit: impl FnOnce(&mut Value)) -> Outcome {
    let mut value = set_value();
    edit(&mut value);
    let set: ComponentSet = serde_json::from_value(value).expect("the mutation must still decode");
    set.validate(&hosts())
}
fn run_with_hosts(
    edit_hosts: impl FnOnce(&mut Vec<Value>),
    edit: impl FnOnce(&mut Value),
) -> Outcome {
    let mut layers: Vec<Value> = hosts()
        .iter()
        .map(|l| serde_json::to_value(l).unwrap())
        .collect();
    edit_hosts(&mut layers);
    let layers: Vec<ObjectLayer> = layers
        .into_iter()
        .map(|v| serde_json::from_value(v).unwrap())
        .collect();
    let mut value = set_value();
    edit(&mut value);
    let set: ComponentSet = serde_json::from_value(value).expect("the mutation must still decode");
    set.validate(&layers)
}
fn decode_fails(value: Value) -> bool {
    serde_json::from_value::<ComponentSet>(value).is_err()
}
fn inst(v: &mut Value, uid: &str) -> usize {
    v["instances"]
        .as_array()
        .unwrap()
        .iter()
        .position(|i| i["layerUid"] == uid)
        .expect("instance")
}
use ComponentRecordError::{DuplicateId, Invalid, NestedComponent, Unresolved, Unsupported};

#[test]
fn baseline_table_is_admissible() {
    assert_eq!(run(|_| {}), Ok(()));
}

#[test]
fn table_header_and_identity_rules() {
    assert!(matches!(
        run(|v| v["format"] = json!("nemo.other")),
        Err(Unsupported(_))
    ));
    assert!(matches!(
        run(|v| v["formatVersion"] = json!(2)),
        Err(Unsupported(_))
    ));
    assert!(matches!(
        run(|v| {
            let c = v["components"][0].clone();
            v["components"].as_array_mut().unwrap().push(c);
        }),
        Err(DuplicateId("duplicate component symbolId"))
    ));
    for bad in ["", "   ", "scene-root"] {
        assert!(
            matches!(
                run(|v| v["components"][0]["symbolId"] = json!(bad)),
                Err(Invalid(_))
            ),
            "{bad:?}"
        );
    }
    assert!(matches!(
        run(|v| v["components"][0]["totalFrames"] = json!(0)),
        Err(Invalid(_))
    ));
    assert_eq!(
        run(|v| v["components"][0]["totalFrames"] = json!(1)),
        Err(Invalid("object frame or strokeId is outside the component"))
    );
}

#[test]
fn component_content_rules() {
    assert!(matches!(
        run(|v| v["components"][0]["fps"] = json!(0)),
        Err(Invalid(_))
    ));
    assert!(matches!(
        run(|v| v["components"][0]["fps"] = json!(-24)),
        Err(Invalid(_))
    ));
    assert_eq!(run(|v| v["components"][0]["fps"] = json!(30)), Ok(()));
    assert!(matches!(
        run(|v| v["components"][0]["layers"] = json!([])),
        Err(Invalid(_))
    ));
    assert!(matches!(
        run(|v| v["components"][0]["layers"][1]["layerUid"] = json!("ly_comp_ia")),
        Err(DuplicateId("duplicate inner layerUid"))
    ));
    assert!(matches!(
        run(|v| v["components"][0]["layers"][0]["layerUid"] = json!("")),
        Err(Invalid(_))
    ));
    // each object field is judged against its own component
    assert!(matches!(
        run(|v| v["components"][0]["objects"][0]["target"]["contextId"] = json!("scene-root")),
        Err(Invalid("object context differs from its component"))
    ));
    assert!(matches!(
        run(|v| v["components"][0]["objects"][0]["target"]["layerUid"] = json!("ly_ghost")),
        Err(Unresolved("object layer is not a layer of its component"))
    ));
    assert!(matches!(
        run(
            |v| v["components"][0]["objects"][0]["target"]["frameScope"]["kind"] =
                json!("reference")
        ),
        Err(Unsupported(_))
    ));
    assert!(matches!(
        run(|v| v["components"][0]["objects"][0]["target"]["frameScope"]["frame"] = json!(12)),
        Err(Invalid(_))
    ));
    assert!(matches!(
        run(|v| v["components"][0]["objects"][0]["target"]["strokeId"] = json!("")),
        Err(Invalid(_))
    ));
}

#[test]
fn exposed_property_rules() {
    assert!(matches!(
        run(|v| v["components"][0]["exposedProps"][0]["key"] = json!("xx_1")),
        Err(Invalid(_))
    ));
    assert!(matches!(
        run(|v| v["components"][0]["exposedProps"][0]["key"] = json!("ep_")),
        Err(Invalid(_))
    ));
    assert!(matches!(
        run(|v| v["components"][0]["exposedProps"][0]["targetField"] = json!("")),
        Err(Invalid(_))
    ));
    assert!(matches!(
        run(|v| {
            let key = v["components"][0]["exposedProps"][0]["key"].clone();
            v["components"][0]["exposedProps"][1]["key"] = key;
        }),
        Err(DuplicateId("duplicate exposed property key"))
    ));
    assert!(matches!(
        run(|v| v["components"][0]["exposedProps"][0]["targetStrokeId"] = json!("s_ghost")),
        Err(Unresolved(
            "exposed property targets a stroke outside its component"
        ))
    ));
    // The stroke it names must really exist: delete every `s_comp_a_3` object.
    assert!(matches!(
        run(|v| {
            let objects = v["components"][0]["objects"].as_array_mut().unwrap();
            objects.retain(|o| o["target"]["strokeId"] != "s_comp_a_3");
        }),
        Err(Unresolved(_))
    ));
    // A second entry on the same stroke and field is legal in the baseline (last write wins).
    assert_eq!(
        run(|v| {
            let mut again = v["components"][0]["exposedProps"][0].clone();
            again["key"] = json!("ep_again_0001");
            v["components"][0]["exposedProps"]
                .as_array_mut()
                .unwrap()
                .push(again);
        }),
        Ok(())
    );
}

#[test]
fn nested_components_are_refused_like_enter_symbol() {
    assert_eq!(
        run(|v| v["instances"][0]["contextId"] = json!("sym_comp")),
        Err(NestedComponent)
    );
    assert!(matches!(
        run(|v| v["instances"][0]["contextId"] = json!("ghost")),
        Err(Unresolved(_))
    ));
    // An inner layer uid is not a scene layer: an instance cannot be placed on it either.
    assert!(matches!(
        run(|v| v["instances"][0]["layerUid"] = json!("ly_comp_ia")),
        Err(Unresolved(_))
    ));
    assert!(matches!(
        run(|v| v["instances"][0]["layerUid"] = json!("ly_ghost")),
        Err(Unresolved(_))
    ));
    assert!(matches!(
        run(|v| v["instances"][0]["symbolId"] = json!("sym_ghost")),
        Err(Unresolved(_))
    ));
    assert!(matches!(
        run(|v| {
            let copy = v["instances"][0].clone();
            v["instances"].as_array_mut().unwrap().push(copy);
        }),
        Err(DuplicateId("duplicate instance layer"))
    ));
}

#[test]
fn placement_field_rules() {
    for label in ["bogus", "Once", "LOOP", ""] {
        assert!(
            matches!(
                run(|v| v["instances"][0]["symPlayMode"] = json!(label)),
                Err(Invalid(_))
            ),
            "{label:?}"
        );
    }
    for label in ["loop", "once", "pingpong", "single"] {
        assert_eq!(
            run(|v| v["instances"][0]["symPlayMode"] = json!(label)),
            Ok(())
        );
    }
    assert_eq!(
        run(|v| v["instances"][0]["symPlayMode"] = Value::Null),
        Ok(())
    );
    assert!(matches!(
        run(|v| v["instances"][0]["symMatrix"] = json!([1, 0, 0, 1, 0])),
        Err(Invalid(_))
    ));
    assert!(matches!(
        run(|v| v["instances"][0]["symMatrix"] = json!([1, 0, 0, 1, 0, 0, 0])),
        Err(Invalid(_))
    ));
    assert_eq!(
        run(|v| v["instances"][0]["symMatrix"] = json!([1, 0, 0, 1, 0, 0])),
        Ok(())
    );
    // Numbers the baseline reads with `||`/`!= null` are not refused by value.
    for field in [
        "symSpeed",
        "symPlacedAt",
        "symSingleFrame",
        "symTrimIn",
        "symTrimOut",
    ] {
        for value in [json!(0), json!(-3), json!(2.5)] {
            assert_eq!(
                run(|v| v["instances"][0][field] = value.clone()),
                Ok(()),
                "{field} {value}"
            );
        }
    }
}

#[test]
fn time_remap_track_rules() {
    let at = |v: &mut Value| inst(v, "ly_comp_remap");
    assert!(matches!(
        run(|v| {
            let i = at(v);
            v["instances"][i]["timeRemap"]["keys"][1]["frame"] = json!(0)
        }),
        Err(Invalid("timeRemap keys must strictly increase"))
    ));
    assert!(matches!(
        run(|v| {
            let i = at(v);
            v["instances"][i]["timeRemap"]["keys"][0]["v"] = json!([0, 1])
        }),
        Err(Invalid("timeRemap values are one finite number"))
    ));
    assert!(matches!(
        run(|v| {
            let i = at(v);
            v["instances"][i]["timeRemap"]["keys"][0]["hIn"] = json!([0])
        }),
        Err(Invalid("timeRemap handles are two finite numbers"))
    ));
    assert!(matches!(
        run(|v| {
            let i = at(v);
            v["instances"][i]["timeRemap"]["keys"][0]["curvePoints"][0]["x"] = json!(0.5)
        }),
        Err(Invalid(_))
    ));
    assert_eq!(
        run(|v| {
            let i = at(v);
            v["instances"][i]["timeRemap"]["keys"] = json!([])
        }),
        Ok(())
    );
    assert_eq!(
        run(|v| {
            let i = at(v);
            v["instances"][i]["timeRemap"]["keys"][0]["hold"] = json!(true)
        }),
        Ok(())
    );
}

#[test]
fn frame_key_rules() {
    let at = |v: &mut Value| inst(v, "ly_comp_blank");
    // Baseline writes componentFrame / blankOverride only on `isKeyframe` outer frames.
    assert!(matches!(
        run(|v| {
            let i = at(v);
            v["instances"][i]["frameKeys"][0]["frame"] = json!(5)
        }),
        Err(Invalid("a frame key must sit on a timing key of its layer"))
    ));
    assert!(matches!(
        run(|v| {
            let i = at(v);
            v["instances"][i]["frameKeys"] = json!([{"frame": 12, "blankOverride": true}, {"frame": 12, "blankOverride": false}]);
        }),
        Err(Invalid("frameKeys must strictly increase"))
    ));
    assert!(matches!(
        run(|v| {
            let i = at(v);
            v["instances"][i]["frameKeys"] = json!([{"frame": 0}])
        }),
        Err(Invalid(_))
    ));
    assert_eq!(
        run(|v| {
            let i = at(v);
            v["instances"][i]["frameKeys"] =
                json!([{"frame": 0, "componentFrame": null}, {"frame": 12, "blankOverride": false}])
        }),
        Ok(())
    );
    // The same entry is fine once the host layer has the key, and refused when it has no timing at all.
    assert_eq!(
        run_with_hosts(
            |layers| {
                let host = layers
                    .iter_mut()
                    .find(|l| l["layerUid"] == "ly_comp_blank")
                    .unwrap();
                host["timing"]["keys"] = json!([{"frame": 0}, {"frame": 5}, {"frame": 12}]);
            },
            |v| {
                let i = at(v);
                v["instances"][i]["frameKeys"][0]["frame"] = json!(5)
            },
        ),
        Ok(())
    );
    assert!(matches!(
        run_with_hosts(
            |layers| {
                let host = layers
                    .iter_mut()
                    .find(|l| l["layerUid"] == "ly_comp_blank")
                    .unwrap();
                host.as_object_mut().unwrap().remove("timing");
            },
            |_| {},
        ),
        Err(Invalid("a frame key must sit on a timing key of its layer"))
    ));
}

#[test]
fn decoding_is_object_only_strict_and_bounded() {
    let unknown = |path: &[&str]| {
        let mut v = set_value();
        let mut cursor = &mut v;
        for step in path {
            cursor = match step.parse::<usize>() {
                Ok(i) => &mut cursor[i],
                Err(_) => &mut cursor[*step],
            };
        }
        cursor["surprise"] = json!(1);
        v
    };
    for path in [
        vec![],
        vec!["components", "0"],
        vec!["components", "0", "exposedProps", "0"],
        vec!["components", "0", "layers", "0"],
        vec!["components", "0", "objects", "0"],
        vec!["instances", "0"],
        vec!["instances", "6", "frameKeys", "0"],
    ] {
        assert!(decode_fails(unknown(&path)), "{path:?}");
    }
    // A baseline symbol also carries these; they are refused, never silently dropped.
    for dropped in [
        "cameraKeys",
        "markers",
        "motionArcs",
        "tweenOverrides",
        "tweenEasing",
    ] {
        let mut v = set_value();
        v["components"][0][dropped] = json!([]);
        assert!(decode_fails(v), "{dropped}");
    }
    let json = set_json();
    assert!(serde_json::from_str::<ComponentSet>(&json.replacen(
        "\"formatVersion\":1",
        "\"formatVersion\":1,\"formatVersion\":1",
        1
    ))
    .is_err());
    let positional = json.replace("{\"frame\":0,\"componentFrame\":3}", "[0,3]");
    assert_ne!(positional, json);
    assert!(serde_json::from_str::<ComponentSet>(&positional).is_err());
    // null is only legal where the baseline writes null
    let mut v = set_value();
    v["components"][0]["name"] = Value::Null;
    assert!(decode_fails(v));
    for value in [json!(1.5), json!(-1), json!("12"), Value::Null] {
        let mut v = set_value();
        v["components"][0]["totalFrames"] = value;
        assert!(decode_fails(v));
    }
    let mut v = set_value();
    v["instances"][0]["frameKeys"] = Value::Null;
    assert!(decode_fails(v));
    // bounded arrays: 4096 decode, 4097 do not
    let one = set_value()["instances"][0].clone();
    for (count, ok) in [(4096usize, true), (4097, false)] {
        let mut v = set_value();
        v["instances"] = Value::Array(vec![one.clone(); count]);
        assert_eq!(!decode_fails(v), ok, "{count} instances");
    }
}
