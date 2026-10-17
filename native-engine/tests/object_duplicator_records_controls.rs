//! Negative and mutation controls for the native duplicator records (L04-G).
//!
//! Each control starts from the baseline table of `tests/object_duplicator_records.rs`
//! (`baseline_is_admissible` proves the unmutated table passes, so no control can be
//! vacuous), changes exactly one thing, and pins the exact error variant.
use super::tests::{layers, set_json, set_value};
use super::*;
use serde_json::{json, Value};
use DuplicatorRecordError::{Chained, DuplicateId, Invalid, Unresolved, Unsupported};

type Outcome = Result<(), DuplicatorRecordError>;

fn run(edit: impl FnOnce(&mut Value)) -> Outcome {
    let mut value = set_value();
    edit(&mut value);
    let set: DuplicatorSet = serde_json::from_value(value).expect("the mutation must still decode");
    set.validate(&layers())
}
fn host(v: &mut Value, uid: &str) -> usize {
    v["duplicators"]
        .as_array()
        .unwrap()
        .iter()
        .position(|d| d["layerUid"] == uid)
        .expect("host")
}
/// Mutates one field of the descriptor of the duplicator on `uid`.
fn dup(uid: &'static str, field: &'static str, value: Value) -> impl FnOnce(&mut Value) {
    move |v: &mut Value| {
        let i = host(v, uid);
        v["duplicators"][i]["duplicator"][field] = value;
    }
}
fn effector(field: &'static str, value: Value) -> impl FnOnce(&mut Value) {
    move |v: &mut Value| v["effectorLayers"][0]["effector"][field] = value
}
fn decode_fails(value: Value) -> bool {
    serde_json::from_value::<DuplicatorSet>(value).is_err()
}

#[test]
fn baseline_is_admissible() {
    assert_eq!(run(|_| {}), Ok(()));
}

#[test]
fn header_and_layer_identity_rules() {
    assert!(matches!(
        run(|v| v["format"] = json!("nemo.other")),
        Err(Unsupported(_))
    ));
    assert!(matches!(
        run(|v| v["formatVersion"] = json!(2)),
        Err(Unsupported(_))
    ));
    assert_eq!(
        run(|v| v["duplicators"][0]["layerUid"] = json!("ly_ghost")),
        Err(Unresolved("duplicator layer is not a layer of its context"))
    );
    assert!(matches!(
        run(|v| v["duplicators"][0]["contextId"] = json!("sym_comp")),
        Err(Unresolved(_))
    ));
    assert!(matches!(
        run(|v| v["duplicators"][0]["layerUid"] = json!("")),
        Err(Invalid(_))
    ));
    assert_eq!(
        run(|v| {
            let copy = v["duplicators"][0].clone();
            v["duplicators"].as_array_mut().unwrap().push(copy);
        }),
        Err(DuplicateId("a layer carries one duplicator"))
    );
}

#[test]
fn labels_are_the_closed_sets_the_ui_writes() {
    for bad in ["spiral", "", "Grid", "RADIAL"] {
        assert_eq!(
            run(dup("ly_toggle", "mode", json!(bad))),
            Err(Invalid("mode is not grid, radial or path")),
            "{bad:?}"
        );
    }
    for ok in ["grid", "radial", "path"] {
        assert_eq!(run(dup("ly_mig_dup", "mode", json!(ok))), Ok(()), "{ok}");
    }
    assert!(matches!(
        run(dup("ly_toggle", "sourceMode", json!("pool"))),
        Err(Invalid(_))
    ));
    for ok in ["shape", "objects"] {
        assert_eq!(run(dup("ly_toggle", "sourceMode", json!(ok))), Ok(()));
    }
    let direction = |label: &'static str| {
        dup(
            "ly_toggle",
            "timeOffset",
            json!({"enabled": true, "offsetFrames": 2, "direction": label}),
        )
    };
    assert!(matches!(run(direction("alternate")), Err(Invalid(_))));
    for ok in ["forward", "backward", "centerOut", "random"] {
        assert_eq!(run(direction(ok)), Ok(()), "{ok}");
    }
    // the panel creates the sub-objects lazily, so partial ones are legal
    assert_eq!(run(dup("ly_toggle", "timeOffset", json!({}))), Ok(()));
    assert_eq!(
        run(dup("ly_toggle", "staggerRandom", json!({"hue": true}))),
        Ok(())
    );
}

