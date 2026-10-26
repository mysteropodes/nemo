//! L01-F: sealed vector create / replace / delete through the real dispatch caller, the existing history and
//! the existing revision owner. Literal expectations: baseline rows executed over the shipped Paper.js
//! (fixtures/object-vector-edit/oracle.txt) and hand-written order-plan literals.
use crate::object_codec::decode_storage_project;
use serde_json::{json, Value};

#[path = "object_vector_edit_controls.rs"]
mod controls;
#[path = "object_vector_edit_limits.rs"]
mod limits;
#[path = "object_vector_edit_lock.rs"]
mod lock;
#[path = "object_vector_create_many.rs"]
mod create_many;
#[path = "object_vector_create_held.rs"]
mod create_held;
#[path = "object_vector_batch.rs"]
mod batch;
#[path = "object_vector_trim_ends.rs"]
mod trim_ends;
#[path = "object_vector_edit_support.rs"]
mod support;
use support::*;

// label, closed, anchors, baseline fillColor, baseline strokeColor (None: no real stroke), width, opacity.
// Hand-written baseline facts: the generator mapping and the oracle file are not trusted alone.
const PINS: [(&str, bool, usize, Option<&str>, Option<&str>, f64, f64); 6] = [
    ("A", false, 2, None, Some("#112233"), 4.0, 1.0),
    ("B", true, 4, Some("#11223380"), Some("#445566"), 2.5, 1.0),
    ("C", true, 4, Some("#336699"), None, 4.0, 1.0),
    ("D", false, 5, None, Some("#ff00aa"), 4.0, 1.0),
    ("E", false, 2, None, Some("#112233"), 4.0, 0.5),
    ("D2", true, 5, Some("#00ff00"), Some("#ff00aa"), 4.0, 1.0),
];

fn plan_elements(state: &Value) -> Value {
    state["compositionOrder"]["contexts"][0]["scopes"].clone()
}

#[test]
fn oracle_is_complete_and_pinned_to_the_baseline() {
    let (meta, recs, steps) = oracle();
    let get = |key: &str| -> String {
        meta.iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| panic!("meta {key}"))
    };
    assert_eq!(get("baseline"), BASELINE);
    assert_eq!(
        get("anchors"),
        "colorHex8:551,applyBrushKeyline:591,_r3:608,serP:609,desP:846"
    );
    assert_eq!(get("slice-sha256-serP"), SERP_SLICE);
    assert_eq!(get("slice-sha256-desP"), DESP_SLICE);
    assert_eq!((get("records"), get("steps")), ("6".into(), "10".into()));
    let labels: Vec<&str> = recs.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(labels, ["A", "B", "C", "D", "E", "D2"]);
    assert_eq!(steps.len(), 10);
}

#[test]
fn oracle_records_agree_with_their_baseline_serp_output() {
    for rec in oracle().1 {
        check_record(&rec).unwrap();
        let pin = PINS.iter().find(|p| p.0 == rec.label).unwrap();
        let (b, n) = (&rec.baseline, &rec.native);
        assert_eq!(b["closed"], json!(pin.1), "{}", rec.label);
        assert_eq!(b["segments"].as_array().unwrap().len(), pin.2);
        assert_eq!(b["fillColor"].as_str(), pin.3);
        assert_eq!(n["fill"].is_null(), pin.3.is_none());
        // The baseline writes the white fallback colour for a fill-only shape; the flag is what is real.
        assert_eq!(b["hasRealStroke"], json!(pin.4.is_some()));
        match pin.4 {
            Some(hex) => assert_eq!(b["strokeColor"].as_str(), Some(hex)),
            None => {
                assert_eq!(b["strokeColor"], "#ffffff");
                assert!(n["vector"]["stroke"].is_null());
            }
        }
        assert_eq!(b["strokeWidth"].as_f64(), Some(pin.5));
        assert_eq!(b["opacity"].as_f64(), Some(pin.6));
    }
}

