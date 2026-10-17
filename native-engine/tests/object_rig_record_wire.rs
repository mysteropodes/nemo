//! Admitted-oddity controls and malformed-wire tables of the rig record (L03-N, I11): what admission
//! must NOT refuse because the baseline accepts it, and what can never decode at all.

use super::flaw_tests::{
    bind, bone, ik, rig_text, twin, twin_bones, verdict, with_binds, with_bones, with_ik, S1,
    WEIGHT,
};
use super::RigRecord;

#[test]
fn things_the_baseline_accepts_stay_admitted_and_authored() {
    let two_s1 = [bind(S1, 3, 3, None, WEIGHT), bind(S1, 3, 3, None, "")];
    let odd: Vec<(&str, String)> = vec![
        // old saves hold duplicate binds for one shape; applyRigDeform sums them, so they are data
        ("duplicate bind targets", with_binds(&two_s1)),
        (
            "bind to a stroke that does not exist anywhere",
            with_binds(&[bind(r#""strokeId":"nope","#, 3, 3, None, WEIGHT)]),
        ),
        (
            "weight naming a missing bone",
            with_binds(&[bind(
                S1,
                3,
                3,
                None,
                r#"{"boneId":"gone","ptIndex":0,"w":1}"#,
            )]),
        ),
        (
            "weight naming a control point beyond the bone",
            with_binds(&[bind(
                S1,
                3,
                3,
                None,
                r#"{"boneId":"bone1","ptIndex":9,"w":1}"#,
            )]),
        ),
        (
            "weight with both ptIndex and offset",
            with_binds(&[bind(
                S1,
                3,
                3,
                None,
                r#"{"boneId":"bone1","ptIndex":0,"offset":4,"w":1}"#,
            )]),
        ),
        (
            "weight outside 0..1",
            with_binds(&[bind(
                S1,
                3,
                3,
                None,
                r#"{"boneId":"bone1","offset":4,"w":-2.5}"#,
            )]),
        ),
        (
            "ik root on the last vertex",
            with_ik(&[ik(("bone1", 1), ("bone1", 0), ("bone2", 1))]),
        ),
        (
            "ik chain whose three refs are one vertex",
            with_ik(&[ik(("bone1", 0), ("bone1", 0), ("bone1", 0))]),
        ),
        (
            "bone ids that are not boneN",
            with_bones(&[
                bone("arm", 2, 2, ""),
                bone("bone", 2, 2, ""),
                bone("boneX", 2, 2, ""),
                bone("Bone9", 2, 2, ""),
            ]),
        ),
        (
            "bone0 and bone01 are below nextId 2",
            rig_text(
                &[bone("bone0", 2, 2, ""), bone("bone01", 2, 2, "")],
                &[],
                "2",
                &[],
            ),
        ),
        (
            "nextId at the largest safe integer",
            rig_text(&twin_bones(), &[], "9007199254740991", &[]),
        ),
        (
            "bone radius zero and negative",
            with_bones(&[
                bone("bone1", 2, 2, ",\"radius\":0"),
                bone("bone2", 2, 2, ",\"radius\":-3"),
            ]),
        ),
        (
            "live pose longer than the rest pose",
            with_bones(&[bone("bone1", 4, 2, "")]),
        ),
        (
            "mesh bind",
            with_binds(&[bind(r#""meshId":"im_1","#, 9, 9, None, WEIGHT)]),
        ),
        (
            "empty weight lists",
            with_binds(&[bind(S1, 2, 2, Some(2), "")]),
        ),
    ];
    for (name, text) in &odd {
        assert_eq!(verdict(text), Ok(()), "{name}");
        let record: RigRecord = serde_json::from_str(text).unwrap();
        assert_eq!(
            &serde_json::to_string(&record).unwrap(),
            text,
            "{name}: kept as authored"
        );
    }
    let record: RigRecord = serde_json::from_str(&odd[0].1).unwrap();
    assert_eq!(record.binds.len(), 2, "duplicates are not merged");
    assert_eq!(record.binds[0].weights[0].len(), 1);
    assert!(record.binds[1].weights[0].is_empty());
}

#[test]
fn malformed_wire_shapes_never_decode() {
    let t = twin();
    let swap = |from: &str, to: &str| {
        assert!(t.contains(from), "twin lacks {from}");
        t.replacen(from, to, 1)
    };
    let cases: Vec<(&str, String)> = vec![
        ("nextId fractional", swap("\"nextId\":3", "\"nextId\":1.5")),
        ("nextId negative", swap("\"nextId\":3", "\"nextId\":-1")),
        ("nextId string", swap("\"nextId\":3", "\"nextId\":\"3\"")),
        ("nextId null", swap("\"nextId\":3", "\"nextId\":null")),
        (
            "nextId twice",
            swap("\"nextId\":3", "\"nextId\":3,\"nextId\":4"),
        ),
        ("binds missing", swap(",\"binds\":[", ",\"x\":[")),
        (
            "bones as a list",
            swap("\"bones\":{", "\"bones\":[{").replacen("},\"ikChains\"", "}],\"ikChains\"", 1),
        ),
        (
            "ikChains null",
            swap("\"ikChains\":{", "\"ikChains\":null,\"y\":{"),
        ),
        (
            "unknown rig member",
            swap("\"nextId\":3", "\"nextId\":3,\"pose\":{}"),
        ),
        (
            "unknown bone member",
            swap("\"closed\":false}", "\"closed\":false,\"label\":\"x\"}"),
        ),
        (
            "point with three numbers",
            swap(
                "[0,0]},{\"point\":[10,0]}],\"restSegments\"",
                "[0,0,0]},{\"point\":[10,0]}],\"restSegments\"",
            ),
        ),
        (
            "point with one number",
            swap("\"point\":[0,0]", "\"point\":[0]"),
        ),
        (
            "point as a string",
            swap("\"point\":[0,0]", "\"point\":\"0,0\""),
        ),
        (
            "segment without a point",
            swap("{\"point\":[0,0]}", "{\"handleIn\":[0,0]}"),
        ),
        (
            "segment with an unknown member",
            swap("{\"point\":[0,0]}", "{\"point\":[0,0],\"mirror\":true}"),
        ),
        ("ik vi negative", swap("\"vi\":0", "\"vi\":-1")),
        ("ik vi fractional", swap("\"vi\":0", "\"vi\":0.5")),
        ("ik flip numeric", swap("\"flip\":false", "\"flip\":0")),
        ("ik l1 null", swap("\"l1\":10", "\"l1\":null")),
        (
            "ik unknown member",
            swap("\"flip\":false", "\"flip\":false,\"angle\":0"),
        ),
        (
            "bind rotate null",
            swap("\"rotate\":true", "\"rotate\":null"),
        ),
        ("bind rotate missing", swap(",\"rotate\":true", "")),
        (
            "bind strokeId null",
            swap("\"strokeId\":\"s1\"", "\"strokeId\":null"),
        ),
        (
            "bind _live",
            swap("\"rotate\":true", "\"rotate\":true,\"_live\":{}"),
        ),
        (
            "bind _liveRaster",
            swap("\"rotate\":true", "\"rotate\":true,\"_liveRaster\":{}"),
        ),
        (
            "bind centerRest",
            swap("\"rotate\":true", "\"rotate\":true,\"centerRest\":[]"),
        ),
        (
            "bind centerWeights",
            swap("\"rotate\":true", "\"rotate\":true,\"centerWeights\":[]"),
        ),
        (
            "weights as objects",
            swap("\"weights\":[[", "\"weights\":[{},["),
        ),
        (
            "weight ptIndex null",
            swap("\"ptIndex\":0", "\"ptIndex\":null"),
        ),
        (
            "weight ptIndex negative",
            swap("\"ptIndex\":0", "\"ptIndex\":-1"),
        ),
        (
            "weight offset null",
            swap(
                "\"ptIndex\":0,\"w\":1}",
                "\"ptIndex\":0,\"offset\":null,\"w\":1}",
            ),
        ),
        ("weight w string", swap("\"w\":1}", "\"w\":\"1\"}")),
        (
            "weight boneId number",
            swap(
                "\"boneId\":\"bone1\",\"ptIndex\"",
                "\"boneId\":1,\"ptIndex\"",
            ),
        ),
        (
            "weight unknown member",
            swap("\"w\":1}", "\"w\":1,\"id\":1}"),
        ),
        (
            "rest point of three numbers",
            swap("\"rest\":[[0,0]", "\"rest\":[[0,0,1]"),
        ),
    ];
    for (name, text) in &cases {
        assert_ne!(text, &t, "{name}");
        assert!(serde_json::from_str::<RigRecord>(text).is_err(), "{name}");
    }
    // a value that is not an object at all
    for text in ["[]", "null", "3", "\"rig\"", "{}"] {
        assert!(serde_json::from_str::<RigRecord>(text).is_err(), "{text}");
    }
}

#[test]
fn non_finite_numbers_cannot_enter_the_wire_or_a_number() {
    assert!(
        serde_json::from_str::<RigRecord>(&twin().replacen("\"l1\":10", "\"l1\":1e999", 1))
            .is_err()
    );
    assert!(serde_json::from_str::<RigRecord>(&twin().replacen(
        "\"nextId\":3",
        "\"nextId\":NaN",
        1
    ))
    .is_err());
    assert!(
        serde_json::Number::from_f64(f64::NAN).is_none()
            && serde_json::Number::from_f64(f64::INFINITY).is_none()
    );
}