#[test]
fn inline_effector_rules() {
    let with = |effector: Value| dup("ly_toggle", "effectors", json!([effector]));
    assert!(matches!(
        run(with(json!({"falloff": "gaussian"}))),
        Err(Invalid(_))
    ));
    for ok in ["radial", "linear", "step"] {
        assert_eq!(run(with(json!({"falloff": ok}))), Ok(()), "{ok}");
    }
    assert!(matches!(
        run(with(json!({"channels": [{"prop": "skew", "value": [1]}]}))),
        Err(Invalid(_))
    ));
    assert!(matches!(
        run(with(
            json!({"channels": [{"prop": "scale", "value": [1, 2, 3]}]})
        )),
        Err(Invalid(_))
    ));
    assert!(matches!(
        run(with(json!({"offsetPos": [1, 2, 3]}))),
        Err(Invalid(_))
    ));
    assert!(matches!(
        run(with(json!({"offsetScale": [1, 2, 3]}))),
        Err(Invalid(_))
    ));
    for prop in [
        "position",
        "positionZ",
        "rotation",
        "rotationX",
        "rotationY",
        "scale",
        "opacity",
        "hue",
    ] {
        assert_eq!(
            run(with(json!({"channels": [{"prop": prop, "value": [1, 2]}]}))),
            Ok(()),
            "{prop}"
        );
    }
    // null channels means "migrate the legacy fields"; an empty list and a missing `value` are legal
    assert_eq!(
        run(with(json!({"channels": null, "offsetPos": [5, 0]}))),
        Ok(())
    );
    assert_eq!(run(with(json!({"channels": [{"prop": "hue"}]}))), Ok(()));
    assert_eq!(run(with(json!({"channels": []}))), Ok(()));
}

#[test]
fn a_path_layer_may_not_chain_and_must_exist_in_path_mode() {
    let path = |uid: Value| dup("ly_pathdup", "pathLayerUid", uid);
    assert_eq!(
        run(path(json!("ly_pathdup"))),
        Err(Chained("a duplicator cannot path along itself"))
    );
    assert_eq!(
        run(path(json!("ly_toggle"))),
        Err(Chained("the path layer is itself a duplicator"))
    );
    assert_eq!(
        run(path(json!("ly_ghost"))),
        Err(Unresolved("the path layer names no layer"))
    );
    assert_eq!(
        run(path(Value::Null)),
        Ok(()),
        "path mode with no layer picked yet is legal"
    );
    assert_eq!(run(path(json!("ly_pool_a"))), Ok(()));
    // The reference only matters for the mode that reads it: a stale uid in grid mode is carried.
    let stale = |mode: &'static str| {
        move |v: &mut Value| {
            let i = host(v, "ly_pathdup");
            v["duplicators"][i]["duplicator"]["mode"] = json!(mode);
            v["duplicators"][i]["duplicator"]["pathLayerUid"] = json!("ly_ghost");
        }
    };
    assert_eq!(
        run(stale("path")),
        Err(Unresolved("the path layer names no layer"))
    );
    assert_eq!(run(stale("grid")), Ok(()));
    assert_eq!(run(stale("radial")), Ok(()));
}

#[test]
fn an_object_pool_may_not_chain_but_may_dangle() {
    let pool = |uids: Value| dup("ly_objhost", "sourceLayerUids", uids);
    let chained = Err(Chained(
        "a pool member is a duplicator (or the host itself)",
    ));
    assert_eq!(run(pool(json!(["ly_objhost"]))), chained.clone());
    assert_eq!(
        run(pool(json!(["ly_pool_a", "ly_toggle"]))),
        chained.clone()
    );
    assert_eq!(run(pool(json!(["ly_mig_dup"]))), chained);
    assert_eq!(
        run(pool(json!(["ly_ghost"]))),
        Ok(()),
        "a deleted source silently drops out at evaluation"
    );
    assert_eq!(run(pool(json!([]))), Ok(()));
    assert!(matches!(
        run(pool(json!(["ly_pool_a", ""]))),
        Err(Invalid(_))
    ));
    // in shape mode the pool is not read, so a stale chained uid is carried
    let shape = |v: &mut Value| {
        let i = host(v, "ly_objhost");
        v["duplicators"][i]["duplicator"]["sourceMode"] = json!("shape");
        v["duplicators"][i]["duplicator"]["sourceLayerUids"] = json!(["ly_toggle"]);
    };
    assert_eq!(run(shape), Ok(()));
}

