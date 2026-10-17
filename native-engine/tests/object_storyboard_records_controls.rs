//! Negative and mutation controls for the native StoryBoard records (L04-G).
//!
//! Each control starts from the baseline storyboard of `tests/object_storyboard_records.rs`
//! (`baseline_is_admissible` proves the unmutated record passes, so no control can be
//! vacuous), changes exactly one thing, and pins the exact error variant.
use super::tests::{storyboard_json, storyboard_value, SYMBOLS};
use super::*;
use serde_json::{json, Value};
use StoryboardRecordError::{DuplicateId, Invalid, Unresolved, Unsupported};

type Outcome = Result<(), StoryboardRecordError>;

fn run(edit: impl FnOnce(&mut Value)) -> Outcome {
    let mut value = storyboard_value();
    edit(&mut value);
    let record: StoryboardRecord =
        serde_json::from_value(value).expect("the mutation must still decode");
    record.validate(&SYMBOLS)
}
fn at(v: &Value, id: &str) -> usize {
    v["modules"]
        .as_array()
        .unwrap()
        .iter()
        .position(|m| m["id"] == id)
        .expect("module")
}
fn set_module(id: &'static str, field: &'static str, value: Value) -> impl FnOnce(&mut Value) {
    move |v: &mut Value| {
        let i = at(v, id);
        v["modules"][i][field] = value;
    }
}
fn drop_module_field(id: &'static str, field: &'static str) -> impl FnOnce(&mut Value) {
    move |v: &mut Value| {
        let i = at(v, id);
        v["modules"][i].as_object_mut().unwrap().remove(field);
    }
}
fn decode_fails(value: Value) -> bool {
    serde_json::from_value::<StoryboardRecord>(value).is_err()
}

#[test]
fn baseline_is_admissible() {
    assert_eq!(run(|_| {}), Ok(()));
}

#[test]
fn header_identity_and_type_rules() {
    assert!(matches!(
        run(|v| v["format"] = json!("nemo.other")),
        Err(Unsupported(_))
    ));
    assert!(matches!(
        run(|v| v["formatVersion"] = json!(2)),
        Err(Unsupported(_))
    ));
    assert_eq!(
        run(|v| {
            let copy = v["modules"][1].clone();
            v["modules"].as_array_mut().unwrap().push(copy);
        }),
        Err(DuplicateId("duplicate module id"))
    );
    assert!(matches!(
        run(set_module("sbm10", "id", json!(""))),
        Err(Invalid(_))
    ));
    assert!(matches!(
        run(set_module("sbm10", "type", json!("folder"))),
        Err(Unsupported(_))
    ));
    assert!(matches!(
        run(set_module("sbm10", "type", json!("Instance"))),
        Err(Unsupported(_))
    ));
}

#[test]
fn a_module_carries_only_the_fields_of_its_own_type() {
    let other = Err(Invalid("module carries a field of another module type"));
    assert_eq!(run(set_module("sbm10", "chain", json!([]))), other);
    assert_eq!(run(set_module("sbm10", "name", json!("n"))), other);
    assert_eq!(run(set_module("sbm10", "assetId", json!("a"))), other);
    assert_eq!(
        run(set_module("sbm9", "symbolId", json!("sym_comp"))),
        other
    );
    assert_eq!(run(set_module("sbm9", "trimIn", json!(0))), other);
    assert_eq!(run(set_module("sbm9", "duration", json!(4))), other);
    assert_eq!(run(set_module("sbm9", "assetId", json!("a"))), other);
    assert_eq!(run(set_module("sbm12", "trimIn", json!(0))), other);
    assert_eq!(run(set_module("sbm12", "playhead", json!(0))), other);
    assert_eq!(run(set_module("sbm12", "audio", json!([]))), other);
    assert_eq!(
        run(set_module("sbm12", "symbolId", json!("sym_comp"))),
        other
    );
}