#[test]
fn created_open_and_closed_records_round_trip_literally_through_dispatch() {
    for rec in oracle().1 {
        let mut owner = owner_of(&document(1, seeds(), None));
        let before = state(&owner);
        let response = send(&mut owner, "c", create(record_at(&rec.label, "L1", 0, "")));
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(response["contentRevision"], 1);
        assert_eq!(
            response["result"],
            json!({"applied":true,"historyEntriesAdded":1,"operation":"create",
                "strokeId":"native-stroke:1:0"})
        );
        let after = state(&owner);
        let expected = record_at(&rec.label, "L1", 0, "native-stroke:1:0");
        assert_eq!(after["objects"][3], expected, "{}", rec.label);
        assert_eq!(objects(&after).len(), 4);
        assert_eq!(objects(&after)[..3], objects(&before)[..]);
        assert_eq!(owner.history_depths(), (1, 0));
        // The stored bytes decode and re-encode to the same document.
        let again = owner_of(&after);
        assert_eq!(state(&again), after);
    }
}

#[test]
fn stroke_ids_skip_imported_ids_and_a_refusal_consumes_none() {
    let mut seeded = seeds();
    seeded[0]["target"]["strokeId"] = json!("native-stroke:1:0");
    let mut owner = owner_of(&document(1, seeded, None));
    let refused = send(&mut owner, "r", create(record_at("A", "L9", 0, "")));
    assert_eq!(refused["error"]["code"], "not_found");
    assert_eq!(owner.content_revision(), 0);
    let first = send(&mut owner, "a", create(record_at("A", "L1", 0, "")));
    assert_eq!(created_id(&first), "native-stroke:1:1");
    // Identity stays monotonic across undo: revision 2 is the undo, so the next create is 3:0, not 1:1.
    owner.undo(1).unwrap();
    let second = send(&mut owner, "b", create(record_at("B", "L1", 0, "")));
    assert_eq!(created_id(&second), "native-stroke:3:0");
    assert_ne!(created_id(&first), created_id(&second));
}

#[test]
fn create_and_delete_maintain_the_order_plan_exactly() {
    let mut owner = owner_of(&document(2, seeds(), None));
    let original = state(&owner);
    assert_eq!(
        plan_elements(&original),
        json!([
            {"frameScope":{"kind":"authored","frame":0},"elements":[
                {"layerUid":"L1","strokeId":"s-a","naturalPosition":0,"order":{}},
                {"layerUid":"L2","strokeId":"s-b","naturalPosition":0,"order":{}}]},
            {"frameScope":{"kind":"authored","frame":5},"elements":[
                {"layerUid":"L1","strokeId":"s-c","naturalPosition":0,"order":{}}]}])
    );
    let top = send(&mut owner, "top", create(record_at("D", "L1", 0, "")));
    assert_eq!(created_id(&top), "native-stroke:1:0");
    let fresh = send(&mut owner, "fresh", create(record_at("E", "L1", 3, "")));
    assert_eq!(created_id(&fresh), "native-stroke:2:0");
    let made = state(&owner);
    assert_eq!(
        plan_elements(&made),
        json!([
            {"frameScope":{"kind":"authored","frame":0},"elements":[
                {"layerUid":"L1","strokeId":"s-a","naturalPosition":0,"order":{}},
                {"layerUid":"L2","strokeId":"s-b","naturalPosition":0,"order":{}},
                {"layerUid":"L1","strokeId":"native-stroke:1:0","naturalPosition":1,"order":{}}]},
            {"frameScope":{"kind":"authored","frame":3},"elements":[
                {"layerUid":"L1","strokeId":"native-stroke:2:0","naturalPosition":0,"order":{}}]},
            {"frameScope":{"kind":"authored","frame":5},"elements":[
                {"layerUid":"L1","strokeId":"s-c","naturalPosition":0,"order":{}}]}])
    );
    // The other declared context is untouched.
    assert_eq!(
        made["compositionOrder"]["contexts"][1],
        original["compositionOrder"]["contexts"][1]
    );
    let gone = send(&mut owner, "del", delete(target("L1", 0, "s-a")));
    assert_eq!(gone["result"]["operation"], "delete");
    let after = state(&owner);
    assert_eq!(objects(&after).len(), 4);
    assert_eq!(
        plan_elements(&after)[0],
        json!({"frameScope":{"kind":"authored","frame":0},"elements":[
            {"layerUid":"L2","strokeId":"s-b","naturalPosition":0,"order":{}},
            {"layerUid":"L1","strokeId":"native-stroke:1:0","naturalPosition":0,"order":{}}]})
    );
    // Removing the only element of a scope keeps the scope as an explicit empty list.
    send(
        &mut owner,
        "del3",
        delete(target("L1", 3, "native-stroke:2:0")),
    );
    assert_eq!(
        plan_elements(&state(&owner))[1],
        json!({"frameScope":{"kind":"authored","frame":3},"elements":[]})
    );
}

