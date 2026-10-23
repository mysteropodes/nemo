//! L01-G (G13): the sealed path-effect stack edit through the real dispatch caller, the existing history and the
//! existing revision owner, and the draw/edit guard parity with the baseline (`editRefusalReason`, tools.js 1817).
//! Evaluated vectors are the oracle's `fx` rows (path-fx.js executed at fbe4b8a3), reached through the L01-H join.
use super::support::*;
use super::vsupport::{self, assert_unavailable, bytes, document, owner_of, seeds, state, target};
use crate::history::NativeObjectHistory;
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

fn fresh() -> NativeObjectHistory {
    owner_of(&document(1, seeds(), None))
}

/// Dispatch a stack, expect `code`, and prove nothing moved (including an exact replay of the refusal).
fn assert_fx_refused(owner: &mut NativeObjectHistory, id: &str, payload: Value, code: &str) {
    let revision = owner.content_revision();
    let before = bytes(owner);
    let depths = owner.history_depths();
    let envelope = request(owner, id, OP_FX, payload);
    let response = vsupport::dispatch(owner, &envelope);
    assert_eq!(refusal(&response), code, "{id}: {response}");
    assert_eq!(
        vsupport::dispatch(owner, &envelope),
        response,
        "{id}: exact replay"
    );
    assert_eq!(owner.content_revision(), revision, "{id}");
    assert_eq!(owner.history_depths(), depths, "{id}");
    assert_eq!(bytes(owner), before, "{id}");
}

#[test]
fn set_stores_the_complete_ordered_stack_exactly_as_sent() {
    let mut owner = fresh();
    let before = state(&owner);
    assert!(before["layers"][0].get("pathFx").is_none());
    let stack = json!([
        {"type": "zigzag", "enabled": true, "p1": 12, "p2": 4, "p3": 0},
        {"type": "roughen", "p1": null, "p2": 6.5},
        {"type": "zigzag", "enabled": false}
    ]);
    let response = send(&mut owner, "fx", OP_FX, effects("L1", stack.clone()));
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(response["contentRevision"], 1);
    assert_eq!(
        response["result"],
        json!({"applied": true, "historyEntriesAdded": 1})
    );
    let after = state(&owner);
    // Order, enabled absent/true/false, an explicit null, an absent member and a zero all survive exactly.
    assert_eq!(after["layers"][0]["pathFx"], stack);
    assert!(after["layers"][1].get("pathFx").is_none());
    assert_eq!(
        after["objects"], before["objects"],
        "no source geometry moved"
    );
    assert_eq!(owner.history_depths(), (1, 0));
}

#[test]
fn a_reorder_is_a_real_change_and_an_identical_stack_is_a_no_op() {
    let mut owner = fresh();
    let initial = bytes(&owner);
    let first = json!([{"type": "zigzag", "p1": 12}, {"type": "roughen", "p1": 8}]);
    let swapped = json!([{"type": "roughen", "p1": 8}, {"type": "zigzag", "p1": 12}]);
    assert!(applied(&send(&mut owner, "a", OP_FX, effects("L1", first))));
    assert!(applied(&send(
        &mut owner,
        "b",
        OP_FX,
        effects("L1", swapped.clone())
    )));
    assert_eq!(
        owner.history_depths(),
        (2, 0),
        "the reorder is its own history entry"
    );
    let content = bytes(&owner);
    let same = send(&mut owner, "c", OP_FX, effects("L1", swapped));
    assert!(!applied(&same), "{same}");
    assert_eq!(same["result"]["historyEntriesAdded"], 0);
    assert_eq!(same["contentRevision"], 2);
    // 8 and 8.0 are the same number: the float form is a no-op too.
    let float_form = json!([{"type": "roughen", "p1": 8.0}, {"type": "zigzag", "p1": 12.0}]);
    assert!(!applied(&send(
        &mut owner,
        "d",
        OP_FX,
        effects("L1", float_form)
    )));
    assert_eq!(bytes(&owner), content);
    assert_eq!(owner.history_depths(), (2, 0));
    assert_eq!(owner.undo(2), Ok(3));
    assert_eq!(owner.undo(3), Ok(4));
    assert_eq!(
        bytes(&owner),
        initial,
        "two undos restore the exact initial bytes"
    );
    assert_eq!(owner.redo(4), Ok(5));
}

