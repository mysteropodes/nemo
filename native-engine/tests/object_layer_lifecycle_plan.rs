//! Oracle replay for the layer lifecycle plan kernel (F07 + F08, L02-E).
//!
//! `fixtures/object-layer-lifecycle-plan/oracle.txt` (`ORACLE`, 569 JSON lines)
//! was produced by running the verbatim `reorderLayersAtGap`, `uniqueLayerName`,
//! `createUserLayer` and `activateUL` of `src/js/app.js` and `deleteLayer`,
//! `duplicateLayer` (with `dupOne`), `renameLayer`, `layerColorTargets` and
//! `applyLayerColor` of `src/js/timeline.js` at fbe4b8a3 in a Node `vm` context
//! (generator `tools/gen_oracle.js`, stubs only record calls). The baseline is
//! index-addressed: layer `i` is the native layer `L<i>`, so rows map by position.
//!
//! Fixture document (`build`, in `object_layer_lifecycle_plan_fixture.rs`): layer `L<i>` owns the stroke `s<i>` at frame 0, odd
//! layers also at frame 1; even layers own a layer-level holder, every third layer
//! an element-level holder with a control; `L<i>` has natural slot `i` in the
//! `scene-root` order context and `n-1-i` in the second context (`unused-symbol`),
//! odd layers carry a static Order of 9. Every row also checks that objects,
//! holders and order entries moved with their layer and that the candidate passes
//! the document's storage admission (`validate_storage`, run here as the
//! transaction leaf will run it).

use super::fixture::{admitted, build, json_of, layer_uids, plain, slot_of, text_of, uid};
use super::{
    plan_delete, plan_reorder, LayerSelection, LifecycleError, UnavailableLayerEditPolicy,
};
use serde_json::Value;

const ORACLE: &str = include_str!("fixtures/object-layer-lifecycle-plan/oracle.txt");

pub(crate) fn rows(kind: &str) -> Vec<Value> {
    ORACLE
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("oracle row"))
        .filter(|row| row["k"] == kind)
        .collect()
}

pub(crate) fn ints(value: &Value) -> Vec<i64> {
    value
        .as_array()
        .expect("array")
        .iter()
        .map(|v| v.as_i64().expect("int"))
        .collect()
}

pub(crate) fn texts(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("array")
        .iter()
        .map(|v| v.as_str().expect("text").to_string())
        .collect()
}

pub(crate) fn usizes(value: &Value) -> Vec<usize> {
    ints(value).into_iter().map(|v| v as usize).collect()
}

fn selection_of(input: &Value) -> LayerSelection {
    LayerSelection {
        active: uid(input["active"].as_i64().expect("active")),
        selected: ints(&input["sel"]).into_iter().map(uid).collect(),
        anchor: input["anchor"]
            .as_i64()
            .filter(|index| *index >= 0)
            .map(uid),
    }
}

#[test]
fn reorder_replays_every_oracle_row() {
    let mut checked = 0;
    for row in rows("reorder") {
        let (input, out) = (&row["in"], &row["o"]);
        let count = input["n"].as_u64().expect("n") as usize;
        let before = build(&plain(count), 2);
        let gap = input["gap"].as_f64().expect("gap");
        let plan = plan_reorder(
            &before,
            &ints(&input["moving"]),
            gap,
            &selection_of(input),
            &UnavailableLayerEditPolicy,
        )
        .unwrap_or_else(|error| panic!("{error:?} for {row}"));
        assert!(admitted(&plan.document), "{row}");
        let order = texts(&out["order"]);
        assert_eq!(layer_uids(&plan.document), order, "{row}");
        assert_eq!(
            plan.changed,
            out["changed"].as_bool().expect("changed"),
            "{row}"
        );
        assert_eq!(plan.changed, out["undo"].as_u64() == Some(1), "{row}");
        let selection = plan.selection.as_ref().expect("selection");
        assert_eq!(
            selection.active as i64,
            out["active"].as_i64().expect("active"),
            "{row}"
        );
        assert_eq!(selection.selected, usizes(&out["sel"]), "{row}");
        assert_eq!(
            selection.anchor.map_or(-1, |slot| slot as i64),
            out["anchor"].as_i64().expect("anchor"),
            "{row}"
        );
        for (index, name) in order.iter().enumerate() {
            assert_eq!(slot_of(&plan.document, 0, name), index as u32, "{row}");
            assert_eq!(
                slot_of(&plan.document, 1, name),
                (count - 1 - index) as u32,
                "{row}"
            );
        }
        assert_eq!(
            json_of(&plan.document)["objects"],
            json_of(&before)["objects"],
            "{row}"
        );
        assert_eq!(
            json_of(&plan.document)["propertyHolders"],
            json_of(&before)["propertyHolders"],
            "{row}"
        );
        if !plan.changed {
            assert_eq!(text_of(&plan.document), text_of(&before), "{row}");
        }
        checked += 1;
    }
    assert_eq!(checked, 358);
}

