//! G21: fixtures are emitted by the pinned original production editor, never this kernel.
use crate::object_document::subselect::{edit, Anchor, Intent, Refusal};
use serde_json::Value;
fn oracle() -> Value {
    serde_json::from_str(include_str!("fixtures/subselect-baseline/oracle.json")).unwrap()
}
#[test]
fn subselect_matches_all_original_point_type_toggle_transform_and_insert_cases() {
    let oracle = oracle();
    assert_eq!(oracle["cases"].as_array().unwrap().len(), 196);
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