#[test]
fn an_empty_stack_is_authored_and_distinct_from_no_stack() {
    let mut owner = fresh();
    let initial = bytes(&owner);
    let cleared = send(&mut owner, "e", OP_FX, effects("L1", json!([])));
    assert!(applied(&cleared), "{cleared}");
    assert_eq!(state(&owner)["layers"][0]["pathFx"], json!([]));
    assert!(state(&owner)["layers"][1].get("pathFx").is_none());
    assert!(!applied(&send(
        &mut owner,
        "e2",
        OP_FX,
        effects("L1", json!([]))
    )));
    assert_eq!(owner.undo(1), Ok(2));
    assert_eq!(bytes(&owner), initial);
    assert!(state(&owner)["layers"][0].get("pathFx").is_none());
}

#[test]
fn any_invalid_entry_rejects_the_whole_request_and_keeps_the_stored_stack() {
    let mut owner = fresh();
    let stored = json!([{"type": "zigzag", "p1": 3}]);
    assert!(applied(&send(
        &mut owner,
        "seed",
        OP_FX,
        effects("L1", stored.clone())
    )));
    let cases = [
        (
            "unknown-type",
            json!([{"type": "zigzag"}, {"type": "blur"}]),
        ),
        ("string-parameter", json!([{"type": "zigzag", "p1": "12"}])),
        ("unknown-member", json!([{"type": "zigzag", "size": 12}])),
        ("missing-type", json!([{"enabled": true}])),
        ("positional-entry", json!([["zigzag"]])),
        ("enabled-null", json!([{"type": "zigzag", "enabled": null}])),
        ("not-an-array", json!({"type": "zigzag"})),
        ("null-stack", Value::Null),
    ];
    for (id, stack) in cases {
        assert_fx_refused(&mut owner, id, effects("L1", stack), "invalid_request");
    }
    assert_eq!(state(&owner)["layers"][0]["pathFx"], stored);
}

#[test]
fn unknown_layers_and_malformed_commands_refuse_before_publishing() {
    let mut owner = fresh();
    let stack = json!([{"type": "zigzag"}]);
    assert_fx_refused(
        &mut owner,
        "absent",
        effects("L9", stack.clone()),
        "not_found",
    );
    assert_fx_refused(
        &mut owner,
        "empty-uid",
        effects("", stack.clone()),
        "invalid_request",
    );
    let wrong = json!({"command": "object.path.effects.add", "layerUid": "L1", "stack": stack});
    assert_fx_refused(&mut owner, "wrong-command", wrong, "invalid_request");
    let extra =
        json!({"command": "object.path.effects.set", "layerUid": "L1", "stack": [], "frame": 3});
    assert_fx_refused(&mut owner, "extra-member", extra, "invalid_request");
    let missing = json!({"command": "object.path.effects.set", "layerUid": "L1"});
    assert_fx_refused(&mut owner, "missing-stack", missing, "invalid_request");
    let duplicate =
        br#"{"command":"object.path.effects.set","layerUid":"L1","layerUid":"L2","stack":[]}"#;
    assert_eq!(
        owner.owner.prepare_path_effects_json(duplicate).err(),
        Some(DispatchErrorCode::InvalidRequest)
    );
    let oversized = vec![b' '; 5000];
    assert_eq!(
        owner.owner.prepare_path_effects_json(&oversized).err(),
        Some(DispatchErrorCode::InvalidRequest)
    );
}