#[test]
fn required_fields_and_references() {
    assert_eq!(
        run(drop_module_field("sbm10", "symbolId")),
        Err(Invalid("an instance module needs a symbolId"))
    );
    assert_eq!(
        run(set_module("sbm10", "symbolId", json!("sym_ghost"))),
        Err(Unresolved("instance module names a missing symbol"))
    );
    assert_eq!(
        run(drop_module_field("sbm12", "assetId")),
        Err(Invalid("a sound needs an assetId"))
    );
    assert!(matches!(
        run(set_module("sbm12", "assetId", json!(""))),
        Err(Invalid(_))
    ));
    assert!(matches!(
        run(set_module("sbm12", "assetId", json!("a".repeat(256)))),
        Err(Invalid(_))
    ));
    assert_eq!(
        run(set_module("sbm12", "assetId", json!("a".repeat(255)))),
        Ok(())
    );
    // a montage needs nothing but its identity
    assert_eq!(run(drop_module_field("sbm14", "chain")), Ok(()));
    assert_eq!(run(drop_module_field("sbm14", "audio")), Ok(()));
}

#[test]
fn retime_must_keep_the_chain_formula_defined() {
    let msg = Err(Invalid(
        "retime needs duration > 0 and 0 <= trimIn <= trimOut",
    ));
    assert_eq!(run(set_module("sbm10", "duration", json!(0))), msg);
    assert_eq!(run(set_module("sbm10", "duration", json!(-4))), msg);
    assert_eq!(run(set_module("sbm10", "trimIn", json!(-1))), msg);
    assert_eq!(
        run(set_module("sbm10", "trimOut", json!(1))),
        msg,
        "trimOut < trimIn (2)"
    );
    assert_eq!(
        run(set_module("sbm11", "trimOut", json!(-1))),
        msg,
        "trimOut < the default trimIn 0"
    );
    assert_eq!(
        run(set_module("sbm10", "trimOut", json!(2))),
        Ok(()),
        "a one-frame source range"
    );
    assert_eq!(run(set_module("sbm10", "duration", json!(0.5))), Ok(()));
    assert_eq!(run(set_module("sbm10", "duration", json!(1000))), Ok(()));
}

#[test]
fn the_id_counter_must_exceed_every_allocated_id() {
    // the baseline holds nextId 15 and the highest allocated id sbm14
    assert_eq!(run(|v| v["nextId"] = json!(15)), Ok(()));
    let msg = Err(Invalid("nextId must exceed every allocated module id"));
    assert_eq!(run(|v| v["nextId"] = json!(14)), msg);
    assert_eq!(run(|v| v["nextId"] = json!(1)), msg);
    assert_eq!(run(|v| v["nextId"] = json!(0)), msg);
    // only `sbm<digits>` is an allocator spelling
    let renamed = |id: &'static str| {
        move |v: &mut Value| {
            let i = at(v, "sbm14");
            v["modules"][i]["id"] = json!(id);
            v["activeMontageId"] = json!(id);
        }
    };
    assert_eq!(run(renamed("sbm15")), msg);
    assert_eq!(run(renamed("sbm14x")), Ok(()));
    assert_eq!(run(renamed("SBM99")), Ok(()));
    assert_eq!(run(renamed("sbm")), Ok(()));
    assert_eq!(run(renamed("mod-7")), Ok(()));
}

#[test]
fn active_montage_must_name_a_montage() {
    assert_eq!(
        run(|v| v["activeMontageId"] = json!("sbm10")),
        Err(Unresolved("activeMontageId names no montage module"))
    );
    assert_eq!(
        run(|v| v["activeMontageId"] = json!("sbm12")),
        Err(Unresolved("activeMontageId names no montage module"))
    );
    assert_eq!(
        run(|v| v["activeMontageId"] = json!("ghost")),
        Err(Unresolved("activeMontageId names no montage module"))
    );
    assert_eq!(run(|v| v["activeMontageId"] = json!("sbm9")), Ok(()));
    assert_eq!(run(|v| v["activeMontageId"] = Value::Null), Ok(()));
}