#[test]
fn delete_replays_every_oracle_row() {
    let mut checked = 0;
    for row in rows("delete") {
        let (input, out) = (&row["in"], &row["o"]);
        let count = input["n"].as_u64().expect("n") as usize;
        let before = build(&plain(count), 2);
        let selection = LayerSelection {
            active: uid(input["active"].as_i64().expect("active")),
            selected: ints(&input["sel"]).into_iter().map(uid).collect(),
            anchor: None,
        };
        let result = plan_delete(&before, &selection, &UnavailableLayerEditPolicy);
        if out["refused"] == "last" {
            assert_eq!(
                result.expect_err("refusal"),
                LifecycleError::LastLayer,
                "{row}"
            );
            checked += 1;
            continue;
        }
        let plan = result.unwrap_or_else(|error| panic!("{error:?} for {row}"));
        let kept = texts(&out["kept"]);
        assert_eq!(layer_uids(&plan.document), kept, "{row}");
        assert!(plan.changed && admitted(&plan.document), "{row}");
        let outcome = plan.selection.as_ref().expect("selection");
        assert_eq!(
            outcome.active as i64,
            out["active"].as_i64().expect("active"),
            "{row}"
        );
        assert_eq!(outcome.selected, usizes(&out["sel"]), "{row}");
        assert_eq!(
            outcome.anchor.map(|slot| slot as i64),
            out["anchor"].as_i64(),
            "{row}"
        );
        assert_eq!(plan.removed.len() + kept.len(), count, "{row}");
        let covers_all = input["sel"].as_array().expect("sel").len() == count;
        assert_eq!(plan.spared, covers_all.then(|| "L0".to_string()), "{row}");
        let kept_ref: Vec<&str> = kept.iter().map(String::as_str).collect();
        let belongs = |value: &Value| kept_ref.contains(&value["layerUid"].as_str().expect("uid"));
        let original = json_of(&before);
        let wanted = |key: &str| -> Value {
            let all = original[key].as_array().expect("array");
            Value::Array(
                all.iter()
                    .filter(|item| belongs(&item["target"]))
                    .cloned()
                    .collect(),
            )
        };
        let after = json_of(&plan.document);
        assert_eq!(after["objects"], wanted("objects"), "{row}");
        assert_eq!(after["propertyHolders"], wanted("propertyHolders"), "{row}");
        for (index, name) in kept.iter().enumerate() {
            assert_eq!(slot_of(&plan.document, 0, name), index as u32, "{row}");
            assert_eq!(
                slot_of(&plan.document, 1, name),
                (kept.len() - 1 - index) as u32,
                "{row}"
            );
        }
        let plan_json = &after["compositionOrder"]["contexts"];
        for context in plan_json.as_array().expect("contexts") {
            assert_eq!(
                context["layers"].as_array().expect("layers").len(),
                kept.len(),
                "{row}"
            );
            for scope in context["scopes"].as_array().expect("scopes") {
                assert!(
                    scope["elements"]
                        .as_array()
                        .expect("elements")
                        .iter()
                        .all(|e| belongs(e)),
                    "{row}"
                );
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 167);
}