#[test]
fn editing_the_stack_never_touches_a_record_or_the_order_plan() {
    let shape = record("s-1", "L1", &straight_rect(), true, None);
    let other = record("s-2", "L2", &straight_rect(), true, None);
    let mut owner = owner_of(&document(2, vec![shape, other], None));
    let before = state(&owner);
    let stack = json!([{"type": "zigzag", "p1": 12, "p2": 4}]);
    assert!(applied(&send(
        &mut owner,
        "v2",
        OP_FX,
        effects("L1", stack.clone())
    )));
    let after = state(&owner);
    assert_eq!(after["objects"], before["objects"]);
    assert_eq!(after["compositionOrder"], before["compositionOrder"]);
    assert_eq!(after["layers"][1], before["layers"][1]);
    let mut expected = before["layers"][0].clone();
    expected["pathFx"] = stack;
    assert_eq!(
        after["layers"][0], expected,
        "only the stack member of the target layer changed"
    );
}

#[test]
fn stack_edits_do_not_depend_on_the_drawing_lock_like_the_baseline_panel() {
    let mut doc = document(1, seeds(), None);
    doc["layers"][0]["locked"] = json!(true);
    let mut owner = owner_of(&doc);
    let stack = json!([{"type": "roughen"}]);
    assert!(applied(&send(
        &mut owner,
        "locked",
        OP_FX,
        effects("L1", stack.clone())
    )));
    assert_eq!(state(&owner)["layers"][0]["pathFx"], stack);
    assert_eq!(state(&owner)["layers"][0]["locked"], true);
    // Drawing on the same layer stays refused by the lock (and by the active effect).
    assert_unavailable(
        &mut owner,
        "still-locked",
        vsupport::replace(vsupport::record_at("D2", "L1", 0, "s-a")),
    );
}

#[test]
fn drawing_edits_refuse_on_a_layer_with_an_active_effect_and_work_again_when_it_is_cleared() {
    let cases = [
        ("active", json!([{"type": "zigzag"}]), true),
        (
            "enabled-true",
            json!([{"type": "roughen", "enabled": true}]),
            true,
        ),
        (
            "mixed",
            json!([{"type": "zigzag", "enabled": false}, {"type": "roughen"}]),
            true,
        ),
        (
            "disabled-only",
            json!([{"type": "zigzag", "enabled": false}, {"type": "roughen", "enabled": false}]),
            false,
        ),
        ("empty", json!([]), false),
    ];
    for (name, stack, refuses) in cases {
        let operations = [
            (
                "create",
                vsupport::create(vsupport::record_at("D", "L1", 0, "")),
            ),
            (
                "replace",
                vsupport::replace(vsupport::record_at("D2", "L1", 0, "s-a")),
            ),
            ("delete", vsupport::delete(target("L1", 0, "s-a"))),
        ];
        for (operation, payload) in operations {
            let mut doc = document(1, seeds(), None);
            doc["layers"][0]["pathFx"] = stack.clone();
            let mut owner = owner_of(&doc);
            let id = format!("{name}-{operation}");
            if refuses {
                assert_unavailable(&mut owner, &id, payload);
            } else {
                let response = vsupport::send(&mut owner, &id, payload);
                assert_eq!(response["ok"], true, "{id}: {response}");
            }
        }
    }
    // The guard is per layer, and clearing the stack (the baseline's "deactivate them") lifts it.
    let mut doc = document(1, seeds(), None);
    doc["layers"][0]["pathFx"] = json!([{"type": "zigzag"}]);
    let mut owner = owner_of(&doc);
    let other_layer = vsupport::create(vsupport::record_at("B", "L2", 0, ""));
    assert_eq!(vsupport::send(&mut owner, "l2", other_layer)["ok"], true);
    assert_unavailable(
        &mut owner,
        "l1",
        vsupport::create(vsupport::record_at("D", "L1", 0, "")),
    );
    let disabled = json!([{"type": "zigzag", "enabled": false}]);
    assert!(applied(&send(
        &mut owner,
        "off",
        OP_FX,
        effects("L1", disabled)
    )));
    let retry = vsupport::send(
        &mut owner,
        "l1-again",
        vsupport::create(vsupport::record_at("D", "L1", 0, "")),
    );
    assert_eq!(retry["ok"], true, "{retry}");
}

