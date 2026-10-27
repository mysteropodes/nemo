//! G21: fixtures are emitted by the pinned original production editor, never this kernel.
use crate::object_document::subselect::{edit, Anchor, Intent, Refusal};
use serde_json::Value;
fn oracle() -> Value {
    serde_json::from_str(include_str!("fixtures/subselect-baseline/oracle.json")).unwrap()
}
#[test]
fn subselect_matches_all_original_point_type_toggle_transform_and_insert_cases() {
    let oracle = oracle();
    assert_eq!(oracle["cases"].as_array().unwrap().len(), 220);
    for case in oracle["cases"].as_array().unwrap() {
        let source: Vec<Anchor> = serde_json::from_value(case["segments"].clone()).unwrap();
        let expected: Vec<Anchor> = serde_json::from_value(case["expected"].clone()).unwrap();
        let intent: Intent = serde_json::from_value(case["intent"].clone()).unwrap();
        let actual = edit(
            &source,
            case["closed"].as_bool().unwrap(),
            case["pressure"].as_bool().unwrap(),
            &intent,
        )
        .unwrap();
        assert_eq!(actual.len(), expected.len(), "{case}");
        for (a, b) in actual.iter().zip(expected.iter()) {
            for (x, y) in a
                .point
                .iter()
                .chain(&a.handle_in)
                .chain(&a.handle_out)
                .zip(b.point.iter().chain(&b.handle_in).chain(&b.handle_out))
            {
                assert!((x - y).abs() < 1e-6, "{case}: {actual:?} != {expected:?}");
            }
        }
    }
}
#[test]
fn subselect_refuses_invalid_selection_and_limits_without_changing_input() {
    let source: Vec<Anchor> =
        serde_json::from_value(oracle()["cases"][0]["segments"].clone()).unwrap();
    let before = source.clone();
    for intent in [
        Intent::Move {
            indices: vec![1, 1],
            delta: [1.0, 2.0],
        },
        Intent::Delete {
            indices: vec![0, 1],
        },
        Intent::Handle {
            index: 99,
            handle: "handleIn".into(),
            point: [1.0, 2.0],
            break_pair: false,
            paired_point: Default::default(),
        },
        Intent::Insert {
            curve: 0,
            time: f64::NAN,
        },
        Intent::Move {
            indices: vec![0],
            delta: [f64::INFINITY, 0.0],
        },
    ] {
        assert_eq!(edit(&source, false, false, &intent), Err(Refusal::Invalid));
    }
    assert_eq!(source, before);
    assert_eq!(
        edit(
            &vec![source[0].clone(); 256],
            false,
            false,
            &Intent::Insert {
                curve: 0,
                time: 0.5
            }
        ),
        Err(Refusal::Limit)
    );
}
#[test]
fn subselect_delete_preserves_order_and_handles() {
    let mut source: Vec<Anchor> =
        serde_json::from_value(oracle()["cases"][0]["segments"].clone()).unwrap();
    source.push(Anchor {
        point: [200.0, 30.0],
        handle_in: [-9.0, 2.0],
        handle_out: [1.0, 3.0],
    });
    assert_eq!(
        edit(
            &source,
            false,
            false,
            &Intent::Delete {
                indices: vec![2, 0]
            }
        )
        .unwrap(),
        vec![source[1].clone(), source[3].clone()]
    );
}

#[test]
fn subselect_modifier_intents_match_frozen_pointer_history_at_every_step() {
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/subselect-baseline/modifiers.json")).unwrap();
    assert_eq!(oracle["cases"].as_array().unwrap().len(), 45);
    for row in oracle["cases"].as_array().unwrap() {
        let source: Vec<Anchor> = serde_json::from_value(row["segments"].clone()).unwrap();
        for step in row["steps"].as_array().unwrap() {
            let expected: Vec<Anchor> = serde_json::from_value(step["expected"].clone()).unwrap();
            let intent: Intent = serde_json::from_value(step["intent"].clone()).unwrap();
            let actual = edit(&source, true, false, &intent).unwrap();
            for (a, b) in actual.iter().zip(&expected) {
                for (x, y) in a
                    .point
                    .iter()
                    .chain(&a.handle_in)
                    .chain(&a.handle_out)
                    .zip(b.point.iter().chain(&b.handle_in).chain(&b.handle_out))
                {
                    assert!((x - y).abs() < 1e-9, "{}: {step}", row["name"]);
                }
            }
        }
    }
    let source: Vec<Anchor> =
        serde_json::from_value(oracle["cases"][0]["segments"].clone()).unwrap();
    for paired_point in [
        crate::object_document::Presence::Present([1.0, 2.0]),
        crate::object_document::Presence::Present([f64::NAN, 0.0]),
    ] {
        let intent = Intent::Handle {
            index: 0,
            handle: "handleOut".into(),
            point: [30.0, 40.0],
            break_pair: false,
            paired_point,
        };
        assert_eq!(edit(&source, true, false, &intent), Err(Refusal::Invalid));
    }
}

#[test]
fn subselect_pressure_centerlines_match_frozen_production_edits() {
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/subselect-baseline/pressure.json")).unwrap();
    assert_eq!(
        oracle["baseline"],
        "46442ea96c09e6bfaadf956eb8586e273a658085"
    );
    assert_eq!(oracle["cases"].as_array().unwrap().len(), 180);
    let anchors = |center: &Value| -> Vec<Anchor> {
        center
            .as_array()
            .unwrap()
            .iter()
            .map(|segment| Anchor {
                point: serde_json::from_value(segment["point"].clone()).unwrap(),
                handle_in: serde_json::from_value(segment["handleIn"].clone()).unwrap(),
                handle_out: serde_json::from_value(segment["handleOut"].clone()).unwrap(),
            })
            .collect()
    };
    for case in oracle["cases"].as_array().unwrap() {
        let source = anchors(&case["before"]["center"]);
        let before = source.clone();
        let expected = anchors(&case["expected"]["center"]);
        let intent: Intent = serde_json::from_value(case["intent"].clone()).unwrap();
        // Pass the painted closure flag: original toggleTangentAt wraps endpoint
        // neighbours on a closed ribbon. Other pressure operations use an open
        // centerline. Width/metadata retention and outline reconstruction remain
        // obligations of the pending record/paint port, outside this kernel check.
        let actual = edit(
            &source,
            case["before"]["closed"].as_bool().unwrap(),
            true,
            &intent,
        )
        .unwrap();
        if case["name"] == "line"
            && case["profile"].is_null()
            && matches!(intent, Intent::Toggle { index: 0 })
        {
            assert_ne!(
                edit(&source, false, true, &intent).unwrap(),
                expected,
                "forcing open neighbours would erase the original endpoint-toggle quirk"
            );
        }
        assert_eq!(source, before, "input mutated: {}", case["name"]);
        assert_eq!(actual.len(), expected.len());
        for (index, (a, b)) in actual.iter().zip(&expected).enumerate() {
            for (x, y) in a
                .point
                .iter()
                .chain(&a.handle_in)
                .chain(&a.handle_out)
                .zip(b.point.iter().chain(&b.handle_in).chain(&b.handle_out))
            {
                assert!(
                    (x - y).abs() < 1e-9,
                    "{} / {} / anchor {index}: {a:?} != {b:?}",
                    case["name"],
                    case["intent"]
                );
            }
        }
    }
}