#[test]
fn replace_changes_only_the_target_record_in_place() {
    for version in [1, 2] {
        let mut owner = owner_of(&document(version, seeds(), None));
        let before = state(&owner);
        let response = send(&mut owner, "r", replace(record_at("D2", "L1", 0, "s-a")));
        assert_eq!(
            response["result"],
            json!({"applied":true,"historyEntriesAdded":1,"operation":"replace","strokeId":"s-a"})
        );
        let after = state(&owner);
        assert_eq!(after["objects"][0], record_at("D2", "L1", 0, "s-a"));
        assert_eq!(objects(&after)[1..], objects(&before)[1..]);
        assert_eq!(
            after.get("compositionOrder"),
            before.get("compositionOrder")
        );
        assert_eq!(
            (owner.content_revision(), owner.history_depths()),
            (1, (1, 0))
        );
    }
}

#[test]
fn replace_with_identical_values_is_a_no_op_that_keeps_redo() {
    let mut owner = owner_of(&document(2, seeds(), None));
    let original = bytes(&owner);
    let same = send(&mut owner, "same", replace(record_at("A", "L1", 0, "s-a")));
    assert_eq!(same["ok"], true);
    assert_eq!(same["result"]["applied"], false);
    assert_eq!(same["result"]["historyEntriesAdded"], 0);
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (0, (0, 0))
    );
    assert_eq!(bytes(&owner), original);
    // The same numbers spelled `10.0` are the same value: still a no-op, the stored bytes are not rewritten.
    let spelled = serde_json::to_string(&record_at("A", "L1", 0, "s-a"))
        .unwrap()
        .replace("\"x\":10,\"y\":20", "\"x\":10.0,\"y\":20.0");
    assert!(spelled.contains("10.0"));
    let respelled = send(
        &mut owner,
        "spelled",
        replace(serde_json::from_str(&spelled).unwrap()),
    );
    assert_eq!(respelled["result"]["applied"], false);
    assert_eq!(bytes(&owner), original);
    // A no-op after undo must not clear the redo stack.
    send(&mut owner, "edit", replace(record_at("D2", "L1", 0, "s-a")));
    owner.undo(1).unwrap();
    assert_eq!(owner.history_depths(), (0, 1));
    let revision = owner.content_revision();
    let held = send(&mut owner, "held", replace(record_at("A", "L1", 0, "s-a")));
    assert_eq!(held["result"]["applied"], false);
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (revision, (0, 1))
    );
    owner.redo(revision).unwrap();
    assert_eq!(state(&owner)["objects"][0], record_at("D2", "L1", 0, "s-a"));
}

#[test]
fn receipts_replay_without_executing_twice_and_a_changed_body_is_refused() {
    let mut owner = owner_of(&document(2, seeds(), None));
    let req = request(&mut owner, "once", create(record_at("B", "L1", 0, "")));
    let first = dispatch(&mut owner, &req);
    assert_eq!(first["ok"], true);
    let after = bytes(&owner);
    assert_eq!(dispatch(&mut owner, &req), first);
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (1, (1, 0))
    );
    assert_eq!(bytes(&owner), after);
    let mut altered = req.clone();
    altered["payload"]["record"] = record_at("C", "L1", 0, "");
    let refused = dispatch(&mut owner, &altered);
    assert_eq!(refused["ok"], false);
    assert_eq!(refused["error"]["code"], "invalid_request");
    assert_eq!(bytes(&owner), after);
    owner.undo(1).unwrap();
    assert_eq!(
        dispatch(&mut owner, &req),
        first,
        "replay cannot execute after undo"
    );
    assert_eq!(objects(&state(&owner)).len(), 3);
}