#[test]
fn chain_and_audio_membership_rules() {
    let chain_dup = Err(DuplicateId("chain entry is empty or listed twice"));
    assert_eq!(
        run(set_module("sbm9", "chain", json!(["sbm10", "sbm10"]))),
        chain_dup
    );
    assert_eq!(
        run(set_module("sbm14", "chain", json!(["sbm10"]))),
        chain_dup,
        "already in the sbm9 chain"
    );
    assert_eq!(
        run(set_module("sbm14", "chain", json!(["sbm13", ""]))),
        chain_dup
    );
    assert_eq!(run(set_module("sbm14", "chain", json!([]))), Ok(()));
    let bad_join = Err(Invalid(
        "audio join needs a moduleId and a finite offset >= 0",
    ));
    assert_eq!(
        run(set_module(
            "sbm9",
            "audio",
            json!([{"moduleId": "sbm12", "offsetFrames": -1}])
        )),
        bad_join
    );
    assert_eq!(
        run(set_module(
            "sbm9",
            "audio",
            json!([{"moduleId": "", "offsetFrames": 0}])
        )),
        bad_join
    );
    assert_eq!(
        run(set_module(
            "sbm9",
            "audio",
            json!([{"moduleId": "sbm10", "offsetFrames": 0}])
        )),
        Err(Invalid("audio join names a module that is not a sound"))
    );
    assert_eq!(
        run(set_module(
            "sbm14",
            "audio",
            json!([{"moduleId": "sbm12", "offsetFrames": 0}])
        )),
        Err(DuplicateId("sound module joined twice"))
    );
    assert_eq!(
        run(set_module(
            "sbm9",
            "audio",
            json!([{"moduleId": "sbm12", "offsetFrames": 2.5}])
        )),
        Ok(())
    );
    assert_eq!(
        run(set_module(
            "sbm9",
            "audio",
            json!([{"moduleId": "sbm12", "offsetFrames": 0}])
        )),
        Ok(())
    );
}

#[test]
fn decoding_is_object_only_strict_and_bounded() {
    let unknown = |path: &[&str]| {
        let mut v = storyboard_value();
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
        vec!["pan"],
        vec!["modules", "0"],
        vec!["modules", "0", "audio", "0"],
        vec!["modules", "3"],
    ] {
        assert!(decode_fails(unknown(&path)), "{path:?}");
    }
    let text = storyboard_json();
    assert!(serde_json::from_str::<StoryboardRecord>(&text.replacen(
        "\"nextId\":15",
        "\"nextId\":15,\"nextId\":15",
        1
    ))
    .is_err());
    let positional = text.replace(
        "{\"moduleId\":\"sbm12\",\"offsetFrames\":3}",
        "[\"sbm12\",3]",
    );
    assert_ne!(positional, text);
    assert!(serde_json::from_str::<StoryboardRecord>(&positional).is_err());
    let mut v = storyboard_value();
    v["modules"][1].as_object_mut().unwrap().remove("type");
    assert!(decode_fails(v));
    let mut v = storyboard_value();
    v["modules"][1].as_object_mut().unwrap().remove("x");
    assert!(decode_fails(v));
    let mut v = storyboard_value();
    v.as_object_mut().unwrap().remove("nextId");
    assert!(decode_fails(v));
    for (field, value) in [
        ("trimIn", Value::Null),
        ("duration", json!("16")),
        ("id", json!(7)),
        ("x", Value::Null),
    ] {
        let mut v = storyboard_value();
        v["modules"][1][field] = value.clone();
        assert!(decode_fails(v), "{field} {value}");
    }
    let mut v = storyboard_value();
    v["nextId"] = json!(-1);
    assert!(decode_fails(v));
    let one = storyboard_value()["modules"][2].clone();
    for (count, ok) in [(4096usize, true), (4097, false)] {
        let mut v = storyboard_value();
        v["modules"] = Value::Array(vec![one.clone(); count]);
        assert_eq!(!decode_fails(v), ok, "{count} modules");
    }
}