#[cfg(feature = "evaluation")]
#[test]
fn stored_stacks_evaluate_to_the_baseline_vectors_while_the_source_stays_put() {
    use crate::evaluation::object_vector_kernels::render_vector_eval::evaluate_vector;
    let mut checked = 0;
    let mut distinct = std::collections::BTreeSet::new();
    for row in rows("fx") {
        let (label, frame_no) = (row[1], row[2].parse::<u32>().expect("frame"));
        let stack: Value = serde_json::from_str(row[3]).expect("stack json");
        let input = segs(row[5]);
        let shape = record("s-1", "L1", &input, row[4] == "1", None);
        let other = record("s-2", "L2", &straight_rect(), true, None);
        let mut owner = owner_of(&document(1, vec![shape, other], None));
        let before = state(&owner);
        let response = send(&mut owner, label, OP_FX, effects("L1", stack.clone()));
        assert_eq!(response["ok"], true, "{label}: {response}");
        let snapshot = owner
            .acquire_snapshot(owner.content_revision())
            .expect("head snapshot");
        let evaluated = evaluate_vector(&snapshot, 0, frame_no).expect("evaluated");
        let got: Segs = evaluated
            .segments
            .iter()
            .map(|s| {
                [
                    s.point[0],
                    s.point[1],
                    s.handle_in[0],
                    s.handle_in[1],
                    s.handle_out[0],
                    s.handle_out[1],
                ]
            })
            .collect();
        let want = segs(row[6]);
        let hashed = stack.as_array().expect("stack").iter().any(|entry| {
            entry["type"] == "roughen"
                && entry["enabled"] != json!(false)
                && entry["p1"] != json!(0)
        });
        let outcome = if hashed {
            within_tolerance(&got, &want, 1e-9)
        } else {
            same_bits(&got, &want)
        };
        outcome.unwrap_or_else(|why| panic!("{label}: {why}"));
        assert_eq!(evaluated.closed, row[7] == "1", "{label}");
        assert_eq!(
            state(&owner)["objects"],
            before["objects"],
            "{label}: stored anchors unchanged"
        );
        distinct.insert(row[6].to_string());
        checked += 1;
    }
    assert_eq!(checked, 16);
    assert_eq!(distinct.len(), 9, "distinct evaluated outputs");
}

#[cfg(feature = "evaluation")]
#[test]
fn the_oracle_rows_distinguish_order_frames_defaults_and_disabled_effects() {
    let fx = rows("fx");
    let row = |label: &str| {
        fx.iter()
            .find(|row| row[1] == label)
            .unwrap_or_else(|| panic!("fx row {label}"))
    };
    let out = |label: &str| segs(row(label)[6]);
    // Literal anchor counts of the baseline walk: ridges * 2 per curve, plus the closing anchor of an open path.
    assert_eq!(out("zigzag-defaults-open-line").len(), 9);
    assert_eq!(out("zigzag-explicit-open-line").len(), 9);
    assert_eq!(out("zigzag-smooth-closed-rect").len(), 32);
    assert_ne!(
        out("order-zigzag-then-roughen"),
        out("order-roughen-then-zigzag")
    );
    assert_ne!(
        out("roughen-frame-10-evolving"),
        out("roughen-frame-12-evolving")
    );
    assert_ne!(out("roughen-frame-0"), out("roughen-frame-10-evolving"));
    // A disabled entry is skipped; null and absent parameters read the type defaults; a stack that does
    // nothing returns the input untouched.
    assert_eq!(
        out("disabled-first-skipped"),
        out("roughen-frame-10-evolving")
    );
    assert_eq!(
        out("zigzag-null-params-read-defaults"),
        out("zigzag-explicit-defaults-closed-rect")
    );
    assert_eq!(
        out("zigzag-defaults-open-line"),
        out("enabled-absent-counts-active")
    );
    for label in [
        "all-disabled-untouched",
        "empty-stack-untouched",
        "roughen-amount-zero-untouched",
    ] {
        assert_eq!(out(label), segs(row(label)[5]), "{label}");
    }
}