#[test]
fn undo_restores_the_exact_prior_bytes_and_redo_reapplies() {
    let mut owner = owner_of(&document(2, seeds(), None));
    let steps = [
        ("create", create(record_at("E", "L1", 0, ""))),
        ("replace", replace(record_at("D2", "L2", 0, "s-b"))),
        ("delete", delete(target("L1", 5, "s-c"))),
    ];
    for (name, payload) in steps {
        let before = bytes(&owner);
        let applied = send(&mut owner, name, payload);
        assert_eq!(applied["ok"], true, "{name}");
        let after = bytes(&owner);
        assert_ne!(before, after, "{name}");
        let head = owner.content_revision();
        owner.undo(head).unwrap();
        assert_eq!(bytes(&owner), before, "undo {name}");
        owner.redo(head + 1).unwrap();
        assert_eq!(bytes(&owner), after, "redo {name}");
    }
}

#[test]
fn delete_is_refused_while_a_holder_would_be_orphaned() {
    let holders = json!([{"target":{"contextId":"scene-root","layerUid":"L1","strokeId":"s-a"},
        "properties":[{"property":"fillColor","static":[255,12.5,0,255]}]}]);
    let mut objects = seeds();
    objects.push(record_at("A", "L1", 7, "s-a"));
    for version in [1, 2] {
        let mut owner = owner_of(&document(version, objects.clone(), Some(holders.clone())));
        // Another frame still draws the element: the holder keeps resolving, delete proceeds.
        let first = send(&mut owner, "one", delete(target("L1", 0, "s-a")));
        assert_eq!(first["ok"], true, "{first}");
        let kept = bytes(&owner);
        // The last drawing of the animated element cannot be removed: refused, nothing changes.
        let last = send(&mut owner, "two", delete(target("L1", 7, "s-a")));
        assert_eq!(last["ok"], false);
        assert_eq!(last["error"]["code"], "unavailable");
        assert_eq!((bytes(&owner), owner.content_revision()), (kept, 1));
        assert_eq!(owner.history_depths(), (1, 0));
        assert!(state(&owner)["propertyHolders"][0]["target"]["strokeId"] == "s-a");
    }
}

#[test]
fn vector_reference_refusal_preserves_existing_fill_and_other_target_errors() {
    let cases = [
        (
            2,
            json!({"contextId":"scene-root","frameScope":{"kind":"reference","frame":0},"layerUid":"L1","strokeId":"s-a"}),
            "unavailable",
        ),
        (
            1,
            json!({"contextId":"scene-root","frameScope":{"kind":"reference","frame":0},"layerUid":"L1","strokeId":"zz"}),
            "not_found",
        ),
        (1, target("L9", 0, "s-a"), "not_found"),
        (2, target("L1", 0, "nope"), "not_found"),
        (1, target("L1", 99, "s-a"), "invalid_request"),
        (
            1,
            json!({"contextId":"other","frameScope":{"kind":"authored","frame":0},"layerUid":"L1","strokeId":"s-a"}),
            "invalid_request",
        ),
    ];
    for (version, aim, code) in cases {
        for action in ["fill", "replace", "delete"] {
            let mut owner = owner_of(&document(version, seeds(), None));
            let before = bytes(&owner);
            let payload = match action {
                "fill" => json!({"command":"object.fill.set","stableTarget":aim,
                    "fill":{"kind":"solid","r":1,"g":0,"b":0,"a":1}}),
                "replace" => {
                    let mut record = record_at("D2", "L1", 0, "x");
                    record["target"] = aim.clone();
                    replace(record)
                }
                _ => delete(aim.clone()),
            };
            let mut req = request(&owner, "t", payload);
            if action == "fill" {
                req["operation"] = json!("command.document.object.fill.set");
            }
            let response = dispatch(&mut owner, &req);
            // Drawing vector edits never author Reference content, including v1.
            // The pre-existing fill route keeps its separately characterized code.
            let expected = if action != "fill" && aim["frameScope"]["kind"] == "reference" {
                "unavailable"
            } else {
                code
            };
            assert_eq!(response["error"]["code"], expected, "{action} {aim}");
            assert_eq!(bytes(&owner), before);
        }
    }
}