#[test]
fn effector_layer_rules() {
    assert_eq!(
        run(|v| v["effectorLayers"][0]["layerUid"] = json!("ly_ghost")),
        Err(Unresolved("effector layer is not a layer of its context"))
    );
    assert!(matches!(
        run(|v| v["effectorLayers"][0]["contextId"] = json!("sym_comp")),
        Err(Unresolved(_))
    ));
    assert_eq!(
        run(|v| v["effectorLayers"][0]["layerUid"] = json!("ly_toggle")),
        Err(Invalid(
            "a layer cannot be both a duplicator and an effector"
        ))
    );
    assert_eq!(
        run(|v| {
            let copy = v["effectorLayers"][0].clone();
            v["effectorLayers"].as_array_mut().unwrap().push(copy);
        }),
        Err(DuplicateId("a layer carries one effector"))
    );
    assert_eq!(
        run(effector("targetLayerUid", json!("ly_effector"))),
        Err(Invalid("effector target must be another layer"))
    );
    assert!(matches!(
        run(effector("targetLayerUid", json!(""))),
        Err(Invalid(_))
    ));
    // the baseline never reads an effector layer whose target is gone, so it stays representable
    assert_eq!(run(effector("targetLayerUid", json!("ly_ghost"))), Ok(()));
    assert!(matches!(
        run(effector("falloff", json!("gaussian"))),
        Err(Invalid(_))
    ));
    for ok in ["radial", "linear", "step"] {
        assert_eq!(run(effector("falloff", json!(ok))), Ok(()));
    }
    assert_eq!(
        run(effector("pos", json!([1, 2, 3]))),
        Err(Invalid("effector pos is a two-number world anchor"))
    );
    assert!(matches!(run(effector("pos", json!([1]))), Err(Invalid(_))));
    assert_eq!(run(effector("pos", json!([0, 0]))), Ok(()));
    let mut gone = set_value();
    gone["effectorLayers"][0]["effector"]
        .as_object_mut()
        .unwrap()
        .remove("pos");
    assert_eq!(
        serde_json::from_value::<DuplicatorSet>(gone)
            .unwrap()
            .validate(&layers()),
        Ok(())
    );
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
        vec!["duplicators", "0"],
        vec!["duplicators", "0", "duplicator"],
        vec!["duplicators", "0", "duplicator", "staggerRandom"],
        vec!["duplicators", "0", "duplicator", "timeOffset"],
        vec!["duplicators", "0", "duplicator", "effectors", "0"],
        vec!["duplicators", "3", "duplicator", "effectors", "0", "pos"],
        vec![
            "duplicators",
            "3",
            "duplicator",
            "effectors",
            "0",
            "channels",
            "0",
        ],
        vec!["effectorLayers", "0"],
        vec!["effectorLayers", "0", "effector"],
    ] {
        assert!(decode_fails(unknown(&path)), "{path:?}");
    }
    let text = set_json();
    assert!(serde_json::from_str::<DuplicatorSet>(&text.replacen(
        "\"formatVersion\":1",
        "\"formatVersion\":1,\"formatVersion\":1",
        1
    ))
    .is_err());
    let positional = text.replace("\"pos\":[320,180]", "\"pos\":{\"0\":320}");
    assert_ne!(positional, text);
    assert!(serde_json::from_str::<DuplicatorSet>(&positional).is_err());
    let positional = text.replace("{\"x\":320,\"y\":180}", "[320,180]");
    assert_ne!(positional, text);
    assert!(serde_json::from_str::<DuplicatorSet>(&positional).is_err());
    // null is only legal where the baseline writes null (endAngle, pathLayerUid)
    for field in [
        "rows",
        "cols",
        "count",
        "mode",
        "seed",
        "radialOrient",
        "staggerRandom",
    ] {
        let mut v = set_value();
        v["duplicators"][1]["duplicator"][field] = Value::Null;
        assert!(decode_fails(v), "{field}");
    }
    for field in ["endAngle", "pathLayerUid"] {
        let mut v = set_value();
        v["duplicators"][1]["duplicator"][field] = Value::Null;
        assert!(!decode_fails(v), "{field}");
    }
    let mut v = set_value();
    v["duplicators"][1]["duplicator"]["rows"] = json!("3");
    assert!(decode_fails(v));
    let mut v = set_value();
    v["duplicators"][0]
        .as_object_mut()
        .unwrap()
        .remove("layerUid");
    assert!(decode_fails(v));
    let one = set_value()["duplicators"][1].clone();
    for (count, ok) in [(4096usize, true), (4097, false)] {
        let mut v = set_value();
        v["duplicators"] = Value::Array(vec![one.clone(); count]);
        assert_eq!(!decode_fails(v), ok, "{count} duplicators");
    }
    let eff = json!({"radius": 1});
    for (count, ok) in [(4096usize, true), (4097, false)] {
        let mut v = set_value();
        v["duplicators"][1]["duplicator"]["effectors"] = Value::Array(vec![eff.clone(); count]);
        assert_eq!(!decode_fails(v), ok, "{count} effectors");
    }
    let uid = json!("ly_pool_a");
    let mut v = set_value();
    v["duplicators"][2]["duplicator"]["sourceLayerUids"] = Value::Array(vec![uid; 4097]);
    assert!(decode_fails(v));
}