#[test]
fn distinct_numeric_coordinates_do_not_collapse_into_a_false_noop() {
    for (old, new) in [
        (
            json!(-9_007_199_254_740_993_i64),
            json!(-9_007_199_254_740_992_i64),
        ),
        (
            json!(-9_007_199_254_740_993_i64),
            json!(-9_007_199_254_740_992.0_f64),
        ),
        (
            json!(9_007_199_254_740_993_u64),
            json!(9_007_199_254_740_992.0_f64),
        ),
        (json!(u64::MAX), json!(18_446_744_073_709_551_616.0_f64)),
        (json!(i64::MAX), json!(9_223_372_036_854_775_808.0_f64)),
    ] {
        let mut initial = seeds();
        initial[0]["geometry"]["segments"][0]["point"]["x"] = old;
        let mut owner = owner_of(&document(2, initial, None));
        let before = bytes(&owner);
        let mut changed = state(&owner)["objects"][0].clone();
        changed["geometry"]["segments"][0]["point"]["x"] = new;
        let response = send(&mut owner, "integer-edit", replace(changed.clone()));
        assert_eq!(response["ok"], true);
        assert_eq!(response["result"]["applied"], true);
        assert_eq!(response["result"]["historyEntriesAdded"], 1);
        assert_eq!(state(&owner)["objects"][0], changed);
        owner.undo(owner.content_revision()).unwrap();
        assert_eq!(bytes(&owner), before);
        owner.redo(owner.content_revision()).unwrap();
        assert_eq!(state(&owner)["objects"][0], changed);
    }
}

#[test]
fn the_edit_sequence_reproduces_the_baseline_layer_order() {
    for version in [1, 2] {
        let steps = oracle().2;
        let mut owner = owner_of(&document(version, vec![], None));
        let mut ids: Vec<(String, String)> = Vec::new(); // label -> native stroke id
        let id_of = |ids: &[(String, String)], label: &str| {
            ids.iter().find(|(l, _)| l == label).unwrap().1.clone()
        };
        let layer_of = |label: &str| if label == "C" { "L2" } else { "L1" };
        for (i, step) in steps.iter().enumerate().skip(1) {
            let words: Vec<&str> = step.what.split(' ').collect();
            let label = words[1];
            let payload = match words[0] {
                "create" => create(record_at(label, layer_of(label), 0, "")),
                "delete" => delete(target(layer_of(label), 0, &id_of(&ids[..], label))),
                _ => replace(record_at("D2", "L1", 0, &id_of(&ids[..], label))),
            };
            let response = send(&mut owner, &format!("s{i}"), payload);
            assert_eq!(response["ok"], true, "{} {response}", step.what);
            match words[0] {
                "create" => ids.push((label.to_string(), created_id(&response))),
                "delete" => ids.retain(|(l, _)| l != label),
                _ => {}
            }
            let now = state(&owner);
            for layer in ["L1", "L2"] {
                let by_objects: Vec<String> = now["objects"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|o| o["target"]["layerUid"] == layer)
                    .map(|o| o["target"]["strokeId"].as_str().unwrap().to_string())
                    .collect();
                let want: Vec<String> = step.order[layer]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|l| id_of(&ids[..], l.as_str().unwrap()))
                    .collect();
                assert_eq!(by_objects, want, "v{version} {} {layer}", step.what);
                if version == 2 {
                    let mut by_plan: Vec<(u64, String)> = Vec::new();
                    for scope in now["compositionOrder"]["contexts"][0]["scopes"]
                        .as_array()
                        .unwrap()
                    {
                        for e in scope["elements"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .filter(|e| e["layerUid"] == layer)
                        {
                            by_plan.push((
                                e["naturalPosition"].as_u64().unwrap(),
                                e["strokeId"].as_str().unwrap().to_string(),
                            ));
                        }
                    }
                    by_plan.sort();
                    let positions: Vec<u64> = by_plan.iter().map(|p| p.0).collect();
                    assert_eq!(positions, (0..want.len() as u64).collect::<Vec<_>>());
                    assert_eq!(by_plan.into_iter().map(|p| p.1).collect::<Vec<_>>(), want);
                }
            }
        }
        // The replaced D now carries the closed + filled geometry of the baseline's mutated item.
        let d = id_of(&ids[..], "D");
        let record = state(&owner)["objects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["target"]["strokeId"] == d.as_str())
            .unwrap()
            .clone();
        assert_eq!(record, record_at("D2", "L1", 0, &d));
    }
}

#[test]
fn f06_locked_layers_remain_literal_storage_while_drawing_edits_use_the_owner_guard() {
    for version in [1, 2] {
        let mut doc = document(version, seeds(), None);
        doc["layers"][0]["locked"] = json!(true);
        let decoded = decode_storage_project(doc.to_string().as_bytes()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), doc);
    }
}
